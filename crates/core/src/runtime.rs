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
use crate::prompt::{
    compose_system_with_docs, env_context, load_custom_system, load_project_docs, DEFAULT_SYSTEM,
};
use crate::provider::normalize_effort;
use crate::provider::registry::{ProviderRegistry, Skipped};
use crate::routing::Router;
use crate::skills::SkillSet;
use crate::tools::approval::{ApprovalPolicy, Approver, DenyApprover};
use crate::tools::delegate::Delegate;
use crate::tools::{
    register_exec_tools, register_read_tools, register_skill_tools, register_write_tools,
    ToolRegistry,
};
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
    /// Shared with the agent; flip the mode here to change it mid-session.
    pub policy: Arc<Mutex<ApprovalPolicy>>,
    /// Shared with `read_skill`; can be replaced on Settings save.
    pub skills: Arc<RwLock<SkillSet>>,
    /// Base system prompt before custom/skills layers (front-end flavor).
    pub base_system: String,
    /// Non-fatal things the front-end should surface — chiefly a remembered
    /// model or effort that had to be dropped. Silently correcting a stored
    /// preference would leave the user wondering why the picker moved.
    pub warnings: Vec<String>,
}

/// Assembles config, providers, tools, ledger, and an [`Agent`].
pub struct RuntimeBuilder {
    root: PathBuf,
    config: Option<Config>,
    provider_id: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    /// Sticky model/effort from a previous session. Unlike [`Self::model`] this
    /// is dropped rather than fatal when it does not fit the provider.
    remembered: Option<(Option<String>, Option<String>)>,
    system: Option<String>,
    ledger: Option<Arc<Mutex<Ledger>>>,
    approver: Option<Arc<dyn Approver>>,
    policy: Option<Arc<Mutex<ApprovalPolicy>>>,
    enable_delegate: bool,
    register_write: bool,
    register_exec: bool,
}

impl RuntimeBuilder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            config: None,
            provider_id: None,
            model: None,
            effort: None,
            remembered: None,
            system: None,
            ledger: None,
            approver: None,
            policy: None,
            enable_delegate: true,
            register_write: true,
            register_exec: true,
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

    /// Sticky model/effort restored from a previous session.
    ///
    /// Separate from [`Self::with_model`] because the two deserve opposite
    /// treatment when they do not fit the provider. A model the user just
    /// picked should fail loudly. A *remembered* one must not: a stale value on
    /// disk would otherwise make the provider permanently unselectable, and the
    /// only way to change it is to start a session you can no longer start.
    pub fn with_remembered_options(
        mut self,
        model: Option<String>,
        effort: Option<String>,
    ) -> Self {
        self.remembered = Some((model, effort));
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

    /// Share the permission policy so the front-end can switch mode later.
    /// Omitted means [`ApprovalMode::Manual`](crate::ApprovalMode::Manual).
    pub fn with_policy(mut self, policy: Arc<Mutex<ApprovalPolicy>>) -> Self {
        self.policy = Some(policy);
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

    /// Off for callers that must not run commands regardless of config —
    /// `doctor --live` and delegated workers.
    pub fn register_exec_tools(mut self, on: bool) -> Self {
        self.register_exec = on;
        self
    }

    pub fn build(self) -> Result<RuntimeSession> {
        let root = self.root;
        let config = match self.config {
            Some(c) => c,
            None => Config::find(&root)?,
        };

        let (registry, skipped) = ProviderRegistry::from_config(&config);

        let provider_id = self
            .provider_id
            .or_else(|| config.default_target().map(|t| t.provider.clone()))
            .ok_or_else(|| {
                HarnessError::Other(
                    "no provider selected and zest.toml has no [routing].default".into(),
                )
            })?;

        let provider = match registry.get(&provider_id) {
            Some(provider) => provider,
            // Two very different failures used to share one message. Telling
            // them apart is the difference between "set a key" and "this folder
            // has no config", and the registry already worked out which.
            None => {
                return Err(HarnessError::Other(unloadable_provider(
                    &provider_id,
                    &config,
                    &skipped,
                    &root,
                )))
            }
        };

        let (remembered_model, remembered_effort) = self.remembered.unwrap_or((None, None));
        let mut warnings: Vec<String> = Vec::new();

        // A remembered model that this provider cannot serve is discarded here,
        // before it can reach validation and make the provider unreachable.
        // Cross-provider bleed is the usual cause: a Codex model left in a
        // Claude slot by an old single-provider preference file.
        let remembered_model = remembered_model.filter(|m| {
            let ok = provider.validate_selection(m, "high").is_ok()
                || provider.models().iter().any(|spec| spec.id == *m);
            if !ok {
                warnings.push(format!(
                    "ignored remembered model `{m}`: provider `{provider_id}` does not offer it"
                ));
            }
            ok
        });

        let model = self
            .model
            .or(remembered_model)
            .or_else(|| {
                config.default_target().and_then(|t| {
                    if t.provider == provider_id {
                        t.model.clone()
                    } else {
                        None
                    }
                })
            })
            // ZEST_MODEL is a global override and cannot know which provider it
            // is being applied to, so it only counts when it actually fits.
            .or_else(|| {
                std::env::var("ZEST_MODEL")
                    .ok()
                    .filter(|m| provider.models().iter().any(|spec| spec.id == *m))
            })
            .unwrap_or_else(|| provider.default_model().to_string());

        // An effort the caller passed in must fail loudly, exactly like a model
        // they passed in. Only inherited values get the soft landing.
        let effort_is_explicit = self.effort.is_some();
        let effort_source = self
            .effort
            .or(remembered_effort)
            .or_else(|| std::env::var("ZEST_EFFORT").ok())
            .unwrap_or_else(|| "high".to_string());
        let mut effort = normalize_effort(&effort_source);

        // Same reasoning as the model: an inherited value must not strand the
        // provider, because the only way to change it is a session you can no
        // longer start.
        if !effort_is_explicit && provider.validate_selection(&model, &effort).is_err() {
            let fallback = provider
                .models()
                .iter()
                .find(|spec| spec.id == model)
                .and_then(|spec| spec.efforts.first().cloned());
            if let Some(fallback) = fallback {
                warnings.push(format!(
                    "effort `{effort}` is not offered for `{model}`; using `{fallback}`"
                ));
                effort = fallback;
            }
        }

        provider
            .validate_selection(&model, &effort)
            .map_err(HarnessError::Other)?;

        let ledger = self
            .ledger
            .unwrap_or_else(|| Arc::new(Mutex::new(Ledger::load())));

        // One condition, read twice: whether `delegate` gets registered, and
        // whether the prompt is allowed to talk about it. Computing it in two
        // places would eventually let them disagree, and the failure mode is a
        // prompt describing a tool the model cannot see.
        let delegate_enabled =
            self.enable_delegate && config.routing.delegation && registry.len() > 1;

        let mut base_system = self.system.unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
        if delegate_enabled {
            base_system.push_str("\n\n");
            base_system.push_str(crate::prompt::DELEGATION_SYSTEM);
        }
        let custom = load_custom_system(&root).map_err(HarnessError::Other)?;
        let project_docs = load_project_docs(&root);
        let skills = Arc::new(RwLock::new(SkillSet::discover(&root)));
        let system = {
            let guard = skills
                .read()
                .map_err(|_| HarnessError::Other("skill registry lock poisoned".into()))?;
            let composed = compose_system_with_docs(&base_system, &custom, &project_docs, &guard);
            // Environment goes last, after everything a cache breakpoint would
            // cover. The branch name changes; the prefix above it must not.
            format!("{composed}\n\n{}", env_context(&root))
        };

        let mut worker_tools = ToolRegistry::new();
        register_read_tools(&mut worker_tools, &root)
            .map_err(|e| HarnessError::Other(format!("register read tools: {e}")))?;
        if self.register_write {
            register_write_tools(&mut worker_tools, &root)
                .map_err(|e| HarnessError::Other(format!("register write tools: {e}")))?;
        }
        register_skill_tools(&mut worker_tools, skills.clone());

        // `bash` is deliberately *not* in `worker_tools`. A delegated worker
        // runs on a different provider to think about something; letting it
        // also run shell commands widens the blast radius for no benefit that
        // the parent conversation cannot already provide.
        let mut tools = worker_tools.clone();
        if self.register_exec && config.tools.bash.enabled {
            register_exec_tools(&mut tools, &root, config.tools.bash.settings())
                .map_err(|e| HarnessError::Other(format!("register exec tools: {e}")))?;
        }

        let registry = Arc::new(registry);

        // Multi-provider routing is delegated workers only — parent stays pinned.
        // `delegate_enabled` above is the user's opt-in and defaults to false:
        // handing work to a second provider spends a second subscription, so it
        // does not switch itself on just because a second account is configured.
        if delegate_enabled {
            let kinds = config.routing.kinds();
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
        let policy = self
            .policy
            .unwrap_or_else(|| Arc::new(Mutex::new(ApprovalPolicy::default())));

        let mut agent = Agent::new(provider, tools)
            .with_system(system)
            .with_ledger(ledger.clone())
            .with_approver(approver)
            .with_policy(policy.clone());
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
            policy,
            skills,
            base_system,
            warnings,
        })
    }

    /// Resolve workspace root for callers that only have a path hint.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Explain why a selected provider is not available, and what to do about it.
fn unloadable_provider(
    provider_id: &str,
    config: &Config,
    skipped: &[Skipped],
    root: &Path,
) -> String {
    // The registry tried and failed — it knows exactly why (usually a missing
    // key env var), so quote it rather than paraphrase.
    if let Some(entry) = skipped.iter().find(|s| s.id == provider_id) {
        return format!(
            "provider `{provider_id}` is configured but could not be loaded: {}",
            entry.reason
        );
    }

    // Not in the config at all. Almost always means this folder has no
    // zest.toml and there is no user-global one either.
    let user_path = crate::config::user_config_path()
        .map(|p| crate::fsutil::display_path(&p))
        .unwrap_or_else(|| "~/.zest/zest.toml".to_string());
    let known: Vec<&str> = config.providers.keys().map(String::as_str).collect();
    let available = if known.is_empty() {
        "none are configured".to_string()
    } else {
        format!("configured here: {}", known.join(", "))
    };

    format!(
        "provider `{provider_id}` is not configured for {} ({available}). \
         Add a zest.toml to that folder, or create {user_path} so your providers \
         apply to every project.",
        crate::fsutil::display_path(root)
    )
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

    /// Two loaded providers is not consent to spend both.
    #[test]
    fn delegate_stays_unregistered_until_delegation_is_turned_on() {
        let dir = two_provider_dir("delegate-off");
        let base = std::fs::read_to_string(dir.join("zest.toml")).unwrap();

        let tools_for = |extra: &str| {
            let config = Config::parse(&format!("{base}{extra}")).unwrap();
            RuntimeBuilder::new(&dir)
                .with_config(config)
                .with_provider("codex")
                .register_write_tools(false)
                .register_exec_tools(false)
                .build()
                .unwrap()
                .agent
                .tool_names()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };

        let rule = "\n[[routing.rules]]\nkind = \"frontend\"\nprovider = \"claude\"\n";

        let off = tools_for("");
        assert!(
            !off.iter().any(|t| t == "delegate"),
            "a second account is not consent to spend it: {off:?}"
        );

        let rules_only = tools_for(rule);
        assert!(
            !rules_only.iter().any(|t| t == "delegate"),
            "rules alone are not the switch: {rules_only:?}"
        );

        // `delegation` sits under the existing [routing] table, so it has to be
        // written before the [[routing.rules]] array-of-tables stanza.
        let on = tools_for(&format!("delegation = true\n{rule}"));
        assert!(
            on.iter().any(|t| t == "delegate"),
            "opting in should register it: {on:?}"
        );
    }

    /// The prompt must not describe a tool the model cannot see, and must
    /// describe one it can. These are two reads of one condition; a test keeps
    /// them from drifting apart.
    #[test]
    fn delegation_guidance_tracks_whether_the_tool_exists() {
        let dir = two_provider_dir("delegate-prompt");
        let base = std::fs::read_to_string(dir.join("zest.toml")).unwrap();
        let rule = "\n[[routing.rules]]\nkind = \"frontend\"\nprovider = \"claude\"\n";

        let session_for = |extra: &str| {
            let config = Config::parse(&format!("{base}{extra}")).unwrap();
            RuntimeBuilder::new(&dir)
                .with_config(config)
                .with_provider("codex")
                .register_write_tools(false)
                .register_exec_tools(false)
                .build()
                .unwrap()
        };

        let off = session_for(rule);
        let off_system = off.agent.system.clone().unwrap_or_default();
        assert!(
            !off_system.contains("# Delegating"),
            "dead instructions in the cached prefix"
        );

        let on = session_for(&format!("delegation = true\n{rule}"));
        let on_system = on.agent.system.clone().unwrap_or_default();
        assert!(on_system.contains("# Delegating"), "guidance is missing");
        assert!(
            on_system.contains("same turn"),
            "the concurrency hint is the point of the paragraph"
        );
    }

    /// Reproduces the reported failure: opening a folder that has no
    /// `zest.toml` while `codex` is the selected provider.
    #[test]
    fn opening_a_folder_with_no_config_names_the_real_problem() {
        // Canonicalized, because that is what the desktop passes in and it is
        // where the `\\?\` prefix comes from on Windows.
        let dir = std::fs::canonicalize(scratch("no-config")).unwrap();
        // Guard against the assertion below quietly becoming vacuous if
        // canonicalize ever stops producing the prefix.
        #[cfg(windows)]
        assert!(
            dir.display().to_string().starts_with(r"\\?\"),
            "test no longer exercises the prefix it is checking for"
        );
        let config = Config::env_fallback();
        let (_, skipped) = ProviderRegistry::from_config(&config);

        let message = unloadable_provider("codex", &config, &skipped, &dir);

        // The old message claimed codex was "configured", which was false and
        // sent you looking for a key that was never the problem.
        assert!(
            !message.contains("is configured but"),
            "must not claim it was configured: {message}"
        );
        assert!(message.contains("not configured for"), "{message}");
        assert!(message.contains("zest.toml"), "{message}");
        // Says where to put a config that survives switching projects.
        assert!(message.contains(".zest"), "{message}");
        // A raw `\\?\` extended-length path in user-facing copy reads as
        // corruption. canonicalize() produces them, so this is easy to reintroduce.
        assert!(
            !message.contains(r"\\?\"),
            "extended-length prefix leaked into the message: {message}"
        );
    }

    #[test]
    fn a_configured_provider_missing_its_key_quotes_the_reason() {
        let dir = scratch("missing-key");
        std::env::remove_var("ZEST_TEST_UNLOADABLE_KEY");
        let config = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
api_key_env = "ZEST_TEST_UNLOADABLE_KEY"
model = "gpt-5.6-sol"
"#,
        )
        .unwrap();
        let (_, skipped) = ProviderRegistry::from_config(&config);

        let message = unloadable_provider("codex", &config, &skipped, &dir);
        assert!(message.contains("is configured but"), "{message}");
        // Naming the variable is the whole point — it is the fix.
        assert!(message.contains("ZEST_TEST_UNLOADABLE_KEY"), "{message}");
    }

    #[test]
    fn user_config_is_used_when_the_project_has_none() {
        // Providers follow the machine, not the repository: opening an
        // unrelated folder must not lose your Codex login.
        let project = scratch("bare-project");
        let home = scratch("fake-home");
        let user_dir = home.join(".zest");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(
            user_dir.join("zest.toml"),
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
model = "gpt-5.6-sol"
"#,
        )
        .unwrap();

        // `Config::find` consults the real home dir, so exercise the layering
        // through the pieces it composes rather than by moving the user's home.
        assert!(!project.join("zest.toml").is_file());
        let user = Config::load_from(user_dir.join("zest.toml")).unwrap();
        assert!(user.providers.contains_key("codex"));
        let (registry, skipped) = ProviderRegistry::from_config(&user);
        assert!(
            registry.get("codex").is_some(),
            "gateway with no api_key_env needs no key: {skipped:?}"
        );
    }

    #[test]
    fn project_config_still_wins_over_user_config() {
        let dir = scratch("project-wins");
        std::fs::write(
            dir.join("zest.toml"),
            r#"
[providers.local]
kind = "gateway"
base_url = "http://127.0.0.1:11434"
model = "llama"
"#,
        )
        .unwrap();
        let config = Config::find(&dir).unwrap();
        assert!(config.providers.contains_key("local"));
        // Whatever is in ~/.zest/zest.toml must not leak in beside it —
        // a merged provider table makes "which account pays" ambiguous.
        assert_eq!(config.providers.len(), 1);
    }

    /// Two gateway providers with disjoint model catalogues, both loadable.
    fn two_provider_dir(name: &str) -> PathBuf {
        let dir = scratch(name);
        std::env::set_var("ZEST_TEST_TWO_KEY", "present");
        let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
        writeln!(
            f,
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
api_key_env = "ZEST_TEST_TWO_KEY"
model = "gpt-5.6-sol"

[providers.claude]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
api_key_env = "ZEST_TEST_TWO_KEY"
model = "claude-opus-5"
models = ["claude-opus-5", "claude-sonnet-5"]

[routing]
default = {{ provider = "codex", model = "gpt-5.6-sol" }}
"#
        )
        .unwrap();
        dir
    }

    /// The reported failure: a Codex model left in the Claude slot by an old
    /// preference file made Claude impossible to select — and the only way to
    /// change the model was to start a session that could no longer start.
    #[test]
    fn a_stale_remembered_model_is_dropped_not_fatal() {
        let dir = two_provider_dir("stale-model");
        let session = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_provider("claude")
            .with_remembered_options(Some("gpt-5.6-luna".into()), Some("medium".into()))
            .enable_delegate(false)
            .register_write_tools(false)
            .register_exec_tools(false)
            .build()
            .expect("a stale preference must not strand the provider");

        assert_eq!(session.model, "claude-opus-5", "fell back to the default");
        assert!(
            session.warnings.iter().any(|w| w.contains("gpt-5.6-luna")),
            "the drop must be reported: {:?}",
            session.warnings
        );
    }

    /// The soft landing must not extend to an effort the caller asked for —
    /// `alpha_prove` relies on that rejection.
    #[test]
    fn a_stale_remembered_effort_falls_back_but_an_explicit_one_errors() {
        let dir = scratch("effort-split");
        std::env::set_var("ZEST_TEST_EFFORT_KEY", "present");
        let mut f = std::fs::File::create(dir.join("zest.toml")).unwrap();
        writeln!(
            f,
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_TEST_EFFORT_KEY"
model = "gpt-a"
efforts = ["low", "high"]

[routing]
default = {{ provider = "codex", model = "gpt-a" }}
"#
        )
        .unwrap();

        // Remembered: dropped, with a warning.
        let session = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_remembered_options(None, Some("max".into()))
            .enable_delegate(false)
            .register_exec_tools(false)
            .build()
            .expect("a stale effort must not strand the provider");
        assert_eq!(session.effort, "low");
        assert!(session.warnings.iter().any(|w| w.contains("max")));

        // Explicit: still an error.
        let explicit = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_effort("max")
            .enable_delegate(false)
            .register_exec_tools(false)
            .build();
        assert!(explicit.is_err(), "explicit effort must not be swallowed");
    }

    #[test]
    fn a_valid_remembered_model_is_still_honoured() {
        let dir = two_provider_dir("good-model");
        let session = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_provider("claude")
            .with_remembered_options(Some("claude-sonnet-5".into()), None)
            .enable_delegate(false)
            .register_write_tools(false)
            .register_exec_tools(false)
            .build()
            .unwrap();
        assert_eq!(session.model, "claude-sonnet-5");
        assert!(session.warnings.is_empty(), "{:?}", session.warnings);
    }

    /// The opposite treatment: something the user just picked must fail loudly
    /// rather than silently becoming a different model.
    #[test]
    fn an_explicitly_chosen_bad_model_still_errors() {
        let dir = two_provider_dir("explicit-bad");
        let built = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_provider("claude")
            .with_model("gpt-5.6-luna")
            .enable_delegate(false)
            .register_write_tools(false)
            .register_exec_tools(false)
            .build();
        let err = match built {
            Ok(session) => panic!("expected a rejection, got model {}", session.model),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("gpt-5.6-luna"), "{err}");
        assert!(err.contains("not supported"), "{err}");
    }

    /// `ZEST_MODEL` is global and cannot know which provider it lands on, so it
    /// must not strand one either.
    #[test]
    fn zest_model_env_is_ignored_for_a_provider_that_lacks_it() {
        let dir = two_provider_dir("env-model");
        std::env::set_var("ZEST_MODEL", "gpt-5.6-luna");
        let built = RuntimeBuilder::new(&dir)
            .with_config(Config::find(&dir).unwrap())
            .with_provider("claude")
            .enable_delegate(false)
            .register_write_tools(false)
            .register_exec_tools(false)
            .build();
        std::env::remove_var("ZEST_MODEL");
        assert_eq!(built.expect("must not strand").model, "claude-opus-5");
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
