//! Desktop front-end: provider picker + chat session.
//!
//! Connect is a native shell over vendor OAuth (no token exchange in Zest).
//! Chat drives the same `Agent` loop as the CLI, streaming events into the UI.
//! Thread projection is persisted under `<workspace>/.zest/threads/`.

mod attachments;
mod context_meter;
mod session;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;
#[cfg(feature = "export-bindings")]
use ts_rs::TS;
use zest_core::routing_edit::{routing_document, validate_rules, RuleInput};
use zest_core::{
    can_start_login, compose_system_with_docs, descriptor_for_picker_id, descriptor_from_config,
    detect_all, display_path, env_context, load_custom_system, load_project_docs, new_id,
    save_custom_system, start_login as core_start_login, truncate_chars, ApprovalDecision,
    ApprovalMode, ApprovalPolicy, ApprovalRequest, Approver, AuthStatus, Config, HarnessError,
    Ledger, PersistPriority, PersistWorker, ProjectSessionState, ProviderRegistry, ProviderSlot,
    RuntimeBuilder, SkillSet, SkillSummary, StoredMessage, StreamEvent, Thread, ThreadLoadError,
    ThreadStore, ThreadSummary, ToolMetadata, ToolRisk, UsageSnapshot, DEFAULT_SYSTEM,
};

use attachments::{
    build_user_content, format_display_message, has_images, has_usable_attachment,
    prepare_image_bytes, prepare_paths, AttachmentInput, PreparedAttachment,
};
use context_meter::{estimate_context, ContextUsageView};
use session::{Session, SessionController, SessionError};

/// Providers shown in the launch picker. BYOK stays terminal-only for now.
const PICKER_IDS: &[&str] = &["codex", "claude", "antigravity"];

/// Turn-scoped pending approval waiters (not persisted).
struct ApprovalHub {
    /// Active turn that may own waiters. Resolves outside this turn are rejected.
    active_turn: Mutex<Option<String>>,
    senders: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    receivers: Mutex<HashMap<String, oneshot::Receiver<ApprovalDecision>>>,
}

impl ApprovalHub {
    fn new() -> Self {
        Self {
            active_turn: Mutex::new(None),
            senders: Mutex::new(HashMap::new()),
            receivers: Mutex::new(HashMap::new()),
        }
    }

    fn begin_turn(&self, turn_id: &str) {
        if let Ok(mut g) = self.active_turn.lock() {
            *g = Some(turn_id.to_string());
        }
    }

    fn prepare(&self, approval_id: &str) {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut senders) = self.senders.lock() {
            senders.insert(approval_id.to_string(), tx);
        }
        if let Ok(mut receivers) = self.receivers.lock() {
            receivers.insert(approval_id.to_string(), rx);
        }
    }

    /// Anything that is not an explicit allow — a dropped sender, a poisoned
    /// lock, an unknown id — resolves to Deny.
    async fn wait(&self, approval_id: &str) -> ApprovalDecision {
        let rx = {
            let mut receivers = match self.receivers.lock() {
                Ok(g) => g,
                Err(_) => return ApprovalDecision::Deny,
            };
            receivers.remove(approval_id)
        };
        match rx {
            Some(rx) => rx.await.unwrap_or(ApprovalDecision::Deny),
            None => ApprovalDecision::Deny,
        }
    }

    fn resolve(&self, approval_id: &str, decision: ApprovalDecision) -> Result<(), String> {
        let turn_alive = self
            .active_turn
            .lock()
            .map_err(|_| "approval lock poisoned".to_string())?
            .is_some();
        if !turn_alive {
            return Err("no active turn for approval".into());
        }
        let mut senders = self
            .senders
            .lock()
            .map_err(|_| "approval lock poisoned".to_string())?;
        let tx = senders
            .remove(approval_id)
            .ok_or_else(|| "no pending approval with that id".to_string())?;
        let _ = tx.send(decision);
        Ok(())
    }

    /// Deny every waiter. Call after cancelling the turn token.
    fn clear(&self) {
        if let Ok(mut senders) = self.senders.lock() {
            for (_, tx) in senders.drain() {
                let _ = tx.send(ApprovalDecision::Deny);
            }
        }
        if let Ok(mut receivers) = self.receivers.lock() {
            receivers.clear();
        }
        if let Ok(mut g) = self.active_turn.lock() {
            *g = None;
        }
    }
}

struct HubApprover {
    hub: Arc<ApprovalHub>,
}

#[async_trait]
impl Approver for HubApprover {
    async fn prepare(&self, approval_id: &str) {
        self.hub.prepare(approval_id);
    }

    async fn decide(&self, request: &ApprovalRequest) -> ApprovalDecision {
        self.hub.wait(&request.approval_id).await
    }
}

/// The desktop opens in Auto: writes apply, allowlisted commands run, anything
/// else asks. Core's own default is Manual — see `ApprovalMode` — because a
/// library with no wired-up gate must not be permissive. Choosing the product
/// default here is the front-end's job.
const DESKTOP_DEFAULT_MODE: ApprovalMode = ApprovalMode::Auto;

struct AppState {
    sessions: SessionController,
    approvals: Arc<ApprovalHub>,
    persist: Mutex<Option<PersistWorker>>,
    /// Preferred project root (folder picker / last-workspace). Falls back to cwd.
    workspace_root: Mutex<Option<PathBuf>>,
    /// Mode + session grants. Outlives any one project so switching folders
    /// does not silently reset the user's chosen permission level.
    policy: Arc<Mutex<ApprovalPolicy>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ModelCapability.ts", rename_all = "camelCase")
)]
struct ModelCapability {
    id: String,
    efforts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ProviderView.ts", rename_all = "camelCase")
)]
struct ProviderView {
    id: String,
    label: String,
    method: String,
    status_kind: String,
    status_label: String,
    detail: String,
    selectable: bool,
    can_connect: bool,
    /// Present in `zest.toml` / env fallback — Rust is authoritative for availability.
    configured: bool,
    default_model: String,
    models: Vec<ModelCapability>,
}

fn provider_view_from_slot(slot: &ProviderSlot, config: &Config) -> ProviderView {
    let (status_kind, status_label, detail) = match &slot.status {
        AuthStatus::Ready { account } => (
            "ready".into(),
            "Signed in".into(),
            account.clone().unwrap_or_else(|| slot.method.to_string()),
        ),
        AuthStatus::Unknown { reason } => {
            let detail = if reason.contains("outside a readable file") {
                "Installed — session stored outside a readable file".into()
            } else {
                format!("Installed — {reason}")
            };
            ("unknown".into(), "Unverified".into(), detail)
        }
        AuthStatus::NotLoggedIn { fix } => (
            "not_logged_in".into(),
            "Not signed in".into(),
            if fix.starts_with("Connect") {
                fix.clone()
            } else {
                format!("Run: {fix}")
            },
        ),
        AuthStatus::Unconfigured => (
            "unconfigured".into(),
            "Not configured".into(),
            "No key set".into(),
        ),
    };

    let (configured, descriptor) = match config.providers.get(slot.id) {
        Some(pc) => (true, descriptor_from_config(slot.id, pc)),
        None => (false, descriptor_for_picker_id(slot.id)),
    };

    // Being signed in is not the same as being reachable. A vendor CLI can hold
    // a perfectly good session for a provider this project has no entry for,
    // and offering it as ready meant Continue failed *after* the click with
    // "not configured". Say so on the row instead.
    let (status_kind, status_label, detail) = if configured {
        (status_kind, status_label, detail)
    } else {
        let where_to = zest_core::user_config_path()
            .map(|p| display_path(p.as_path()))
            .unwrap_or_else(|| "~/.zest/zest.toml".to_string());
        (
            "unconfigured".to_string(),
            "Not configured".to_string(),
            match slot.status {
                AuthStatus::Ready { .. } => {
                    format!("Signed in, but no provider entry — add one to {where_to}")
                }
                _ => format!("No provider entry in zest.toml or {where_to}"),
            },
        )
    };

    ProviderView {
        id: slot.id.to_string(),
        label: slot.label.to_string(),
        method: slot.method.to_string(),
        status_kind,
        status_label,
        detail,
        // Both halves are required: a signed-in provider with no config cannot
        // serve a turn, and a configured provider with no sign-in cannot either.
        selectable: slot.status.selectable() && configured,
        can_connect: can_start_login(slot.id),
        configured,
        default_model: descriptor.default_model,
        models: descriptor
            .models
            .into_iter()
            .map(|m| ModelCapability {
                id: m.id,
                efforts: m.efforts,
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ToolMetaView.ts", rename_all = "camelCase")
)]
struct SkippedProviderView {
    provider_id: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ToolMetaView.ts", rename_all = "camelCase")
)]
struct UsageDeltaView {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
}

/// Desktop wire view of core `ToolMetadata` (ts-rs exportable).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ToolMetaView.ts", rename_all = "snake_case")
)]
enum ToolMetaView {
    Delegation {
        provider_id: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "export-bindings", ts(optional))]
        routing_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<SkippedProviderView>,
        usage_delta: UsageDeltaView,
    },
}

impl From<ToolMetadata> for ToolMetaView {
    fn from(meta: ToolMetadata) -> Self {
        match meta {
            ToolMetadata::Delegation {
                provider_id,
                model,
                routing_kind,
                skipped,
                usage_delta,
            } => Self::Delegation {
                provider_id,
                model,
                routing_kind,
                skipped: skipped
                    .into_iter()
                    .map(|s| SkippedProviderView {
                        provider_id: s.provider_id,
                        reason: s.reason,
                    })
                    .collect(),
                usage_delta: UsageDeltaView {
                    requests: usage_delta.requests,
                    input_tokens: usage_delta.input_tokens,
                    output_tokens: usage_delta.output_tokens,
                },
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginStarted {
    browser_title: String,
    browser_body: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "SessionInfo.ts", rename_all = "camelCase")
)]
struct SessionInfo {
    session_id: String,
    provider: String,
    label: String,
    model: String,
    effort: String,
    root: String,
    thread_id: String,
    /// Rust-authoritative catalogue for the active provider (UI may only add labels).
    default_model: String,
    models: Vec<ModelCapability>,
    /// UI projects these as `ChatMessage[]` (see `types.ts`); keep codegen free of StoredMessage.
    #[cfg_attr(feature = "export-bindings", ts(type = "unknown[]"))]
    messages: Vec<StoredMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export-bindings", ts(optional))]
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ChatEvent.ts", rename_all = "snake_case")
)]
enum ChatEvent {
    User {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        text: String,
    },
    /// Empty streaming assistant row — emitted right after `User` so the UI can
    /// show Thinking… before the first provider delta.
    AssistantStart {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        /// Slash command that produced this turn, when one did. The UI titles
        /// the answer with it — Rust decides, because only Rust knows whether
        /// a leading `/token` matched a real skill.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    TextDelta {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        text: String,
    },
    ThinkingDelta {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        text: String,
    },
    ToolCallStart {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        name: String,
        id: String,
    },
    ToolCallResult {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        name: String,
        id: String,
        summary: String,
        #[serde(rename = "isError")]
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "export-bindings", ts(optional))]
        metadata: Option<ToolMetaView>,
    },
    ApprovalNeeded {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        approval_id: String,
        tool_name: String,
        tool_call_id: String,
        risk: String,
        path: String,
        summary: String,
        diff: String,
    },
    Done {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
    },
    Error {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
        message: String,
        /// Provider to offer a Reconnect for, when the failure is one that only
        /// signing in again can fix. `None` for everything else — a Reconnect
        /// button on a rate limit would send the user through OAuth for nothing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reconnect_provider: Option<String>,
    },
    Cancelled {
        session_id: String,
        thread_id: String,
        turn_id: String,
        message_id: String,
    },
    Warning {
        session_id: String,
        thread_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "export-bindings", ts(optional))]
        turn_id: Option<String>,
        message: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopError {
    code: String,
    message: String,
}

fn desktop_err(code: &str, message: impl Into<String>) -> String {
    let message = message.into();
    serde_json::to_string(&DesktopError {
        code: code.into(),
        message: message.clone(),
    })
    .unwrap_or(message)
}

fn map_session_err(e: SessionError) -> String {
    desktop_err(e.code(), e.message())
}

fn load_workspace_config(state: &AppState) -> Config {
    match resolve_workspace_root(state) {
        Ok(root) => Config::find(&root).unwrap_or_else(|_| Config::env_fallback()),
        Err(_) => Config::env_fallback(),
    }
}

#[tauri::command]
fn list_providers(state: State<'_, AppState>) -> Vec<ProviderView> {
    let config = load_workspace_config(&state);
    detect_all()
        .iter()
        .filter(|s| PICKER_IDS.contains(&s.id))
        .map(|s| provider_view_from_slot(s, &config))
        .collect()
}

#[tauri::command]
fn refresh_providers(state: State<'_, AppState>) -> Vec<ProviderView> {
    list_providers(state)
}

#[tauri::command]
fn usage_snapshot() -> UsageSnapshot {
    Ledger::load().snapshot()
}

/// Send one minimal turn to prove the provider can actually serve.
///
/// A credentials file on disk is not a working session — the gateway can hold
/// an account it has put in cooldown, and that never shows up locally. Called
/// after a sign-in so "Signed in" is something observed rather than assumed.
/// Costs a few tokens, which is why it is not on every render.
#[tauri::command]
async fn verify_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    zest_core::load_env();
    let root = resolve_workspace_root(&state)?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;
    let (registry, skipped) = ProviderRegistry::from_config(&config);

    let provider = registry.get(&id).ok_or_else(|| {
        skipped
            .iter()
            .find(|s| s.id == id)
            .map(|s| format!("{id} could not be loaded: {}", s.reason))
            .unwrap_or_else(|| format!("provider `{id}` is not configured"))
    })?;

    let model = provider.default_model().to_string();
    zest_core::probe(provider.as_ref(), &model)
        .await
        .map_err(|e| format_turn_error(&e))
}

#[tauri::command]
fn start_login(id: String) -> Result<LoginStarted, String> {
    let spawn = core_start_login(&id)?;
    Ok(LoginStarted {
        browser_title: spawn.browser_title.to_string(),
        browser_body: spawn.browser_body.to_string(),
    })
}

fn canonicalize_dir(path: PathBuf) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err(format!("not a directory: {}", path.display()));
    }
    path.canonicalize().or(Ok(path))
}

fn cwd_workspace() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    canonicalize_dir(cwd)
}

fn load_persisted_workspace() -> Option<PathBuf> {
    let path = dirs::config_dir()?.join("zest").join("last-workspace");
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(value);
    canonicalize_dir(candidate).ok()
}

fn persist_workspace(root: &Path) -> Result<(), String> {
    let path = zest_config_dir()?.join("last-workspace");
    std::fs::write(&path, display_path(root)).map_err(|e| e.to_string())?;
    remember_workspace(root);
    Ok(())
}

const KNOWN_WORKSPACES_FILE: &str = "known-workspaces.json";
const MAX_KNOWN_WORKSPACES: usize = 40;

fn known_workspaces_path() -> Result<PathBuf, String> {
    Ok(zest_config_dir()?.join(KNOWN_WORKSPACES_FILE))
}

fn load_known_workspaces() -> Vec<PathBuf> {
    let Ok(path) = known_workspaces_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<String>>(&raw) else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|s| {
            let p = PathBuf::from(s.trim());
            if p.as_os_str().is_empty() {
                return None;
            }
            canonicalize_dir(p).ok()
        })
        .collect()
}

fn remember_workspace(root: &Path) {
    let Ok(root) = canonicalize_dir(root.to_path_buf()) else {
        return;
    };
    let mut list = load_known_workspaces();
    list.retain(|p| p != &root);
    list.insert(0, root);
    list.truncate(MAX_KNOWN_WORKSPACES);
    let display: Vec<String> = list.iter().map(|p| display_path(p)).collect();
    if let Ok(path) = known_workspaces_path() {
        if let Ok(raw) = serde_json::to_string_pretty(&display) {
            let _ = std::fs::write(path, raw);
        }
    }
}

fn resolve_workspace_root(state: &AppState) -> Result<PathBuf, String> {
    if let Ok(guard) = state.workspace_root.lock() {
        if let Some(root) = guard.as_ref() {
            return Ok(root.clone());
        }
    }
    if let Some(persisted) = load_persisted_workspace() {
        if let Ok(mut guard) = state.workspace_root.lock() {
            *guard = Some(persisted.clone());
        }
        return Ok(persisted);
    }
    cwd_workspace()
}

fn set_workspace_root(state: &AppState, root: PathBuf) -> Result<PathBuf, String> {
    let root = canonicalize_dir(root)?;
    persist_workspace(&root)?;
    if let Ok(mut guard) = state.workspace_root.lock() {
        *guard = Some(root.clone());
    }
    // Drop any stale persist worker bound to the previous project.
    if let Ok(mut guard) = state.persist.lock() {
        *guard = None;
    }
    Ok(root)
}

fn open_store(root: &std::path::Path) -> Result<ThreadStore, String> {
    ThreadStore::open(root).map_err(|e| e.to_string())
}

fn ensure_persist(state: &AppState, root: &std::path::Path) -> Result<PersistWorker, String> {
    let mut guard = state
        .persist
        .lock()
        .map_err(|_| "persist lock poisoned".to_string())?;
    if let Some(worker) = guard.as_ref() {
        return Ok(worker.clone());
    }
    let worker = PersistWorker::spawn(root).map_err(|e| e.to_string())?;
    *guard = Some(worker.clone());
    Ok(worker)
}

fn resolve_thread(
    root: &std::path::Path,
    store: &ThreadStore,
    provider_id: &str,
) -> Result<(Thread, Option<String>), String> {
    let mut state = ProjectSessionState::load(root, provider_id);
    if let Some(id) = state.get(provider_id).thread_id {
        match store.load_for_provider(&id, provider_id) {
            Ok(loaded) => {
                let mut thread = loaded.thread;
                // Pin missing owner once; never rewrite a different owner.
                thread
                    .ensure_provider(provider_id)
                    .map_err(|e| e.to_string())?;
                return Ok((thread, loaded.warning));
            }
            Err(ThreadLoadError::Corrupt { detail, .. }) => {
                let thread = store
                    .create_for_provider(provider_id)
                    .map_err(|e| e.to_string())?;
                state.set_thread(provider_id, &thread.id);
                let _ = state.save(root);
                return Ok((
                    thread,
                    Some(format!("history not saved: {detail}; started a new thread")),
                ));
            }
            Err(ThreadLoadError::ProviderMismatch { .. })
            | Err(ThreadLoadError::Missing(_))
            | Err(ThreadLoadError::UnsupportedVersion { .. }) => {
                // Fall through to a fresh provider-owned thread.
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let thread = store
        .create_for_provider(provider_id)
        .map_err(|e| e.to_string())?;
    state.set_thread(provider_id, &thread.id);
    let _ = state.save(root);
    Ok((thread, None))
}

fn persist_provider_thread(
    root: &std::path::Path,
    provider_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    let mut state = ProjectSessionState::load(root, provider_id);
    state.set_thread(provider_id, thread_id);
    state.save(root).map_err(|e| e.to_string())
}

fn persist_provider_model_effort(
    root: &std::path::Path,
    provider_id: &str,
    model: &str,
    effort: &str,
) -> Result<(), String> {
    let mut state = ProjectSessionState::load(root, provider_id);
    state.set_model_effort(provider_id, model, effort);
    state.save(root).map_err(|e| e.to_string())
}

fn session_capabilities(session: &Session) -> (String, Vec<ModelCapability>) {
    let descriptor = session.agent.descriptor();
    (
        descriptor.default_model,
        descriptor
            .models
            .into_iter()
            .map(|m| ModelCapability {
                id: m.id,
                efforts: m.efforts,
            })
            .collect(),
    )
}

fn session_info_from(session: &Session, warning: Option<String>) -> SessionInfo {
    let (default_model, models) = session_capabilities(session);
    SessionInfo {
        session_id: session.session_id.clone(),
        provider: session.provider_id.clone(),
        label: session.provider_label.clone(),
        model: session.model.clone(),
        effort: session.effort.clone(),
        root: display_path(&session.root),
        thread_id: session.thread_id.clone(),
        default_model,
        models,
        messages: session.thread.messages.clone(),
        warning,
    }
}

fn apply_event_to_thread(thread: &mut Thread, event: &ChatEvent) {
    match event {
        ChatEvent::User {
            message_id, text, ..
        } => thread.apply_user(message_id, text),
        ChatEvent::AssistantStart {
            message_id,
            command,
            ..
        } => {
            thread.apply_assistant_start(message_id, command.as_deref());
        }
        ChatEvent::TextDelta {
            message_id, text, ..
        } => thread.apply_text_delta(message_id, text),
        ChatEvent::ThinkingDelta {
            message_id, text, ..
        } => thread.apply_thinking_delta(message_id, text),
        ChatEvent::ToolCallStart {
            message_id,
            name,
            id,
            ..
        } => thread.apply_tool_start(message_id, id, name),
        ChatEvent::ToolCallResult {
            message_id,
            name,
            id,
            summary,
            is_error,
            metadata,
            ..
        } => {
            let core_meta = metadata.clone().map(|m| match m {
                ToolMetaView::Delegation {
                    provider_id,
                    model,
                    routing_kind,
                    skipped,
                    usage_delta,
                } => ToolMetadata::Delegation {
                    provider_id,
                    model,
                    routing_kind,
                    skipped: skipped
                        .into_iter()
                        .map(|s| zest_core::SkippedProvider {
                            provider_id: s.provider_id,
                            reason: s.reason,
                        })
                        .collect(),
                    usage_delta: zest_core::UsageDelta {
                        requests: usage_delta.requests,
                        input_tokens: usage_delta.input_tokens,
                        output_tokens: usage_delta.output_tokens,
                    },
                },
            });
            thread.apply_tool_result(message_id, id, name, summary, *is_error, core_meta);
        }
        ChatEvent::ApprovalNeeded {
            message_id,
            approval_id,
            tool_name,
            tool_call_id,
            path,
            summary,
            diff,
            ..
        } => thread.apply_approval_needed(
            message_id,
            tool_call_id,
            tool_name,
            approval_id,
            path,
            summary,
            diff,
        ),
        ChatEvent::Done { message_id, .. } => thread.apply_done(message_id),
        ChatEvent::Error {
            message_id,
            message,
            ..
        } => thread.apply_error(message_id, message),
        ChatEvent::Cancelled { message_id, .. } => {
            thread.apply_error(message_id, "turn cancelled");
        }
        ChatEvent::Warning { .. } => {}
    }
}

fn event_priority(event: &ChatEvent) -> PersistPriority {
    match event {
        ChatEvent::TextDelta { .. } | ChatEvent::ThinkingDelta { .. } => PersistPriority::Delta,
        _ => PersistPriority::Immediate,
    }
}

#[tauri::command]
fn start_session(
    state: State<'_, AppState>,
    id: String,
    model: Option<String>,
    effort: Option<String>,
) -> Result<SessionInfo, String> {
    zest_core::load_env();
    state.sessions.require_idle().map_err(map_session_err)?;
    state.approvals.clear();

    let slot = detect_all()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("unknown provider `{id}`"))?;

    if !slot.status.selectable() {
        return Err(format!("{} is not ready — connect it first", slot.label));
    }

    persist_choice(&id)?;

    let root = resolve_workspace_root(&state)?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;

    let prefs = ProjectSessionState::load(&root, &id).get(&id);

    // Only what the caller explicitly asked for is `explicit`. The sticky
    // values go in as *remembered*, which RuntimeBuilder drops instead of
    // erroring when they do not fit this provider — otherwise one stale entry
    // makes the provider impossible to select and therefore impossible to fix.
    let explicit_model = model.filter(|m| !m.trim().is_empty());
    let explicit_effort = effort
        .filter(|e| !e.trim().is_empty())
        .map(|e| normalize_effort(&e));

    let store = open_store(&root)?;
    let (mut thread, load_warning) = resolve_thread(&root, &store, &id)?;
    thread.ensure_provider(&id).map_err(|e| e.to_string())?;
    persist_provider_thread(&root, &id, &thread.id)?;

    let approver: Arc<dyn Approver> = Arc::new(HubApprover {
        hub: state.approvals.clone(),
    });

    let mut builder = RuntimeBuilder::new(&root)
        .with_config(config)
        .with_provider(&id)
        .with_system(DEFAULT_SYSTEM)
        .with_approver(approver)
        .with_policy(state.policy.clone())
        .with_remembered_options(prefs.model, prefs.effort)
        .enable_delegate(true)
        .register_write_tools(true)
        // Every non-allowlisted command reaches HubApprover, which is the same
        // card `write_file` already uses.
        .register_exec_tools(true);
    if let Some(model) = explicit_model {
        builder = builder.with_model(model);
    }
    if let Some(effort) = explicit_effort {
        builder = builder.with_effort(effort);
    }

    let runtime = builder.build().map_err(|e| e.to_string())?;
    let runtime_warnings = runtime.warnings.clone();
    let mut agent = runtime.agent;
    agent.messages = thread.agent_messages.clone();

    persist_provider_model_effort(&root, &id, &runtime.model, &runtime.effort)?;

    let session = Session {
        session_id: String::new(),
        agent,
        model: runtime.model,
        effort: runtime.effort,
        provider_id: id,
        provider_label: slot.label.to_string(),
        root,
        thread_id: thread.id.clone(),
        thread,
        base_system: runtime.base_system,
        skills: runtime.skills,
    };

    state
        .sessions
        .set_session(session)
        .map_err(map_session_err)?;

    // A dropped preference is worth saying out loud — otherwise the picker just
    // shows a different model than last time with no explanation.
    let warning = merge_warnings(load_warning, runtime_warnings);

    let info = state
        .sessions
        .session_info_snapshot(|s| session_info_from(s, warning.clone()))
        .map_err(map_session_err)?
        .ok_or_else(|| map_session_err(SessionError::NoSession))?;
    Ok(info)
}

/// Join a thread-load warning with any runtime warnings into the single slot
/// `SessionInfo` has for them.
fn merge_warnings(load_warning: Option<String>, runtime: Vec<String>) -> Option<String> {
    let mut all: Vec<String> = load_warning.into_iter().collect();
    all.extend(runtime);
    (!all.is_empty()).then(|| all.join("; "))
}

#[tauri::command]
fn update_session_options(
    state: State<'_, AppState>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state
        .sessions
        .with_session_mut(|session| -> Result<SessionInfo, String> {
            let next_model = model
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| session.model.clone());
            let next_effort = effort
                .filter(|e| !e.trim().is_empty())
                .map(|e| normalize_effort(&e))
                .unwrap_or_else(|| session.effort.clone());
            session.agent.validate_options(&next_model, &next_effort)?;
            session.model = next_model.clone();
            session.agent.model = next_model;
            session.effort = next_effort.clone();
            session.agent.effort = next_effort;
            persist_provider_model_effort(
                &session.root,
                &session.provider_id,
                &session.model,
                &session.effort,
            )?;
            Ok(session_info_from(session, None))
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

/// Atomically reset sticky model+effort for the active provider (clears prefs).
#[tauri::command]
fn reset_session_options(state: State<'_, AppState>) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state
        .sessions
        .with_session_mut(|session| -> Result<SessionInfo, String> {
            let descriptor = session.agent.descriptor();
            let next_model = descriptor.default_model.clone();
            let next_effort = "high".to_string();
            session.agent.validate_options(&next_model, &next_effort)?;
            session.model = next_model.clone();
            session.agent.model = next_model;
            session.effort = next_effort.clone();
            session.agent.effort = next_effort;
            let mut prefs = ProjectSessionState::load(&session.root, &session.provider_id);
            prefs.clear_model_effort(&session.provider_id);
            prefs.save(&session.root).map_err(|e| e.to_string())?;
            Ok(session_info_from(session, None))
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[tauri::command]
fn list_threads(state: State<'_, AppState>) -> Result<Vec<ThreadSummary>, String> {
    state
        .sessions
        .with_session_mut(|session| {
            open_store(&session.root)?
                .list_for_provider(&session.provider_id)
                .map_err(|e| e.to_string())
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectChats {
    name: String,
    path: String,
    active: bool,
    threads: Vec<ThreadSummary>,
}

fn project_display_name(root: &Path) -> String {
    root.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| display_path(root))
}

/// Chats grouped by known project folders (MRU), for the sidebar.
#[tauri::command]
fn list_chat_projects(state: State<'_, AppState>) -> Result<Vec<ProjectChats>, String> {
    let (provider_id, active_root) = state
        .sessions
        .with_session_mut(|session| {
            remember_workspace(&session.root);
            (session.provider_id.clone(), session.root.clone())
        })
        .map_err(map_session_err)?;

    let mut roots = load_known_workspaces();
    if !roots.iter().any(|p| p == &active_root) {
        roots.insert(0, active_root.clone());
    }

    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let threads = match open_store(&root) {
            Ok(store) => store.list_for_provider(&provider_id).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let active = root == active_root;
        out.push(ProjectChats {
            name: project_display_name(&root),
            path: display_path(&root),
            active,
            threads,
        });
    }

    // Active project first; then by newest thread activity.
    out.sort_by(|a, b| match (a.active, b.active) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => {
            let a_t = a.threads.first().map(|t| t.updated_at).unwrap_or(0);
            let b_t = b.threads.first().map(|t| t.updated_at).unwrap_or(0);
            b_t.cmp(&a_t).then_with(|| a.name.cmp(&b.name))
        }
    });
    Ok(out)
}

/// Switch project (and optional thread) while keeping the current provider.
#[tauri::command]
fn open_project_chat(
    state: State<'_, AppState>,
    root: String,
    thread_id: Option<String>,
    new_thread: Option<bool>,
) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;

    let provider_id = state
        .sessions
        .session_info_snapshot(|s| s.provider_id.clone())
        .map_err(map_session_err)?
        .or_else(last_provider)
        .ok_or_else(|| desktop_err("invalid", "no provider — connect one first"))?;

    let root = set_workspace_root(&state, PathBuf::from(root.trim()))?;

    let had_session = state
        .sessions
        .session_info_snapshot(|_| ())
        .map_err(map_session_err)?
        .is_some();
    if had_session {
        state.sessions.end_session().map_err(map_session_err)?;
        state.approvals.clear();
    }

    if new_thread.unwrap_or(false) {
        let store = open_store(&root)?;
        let thread = store
            .create_for_provider(&provider_id)
            .map_err(|e| e.to_string())?;
        persist_provider_thread(&root, &provider_id, &thread.id)?;
    } else if let Some(tid) = thread_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // Pin sticky thread before start_session resolves it.
        let store = open_store(&root)?;
        let _ = store
            .load_for_provider(tid, &provider_id)
            .map_err(|e| e.to_string())?;
        persist_provider_thread(&root, &provider_id, tid)?;
    }

    start_session(state, provider_id, None, None)
}

#[tauri::command]
fn load_thread(state: State<'_, AppState>, id: String) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state.approvals.clear();

    state
        .sessions
        .with_session_mut(|session| -> Result<SessionInfo, String> {
            let store = open_store(&session.root)?;
            let loaded = store
                .load_for_provider(&id, &session.provider_id)
                .map_err(|e| e.to_string())?;
            session.agent.clear_messages();
            session.agent.messages = loaded.thread.agent_messages.clone();
            session.thread_id = loaded.thread.id.clone();
            session.thread = loaded.thread;
            persist_provider_thread(&session.root, &session.provider_id, &session.thread_id)?;
            Ok(session_info_from(session, loaded.warning))
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[tauri::command]
fn new_thread(state: State<'_, AppState>) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state.approvals.clear();

    state
        .sessions
        .with_session_mut(|session| -> Result<SessionInfo, String> {
            let store = open_store(&session.root)?;
            let thread = store
                .create_for_provider(&session.provider_id)
                .map_err(|e| e.to_string())?;
            session.agent.clear_messages();
            session.thread_id = thread.id.clone();
            session.thread = thread;
            persist_provider_thread(&session.root, &session.provider_id, &session.thread_id)?;
            Ok(session_info_from(session, None))
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

/// Delete a saved chat. If it is the active thread, switches the session to a
/// fresh empty thread for the same provider. `project_path` deletes from another
/// known project without switching the open workspace.
#[tauri::command]
fn delete_thread(
    state: State<'_, AppState>,
    id: String,
    project_path: Option<String>,
) -> Result<SessionInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state.approvals.clear();

    state
        .sessions
        .with_session_mut(|session| -> Result<SessionInfo, String> {
            let target_root = match project_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(raw) => canonicalize_dir(PathBuf::from(raw))?,
                None => session.root.clone(),
            };
            let store = open_store(&target_root)?;
            // Ownership check — never delete another provider's thread.
            let _ = store
                .load_for_provider(&id, &session.provider_id)
                .map_err(|e| e.to_string())?;
            store.delete(&id).map_err(|e| e.to_string())?;

            // Compare via display paths — `session.root` may be `\\?\…` while the
            // sidebar sends a stripped path that still canonicalizes differently.
            let same_project = display_path(&session.root) == display_path(&target_root)
                || session.root == target_root;
            if same_project && session.thread_id == id {
                let thread = store
                    .create_for_provider(&session.provider_id)
                    .map_err(|e| e.to_string())?;
                session.agent.clear_messages();
                session.thread_id = thread.id.clone();
                session.thread = thread;
                persist_provider_thread(&session.root, &session.provider_id, &session.thread_id)?;
            }
            Ok(session_info_from(session, None))
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    let attachments = attachments.unwrap_or_default();
    if text.is_empty() && !has_usable_attachment(&attachments) {
        return Err(desktop_err("invalid", "empty message"));
    }

    // The transcript keeps what was typed; only the model sees an expansion.
    let display_text = format_display_message(&text, &attachments);
    if build_user_content(&text, &attachments).is_empty() {
        return Err(desktop_err("invalid", "empty message"));
    }
    let multimodal = has_images(&attachments);

    let (mut session, turn) = state.sessions.begin_turn().map_err(map_session_err)?;
    state.approvals.begin_turn(&turn.turn_id);

    // Slash commands resolve against the session's skills, so this has to come
    // after the session is in hand. An unknown command expands to itself.
    let (prompt, command) = match session.skills.read() {
        Ok(skills) => {
            let expansion = zest_core::expand_command(&text, &skills);
            (expansion.prompt, expansion.command)
        }
        // A poisoned lock must not lose the message — send it verbatim.
        Err(_) => (text.clone(), None),
    };
    let user_blocks = build_user_content(&prompt, &attachments);
    let worker = match ensure_persist(&state, &session.root) {
        Ok(w) => w,
        Err(e) => {
            state.approvals.clear();
            let _ = state.sessions.finish_turn(&turn, session);
            return Err(desktop_err("persistence", e));
        }
    };

    let session_id = turn.session_id.clone();
    let thread_id = turn.thread_id.clone();
    let turn_id = turn.turn_id.clone();
    let user_message_id = new_id("user");
    let assistant_message_id = new_id("assistant");

    let user_event = ChatEvent::User {
        session_id: session_id.clone(),
        thread_id: thread_id.clone(),
        turn_id: turn_id.clone(),
        message_id: user_message_id,
        text: display_text,
    };
    apply_event_to_thread(&mut session.thread, &user_event);
    let assistant_start = ChatEvent::AssistantStart {
        session_id: session_id.clone(),
        thread_id: thread_id.clone(),
        turn_id: turn_id.clone(),
        message_id: assistant_message_id.clone(),
        command: command.clone(),
    };
    apply_event_to_thread(&mut session.thread, &assistant_start);
    if let Err(e) = worker
        .save_and_wait(session.thread.clone(), PersistPriority::Immediate)
        .await
    {
        let _ = app.emit(
            "chat-event",
            ChatEvent::Warning {
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: Some(turn_id.clone()),
                message: format!("history not saved: {e}"),
            },
        );
    }
    let _ = app.emit("chat-event", &user_event);
    let _ = app.emit("chat-event", &assistant_start);

    let live_thread = Arc::new(Mutex::new(std::mem::take(&mut session.thread)));
    let cancel = turn.cancel.clone();

    let result = {
        let app = app.clone();
        let assistant_message_id = assistant_message_id.clone();
        let session_id = session_id.clone();
        let thread_id = thread_id.clone();
        let turn_id = turn_id.clone();
        let live_thread = live_thread.clone();
        let worker = worker.clone();
        let mut on_event = move |ev: StreamEvent<'_>| {
            let event = match ev {
                StreamEvent::Text(t) => ChatEvent::TextDelta {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    text: t.to_string(),
                },
                StreamEvent::Thinking(t) => ChatEvent::ThinkingDelta {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    text: t.to_string(),
                },
                StreamEvent::ToolCallStart { name, id } => ChatEvent::ToolCallStart {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    name: name.to_string(),
                    id: id.to_string(),
                },
                StreamEvent::ToolCallResult {
                    name,
                    id,
                    summary,
                    is_error,
                    metadata,
                } => ChatEvent::ToolCallResult {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    name: name.to_string(),
                    id: id.to_string(),
                    summary: summary.to_string(),
                    is_error,
                    metadata: metadata.map(ToolMetaView::from),
                },
                StreamEvent::ApprovalNeeded {
                    approval_id,
                    tool_name,
                    tool_call_id,
                    risk,
                    path,
                    summary,
                    diff,
                } => ChatEvent::ApprovalNeeded {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    approval_id,
                    tool_name,
                    tool_call_id,
                    risk: tool_risk_wire(risk).into(),
                    path,
                    summary,
                    diff,
                },
            };

            if let Ok(mut thread) = live_thread.lock() {
                let priority = event_priority(&event);
                apply_event_to_thread(&mut thread, &event);
                // Schedule the checkpoint, then clone for the worker — Immediate
                // for tools/approvals/terminal; Delta coalesces text/thinking.
                let snapshot = thread.clone();
                if let Err(e) = worker.enqueue(snapshot, priority) {
                    let _ = app.emit(
                        "chat-event",
                        ChatEvent::Warning {
                            session_id: session_id.clone(),
                            thread_id: thread_id.clone(),
                            turn_id: Some(turn_id.clone()),
                            message: format!("history not saved: {e}"),
                        },
                    );
                }
            }
            let _ = app.emit("chat-event", &event);
        };

        if multimodal {
            session
                .agent
                .send_blocks_cancellable(user_blocks, &mut on_event, Some(&cancel))
                .await
        } else {
            // Text-only path keeps prior wire shape (single text block).
            let agent_text = user_blocks
                .iter()
                .find_map(|b| {
                    (b.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .then(|| {
                            b.get("text")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string())
                        })
                        .flatten()
                })
                .unwrap_or_default();
            session
                .agent
                .send_cancellable(&agent_text, &mut on_event, Some(&cancel))
                .await
        }
    };

    session.thread = match Arc::try_unwrap(live_thread) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    };

    // Wire history is already transactional inside Agent; only sync committed
    // messages after a successful terminal turn.
    let final_event = match &result {
        Ok(()) => {
            // Persist redacted wire history; live agent memory keeps secrets.
            session
                .thread
                .set_agent_messages(session.agent.messages_for_persist());
            ChatEvent::Done {
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
                message_id: assistant_message_id.clone(),
            }
        }
        Err(HarnessError::Cancelled) => {
            // Keep UI transcript; leave agent.messages at the last committed turn.
            // Terminalize any pending approval/running tool cards.
            let _ = session.thread.terminalize_interrupted();
            session
                .thread
                .set_agent_messages(session.agent.messages_for_persist());
            ChatEvent::Cancelled {
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
                message_id: assistant_message_id.clone(),
            }
        }
        Err(e) => {
            let _ = session.thread.terminalize_interrupted();
            session
                .thread
                .set_agent_messages(session.agent.messages_for_persist());
            ChatEvent::Error {
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
                message_id: assistant_message_id.clone(),
                message: format_turn_error(e),
                // Only for failures that signing in again actually fixes.
                reconnect_provider: e.is_auth_problem().then(|| session.provider_id.clone()),
            }
        }
    };
    apply_event_to_thread(&mut session.thread, &final_event);
    if let Err(e) = worker
        .save_and_wait(session.thread.clone(), PersistPriority::Immediate)
        .await
    {
        let _ = app.emit(
            "chat-event",
            ChatEvent::Warning {
                session_id: session_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: Some(turn_id.clone()),
                message: format!("history not saved: {e}"),
            },
        );
    } else if let Err(e) = worker.flush().await {
        let _ = app.emit(
            "chat-event",
            ChatEvent::Warning {
                session_id,
                thread_id,
                turn_id: Some(turn_id),
                message: format!("history not saved: {e}"),
            },
        );
    }
    let _ = app.emit("chat-event", &final_event);

    state.approvals.clear();
    let _ = state.sessions.finish_turn(&turn, session);

    // Error/cancel already emitted as chat-events; keep invoke Ok to avoid
    // double toasts on the frontend catch path.
    Ok(())
}

#[tauri::command]
fn cancel_turn(state: State<'_, AppState>) -> Result<(), String> {
    // Cancel token first so in-flight select! races abort before waiters clear.
    let cancelled = state.sessions.cancel_turn().map_err(map_session_err)?;
    if !cancelled {
        return Err(desktop_err("no_turn", "no turn in progress"));
    }
    state.approvals.clear();
    Ok(())
}

#[tauri::command]
fn resolve_approval(
    state: State<'_, AppState>,
    approval_id: String,
    decision: String,
) -> Result<(), String> {
    // Unknown strings deny rather than default to allow: a UI/backend version
    // skew must fail closed.
    let decision = match decision.as_str() {
        "once" => ApprovalDecision::AllowOnce,
        "session" => ApprovalDecision::AllowSession,
        "deny" => ApprovalDecision::Deny,
        other => return Err(format!("unknown approval decision `{other}`")),
    };
    state.approvals.resolve(&approval_id, decision)
}

/// Switch the permission mode for the live session.
///
/// Grants made under the previous mode are dropped by `ApprovalPolicy`. The
/// policy outlives any one project, so switching folders keeps the mode.
#[tauri::command]
fn set_approval_mode(state: State<'_, AppState>, mode: String) -> Result<String, String> {
    let mode = ApprovalMode::parse(&mode).ok_or_else(|| format!("unknown mode `{mode}`"))?;
    state
        .policy
        .lock()
        .map_err(|_| "approval policy lock poisoned".to_string())?
        .set_mode(mode);
    Ok(mode.as_str().to_string())
}

#[tauri::command]
fn approval_mode(state: State<'_, AppState>) -> Result<String, String> {
    let mode = state
        .policy
        .lock()
        .map_err(|_| "approval policy lock poisoned".to_string())?
        .mode();
    Ok(mode.as_str().to_string())
}

#[tauri::command]
fn end_session(state: State<'_, AppState>) -> Result<(), String> {
    // end_session cancels the turn token; clear waiters after.
    state.sessions.end_session().map_err(map_session_err)?;
    state.approvals.clear();
    Ok(())
}

#[tauri::command]
fn session_info(state: State<'_, AppState>) -> Option<SessionInfo> {
    state
        .sessions
        .session_info_snapshot(|s| session_info_from(s, None))
        .ok()
        .flatten()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemPromptInfo {
    base: String,
    custom: String,
    /// Truncated composed preview for the Settings UI.
    composed_preview: String,
    custom_path: String,
}

const COMPOSED_PREVIEW_MAX: usize = 2400;

#[tauri::command]
fn get_system_prompt(state: State<'_, AppState>) -> Result<SystemPromptInfo, String> {
    state
        .sessions
        .with_session_mut(|session| system_prompt_info(session))
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[tauri::command]
fn set_system_prompt(
    state: State<'_, AppState>,
    custom: String,
) -> Result<SystemPromptInfo, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    state
        .sessions
        .with_session_mut(|session| {
            save_custom_system(&session.root, &custom).map_err(|e| e.to_string())?;
            let skills = SkillSet::discover(&session.root);
            {
                let mut guard = session
                    .skills
                    .write()
                    .map_err(|_| "skill registry lock poisoned".to_string())?;
                *guard = skills;
            }
            // Must mirror RuntimeBuilder::build exactly — docs and environment
            // included — or saving Settings would quietly strip them.
            let composed = {
                let guard = session
                    .skills
                    .read()
                    .map_err(|_| "skill registry lock poisoned".to_string())?;
                let docs = load_project_docs(&session.root);
                let body = compose_system_with_docs(&session.base_system, &custom, &docs, &guard);
                format!("{body}\n\n{}", env_context(&session.root))
            };
            session.agent.system = Some(composed);
            system_prompt_info(session)
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "RoutingRuleView.ts", rename_all = "camelCase")
)]
pub struct RoutingRuleView {
    pub kind: String,
    pub provider: String,
    /// Empty means "the provider's own default model".
    #[serde(default)]
    pub model: String,
    /// Empty means `high`.
    #[serde(default)]
    pub effort: String,
    /// Extra framing for this worker. Empty means the generic worker contract.
    #[serde(default)]
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "RoutingView.ts", rename_all = "camelCase")
)]
pub struct RoutingView {
    pub delegation: bool,
    pub rules: Vec<RoutingRuleView>,
    /// Every configured provider with its real catalogue, so the editor can
    /// offer only pairs that exist rather than free text.
    pub providers: Vec<ProviderModelsView>,
    /// Where a save will be written.
    pub config_path: String,
    /// `[routing].default` provider (config). UI same-account warnings use the
    /// open session's provider instead — the picker can start a chat on Claude
    /// even when this default is still Codex.
    pub default_provider: String,
    /// True when the active project has its own zest.toml, which **replaces**
    /// the user one — editing here would then have no effect on this project.
    pub project_scoped: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ProviderModelsView.ts", rename_all = "camelCase")
)]
pub struct ProviderModelsView {
    pub id: String,
    pub default_model: String,
    pub models: Vec<String>,
    /// Efforts accepted by the default model, for the effort dropdown.
    pub efforts: Vec<String>,
}

fn routing_view(state: &State<'_, AppState>) -> Result<RoutingView, String> {
    let root = resolve_workspace_root(state)?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;
    let user_path = zest_core::user_config_path()
        .map(|p| display_path(p.as_path()))
        .unwrap_or_else(|| "~/.zest/zest.toml".to_string());

    let providers = config
        .providers
        .iter()
        .map(|(id, entry)| {
            let descriptor = descriptor_from_config(id, entry);
            let efforts = descriptor
                .models
                .iter()
                .find(|m| m.id == descriptor.default_model)
                .map(|m| m.efforts.clone())
                .unwrap_or_default();
            ProviderModelsView {
                id: id.clone(),
                default_model: descriptor.default_model,
                models: descriptor.models.into_iter().map(|m| m.id).collect(),
                efforts,
            }
        })
        .collect();

    Ok(RoutingView {
        default_provider: config
            .default_target()
            .map(|t| t.provider)
            .unwrap_or_default(),
        delegation: config.routing.delegation,
        rules: config
            .routing
            .rules
            .iter()
            .map(|r| RoutingRuleView {
                kind: r.kind.clone(),
                provider: r.provider.clone(),
                model: r.model.clone().unwrap_or_default(),
                effort: r.effort.clone().unwrap_or_default(),
                prompt: r.prompt.clone().unwrap_or_default(),
            })
            .collect(),
        providers,
        config_path: user_path,
        project_scoped: root.join(zest_core::config::CONFIG_FILE).is_file(),
    })
}

#[tauri::command]
fn routing_config(state: State<'_, AppState>) -> Result<RoutingView, String> {
    routing_view(&state)
}

/// Rules to start from, derived from the providers actually configured.
///
/// Returned rather than saved: a preset the user has not looked at should not
/// silently become their routing policy.
#[tauri::command]
fn suggested_routing(state: State<'_, AppState>) -> Result<Vec<RoutingRuleView>, String> {
    let root = resolve_workspace_root(&state)?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;
    Ok(zest_core::routing_edit::suggest_rules(&config)
        .into_iter()
        .map(|r| RoutingRuleView {
            kind: r.kind,
            provider: r.provider,
            model: r.model.unwrap_or_default(),
            effort: r.effort.unwrap_or_default(),
            prompt: r.prompt.unwrap_or_default(),
        })
        .collect())
}

/// Persist delegation + rules to the **user** config.
///
/// Validated against the live provider catalogues first: an unroutable rule
/// would otherwise fail much later, mid-delegation, on a turn already paid for.
#[tauri::command]
fn set_routing_config(
    state: State<'_, AppState>,
    delegation: bool,
    rules: Vec<RoutingRuleView>,
) -> Result<RoutingView, String> {
    let root = resolve_workspace_root(&state)?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;

    let inputs: Vec<RuleInput> = rules
        .into_iter()
        .map(|r| RuleInput {
            kind: r.kind,
            provider: r.provider,
            model: Some(r.model).filter(|m| !m.trim().is_empty()),
            effort: Some(r.effort).filter(|e| !e.trim().is_empty()),
            prompt: Some(r.prompt).filter(|p| !p.trim().is_empty()),
        })
        .collect();

    validate_rules(&config, &inputs)?;

    let path = zest_core::user_config_path()
        .ok_or_else(|| "cannot locate the user config directory".to_string())?;
    let updated = routing_document(&path, delegation, &inputs)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    zest_core::atomic_write(&path, updated.as_bytes()).map_err(|e| e.to_string())?;

    routing_view(&state)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "CommandView.ts", rename_all = "camelCase")
)]
pub struct CommandView {
    pub name: String,
    pub description: String,
}

/// Slash commands available here — one per discovered skill.
///
/// Workspace-based for the same reason as [`list_skills`]: the composer must be
/// able to list commands while a turn is still streaming.
#[tauri::command]
fn list_commands(state: State<'_, AppState>) -> Result<Vec<CommandView>, String> {
    let root = resolve_workspace_root(&state)?;
    Ok(SkillSet::discover(&root)
        .command_names()
        .into_iter()
        .map(|(name, description)| CommandView { name, description })
        .collect())
}

#[tauri::command]
/// Discovered from the workspace rather than the live session.
///
/// `begin_turn` *takes* the session out of the controller, so anything that
/// reaches through it is unreadable while a turn runs — which made opening
/// Settings mid-turn fail with "a turn is already in progress". Skills come
/// from disk, and disk is readable whenever.
fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillSummary>, String> {
    let root = resolve_workspace_root(&state)?;
    Ok(SkillSet::discover(&root).summaries())
}

fn system_prompt_info(session: &Session) -> Result<SystemPromptInfo, String> {
    let custom = load_custom_system(&session.root)?;
    let composed = session
        .agent
        .system
        .clone()
        .unwrap_or_else(|| session.base_system.clone());
    let composed_preview = truncate_chars(&composed, COMPOSED_PREVIEW_MAX);
    Ok(SystemPromptInfo {
        base: session.base_system.clone(),
        custom,
        composed_preview,
        custom_path: display_path(&session.root.join(".zest").join("system.md")),
    })
}

fn zest_config_dir() -> Result<std::path::PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "no config directory".to_string())?
        .join("zest");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn persist_choice(id: &str) -> Result<(), String> {
    let path = zest_config_dir()?.join("last-provider");
    std::fs::write(&path, id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn last_provider() -> Option<String> {
    let path = dirs::config_dir()?.join("zest").join("last-provider");
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[tauri::command]
fn get_workspace_folder(state: State<'_, AppState>) -> Result<String, String> {
    Ok(display_path(&resolve_workspace_root(&state)?))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePickResult {
    path: String,
    /// True when an open session was closed so the UI must start a new one.
    session_ended: bool,
}

/// Native folder picker. Stores preference for the next `start_session`.
/// Returns `null` when the user cancels. Ends an idle open session so tools
/// stay scoped to the new root.
#[tauri::command]
fn pick_workspace_folder(
    state: State<'_, AppState>,
) -> Result<Option<WorkspacePickResult>, String> {
    state.sessions.require_idle().map_err(map_session_err)?;
    let mut dialog = rfd::FileDialog::new().set_title("Open project folder");
    if let Ok(current) = resolve_workspace_root(&state) {
        dialog = dialog.set_directory(current);
    }
    let Some(folder) = dialog.pick_folder() else {
        return Ok(None);
    };
    let root = set_workspace_root(&state, folder)?;
    let had_session = state
        .sessions
        .session_info_snapshot(|_| ())
        .ok()
        .flatten()
        .is_some();
    let session_ended = if had_session {
        state.sessions.end_session().map_err(map_session_err)?;
        state.approvals.clear();
        true
    } else {
        false
    };
    Ok(Some(WorkspacePickResult {
        path: display_path(&root),
        session_ended,
    }))
}

/// Native multi-file picker. PDFs are extracted via pdf-inspector.
#[tauri::command]
fn pick_files(state: State<'_, AppState>) -> Result<Vec<PreparedAttachment>, String> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Attach files")
        .add_filter(
            "Documents",
            &[
                "pdf", "md", "txt", "rs", "ts", "tsx", "js", "jsx", "json", "toml", "yaml", "yml",
                "py", "go", "java", "c", "h", "cpp", "cs", "html", "css", "svg", "csv", "log",
            ],
        )
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
        .add_filter("PDF", &["pdf"])
        .add_filter("All files", &["*"]);
    if let Ok(current) = resolve_workspace_root(&state) {
        dialog = dialog.set_directory(current);
    }
    let Some(paths) = dialog.pick_files() else {
        return Ok(Vec::new());
    };
    Ok(prepare_paths(&paths))
}

/// Paste / drop path: raw image bytes from the webview (base64).
#[tauri::command]
fn prepare_pasted_image(
    data_base64: String,
    media_type: String,
    name: Option<String>,
) -> Result<PreparedAttachment, String> {
    let raw = data_base64
        .split(',')
        .next_back()
        .unwrap_or(data_base64.as_str());
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| format!("invalid image data: {e}"))?;
    let name = name.unwrap_or_else(|| {
        let ext = match media_type.to_ascii_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "png",
        };
        format!("paste-{}.{}", zest_core::new_id("img"), ext)
    });
    Ok(prepare_image_bytes(&bytes, &media_type, &name))
}

#[tauri::command]
fn git_branch(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let root = resolve_workspace_root(&state)?;
    Ok(read_git_branch(&root))
}

fn read_git_branch(root: &Path) -> Option<String> {
    let head = root.join(".git").join("HEAD");
    let contents = std::fs::read_to_string(head).ok()?;
    let line = contents.lines().next()?.trim();
    if let Some(branch) = line.strip_prefix("ref: refs/heads/") {
        return Some(branch.to_string());
    }
    // Detached HEAD — short hash.
    if line.len() >= 7 && line.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!("{}…", &line[..7]));
    }
    None
}

#[tauri::command]
fn context_usage(state: State<'_, AppState>) -> Result<ContextUsageView, String> {
    state
        .sessions
        .with_session_mut(|session| estimate_context(&session.agent))
        .map_err(map_session_err)
}

/// Wire view for the UI. Avatar bytes live in `avatar.jpg`, not in JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    display_name: String,
    /// data:image/...;base64,... for display / optimized upload; empty clears file.
    #[serde(default)]
    avatar_data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserProfileDisk {
    display_name: String,
}

/// Soft cap for optimized avatar payloads (JPEG ~128px is typically far smaller).
const MAX_AVATAR_DATA_URL_CHARS: usize = 80_000;
const MAX_AVATAR_BYTES: usize = 48_000;

fn user_profile_path() -> Result<PathBuf, String> {
    Ok(zest_config_dir()?.join("user-profile.json"))
}

fn user_avatar_path() -> Result<PathBuf, String> {
    Ok(zest_config_dir()?.join("avatar.jpg"))
}

fn load_avatar_data_url() -> Result<String, String> {
    let path = user_avatar_path()?;
    match std::fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(format!("data:image/jpeg;base64,{b64}"))
        }
        Ok(_) => Ok(String::new()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_avatar_from_data_url(data_url: &str) -> Result<(), String> {
    let path = user_avatar_path()?;
    let trimmed = data_url.trim();
    if trimmed.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if trimmed.chars().count() > MAX_AVATAR_DATA_URL_CHARS {
        return Err("avatar too large after optimize (pick a smaller image)".into());
    }
    let b64 = trimmed
        .split(',')
        .next_back()
        .ok_or_else(|| "invalid avatar data URL".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("invalid avatar encoding: {e}"))?;
    if bytes.is_empty() {
        return Err("empty avatar".into());
    }
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err("avatar too large after optimize (max ~48KB)".into());
    }
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err("avatar must be JPEG (optimize in the UI before save)".into());
    }
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_user_profile() -> Result<UserProfile, String> {
    let path = user_profile_path()?;
    let display_name = match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let disk: UserProfileDisk = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            disk.display_name
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    Ok(UserProfile {
        display_name,
        avatar_data_url: load_avatar_data_url()?,
    })
}

#[tauri::command]
fn set_user_profile(profile: UserProfile) -> Result<UserProfile, String> {
    write_avatar_from_data_url(&profile.avatar_data_url)?;
    let path = user_profile_path()?;
    let disk = UserProfileDisk {
        display_name: profile.display_name.trim().to_string(),
    };
    let raw = serde_json::to_string_pretty(&disk).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    Ok(UserProfile {
        display_name: disk.display_name,
        avatar_data_url: load_avatar_data_url()?,
    })
}

fn normalize_effort(effort: &str) -> String {
    zest_core::normalize_effort(effort)
}

/// User-facing turn errors. Connection refused to the local gateway is the
/// usual alpha failure mode and should not look like a missing system prompt.
fn format_turn_error(err: &HarnessError) -> String {
    match err {
        HarnessError::Http(http) if http.is_connect() || http.is_timeout() => {
            format!(
                "Can't reach the model gateway (usually CLIProxyAPI on http://127.0.0.1:8317). \
Start it with scripts/start-gateway.ps1, then try again.\n\n{err}"
            )
        }
        // Lead with what to do. The raw envelope still follows, because the
        // detail in it ("cooldown", a provider id) is what makes an unusual
        // failure diagnosable — but it should not be the first thing read.
        _ if err.is_auth_problem() => {
            format!(
                "That account needs signing in again — the gateway is holding \
credentials it can't currently use. Reconnect below, then resend.\n\n{}",
                api_error_message(err).unwrap_or_else(|| err.to_string())
            )
        }
        _ => err.to_string(),
    }
}

/// Pull `error.message` out of an API error envelope.
///
/// Returns `None` when the body is not the shape we expect, so the caller falls
/// back to the raw text rather than swallowing an error it failed to parse.
fn api_error_message(err: &HarnessError) -> Option<String> {
    let HarnessError::Api { status, body } = err else {
        return None;
    };
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = parsed.get("error")?.get("message")?.as_str()?;
    Some(format!("{status}: {message}"))
}

/// Wire label for approval / chat-event payloads (snake_case string).
fn tool_risk_wire(risk: ToolRisk) -> &'static str {
    match risk {
        ToolRisk::Read => "read",
        ToolRisk::Sensitive => "sensitive",
        ToolRisk::Write => "write",
        ToolRisk::Exec => "exec",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    zest_core::load_env();

    tauri::Builder::default()
        .manage(AppState {
            sessions: SessionController::new(),
            approvals: Arc::new(ApprovalHub::new()),
            persist: Mutex::new(None),
            workspace_root: Mutex::new(load_persisted_workspace()),
            policy: Arc::new(Mutex::new(ApprovalPolicy::new(DESKTOP_DEFAULT_MODE))),
        })
        .invoke_handler(tauri::generate_handler![
            list_providers,
            refresh_providers,
            usage_snapshot,
            last_provider,
            start_login,
            verify_provider,
            start_session,
            update_session_options,
            reset_session_options,
            list_threads,
            list_chat_projects,
            open_project_chat,
            load_thread,
            new_thread,
            delete_thread,
            send_message,
            cancel_turn,
            resolve_approval,
            set_approval_mode,
            approval_mode,
            end_session,
            session_info,
            get_system_prompt,
            set_system_prompt,
            list_skills,
            list_commands,
            routing_config,
            suggested_routing,
            set_routing_config,
            get_workspace_folder,
            pick_workspace_folder,
            pick_files,
            prepare_pasted_image,
            git_branch,
            context_usage,
            get_user_profile,
            set_user_profile
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zest desktop");
}

#[cfg(all(test, feature = "export-bindings"))]
mod export_bindings {
    use super::*;

    #[test]
    fn export_bindings() {
        ChatEvent::export_all().expect("export ChatEvent bindings");
        SessionInfo::export_all().expect("export SessionInfo bindings");
        ProviderView::export_all().expect("export ProviderView bindings");
        ModelCapability::export_all().expect("export ModelCapability bindings");
        ToolMetaView::export_all().expect("export ToolMetaView bindings");
    }
}

#[cfg(test)]
mod characterization {
    use super::*;
    use zest_core::ToolRisk;

    #[test]
    fn normalize_effort_aliases_and_default() {
        assert_eq!(normalize_effort("HIGH"), "high");
        assert_eq!(normalize_effort(" med "), "medium");
        assert_eq!(normalize_effort("extra_high"), "xhigh");
        assert_eq!(normalize_effort("nonsense"), "high");
        assert_eq!(normalize_effort("max"), "max");
    }

    #[test]
    fn tool_risk_wire_labels() {
        assert_eq!(tool_risk_wire(ToolRisk::Read), "read");
        assert_eq!(tool_risk_wire(ToolRisk::Sensitive), "sensitive");
        assert_eq!(tool_risk_wire(ToolRisk::Write), "write");
        assert_eq!(tool_risk_wire(ToolRisk::Exec), "exec");
    }

    #[test]
    fn chat_event_requires_identity_fields() {
        let event = ChatEvent::ToolCallResult {
            session_id: "s1".into(),
            thread_id: "th1".into(),
            turn_id: "turn-1".into(),
            message_id: "a1".into(),
            name: "write_file".into(),
            id: "t1".into(),
            summary: "wrote f.txt".into(),
            is_error: false,
            metadata: None,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["kind"], "tool_call_result");
        assert_eq!(v["session_id"], "s1");
        assert_eq!(v["thread_id"], "th1");
        assert_eq!(v["turn_id"], "turn-1");
        assert_eq!(v["isError"], false);
    }

    #[test]
    fn apply_event_to_thread_covers_full_chat_sequence() {
        let mut thread = Thread::new();
        let sid = "s1";
        let tid = "th1";
        let turn = "turn-1";
        apply_event_to_thread(
            &mut thread,
            &ChatEvent::User {
                session_id: sid.into(),
                thread_id: tid.into(),
                turn_id: turn.into(),
                message_id: "u1".into(),
                text: "please edit".into(),
            },
        );
        apply_event_to_thread(
            &mut thread,
            &ChatEvent::AssistantStart {
                session_id: sid.into(),
                thread_id: tid.into(),
                turn_id: turn.into(),
                message_id: "a1".into(),
                command: None,
            },
        );
        apply_event_to_thread(
            &mut thread,
            &ChatEvent::TextDelta {
                session_id: sid.into(),
                thread_id: tid.into(),
                turn_id: turn.into(),
                message_id: "a1".into(),
                text: "ok".into(),
            },
        );
        apply_event_to_thread(
            &mut thread,
            &ChatEvent::Done {
                session_id: sid.into(),
                thread_id: tid.into(),
                turn_id: turn.into(),
                message_id: "a1".into(),
            },
        );
        assert_eq!(thread.messages.len(), 2);
    }

    fn slot(id: &'static str, status: AuthStatus) -> ProviderSlot {
        ProviderSlot {
            id,
            label: id,
            method: "test sign-in",
            status,
        }
    }

    fn config_with(ids: &[&str]) -> Config {
        let mut toml = String::new();
        for id in ids {
            toml.push_str(&format!(
                "[providers.{id}]\nkind = \"gateway\"\nbase_url = \"http://127.0.0.1:8317\"\nmodel = \"m\"\n\n"
            ));
        }
        Config::parse(&toml).expect("valid test config")
    }

    /// The reported failure: the picker offered Claude as ready because a CLI
    /// session existed, then Continue died with "not configured".
    #[test]
    fn a_signed_in_provider_with_no_config_is_not_selectable() {
        let config = config_with(&["codex"]);
        let view = provider_view_from_slot(
            &slot("claude", AuthStatus::Ready { account: None }),
            &config,
        );

        assert!(!view.selectable, "must not be offered as usable");
        assert!(!view.configured);
        assert_eq!(view.status_kind, "unconfigured");
        assert_eq!(view.status_label, "Not configured");
        // The row has to say what to do, since the green "Signed in" it used to
        // show sent the user looking at their Claude login instead.
        assert!(view.detail.contains("Signed in, but"), "{}", view.detail);
        assert!(view.detail.contains("zest.toml"), "{}", view.detail);
    }

    #[test]
    fn a_signed_in_configured_provider_stays_selectable() {
        let config = config_with(&["codex", "claude"]);
        let view = provider_view_from_slot(
            &slot("claude", AuthStatus::Ready { account: None }),
            &config,
        );
        assert!(view.selectable);
        assert!(view.configured);
        assert_eq!(view.status_kind, "ready");
        assert_eq!(view.status_label, "Signed in");
    }

    #[test]
    fn a_configured_provider_without_a_sign_in_is_still_not_selectable() {
        // Both halves are required; config alone cannot serve a turn either.
        let config = config_with(&["claude"]);
        let view = provider_view_from_slot(
            &slot(
                "claude",
                AuthStatus::NotLoggedIn {
                    fix: "claude login".into(),
                },
            ),
            &config,
        );
        assert!(!view.selectable);
        assert!(view.configured);
    }

    #[tokio::test]
    async fn approval_hub_prepare_resolve_and_unknown_id() {
        let hub = ApprovalHub::new();
        hub.begin_turn("turn-1");
        hub.prepare("ap1");
        hub.resolve("ap1", ApprovalDecision::AllowOnce).unwrap();
        assert_eq!(hub.wait("ap1").await, ApprovalDecision::AllowOnce);

        assert!(hub.resolve("missing", ApprovalDecision::Deny).is_err());
        // Never prepared: no waiter, and the answer must be Deny, not a default
        // that happens to look permissive.
        assert_eq!(hub.wait("never-prepared").await, ApprovalDecision::Deny);

        hub.clear();
        assert!(hub.resolve("ap2", ApprovalDecision::AllowOnce).is_err());
    }

    #[tokio::test]
    async fn approval_hub_carries_a_session_grant_through() {
        // The three-way decision has to survive the channel — collapsing it to
        // a bool is what this widening exists to prevent.
        let hub = ApprovalHub::new();
        hub.begin_turn("turn-1");
        hub.prepare("ap-session");
        hub.resolve("ap-session", ApprovalDecision::AllowSession)
            .unwrap();
        assert_eq!(hub.wait("ap-session").await, ApprovalDecision::AllowSession);
    }

    #[tokio::test]
    async fn clearing_the_hub_denies_pending_waiters() {
        let hub = ApprovalHub::new();
        hub.begin_turn("turn-1");
        hub.prepare("ap-pending");
        hub.clear();
        assert_eq!(hub.wait("ap-pending").await, ApprovalDecision::Deny);
    }
}
