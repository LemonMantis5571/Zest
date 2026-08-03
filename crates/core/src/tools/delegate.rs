//! Handing a subtask to another provider.
//!
//! This is the orchestration surface: the main agent describes a subtask and the
//! router decides which account serves it — cheap fast model for mechanical work,
//! expensive model for the hard reasoning.
//!
//! Two failure modes are designed out rather than guarded against:
//!
//! - **Runaway delegation.** The worker's tool registry cannot contain this tool,
//!   checked at construction. A depth counter would be a setting someone could
//!   get wrong; an empty capability cannot be.
//! - **A failed subtask killing the parent turn.** A delegation that fails comes
//!   back as a `tool_result` with `is_error: true`, so the main agent can try
//!   something else. That falls out of the `Tool` contract.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::approval::{ApprovalPreview, ToolRisk};
use super::outcome::{SkippedProvider, ToolMetadata, ToolOutcome, UsageDelta};
use super::prepared::PreparedToolCall;
use super::{Tool, ToolRegistry};
use crate::agent::Agent;
use crate::anthropic::types::text_of;
use crate::provider::registry::ProviderRegistry;
use crate::provider::StreamEvent;
use crate::routing::{Resolution, Router, DEFAULT_WORKER_EFFORT};
use crate::usage::{Ledger, ProviderUsage};

pub const DELEGATE_TOOL: &str = "delegate";

const WORKER_SYSTEM: &str = "\
You are a worker handling one self-contained subtask. Do the task and report the \
result. You are not talking to a person — your reply is read by another agent, so \
lead with the answer and leave out preamble and pleasantries.";

pub struct Delegate {
    registry: Arc<ProviderRegistry>,
    router: Arc<Router>,
    ledger: Option<Arc<Mutex<Ledger>>>,
    /// What the worker can do. Cannot contain `delegate` — see module note.
    worker_tools: ToolRegistry,
    worker_system: String,
    /// Task kinds the routing rules know about, used to constrain the schema.
    kinds: Vec<String>,
}

impl Delegate {
    /// # Panics
    /// If `worker_tools` contains the delegate tool. That is a wiring mistake
    /// with no safe runtime behaviour, and it should fail loudly at startup
    /// rather than mid-conversation.
    pub fn new(
        registry: Arc<ProviderRegistry>,
        router: Arc<Router>,
        worker_tools: ToolRegistry,
    ) -> Self {
        assert!(
            !worker_tools.names().contains(&DELEGATE_TOOL),
            "workers must not be given the `{DELEGATE_TOOL}` tool — that is an unbounded delegation loop"
        );

        Self {
            registry,
            router,
            ledger: None,
            worker_tools,
            worker_system: WORKER_SYSTEM.to_string(),
            kinds: Vec::new(),
        }
    }

    pub fn with_ledger(mut self, ledger: Arc<Mutex<Ledger>>) -> Self {
        self.ledger = Some(ledger);
        self
    }

    /// Constrain the `kind` argument to the kinds routing actually knows.
    pub fn with_kinds(mut self, kinds: Vec<String>) -> Self {
        self.kinds = kinds;
        self
    }

    pub fn with_worker_system(mut self, system: impl Into<String>) -> Self {
        self.worker_system = system.into();
        self
    }
}

fn parse_input(input: &Value) -> std::result::Result<(&str, Option<&str>), String> {
    let task = input
        .get("task")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "missing required field `task`".to_string())?;
    Ok((task, input.get("kind").and_then(Value::as_str)))
}

/// First line of the task, clipped — the approval card needs to say what is
/// being handed over without becoming a wall of text.
fn first_line(task: &str) -> String {
    const MAX: usize = 120;
    let line = task
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    let clipped: String = line.chars().take(MAX - 1).collect();
    format!("{clipped}…")
}

fn usage_snapshot(ledger: &Option<Arc<Mutex<Ledger>>>, provider_id: &str) -> ProviderUsage {
    ledger
        .as_ref()
        .and_then(|l| l.lock().ok())
        .and_then(|g| g.get(provider_id).cloned())
        .unwrap_or_default()
}

fn usage_delta(before: &ProviderUsage, after: &ProviderUsage) -> UsageDelta {
    UsageDelta {
        requests: after.requests.saturating_sub(before.requests),
        input_tokens: after.input_tokens.saturating_sub(before.input_tokens),
        output_tokens: after.output_tokens.saturating_sub(before.output_tokens),
    }
}

#[async_trait]
impl Tool for Delegate {
    fn name(&self) -> &str {
        DELEGATE_TOOL
    }

    fn description(&self) -> &str {
        "Hand a self-contained subtask to another model, chosen by routing policy. \
         Use this when a task splits into independent pieces, or when a piece is \
         mechanical enough that a cheaper model should do it. The subtask gets no \
         conversation history, so describe it completely — including any file paths \
         and context it needs."
    }

    /// Delegation spends a **second** subscription, so it is gated like every
    /// other thing that costs money and the permission mode decides whether the
    /// user is asked. It was previously read-risk, which meant a fan-out across
    /// three accounts happened silently even in Manual mode.
    fn risk(&self) -> ToolRisk {
        ToolRisk::Exec
    }

    fn input_schema(&self) -> Value {
        let mut kind = json!({
            "type": "string",
            "description": "What sort of work this is. Routing maps it to a provider."
        });
        if !self.kinds.is_empty() {
            kind["enum"] = json!(self.kinds);
        }

        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The complete, self-contained subtask."
                },
                "kind": kind
            },
            "required": ["task"],
            "additionalProperties": false
        })
    }

    /// Resolve the route up front so the approval card can name the provider
    /// and model that will actually be spent. "Allow delegate?" without saying
    /// which account is not a question anyone can answer.
    fn prepare(&self, input: Value) -> Result<PreparedToolCall, String> {
        let (task, kind) = parse_input(&input)?;
        let resolution = self.resolve(kind)?;
        let model = self.model_for(&resolution)?;
        let provider_id = &resolution.target.provider;

        // The target doubles as the session-grant key, so "allow for session"
        // covers this provider+model pair rather than all delegation.
        let target = format!("{provider_id}/{model}");
        let summary = format!("Delegate to {target}: {}", first_line(task));

        Ok(PreparedToolCall::plain_with_preview(
            DELEGATE_TOOL,
            ToolRisk::Exec,
            input,
            ApprovalPreview {
                path: target,
                summary,
                diff: String::new(),
            },
        ))
    }

    /// Carries the approved target through from [`Self::prepare`] so the
    /// dispatch can prove it is spending the account the user agreed to.
    async fn execute_prepared(
        &self,
        prepared: PreparedToolCall,
    ) -> std::result::Result<ToolOutcome, String> {
        let approved = prepared.preview.path.clone();
        let input = prepared
            .plain_input()
            .cloned()
            .ok_or_else(|| "internal error: delegate prepared kind mismatch".to_string())?;
        self.dispatch(input, Some(approved)).await
    }

    async fn run(&self, input: Value) -> std::result::Result<ToolOutcome, String> {
        self.dispatch(input, None).await
    }
}

impl Delegate {
    fn resolve(&self, kind: Option<&str>) -> std::result::Result<Resolution, String> {
        // Resolve inside a tight scope so no lock is held across an await.
        let resolution = {
            let guard = self.ledger.as_ref().and_then(|l| l.lock().ok());
            match guard.as_deref() {
                Some(ledger) => self.router.resolve(kind, &self.registry, ledger),
                None => self
                    .router
                    .resolve(kind, &self.registry, &Ledger::default()),
            }
        };
        resolution.ok_or_else(|| "no provider is available to take this subtask".to_string())
    }

    fn model_for(&self, resolution: &Resolution) -> std::result::Result<String, String> {
        let provider_id = &resolution.target.provider;
        let provider = self
            .registry
            .get(provider_id)
            .ok_or_else(|| format!("provider `{provider_id}` is not loaded"))?;
        Ok(resolution
            .target
            .model
            .clone()
            .unwrap_or_else(|| provider.default_model().to_string()))
    }

    async fn dispatch(
        &self,
        input: Value,
        approved: Option<String>,
    ) -> std::result::Result<ToolOutcome, String> {
        let (task, kind) = parse_input(&input)?;

        let resolution = self.resolve(kind)?;
        let provider_id = resolution.target.provider.clone();
        let provider = self.registry.get(&provider_id).ok_or_else(|| {
            format!("provider `{provider_id}` disappeared between resolve and run")
        })?;

        let model = self.model_for(&resolution)?;

        // The route is resolved once for the approval card and again here.
        // In between, a provider can hit a rate limit and drop out of the
        // candidate list — which would mean spending a different account than
        // the one the user agreed to. Same staleness reasoning as
        // `write_file`'s pre-image check.
        if let Some(approved) = approved {
            let now = format!("{provider_id}/{model}");
            if approved != now {
                return Err(format!(
                    "routing changed after approval ({approved} → {now}); \
                     aborting — fresh approval required"
                ));
            }
        }

        let effort = resolution
            .target
            .effort
            .clone()
            .unwrap_or_else(|| DEFAULT_WORKER_EFFORT.to_string());

        // Belt-and-suspenders: the router already skips invalid pairs, but
        // never dispatch without a final catalogue check.
        provider
            .validate_selection(&model, &effort)
            .map_err(|e| format!("delegated model rejected: {e}"))?;

        let before = usage_snapshot(&self.ledger, &provider_id);

        // The rule's framing goes first, the generic worker contract after, so
        // "you are writing UI" is read before "report the result tersely".
        let system = match resolution.prompt.as_deref().map(str::trim) {
            Some(extra) if !extra.is_empty() => format!("{extra}\n\n{}", self.worker_system),
            _ => self.worker_system.clone(),
        };

        let mut worker = Agent::new(provider, self.worker_tools.clone()).with_system(system);
        if let Some(ledger) = &self.ledger {
            worker = worker.with_ledger(ledger.clone());
        }
        worker.model = model.clone();
        worker.effort = effort;

        // The worker's stream is not rendered — interleaving it with the parent's
        // output would be unreadable. Its answer is the tool result.
        let mut discard = |_: StreamEvent<'_>| {};
        worker
            .send(task, &mut discard)
            .await
            .map_err(|e| format!("{provider_id}/{model} failed: {e}"))?;

        let answer = worker
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .map(|m| text_of(&m.content))
            .unwrap_or_default();

        if answer.trim().is_empty() {
            return Err(format!("{provider_id}/{model} returned no text"));
        }

        let after = usage_snapshot(&self.ledger, &provider_id);
        let skipped: Vec<SkippedProvider> = resolution
            .skipped
            .iter()
            .map(|(id, reason)| SkippedProvider {
                provider_id: id.clone(),
                reason: reason.clone(),
            })
            .collect();

        let metadata = ToolMetadata::Delegation {
            provider_id: provider_id.clone(),
            model: model.clone(),
            routing_kind: kind.map(str::to_string),
            skipped: skipped.clone(),
            usage_delta: usage_delta(&before, &after),
        };

        // Name who answered. The orchestrator is spending several accounts and
        // should be able to see which one produced what.
        let mut header = format!("[{provider_id} · {model}]");
        for skip in &skipped {
            header.push_str(&format!(" (skipped {}: {})", skip.provider_id, skip.reason));
        }

        Ok(ToolOutcome::with_metadata(
            format!("{header}\n{answer}"),
            metadata,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn empty_delegate() -> Delegate {
        let config = Config::env_fallback();
        let registry = Arc::new(ProviderRegistry::default());
        let router = Arc::new(Router::from_config(&config));
        Delegate::new(registry, router, ToolRegistry::new())
    }

    fn two_provider_delegate() -> Delegate {
        std::env::set_var("ZEST_DELEGATE_TEST_KEY", "present");
        let config = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_DELEGATE_TEST_KEY"
model = "gpt-5.6-sol"

[providers.claude]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_DELEGATE_TEST_KEY"
model = "claude-opus-5"

[routing]
default = { provider = "codex", model = "gpt-5.6-sol" }
delegation = true

[[routing.rules]]
kind = "planning"
provider = "claude"
"#,
        )
        .unwrap();
        let registry = Arc::new(ProviderRegistry::from_config(&config).0);
        let router = Arc::new(Router::from_config(&config));
        Delegate::new(registry, router, ToolRegistry::new())
    }

    /// Spending a second subscription is gated like anything else that costs
    /// money — it used to be read-risk and fan out silently even in Manual.
    #[test]
    fn delegate_is_exec_risk() {
        assert_eq!(empty_delegate().risk(), ToolRisk::Exec);
        assert!(empty_delegate().risk().requires_approval());
    }

    /// "Allow delegate?" without saying which account is unanswerable.
    #[test]
    fn the_card_names_the_provider_and_model_that_will_be_spent() {
        let delegate = two_provider_delegate();

        let planning = delegate
            .prepare(json!({ "task": "Design the auth flow", "kind": "planning" }))
            .unwrap();
        assert_eq!(planning.risk, ToolRisk::Exec);
        assert_eq!(planning.preview.path, "claude/claude-opus-5");
        assert!(
            planning.preview.summary.contains("Design the auth flow"),
            "{}",
            planning.preview.summary
        );

        // No matching rule: falls to the default, and the card says so.
        let other = delegate
            .prepare(json!({ "task": "Rename a variable", "kind": "mechanical" }))
            .unwrap();
        assert_eq!(other.preview.path, "codex/gpt-5.6-sol");
    }

    #[test]
    fn the_card_target_is_the_session_grant_key() {
        // "Allow for session" keys on (tool, preview.path), so a grant covers
        // this provider+model pair rather than all future delegation.
        let delegate = two_provider_delegate();
        let a = delegate
            .prepare(json!({ "task": "x", "kind": "planning" }))
            .unwrap();
        let b = delegate.prepare(json!({ "task": "y" })).unwrap();
        assert_ne!(
            a.preview.path, b.preview.path,
            "different accounts must not share one grant"
        );
    }

    #[tokio::test]
    async fn a_route_that_changed_after_approval_aborts() {
        let delegate = two_provider_delegate();
        // Approved one account; the resolver now yields another.
        let stale = PreparedToolCall::plain_with_preview(
            DELEGATE_TOOL,
            ToolRisk::Exec,
            json!({ "task": "do a thing", "kind": "planning" }),
            ApprovalPreview {
                path: "codex/gpt-5.6-sol".into(),
                summary: "Delegate to codex".into(),
                diff: String::new(),
            },
        );
        let err = delegate.execute_prepared(stale).await.unwrap_err();
        assert!(err.contains("routing changed after approval"), "{err}");
        assert!(err.contains("fresh approval required"), "{err}");
    }

    #[test]
    fn a_long_task_is_clipped_for_the_card() {
        let long = "a".repeat(400);
        let summary = first_line(&long);
        assert!(summary.chars().count() <= 120, "{}", summary.len());
        assert!(summary.ends_with('…'));
        // Multi-line tasks show the first meaningful line only.
        assert_eq!(first_line("\n\n  first line \nsecond"), "first line");
    }

    #[test]
    fn schema_omits_the_enum_when_no_kinds_are_configured() {
        let schema = empty_delegate().input_schema();
        assert!(schema["properties"]["kind"].get("enum").is_none());
        assert_eq!(schema["required"], json!(["task"]));
    }

    #[test]
    fn schema_constrains_kind_to_configured_rules() {
        let delegate = empty_delegate().with_kinds(vec!["mechanical".into(), "review".into()]);
        let schema = delegate.input_schema();
        assert_eq!(
            schema["properties"]["kind"]["enum"],
            json!(["mechanical", "review"])
        );
    }

    #[tokio::test]
    async fn a_blank_task_is_rejected_before_any_provider_is_touched() {
        let delegate = empty_delegate();
        assert!(delegate.run(json!({ "task": "   " })).await.is_err());
        assert!(delegate.run(json!({})).await.is_err());
    }

    #[tokio::test]
    async fn no_available_provider_is_a_tool_error_not_a_panic() {
        // Empty registry: this must come back as Err so the parent turn survives.
        let err = empty_delegate()
            .run(json!({ "task": "do a thing" }))
            .await
            .unwrap_err();
        assert!(err.contains("no provider is available"), "{err}");
    }

    #[test]
    #[should_panic(expected = "unbounded delegation loop")]
    fn giving_a_worker_the_delegate_tool_fails_loudly() {
        let config = Config::env_fallback();
        let registry = Arc::new(ProviderRegistry::default());
        let router = Arc::new(Router::from_config(&config));

        let mut worker_tools = ToolRegistry::new();
        worker_tools.register(Arc::new(Delegate::new(
            registry.clone(),
            router.clone(),
            ToolRegistry::new(),
        )));

        // Wiring a delegate into the worker's own toolset is the one mistake that
        // has no safe runtime behaviour.
        let _ = Delegate::new(registry, router, worker_tools);
    }
}
