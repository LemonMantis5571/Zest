//! Choosing which provider serves a task.
//!
//! Resolution walks a candidate list in preference order and takes the first one
//! that is both loaded and not known to be exhausted:
//!
//! 1. a routing rule whose `kind` matches
//! 2. the configured default
//! 3. anything else that loaded, so a task still runs rather than failing
//!
//! "Exhausted" is only ever claimed on **evidence** — a provider that reports no
//! headroom is not the same as one that reports nothing. Skipping a working
//! provider because we cannot see its limits would be worse than trying it.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{Config, Rule, Target};
use crate::provider::registry::ProviderRegistry;
use crate::usage::Ledger;

#[derive(Debug, Clone)]
pub struct Resolution {
    pub target: Target,
    /// Providers passed over on the way here, with why. Worth surfacing — a
    /// silent fallback spends a different account than the user expected.
    pub skipped: Vec<(String, String)>,
}

pub struct Router {
    default: Option<Target>,
    rules: Vec<Rule>,
}

impl Router {
    pub fn from_config(config: &Config) -> Self {
        Router {
            default: config.default_target(),
            rules: config.routing.rules.clone(),
        }
    }

    /// Preference-ordered candidates for a task `kind`, most specific first.
    pub fn candidates(&self, kind: Option<&str>, registry: &ProviderRegistry) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();

        if let Some(kind) = kind {
            for rule in self.rules.iter().filter(|r| r.kind == kind) {
                out.push(Target {
                    provider: rule.provider.clone(),
                    model: rule.model.clone(),
                });
            }
        }

        if let Some(default) = &self.default {
            out.push(default.clone());
        }

        // Last resort: anything that loaded. Running the task on a second-choice
        // provider beats refusing to run it.
        for id in registry.ids() {
            out.push(Target {
                provider: id.to_string(),
                model: None,
            });
        }

        out.dedup_by(|a, b| a.provider == b.provider);
        out
    }

    pub fn resolve(
        &self,
        kind: Option<&str>,
        registry: &ProviderRegistry,
        ledger: &Ledger,
    ) -> Option<Resolution> {
        let mut skipped = Vec::new();

        for target in self.candidates(kind, registry) {
            if registry.get(&target.provider).is_none() {
                skipped.push((target.provider.clone(), "not loaded".to_string()));
                continue;
            }
            if let Some(reason) = exhausted(ledger, &target.provider) {
                skipped.push((target.provider.clone(), reason));
                continue;
            }
            return Some(Resolution { target, skipped });
        }

        None
    }
}

/// Why a provider should be passed over, if there is evidence for it.
///
/// `None` covers both "has headroom" and "reports nothing", which are treated
/// identically on purpose: absence of a reading is not a reason to skip.
fn exhausted(ledger: &Ledger, provider_id: &str) -> Option<String> {
    let usage = ledger.get(provider_id)?;
    let headroom = usage.headroom.as_ref()?;

    if headroom.requests_remaining == Some(0) {
        return Some("no requests remaining".to_string());
    }

    // retry-after only counts while it is still plausibly in effect. A snapshot
    // from an hour ago says nothing about now.
    if let (Some(wait), Some(at)) = (headroom.retry_after_secs, usage.headroom_at) {
        let elapsed = now_secs().saturating_sub(at);
        if elapsed < wait {
            return Some(format!("rate limited for another {}s", wait - elapsed));
        }
    }

    None
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::RateLimitSnapshot;

    fn registry_with(ids: &[&str]) -> ProviderRegistry {
        let mut toml = String::new();
        for id in ids {
            std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
            toml.push_str(&format!(
                "[providers.{id}]\nkind = \"gateway\"\nbase_url = \"http://127.0.0.1:1\"\napi_key_env = \"ZEST_ROUTE_TEST_KEY\"\nmodel = \"m-{id}\"\n\n"
            ));
        }
        let config = Config::parse(&toml).expect("valid");
        ProviderRegistry::from_config(&config).0
    }

    fn config_with_rule() -> Config {
        Config::parse(
            r#"
[providers.anthropic]
kind = "anthropic"
api_key_env = "ZEST_ROUTE_TEST_KEY"

[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ROUTE_TEST_KEY"
model = "gpt-5.3-codex"

[routing]
default = { provider = "anthropic", model = "claude-opus-5" }

[[routing.rules]]
kind = "mechanical"
provider = "codex"
"#,
        )
        .expect("valid")
    }

    #[test]
    fn a_matching_rule_beats_the_default() {
        std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
        let config = config_with_rule();
        let registry = ProviderRegistry::from_config(&config).0;
        let router = Router::from_config(&config);

        let hit = router
            .resolve(Some("mechanical"), &registry, &Ledger::default())
            .expect("resolved");
        assert_eq!(hit.target.provider, "codex");

        let miss = router
            .resolve(Some("something-else"), &registry, &Ledger::default())
            .expect("resolved");
        assert_eq!(miss.target.provider, "anthropic", "falls to the default");
    }

    #[test]
    fn an_unmatched_kind_uses_the_default_not_the_first_rule() {
        std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
        let config = config_with_rule();
        let registry = ProviderRegistry::from_config(&config).0;
        let router = Router::from_config(&config);

        let hit = router
            .resolve(None, &registry, &Ledger::default())
            .expect("resolved");
        assert_eq!(hit.target.provider, "anthropic");
        assert_eq!(hit.target.model.as_deref(), Some("claude-opus-5"));
    }

    #[test]
    fn an_exhausted_provider_is_passed_over_with_a_reason() {
        std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
        let config = config_with_rule();
        let registry = ProviderRegistry::from_config(&config).0;
        let router = Router::from_config(&config);

        let mut ledger = Ledger::default();
        ledger.record(
            "codex",
            &crate::provider::Completion {
                content: vec![],
                stop_reason: None,
                usage: Default::default(),
                limits: Some(RateLimitSnapshot {
                    requests_remaining: Some(0),
                    ..Default::default()
                }),
            },
        );

        let hit = router
            .resolve(Some("mechanical"), &registry, &ledger)
            .expect("still resolves");
        assert_eq!(hit.target.provider, "anthropic", "fell back");
        assert_eq!(hit.skipped.len(), 1);
        assert_eq!(hit.skipped[0].0, "codex");
        assert!(hit.skipped[0].1.contains("no requests remaining"));
    }

    #[test]
    fn a_provider_that_reports_nothing_is_not_treated_as_exhausted() {
        std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
        let config = config_with_rule();
        let registry = ProviderRegistry::from_config(&config).0;
        let router = Router::from_config(&config);

        let mut ledger = Ledger::default();
        // A gateway turn: spend recorded, no headroom reported at all.
        ledger.record(
            "codex",
            &crate::provider::Completion {
                content: vec![],
                stop_reason: None,
                usage: Default::default(),
                limits: None,
            },
        );

        let hit = router
            .resolve(Some("mechanical"), &registry, &ledger)
            .expect("resolved");
        assert_eq!(
            hit.target.provider, "codex",
            "silence is not evidence of exhaustion"
        );
        assert!(hit.skipped.is_empty());
    }

    #[test]
    fn falls_back_to_any_loaded_provider_when_config_names_a_missing_one() {
        std::env::set_var("ZEST_ROUTE_TEST_KEY", "present");
        let registry = registry_with(&["codex"]);
        let config = Config::parse(
            r#"
[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:1"
api_key_env = "ZEST_ROUTE_TEST_KEY"
model = "m"

[routing]
default = { provider = "ghost" }
"#,
        )
        .unwrap();
        let router = Router::from_config(&config);

        let hit = router
            .resolve(None, &registry, &Ledger::default())
            .expect("still runs");
        assert_eq!(hit.target.provider, "codex");
        assert_eq!(hit.skipped[0].0, "ghost");
    }

    #[test]
    fn resolves_to_nothing_when_no_provider_loaded() {
        let registry = ProviderRegistry::default();
        let config = Config::env_fallback();
        let router = Router::from_config(&config);

        assert!(router.resolve(None, &registry, &Ledger::default()).is_none());
    }
}
