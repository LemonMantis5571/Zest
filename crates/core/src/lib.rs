//! Zest core: provider layer, agent loop, tool layer.
//!
//! Deliberately headless — no UI, no terminal assumptions. The CLI crate is one
//! front-end; a desktop app would be another.

pub mod agent;
pub mod anthropic;
pub mod auth;
pub mod config;
pub mod error;
pub mod provider;
pub mod routing;
pub mod tools;
pub mod usage;

pub use agent::Agent;
pub use auth::{detect_all, AuthStatus, ProviderSlot};
pub use config::{Config, ProviderConfig, Routing, Rule, Target};
pub use provider::registry::{ProviderRegistry, Skipped};
pub use usage::{Ledger, ProviderUsage};
pub use anthropic::client::AnthropicClient;
pub use anthropic::types::{
    tool_result, tool_uses, Message, OutputConfig, Request, Thinking, ToolDef, ToolUse, Usage,
    DEFAULT_MODEL,
};
pub use error::{HarnessError, Result};
pub use provider::anthropic::AnthropicProvider;
pub use provider::{Completion, Provider, RateLimitSnapshot, StreamEvent, TurnRequest};
pub use routing::{Resolution, Router};
pub use tools::delegate::{Delegate, DELEGATE_TOOL};
pub use tools::read_file::ReadFile;
pub use tools::{Tool, ToolRegistry};
