//! HTTP client for llama.cpp's `llama-server` OpenAI-compatible endpoint.
//!
//! The server is a separate process. This adapter does not spawn it and does
//! not see the weights. Start it with `scripts/build-llama.sh` and a GGUF
//! checked by `scripts/fetch-model.sh`.

use super::{bump, openai_body, Engine, EngineError, EngineRequest, EngineResponse, ToolCall};
use serde_json::Value;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

pub struct LlamaEngine {
    name: String,
    base_url: String,
    timeout: Duration,
    calls: AtomicU64,
}

impl LlamaEngine {
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, timeout_secs: u64) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(timeout_secs.max(1)),
            calls: AtomicU64::new(0),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

impl Engine for LlamaEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn calls(&self) -> u64 {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn complete(&self, request: &EngineRequest) -> Result<EngineResponse, EngineError> {
        bump(&self.calls);
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let response = agent
            .post(&self.endpoint())
            .send_json(openai_body(request))
            .map_err(|err| EngineError::Failed {
                engine: self.name.clone(),
                message: err.to_string(),
            })?;
        let body: Value = response.into_json().map_err(|err| EngineError::Failed {
            engine: self.name.clone(),
            message: err.to_string(),
        })?;
        parse_openai(&self.name, &body)
    }
}

pub fn parse_openai(engine: &str, body: &Value) -> Result<EngineResponse, EngineError> {
    let message = body
        .pointer("/choices/0/message")
        .ok_or_else(|| EngineError::Failed {
            engine: engine.into(),
            message: "llama-server response has no choices[0].message".into(),
        })?;
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
    let prompt_tokens = body
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let completion_tokens = body
        .pointer("/usage/completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Ok(EngineResponse {
        content,
        tool_calls,
        prompt_tokens,
        completion_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{openai_body, ChatMessage, EngineRequest};
    use serde_json::json;

    #[test]
    fn body_strips_nothing_but_roles_and_content() {
        let body = openai_body(&EngineRequest {
            model: "local".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
            confidentiality: None,
        });
        assert_eq!(body["messages"][0]["content"], "hi");
        assert!(body["messages"][0].get("dj_labels").is_none());
    }

    #[test]
    fn parses_tool_call() {
        let parsed = parse_openai(
            "llama",
            &json!({
                "choices": [{
                    "message": {
                        "content": "",
                        "tool_calls": [{
                            "function": {"name": "email.send", "arguments": "{\"to\":\"a@b.c\"}"}
                        }]
                    }
                }],
                "usage": {"prompt_tokens": 3, "completion_tokens": 1}
            }),
        )
        .unwrap();
        assert_eq!(parsed.tool_calls[0].name, "email.send");
        assert_eq!(parsed.prompt_tokens, 3);
    }
}
