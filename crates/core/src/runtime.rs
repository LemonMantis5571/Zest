//! Shared assembly for CLI and desktop front-ends.
//!
//! One place for config → registry → tools → agent so both entrypoints stay
//! aligned. Multi-provider routing is only exposed via [`Delegate`] workers;
//! the parent conversation stays pinned to a single provider.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::agent::Agent;
use crate::config::Config;
use crate::error::{HarnessError, Result};
use crate::prompt::{compose_system, load_custom_system, DEFAULT_SYSTEM};
use crate::provider::normalize_effort;
use crate::provider::registry::ProviderRegistry;
use crate::routing::Router;
use crate::skills::SkillSet;
use crate::tools::approval::{Approver, DenyApprover};
use crate::tools::delegate::Delegate;
use crate::tools::{register_read_tools, register_skill_tools, register_write_tools, ToolRegistry};
use crate::usage::Ledger;

/// Built runtime ready for a provider-pinned conversation.
pub struct RuntimeSession {
    pub root: PathBuf,
    pub config: Config,
    pub registry: Arc<ProviderRegistry>,
    pub provider_id: String,
    pub model: String,
    pub effort: String,
    pub agent: Agent,
    pub ledger: Arc<Mutex<Ledger>>,
    /// Shared with `read_skill`; can be replaced on Settings save.
    pub skills: Arc<RwLock<SkillSet>>,
    /// Base system prompt before custom/skills layers (front-end flavor).
    pub base_system: String,
}

/// Assembles config, providers, tools, ledger, and an [`Agent`].
pub struct RuntimeBuilder {
    root: PathBuf,
    config: Option<Config>,
    provider_id: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    system: Option<String>,
    ledger: Option<Arc<Mutex<Ledger>>>,
    approver: Option<Arc<dyn Approver>>,
    enable_delegate: bool,
    register_write: bool,
}

impl RuntimeBuilder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            config: None,
            provider_id: None,
            model: None,
            effort: None,
            system: None,
            ledger: None,
            approver: None,
            enable_delegate: true,
            register_write: true,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_provider(mut self, id: impl Into<String>) -> Self {
        self.provider_id = Some(id.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        self.effort = Some(effort.into());
        self
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_ledger(mut self, ledger: Arc<Mutex<Ledger>>) -> Self {
        self.ledger = Some(ledger);
        self
    }

    pub fn with_approver(mut self, approver: Arc<dyn Approver>) -> Self {
        self.approver = Some(approver);
        self
    }

    pub fn enable_delegate(mut self, on: bool) -> Self {
        self.enable_delegate = on;
        self
    }

    pub fn register_write_tools(mut self, on: bool) -> Self {
        self.register_write = on;
        self
    }

    pub fn build(self) -> Result<RuntimeSession> {
        let root = self.root;
        let config = match self.config {
            Some(c) => c,
            None => Config::find(&root)?,
        };

        let (registry, _skipped) = ProviderRegistry::from_config(&config);

        let provider_id = self
            .provider_id
            .or_else(|| {
                config
                    .default_target()
                    .map(|t| t.provider.clone())
            })
            .ok_or_else(|| {
                HarnessError::Other(
                    "no provider selected and zest.toml has no [routing].default".into(),
                )
            })?;

        let provider = registry.get(&provider_id).ok_or_else(|| {
            HarnessError::Other(format!(
                "provider `{provider_id}` is configured but could not be loaded"
            ))
        })?;

        let model = self
            .model
            .or_else(|| {
                config.default_target().and_then(|t| {
                    if t.provider == provider_id {
                        t.model.clone()
                    } else {
                        None
                    }
                })
            })
            .or_else(|| std::env::var("ZEST_MODEL").ok())
            .unwrap_or_else(|| provider.default_model().to_string());

        let effort = normalize_effort(
            &self
                .effort
                .or_else(|| std::env::var("ZEST_EFFORT").ok())
                .unwrap_or_else(|| "high".to_string()),
        );

        provider
            .validate_selection(&model, &effort)
            .map_err(HarnessError::Other)?;

        let ledger = self
            .ledger
            .unwrap_or_else(|| Arc::new(Mutex::new(Ledger::load())));

        let base_system = self.system.unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
        let custom = load_custom_system(&root);
        let skills = Arc::new(RwLock::new(SkillSet::discover(&root)));
        let system = {
            let guard = skills
                .read()
                .map_err(|_| HarnessError::Other("skill registry lock poisoned".into()))?;
            compose_system(&base_system, &custom, &guard)
        };

        let mut worker_tools = ToolRegistry::new();
        register_read_tools(&mut worker_tools, &root).map_err(|e| {
            HarnessError::Other(format!("register read tools: {e}"))
        })?;
        if self.register_write {
            register_write_tools(&mut worker_tools, &root).map_err(|e| {
                HarnessError::Other(format!("register write tools: {e}"))
            })?;
        }
        register_skill_tools(&mut worker_tools, skills.clone());

        let mut tools = worker_tools.clone();
        let registry = Arc::new(registry);

        // Multi-provider routing is delegated workers only — parent stays pinned.
        if self.enable_delegate && registry.len() > 1 {
            let mut kinds: Vec<String> =
                config.routing.rules.iter().map(|r| r.kind.clone()).collect();
            kinds.sort();
            kinds.dedup();
            tools.register(Arc::new(
                Delegate::new(
                    registry.clone(),
                    Arc::new(Router::from_config(&config)),
                    worker_tools,
                )
                .with_ledger(ledger.clone())
                .with_kinds(kinds),
            ));
        }

        let approver = self
            .approver
            .unwrap_or_else(|| Arc::new(DenyApprover) as Arc<dyn Approver>);

        let mut agent = Agent::new(provider, tools)
            .with_system(system)
            .with_ledger(ledger.clone())
            .with_approver(approver);
        agent.model = model.clone();
        agent.effort = effort.clone();

        Ok(RuntimeSession {
            root,
            config,
            registry,
            provider_id,
            model,
            effort,
            agent,
            ledger,
            skills,
            base_system,
        })
    }

    /// Resolve workspace root for callers that only have a path hint.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-runtime-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builds_with_gateway_config_without_key_is_skipped() {
        let dir = scratch("cfg");
        let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
        writeln!(
            f,
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
api_key_env = "ZEST_TEST_RUNTIME_ABSENT_KEY"
model = "gpt-5.6-sol"

[routing]
default = {{ provider = "codex", model = "gpt-5.6-sol" }}
"#
        )
        .unwrap();
        std::env::remove_var("ZEST_TEST_RUNTIME_ABSENT_KEY");

        let err = match RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .build()
        {
            Ok(_) => panic!("expected build to fail without gateway key"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("could not be loaded") || msg.contains("unavailable"),
            "{msg}"
        );
    }
}
