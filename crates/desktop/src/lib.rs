//! Desktop front-end: provider picker + chat session.
//!
//! Connect is a native shell over vendor OAuth (no token exchange in Zest).
//! Chat drives the same `Agent` loop as the CLI, streaming events into the UI.
//! Thread projection is persisted under `<workspace>/.zest/threads/`.

mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;
#[cfg(feature = "export-bindings")]
use ts_rs::TS;
use zest_core::{
    can_start_login, compose_system, detect_all, load_custom_system, new_id, save_custom_system,
    start_login as core_start_login, truncate_chars, ApprovalDecision, ApprovalRequest, Approver,
    AuthStatus, Config, HarnessError, PersistPriority, PersistWorker, ProjectSessionState,
    ProviderSlot, RuntimeBuilder, SkillSet, SkillSummary, StoredMessage, StreamEvent, Thread,
    ThreadLoadError, ThreadStore, ThreadSummary, ToolRisk,
};

use session::{Session, SessionController, SessionError};

/// Providers shown in the launch picker. BYOK stays terminal-only for now.
const PICKER_IDS: &[&str] = &["codex", "claude", "antigravity"];

const SYSTEM: &str = "\
You are Zest, a coding agent running in a desktop app inside the user's project. You \
have project tools (list_dir, glob, grep, read_file, write_file) scoped to that \
project. Explore and read files before answering questions about them rather than \
inferring from names. write_file requires the user to Allow once before it runs. \
Keep responses focused and concise.";

/// Turn-scoped pending approval waiters (not persisted).
struct ApprovalHub {
    /// Active turn that may own waiters. Resolves outside this turn are rejected.
    active_turn: Mutex<Option<String>>,
    senders: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    receivers: Mutex<HashMap<String, oneshot::Receiver<bool>>>,
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

    async fn wait(&self, approval_id: &str) -> bool {
        let rx = {
            let mut receivers = match self.receivers.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            receivers.remove(approval_id)
        };
        match rx {
            Some(rx) => rx.await.unwrap_or(false),
            None => false,
        }
    }

    fn resolve(&self, approval_id: &str, allow: bool) -> Result<(), String> {
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
        let _ = tx.send(allow);
        Ok(())
    }

    /// Deny every waiter. Call after cancelling the turn token.
    fn clear(&self) {
        if let Ok(mut senders) = self.senders.lock() {
            for (_, tx) in senders.drain() {
                let _ = tx.send(false);
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
        if self.hub.wait(&request.approval_id).await {
            ApprovalDecision::AllowOnce
        } else {
            ApprovalDecision::Deny
        }
    }
}

struct AppState {
    sessions: SessionController,
    approvals: Arc<ApprovalHub>,
    persist: Mutex<Option<PersistWorker>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRow {
    id: String,
    label: String,
    method: String,
    status_kind: String,
    status_label: String,
    detail: String,
    selectable: bool,
    can_connect: bool,
}

impl From<&ProviderSlot> for ProviderRow {
    fn from(slot: &ProviderSlot) -> Self {
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

        Self {
            id: slot.id.to_string(),
            label: slot.label.to_string(),
            method: slot.method.to_string(),
            status_kind,
            status_label,
            detail,
            selectable: slot.status.selectable(),
            can_connect: can_start_login(slot.id),
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

#[tauri::command]
fn list_providers() -> Vec<ProviderRow> {
    detect_all()
        .iter()
        .filter(|s| PICKER_IDS.contains(&s.id))
        .map(ProviderRow::from)
        .collect()
}

#[tauri::command]
fn refresh_providers() -> Vec<ProviderRow> {
    list_providers()
}

#[tauri::command]
fn start_login(id: String) -> Result<LoginStarted, String> {
    let spawn = core_start_login(&id)?;
    Ok(LoginStarted {
        browser_title: spawn.browser_title.to_string(),
        browser_body: spawn.browser_body.to_string(),
    })
}

fn workspace_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    cwd.canonicalize().or(Ok(cwd))
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

fn session_info_from(session: &Session, warning: Option<String>) -> SessionInfo {
    SessionInfo {
        session_id: session.session_id.clone(),
        provider: session.provider_id.clone(),
        label: session.provider_label.clone(),
        model: session.model.clone(),
        effort: session.effort.clone(),
        root: session.root.display().to_string(),
        thread_id: session.thread_id.clone(),
        messages: session.thread.messages.clone(),
        warning,
    }
}

fn apply_event_to_thread(thread: &mut Thread, event: &ChatEvent) {
    match event {
        ChatEvent::User {
            message_id, text, ..
        } => thread.apply_user(message_id, text),
        ChatEvent::AssistantStart { message_id, .. } => {
            thread.apply_assistant_start(message_id);
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
            ..
        } => thread.apply_tool_result(message_id, id, name, summary, *is_error),
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
    let _ = dotenvy::dotenv();
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

    let root = workspace_root()?;
    let config = Config::find(&root).map_err(|e| e.to_string())?;

    let prefs = ProjectSessionState::load(&root, &id).get(&id);

    let model = model
        .filter(|m| !m.trim().is_empty())
        .or(prefs.model)
        .or_else(|| {
            config.default_target().and_then(|t| {
                if t.provider == id {
                    t.model.clone()
                } else {
                    None
                }
            })
        })
        .or_else(|| std::env::var("ZEST_MODEL").ok());

    let effort = effort
        .filter(|e| !e.trim().is_empty())
        .or(prefs.effort)
        .or_else(|| std::env::var("ZEST_EFFORT").ok())
        .unwrap_or_else(|| "high".to_string());
    let effort = normalize_effort(&effort);

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
        .with_effort(&effort)
        .with_system(SYSTEM)
        .with_approver(approver)
        .enable_delegate(true)
        .register_write_tools(true);
    if let Some(model) = model {
        builder = builder.with_model(model);
    }

    let runtime = builder.build().map_err(|e| e.to_string())?;
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

    let info = state
        .sessions
        .session_info_snapshot(|s| session_info_from(s, load_warning))
        .map_err(map_session_err)?
        .ok_or_else(|| map_session_err(SessionError::NoSession))?;
    Ok(info)
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

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(desktop_err("invalid", "empty message"));
    }

    let (mut session, turn) = state.sessions.begin_turn().map_err(map_session_err)?;
    state.approvals.begin_turn(&turn.turn_id);
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
        text: text.clone(),
    };
    apply_event_to_thread(&mut session.thread, &user_event);
    let assistant_start = ChatEvent::AssistantStart {
        session_id: session_id.clone(),
        thread_id: thread_id.clone(),
        turn_id: turn_id.clone(),
        message_id: assistant_message_id.clone(),
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
                } => ChatEvent::ToolCallResult {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: assistant_message_id.clone(),
                    name: name.to_string(),
                    id: id.to_string(),
                    summary: summary.to_string(),
                    is_error,
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

        session
            .agent
            .send_cancellable(&text, &mut on_event, Some(&cancel))
            .await
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
                message: e.to_string(),
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
    allow: bool,
) -> Result<(), String> {
    state.approvals.resolve(&approval_id, allow)
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
            let composed = {
                let guard = session
                    .skills
                    .read()
                    .map_err(|_| "skill registry lock poisoned".to_string())?;
                compose_system(&session.base_system, &custom, &guard)
            };
            session.agent.system = Some(composed);
            system_prompt_info(session)
        })
        .map_err(map_session_err)
        .and_then(|r| r)
}

#[tauri::command]
fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillSummary>, String> {
    state
        .sessions
        .with_session_mut(|session| {
            let guard = session
                .skills
                .read()
                .map_err(|_| "skill registry lock poisoned".to_string())?;
            Ok(guard.summaries())
        })
        .map_err(map_session_err)
        .and_then(|r| r)
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
        custom_path: session
            .root
            .join(".zest")
            .join("system.md")
            .display()
            .to_string(),
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

fn normalize_effort(effort: &str) -> String {
    zest_core::normalize_effort(effort)
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
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .manage(AppState {
            sessions: SessionController::new(),
            approvals: Arc::new(ApprovalHub::new()),
            persist: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_providers,
            refresh_providers,
            last_provider,
            start_login,
            start_session,
            update_session_options,
            reset_session_options,
            list_threads,
            load_thread,
            new_thread,
            send_message,
            cancel_turn,
            resolve_approval,
            end_session,
            session_info,
            get_system_prompt,
            set_system_prompt,
            list_skills
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

    #[tokio::test]
    async fn approval_hub_prepare_resolve_and_unknown_id() {
        let hub = ApprovalHub::new();
        hub.begin_turn("turn-1");
        hub.prepare("ap1");
        hub.resolve("ap1", true).unwrap();
        assert!(hub.wait("ap1").await);

        assert!(hub.resolve("missing", false).is_err());
        assert!(!hub.wait("never-prepared").await);

        hub.clear();
        assert!(hub.resolve("ap2", true).is_err());
    }
}
