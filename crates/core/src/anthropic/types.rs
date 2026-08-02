//! Wire types for the Messages API.
//!
//! Assistant content blocks are kept as `serde_json::Value`, not a typed enum.
//! That is deliberate: the API adds block types over time (`server_tool_use`,
//! `fallback`, ...), and thinking blocks carry a `signature` that must be echoed
//! back byte-for-byte on the next turn or the request is rejected. Round-tripping
//! the raw JSON is lossless by construction; a typed enum would silently drop
//! anything it didn't know about. Typed access is via `tool_uses()` below.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const API_BASE: &str = "https://api.anthropic.com";
pub const API_VERSION: &str = "2023-06-01";
pub const DEFAULT_MODEL: &str = "claude-opus-5";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<Value>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Message {
            role: "user".into(),
            content: vec![json!({ "type": "text", "text": text.into() })],
        }
    }

    pub fn user_blocks(content: Vec<Value>) -> Self {
        Message {
            role: "user".into(),
            content,
        }
    }

    pub fn assistant(content: Vec<Value>) -> Self {
        Message {
            role: "assistant".into(),
            content,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Thinking {
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// `"summarized"` streams a readable summary. The API default is `"omitted"`,
    /// which still emits thinking blocks but with empty text — to a streaming UI
    /// that reads as a long stall before any output.
    pub display: &'static str,
}

impl Default for Thinking {
    fn default() -> Self {
        Thinking {
            kind: "adaptive",
            display: "summarized",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputConfig {
    pub effort: String,
}

/// Note what is absent: `temperature`, `top_p`, `top_k`. Those are rejected with
/// a 400 on Opus 5 — steering is done through the prompt, not sampling knobs.
#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub model: String,
    /// Caps thinking **and** response text together. Thinking is on by default
    /// on Opus 5, so a value tuned for text alone will truncate mid-answer.
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Pull the client-side tool calls out of an assistant turn.
///
/// Only `tool_use` blocks — `server_tool_use` runs on Anthropic's side and needs
/// no result from us.
pub fn tool_uses(content: &[Value]) -> Vec<ToolUse> {
    content
        .iter()
        .filter_map(|block| {
            if block.get("type")?.as_str()? != "tool_use" {
                return None;
            }
            Some(ToolUse {
                id: block.get("id")?.as_str()?.to_string(),
                name: block.get("name")?.as_str()?.to_string(),
                input: block.get("input").cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

/// Concatenate a turn's text blocks, ignoring thinking and tool blocks.
pub fn text_of(content: &[Value]) -> String {
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

/// A `tool_result` block. `tool_use_id` must match the `tool_use` it answers.
///
/// Every result for one assistant turn goes into a *single* user message —
/// splitting them across messages trains the model out of parallel tool calls.
pub fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Value {
    json!({
        "type": "tool_result",
        "tool_use_id": tool_use_id,
        "content": content,
        "is_error": is_error,
    })
}
