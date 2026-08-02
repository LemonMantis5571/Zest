//! Provider abstraction.
//!
//! A provider is one authenticated backend: Anthropic on a Claude login, Codex on
//! a ChatGPT login, Gemini on an Antigravity login. Routing across them is the
//! point of this harness, so the loop must not know which one it is talking to.
//!
//! Crucially, *how* a provider is reached is an implementation detail behind this
//! trait. Anthropic is reached natively; another backend may be reached through a
//! gateway that re-exposes it as the Messages API. The router cannot tell the
//! difference, which is what lets a gateway be swapped for a native client later
//! without anything above noticing.

pub mod anthropic;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anthropic::types::{Message, ToolDef, Usage};
use crate::auth::AuthStatus;
use crate::error::Result;

/// One model turn, described without reference to any provider's wire format.
///
/// `effort` and `thinking` are *requests*, not commands. A provider maps them
/// onto its own controls or ignores them — which is why the agent loop no longer
/// carries a flag for whether the backend understands Anthropic's extensions.
pub struct TurnRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    /// Budgets reasoning *and* response text together on providers that think.
    pub max_tokens: u32,
    pub effort: Option<String>,
    pub thinking: bool,
}

/// Incremental output, for rendering. Everything here is also present in the
/// final `Completion` — a front-end that ignores these still gets a correct turn.
#[derive(Debug)]
pub enum StreamEvent<'a> {
    Text(&'a str),
    Thinking(&'a str),
    ToolCallStart { name: &'a str },
}

/// Throughput headroom as reported by the provider.
///
/// This answers "can I send right now", **not** "how much of my plan is left".
/// Those are different numbers and merging them produces a confident lie — see
/// `memory/decisions.md`. Subscription quota has no equivalent endpoint on any of
/// the CLI-login providers, so it is metered locally instead.
///
/// `None` on a provider means it reports nothing, which is itself information the
/// ledger has to represent rather than silently treat as zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitSnapshot {
    pub requests_limit: Option<u64>,
    pub requests_remaining: Option<u64>,
    pub requests_reset: Option<String>,
    pub input_tokens_remaining: Option<u64>,
    pub output_tokens_remaining: Option<u64>,
    /// RFC 3339, stored raw. Nothing parses dates yet, so no date dependency.
    pub tokens_reset: Option<String>,
    pub retry_after_secs: Option<u64>,
}

impl RateLimitSnapshot {
    /// True when the provider reported nothing at all.
    pub fn is_empty(&self) -> bool {
        self.requests_limit.is_none()
            && self.requests_remaining.is_none()
            && self.requests_reset.is_none()
            && self.input_tokens_remaining.is_none()
            && self.output_tokens_remaining.is_none()
            && self.tokens_reset.is_none()
            && self.retry_after_secs.is_none()
    }
}

#[derive(Debug)]
pub struct Completion {
    /// The assistant turn's content blocks, in index order. Push this back into
    /// the message history verbatim.
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
    pub limits: Option<RateLimitSnapshot>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Stable identifier used by config, routing rules, and the usage ledger.
    fn id(&self) -> &str;

    fn default_model(&self) -> &str;

    /// Whether this provider can be used right now. Rendered by the launch
    /// picker, and consulted before routing a task here.
    fn auth_status(&self) -> AuthStatus;

    /// The callback must be `Send`: provider futures are `Send` so that delegated
    /// sub-agents can run concurrently on the tokio runtime.
    async fn stream_turn(
        &self,
        req: &TurnRequest,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> Result<Completion>;
}
