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
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{HarnessError, Result};

pub const CONFIG_FILE: &str = "zest.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: Routing,
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
    },
}

impl ProviderConfig {
    pub fn key_env(&self) -> Option<&str> {
        match self {
            ProviderConfig::Anthropic { api_key_env, .. } => Some(api_key_env),
            ProviderConfig::Gateway { api_key_env, .. } => api_key_env.as_deref(),
        }
    }
}

fn default_anthropic_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    /// Where a task goes when no rule matches.
    #[serde(default)]
    pub default: Option<Target>,
    /// Consulted in order; first match wins. Used from Step 5 onward.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub provider: String,
    /// Omitted means the provider's own default.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Matched against the `kind` a delegate call declares.
    pub kind: String,
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
}

impl Config {
    /// Look for `zest.toml` in `dir`. Absent is not an error — see module note.
    pub fn find(dir: impl AsRef<Path>) -> Result<Self> {
        let path = dir.as_ref().join(CONFIG_FILE);
        if path.is_file() {
            Self::load_from(path)
        } else {
            Ok(Self::env_fallback())
        }
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
            routing: Routing {
                default: Some(Target {
                    provider: "anthropic".to_string(),
                    model: None,
                }),
                rules: Vec::new(),
            },
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
                base_url, model, ..
            } => {
                assert_eq!(base_url, "http://127.0.0.1:8317");
                assert_eq!(model, "gpt-5.3-codex");
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
    fn env_fallback_is_a_working_single_provider_config() {
        let config = Config::env_fallback();
        assert_eq!(config.default_target().unwrap().provider, "anthropic");
        assert!(config.lint().is_empty());
    }
}
