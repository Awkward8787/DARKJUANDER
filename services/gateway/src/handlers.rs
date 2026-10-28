use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use dj_audit::Event;
use dj_broker::{read_mediated_file, ActionRequest};
use dj_label::Labels;
use dj_policy::RouteKind;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use umbra::{decide, RouteInput};

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({"error": {"message": self.message, "type": "invalid_request_error"}});
        (self.status, Json(body)).into_response()
    }
}

pub async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

pub async fn models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let _caller = authenticate(&state, &headers)?;
    let data: Vec<_> = state
        .models()
        .keys()
        .chain(state.aliases().keys())
        .map(|id| json!({"id": id, "object": "model", "owned_by": "darkjuander"}))
        .collect();
    Ok(Json(json!({"object": "list", "data": data})))
}

pub async fn top(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let caller = authenticate(&state, &headers)?;
    if !caller.has_grant("admin.control") {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "admin.control required",
        ));
    }
    let models = state.registry().lock().expect("registry").view();
    let cache = state.cache().lock().expect("cache").stats();
    let engines: Vec<_> = state
        .engines()
        .iter()
        .map(|(name, engine)| json!({"name": name, "calls": engine.calls()}))
        .collect();
    Ok(Json(json!({
        "models": models,
        "cache": cache,
        "engines": engines,
    })))
}

#[derive(Deserialize)]
pub struct PsiBody {
    memory_some_avg10: f64,
    #[serde(default)]
    memory_full_avg10: f64,
    #[serde(default)]
    io_some_avg10: f64,
    #[serde(default)]
    clear: bool,
}

/// Test hook. The image config leaves `allow_psi_override` false, and it must
/// stay false on any machine that treats routing as a boundary. Enabling it
/// lets `admin.control` invent pressure and steer model choice.
pub async fn psi_override(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PsiBody>,
) -> Result<Json<Value>, ApiError> {
    let caller = authenticate(&state, &headers)?;
    if !caller.has_grant("admin.control") {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "admin.control required",
        ));
    }
    if !state.config().allow_psi_override {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "psi override is disabled",
        ));
    }
    if body.clear {
        state.set_psi_override(None);
        return Ok(Json(json!({"override": false})));
    }
    state.set_psi_override(Some(umbra::PsiSnapshot {
        memory_some_avg10: body.memory_some_avg10,
        memory_full_avg10: body.memory_full_avg10,
        io_some_avg10: body.io_some_avg10,
        source: "override".into(),
    }));
    Ok(Json(
        json!({"override": true, "memory_some_avg10": body.memory_some_avg10}),
    ))
}

#[derive(Deserialize)]
pub struct ChatRequest {
    model: String,
    messages: Vec<IncomingMessage>,
}

#[derive(Deserialize)]
struct IncomingMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    dj_labels: Option<Labels>,
    #[serde(default)]
    dj_span: Option<String>,
}

struct ResolvedMessage {
    role: String,
    content: String,
    labels: Labels,
}

pub async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatRequest>,
) -> Result<Json<Value>, ApiError> {
    let caller = authenticate(&state, &headers)?;
    if request.messages.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "messages is empty"));
    }
    let mut resolved = Vec::new();
    for message in &request.messages {
        resolved.push(resolve_message(&state, &caller, message)?);
    }
    let labels =
        Labels::join_all(resolved.iter().map(|message| &message.labels)).pin_owner(&caller.id);
    for message in &resolved {
        if message.labels.integrity == dj_label::Integrity::Untrusted
            || message.labels.origin.contains("web:")
            || message.labels.origin.contains("email:")
            || message.labels.origin.contains("file:")
        {
            state
                .audit(Event {
                    kind: "content_ingest".into(),
                    actor: caller.id.clone(),
                    action: "span".into(),
                    outcome: "labeled".into(),
                    reason: "input span labeled before the model call".into(),
                    labels: message.labels.clone(),
                    details: json!({
                        "role": message.role,
                        "origin": message.labels.origin,
                    }),
                })
                .map_err(internal)?;
        }
    }

    let authz = state
        .broker()
        .authorize(
            &caller,
            &ActionRequest {
                action: "model.use".into(),
                target: request.model.clone(),
                labels: labels.clone(),
                arguments: json!({}),
                route: None,
            },
        )
        .map_err(internal)?;
    if !authz.allowed {
        return Err(ApiError::new(StatusCode::FORBIDDEN, authz.reason));
    }

    let psi = state.psi();
    let local_only = state.config().local_only || caller.local_only;
    let granted = granted_models(&caller);
    let decision = decide(&RouteInput {
        requested: &request.model,
        models: state.models(),
        aliases: state.aliases(),
        labels: &labels,
        psi: &psi,
        threshold: state.config().psi_memory_some_avg10_threshold,
        prefer_cloud: state.config().prefer_cloud,
        local_only,
        policy: state.policy(),
        granted: Some(&granted),
    })
    .map_err(|err| ApiError::new(StatusCode::FORBIDDEN, err.to_string()))?;
    if !caller.has_grant(&format!("model.use:{}", decision.selected)) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("capability not granted for model.use:{}", decision.selected),
        ));
    }

    state
        .registry()
        .lock()
        .expect("registry")
        .record_use(&decision.selected, &caller.id);

    let prefix = prefix_text(&resolved);
    let prefix_labels = prefix_labels(&resolved);
    let key = umbra::CacheKey {
        model: decision.selected.clone(),
        prefix_hash: sha256_hex(&prefix),
        trust_domain: caller.trust_domain.clone(),
        label_fingerprint: prefix_labels.fingerprint(),
    };
    let lookup = state.cache().lock().expect("cache").lookup(key);

    let engine = state.engine(&decision.engine).ok_or_else(|| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("engine {} is not loaded", decision.engine),
        )
    })?;
    let engine_response = engine
        .complete(&umbra::EngineRequest {
            model: decision.selected.clone(),
            messages: resolved
                .iter()
                .map(|message| umbra::engine::ChatMessage {
                    role: message.role.clone(),
                    content: message.content.clone(),
                })
                .collect(),
        })
        .map_err(|err| ApiError::new(StatusCode::BAD_GATEWAY, err.to_string()))?;

    let mut output_labels = labels.clone();
    if decision.location == umbra::Location::Cloud {
        output_labels = output_labels.join(&Labels {
            integrity: dj_label::Integrity::Untrusted,
            confidentiality: labels.confidentiality,
            origin: "cloud:model".into(),
            owner: caller.id.clone(),
        });
    }
    output_labels = output_labels.pin_owner(&caller.id);
    state.remember_labels(&caller.id, output_labels.clone());

    state
        .audit(Event {
            kind: "route".into(),
            actor: caller.id.clone(),
            action: "model.route".into(),
            outcome: decision.selected.clone(),
            reason: decision.reason.clone(),
            labels: output_labels.clone(),
            details: json!({
                "requested": request.model,
                "scorer_preferred": decision.scorer_preferred,
                "memory_some_avg10": format!("{:.2}", decision.memory_some_avg10),
                "location": format!("{:?}", decision.location).to_lowercase(),
            }),
        })
        .map_err(internal)?;
    state
        .audit(Event {
            kind: "cache".into(),
            actor: caller.id.clone(),
            action: "prefix".into(),
            outcome: if lookup.hit { "hit" } else { "miss" }.into(),
            reason: "exact key match on model, prefix, trust domain, and labels".into(),
            labels: prefix_labels,
            details: json!({"trust_domain": caller.trust_domain, "model": decision.selected}),
        })
        .map_err(internal)?;
    state
        .audit(Event {
            kind: "model_call".into(),
            actor: caller.id.clone(),
            action: decision.selected.clone(),
            outcome: "ok".into(),
            reason: decision.reason.clone(),
            labels: output_labels.clone(),
            details: json!({
                "engine": decision.engine,
                "prompt_tokens": engine_response.prompt_tokens,
                "completion_tokens": engine_response.completion_tokens,
                "cache_hit": lookup.hit,
            }),
        })
        .map_err(internal)?;

    let tool_calls: Vec<_> = engine_response
        .tool_calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            json!({
                "id": format!("call_{index}"),
                "type": "function",
                "function": {"name": call.name, "arguments": call.arguments},
            })
        })
        .collect();
    let finish = if tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    let id = state.next_chat_id();
    Ok(Json(json!({
        "id": format!("chatcmpl-{id}"),
        "object": "chat.completion",
        "model": decision.selected,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": engine_response.content,
                "tool_calls": tool_calls,
            },
            "finish_reason": finish,
        }],
        "usage": {
            "prompt_tokens": engine_response.prompt_tokens,
            "completion_tokens": engine_response.completion_tokens,
            "total_tokens": engine_response.prompt_tokens + engine_response.completion_tokens,
        },
        "dj": {
            "requested": request.model,
            "route": decision,
            "labels": output_labels,
            "cache": lookup,
        }
    })))
}

#[derive(Deserialize)]
pub struct ToolBody {
    action: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    labels: Option<Labels>,
}

pub async fn tool(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ToolBody>,
) -> Result<Json<Value>, ApiError> {
    let caller = authenticate(&state, &headers)?;
    let floor = state
        .last_labels(&caller.id)
        .unwrap_or_else(|| Labels::user(&caller.id));
    let claimed = body
        .labels
        .clone()
        .unwrap_or_else(|| Labels::user(&caller.id))
        .ingest();
    let labels = floor.with_claim(&claimed).ingest().pin_owner(&caller.id);
    let route = model_route(&state, &body.action, &body.target);
    let authz = state
        .broker()
        .authorize(
            &caller,
            &ActionRequest {
                action: body.action.clone(),
                target: body.target.clone(),
                labels: labels.clone(),
                arguments: body.arguments.clone(),
                route,
            },
        )
        .map_err(internal)?;
    if !authz.allowed || body.action != "files.read" {
        return Ok(Json(json!({
            "decision": if authz.allowed { "allow" } else { "deny" },
            "performed": false,
            "prompted": authz.prompted,
            "rule_id": authz.rule_id,
            "reason": authz.reason,
            "labels": labels,
        })));
    }
    let mediated = read_mediated_file(
        &caller,
        Path::new(&body.target),
        &state.config().private_paths,
        &state.config().secret_paths,
    )
    .map_err(|err| ApiError::new(StatusCode::FORBIDDEN, err.to_string()))?;
    let span = state.store_span(
        caller.id.clone(),
        mediated.text.clone(),
        mediated.labels.clone(),
    );
    let raised = floor.with_claim(&mediated.labels).pin_owner(&caller.id);
    state.remember_labels(&caller.id, raised);
    // The bytes stay in the span. Returning them would let the caller resubmit
    // the same text under a public label. Copying through a side channel that
    // never calls this broker is still not detected; that needs PE-6/PE-7.
    Ok(Json(json!({
        "decision": "allow",
        "performed": true,
        "prompted": false,
        "rule_id": authz.rule_id,
        "reason": authz.reason,
        "labels": mediated.labels,
        "span_id": span.id,
        "path": mediated.path,
    })))
}

fn granted_models(caller: &dj_broker::Principal) -> BTreeSet<String> {
    caller
        .grants
        .iter()
        .filter_map(|grant| grant.strip_prefix("model.use:"))
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string())
        .collect()
}

fn model_route(state: &AppState, action: &str, target: &str) -> Option<RouteKind> {
    if action != "model.use" {
        return None;
    }
    state
        .models()
        .get(target)
        .map(|model| match model.location {
            umbra::Location::Local => RouteKind::Local,
            umbra::Location::Cloud => RouteKind::Cloud,
        })
}

fn resolve_message(
    state: &AppState,
    caller: &dj_broker::Principal,
    message: &IncomingMessage,
) -> Result<ResolvedMessage, ApiError> {
    let floor = state
        .last_labels(&caller.id)
        .unwrap_or_else(|| Labels::user(&caller.id));
    let claimed = message
        .dj_labels
        .clone()
        .unwrap_or_else(|| Labels::user(&caller.id))
        .ingest();
    let mut labels = floor.with_claim(&claimed).ingest().pin_owner(&caller.id);
    let mut content = message.content.clone();
    if let Some(span_id) = &message.dj_span {
        let span = state.span(span_id).ok_or_else(|| {
            ApiError::new(StatusCode::BAD_REQUEST, format!("unknown span {span_id}"))
        })?;
        if span.owner != caller.id {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "span belongs to another principal",
            ));
        }
        if content.is_empty() {
            content = span.text;
        } else {
            content = format!("{content}\n{}", span.text);
        }
        labels = labels.join(&span.labels).pin_owner(&caller.id);
    }
    if content.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "message content is empty",
        ));
    }
    Ok(ResolvedMessage {
        role: message.role.clone(),
        content,
        labels,
    })
}

fn prefix_text(messages: &[ResolvedMessage]) -> String {
    let system: Vec<_> = messages
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| message.content.as_str())
        .collect();
    if system.is_empty() {
        messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        system.join("\n")
    }
}

fn prefix_labels(messages: &[ResolvedMessage]) -> Labels {
    let system: Vec<_> = messages
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| &message.labels)
        .collect();
    if system.is_empty() {
        Labels::join_all(messages.iter().map(|message| &message.labels))
    } else {
        Labels::join_all(system)
    }
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<dj_broker::Principal, ApiError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
    state
        .principal_for_token(token)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "unknown token"))
}

fn internal(err: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
