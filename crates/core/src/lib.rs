//! Zest core: provider layer, agent loop, tool layer.
//!
//! Deliberately headless — no UI, no terminal assumptions. The CLI crate is one
//! front-end; a desktop app would be another.

pub mod agent;
#[cfg(test)]
mod alpha_prove;
pub mod anthropic;
pub mod auth;
pub mod cancel;
pub mod config;
pub mod error;
pub mod persist;
pub mod prompt;
pub mod provider;
pub mod routing;
pub mod runtime;
pub mod skills;
pub mod thread;
pub mod tools;
pub mod usage;

pub use agent::Agent;
pub use auth::{
    can_start_login, detect_all, gateway_auth_present, login_command, resolve_login, start_login,
    AuthStatus, LoginSpawn, ProviderSlot,
};
pub use cancel::CancelFlag;
pub use config::{Config, ProviderConfig, Routing, Rule, Target};
pub use provider::registry::{ProviderRegistry, Skipped};
pub use usage::{Ledger, ProviderUsage};
pub use anthropic::client::AnthropicClient;
pub use anthropic::types::{
    tool_result, tool_uses, Message, OutputConfig, Request, Thinking, ToolDef, ToolUse, Usage,
    DEFAULT_MODEL,
};
pub use error::{HarnessError, Result};
pub use persist::{PersistPriority, PersistWorker, DELTA_CHECKPOINT_MS};
pub use prompt::{
    compose_for_project, compose_system, custom_system_path, load_custom_system, save_custom_system,
    DEFAULT_SYSTEM,
};
pub use provider::anthropic::AnthropicProvider;
pub use provider::{
    catalogue_for_provider, catalogue_from_lists, normalize_effort, Completion, ModelSpec,
    Provider, ProviderDescriptor, RateLimitSnapshot, StreamEvent, TurnRequest, CODEX_KNOWN_MODELS,
    STANDARD_EFFORTS,
};
pub use routing::{Resolution, Router};
pub use runtime::{RuntimeBuilder, RuntimeSession};
pub use skills::{Skill, SkillSet, SkillSource, SkillSummary, INLINE_MAX_BYTES, MAX_SKILLS};
pub use thread::{
    new_id, StoredMessage, Thread, ThreadId, ThreadLoad, ThreadStore, ThreadSummary,
    ToolPart as ThreadToolPart, THREAD_FORMAT_VERSION, WIRE_FORMAT_ANTHROPIC_MESSAGES,
};
pub use tools::approval::{
    AllowApprover, ApprovalDecision, ApprovalPreview, ApprovalRequest, Approver, DenyApprover,
    ToolRisk,
};
pub use tools::delegate::{Delegate, DELEGATE_TOOL};
pub use tools::glob_files::GlobFiles;
pub use tools::grep::Grep;
pub use tools::list_dir::ListDir;
pub use tools::prepared::{PreImage, PreparedToolCall};
pub use tools::read_file::ReadFile;
pub use tools::sensitive::is_sensitive_path;
pub use tools::write_file::WriteFile;
pub use tools::{
    register_read_tools, register_skill_tools, register_write_tools, Tool, ToolRegistry,
};
