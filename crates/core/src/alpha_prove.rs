//! Deterministic fake-provider proofs for Stable Windows Alpha §5.
//!
//! Covers route selection, selected model, tool round-trip, ledger attribution,
//! fallback reasons, and thread restoration — without spending quota.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use crate::agent::Agent;
use crate::anthropic::types::{Message, Usage};
use crate::auth::AuthStatus;
use crate::config::Config;
use crate::provider::registry::ProviderRegistry;
use crate::provider::{
    catalogue_from_lists, Completion, ModelSpec, Provider, RateLimitSnapshot, StreamEvent,
    TurnRequest,
};
use crate::routing::Router;
use crate::runtime::RuntimeBuilder;
use crate::thread::{Thread, ThreadStore, WIRE_FORMAT_ANTHROPIC_MESSAGES};
use crate::tools::delegate::{Delegate, DELEGATE_TOOL};
use crate::tools::{register_read_tools, Tool, ToolRegistry};
use crate::usage::Ledger;

struct ScriptedProvider {
    id: String,
    default_model: String,
    models: Vec<ModelSpec>,
    /// Models seen on each `stream_turn` call.
    seen_models: Mutex<Vec<String>>,
    calls: AtomicUsize,
    /// When true, first call asks for `read_file` on README.md; second ends.
    tool_roundtrip: bool,
    usage_in: u32,
    usage_out: u32,
}

impl ScriptedProvider {
    fn new(id: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            default_model: model.into(),
            models: catalogue_from_lists(model, &[], &[]),
            seen_models: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
            tool_roundtrip: false,
            usage_in: 10,
            usage_out: 5,
        }
    }

    fn with_tool_roundtrip(mut self) -> Self {
        self.tool_roundtrip = true;
        self
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn models(&self) -> Vec<ModelSpec> {
        self.models.clone()
    }

    fn auth_status(&self) -> AuthStatus {
        AuthStatus::Ready { account: None }
    }

    async fn stream_turn(
        &self,
        req: &TurnRequest,
        on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
    ) -> crate::Result<Completion> {
        self.seen_models.lock().unwrap().push(req.model.clone());
        let n = self.calls.fetch_add(1, Ordering::SeqCst);

        if self.tool_roundtrip && n == 0 {
            let id = "toolu_readme";
            on_event(StreamEvent::ToolCallStart {
                name: "read_file",
                id,
            });
            return Ok(Completion {
                content: vec![json!({
                    "type": "tool_use",
                    "id": id,
                    "name": "read_file",
                    "input": { "path": "README.md" }
                })],
                stop_reason: Some("tool_use".into()),
                usage: Usage {
                    input_tokens: self.usage_in,
                    output_tokens: self.usage_out,
                    ..Default::default()
                },
                limits: None,
            });
        }

        on_event(StreamEvent::Text("alpha-ok"));
        Ok(Completion {
            content: vec![json!({ "type": "text", "text": "alpha-ok" })],
            stop_reason: Some("end_turn".into()),
            usage: Usage {
                input_tokens: self.usage_in,
                output_tokens: self.usage_out,
                ..Default::default()
            },
            limits: None,
        })
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zest-alpha-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("README.md"), "# Zest alpha\n").unwrap();
    dir
}

fn multi_provider_config() -> Config {
    Config::parse(
        r#"
[providers.primary]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_KEY"
model = "model-primary"

[providers.worker]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_KEY"
model = "model-worker"
models = ["model-worker", "model-worker-fast"]
efforts = ["low", "high"]

[routing]
default = { provider = "primary", model = "model-primary" }

[[routing.rules]]
kind = "mechanical"
provider = "worker"
model = "model-worker-fast"
"#,
    )
    .expect("valid")
}

#[test]
fn route_selection_picks_rule_model() {
    std::env::set_var("ZEST_ALPHA_KEY", "present");
    let config = multi_provider_config();
    let mut registry = ProviderRegistry::default();
    registry.insert(Arc::new(ScriptedProvider::new("primary", "model-primary")));
    registry.insert(Arc::new(ScriptedProvider::new("worker", "model-worker")));
    let router = Router::from_config(&config);

    let hit = router
        .resolve(Some("mechanical"), &registry, &Ledger::default())
        .expect("resolved");
    assert_eq!(hit.target.provider, "worker");
    assert_eq!(hit.target.model.as_deref(), Some("model-worker-fast"));
    assert!(hit.skipped.is_empty());
}

#[test]
fn fallback_reasons_surface_exhausted_provider() {
    std::env::set_var("ZEST_ALPHA_KEY", "present");
    let config = multi_provider_config();
    let mut registry = ProviderRegistry::default();
    registry.insert(Arc::new(ScriptedProvider::new("primary", "model-primary")));
    registry.insert(Arc::new(ScriptedProvider::new("worker", "model-worker")));
    let router = Router::from_config(&config);

    let mut ledger = Ledger::default();
    ledger.record(
        "worker",
        &Completion {
            content: vec![],
            stop_reason: None,
            usage: Usage::default(),
            limits: Some(RateLimitSnapshot {
                requests_remaining: Some(0),
                ..Default::default()
            }),
        },
    );

    let hit = router
        .resolve(Some("mechanical"), &registry, &ledger)
        .expect("falls back");
    assert_eq!(hit.target.provider, "primary");
    assert_eq!(hit.skipped.len(), 1);
    assert_eq!(hit.skipped[0].0, "worker");
    assert!(hit.skipped[0].1.contains("no requests remaining"));
}

#[tokio::test]
async fn selected_model_tool_roundtrip_and_ledger() {
    let dir = scratch("tool");
    let provider = Arc::new(ScriptedProvider::new("codex", "gpt-alpha").with_tool_roundtrip());
    let mut tools = ToolRegistry::new();
    register_read_tools(&mut tools, &dir).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::default()));

    let mut agent = Agent::new(provider.clone(), tools)
        .with_ledger(ledger.clone())
        .with_system("test");
    agent.model = "gpt-alpha".into();
    agent.effort = "high".into();

    let mut saw_text = false;
    let mut saw_tool_start = false;
    let mut saw_tool_ok = false;
    let mut on_event = |ev: StreamEvent<'_>| match ev {
        StreamEvent::Text(_) => saw_text = true,
        StreamEvent::ToolCallStart { name, .. } => {
            assert_eq!(name, "read_file");
            saw_tool_start = true;
        }
        StreamEvent::ToolCallResult {
            name,
            is_error,
            summary,
            ..
        } => {
            assert_eq!(name, "read_file");
            assert!(!is_error, "{summary}");
            saw_tool_ok = true;
        }
        _ => {}
    };

    agent
        .send("Read README.md and confirm the title.", &mut on_event)
        .await
        .unwrap();

    assert!(saw_tool_start && saw_tool_ok && saw_text);
    let seen = provider.seen_models.lock().unwrap().clone();
    assert_eq!(seen, vec!["gpt-alpha".to_string(), "gpt-alpha".to_string()]);

    let guard = ledger.lock().unwrap();
    let usage = guard.get("codex").expect("ledger entry");
    assert_eq!(usage.requests, 2, "tool turn + final");
    assert_eq!(usage.input_tokens, 20);
    assert_eq!(usage.output_tokens, 10);
}

#[tokio::test]
async fn delegate_uses_routed_model_and_attributes_ledger() {
    std::env::set_var("ZEST_ALPHA_KEY", "present");
    let dir = scratch("delegate");
    let config = multi_provider_config();

    let worker = Arc::new(ScriptedProvider::new("worker", "model-worker"));
    let primary = Arc::new(ScriptedProvider::new("primary", "model-primary"));

    let mut registry = ProviderRegistry::default();
    registry.insert(primary);
    registry.insert(worker.clone());
    let registry = Arc::new(registry);

    let mut worker_tools = ToolRegistry::new();
    register_read_tools(&mut worker_tools, &dir).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::default()));

    let delegate = Delegate::new(
        registry,
        Arc::new(Router::from_config(&config)),
        worker_tools,
    )
    .with_ledger(ledger.clone())
    .with_kinds(vec!["mechanical".into()]);

    let out = delegate
        .run(json!({
            "task": "Say alpha-ok",
            "kind": "mechanical"
        }))
        .await
        .unwrap();

    assert!(
        out.contains("[worker · model-worker-fast]"),
        "header must name routed provider/model: {out}"
    );
    assert!(out.contains("alpha-ok"));

    let seen = worker.seen_models.lock().unwrap().clone();
    assert_eq!(seen, vec!["model-worker-fast".to_string()]);

    let guard = ledger.lock().unwrap();
    let usage = guard.get("worker").expect("worker billed");
    assert_eq!(usage.requests, 1);
    assert!(
        guard.get("primary").is_none(),
        "parent not billed for worker"
    );
}

#[test]
fn thread_restoration_reloads_wire_history() {
    let dir = scratch("thread");
    let store = ThreadStore::open(&dir).unwrap();

    let mut thread = Thread::new().with_provider("codex");
    thread.title = Some("alpha".into());
    thread.wire_format = WIRE_FORMAT_ANTHROPIC_MESSAGES.into();
    thread.agent_messages = vec![
        Message::user_text("hello"),
        Message::assistant(vec![json!({ "type": "text", "text": "world" })]),
    ];
    store.save(&thread).unwrap();

    let loaded = store.load_with_recovery(&thread.id).unwrap();
    assert_eq!(loaded.thread.provider_id.as_deref(), Some("codex"));
    assert_eq!(loaded.thread.agent_messages.len(), 2);
    assert_eq!(loaded.thread.agent_messages[0].role, "user");
    assert_eq!(loaded.thread.agent_messages[1].role, "assistant");
    assert!(loaded.warning.is_none());

    // Restore into an agent the way desktop does.
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new("codex", "m"));
    let agent = Agent::new(provider, ToolRegistry::new())
        .with_messages(loaded.thread.agent_messages.clone());
    assert_eq!(agent.messages.len(), 2);
    assert_eq!(agent.provider_id(), "codex");
}

#[test]
fn model_spec_rejects_unknown_model_when_catalogue_omitted() {
    std::env::set_var("ZEST_ALPHA_OMIT_KEY", "present");
    let dir = scratch("modelspec");
    let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
    use std::io::Write;
    writeln!(
        f,
        r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_OMIT_KEY"
model = "gpt-only-default"

[routing]
default = {{ provider = "codex", model = "gpt-only-default" }}
"#
    )
    .unwrap();

    let err = match RuntimeBuilder::new(&dir)
        .with_config(Config::find(&dir).unwrap())
        .with_model("gpt-not-listed")
        .with_effort("high")
        .enable_delegate(false)
        .build()
    {
        Ok(_) => panic!("expected unknown model to be rejected"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("not supported"),
        "unexpected: {err}"
    );
}

#[test]
fn model_spec_rejects_effort_outside_gateway_allow_list() {
    std::env::set_var("ZEST_ALPHA_EFFORT_KEY", "present");
    let dir = scratch("effort");
    let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
    use std::io::Write;
    writeln!(
        f,
        r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_EFFORT_KEY"
model = "gpt-a"
efforts = ["low", "high"]

[routing]
default = {{ provider = "codex", model = "gpt-a" }}
"#
    )
    .unwrap();

    let err = match RuntimeBuilder::new(&dir)
        .with_config(Config::find(&dir).unwrap())
        .with_effort("max")
        .enable_delegate(false)
        .build()
    {
        Ok(_) => panic!("expected effort max to be rejected"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("effort"), "unexpected: {err}");
}

#[test]
fn runtime_registers_delegate_when_multiple_providers_load() {
    std::env::set_var("ZEST_ALPHA_MULTI_KEY", "present");
    let dir = scratch("multi");
    let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
    use std::io::Write;
    writeln!(
        f,
        r#"
[providers.a]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_MULTI_KEY"
model = "m-a"

[providers.b]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ALPHA_MULTI_KEY"
model = "m-b"

[routing]
default = {{ provider = "a", model = "m-a" }}

[[routing.rules]]
kind = "mechanical"
provider = "b"
"#
    )
    .unwrap();

    let runtime = RuntimeBuilder::new(&dir)
        .with_config(Config::find(&dir).unwrap())
        .with_provider("a")
        .enable_delegate(true)
        .register_write_tools(false)
        .build()
        .unwrap();

    assert_eq!(runtime.registry.len(), 2);
    assert!(
        runtime.agent.tool_names().contains(&DELEGATE_TOOL),
        "desktop/CLI must expose delegate when multi-provider: {:?}",
        runtime.agent.tool_names()
    );
}
