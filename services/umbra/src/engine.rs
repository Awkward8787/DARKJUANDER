//! Swappable inference engines. The mock engine is the CI default.
//! The llama.cpp adapter speaks HTTP to a `llama-server` the operator starts.

#![forbid(unsafe_code)]

pub mod llama;
pub mod mock;
pub mod remote;

use dj_label::Confidentiality;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct EngineRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Cloud adapters send a request only when this is `Some(Public)`.
    /// `None` fails closed so a caller cannot forget the label.
    pub confidentiality: Option<Confidentiality>,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug)]
pub struct EngineResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("engine `{engine}` failed: {message}")]
    Failed { engine: String, message: String },
}

pub trait Engine: Send + Sync {
    fn name(&self) -> &str;
    fn calls(&self) -> u64;
    fn complete(&self, request: &EngineRequest) -> Result<EngineResponse, EngineError>;
}

pub(crate) fn bump(counter: &AtomicU64) -> u64 {
    counter.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn openai_body(request: &EngineRequest) -> Value {
    serde_json::json!({
        "model": request.model,
        "messages": request.messages.iter().map(|message| {
            serde_json::json!({"role": message.role, "content": message.content})
        }).collect::<Vec<_>>()
    })
}
