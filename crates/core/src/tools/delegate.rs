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

use super::{Tool, ToolRegistry};
use crate::agent::Agent;
use crate::anthropic::types::text_of;
use crate::provider::registry::ProviderRegistry;
use crate::provider::StreamEvent;
use crate::routing::Router;
use crate::usage::Ledger;

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

    async fn run(&self, input: Value) -> std::result::Result<String, String> {
        let task = input
            .get("task")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| "missing required field `task`".to_string())?;
        let kind = input.get("kind").and_then(Value::as_str);

        // Resolve inside a tight scope so no lock is held across the await below.
        let resolution = {
            let guard = self.ledger.as_ref().and_then(|l| l.lock().ok());
            match guard.as_deref() {
                Some(ledger) => self.router.resolve(kind, &self.registry, ledger),
                None => self
                    .router
                    .resolve(kind, &self.registry, &Ledger::default()),
            }
        }
        .ok_or_else(|| "no provider is available to take this subtask".to_string())?;

        let provider_id = resolution.target.provider.clone();
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or_else(|| format!("provider `{provider_id}` disappeared between resolve and run"))?;

        let model = resolution
            .target
            .model
            .clone()
            .unwrap_or_else(|| provider.default_model().to_string());

        let mut worker = Agent::new(provider, self.worker_tools.clone())
            .with_system(self.worker_system.clone());
        if let Some(ledger) = &self.ledger {
            worker = worker.with_ledger(ledger.clone());
        }
        worker.model = model.clone();

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

        // Name who answered. The orchestrator is spending several accounts and
        // should be able to see which one produced what.
        let mut header = format!("[{provider_id} · {model}]");
        for (skipped, reason) in &resolution.skipped {
            header.push_str(&format!(" (skipped {skipped}: {reason})"));
        }

        Ok(format!("{header}\n{answer}"))
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
