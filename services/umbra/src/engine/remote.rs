//! One OpenAI-compatible client for cloud providers.
//!
//! xAI and OpenAI both accept `POST {base}/chat/completions` with a bearer
//! token. ChatGPT subscription tokens use the documented Responses API
//! instead (`store: false`, `stream: true`). Streaming on the chat-completions
//! wire is optional. Errors fail closed: no retry after a 4xx, no retry after
//! a body has been accepted, and a non-public label never opens a socket.

#![forbid(unsafe_code)]

use super::{bump, Engine, EngineError, EngineRequest, EngineResponse, ToolCall};
use crate::credential::{self, redact, CredentialFormat, CredentialSource, Secret};
use crate::siwc::{self, SubscriptionCredential};
use dj_label::Confidentiality;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use ureq::AgentBuilder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    ChatCompletions,
    Responses,
}

#[derive(Clone, Debug)]
pub struct RemoteConfig {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub timeout: Duration,
    pub retries: u32,
    pub backoff: Duration,
    pub stream: bool,
    pub wire: Wire,
    pub credential: CredentialSource,
}

pub struct RemoteEngine {
    config: RemoteConfig,
    calls: AtomicU64,
}

impl RemoteEngine {
    pub fn new(config: RemoteConfig) -> Result<Self, EngineError> {
        check_base_url(&config.base_url).map_err(|message| EngineError::Failed {
            engine: config.name.clone(),
            message,
        })?;
        if config.timeout.is_zero() {
            return Err(EngineError::Failed {
                engine: config.name,
                message: "timeout must be non-zero".into(),
            });
        }
        Ok(Self {
            config,
            calls: AtomicU64::new(0),
        })
    }

    pub fn provider(&self) -> &str {
        &self.config.provider
    }

    fn endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        match self.config.wire {
            Wire::ChatCompletions => format!("{base}/chat/completions"),
            Wire::Responses => format!("{base}/responses"),
        }
    }

    fn fail(&self, secret: &str, message: impl AsRef<str>) -> EngineError {
        EngineError::Failed {
            engine: self.config.name.clone(),
            message: format!(
                "cloud request failed closed: {}",
                redact(secret, message.as_ref())
            ),
        }
    }

    fn load_secret(&self) -> Result<Secret, EngineError> {
        match self.config.credential.format() {
            CredentialFormat::ApiKey => {
                let bytes = credential::load_bytes(&self.config.credential).map_err(|err| {
                    self.fail(
                        "",
                        format!(
                            "credential for {} is not available: {err}",
                            self.config.provider
                        ),
                    )
                })?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| self.fail("", "credential file is not utf-8"))?;
                let key = text.trim();
                if key.is_empty() || key.contains('\n') {
                    return Err(self.fail("", "credential file is empty or has more than one line"));
                }
                Ok(Secret::new(key.to_string()))
            }
            CredentialFormat::ChatgptSubscription => {
                let bytes = credential::load_bytes(&self.config.credential).map_err(|err| {
                    self.fail(
                        "",
                        format!("subscription credential is not available: {err}"),
                    )
                })?;
                let mut record: SubscriptionCredential = serde_json::from_slice(&bytes)
                    .map_err(|_| self.fail("", "subscription credential is not a valid record"))?;
                if record.access_token.is_empty()
                    || !record
                        .scopes
                        .iter()
                        .any(|scope| scope == "chatgpt.tokens.use.direct")
                {
                    return Err(self.fail("", "subscription credential is missing plan scope"));
                }
                if record.needs_refresh(60) {
                    let path = match &self.config.credential {
                        CredentialSource::File { path, .. } => path.clone(),
                        CredentialSource::Systemd { .. } => {
                            return Err(self.fail(
                                &record.access_token,
                                "refusing to refresh a systemd credential in place",
                            ));
                        }
                    };
                    siwc::refresh_file(&path, &mut record, self.config.timeout)
                        .map_err(|err| self.fail(&record.access_token, err.to_string()))?;
                }
                Ok(Secret::new(record.access_token))
            }
        }
    }
}

impl Engine for RemoteEngine {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    fn complete(&self, request: &EngineRequest) -> Result<EngineResponse, EngineError> {
        if request.confidentiality != Some(Confidentiality::Public) {
            return Err(self.fail(
                "",
                "private, secret, or unlabeled data cannot be sent to a cloud provider",
            ));
        }
        check_base_url(&self.config.base_url).map_err(|message| self.fail("", message))?;
        let secret = self.load_secret()?;
        let body = request_body(&self.config, request);
        let attempts = self.config.retries.saturating_add(1);
        let mut delay = self.config.backoff;
        let mut last = String::from("no attempt");
        for attempt in 1..=attempts {
            bump(&self.calls);
            match post_once(&self.endpoint(), &secret, &body, self.config.timeout) {
                Ok(text) => {
                    return parse_provider(&self.config, &text)
                        .map_err(|message| self.fail(secret.expose(), message));
                }
                Err(Attempt::Fatal(message)) => {
                    return Err(self.fail(secret.expose(), message));
                }
                Err(Attempt::Retry(message)) => {
                    last = message;
                    if attempt == attempts {
                        break;
                    }
                    thread::sleep(delay.min(Duration::from_secs(2)));
                    delay = delay.saturating_mul(2).max(Duration::from_millis(1));
                }
            }
        }
        Err(self.fail(secret.expose(), format!("retries exhausted: {last}")))
    }
}

enum Attempt {
    Retry(String),
    Fatal(String),
}

fn post_once(
    url: &str,
    secret: &Secret,
    body: &Value,
    timeout: Duration,
) -> Result<String, Attempt> {
    let agent = AgentBuilder::new()
        .redirects(0)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let response = agent
        .post(url)
        .set("Authorization", &format!("Bearer {}", secret.expose()))
        .set("Accept", "application/json")
        .send_json(body);
    match response {
        Ok(response) => response
            .into_string()
            .map_err(|err| Attempt::Fatal(format!("could not read provider response: {err}"))),
        Err(ureq::Error::Status(code, response)) => {
            let _ = response.into_string();
            let message = format!("provider returned HTTP {code}");
            if matches!(code, 408 | 429 | 500 | 502 | 503 | 504) {
                Err(Attempt::Retry(message))
            } else {
                Err(Attempt::Fatal(message))
            }
        }
        Err(ureq::Error::Transport(err)) => Err(Attempt::Retry(format!("transport error: {err}"))),
    }
}

fn request_body(config: &RemoteConfig, request: &EngineRequest) -> Value {
    match config.wire {
        Wire::ChatCompletions => json!({
            "model": request.model,
            "messages": request.messages.iter().map(|message| {
                json!({"role": message.role, "content": message.content})
            }).collect::<Vec<_>>(),
            "stream": config.stream,
        }),
        Wire::Responses => json!({
            "model": request.model,
            "input": request.messages.iter().map(|message| {
                json!({"role": message.role, "content": message.content})
            }).collect::<Vec<_>>(),
            "store": false,
            "stream": true,
        }),
    }
}

fn parse_provider(config: &RemoteConfig, text: &str) -> Result<EngineResponse, String> {
    match config.wire {
        Wire::ChatCompletions if config.stream => parse_chat_stream(text),
        Wire::ChatCompletions => {
            let body: Value = serde_json::from_str(text)
                .map_err(|_| "provider response was not json".to_string())?;
            parse_chat_json(&body)
        }
        Wire::Responses => parse_responses_stream(text),
    }
}

pub fn parse_chat_json(body: &Value) -> Result<EngineResponse, String> {
    if body.get("error").is_some() {
        return Err("provider response contained an error object".into());
    }
    let message = body
        .pointer("/choices/0/message")
        .ok_or_else(|| "provider response has no choices[0].message".to_string())?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = match call.pointer("/function/arguments") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => "{}".into(),
            };
            if !name.is_empty() {
                tool_calls.push(ToolCall { name, arguments });
            }
        }
    }
    Ok(EngineResponse {
        content,
        tool_calls,
        prompt_tokens: json_u32(body, "/usage/prompt_tokens"),
        completion_tokens: json_u32(body, "/usage/completion_tokens"),
    })
}

pub fn parse_chat_stream(text: &str) -> Result<EngineResponse, String> {
    let mut content = String::new();
    let mut saw_done = false;
    for data in sse_data(text) {
        if data == "[DONE]" {
            saw_done = true;
            break;
        }
        let body: Value =
            serde_json::from_str(data).map_err(|_| "chat stream event was not json".to_string())?;
        if let Some(delta) = body
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
        {
            content.push_str(delta);
        }
    }
    if !saw_done {
        return Err("chat stream ended without [DONE]".into());
    }
    Ok(EngineResponse {
        content,
        tool_calls: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
    })
}

pub fn parse_responses_stream(text: &str) -> Result<EngineResponse, String> {
    let mut content = String::new();
    let mut completed = false;
    let mut prompt_tokens = 0;
    let mut completion_tokens = 0;
    for data in sse_data(text) {
        let body: Value = serde_json::from_str(data)
            .map_err(|_| "responses stream event was not json".to_string())?;
        let kind = body.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "response.output_text.delta" => {
                if let Some(delta) = body.get("delta").and_then(Value::as_str) {
                    content.push_str(delta);
                }
            }
            "response.failed" | "error" => {
                return Err("provider reported response.failed".into());
            }
            "response.incomplete" => {
                return Err("provider reported response.incomplete".into());
            }
            "response.completed" => {
                completed = true;
                prompt_tokens = json_u32(&body, "/response/usage/input_tokens");
                completion_tokens = json_u32(&body, "/response/usage/output_tokens");
            }
            _ => {}
        }
    }
    if !completed {
        return Err("responses stream ended without response.completed".into());
    }
    Ok(EngineResponse {
        content,
        tool_calls: Vec::new(),
        prompt_tokens,
        completion_tokens,
    })
}

fn sse_data(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("data:")
                .map(str::trim)
                .filter(|data| !data.is_empty())
        })
        .collect()
}

fn json_u32(body: &Value, pointer: &str) -> u32 {
    body.pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32
}

pub fn check_base_url(base: &str) -> Result<(), String> {
    if base.contains('@') {
        return Err("refusing a provider URL that embeds credentials".into());
    }
    if base.starts_with("https://") {
        return Ok(());
    }
    let Some(rest) = base.strip_prefix("http://") else {
        return Err("provider URL must be https, or http on loopback for tests".into());
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host);
    if host == "127.0.0.1" || host == "localhost" || host == "[::1]" {
        Ok(())
    } else {
        Err("refusing cleartext provider URL that is not loopback".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{install_secret, CredentialFormat, CredentialSource};
    use crate::engine::{ChatMessage, EngineRequest};
    use dj_label::Confidentiality;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn request(text: &str, confidentiality: Option<Confidentiality>) -> EngineRequest {
        EngineRequest {
            model: "grok-test".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: text.into(),
            }],
            confidentiality,
        }
    }

    struct HitServer {
        url: String,
        hits: Arc<AtomicU64>,
        bodies: Arc<Mutex<Vec<String>>>,
    }

    fn spawn_server(script: Vec<u16>, body_for_ok: &'static str) -> HitServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicU64::new(0));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let statuses = Arc::new(Mutex::new(script));
        let hits_thread = hits.clone();
        let bodies_thread = bodies.clone();
        let statuses_thread = statuses.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 1024];
                loop {
                    let read = stream.read(&mut tmp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..read]);
                    if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                if buf.len() < 4 {
                    break;
                }
                let header_end = buf
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
                let length = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                while buf.len() < header_end + length {
                    let read = stream.read(&mut tmp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..read]);
                }
                let body = String::from_utf8_lossy(
                    &buf[header_end..header_end + length.min(buf.len() - header_end)],
                )
                .to_string();
                hits_thread.fetch_add(1, Ordering::SeqCst);
                bodies_thread.lock().unwrap().push(body);
                let status = statuses_thread.lock().unwrap().pop().unwrap_or(200);
                let payload = if status == 200 { body_for_ok } else { "" };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        HitServer {
            url: format!("http://127.0.0.1:{port}/v1"),
            hits,
            bodies,
        }
    }

    fn engine(server: &HitServer, key: &str, stream: bool, retries: u32) -> RemoteEngine {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        install_secret(&path, format!("{key}\n").as_bytes()).unwrap();
        // Leak the tempdir for the process lifetime of the test. The engine
        // re-reads the file on each call, so the directory must outlive it.
        let path = path.clone();
        std::mem::forget(dir);
        RemoteEngine::new(RemoteConfig {
            name: "xai".into(),
            provider: "xai".into(),
            base_url: server.url.clone(),
            timeout: Duration::from_secs(2),
            retries,
            backoff: Duration::from_millis(1),
            stream,
            wire: Wire::ChatCompletions,
            credential: CredentialSource::File {
                path,
                format: CredentialFormat::ApiKey,
            },
        })
        .unwrap()
    }

    #[test]
    fn private_and_unlabeled_do_not_open_a_socket() {
        let server = spawn_server(vec![200], r#"{"choices":[{"message":{"content":"pong"}}]}"#);
        let engine = engine(&server, "sk-test-CANARYKEY", false, 0);
        for label in [
            Some(Confidentiality::Private),
            Some(Confidentiality::Secret),
            None,
        ] {
            let err = engine
                .complete(&request("PRIVATE_CANARY", label))
                .unwrap_err();
            assert!(!err.to_string().contains("CANARYKEY"));
            assert!(!err.to_string().contains("PRIVATE_CANARY"));
        }
        assert_eq!(server.hits.load(Ordering::SeqCst), 0);
        assert_eq!(engine.calls(), 0);
    }

    #[test]
    fn public_request_retries_503_then_parses_and_hides_the_key() {
        let ok = r#"{"choices":[{"message":{"role":"assistant","content":"pong","tool_calls":[{"function":{"name":"noop","arguments":"{}"}}]}}],"usage":{"prompt_tokens":2,"completion_tokens":1}}"#;
        let server = spawn_server(vec![200, 503, 503], ok);
        let engine = engine(&server, "sk-test-CANARYKEY", false, 2);
        let response = engine
            .complete(&request("ping", Some(Confidentiality::Public)))
            .unwrap();
        assert_eq!(response.content, "pong");
        assert_eq!(response.tool_calls[0].name, "noop");
        assert_eq!(server.hits.load(Ordering::SeqCst), 3);
        let bodies = server.bodies.lock().unwrap();
        assert!(bodies.iter().all(|body| !body.contains("CANARYKEY")));
        assert!(bodies.iter().all(|body| !body.contains("dj_labels")));
        assert!(bodies.iter().any(|body| body.contains("ping")));
    }

    #[test]
    fn unauthorized_does_not_retry_or_echo_the_key() {
        let server = spawn_server(vec![401], "");
        let engine = engine(&server, "sk-test-CANARYKEY", false, 3);
        let err = engine
            .complete(&request("ping", Some(Confidentiality::Public)))
            .unwrap_err();
        assert!(err.to_string().contains("HTTP 401"));
        assert!(!err.to_string().contains("CANARYKEY"));
        assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stream_and_responses_parsers_fail_closed() {
        let streamed = parse_chat_stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"po\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"ng\"}}]}\n\ndata: [DONE]\n",
        )
        .unwrap();
        assert_eq!(streamed.content, "pong");
        assert!(
            parse_chat_stream("data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n").is_err()
        );
        let responses = parse_responses_stream(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n",
        )
        .unwrap();
        assert_eq!(responses.content, "hi");
        assert_eq!(responses.prompt_tokens, 3);
        assert!(parse_responses_stream(
            "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_usage_limit_exceeded\"}}}\n"
        )
        .is_err());
    }

    #[test]
    fn cleartext_off_loopback_is_refused() {
        let err = check_base_url("http://api.x.ai/v1").unwrap_err();
        assert!(err.contains("cleartext"));
        assert!(check_base_url("https://api.x.ai/v1").is_ok());
        assert!(check_base_url("http://127.0.0.1:9/v1").is_ok());
        assert!(check_base_url("http://user:secret@127.0.0.1:9/v1").is_err());
    }

    #[test]
    fn responses_body_forces_store_false_and_stream_true() {
        let config = RemoteConfig {
            name: "openai".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            timeout: Duration::from_secs(1),
            retries: 0,
            backoff: Duration::from_millis(1),
            stream: false,
            wire: Wire::Responses,
            credential: CredentialSource::File {
                path: "unused".into(),
                format: CredentialFormat::ChatgptSubscription,
            },
        };
        let body = request_body(&config, &request("ping", Some(Confidentiality::Public)));
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("messages").is_none());
    }
}
