//! Project-scoped chat thread projection.
//!
//! Threads live under `<workspace>/.zest/threads/<id>.json` so history follows the
//! repo you launched from. The projection is a durable UI transcript plus the
//! agent wire messages needed to restore model context on reopen.
//!
//! On-disk format is versioned ([`THREAD_FORMAT_VERSION`]) and binds provider /
//! wire-format metadata so reopen can migrate non-destructively.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::anthropic::types::Message;
use crate::error::{HarnessError, Result};
use crate::fsutil;

/// Current on-disk thread document version.
///
/// v2 adds optional typed [`ToolPart::metadata`] (delegation provenance).
pub const THREAD_FORMAT_VERSION: u32 = 2;

/// Anthropic Messages API content blocks (today's only wire format).
pub const WIRE_FORMAT_ANTHROPIC_MESSAGES: &str = "anthropic_messages";

static ID_SEQ: AtomicU64 = AtomicU64::new(0);

/// Stable id for messages / turns / threads.
pub fn new_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos:x}-{seq:x}")
}

/// Validated thread identifier safe for use as a single path segment.
///
/// Rejects separators, absolute paths, drive prefixes, and `.` / `..` segments
/// so store paths cannot escape `.zest/threads/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn parse(raw: impl AsRef<str>) -> std::result::Result<Self, String> {
        let s = raw.as_ref();
        validate_thread_id(s)?;
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_thread_id(s: &str) -> std::result::Result<(), String> {
    if s.is_empty() {
        return Err("thread id must not be empty".into());
    }
    if s.len() > 200 {
        return Err("thread id is too long".into());
    }
    if s.contains('/') || s.contains('\\') {
        return Err("thread id must not contain path separators".into());
    }
    if s.contains('\0') {
        return Err("thread id must not contain NUL".into());
    }
    // Drive prefix (`C:`) or bare colon tricks.
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        return Err("thread id must not contain a drive prefix".into());
    }
    if s.contains(':') {
        return Err("thread id must not contain ':'".into());
    }
    // Dot segments (whole id or as a path component if separators slipped through).
    if s == "." || s == ".." {
        return Err("thread id must not be a dot segment".into());
    }
    if s.split(['/', '\\']).any(|part| part == "." || part == "..") {
        return Err("thread id must not contain dot segments".into());
    }
    // Keep store filenames boring: alnum, hyphen, underscore only.
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("thread id may only contain ASCII letters, digits, '-' and '_'".into());
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPart {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Typed side-channel (e.g. delegation provenance). Empty on v1 threads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::tools::ToolMetadata>,
}

impl ToolPart {
    pub fn running(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: "running".into(),
            summary: None,
            approval_id: None,
            path: None,
            diff: None,
            metadata: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum StoredMessage {
    User {
        id: String,
        text: String,
    },
    Assistant {
        id: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        thinking: String,
        #[serde(default)]
        tools: Vec<ToolPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Slash command that produced this turn. Persisted so a reopened chat
        /// still frames the answer the way it was framed when written —
        /// otherwise an old plan silently degrades to plain text and looks
        /// like a rendering bug. Optional, so older threads load unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(default)]
        streaming: bool,
    },
}

impl StoredMessage {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. } | Self::Assistant { id, .. } => id,
        }
    }
}

#[allow(clippy::type_complexity)]
fn assistant_fields(
    msg: &mut StoredMessage,
) -> Option<(
    &mut String,
    &mut String,
    &mut Vec<ToolPart>,
    &mut Option<String>,
    &mut bool,
)> {
    match msg {
        StoredMessage::Assistant {
            text,
            thinking,
            tools,
            error,
            streaming,
            ..
        } => Some((text, thinking, tools, error, streaming)),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub message_count: usize,
}

/// A durable conversation checkpoint. The full snapshot lives beside the
/// thread file; this small record is what the UI needs to render the rewind
/// affordance without loading every snapshot up front.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCheckpoint {
    pub id: String,
    pub created_at: u64,
    pub label: String,
    pub message_count: usize,
    pub agent_message_count: usize,
}

/// Typed outcomes when loading a thread from disk.
#[derive(Debug, Error)]
pub enum ThreadLoadError {
    #[error("thread `{0}` not found")]
    Missing(String),
    #[error("thread `{id}` is corrupt: {detail}")]
    Corrupt { id: String, detail: String },
    #[error(
        "thread `{id}` format v{found} is newer than supported v{supported}; refusing to rewrite"
    )]
    UnsupportedVersion {
        id: String,
        found: u32,
        supported: u32,
    },
    #[error("thread `{id}` I/O error: {detail}")]
    Io { id: String, detail: String },
    #[error("invalid thread id: {0}")]
    InvalidId(String),
    #[error("thread `{id}` belongs to provider `{owned}`, not `{wanted}`")]
    ProviderMismatch {
        id: String,
        owned: String,
        wanted: String,
    },
}

impl From<ThreadLoadError> for HarnessError {
    fn from(err: ThreadLoadError) -> Self {
        HarnessError::Other(err.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    /// On-disk schema version. Missing / 0 in pre-alpha files → migrated to 1.
    #[serde(default)]
    pub version: u32,
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Provider that owns this conversation (parent is always pinned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Wire format for `agent_messages` (e.g. anthropic_messages).
    #[serde(default = "default_wire_format")]
    pub wire_format: String,
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
    /// Conversation checkpoints stored under `.zest/threads/checkpoints/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<ThreadCheckpoint>,
    /// Wire messages for restoring `Agent.messages` so the model sees prior context.
    #[serde(default)]
    pub agent_messages: Vec<Message>,
}

fn default_wire_format() -> String {
    WIRE_FORMAT_ANTHROPIC_MESSAGES.to_string()
}

/// Outcome of loading a thread, including non-fatal migration notes.
#[derive(Debug, Clone)]
pub struct ThreadLoad {
    pub thread: Thread,
    /// Soft warning for the UI (migration notes, recovered interrupted tools, …).
    pub warning: Option<String>,
}

impl Thread {
    pub fn new() -> Self {
        let now = now_secs();
        let id = ThreadId::parse(new_id("thread")).expect("generated thread id is always valid");
        Self {
            version: THREAD_FORMAT_VERSION,
            id: id.as_str().to_string(),
            created_at: now,
            updated_at: now,
            title: None,
            provider_id: None,
            wire_format: default_wire_format(),
            messages: Vec::new(),
            checkpoints: Vec::new(),
            agent_messages: Vec::new(),
        }
    }

    pub fn with_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    /// Fill missing version / wire-format fields from older files.
    ///
    /// Returns `Err` when the on-disk version is newer than this binary supports
    /// — callers must not rewrite those threads.
    pub fn migrate_in_place(&mut self) -> std::result::Result<Option<String>, ThreadLoadError> {
        if self.version > THREAD_FORMAT_VERSION {
            return Err(ThreadLoadError::UnsupportedVersion {
                id: self.id.clone(),
                found: self.version,
                supported: THREAD_FORMAT_VERSION,
            });
        }
        let mut notes = Vec::new();
        if self.version < THREAD_FORMAT_VERSION {
            let from = self.version;
            self.version = THREAD_FORMAT_VERSION;
            if from == 0 {
                notes.push("migrated thread to format v2".to_string());
            } else {
                notes.push(format!(
                    "migrated thread from format v{from} to v{THREAD_FORMAT_VERSION}"
                ));
            }
        }
        if self.wire_format.trim().is_empty() {
            self.wire_format = default_wire_format();
            notes.push("filled missing wireFormat".into());
        }
        Ok(if notes.is_empty() {
            None
        } else {
            Some(notes.join("; "))
        })
    }

    /// Refuse to change an already-pinned provider owner.
    pub fn assert_provider(&self, provider_id: &str) -> std::result::Result<(), ThreadLoadError> {
        match self.provider_id.as_deref() {
            None => Ok(()),
            Some(owned) if owned == provider_id => Ok(()),
            Some(owned) => Err(ThreadLoadError::ProviderMismatch {
                id: self.id.clone(),
                owned: owned.to_string(),
                wanted: provider_id.to_string(),
            }),
        }
    }

    /// Pin provider once. Never rewrites an existing owner.
    pub fn ensure_provider(
        &mut self,
        provider_id: &str,
    ) -> std::result::Result<(), ThreadLoadError> {
        self.assert_provider(provider_id)?;
        if self.provider_id.is_none() {
            self.provider_id = Some(provider_id.to_string());
        }
        Ok(())
    }

    /// Convert interrupted approvals / still-running tools into terminal error
    /// cards so a restart never leaves forever-pending UI state.
    pub fn terminalize_interrupted(&mut self) -> bool {
        let mut changed = false;
        for msg in &mut self.messages {
            let Some((_, _, tools, _, streaming)) = assistant_fields(msg) else {
                continue;
            };
            if *streaming {
                *streaming = false;
                changed = true;
            }
            for tool in tools.iter_mut() {
                match tool.status.as_str() {
                    "awaiting_approval" => {
                        tool.status = "error".into();
                        tool.summary = Some(match tool.summary.take() {
                            Some(s) if !s.is_empty() => format!("{s} (approval interrupted)"),
                            _ => "approval interrupted".into(),
                        });
                        tool.approval_id = None;
                        changed = true;
                    }
                    "running" => {
                        tool.status = "error".into();
                        tool.summary = Some(match tool.summary.take() {
                            Some(s) if !s.is_empty() => format!("{s} (interrupted)"),
                            _ => "tool interrupted".into(),
                        });
                        tool.approval_id = None;
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        if changed {
            self.touch();
        }
        changed
    }

    pub fn thread_id(&self) -> std::result::Result<ThreadId, String> {
        ThreadId::parse(&self.id)
    }

    pub fn summary(&self) -> ThreadSummary {
        ThreadSummary {
            id: self.id.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            title: self.title.clone(),
            provider_id: self.provider_id.clone(),
            message_count: self.messages.len(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_secs();
    }

    fn ensure_title_from_user(&mut self, text: &str) {
        if self.title.is_some() {
            return;
        }
        let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.is_empty() {
            return;
        }
        let title: String = flat.chars().take(72).collect();
        self.title = Some(title);
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut StoredMessage> {
        self.messages.iter_mut().find(|m| m.id() == id)
    }

    fn ensure_assistant(&mut self, message_id: &str) {
        if self.find_mut(message_id).is_some() {
            return;
        }
        self.messages.push(StoredMessage::Assistant {
            id: message_id.to_string(),
            text: String::new(),
            thinking: String::new(),
            tools: Vec::new(),
            error: None,
            command: None,
            streaming: true,
        });
    }

    /// Upsert UI projection from a chat-event shape (desktop emits these).
    pub fn apply_user(&mut self, message_id: &str, text: &str) {
        if self.find_mut(message_id).is_none() {
            self.messages.push(StoredMessage::User {
                id: message_id.to_string(),
                text: text.to_string(),
            });
        }
        self.ensure_title_from_user(text);
        self.touch();
    }

    /// Create an empty streaming assistant row before the first delta.
    pub fn apply_assistant_start(&mut self, message_id: &str, command: Option<&str>) {
        self.ensure_assistant(message_id);
        if let Some(name) = command {
            if let Some(StoredMessage::Assistant { command, .. }) = self.find_mut(message_id) {
                *command = Some(name.to_string());
            }
        }
        self.touch();
    }

    pub fn apply_text_delta(&mut self, message_id: &str, text: &str) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((body, _, _, _, streaming)) = assistant_fields(msg) {
                body.push_str(text);
                *streaming = true;
            }
        }
        self.touch();
    }

    pub fn apply_thinking_delta(&mut self, message_id: &str, text: &str) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, thinking, _, _, streaming)) = assistant_fields(msg) {
                thinking.push_str(text);
                *streaming = true;
            }
        }
        self.touch();
    }

    pub fn apply_tool_start(&mut self, message_id: &str, tool_id: &str, name: &str) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, _, tools, _, streaming)) = assistant_fields(msg) {
                if !tools.iter().any(|t| t.id == tool_id) {
                    tools.push(ToolPart::running(tool_id, name));
                }
                *streaming = true;
            }
        }
        self.touch();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_approval_needed(
        &mut self,
        message_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        approval_id: &str,
        path: &str,
        summary: &str,
        diff: &str,
    ) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, _, tools, _, streaming)) = assistant_fields(msg) {
                if let Some(tool) = tools.iter_mut().find(|t| t.id == tool_call_id) {
                    tool.status = "awaiting_approval".into();
                    tool.approval_id = Some(approval_id.to_string());
                    tool.path = Some(path.to_string());
                    tool.summary = Some(summary.to_string());
                    tool.diff = Some(diff.to_string());
                } else {
                    tools.push(ToolPart {
                        id: tool_call_id.to_string(),
                        name: tool_name.to_string(),
                        status: "awaiting_approval".into(),
                        summary: Some(summary.to_string()),
                        approval_id: Some(approval_id.to_string()),
                        path: Some(path.to_string()),
                        diff: Some(diff.to_string()),
                        metadata: None,
                    });
                }
                *streaming = true;
            }
        }
        self.touch();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_result(
        &mut self,
        message_id: &str,
        tool_id: &str,
        name: &str,
        summary: &str,
        is_error: bool,
        path: Option<&str>,
        diff: Option<&str>,
        metadata: Option<crate::tools::ToolMetadata>,
    ) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, _, tools, _, streaming)) = assistant_fields(msg) {
                if let Some(tool) = tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.status = if is_error { "error" } else { "done" }.into();
                    tool.summary = Some(summary.to_string());
                    tool.approval_id = None;
                    if let Some(path) = path {
                        tool.path = Some(path.to_string());
                    }
                    if let Some(diff) = diff {
                        tool.diff = Some(diff.to_string());
                    }
                    if metadata.is_some() {
                        tool.metadata = metadata;
                    }
                    // Keep path/diff on the card for context after allow/deny.
                } else {
                    tools.push(ToolPart {
                        id: tool_id.to_string(),
                        name: name.to_string(),
                        status: if is_error { "error" } else { "done" }.into(),
                        summary: Some(summary.to_string()),
                        approval_id: None,
                        path: path.map(str::to_string),
                        diff: diff.map(str::to_string),
                        metadata,
                    });
                }
                *streaming = true;
            }
        }
        self.touch();
    }

    pub fn apply_done(&mut self, message_id: &str) {
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, _, _, _, streaming)) = assistant_fields(msg) {
                *streaming = false;
            }
        }
        self.touch();
    }

    pub fn apply_error(&mut self, message_id: &str, message: &str) {
        self.ensure_assistant(message_id);
        if let Some(msg) = self.find_mut(message_id) {
            if let Some((_, _, _, error, streaming)) = assistant_fields(msg) {
                *error = Some(message.to_string());
                *streaming = false;
            }
        }
        self.touch();
    }

    pub fn set_agent_messages(&mut self, messages: Vec<Message>) {
        self.agent_messages = messages;
        self.touch();
    }
}

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}

/// `<workspace>/.zest/threads`.
pub struct ThreadStore {
    dir: PathBuf,
}

impl ThreadStore {
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let dir = workspace_root.as_ref().join(".zest").join("threads");
        fs::create_dir_all(&dir).map_err(|e| {
            HarnessError::Other(format!("create thread dir {}: {e}", dir.display()))
        })?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_for(&self, id: &ThreadId) -> PathBuf {
        self.dir.join(format!("{}.json", id.as_str()))
    }

    fn checkpoints_dir_for(&self, id: &ThreadId) -> PathBuf {
        self.dir.join("checkpoints").join(id.as_str())
    }

    pub fn save(&self, thread: &Thread) -> Result<()> {
        let id = ThreadId::parse(&thread.id)
            .map_err(|e| HarnessError::Other(format!("invalid thread id: {e}")))?;
        if thread.version > THREAD_FORMAT_VERSION {
            return Err(ThreadLoadError::UnsupportedVersion {
                id: thread.id.clone(),
                found: thread.version,
                supported: THREAD_FORMAT_VERSION,
            }
            .into());
        }
        let mut thread = thread.clone();
        thread.version = THREAD_FORMAT_VERSION;
        if thread.wire_format.trim().is_empty() {
            thread.wire_format = default_wire_format();
        }
        let path = self.path_for(&id);
        fsutil::atomic_write_json(&path, &thread)
            .map_err(|e| HarnessError::Other(format!("write thread {}: {e}", path.display())))?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Thread> {
        Ok(self.load_with_recovery(id)?.thread)
    }

    /// Load + migrate + terminalize interrupted in-flight tool/approval cards.
    pub fn load_with_recovery(&self, id: &str) -> Result<ThreadLoad> {
        self.load_typed(id).map_err(Into::into)
    }

    /// Typed load used by desktop restore / provider ownership checks.
    pub fn load_typed(&self, id: &str) -> std::result::Result<ThreadLoad, ThreadLoadError> {
        let tid = ThreadId::parse(id).map_err(ThreadLoadError::InvalidId)?;
        let path = self.path_for(&tid);
        let body = match fs::read_to_string(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ThreadLoadError::Missing(tid.as_str().to_string()));
            }
            Err(e) => {
                return Err(ThreadLoadError::Io {
                    id: tid.as_str().to_string(),
                    detail: e.to_string(),
                });
            }
        };
        let mut thread: Thread = match serde_json::from_str(&body) {
            Ok(t) => t,
            Err(e) => {
                let preserved = preserve_corrupt(&path).map_err(|err| ThreadLoadError::Io {
                    id: tid.as_str().to_string(),
                    detail: err.to_string(),
                })?;
                return Err(ThreadLoadError::Corrupt {
                    id: tid.as_str().to_string(),
                    detail: format!("preserved as {}; parse error: {e}", preserved.display()),
                });
            }
        };

        // Ensure id in file matches request (path is authoritative).
        if thread.id != tid.as_str() {
            thread.id = tid.as_str().to_string();
        }

        let mut warnings = Vec::new();
        match thread.migrate_in_place() {
            Ok(Some(note)) => warnings.push(note),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        if thread.terminalize_interrupted() {
            warnings.push("interrupted tools/approvals were closed after restart".into());
            // Persist the terminalized projection so reopen stays stable.
            let _ = self.save(&thread);
        } else if !warnings.is_empty() {
            let _ = self.save(&thread);
        }

        Ok(ThreadLoad {
            thread,
            warning: if warnings.is_empty() {
                None
            } else {
                Some(warnings.join("; "))
            },
        })
    }

    /// Load and reject cross-provider restore (never rewrites `provider_id`).
    pub fn load_for_provider(
        &self,
        id: &str,
        provider_id: &str,
    ) -> std::result::Result<ThreadLoad, ThreadLoadError> {
        let loaded = self.load_typed(id)?;
        loaded.thread.assert_provider(provider_id)?;
        Ok(loaded)
    }

    pub fn load_or_none(&self, id: &str) -> Option<Thread> {
        self.load_with_recovery(id).ok().map(|l| l.thread)
    }

    pub fn create(&self) -> Result<Thread> {
        let thread = Thread::new();
        self.save(&thread)?;
        Ok(thread)
    }

    pub fn create_for_provider(&self, provider_id: &str) -> Result<Thread> {
        let thread = Thread::new().with_provider(provider_id);
        self.save(&thread)?;
        Ok(thread)
    }

    /// Save a full conversation snapshot before a turn mutates the thread.
    ///
    /// The snapshot is intentionally separate from the main thread document:
    /// retaining a bounded list of metadata there keeps the sidebar cheap while
    /// rewind still has the exact provider wire history it needs.
    pub fn create_checkpoint(
        &self,
        thread: &mut Thread,
        label: impl Into<String>,
    ) -> Result<ThreadCheckpoint> {
        let thread_id = ThreadId::parse(&thread.id)
            .map_err(|e| HarnessError::Other(format!("invalid thread id: {e}")))?;
        let checkpoint_id =
            ThreadId::parse(new_id("checkpoint")).expect("generated checkpoint id is always valid");
        let dir = self.checkpoints_dir_for(&thread_id);
        fs::create_dir_all(&dir).map_err(|e| {
            HarnessError::Other(format!("create checkpoint dir {}: {e}", dir.display()))
        })?;

        let mut snapshot = thread.clone();
        // Metadata belongs to the live thread. Keeping it out of the snapshot
        // prevents a rewind from recursively carrying future checkpoints back.
        snapshot.checkpoints.clear();
        let path = dir.join(format!("{}.json", checkpoint_id.as_str()));
        fsutil::atomic_write_json(&path, &snapshot).map_err(|e| {
            HarnessError::Other(format!("write checkpoint {}: {e}", path.display()))
        })?;

        let checkpoint = ThreadCheckpoint {
            id: checkpoint_id.to_string(),
            created_at: now_secs(),
            label: label.into(),
            message_count: thread.messages.len(),
            agent_message_count: thread.agent_messages.len(),
        };
        thread.checkpoints.push(checkpoint.clone());

        const MAX_CHECKPOINTS: usize = 24;
        if thread.checkpoints.len() > MAX_CHECKPOINTS {
            let removed = thread
                .checkpoints
                .drain(0..thread.checkpoints.len() - MAX_CHECKPOINTS)
                .collect::<Vec<_>>();
            for old in removed {
                let _ = fs::remove_file(dir.join(format!("{}.json", old.id)));
            }
        }
        thread.touch();
        self.save(thread)?;
        Ok(checkpoint)
    }

    /// Restore a checkpoint snapshot. The caller decides whether it should
    /// replace the active session; this method only validates and reads it.
    pub fn load_checkpoint(&self, thread_id: &str, checkpoint_id: &str) -> Result<Thread> {
        let thread_id = ThreadId::parse(thread_id)
            .map_err(|e| HarnessError::Other(format!("invalid thread id: {e}")))?;
        let checkpoint_id = ThreadId::parse(checkpoint_id)
            .map_err(|e| HarnessError::Other(format!("invalid checkpoint id: {e}")))?;
        let path = self
            .checkpoints_dir_for(&thread_id)
            .join(format!("{}.json", checkpoint_id.as_str()));
        let raw = fs::read_to_string(&path)
            .map_err(|e| HarnessError::Other(format!("read checkpoint {}: {e}", path.display())))?;
        let mut snapshot: Thread = serde_json::from_str(&raw).map_err(|e| {
            HarnessError::Other(format!("checkpoint {} is corrupt: {e}", path.display()))
        })?;
        if snapshot.id != thread_id.as_str() {
            return Err(HarnessError::Other(
                "checkpoint belongs to a different thread".into(),
            ));
        }
        snapshot.checkpoints.clear();
        Ok(snapshot)
    }

    /// Fork the current thread without sharing future checkpoint state.
    pub fn fork(&self, source: &Thread, title: Option<&str>) -> Result<Thread> {
        let mut fork = source.clone();
        fork.id = new_id("thread");
        fork.created_at = now_secs();
        fork.updated_at = fork.created_at;
        fork.title = title
            .map(str::to_string)
            .or_else(|| source.title.as_ref().map(|t| format!("Fork: {t}")));
        fork.checkpoints.clear();
        self.save(&fork)?;
        Ok(fork)
    }

    /// Permanently remove a thread file. Missing files are success (idempotent).
    pub fn delete(&self, id: &str) -> Result<()> {
        let tid = ThreadId::parse(id)
            .map_err(|e| HarnessError::Other(format!("invalid thread id: {e}")))?;
        let path = self.path_for(&tid);
        match fs::remove_file(&path) {
            Ok(()) => {
                let _ = fs::remove_dir_all(self.checkpoints_dir_for(&tid));
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(HarnessError::Other(format!(
                "delete thread {}: {e}",
                path.display()
            ))),
        }
    }

    pub fn list(&self) -> Result<Vec<ThreadSummary>> {
        self.list_filtered(None)
    }

    /// Recent threads for one provider (active-provider filter).
    pub fn list_for_provider(&self, provider_id: &str) -> Result<Vec<ThreadSummary>> {
        self.list_filtered(Some(provider_id))
    }

    fn list_filtered(&self, provider_id: Option<&str>) -> Result<Vec<ThreadSummary>> {
        let mut out = Vec::new();
        let entries = fs::read_dir(&self.dir).map_err(|e| {
            HarnessError::Other(format!("list threads {}: {e}", self.dir.display()))
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Skip temps and preserved corrupt siblings.
            if !name.ends_with(".json") || name.contains(".corrupt") {
                continue;
            }
            let Ok(body) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(thread) = serde_json::from_str::<Thread>(&body) else {
                continue;
            };
            // Skip unsupported newer versions rather than rewriting them.
            if thread.version > THREAD_FORMAT_VERSION {
                continue;
            }
            if let Some(want) = provider_id {
                match thread.provider_id.as_deref() {
                    Some(id) if id == want => {}
                    _ => continue,
                }
            }
            out.push(thread.summary());
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(out)
    }
}

/// Rename a corrupt thread file aside so it is not overwritten.
fn preserve_corrupt(path: &Path) -> Result<PathBuf> {
    let stamp = now_secs();
    let preserved = path.with_extension(format!("json.corrupt-{stamp}"));
    fs::rename(path, &preserved).map_err(|e| {
        HarnessError::Other(format!("preserve corrupt thread {}: {e}", path.display()))
    })?;
    Ok(preserved)
}

#[cfg(test)]
mod characterization {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-thread-{name}-{}", new_id("tmp")));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn store_create_save_load_round_trip() {
        let root = scratch("roundtrip");
        let store = ThreadStore::open(&root).unwrap();
        assert!(store.dir().ends_with(Path::new(".zest").join("threads")));

        let mut thread = store.create().unwrap();
        thread.apply_user("user-1", "first question about the repo");
        thread.apply_text_delta("asst-1", "hello ");
        thread.apply_text_delta("asst-1", "world");
        thread.apply_done("asst-1");
        store.save(&thread).unwrap();

        let loaded = store.load(&thread.id).unwrap();
        assert_eq!(loaded.id, thread.id);
        assert_eq!(
            loaded.title.as_deref(),
            Some("first question about the repo")
        );
        assert_eq!(loaded.messages.len(), 2);
        match &loaded.messages[0] {
            StoredMessage::User { id, text } => {
                assert_eq!(id, "user-1");
                assert_eq!(text, "first question about the repo");
            }
            other => panic!("expected user message, got {other:?}"),
        }
        match &loaded.messages[1] {
            StoredMessage::Assistant {
                id,
                text,
                streaming,
                ..
            } => {
                assert_eq!(id, "asst-1");
                assert_eq!(text, "hello world");
                assert!(!streaming);
            }
            other => panic!("expected assistant message, got {other:?}"),
        }

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, thread.id);
        assert_eq!(listed[0].message_count, 2);
    }

    #[test]
    fn checkpoints_restore_wire_history_and_forks_start_clean() {
        let root = scratch("checkpoint");
        let store = ThreadStore::open(&root).unwrap();
        let mut thread = store.create_for_provider("codex").unwrap();
        thread.apply_user("u1", "first question");
        thread.apply_assistant_start("a1", None);
        thread.apply_text_delta("a1", "first answer");
        thread.apply_done("a1");
        thread.agent_messages = vec![Message::user_text("first question")];
        store.save(&thread).unwrap();

        let checkpoint = store
            .create_checkpoint(&mut thread, "Before the next turn")
            .unwrap();
        assert_eq!(thread.checkpoints.len(), 1);
        assert_eq!(checkpoint.message_count, 2);

        thread.apply_user("u2", "second question");
        store.save(&thread).unwrap();
        let restored = store.load_checkpoint(&thread.id, &checkpoint.id).unwrap();
        assert_eq!(restored.id, thread.id);
        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.agent_messages.len(), 1);
        assert!(restored.checkpoints.is_empty());

        let fork = store.fork(&thread, None).unwrap();
        assert_ne!(fork.id, thread.id);
        assert_eq!(fork.provider_id.as_deref(), Some("codex"));
        assert!(fork.title.as_deref().unwrap().starts_with("Fork:"));
        assert!(fork.checkpoints.is_empty());
        assert!(store.load(&fork.id).is_ok());

        store.delete(&thread.id).unwrap();
        assert!(!store
            .checkpoints_dir_for(&ThreadId::parse(&thread.id).unwrap())
            .exists());
    }

    #[test]
    fn apply_chat_event_upserts_preserve_tool_and_approval_fields() {
        let mut thread = Thread::new();
        thread.apply_user("u1", "edit the file");
        thread.apply_thinking_delta("a1", "planning…");
        thread.apply_text_delta("a1", "I'll write");
        thread.apply_tool_start("a1", "tool-1", "write_file");
        thread.apply_approval_needed(
            "a1",
            "tool-1",
            "write_file",
            "approval-1",
            "src/main.rs",
            "write src/main.rs",
            "@@ -1 +1 @@\n-old\n+new\n",
        );

        match &thread.messages[1] {
            StoredMessage::Assistant {
                thinking,
                text,
                tools,
                streaming,
                ..
            } => {
                assert_eq!(thinking, "planning…");
                assert_eq!(text, "I'll write");
                assert!(*streaming);
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].id, "tool-1");
                assert_eq!(tools[0].status, "awaiting_approval");
                assert_eq!(tools[0].approval_id.as_deref(), Some("approval-1"));
                assert_eq!(tools[0].path.as_deref(), Some("src/main.rs"));
                assert_eq!(tools[0].summary.as_deref(), Some("write src/main.rs"));
                assert!(tools[0].diff.as_ref().unwrap().contains("+new"));
            }
            other => panic!("expected assistant, got {other:?}"),
        }

        // Duplicate tool_start is a no-op for the same id.
        thread.apply_tool_start("a1", "tool-1", "write_file");
        assert_eq!(
            match &thread.messages[1] {
                StoredMessage::Assistant { tools, .. } => tools.len(),
                _ => 0,
            },
            1
        );

        thread.apply_tool_result(
            "a1",
            "tool-1",
            "write_file",
            "wrote src/main.rs",
            false,
            None,
            None,
            None,
        );
        match &thread.messages[1] {
            StoredMessage::Assistant { tools, .. } => {
                assert_eq!(tools[0].status, "done");
                assert_eq!(tools[0].summary.as_deref(), Some("wrote src/main.rs"));
                assert!(tools[0].approval_id.is_none());
                // Path/diff retained after allow for card context.
                assert_eq!(tools[0].path.as_deref(), Some("src/main.rs"));
                assert!(tools[0].diff.is_some());
            }
            other => panic!("expected assistant, got {other:?}"),
        }

        thread.apply_error("a1", "upstream failed");
        match &thread.messages[1] {
            StoredMessage::Assistant {
                error, streaming, ..
            } => {
                assert_eq!(error.as_deref(), Some("upstream failed"));
                assert!(!streaming);
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }

    #[test]
    fn load_or_none_and_duplicate_user_id_are_stable() {
        let root = scratch("stable");
        let store = ThreadStore::open(&root).unwrap();
        assert!(store.load_or_none("missing").is_none());

        let mut thread = Thread::new();
        thread.apply_user("u1", "hello");
        thread.apply_user("u1", "ignored duplicate");
        assert_eq!(thread.messages.len(), 1);
        match &thread.messages[0] {
            StoredMessage::User { text, .. } => assert_eq!(text, "hello"),
            other => panic!("expected user, got {other:?}"),
        }
        // First user text still owns the title.
        assert_eq!(thread.title.as_deref(), Some("hello"));
    }

    #[test]
    fn stored_message_json_uses_role_tag_and_camel_case_tools() {
        let mut thread = Thread::new();
        thread.apply_user("u1", "hi");
        thread.apply_approval_needed(
            "a1",
            "t1",
            "write_file",
            "approval-1",
            "f.txt",
            "write f.txt",
            "diff",
        );
        let json = serde_json::to_value(&thread).unwrap();
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][1]["role"], "assistant");
        // ToolPart optional fields are camelCase on the wire.
        let tool = &json["messages"][1]["tools"][0];
        assert_eq!(tool["status"], "awaiting_approval");
        assert_eq!(tool["approvalId"], "approval-1");
        assert_eq!(tool["path"], "f.txt");
        assert!(tool.get("approval_id").is_none());
    }

    #[test]
    fn thread_id_rejects_traversal_and_drive_prefixes() {
        assert!(ThreadId::parse("thread-abc-1").is_ok());
        assert!(ThreadId::parse("../secret").is_err());
        assert!(ThreadId::parse("..\\secret").is_err());
        assert!(ThreadId::parse("C:windows").is_err());
        assert!(ThreadId::parse("foo/bar").is_err());
        assert!(ThreadId::parse(".").is_err());
        assert!(ThreadId::parse("..").is_err());
        assert!(ThreadId::parse("").is_err());
        assert!(ThreadId::parse("has space").is_err());
    }

    #[test]
    fn store_rejects_traversal_thread_id() {
        let root = scratch("traverse");
        let store = ThreadStore::open(&root).unwrap();
        let err = store.load("../outside").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid thread id"), "{msg}");
    }

    #[test]
    fn store_delete_removes_file_and_is_idempotent() {
        let root = scratch("delete");
        let store = ThreadStore::open(&root).unwrap();
        let thread = store.create_for_provider("codex").unwrap();
        let path = store.dir().join(format!("{}.json", thread.id));
        assert!(path.exists());
        store.delete(&thread.id).unwrap();
        assert!(!path.exists());
        store.delete(&thread.id).unwrap(); // idempotent
        assert!(store.delete("../outside").is_err());
    }

    #[test]
    fn migrates_legacy_thread_json_without_version() {
        let root = scratch("migrate");
        let store = ThreadStore::open(&root).unwrap();
        let id = "thread-legacy1";
        let path = store.dir().join(format!("{id}.json"));
        fs::write(
            &path,
            r#"{
  "id": "thread-legacy1",
  "createdAt": 1,
  "updatedAt": 2,
  "messages": [{"role":"user","id":"u1","text":"hi"}],
  "agentMessages": []
}"#,
        )
        .unwrap();

        let loaded = store.load_with_recovery(id).unwrap();
        assert_eq!(loaded.thread.version, THREAD_FORMAT_VERSION);
        assert_eq!(loaded.thread.wire_format, WIRE_FORMAT_ANTHROPIC_MESSAGES);
        assert!(loaded.warning.is_some());
    }

    #[test]
    fn refuses_newer_thread_format() {
        let root = scratch("newer");
        let store = ThreadStore::open(&root).unwrap();
        let id = "thread-newer1";
        let path = store.dir().join(format!("{id}.json"));
        fs::write(
            &path,
            r#"{
  "version": 99,
  "id": "thread-newer1",
  "createdAt": 1,
  "updatedAt": 2,
  "providerId": "codex",
  "wireFormat": "anthropic_messages",
  "messages": [],
  "agentMessages": []
}"#,
        )
        .unwrap();
        let err = store.load_typed(id).unwrap_err();
        assert!(
            matches!(err, ThreadLoadError::UnsupportedVersion { .. }),
            "{err}"
        );
        // Original file must remain (no rewrite).
        assert!(path.exists());
    }

    #[test]
    fn corrupt_thread_is_preserved_aside() {
        let root = scratch("corrupt");
        let store = ThreadStore::open(&root).unwrap();
        let id = "thread-bad1";
        let path = store.dir().join(format!("{id}.json"));
        fs::write(&path, "{not json").unwrap();
        let err = store.load_with_recovery(id).unwrap_err().to_string();
        assert!(err.contains("corrupt"), "{err}");
        assert!(!path.exists());
        let preserved: Vec<_> = fs::read_dir(store.dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("corrupt"))
            .collect();
        assert_eq!(preserved.len(), 1);
    }

    #[test]
    fn terminalize_interrupted_closes_running_and_approvals() {
        let mut thread = Thread::new();
        thread.apply_tool_start("a1", "t1", "write_file");
        thread.apply_approval_needed("a1", "t2", "write_file", "ap1", "f.txt", "write", "diff");
        assert!(thread.terminalize_interrupted());
        match &thread.messages[0] {
            StoredMessage::Assistant {
                tools, streaming, ..
            } => {
                assert!(!streaming);
                assert_eq!(tools[0].status, "error");
                assert!(tools[0].summary.as_deref().unwrap().contains("interrupted"));
                assert_eq!(tools[1].status, "error");
                assert!(tools[1]
                    .summary
                    .as_deref()
                    .unwrap()
                    .contains("approval interrupted"));
                assert!(tools[1].approval_id.is_none());
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }
    /// A plan reopened tomorrow must still look like a plan; otherwise the
    /// card silently degrades to plain text and reads as a rendering bug.
    #[test]
    fn the_command_that_produced_a_turn_survives_a_reload() {
        let mut thread = Thread::new();
        thread.apply_assistant_start("a1", Some("plan"));
        thread.apply_text_delta("a1", "# Plan");

        let json = serde_json::to_string(&thread).unwrap();
        let back: Thread = serde_json::from_str(&json).unwrap();
        match &back.messages[0] {
            StoredMessage::Assistant { command, text, .. } => {
                assert_eq!(command.as_deref(), Some("plan"));
                assert_eq!(text, "# Plan");
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }

    #[test]
    fn an_ordinary_turn_stores_no_command_and_older_threads_still_load() {
        let mut thread = Thread::new();
        thread.apply_assistant_start("a1", None);
        let json = serde_json::to_string(&thread).unwrap();
        // Omitted rather than null, so the field adds nothing to every message.
        assert!(!json.contains("command"), "{json}");

        // A thread written before the field existed must still deserialize —
        // the field is new, and old threads on disk have never heard of it.
        let legacy = r#"{"version":1,"id":"t1","createdAt":1,"updatedAt":1,
"providerId":"codex","wireFormat":"anthropic_messages","agentMessages":[],
"messages":[{"role":"assistant","id":"a1","text":"hi","thinking":"",
"tools":[],"streaming":false}]}"#;
        let back: Thread = serde_json::from_str(legacy).expect("older threads still load");
        match &back.messages[0] {
            StoredMessage::Assistant { command, text, .. } => {
                assert_eq!(*command, None);
                assert_eq!(text, "hi");
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }
}
