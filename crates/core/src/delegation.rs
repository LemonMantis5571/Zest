//! Durable coordinator/worker/reviewer delegation records.
//!
//! The coordinator is intentionally a small data model rather than a second
//! provider runtime.  Jobs are project-local, versioned, and written with the
//! same atomic-file conventions as chat lifecycle records.  The desktop owns
//! scheduling; this module owns the invariants that must survive a restart.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{ExternalAgentConfig, ExternalWorkspace};
use crate::error::{HarnessError, Result};
use crate::fsutil;
use crate::tools::sensitive::is_sensitive_path;

pub const DELEGATION_FORMAT_VERSION: u32 = 1;
pub const MAX_FEATURE_TITLE_CHARS: usize = 200;
pub const MAX_FEATURE_OBJECTIVE_CHARS: usize = 16_000;
pub const MAX_FEATURE_LANE_CHARS: usize = 120;
pub const MAX_FEATURE_PATHS: usize = 128;
pub const MAX_FEATURE_PATH_CHARS: usize = 2_000;
pub const MAX_FEATURE_CONTEXT_CHARS: usize = 24_000;
pub const MAX_FEATURE_CARD_JSON_CHARS: usize = 64_000;
pub const MAX_FEATURE_CHECKS: usize = 32;
pub const MAX_FEATURE_CHECK_CHARS: usize = 2_000;
pub const MAX_FEATURE_DEPENDENCIES: usize = 64;
pub const MAX_REVIEW_FINDINGS: usize = 128;
pub const MAX_REVIEW_CHECKS: usize = 64;
pub const MAX_REVIEW_OUTPUT_CHARS: usize = 16_000;
pub const MAX_REVIEW_REPORT_CHARS: usize = 128_000;

static DELEGATION_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static APPLY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn state_lock() -> Result<MutexGuard<'static, ()>> {
    DELEGATION_STATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| HarnessError::Other("delegation state lock poisoned".into()))
}

fn apply_lock() -> Result<MutexGuard<'static, ()>> {
    APPLY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| HarnessError::Other("delegation apply lock poisoned".into()))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn clip_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn validate_id(raw: &str, kind: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(HarnessError::Other(format!("{kind} id must not be empty")));
    }
    if raw.len() > 200 {
        return Err(HarnessError::Other(format!("{kind} id is too long")));
    }
    if !raw
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(HarnessError::Other(format!(
            "{kind} id may only contain ASCII letters, digits, '-' and '_'"
        )));
    }
    Ok(raw.to_string())
}

/// The smallest useful description of a project checkout at dispatch time.
/// The fingerprint is deliberately opaque; no working-tree content is stored
/// in the job record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub head: Option<String>,
    pub fingerprint: String,
    pub captured_at: u64,
}

/// A bounded, explicit implementation request sent to one external worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureCard {
    pub version: u32,
    pub card_id: String,
    pub title: String,
    pub objective: String,
    pub lane: String,
    pub scope: Vec<String>,
    pub context: Vec<String>,
    pub depends_on: Vec<String>,
    pub agent: String,
    pub acceptance_checks: Vec<String>,
    pub review_required: bool,
    pub created_at: u64,
}

impl FeatureCard {
    pub fn validate(
        &self,
        root: &Path,
        agents: &BTreeMap<String, ExternalAgentConfig>,
    ) -> Result<()> {
        if self.version != DELEGATION_FORMAT_VERSION {
            return Err(HarnessError::Other(format!(
                "unsupported feature card version {}",
                self.version
            )));
        }
        validate_id(&self.card_id, "feature card")?;
        validate_text(&self.title, "title", MAX_FEATURE_TITLE_CHARS)?;
        validate_text(&self.objective, "objective", MAX_FEATURE_OBJECTIVE_CHARS)?;
        validate_text(&self.lane, "lane", MAX_FEATURE_LANE_CHARS)?;
        if self.scope.is_empty() {
            return Err(HarnessError::Other(
                "feature card scope must not be empty".into(),
            ));
        }
        validate_paths(root, &self.scope, "scope")?;
        validate_paths(root, &self.context, "context")?;
        let context_chars: usize = self.context.iter().map(|path| path.chars().count()).sum();
        if context_chars > MAX_FEATURE_CONTEXT_CHARS {
            return Err(HarnessError::Other(
                "feature card context selection is too large".into(),
            ));
        }
        if self.depends_on.len() > MAX_FEATURE_DEPENDENCIES {
            return Err(HarnessError::Other(
                "too many feature card dependencies".into(),
            ));
        }
        for dependency in &self.depends_on {
            validate_id(dependency, "dependency")?;
        }
        if self.acceptance_checks.len() > MAX_FEATURE_CHECKS {
            return Err(HarnessError::Other("too many acceptance checks".into()));
        }
        for check in &self.acceptance_checks {
            validate_check_command(check)?;
        }
        if self.agent.trim().is_empty() {
            return Err(HarnessError::Other(
                "feature card agent must not be empty".into(),
            ));
        }
        let config = agents.get(&self.agent).ok_or_else(|| {
            HarnessError::Other(format!("external agent {} is not configured", self.agent))
        })?;
        if config.workspace != ExternalWorkspace::Isolated {
            return Err(HarnessError::Other(
                "feature cards require an isolated external worker workspace".into(),
            ));
        }
        if !self.review_required {
            return Err(HarnessError::Other(
                "implementation feature cards require an independent review".into(),
            ));
        }
        let encoded = serde_json::to_string(self)
            .map_err(|error| HarnessError::Other(format!("encode feature card: {error}")))?;
        if encoded.chars().count() > MAX_FEATURE_CARD_JSON_CHARS {
            return Err(HarnessError::Other(
                "feature card payload is too large".into(),
            ));
        }
        Ok(())
    }

    pub fn prompt(
        &self,
        root: &Path,
        snapshot: &WorkspaceSnapshot,
        dependency_summary: &str,
    ) -> String {
        let context = bounded_context(root, &self.context);
        let card = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        format!(
            "You are a fresh Zest implementation worker. Work only on this feature card. Do not create delegation jobs, do not address the end user, and do not use unrelated conversation history. Your edits stay in this isolated workspace and will be returned as a diff.\n\n# Feature card\n{card}\n\n# Project instructions and selected context\n{context}\n\n# Dependency summaries\n{dependency_summary}\n\n# Workspace snapshot\nhead={:?}\nfingerprint={}\n\nImplement the objective within the declared scope. Run the listed acceptance checks when practical. Return concise JSON with exactly these fields: summary (string), changedFiles (array of relative paths), checksAttempted (array of strings), blockers (array of strings).",
            snapshot.head, snapshot.fingerprint
        )
    }

    pub fn review_prompt(
        &self,
        root: &Path,
        snapshot: &WorkspaceSnapshot,
        worker_result: &WorkerResult,
    ) -> String {
        let context = bounded_context(root, &self.context);
        let card = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        let worker = serde_json::to_string_pretty(worker_result).unwrap_or_else(|_| "{}".into());
        let checks = if self.acceptance_checks.is_empty() {
            "(none declared)".to_string()
        } else {
            self.acceptance_checks
                .iter()
                .map(|check| format!("- {check}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "You are a fresh, independent Zest reviewer. Inspect the current isolated workspace and the applied worker diff. You may read files and run checks, but you must not edit files and you must not create delegation jobs. Any reviewer-side edits are discarded. Do not address the end user.\n\n# Feature card\n{card}\n\n# Selected context\n{context}\n\n# Worker result\n{worker}\n\n# Required acceptance checks\n{checks}\n\n# Workspace snapshot\nhead={:?}\nfingerprint={}\n\nReturn only one JSON object with this shape: {{\"decision\":\"accepted\" or \"changes_requested\",\"summary\":\"…\",\"findings\":[{{\"severity\":\"blocking\" or \"advisory\",\"path\":\"relative/path\",\"message\":\"…\"}}],\"checks\":[{{\"command\":\"…\",\"status\":\"passed\" or \"failed\" or \"skipped\",\"output\":\"…\"}}]}}. Include evidence for every required check; never claim a check passed without running it.",
            snapshot.head, snapshot.fingerprint
        )
    }
}

fn validate_text(value: &str, field: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() {
        return Err(HarnessError::Other(format!(
            "feature card {field} must not be empty"
        )));
    }
    if value.chars().count() > max {
        return Err(HarnessError::Other(format!(
            "feature card {field} exceeds {max} characters"
        )));
    }
    if value.contains('\0') {
        return Err(HarnessError::Other(format!(
            "feature card {field} contains NUL"
        )));
    }
    Ok(())
}

fn validate_check_command(command: &str) -> Result<()> {
    if command.trim().is_empty() {
        return Err(HarnessError::Other(
            "acceptance check must not be empty".into(),
        ));
    }
    if command.chars().count() > MAX_FEATURE_CHECK_CHARS {
        return Err(HarnessError::Other(format!(
            "acceptance check exceeds {MAX_FEATURE_CHECK_CHARS} characters"
        )));
    }
    if command.contains('\0') || command.contains('\n') || command.contains('\r') {
        return Err(HarnessError::Other(
            "acceptance checks must be one command per item".into(),
        ));
    }
    Ok(())
}

fn validate_paths(root: &Path, paths: &[String], field: &str) -> Result<()> {
    if paths.len() > MAX_FEATURE_PATHS {
        return Err(HarnessError::Other(format!(
            "too many feature card {field} paths"
        )));
    }
    for raw in paths {
        if raw.chars().count() > MAX_FEATURE_PATH_CHARS {
            return Err(HarnessError::Other(format!(
                "feature card {field} path is too long"
            )));
        }
        let path = safe_relative_path(raw, field)?;
        let candidate = root.join(&path);
        if candidate.exists() {
            let resolved = std::fs::canonicalize(&candidate).map_err(|error| {
                HarnessError::Other(format!(
                    "cannot resolve feature card {field} path {raw}: {error}"
                ))
            })?;
            let root = std::fs::canonicalize(root).map_err(|error| {
                HarnessError::Other(format!("cannot resolve project root: {error}"))
            })?;
            if !resolved.starts_with(root) {
                return Err(HarnessError::Other(format!(
                    "feature card {field} path {raw} resolves outside the project"
                )));
            }
        }
    }
    Ok(())
}

fn safe_relative_path(raw: &str, field: &str) -> Result<PathBuf> {
    if raw.trim().is_empty() || Path::new(raw).is_absolute() {
        return Err(HarnessError::Other(format!(
            "feature card {field} paths must be non-empty and relative"
        )));
    }
    let mut output = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return Err(HarnessError::Other(format!(
                        "feature card {field} path {raw} escapes the project"
                    )));
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(HarnessError::Other(format!(
                    "feature card {field} paths must remain inside the project"
                )))
            }
        }
    }
    if output.as_os_str().is_empty() && matches!(raw.trim(), "." | "./") {
        return Ok(PathBuf::from("."));
    }
    if output.as_os_str().is_empty() {
        return Err(HarnessError::Other(format!(
            "feature card {field} path {raw} is invalid"
        )));
    }
    Ok(output)
}

fn bounded_context(root: &Path, paths: &[String]) -> String {
    let mut output = String::new();
    let mut remaining = MAX_FEATURE_CONTEXT_CHARS;
    let instructions = crate::prompt::load_project_docs(root);
    if !instructions.trim().is_empty() {
        output.push_str("## Project instructions\n");
        let clipped = clip_chars(&instructions, remaining.min(12_000));
        remaining = remaining.saturating_sub(clipped.chars().count());
        output.push_str(&clipped);
        output.push('\n');
    }
    for raw in paths {
        if remaining < 100 {
            break;
        }
        let Ok(relative) = safe_relative_path(raw, "context") else {
            continue;
        };
        let path = root.join(&relative);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let take = remaining.min(6_000);
        let clipped = clip_chars(&content, take);
        output.push_str(&format!("\n## {raw}\n{clipped}\n"));
        remaining = remaining.saturating_sub(clipped.chars().count());
    }
    if output.trim().is_empty() {
        "(no selected context files were available)".into()
    } else {
        output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    Planned,
    AwaitingApproval,
    WorkerRunning,
    ReviewRunning,
    ReadyToApply,
    Accepted,
    ChangesRequested,
    Blocked,
    Failed,
    Cancelled,
    ApplyConflict,
}

impl DelegationStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Cancelled | Self::Failed)
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Planned
                | Self::AwaitingApproval
                | Self::WorkerRunning
                | Self::ReviewRunning
                | Self::ChangesRequested
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptRole {
    Worker,
    Reviewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationAttempt {
    pub attempt_id: String,
    pub role: AttemptRole,
    pub agent: String,
    pub started_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationArtifacts {
    pub worker_diff: String,
    pub worker_result: String,
    pub review_result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationJob {
    pub version: u32,
    pub job_id: String,
    pub project_root: String,
    pub parent_thread_id: String,
    pub card: FeatureCard,
    pub status: DelegationStatus,
    pub worker_attempt_id: Option<String>,
    pub reviewer_attempt_id: Option<String>,
    pub attempt: u32,
    pub attempts: Vec<DelegationAttempt>,
    pub base_workspace_snapshot: WorkspaceSnapshot,
    pub artifacts: DelegationArtifacts,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DelegationJob {
    pub fn transition(&mut self, next: DelegationStatus) -> Result<bool> {
        if self.status == next {
            return Ok(false);
        }
        if matches!(
            self.status,
            DelegationStatus::Accepted | DelegationStatus::Cancelled
        ) {
            return Err(HarnessError::Other(format!(
                "terminal delegation status {:?} cannot transition",
                self.status
            )));
        }
        let allowed = match (self.status, next) {
            (DelegationStatus::Planned, DelegationStatus::AwaitingApproval)
            | (DelegationStatus::AwaitingApproval, DelegationStatus::WorkerRunning)
            | (DelegationStatus::WorkerRunning, DelegationStatus::ReviewRunning)
            | (DelegationStatus::ReviewRunning, DelegationStatus::ReadyToApply)
            | (DelegationStatus::ReviewRunning, DelegationStatus::ChangesRequested)
            | (DelegationStatus::ChangesRequested, DelegationStatus::AwaitingApproval)
            | (DelegationStatus::Blocked, DelegationStatus::AwaitingApproval)
            | (DelegationStatus::Failed, DelegationStatus::AwaitingApproval)
            | (DelegationStatus::ApplyConflict, DelegationStatus::AwaitingApproval)
            | (DelegationStatus::ReadyToApply, DelegationStatus::Accepted)
            | (DelegationStatus::ReadyToApply, DelegationStatus::ApplyConflict)
            | (_, DelegationStatus::Blocked)
            | (_, DelegationStatus::Failed)
            | (_, DelegationStatus::Cancelled) => true,
            _ => false,
        };
        if !allowed {
            return Err(HarnessError::Other(format!(
                "invalid delegation transition {:?} -> {:?}",
                self.status, next
            )));
        }
        self.status = next;
        self.updated_at = now_millis();
        Ok(true)
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(clip_chars(&message.into(), 4_000));
        self.updated_at = now_millis();
    }

    pub fn start_attempt(&mut self, role: AttemptRole, agent: &str) -> Result<String> {
        if role == AttemptRole::Worker {
            self.attempt = self.attempt.saturating_add(1);
        }
        let prefix = match role {
            AttemptRole::Worker => "worker",
            AttemptRole::Reviewer => "reviewer",
        };
        let attempt_id = crate::thread::new_id(prefix);
        let attempt = DelegationAttempt {
            attempt_id: attempt_id.clone(),
            role: role.clone(),
            agent: agent.to_string(),
            started_at: now_millis(),
            finished_at: None,
        };
        if role == AttemptRole::Worker {
            self.worker_attempt_id = Some(attempt_id.clone());
        } else {
            self.reviewer_attempt_id = Some(attempt_id.clone());
        }
        self.attempts.push(attempt);
        self.updated_at = now_millis();
        Ok(attempt_id)
    }

    pub fn finish_attempt(&mut self, attempt_id: &str) {
        if let Some(attempt) = self
            .attempts
            .iter_mut()
            .find(|attempt| attempt.attempt_id == attempt_id)
        {
            attempt.finished_at = Some(now_millis());
        }
        self.updated_at = now_millis();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerResult {
    pub summary: String,
    #[serde(alias = "changed_files")]
    pub changed_files: Vec<String>,
    #[serde(alias = "checks_attempted")]
    pub checks_attempted: Vec<String>,
    pub blockers: Vec<String>,
}

impl WorkerResult {
    pub fn from_external(text: &str, diff: &str) -> Option<Self> {
        if let Ok(value) = serde_json::from_str::<Self>(text.trim()) {
            if !value.summary.trim().is_empty() {
                return Some(value);
            }
        }
        let summary = text.trim();
        if summary.is_empty() {
            return None;
        }
        Some(Self {
            summary: clip_chars(summary, 8_000),
            changed_files: diff_paths(diff),
            checks_attempted: Vec::new(),
            blockers: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Accepted,
    ChangesRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Blocking,
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCheckResult {
    pub command: String,
    pub status: CheckStatus,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReport {
    pub decision: ReviewDecision,
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
    pub checks: Vec<AcceptanceCheckResult>,
}

impl ReviewReport {
    pub fn parse(raw: &str, required_checks: &[String]) -> Result<Self> {
        let value = raw.trim();
        if value.chars().count() > MAX_REVIEW_REPORT_CHARS {
            return Err(HarnessError::Other("reviewer report is too large".into()));
        }
        let value = if value.starts_with("```") && value.ends_with("```") {
            value
                .lines()
                .skip(1)
                .take_while(|line| !line.trim_start().starts_with("```"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            value.to_string()
        };
        let report: Self = serde_json::from_str(value.trim()).map_err(|error| {
            HarnessError::Other(format!("reviewer returned malformed JSON: {error}"))
        })?;
        report.validate(required_checks)?;
        Ok(report)
    }

    pub fn validate(&self, required_checks: &[String]) -> Result<()> {
        if self.summary.trim().is_empty() || self.summary.chars().count() > 8_000 {
            return Err(HarnessError::Other(
                "review report summary is missing or too long".into(),
            ));
        }
        if self.findings.len() > MAX_REVIEW_FINDINGS {
            return Err(HarnessError::Other(
                "review report has too many findings".into(),
            ));
        }
        for finding in &self.findings {
            if finding.path.trim().is_empty() || finding.message.trim().is_empty() {
                return Err(HarnessError::Other("review finding is incomplete".into()));
            }
            if finding.message.chars().count() > 8_000 {
                return Err(HarnessError::Other("review finding is too long".into()));
            }
        }
        if self.checks.len() > MAX_REVIEW_CHECKS {
            return Err(HarnessError::Other(
                "review report has too many checks".into(),
            ));
        }
        for check in &self.checks {
            validate_check_command(&check.command)?;
            if check.output.chars().count() > MAX_REVIEW_OUTPUT_CHARS {
                return Err(HarnessError::Other(
                    "review check output is too long".into(),
                ));
            }
        }
        for required in required_checks {
            let Some(evidence) = self.checks.iter().find(|check| check.command == *required) else {
                return Err(HarnessError::Other(format!(
                    "review report is missing required check: {required}"
                )));
            };
            if evidence.output.trim().is_empty()
                || (self.decision == ReviewDecision::Accepted
                    && evidence.status != CheckStatus::Passed)
            {
                return Err(HarnessError::Other(format!(
                    "required check lacks evidence: {required}"
                )));
            }
        }
        Ok(())
    }

    pub fn can_accept(&self, required_checks: &[String]) -> bool {
        self.validate(required_checks).is_ok()
            && self.decision == ReviewDecision::Accepted
            && !self
                .findings
                .iter()
                .any(|finding| finding.severity == ReviewSeverity::Blocking)
    }
}

pub fn validate_review_paths(root: &Path, report: &ReviewReport) -> Result<()> {
    for finding in &report.findings {
        let relative = safe_relative_path(&finding.path, "review finding")?;
        let candidate = root.join(relative);
        let existing = nearest_existing(candidate.parent().unwrap_or(root));
        let root = fs::canonicalize(root)
            .map_err(|error| HarnessError::Other(format!("resolve project root: {error}")))?;
        if !existing.starts_with(root) {
            return Err(HarnessError::Other(format!(
                "review finding path escapes the project: {}",
                finding.path
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct DelegationStore {
    root: PathBuf,
}

impl DelegationStore {
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self> {
        let root = project_root.as_ref().to_path_buf();
        fs::create_dir_all(root.join(".zest").join("delegations"))
            .map_err(|error| HarnessError::Other(format!("create delegation store: {error}")))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir(&self) -> PathBuf {
        self.root.join(".zest").join("delegations")
    }

    fn job_path(&self, job_id: &str) -> Result<PathBuf> {
        Ok(self
            .dir()
            .join(format!("{}.json", validate_id(job_id, "delegation job")?)))
    }

    fn artifact_path(&self, job_id: &str, name: &str) -> Result<PathBuf> {
        let job_id = validate_id(job_id, "delegation job")?;
        let name = match name {
            "worker.diff" | "worker-result.json" | "review-result.json" => name,
            _ => return Err(HarnessError::Other("unknown delegation artifact".into())),
        };
        let path = self.dir().join(job_id).join(name);
        Ok(path)
    }

    pub fn load(&self, job_id: &str) -> Result<Option<DelegationJob>> {
        let job_id = validate_id(job_id, "delegation job")?;
        read_json(&self.job_path(&job_id)?, "delegation job", &job_id)
    }

    pub fn save(&self, job: &DelegationJob) -> Result<()> {
        validate_id(&job.job_id, "delegation job")?;
        if job.version != DELEGATION_FORMAT_VERSION {
            return Err(HarnessError::Other(
                "unsupported delegation job version".into(),
            ));
        }
        let _guard = state_lock()?;
        write_json(
            &self.job_path(&job.job_id)?,
            "delegation job",
            &job.job_id,
            job,
        )
    }

    pub fn create(
        &self,
        parent_thread_id: &str,
        mut card: FeatureCard,
        snapshot: WorkspaceSnapshot,
    ) -> Result<DelegationJob> {
        validate_id(parent_thread_id, "parent thread")?;
        card.validate(&self.root, &BTreeMap::new())
            .or_else(|error| {
                // The caller normally validates with the configured agent map. The
                // store still validates all local invariants without requiring a
                // second copy of config, so only the agent lookup error is ignored.
                if error.to_string().contains("external agent") {
                    Ok(())
                } else {
                    Err(error)
                }
            })?;
        let existing = self.list()?;
        for dependency in &card.depends_on {
            if !existing.iter().any(|job| &job.job_id == dependency) {
                return Err(HarnessError::Other(format!(
                    "feature card dependency {dependency} does not exist"
                )));
            }
        }
        let job_id = crate::thread::new_id("job");
        card.card_id = if card.card_id.trim().is_empty() {
            crate::thread::new_id("card")
        } else {
            card.card_id
        };
        let now = now_millis();
        let job = DelegationJob {
            version: DELEGATION_FORMAT_VERSION,
            job_id: job_id.clone(),
            project_root: self.root.to_string_lossy().into_owned(),
            parent_thread_id: parent_thread_id.to_string(),
            card,
            status: DelegationStatus::AwaitingApproval,
            worker_attempt_id: None,
            reviewer_attempt_id: None,
            attempt: 0,
            attempts: Vec::new(),
            base_workspace_snapshot: snapshot,
            artifacts: DelegationArtifacts {
                worker_diff: format!(".zest/delegations/{job_id}/worker.diff"),
                worker_result: format!(".zest/delegations/{job_id}/worker-result.json"),
                review_result: format!(".zest/delegations/{job_id}/review-result.json"),
            },
            created_at: now,
            updated_at: now,
            error: None,
        };
        if self.load(&job_id)?.is_some() {
            return Err(HarnessError::Other(
                "generated duplicate delegation job id".into(),
            ));
        }
        self.save(&job)?;
        Ok(job)
    }

    pub fn list(&self) -> Result<Vec<DelegationJob>> {
        let _guard = state_lock()?;
        let mut jobs: Vec<DelegationJob> = Vec::new();
        let entries = fs::read_dir(self.dir())
            .map_err(|error| HarnessError::Other(format!("list delegation jobs: {error}")))?;
        for entry in entries {
            let entry = entry.map_err(|error| HarnessError::Other(error.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Some(job) = read_json(&path, "delegation job", id)? {
                jobs.push(job);
            }
        }
        jobs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.job_id.cmp(&right.job_id))
        });
        Ok(jobs)
    }

    pub fn write_artifact(&self, job_id: &str, name: &str, data: &[u8]) -> Result<()> {
        let path = self.artifact_path(job_id, name)?;
        fsutil::atomic_write(&path, data).map_err(|error| {
            HarnessError::Other(format!(
                "write delegation artifact {}: {error}",
                path.display()
            ))
        })
    }

    pub fn read_artifact(&self, job_id: &str, name: &str) -> Result<Vec<u8>> {
        let path = self.artifact_path(job_id, name)?;
        fs::read(&path).map_err(|error| {
            HarnessError::Other(format!(
                "read delegation artifact {}: {error}",
                path.display()
            ))
        })
    }

    pub fn reconcile_after_restart(&self) -> Result<Vec<DelegationJob>> {
        let mut changed = Vec::new();
        for mut job in self.list()? {
            if !matches!(
                job.status,
                DelegationStatus::WorkerRunning | DelegationStatus::ReviewRunning
            ) {
                continue;
            }
            job.transition(DelegationStatus::Blocked)?;
            job.set_error("The app restarted while this job was running. Review the artifacts and start a fresh attempt.");
            self.save(&job)?;
            changed.push(job);
        }
        Ok(changed)
    }

    pub fn transition(
        &self,
        job_id: &str,
        next: DelegationStatus,
    ) -> Result<Option<DelegationJob>> {
        let Some(mut job) = self.load(job_id)? else {
            return Ok(None);
        };
        job.transition(next)?;
        self.save(&job)?;
        Ok(Some(job))
    }

    pub fn update(&self, mut job: DelegationJob) -> Result<DelegationJob> {
        job.updated_at = now_millis();
        self.save(&job)?;
        Ok(job)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, kind: &str, id: &str) -> Result<Option<T>> {
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(HarnessError::Other(format!(
                "read {kind} {id} {}: {error}",
                path.display()
            )))
        }
    };
    serde_json::from_str(&body).map(Some).map_err(|error| {
        HarnessError::Other(format!(
            "{kind} {id} is corrupt at {}: {error}",
            path.display()
        ))
    })
}

fn write_json<T: Serialize>(path: &Path, kind: &str, id: &str, value: &T) -> Result<()> {
    fsutil::atomic_write_json(path, value).map_err(|error| {
        HarnessError::Other(format!("write {kind} {id} {}: {error}", path.display()))
    })
}

/// Capture only stable checkout metadata. This is synchronous because it is
/// used at the tool boundary and by the explicit apply command, never inside
/// an external worker stream.
pub fn capture_workspace_snapshot(root: &Path) -> WorkspaceSnapshot {
    let head = git_output(root, &["rev-parse", "HEAD"]);
    let status =
        git_bytes(root, &["status", "--porcelain=v1", "--untracked-files=all"]).unwrap_or_default();
    let tracked = git_bytes(root, &["diff", "--binary", "HEAD", "--"]).unwrap_or_default();
    let untracked =
        git_bytes(root, &["ls-files", "--others", "--exclude-standard", "-z"]).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    hasher.update(head.as_deref().unwrap_or("none").as_bytes());
    hasher.update(&status);
    hasher.update(&tracked);
    for raw in untracked.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let relative = String::from_utf8_lossy(raw).replace('\\', "/");
        if relative == ".zest" || relative.starts_with(".zest/") || is_sensitive_path(&relative) {
            continue;
        }
        hasher.update(relative.as_bytes());
        if let Ok(content) = fs::read(root.join(&relative)) {
            hasher.update(&content);
        }
    }
    WorkspaceSnapshot {
        head,
        fingerprint: hasher.finalize().to_hex().to_string(),
        captured_at: now_millis(),
    }
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn git_bytes(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

pub fn diff_paths(diff: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut add = |path: &str| {
        if path == "/dev/null" || path == "dev/null" || path == "NUL" {
            return;
        }
        if !path.starts_with("/dev/") && !paths.iter().any(|item| item == path) {
            paths.push(path.to_string());
        }
    };
    for line in diff.lines() {
        if let Some(path) = line
            .strip_prefix("+++ b/")
            .or_else(|| line.strip_prefix("--- a/"))
        {
            add(path);
            continue;
        }
        if let Some(paths_in_header) = line.strip_prefix("diff --git a/") {
            if let Some((from, to)) = paths_in_header.split_once(" b/") {
                add(from);
                add(to);
            }
        }
    }
    paths
}

pub fn validate_diff_paths(root: &Path, diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        return Err(HarnessError::Other("delegation diff is empty".into()));
    }
    let paths = diff_paths(diff);
    if paths.is_empty() {
        return Err(HarnessError::Other(
            "delegation diff contains no file paths".into(),
        ));
    }
    let root = fs::canonicalize(root)
        .map_err(|error| HarnessError::Other(format!("resolve project root: {error}")))?;
    for path in paths {
        let relative = safe_relative_path(&path, "diff")?;
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if relative_text == ".zest"
            || relative_text.starts_with(".zest/")
            || is_sensitive_path(&relative_text)
        {
            return Err(HarnessError::Other(format!(
                "delegation diff touches a protected path: {relative_text}"
            )));
        }
        let candidate = root.join(relative);
        if let Some(parent) = candidate.parent() {
            let existing = nearest_existing(parent);
            if !existing.starts_with(&root) {
                return Err(HarnessError::Other(
                    "delegation diff path escapes the project".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Validate both the project boundary and the feature card's declared edit
/// scope. A scope entry may name a file, a directory, or `.` for the project
/// root; path checks are lexical first and symlink-aware for existing parents.
pub fn validate_diff_scope(root: &Path, diff: &str, scope: &[String]) -> Result<()> {
    validate_diff_paths(root, diff)?;
    if scope.is_empty() {
        return Err(HarnessError::Other(
            "delegation feature scope must not be empty".into(),
        ));
    }
    let allowed = scope
        .iter()
        .map(|raw| safe_relative_path(raw, "scope"))
        .collect::<Result<Vec<_>>>()?;
    for path in diff_paths(diff) {
        let relative = safe_relative_path(&path, "diff")?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        let in_scope = allowed.iter().any(|scope| {
            let scope = scope.to_string_lossy().replace('\\', "/");
            scope == "." || relative == scope || relative.starts_with(&format!("{scope}/"))
        });
        if !in_scope {
            return Err(HarnessError::Other(format!(
                "delegation diff path is outside the feature scope: {relative}"
            )));
        }
    }
    Ok(())
}

fn nearest_existing(path: &Path) -> PathBuf {
    let mut cursor = path.to_path_buf();
    while !cursor.exists() {
        if !cursor.pop() {
            break;
        }
    }
    fs::canonicalize(&cursor).unwrap_or(cursor)
}

/// Apply an accepted diff without ever asking Git to accept partial hunks.
/// The caller should persist the returned job after this function succeeds.
pub fn apply_diff_checked(root: &Path, diff: &str) -> Result<()> {
    validate_diff_paths(root, diff)?;
    let _guard = apply_lock()?;
    let check = Command::new("git")
        .args(["apply", "--check", "--binary", "--whitespace=nowarn", "-"])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| HarnessError::Other(format!("start git apply --check: {error}")))?;
    apply_to_child(check, diff, "check")?;

    let apply = Command::new("git")
        .args(["apply", "--binary", "--whitespace=nowarn", "-"])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| HarnessError::Other(format!("start git apply: {error}")))?;
    apply_to_child(apply, diff, "apply")
}

fn apply_to_child(mut child: std::process::Child, diff: &str, phase: &str) -> Result<()> {
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(diff.as_bytes()).map_err(|error| {
            HarnessError::Other(format!("write git apply {phase} input: {error}"))
        })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| HarnessError::Other(format!("wait for git apply {phase}: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(HarnessError::Other(format!(
            "git apply {phase} failed{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("zest-delegation-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn card(_root: &Path) -> FeatureCard {
        FeatureCard {
            version: DELEGATION_FORMAT_VERSION,
            card_id: "card-1".into(),
            title: "Add feature".into(),
            objective: "Implement the feature".into(),
            lane: "core".into(),
            scope: vec!["src".into()],
            context: vec![],
            depends_on: vec![],
            agent: "worker".into(),
            acceptance_checks: vec!["cargo test -p zest-core".into()],
            review_required: true,
            created_at: now_millis(),
        }
    }

    fn agent_config() -> BTreeMap<String, ExternalAgentConfig> {
        let mut agents = BTreeMap::new();
        agents.insert(
            "worker".into(),
            ExternalAgentConfig {
                mode: crate::config::ExternalAgentMode::Headless,
                command: "worker".into(),
                args: vec!["{prompt}".into()],
                allow_mcp: false,
                model: None,
                workspace: ExternalWorkspace::Isolated,
                timeout_secs: 10,
            },
        );
        agents
    }

    #[test]
    fn feature_card_rejects_escape_and_invalid_worker() {
        let root = scratch("card-validation");
        let mut value = card(&root);
        value.scope = vec!["../outside".into()];
        assert!(value.validate(&root, &agent_config()).is_err());
        value.scope = vec!["src".into()];
        value.agent = "missing".into();
        assert!(value.validate(&root, &agent_config()).is_err());
    }

    #[test]
    fn transitions_are_idempotent_and_reject_skips() {
        let root = scratch("transitions");
        let card = card(&root);
        let now = now_millis();
        let mut job = DelegationJob {
            version: 1,
            job_id: "job-1".into(),
            project_root: root.to_string_lossy().into_owned(),
            parent_thread_id: "thread-1".into(),
            card,
            status: DelegationStatus::Planned,
            worker_attempt_id: None,
            reviewer_attempt_id: None,
            attempt: 0,
            attempts: vec![],
            base_workspace_snapshot: WorkspaceSnapshot {
                head: None,
                fingerprint: "x".into(),
                captured_at: now,
            },
            artifacts: DelegationArtifacts {
                worker_diff: "delegations/job-1/worker.diff".into(),
                worker_result: "delegations/job-1/worker-result.json".into(),
                review_result: "delegations/job-1/review-result.json".into(),
            },
            created_at: now,
            updated_at: now,
            error: None,
        };
        assert!(job.transition(DelegationStatus::AwaitingApproval).unwrap());
        assert!(!job.transition(DelegationStatus::AwaitingApproval).unwrap());
        assert!(job.transition(DelegationStatus::WorkerRunning).unwrap());
        assert!(job.transition(DelegationStatus::ReviewRunning).unwrap());
        assert!(job.transition(DelegationStatus::ReadyToApply).unwrap());
        assert!(job.transition(DelegationStatus::Accepted).unwrap());
        assert!(job.transition(DelegationStatus::WorkerRunning).is_err());
        assert!(job.transition(DelegationStatus::Cancelled).is_err());
    }

    #[test]
    fn review_requires_every_check_and_blocks_blocking_findings() {
        let checks = vec!["cargo test".into()];
        let raw = r#"{
            "decision":"accepted",
            "summary":"Looks good",
            "findings":[],
            "checks":[{"command":"cargo test","status":"passed","output":"ok"}]
        }"#;
        let report = ReviewReport::parse(raw, &checks).unwrap();
        assert!(report.can_accept(&checks));

        let missing = r#"{"decision":"accepted","summary":"Looks good","findings":[],"checks":[]}"#;
        assert!(ReviewReport::parse(missing, &checks).is_err());

        let malformed = "not json";
        assert!(ReviewReport::parse(malformed, &checks).is_err());

        let blocking = r#"{
            "decision":"accepted",
            "summary":"Needs one fix",
            "findings":[{"severity":"blocking","path":"src/a.rs","message":"Missing guard"}],
            "checks":[{"command":"cargo test","status":"passed","output":"ok"}]
        }"#;
        assert!(!ReviewReport::parse(blocking, &checks)
            .unwrap()
            .can_accept(&checks));
        let report = ReviewReport {
            decision: ReviewDecision::ChangesRequested,
            summary: "Needs work".into(),
            findings: vec![ReviewFinding {
                severity: ReviewSeverity::Blocking,
                path: "../outside.rs".into(),
                message: "bad path".into(),
            }],
            checks: vec![],
        };
        assert!(validate_review_paths(&scratch("review-paths"), &report).is_err());
    }

    #[test]
    fn changes_requested_creates_a_fresh_worker_attempt() {
        let root = scratch("fresh-attempt");
        let now = now_millis();
        let mut job = DelegationJob {
            version: DELEGATION_FORMAT_VERSION,
            job_id: "job-fresh".into(),
            project_root: root.to_string_lossy().into_owned(),
            parent_thread_id: "thread-1".into(),
            card: card(&root),
            status: DelegationStatus::AwaitingApproval,
            worker_attempt_id: None,
            reviewer_attempt_id: None,
            attempt: 0,
            attempts: vec![],
            base_workspace_snapshot: WorkspaceSnapshot {
                head: None,
                fingerprint: "x".into(),
                captured_at: now,
            },
            artifacts: DelegationArtifacts {
                worker_diff: "delegations/job-fresh/worker.diff".into(),
                worker_result: "delegations/job-fresh/worker-result.json".into(),
                review_result: "delegations/job-fresh/review-result.json".into(),
            },
            created_at: now,
            updated_at: now,
            error: None,
        };
        job.transition(DelegationStatus::WorkerRunning).unwrap();
        let first = job.start_attempt(AttemptRole::Worker, "worker").unwrap();
        assert_eq!(job.attempt, 1);
        job.transition(DelegationStatus::ReviewRunning).unwrap();
        job.transition(DelegationStatus::ChangesRequested).unwrap();
        job.transition(DelegationStatus::AwaitingApproval).unwrap();
        job.transition(DelegationStatus::WorkerRunning).unwrap();
        // Attempt IDs are generated for each worker invocation, and a retry
        // starts from a new queued state after the reviewer loop.
        let second = job.start_attempt(AttemptRole::Worker, "worker").unwrap();
        assert_ne!(first, second);
        assert_eq!(job.attempt, 2);
    }

    #[test]
    fn store_round_trips_and_reconciles_active_jobs() {
        let root = scratch("store");
        let store = DelegationStore::open(&root).unwrap();
        let feature = card(&root);
        feature.validate(&root, &agent_config()).unwrap();
        let job = store
            .create("thread-1", feature, capture_workspace_snapshot(&root))
            .unwrap();
        assert_eq!(store.load(&job.job_id).unwrap().unwrap().job_id, job.job_id);
        let mut running = job;
        running.transition(DelegationStatus::WorkerRunning).unwrap();
        store.save(&running).unwrap();
        let reconciled = store.reconcile_after_restart().unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(
            store.load(&running.job_id).unwrap().unwrap().status,
            DelegationStatus::Blocked
        );
    }

    #[test]
    fn worker_result_fallback_and_diff_paths_are_bounded() {
        let result = WorkerResult::from_external(
            "implemented it",
            "diff --git a/src/a.rs b/src/a.rs\n+++ b/src/a.rs\n",
        )
        .unwrap();
        assert_eq!(result.changed_files, vec![String::from("src/a.rs")]);
        assert_eq!(
            diff_paths("diff --git a/assets/icon.png b/assets/icon.png\nBinary files differ"),
            vec![String::from("assets/icon.png")]
        );
    }

    #[test]
    fn diff_scope_rejects_paths_outside_the_card() {
        let root = scratch("diff-scope");
        fs::create_dir_all(root.join("src")).unwrap();
        let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n";
        validate_diff_scope(&root, diff, &["src".into()]).unwrap();
        let outside = "diff --git a/docs/a.md b/docs/a.md\n--- a/docs/a.md\n+++ b/docs/a.md\n";
        assert!(validate_diff_scope(&root, outside, &["src".into()]).is_err());
    }

    #[test]
    fn serialized_old_job_defaults_optional_error() {
        let value = serde_json::json!({
            "version": 1,
            "jobId": "job-1",
            "projectRoot": ".",
            "parentThreadId": "thread-1",
            "card": serde_json::json!({
                "version": 1, "cardId": "card-1", "title": "x", "objective": "y",
                "lane": "z", "scope": ["src"], "context": [], "dependsOn": [],
                "agent": "worker", "acceptanceChecks": [], "reviewRequired": true, "createdAt": 0
            }),
            "status": "planned",
            "workerAttemptId": null,
            "reviewerAttemptId": null,
            "attempt": 0,
            "attempts": [],
            "baseWorkspaceSnapshot": {"head": null, "fingerprint": "x", "capturedAt": 0},
            "artifacts": {"workerDiff":"a", "workerResult":"b", "reviewResult":"c"},
            "createdAt": 0,
            "updatedAt": 0
        });
        let job: DelegationJob = serde_json::from_value(value).unwrap();
        assert!(job.error.is_none());
    }

    #[test]
    fn store_rejects_missing_dependencies() {
        let root = scratch("missing-dependency");
        let store = DelegationStore::open(&root).unwrap();
        let mut feature = card(&root);
        feature.depends_on = vec!["job-does-not-exist".into()];
        assert!(store
            .create("thread-1", feature, capture_workspace_snapshot(&root))
            .is_err());
    }

    #[test]
    fn apply_checks_are_non_destructive_on_conflict() {
        let root = scratch("safe-apply");
        run_git(&root, &["init", "--quiet"]);
        run_git(&root, &["config", "user.name", "Zest Test"]);
        run_git(&root, &["config", "user.email", "zest-test@localhost"]);
        fs::write(root.join("file.txt"), "before\n").unwrap();
        run_git(&root, &["add", "file.txt"]);
        run_git(&root, &["commit", "--quiet", "-m", "baseline"]);
        fs::write(root.join("file.txt"), "worker\n").unwrap();
        let diff = String::from_utf8(run_git_output(&root, &["diff", "--binary"]).stdout).unwrap();
        fs::write(root.join("file.txt"), "before\n").unwrap();
        apply_diff_checked(&root, &diff).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("file.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "worker\n"
        );

        fs::write(root.join("file.txt"), "conflicting\n").unwrap();
        let before = fs::read_to_string(root.join("file.txt")).unwrap();
        assert!(apply_diff_checked(&root, &diff).is_err());
        assert_eq!(fs::read_to_string(root.join("file.txt")).unwrap(), before);
    }

    fn run_git(root: &Path, args: &[&str]) {
        let output = run_git_output(root, args);
        assert!(output.status.success(), "git {:?}: {:?}", args, output);
    }

    fn run_git_output(root: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap()
    }
}
