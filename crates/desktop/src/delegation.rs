//! Desktop coordinator for durable feature-card jobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
#[cfg(feature = "export-bindings")]
use ts_rs::TS;
use zest_core::{
    apply_diff_checked, capture_workspace_snapshot, diff_paths, run_delegation_reviewer,
    run_delegation_worker, validate_diff_scope, AttemptRole, CheckStatus, Config, DelegationJob,
    DelegationStatus as CoreDelegationStatus, DelegationStore, ReviewReport,
    ReviewSeverity as CoreReviewSeverity, WorkerResult,
};

const MAX_ACTIVE_WORKER_JOBS: usize = 2;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "DelegationStatus.ts", rename_all = "snake_case")
)]
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

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ReviewSeverity.ts", rename_all = "snake_case")
)]
pub enum ReviewSeverity {
    Blocking,
    Advisory,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(
        export,
        export_to = "AcceptanceCheckStatus.ts",
        rename_all = "snake_case"
    )
)]
pub enum AcceptanceCheckStatus {
    Pending,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "ReviewFinding.ts", rename_all = "camelCase")
)]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "AcceptanceCheckView.ts", rename_all = "camelCase")
)]
pub struct AcceptanceCheckView {
    pub command: String,
    pub status: AcceptanceCheckStatus,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "DelegationJobView.ts", rename_all = "camelCase")
)]
pub struct DelegationJobView {
    pub job_id: String,
    pub parent_thread_id: String,
    pub project_root: String,
    pub card_id: String,
    pub title: String,
    pub objective: String,
    pub lane: String,
    pub scope: Vec<String>,
    pub context: Vec<String>,
    pub depends_on: Vec<String>,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export-bindings", ts(optional))]
    pub worker_attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export-bindings", ts(optional))]
    pub reviewer_attempt_id: Option<String>,
    pub reviewer_agent: String,
    pub attempt: u32,
    pub status: DelegationStatus,
    pub changed_files: Vec<String>,
    pub changed_file_count: usize,
    pub acceptance_checks: Vec<AcceptanceCheckView>,
    pub reviewer_findings: Vec<ReviewFinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export-bindings", ts(optional))]
    pub worker_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export-bindings", ts(optional))]
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(
    feature = "export-bindings",
    ts(export, export_to = "DelegationEvent.ts", rename_all = "snake_case")
)]
pub enum DelegationEvent {
    CardCreated { job: DelegationJobView },
    ApprovalRequired { job: DelegationJobView },
    WorkerStarted { job: DelegationJobView },
    WorkerCompleted { job: DelegationJobView },
    ReviewerStarted { job: DelegationJobView },
    ReviewerCompleted { job: DelegationJobView },
    ChangesRequested { job: DelegationJobView },
    ReadyToApply { job: DelegationJobView },
    Applied { job: DelegationJobView },
    Conflict { job: DelegationJobView },
    Blocked { job: DelegationJobView },
    Failed { job: DelegationJobView },
    Cancelled { job: DelegationJobView },
}

#[derive(Debug, Clone, Copy)]
enum EventKind {
    CardCreated,
    ApprovalRequired,
    WorkerStarted,
    WorkerCompleted,
    ReviewerStarted,
    ReviewerCompleted,
    ChangesRequested,
    ReadyToApply,
    Applied,
    Conflict,
    Blocked,
    Failed,
    Cancelled,
}

impl From<CoreDelegationStatus> for DelegationStatus {
    fn from(status: CoreDelegationStatus) -> Self {
        match status {
            CoreDelegationStatus::Planned => Self::Planned,
            CoreDelegationStatus::AwaitingApproval => Self::AwaitingApproval,
            CoreDelegationStatus::WorkerRunning => Self::WorkerRunning,
            CoreDelegationStatus::ReviewRunning => Self::ReviewRunning,
            CoreDelegationStatus::ReadyToApply => Self::ReadyToApply,
            CoreDelegationStatus::Accepted => Self::Accepted,
            CoreDelegationStatus::ChangesRequested => Self::ChangesRequested,
            CoreDelegationStatus::Blocked => Self::Blocked,
            CoreDelegationStatus::Failed => Self::Failed,
            CoreDelegationStatus::Cancelled => Self::Cancelled,
            CoreDelegationStatus::ApplyConflict => Self::ApplyConflict,
        }
    }
}

impl From<CoreReviewSeverity> for ReviewSeverity {
    fn from(severity: CoreReviewSeverity) -> Self {
        match severity {
            CoreReviewSeverity::Blocking => Self::Blocking,
            CoreReviewSeverity::Advisory => Self::Advisory,
        }
    }
}

impl From<CheckStatus> for AcceptanceCheckStatus {
    fn from(status: CheckStatus) -> Self {
        match status {
            CheckStatus::Passed => Self::Passed,
            CheckStatus::Failed => Self::Failed,
            CheckStatus::Skipped => Self::Skipped,
        }
    }
}

fn report_from_store(store: &DelegationStore, job: &DelegationJob) -> Option<ReviewReport> {
    let bytes = store
        .read_artifact(&job.job_id, "review-result.json")
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn worker_result_from_store(store: &DelegationStore, job: &DelegationJob) -> Option<WorkerResult> {
    let bytes = store
        .read_artifact(&job.job_id, "worker-result.json")
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn job_view(store: &DelegationStore, job: &DelegationJob) -> DelegationJobView {
    let diff = store
        .read_artifact(&job.job_id, "worker.diff")
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let changed_files = diff_paths(&diff);
    let report = report_from_store(store, job);
    let acceptance_checks = job
        .card
        .acceptance_checks
        .iter()
        .map(|command| {
            report
                .as_ref()
                .and_then(|report| report.checks.iter().find(|check| check.command == *command))
                .map(|check| AcceptanceCheckView {
                    command: check.command.clone(),
                    status: check.status.into(),
                    output: check.output.clone(),
                })
                .unwrap_or_else(|| AcceptanceCheckView {
                    command: command.clone(),
                    status: AcceptanceCheckStatus::Pending,
                    output: String::new(),
                })
        })
        .collect::<Vec<_>>();
    let reviewer_findings = report
        .map(|report| {
            report
                .findings
                .into_iter()
                .map(|finding| ReviewFinding {
                    severity: finding.severity.into(),
                    path: finding.path,
                    message: finding.message,
                })
                .collect()
        })
        .unwrap_or_default();
    let worker_summary = worker_result_from_store(store, job).map(|result| result.summary);
    DelegationJobView {
        job_id: job.job_id.clone(),
        parent_thread_id: job.parent_thread_id.clone(),
        project_root: job.project_root.clone(),
        card_id: job.card.card_id.clone(),
        title: job.card.title.clone(),
        objective: job.card.objective.clone(),
        lane: job.card.lane.clone(),
        scope: job.card.scope.clone(),
        context: job.card.context.clone(),
        depends_on: job.card.depends_on.clone(),
        agent: job.card.agent.clone(),
        worker_attempt_id: job.worker_attempt_id.clone(),
        reviewer_attempt_id: job.reviewer_attempt_id.clone(),
        reviewer_agent: job.card.agent.clone(),
        attempt: job.attempt,
        status: job.status.into(),
        changed_file_count: changed_files.len(),
        changed_files,
        acceptance_checks,
        reviewer_findings,
        worker_summary,
        error: job.error.clone(),
        created_at: job.created_at,
        updated_at: job.updated_at,
    }
}

pub struct DelegationCoordinator {
    lanes: Arc<Semaphore>,
    running: Mutex<HashMap<String, Arc<zest_core::CancelToken>>>,
}

impl DelegationCoordinator {
    pub fn new() -> Self {
        Self {
            lanes: Arc::new(Semaphore::new(MAX_ACTIVE_WORKER_JOBS)),
            running: Mutex::new(HashMap::new()),
        }
    }

    pub fn enqueue(self: &Arc<Self>, app: AppHandle, root: PathBuf, job_id: String) {
        let cancel = Arc::new(zest_core::CancelToken::new());
        let inserted = self
            .running
            .lock()
            .map(|mut running| {
                if running.contains_key(&job_id) {
                    false
                } else {
                    running.insert(job_id.clone(), cancel.clone());
                    true
                }
            })
            .unwrap_or(false);
        if !inserted {
            return;
        }
        if let Ok(store) = DelegationStore::open(&root) {
            if let Ok(Some(job)) = store.load(&job_id) {
                self.emit(&app, &store, &job, EventKind::CardCreated);
            }
        }
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let permit = coordinator.lanes.clone().acquire_owned().await;
            let result = match permit {
                Ok(_permit) => {
                    coordinator
                        .run_job(&app, &root, &job_id, cancel.as_ref())
                        .await
                }
                Err(error) => Err(format!("delegation scheduler stopped: {error}")),
            };
            if let Err(error) = result {
                let _ = coordinator.fail(&app, &root, &job_id, &error, EventKind::Failed);
            }
            if let Ok(mut running) = coordinator.running.lock() {
                if running
                    .get(&job_id)
                    .is_some_and(|current| Arc::ptr_eq(current, &cancel))
                {
                    running.remove(&job_id);
                }
            }
        });
    }

    pub fn cancel(&self, app: &AppHandle, root: &Path, job_id: &str) -> ResultView {
        if let Ok(running) = self.running.lock() {
            if let Some(cancel) = running.get(job_id) {
                cancel.cancel();
            }
        }
        let store = match DelegationStore::open(root) {
            Ok(store) => store,
            Err(error) => return Err(error.to_string()),
        };
        let mut job = match store.load(job_id) {
            Ok(Some(job)) => job,
            Ok(None) => return Err("delegation job was not found".into()),
            Err(error) => return Err(error.to_string()),
        };
        if !job.status.is_terminal() {
            if let Err(error) = job.transition(CoreDelegationStatus::Cancelled) {
                return Err(error.to_string());
            }
            if let Err(error) = store.update(job.clone()) {
                return Err(error.to_string());
            }
            self.emit(app, &store, &job, EventKind::Cancelled);
        }
        Ok(job_view(&store, &job))
    }

    pub fn retry(self: &Arc<Self>, app: &AppHandle, root: &Path, job_id: &str) -> ResultView {
        let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
        let mut job = store
            .load(job_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "delegation job was not found".to_string())?;
        if matches!(
            job.status,
            CoreDelegationStatus::ReadyToApply | CoreDelegationStatus::Accepted
        ) {
            return Err(
                "accepted changes must be applied or left untouched before retrying".into(),
            );
        }
        if job.status != CoreDelegationStatus::AwaitingApproval {
            job.transition(CoreDelegationStatus::AwaitingApproval)
                .map_err(|error| error.to_string())?;
        }
        job.base_workspace_snapshot = capture_workspace_snapshot(root);
        job.error = None;
        store
            .update(job.clone())
            .map_err(|error| error.to_string())?;
        self.emit(app, &store, &job, EventKind::ApprovalRequired);
        self.enqueue(app.clone(), root.to_path_buf(), job.job_id.clone());
        Ok(job_view(&store, &job))
    }

    pub fn reconcile(
        self: &Arc<Self>,
        app: &AppHandle,
        root: &Path,
    ) -> Result<Vec<DelegationJobView>, String> {
        let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
        let live_jobs = self
            .running
            .lock()
            .map(|running| {
                running
                    .keys()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let mut changed = Vec::new();
        for mut job in store.list().map_err(|error| error.to_string())? {
            let interrupted = matches!(
                job.status,
                CoreDelegationStatus::WorkerRunning | CoreDelegationStatus::ReviewRunning
            ) && !live_jobs.contains(&job.job_id);
            if !interrupted {
                continue;
            }
            job.transition(CoreDelegationStatus::Blocked)
                .map_err(|error| error.to_string())?;
            job.set_error("external delegation process was interrupted; review and retry");
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            changed.push(job);
        }
        let views = changed
            .iter()
            .map(|job| {
                self.emit(app, &store, job, EventKind::Blocked);
                job_view(&store, job)
            })
            .collect();
        self.kick_pending(app, root, &store);
        Ok(views)
    }

    pub fn apply(self: &Arc<Self>, app: &AppHandle, root: &Path, job_id: &str) -> ResultView {
        let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
        let mut job = store
            .load(job_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "delegation job was not found".to_string())?;
        if job.status != CoreDelegationStatus::ReadyToApply {
            return Err("only reviewed changes are ready to apply".into());
        }
        let diff = String::from_utf8(
            store
                .read_artifact(job_id, "worker.diff")
                .map_err(|error| error.to_string())?,
        )
        .map_err(|_| "worker diff is not valid UTF-8".to_string())?;
        if let Err(error) = validate_diff_scope(root, &diff, &job.card.scope)
            .and_then(|()| apply_diff_checked(root, &diff))
        {
            let _ = job.transition(CoreDelegationStatus::ApplyConflict);
            job.set_error(error.to_string());
            store.update(job.clone()).map_err(|save| save.to_string())?;
            self.emit(app, &store, &job, EventKind::Conflict);
            return Ok(job_view(&store, &job));
        }
        job.transition(CoreDelegationStatus::Accepted)
            .map_err(|error| error.to_string())?;
        job.error = None;
        store
            .update(job.clone())
            .map_err(|error| error.to_string())?;
        self.emit(app, &store, &job, EventKind::Applied);
        self.kick_pending(app, root, &store);
        Ok(job_view(&store, &job))
    }

    fn kick_pending(self: &Arc<Self>, app: &AppHandle, root: &Path, store: &DelegationStore) {
        let Ok(jobs) = store.list() else { return };
        for candidate in &jobs {
            if candidate.status != CoreDelegationStatus::AwaitingApproval {
                continue;
            }
            let ready = candidate.card.depends_on.iter().all(|dependency| {
                jobs.iter()
                    .find(|other| &other.job_id == dependency)
                    .is_some_and(|other| other.status == CoreDelegationStatus::Accepted)
            });
            if ready {
                // This is only a queue wake-up for a card whose initial
                // approval already happened; a fresh fix still comes through
                // `retry`, which is the explicit approval action.
                self.enqueue(app.clone(), root.to_path_buf(), candidate.job_id.clone());
            }
        }
    }

    async fn run_job(
        &self,
        app: &AppHandle,
        root: &Path,
        job_id: &str,
        cancel: &zest_core::CancelToken,
    ) -> Result<(), String> {
        let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
        let mut job = store
            .load(job_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "delegation job disappeared".to_string())?;
        if job.status != CoreDelegationStatus::AwaitingApproval {
            return Ok(());
        }
        let dependencies = store.list().map_err(|error| error.to_string())?;
        if job.card.depends_on.iter().any(|dependency| {
            dependencies
                .iter()
                .find(|candidate| &candidate.job_id == dependency)
                .is_some_and(|candidate| candidate.status != CoreDelegationStatus::Accepted)
        }) {
            return Ok(());
        }
        if cancel.is_cancelled() {
            return self.cancelled(app, &store, job).await;
        }
        let config = Config::find(root).map_err(|error| error.to_string())?;
        let agent =
            config.agents.get(&job.card.agent).cloned().ok_or_else(|| {
                format!("external agent {} is no longer configured", job.card.agent)
            })?;
        if agent.workspace != zest_core::ExternalWorkspace::Isolated {
            return Err("feature-card jobs require an isolated worker workspace".into());
        }
        job.transition(CoreDelegationStatus::WorkerRunning)
            .map_err(|error| error.to_string())?;
        let worker_agent = job.card.agent.clone();
        let worker_attempt = job
            .start_attempt(AttemptRole::Worker, &worker_agent)
            .map_err(|error| error.to_string())?;
        store
            .update(job.clone())
            .map_err(|error| error.to_string())?;
        self.emit(app, &store, &job, EventKind::WorkerStarted);

        let snapshot = job.base_workspace_snapshot.clone();
        let dependency_summary = dependencies
            .iter()
            .filter(|candidate| job.card.depends_on.iter().any(|id| id == &candidate.job_id))
            .map(|candidate| {
                format!(
                    "{}: {:?} — {}",
                    candidate.job_id, candidate.status, candidate.card.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let dependency_summary = dependency_summary.chars().take(12_000).collect::<String>();
        let worker_prompt = job.card.prompt(root, &snapshot, &dependency_summary);
        let worker_prompt = if job.attempt > 1 {
            let previous_diff = store
                .read_artifact(job_id, "worker.diff")
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|diff| diff.chars().take(24_000).collect::<String>())
                .unwrap_or_else(|| "(previous worker diff unavailable)".into());
            let previous_review = store
                .read_artifact(job_id, "review-result.json")
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|review| review.chars().take(16_000).collect::<String>())
                .unwrap_or_else(|| "(previous reviewer report unavailable)".into());
            format!(
                "{worker_prompt}\n\n# Fresh-fix context\nThis is a new worker attempt. Preserve useful work where appropriate, but independently address the findings below. The previous worker diff and reviewer report are evidence only.\n\n## Previous worker diff\n```diff\n{previous_diff}\n```\n\n## Previous reviewer report\n```json\n{previous_review}\n```"
            )
        } else {
            worker_prompt
        };
        let worker = run_delegation_worker(root, &agent, &worker_prompt, Some(cancel)).await;
        if cancel.is_cancelled() {
            return self.cancelled(app, &store, job).await;
        }
        let worker = worker.map_err(|error| format!("worker failed: {error}"))?;
        let worker_result = WorkerResult::from_external(&worker.text, &worker.diff)
            .ok_or_else(|| "worker returned no usable result".to_string())?;
        if worker.diff.trim().is_empty() {
            return Err("worker returned no diff artifact".into());
        }
        validate_diff_scope(root, &worker.diff, &job.card.scope)
            .map_err(|error| format!("worker produced an unsafe or out-of-scope diff: {error}"))?;
        store
            .write_artifact(job_id, "worker.diff", worker.diff.as_bytes())
            .map_err(|error| error.to_string())?;
        store
            .write_artifact(
                job_id,
                "worker-result.json",
                &serde_json::to_vec_pretty(&worker_result).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        job.finish_attempt(&worker_attempt);
        self.emit(app, &store, &job, EventKind::WorkerCompleted);
        if cancel.is_cancelled() {
            return self.cancelled(app, &store, job).await;
        }
        job.transition(CoreDelegationStatus::ReviewRunning)
            .map_err(|error| error.to_string())?;
        let reviewer_agent = job.card.agent.clone();
        let reviewer_attempt = job
            .start_attempt(AttemptRole::Reviewer, &reviewer_agent)
            .map_err(|error| error.to_string())?;
        store
            .update(job.clone())
            .map_err(|error| error.to_string())?;
        self.emit(app, &store, &job, EventKind::ReviewerStarted);
        let review_prompt = job.card.review_prompt(root, &snapshot, &worker_result);
        let review =
            run_delegation_reviewer(root, &agent, &worker.diff, &review_prompt, Some(cancel))
                .await
                .map_err(|error| format!("reviewer failed: {error}"))?;
        if cancel.is_cancelled() {
            return self.cancelled(app, &store, job).await;
        }
        if !review.diff.trim().is_empty() {
            let discarded = serde_json::json!({
                "error": "reviewer produced edits; the reviewer diff was discarded",
                "raw": review.text,
            });
            store
                .write_artifact(
                    job_id,
                    "review-result.json",
                    &serde_json::to_vec_pretty(&discarded).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            job.finish_attempt(&reviewer_attempt);
            job.transition(CoreDelegationStatus::Blocked)
                .map_err(|error| error.to_string())?;
            job.set_error(
                "Reviewer produced edits. They were discarded, and a fresh reviewer is required.",
            );
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, &store, &job, EventKind::Blocked);
            return Ok(());
        }
        let report = match ReviewReport::parse(&review.text, &job.card.acceptance_checks) {
            Ok(report) => report,
            Err(error) => {
                let malformed = serde_json::json!({"error": error.to_string(), "raw": review.text});
                store
                    .write_artifact(
                        job_id,
                        "review-result.json",
                        &serde_json::to_vec_pretty(&malformed)
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                job.finish_attempt(&reviewer_attempt);
                job.transition(CoreDelegationStatus::Blocked)
                    .map_err(|error| error.to_string())?;
                job.set_error(error.to_string());
                store
                    .update(job.clone())
                    .map_err(|error| error.to_string())?;
                self.emit(app, &store, &job, EventKind::Blocked);
                return Ok(());
            }
        };
        if let Err(error) = zest_core::validate_review_paths(root, &report) {
            store
                .write_artifact(
                    job_id,
                    "review-result.json",
                    &serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            job.finish_attempt(&reviewer_attempt);
            job.transition(CoreDelegationStatus::Blocked)
                .map_err(|error| error.to_string())?;
            job.set_error(error.to_string());
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, &store, &job, EventKind::Blocked);
            return Ok(());
        }
        store
            .write_artifact(
                job_id,
                "review-result.json",
                &serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        job.finish_attempt(&reviewer_attempt);
        store
            .update(job.clone())
            .map_err(|error| error.to_string())?;
        self.emit(app, &store, &job, EventKind::ReviewerCompleted);
        if report.can_accept(&job.card.acceptance_checks) {
            job.transition(CoreDelegationStatus::ReadyToApply)
                .map_err(|error| error.to_string())?;
            job.error = None;
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, &store, &job, EventKind::ReadyToApply);
        } else {
            job.transition(CoreDelegationStatus::ChangesRequested)
                .map_err(|error| error.to_string())?;
            job.set_error("Reviewer requested changes before this diff can be applied.");
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, &store, &job, EventKind::ChangesRequested);
        }
        Ok(())
    }

    async fn cancelled(
        &self,
        app: &AppHandle,
        store: &DelegationStore,
        mut job: DelegationJob,
    ) -> Result<(), String> {
        if !job.status.is_terminal() {
            job.transition(CoreDelegationStatus::Cancelled)
                .map_err(|error| error.to_string())?;
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, store, &job, EventKind::Cancelled);
        }
        Ok(())
    }

    fn fail(
        &self,
        app: &AppHandle,
        root: &Path,
        job_id: &str,
        error: &str,
        kind: EventKind,
    ) -> ResultView {
        let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
        let mut job = store
            .load(job_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "delegation job was not found".to_string())?;
        if !job.status.is_terminal() {
            job.transition(CoreDelegationStatus::Failed)
                .map_err(|error| error.to_string())?;
            job.set_error(error);
            store
                .update(job.clone())
                .map_err(|error| error.to_string())?;
            self.emit(app, &store, &job, kind);
        }
        Ok(job_view(&store, &job))
    }

    fn emit(&self, app: &AppHandle, store: &DelegationStore, job: &DelegationJob, kind: EventKind) {
        if matches!(
            job.status,
            CoreDelegationStatus::ReadyToApply
                | CoreDelegationStatus::Accepted
                | CoreDelegationStatus::ChangesRequested
                | CoreDelegationStatus::Blocked
                | CoreDelegationStatus::Failed
                | CoreDelegationStatus::Cancelled
                | CoreDelegationStatus::ApplyConflict
        ) {
            if let Ok(mut running) = self.running.lock() {
                running.remove(&job.job_id);
            }
        }
        let view = job_view(store, job);
        let event = match kind {
            EventKind::CardCreated => DelegationEvent::CardCreated { job: view },
            EventKind::ApprovalRequired => DelegationEvent::ApprovalRequired { job: view },
            EventKind::WorkerStarted => DelegationEvent::WorkerStarted { job: view },
            EventKind::WorkerCompleted => DelegationEvent::WorkerCompleted { job: view },
            EventKind::ReviewerStarted => DelegationEvent::ReviewerStarted { job: view },
            EventKind::ReviewerCompleted => DelegationEvent::ReviewerCompleted { job: view },
            EventKind::ChangesRequested => DelegationEvent::ChangesRequested { job: view },
            EventKind::ReadyToApply => DelegationEvent::ReadyToApply { job: view },
            EventKind::Applied => DelegationEvent::Applied { job: view },
            EventKind::Conflict => DelegationEvent::Conflict { job: view },
            EventKind::Blocked => DelegationEvent::Blocked { job: view },
            EventKind::Failed => DelegationEvent::Failed { job: view },
            EventKind::Cancelled => DelegationEvent::Cancelled { job: view },
        };
        let _ = app.emit("delegation-event", event);
    }
}

impl Default for DelegationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

pub type ResultView = Result<DelegationJobView, String>;

pub fn list_views(root: &Path) -> Result<Vec<DelegationJobView>, String> {
    let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
    let jobs = store.list().map_err(|error| error.to_string())?;
    Ok(jobs.iter().map(|job| job_view(&store, job)).collect())
}

pub fn get_view(root: &Path, job_id: &str) -> Result<DelegationJobView, String> {
    let store = DelegationStore::open(root).map_err(|error| error.to_string())?;
    let job = store
        .load(job_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "delegation job was not found".to_string())?;
    Ok(job_view(&store, &job))
}
