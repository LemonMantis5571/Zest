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
pub mod commands;
pub mod config;
pub mod config_edit;
pub mod credentials;
pub mod error;
pub mod fsutil;
pub mod gateway;
pub mod handoff;
pub mod persist;
pub mod prefs;
pub mod profile;
pub mod prompt;
pub mod provider;
pub mod reading_diff;
pub mod routing;
pub mod routing_edit;
pub mod runtime;
pub mod skills;
pub mod thread;
pub mod tools;
pub mod usage;

pub use agent::Agent;
pub use anthropic::client::AnthropicClient;
pub use anthropic::types::{
    tool_result, tool_uses, Message, OutputConfig, Request, Thinking, ToolDef, ToolUse, Usage,
    DEFAULT_MODEL,
};
pub use auth::{
    adopt_bundled_gateway, can_start_login, cliproxy_exe, cliproxy_install, detect_all,
    gateway_auth_present, login_command, resolve_login, start_login, uses_gateway_auth, AuthStatus,
    LoginProcess, LoginSpawn, ProviderSlot,
};
pub use cancel::{wait_cancel, CancelToken};
pub use commands::{
    expand as expand_command, expand_as as expand_command_as, parse_command, Expansion,
    ParsedCommand,
};
pub use config::{
    ensure_user_config, load_env, user_config_path, Config, ProviderConfig, Routing, Rule, Target,
    DEFAULT_USER_CONFIG,
};
pub use error::{HarnessError, Result};
pub use fsutil::{atomic_write, atomic_write_json, display_path, display_path_str};
pub use gateway::{
    ensure_running as ensure_gateway_running, gateway_dir, provision as provision_gateway,
    runtime as gateway_runtime, GatewayState, Provisioned, DEFAULT_PORT as GATEWAY_DEFAULT_PORT,
    GATEWAY_KEY_ENV,
};
pub use handoff::{ContextHandoff, MAX_HANDOFF_BYTES};
pub use persist::{PersistPriority, PersistWorker, DELTA_CHECKPOINT_MS};
pub use prefs::{ProjectSessionState, ProviderSessionPrefs};
pub use profile::{derive as derive_profile_stats, ChatFacts, DayPoint, ProfileStats};
pub use prompt::{
    compose_for_project, compose_system, compose_system_with_docs, custom_system_path, env_context,
    load_custom_system, load_project_docs, save_custom_system, truncate_chars, DEFAULT_SYSTEM,
    DELEGATION_SYSTEM, MAX_CUSTOM_PROMPT_BYTES, MAX_PROJECT_DOCS_BYTES, PROJECT_DOC_FILES,
};
pub use provider::anthropic::AnthropicProvider;
pub use provider::registry::{ProviderRegistry, Skipped};
pub use provider::{
    catalogue_for_provider, catalogue_from_lists, catalogue_without_efforts,
    context_window_for_model, descriptor_for_picker_id, descriptor_from_config, normalize_effort,
    probe, Completion, ModelSpec, Provider, ProviderDescriptor, RateLimitSnapshot, StreamEvent,
    TurnRequest, CODEX_KNOWN_MODELS, STANDARD_EFFORTS,
};
pub use reading_diff::{
    abridge as abridge_reading_diff, LineRange, ReadingDiffPlan, ReadingDiffResult,
};
pub use routing::{Resolution, Router};
pub use runtime::{RuntimeBuilder, RuntimeSession};
pub use skills::{
    Skill, SkillSet, SkillSource, SkillSummary, INLINE_BUDGET_BYTES, INLINE_MAX_BYTES, MAX_SKILLS,
    MAX_SKILL_BYTES,
};
pub use thread::{
    new_id, StoredMessage, Thread, ThreadCheckpoint, ThreadId, ThreadLoad, ThreadLoadError,
    ThreadStore, ThreadSummary, ToolPart as ThreadToolPart, THREAD_FORMAT_VERSION,
    WIRE_FORMAT_ANTHROPIC_MESSAGES,
};
pub use tools::approval::{
    AllowApprover, ApprovalDecision, ApprovalMode, ApprovalPolicy, ApprovalPreview,
    ApprovalRequest, Approver, DenyApprover, PolicyOutcome, ToolRisk,
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
    register_read_tools, register_skill_tools, register_write_tools, SkippedProvider, Tool,
    ToolMetadata, ToolOutcome, ToolRegistry, UsageDelta,
};
pub use usage::{
    HeadroomView, Ledger, MeasuredUsage, ProviderUsage, ProviderUsageView, UsageSnapshot,
};
