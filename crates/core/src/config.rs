//! `zest.toml` — which providers exist and where tasks go.
//!
//! Two principles shape this file:
//!
//! 1. **A missing config is not an error.** With no `zest.toml`, Zest falls back
//!    to a single Anthropic provider from the environment, which is exactly how
//!    it behaved before config existed.
//! 2. **An unusable provider is skipped, not fatal.** The whole premise is that
//!    some providers are available and some are not. One missing key must not
//!    stop the others from loading — it becomes a warning the picker can show.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{HarnessError, Result};

pub const CONFIG_FILE: &str = "zest.toml";

/// Safe starter config embedded in every build. It contains provider metadata,
/// never a credential, so a fresh install can bootstrap user-global config
/// without asking the user to copy files out of the source checkout.
pub const DEFAULT_USER_CONFIG: &str = include_str!("../../../zest.toml");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    /// Optional external coding agents invoked through a non-interactive CLI
    /// or Agent Client Protocol (ACP) stdio session. These are deliberately
    /// separate from providers: they are workers, not the identity of the
    /// parent conversation.
    #[serde(default)]
    pub agents: BTreeMap<String, ExternalAgentConfig>,
    #[serde(default)]
    pub routing: Routing,
    #[serde(default)]
    pub tools: ToolsConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsConfig {
    #[serde(default)]
    pub bash: BashConfig,
}

/// `[tools.bash]`. Absent means the defaults below, which is a working setup —
/// the tool ships on with only read-only commands running unattended.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BashConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Extra command prefixes that may run without approval, each given as its
    /// own token list (`[["just", "lint"]]`). Still subject to the shell
    /// metacharacter rule — an entry here cannot opt out of that.
    #[serde(default)]
    pub extra_allowlist: Vec<Vec<String>>,
    /// Substrings that force approval even for an otherwise allowlisted
    /// command. Checked first, so this always wins.
    #[serde(default)]
    pub denylist: Vec<String>,
    #[serde(default = "default_bash_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for BashConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extra_allowlist: Vec::new(),
            denylist: Vec::new(),
            timeout_ms: default_bash_timeout_ms(),
        }
    }
}

impl BashConfig {
    pub fn settings(&self) -> crate::tools::bash::BashSettings {
        crate::tools::bash::BashSettings {
            extra_allowlist: self.extra_allowlist.clone(),
            denylist: self.denylist.clone(),
            timeout_ms: self.timeout_ms,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_bash_timeout_ms() -> u64 {
    crate::tools::bash::DEFAULT_TIMEOUT_MS
}

/// How to reach one provider.
///
/// `kind` discriminates: `anthropic` talks to the API directly, `gateway` talks
/// to anything that re-exposes some other backend as the Messages API. That
/// distinction lives here and nowhere else — the router cannot tell them apart.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    Anthropic {
        /// Environment variable holding the key. The key itself is never written
        /// in config — this file is meant to be committed.
        #[serde(default = "default_anthropic_key_env")]
        api_key_env: String,
        #[serde(default)]
        model: Option<String>,
    },
    Gateway {
        /// Origin only — `http://127.0.0.1:8317`, not `.../v1/messages`.
        base_url: String,
        #[serde(default)]
        api_key_env: Option<String>,
        /// Required: a gateway has no sensible default model of its own.
        model: String,
        /// Optional allow-list. When empty/omitted, only `model` is accepted.
        #[serde(default)]
        models: Vec<String>,
        /// Optional effort allow-list for every listed model. When empty/omitted,
        /// the standard effort set (`low`…`max`) is used.
        #[serde(default)]
        efforts: Vec<String>,
    },
    OpenaiCompatible {
        /// API root, for example `https://api.openai.com/v1` or
        /// `https://api.deepseek.com`. The client appends `/chat/completions`.
        base_url: String,
        /// The model used when routing does not choose one explicitly.
        model: String,
        /// Optional allow-list. Empty means only `model` is accepted.
        #[serde(default)]
        models: Vec<String>,
        /// Reserved for future provider-specific effort support. The v1
        /// OpenAI-compatible adapter ignores this field and does not expose an
        /// effort selector until a wire mapping is implemented.
        #[serde(default)]
        efforts: Vec<String>,
        /// OS credential-manager account name. Defaults to the provider id.
        #[serde(default)]
        credential: Option<String>,
        /// Headless/CI fallback. Never written by Zest's setup UI.
        #[serde(default)]
        api_key_env: Option<String>,
    },
}

/// An external coding agent Zest may invoke as an explicit delegated worker.
///
/// `command` and `args` are passed directly to the operating system process
/// API; Zest never constructs a shell command. Put `{prompt}` in `args` when a
/// CLI needs the prompt at a particular position. Without it, headless mode
/// appends the prompt as the final argument. `{model}` is expanded when a model
/// is configured, and is left alone otherwise so a missing model fails clearly
/// in the child CLI rather than silently selecting a different one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalAgentConfig {
    /// `headless` consumes newline-delimited JSON from stdout. `acp` speaks
    /// JSON-RPC over stdio and lets Zest proxy the worker workspace boundary.
    #[serde(default)]
    pub mode: ExternalAgentMode,
    /// Executable name or absolute path. No shell is involved.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional label/model shown in the delegation result and available as
    /// the `{model}` argument placeholder.
    #[serde(default)]
    pub model: Option<String>,
    /// Isolated Git worktree by default. `current` is an explicit escape hatch
    /// for read-only/non-Git projects and is never selected implicitly.
    #[serde(default)]
    pub workspace: ExternalWorkspace,
    /// Child process limit. Capped by the runner to avoid a config typo making
    /// a turn wait indefinitely.
    #[serde(default = "default_external_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentMode {
    #[default]
    Headless,
    Acp,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalWorkspace {
    #[default]
    Isolated,
    Current,
}

fn default_external_timeout_secs() -> u64 {
    900
}

impl ProviderConfig {
    pub fn key_env(&self) -> Option<&str> {
        match self {
            ProviderConfig::Anthropic { api_key_env, .. } => Some(api_key_env),
            ProviderConfig::Gateway { api_key_env, .. } => api_key_env.as_deref(),
            ProviderConfig::OpenaiCompatible { api_key_env, .. } => api_key_env.as_deref(),
        }
    }
}

fn default_anthropic_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    /// Where a task goes when no rule matches.
    #[serde(default)]
    pub default: Option<Target>,
    /// Consulted in order; first match wins. Used from Step 5 onward.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Whether the model may hand work to another provider at all.
    ///
    /// **Off by default**, and off means the `delegate` tool is not registered
    /// — not merely discouraged. Spending a second subscription is not
    /// something to enable by accident, and an absent capability cannot be
    /// talked around the way a flag in the prompt can. Same reasoning as the
    /// worker registries, which structurally cannot contain `delegate`.
    #[serde(default)]
    pub delegation: bool,
}

impl Routing {
    /// The task kinds the model may declare, taken from the rules.
    ///
    /// A kind with no rule routes nowhere in particular, so the rule list *is*
    /// the vocabulary. Sorted and deduped: this becomes an enum in the tool
    /// schema, and a reordering would invalidate the prompt cache for nothing.
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.rules.iter().map(|r| r.kind.clone()).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub provider: String,
    /// Omitted means the provider's own default.
    #[serde(default)]
    pub model: Option<String>,
    /// Omitted means `high`.
    #[serde(default)]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Matched against the `kind` a delegate call declares.
    pub kind: String,
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Reasoning effort for this worker. Omitted means `high`.
    ///
    /// Worth setting: routing a mechanical task to a cheap model and then
    /// running it at maximum effort spends most of what the routing saved.
    #[serde(default)]
    pub effort: Option<String>,
    /// Extra framing prepended to the worker's system prompt.
    ///
    /// A frontend worker and a planner otherwise get identical instructions,
    /// which wastes the one thing routing by kind actually knows.
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Load `.env` from the project (searching upward), then `~/.zest/.env`.
///
/// The second one is the point: a key like `ZEST_GATEWAY_KEY` belongs to the
/// machine for the same reason the provider list does. With only the upward
/// search, opening a folder outside the Zest checkout finds no `.env` at all
/// and a correctly-configured provider fails for want of a credential.
///
/// dotenv semantics are first-wins and never clobber a variable already in the
/// environment, so a project `.env` still overrides the user one, and a real
/// environment variable overrides both.
pub fn load_env() {
    let _ = dotenvy::dotenv();
    if let Some(home) = dirs::home_dir() {
        let _ = dotenvy::from_path(home.join(".zest").join(".env"));
    }
}

/// User-global config: `~/.zest/zest.toml`.
///
/// Which accounts you are signed into is a property of the machine, not of a
/// repository. Without this, opening any folder that happens not to contain a
/// `zest.toml` would drop you back to the bare Anthropic-from-env fallback and
/// fail with "provider `codex` could not be loaded" — even though nothing about
/// your Codex login changed by opening a different directory.
pub fn user_config_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".zest").join(CONFIG_FILE))
}

/// Create the machine-level config on first launch, preserving anything the
/// user already has. A project config still takes precedence when one exists.
pub fn ensure_user_config() -> Result<Option<PathBuf>> {
    // Fail during development/build verification if the committed starter
    // config ever becomes invalid, instead of writing a broken first-run file.
    Config::parse(DEFAULT_USER_CONFIG)?;

    let Some(path) = user_config_path() else {
        return Ok(None);
    };
    if ensure_config_file(&path, DEFAULT_USER_CONFIG)? {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

fn ensure_config_file(path: &Path, contents: &str) -> Result<bool> {
    if path.is_file() {
        return Ok(false);
    }
    let parent = path.parent().ok_or_else(|| {
        HarnessError::Other(format!("config path has no parent: {}", path.display()))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|e| HarnessError::Other(format!("cannot create {}: {e}", parent.display())))?;

    // create_new is deliberate: two Zest processes racing on first launch
    // cannot replace a config that appeared between the existence check and
    // this open.
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => {
            return Err(HarnessError::Other(format!(
                "cannot create {}: {e}",
                path.display()
            )))
        }
    };

    if let Err(e) = (|| -> std::io::Result<()> {
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        file.sync_all()
    })() {
        let _ = std::fs::remove_file(path);
        return Err(HarnessError::Other(format!(
            "cannot write {}: {e}",
            path.display()
        )));
    }
    Ok(true)
}

impl Config {
    /// Look for `zest.toml` in `dir`, then `~/.zest/zest.toml`. Absent is not an
    /// error — see module note.
    ///
    /// Project config **replaces** user config rather than merging into it.
    /// Merging two provider tables would make it genuinely hard to answer
    /// "which account is this about to spend", and that question has to stay
    /// easy.
    pub fn find(dir: impl AsRef<Path>) -> Result<Self> {
        let path = dir.as_ref().join(CONFIG_FILE);
        if path.is_file() {
            return Self::load_from(path);
        }
        if let Some(user) = user_config_path().filter(|p| p.is_file()) {
            return Self::load_from(user);
        }
        Ok(Self::env_fallback())
    }

    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .map_err(|e| HarnessError::Other(format!("cannot read {}: {e}", path.display())))?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self> {
        toml::from_str(raw)
            .map_err(|e| HarnessError::Other(format!("{CONFIG_FILE} is invalid: {e}")))
    }

    /// The zero-config shape: one Anthropic provider keyed off the environment.
    pub fn env_fallback() -> Self {
        let mut providers = BTreeMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig::Anthropic {
                api_key_env: default_anthropic_key_env(),
                model: None,
            },
        );
        Config {
            providers,
            agents: BTreeMap::new(),
            routing: Routing {
                default: Some(Target {
                    provider: "anthropic".to_string(),
                    model: None,
                    effort: None,
                }),
                rules: Vec::new(),
                delegation: false,
            },
            tools: ToolsConfig::default(),
        }
    }

    /// Which provider a task goes to with no rules involved.
    ///
    /// Falls back to the only configured provider when routing is silent, so a
    /// single-provider config needs no `[routing]` section at all.
    pub fn default_target(&self) -> Option<Target> {
        if let Some(target) = &self.routing.default {
            return Some(target.clone());
        }
        if self.providers.len() == 1 {
            return self.providers.keys().next().map(|id| Target {
                provider: id.clone(),
                model: None,
                effort: None,
            });
        }
        None
    }

    /// Config problems worth showing the user that are not parse errors —
    /// dangling references that would otherwise fail much later, at dispatch.
    pub fn lint(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if let Some(target) = &self.routing.default {
            if !self.providers.contains_key(&target.provider) {
                issues.push(format!(
                    "routing.default points at unknown provider `{}`",
                    target.provider
                ));
            }
        }
        for rule in &self.routing.rules {
            if !self.providers.contains_key(&rule.provider) {
                issues.push(format!(
                    "routing rule `{}` points at unknown provider `{}`",
                    rule.kind, rule.provider
                ));
            }
        }
        for (id, agent) in &self.agents {
            if agent.command.trim().is_empty() {
                issues.push(format!("external agent `{id}` has an empty command"));
            }
            if agent.timeout_secs == 0 || agent.timeout_secs > 3_600 {
                issues.push(format!(
                    "external agent `{id}` timeout_secs must be between 1 and 3600"
                ));
            }
        }
        issues
    }
}

/// Where the config was found, for error messages.
pub fn config_path(dir: impl AsRef<Path>) -> PathBuf {
    dir.as_ref().join(CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
[providers.anthropic]
kind = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"

[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
api_key_env = "ZEST_GATEWAY_KEY"
model = "gpt-5.3-codex"

[routing]
default = { provider = "anthropic", model = "claude-opus-5" }

[[routing.rules]]
kind = "mechanical"
provider = "codex"
model = "gpt-5.3-codex"
"#;

    #[test]
    fn parses_providers_and_routing() {
        let config = Config::parse(FULL).expect("valid");

        assert_eq!(config.providers.len(), 2);
        assert!(matches!(
            config.providers["anthropic"],
            ProviderConfig::Anthropic { .. }
        ));
        match &config.providers["codex"] {
            ProviderConfig::Gateway {
                base_url,
                model,
                models,
                efforts,
                ..
            } => {
                assert_eq!(base_url, "http://127.0.0.1:8317");
                assert_eq!(model, "gpt-5.3-codex");
                assert!(models.is_empty());
                assert!(efforts.is_empty());
            }
            other => panic!("expected a gateway, got {other:?}"),
        }

        let target = config.default_target().expect("default");
        assert_eq!(target.provider, "anthropic");
        assert_eq!(target.model.as_deref(), Some("claude-opus-5"));

        assert_eq!(config.routing.rules.len(), 1);
        assert_eq!(config.routing.rules[0].kind, "mechanical");
    }

    #[test]
    fn parses_openai_compatible_provider_without_a_secret() {
        let config = Config::parse(
            r#"
[providers.deepseek]
kind = "openai_compatible"
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
models = ["deepseek-v4-flash", "deepseek-v4-pro"]
credential = "deepseek"
"#,
        )
        .expect("valid OpenAI-compatible config");
        match &config.providers["deepseek"] {
            ProviderConfig::OpenaiCompatible {
                base_url,
                model,
                credential,
                ..
            } => {
                assert_eq!(base_url, "https://api.deepseek.com");
                assert_eq!(model, "deepseek-v4-flash");
                assert_eq!(credential.as_deref(), Some("deepseek"));
            }
            other => panic!("expected OpenAI-compatible provider, got {other:?}"),
        }
    }

    #[test]
    fn parses_external_headless_and_acp_agents_without_provider_changes() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"

[agents.claude]
mode = "headless"
command = "claude"
args = [
    "--print",
    "--output-format", "stream-json",
    "--strict-mcp-config",
    "{prompt}",
]
workspace = "isolated"

[agents.gemini]
mode = "acp"
command = "gemini"
args = ["--acp"]
workspace = "current"
timeout_secs = 120
"#,
        )
        .expect("valid external agent config");

        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents["claude"].mode, ExternalAgentMode::Headless);
        assert_eq!(
            config.agents["claude"].workspace,
            ExternalWorkspace::Isolated
        );
        assert_eq!(config.agents["gemini"].mode, ExternalAgentMode::Acp);
        assert_eq!(config.agents["gemini"].timeout_secs, 120);
    }

    #[test]
    fn external_agent_defaults_are_safe() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"

[agents.claude]
command = "claude"
"#,
        )
        .unwrap();
        let agent = &config.agents["claude"];
        assert_eq!(agent.mode, ExternalAgentMode::Headless);
        assert_eq!(agent.workspace, ExternalWorkspace::Isolated);
        assert_eq!(agent.timeout_secs, 900);
    }

    #[test]
    fn a_single_provider_needs_no_routing_section() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"
"#,
        )
        .expect("valid");

        assert_eq!(config.default_target().unwrap().provider, "anthropic");
    }

    #[test]
    fn two_providers_without_a_default_is_ambiguous() {
        let config = Config::parse(
            r#"
[providers.a]
kind = "anthropic"

[providers.b]
kind = "gateway"
base_url = "http://localhost:1"
model = "m"
"#,
        )
        .expect("valid");

        // Guessing which of two accounts to spend would be the wrong kind of helpful.
        assert!(config.default_target().is_none());
    }

    #[test]
    fn lint_catches_routing_at_a_provider_that_does_not_exist() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"

[routing]
default = { provider = "typo" }

[[routing.rules]]
kind = "mechanical"
provider = "also-missing"
"#,
        )
        .expect("parses");

        let issues = config.lint();
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues[0].contains("typo"));
        assert!(issues[1].contains("also-missing"));
    }

    #[test]
    fn a_gateway_without_a_model_is_rejected() {
        let err = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("model"), "{err}");
    }

    #[test]
    fn gateway_may_list_supported_models_and_efforts() {
        let config = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
model = "gpt-5.6-sol"
models = ["gpt-5.6-sol", "gpt-5.6-terra"]
efforts = ["low", "high", "max"]
"#,
        )
        .expect("valid");
        match &config.providers["codex"] {
            ProviderConfig::Gateway {
                models, efforts, ..
            } => {
                assert_eq!(
                    models,
                    &["gpt-5.6-sol".to_string(), "gpt-5.6-terra".to_string()]
                );
                assert_eq!(
                    efforts,
                    &["low".to_string(), "high".to_string(), "max".to_string()]
                );
            }
            other => panic!("expected gateway, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_kind_is_rejected_rather_than_ignored() {
        let err = Config::parse(
            r#"
[providers.mystery]
kind = "telepathy"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("telepathy"), "{err}");
    }

    #[test]
    fn a_typo_in_a_field_name_is_rejected() {
        // deny_unknown_fields: a silently ignored `base_urls` would send traffic
        // to the wrong place with no warning.
        let err = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_urls = "http://127.0.0.1:8317"
model = "m"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("base_urls"), "{err}");
    }

    #[test]
    fn bash_defaults_to_enabled_with_no_tools_section() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"
"#,
        )
        .expect("valid");
        assert!(config.tools.bash.enabled);
        assert!(config.tools.bash.extra_allowlist.is_empty());
        assert_eq!(
            config.tools.bash.timeout_ms,
            crate::tools::bash::DEFAULT_TIMEOUT_MS
        );
    }

    #[test]
    fn bash_section_parses_allowlist_and_denylist() {
        let config = Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"

[tools.bash]
enabled = false
extra_allowlist = [["just", "lint"], ["make", "test"]]
denylist = ["cargo publish"]
timeout_ms = 5000
"#,
        )
        .expect("valid");
        let bash = &config.tools.bash;
        assert!(!bash.enabled);
        assert_eq!(bash.extra_allowlist.len(), 2);
        assert_eq!(bash.extra_allowlist[0], vec!["just", "lint"]);
        assert_eq!(bash.denylist, vec!["cargo publish".to_string()]);
        assert_eq!(bash.timeout_ms, 5000);

        // The settings a tool actually receives carry the same values.
        let settings = bash.settings();
        assert_eq!(settings.timeout_ms, 5000);
        assert_eq!(settings.denylist, vec!["cargo publish".to_string()]);
    }

    #[test]
    fn the_repo_config_parses() {
        // zest.toml is the file every fresh clone starts from; a typo in the
        // committed `[tools.bash]` block would break launch, not a test.
        let raw = include_str!("../../../zest.toml");
        let config = Config::parse(raw).expect("committed zest.toml must parse");
        assert!(config.tools.bash.enabled);
        assert!(config.lint().is_empty(), "{:?}", config.lint());
    }

    #[test]
    fn first_run_config_is_valid_and_never_overwrites_user_config() {
        let dir = std::env::temp_dir().join(format!(
            "zest-config-bootstrap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CONFIG_FILE);

        assert!(ensure_config_file(&path, DEFAULT_USER_CONFIG).unwrap());
        assert_eq!(
            Config::load_from(&path)
                .unwrap()
                .default_target()
                .unwrap()
                .provider,
            "codex"
        );
        assert!(!ensure_config_file(&path, "this must not replace the user's file").unwrap());
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("[providers.codex]"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn env_fallback_is_a_working_single_provider_config() {
        let config = Config::env_fallback();
        assert_eq!(config.default_target().unwrap().provider, "anthropic");
        assert!(config.lint().is_empty());
    }
}
