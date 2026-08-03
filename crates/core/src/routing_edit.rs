//! Editing `[routing]` in a `zest.toml` without destroying the rest of it.
//!
//! The config file is meant to be read and hand-edited: it carries the comments
//! explaining why the gateway is a transitional dependency, what the bash
//! allowlist does, which env var holds which key. Round-tripping it through
//! serde would silently delete all of that the first time someone toggles a
//! setting in Settings, so this edits the document in place with `toml_edit`
//! and touches only the keys it owns.

use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::config::{Config, Rule};

/// One routing rule as the UI hands it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleInput {
    pub kind: String,
    pub provider: String,
    /// `None` means the provider's own default model.
    pub model: Option<String>,
    /// `None` means `high`.
    pub effort: Option<String>,
    /// Extra framing prepended to this worker's system prompt.
    pub prompt: Option<String>,
}

/// Check rules against the providers actually declared in `config`.
///
/// Validation lives here rather than in the UI because the UI can be out of
/// date, and a rule naming a provider that does not exist would fail much later
/// — mid-delegation, on a turn the user already paid for.
pub fn validate_rules(config: &Config, rules: &[RuleInput]) -> Result<(), String> {
    let mut seen: Vec<(&str, &str)> = Vec::new();

    for rule in rules {
        let kind = rule.kind.trim();
        if kind.is_empty() {
            return Err("a routing rule needs a task kind".into());
        }
        // The kind reaches the model as an enum value in the tool schema, so it
        // has to survive that trip intact.
        if !kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "task kind `{kind}` must use letters, digits, `-` or `_` only"
            ));
        }

        let Some(entry) = config.providers.get(&rule.provider) else {
            let known: Vec<&str> = config.providers.keys().map(String::as_str).collect();
            return Err(format!(
                "rule `{kind}` points at unknown provider `{}` (configured: {})",
                rule.provider,
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        };

        let descriptor = crate::provider::descriptor_from_config(&rule.provider, entry);
        let model = rule
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty());
        if let Some(model) = model {
            if !descriptor.models.iter().any(|spec| spec.id == model) {
                let known: Vec<&str> = descriptor.models.iter().map(|m| m.id.as_str()).collect();
                return Err(format!(
                    "rule `{kind}`: provider `{}` does not offer model `{model}` (known: {})",
                    rule.provider,
                    known.join(", ")
                ));
            }
        }

        // Effort is per-model, so check it against the model this rule will
        // actually use rather than against the provider in general.
        if let Some(effort) = rule
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|e| !e.is_empty())
        {
            let target_model = model.unwrap_or(&descriptor.default_model);
            if let Some(spec) = descriptor.models.iter().find(|s| s.id == target_model) {
                if !spec.efforts.is_empty() && !spec.efforts.iter().any(|e| e == effort) {
                    return Err(format!(
                        "rule `{kind}`: `{target_model}` does not accept effort `{effort}` (known: {})",
                        spec.efforts.join(", ")
                    ));
                }
            }
        }

        // First match wins at resolve time, so a duplicate pair is dead config.
        // Better to reject it than to save something that never fires.
        if seen.contains(&(kind, rule.provider.as_str())) {
            return Err(format!(
                "duplicate rule for kind `{kind}` on provider `{}`",
                rule.provider
            ));
        }
        seen.push((kind, rule.provider.as_str()));
    }

    Ok(())
}

/// Apply `delegation` + `rules` to the TOML text, leaving everything else — and
/// every comment — exactly as it was.
pub fn apply_routing(
    original: &str,
    delegation: bool,
    rules: &[RuleInput],
) -> Result<String, String> {
    let mut doc: DocumentMut = original
        .parse()
        .map_err(|e| format!("cannot parse existing config: {e}"))?;

    if !doc.contains_key("routing") {
        doc["routing"] = Item::Table(Table::new());
    }
    let routing = doc["routing"]
        .as_table_mut()
        .ok_or_else(|| "[routing] is not a table".to_string())?;

    routing["delegation"] = toml_edit::value(delegation);

    // Rebuild the rule list wholesale. Rules are an ordered sequence where
    // position is meaningful (first match wins), so merging edits into the
    // existing array would be guesswork; the UI sends the whole list.
    let mut array = Array::new();
    for rule in rules {
        let mut table = toml_edit::InlineTable::new();
        table.insert("kind", Value::from(rule.kind.trim()));
        table.insert("provider", Value::from(rule.provider.trim()));
        for (key, raw) in [
            ("model", &rule.model),
            ("effort", &rule.effort),
            ("prompt", &rule.prompt),
        ] {
            if let Some(v) = raw.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                table.insert(key, Value::from(v));
            }
        }
        array.push(Value::InlineTable(table));
    }

    if array.is_empty() {
        routing.remove("rules");
    } else {
        // Written as an inline array of tables rather than `[[routing.rules]]`
        // stanzas: one array is replaceable as a unit, whereas repeated stanzas
        // would need each one found and removed.
        routing["rules"] = toml_edit::value(array);
    }

    Ok(doc.to_string())
}

/// Read a config file, apply the routing edit, and return the new text.
///
/// A missing file yields a minimal document rather than an error — the user may
/// be configuring routing before anything else exists.
pub fn routing_document(
    path: &Path,
    delegation: bool,
    rules: &[RuleInput],
) -> Result<String, String> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    apply_routing(&original, delegation, rules)
}

/// A starting set of rules for someone who has not configured routing.
///
/// The one thing this can guarantee is *structural*: every suggested rule points
/// at a provider other than the one chats start on, because a rule that lands on
/// the same account spawns a fresh worker on the same model — which looks like
/// cross-model routing in the UI but is not.
///
/// Which model is "best" or "cheapest" it cannot know: there is no pricing or
/// capability data here, only ids. The `mechanical` rule uses a name heuristic
/// (`mini`, `haiku`, `flash`) and is explicitly a starting point to adjust, not
/// a recommendation to trust.
pub fn suggest_rules(config: &Config) -> Vec<RuleInput> {
    let primary = config
        .default_target()
        .map(|t| t.provider)
        .or_else(|| config.providers.keys().next().cloned());
    let Some(primary) = primary else {
        return Vec::new();
    };

    let others: Vec<&String> = config
        .providers
        .keys()
        .filter(|id| **id != primary)
        .collect();
    let Some(secondary) = others.first() else {
        // Nowhere to delegate to; suggesting a same-provider rule would be
        // suggesting the exact mistake this function exists to avoid.
        return Vec::new();
    };

    let mut rules = vec![
        RuleInput {
            kind: "planning".into(),
            provider: (*secondary).clone(),
            model: None,
            effort: None,
            prompt: Some("Produce a plan, not code. Name real files and real functions.".into()),
        },
        RuleInput {
            kind: "review".into(),
            provider: (*secondary).clone(),
            model: None,
            effort: None,
            prompt: Some("Review for correctness. Say what is wrong, not what is fine.".into()),
        },
    ];

    // Cheap work is only cheap if it also runs at low effort — routing a
    // mechanical task to a small model and then thinking hard about it spends
    // most of what the routing saved.
    if let Some((provider, model)) = cheapest_looking(config) {
        rules.push(RuleInput {
            kind: "mechanical".into(),
            provider,
            model: Some(model),
            effort: Some("low".into()),
            prompt: Some("Make the smallest change that works. Do not refactor.".into()),
        });
    }

    rules
}

/// Best guess at a small/fast model, by name. Returns `None` rather than
/// picking arbitrarily when nothing looks small.
fn cheapest_looking(config: &Config) -> Option<(String, String)> {
    const HINTS: &[&str] = &["mini", "haiku", "flash", "small", "lite"];
    for (id, entry) in &config.providers {
        let descriptor = crate::provider::descriptor_from_config(id, entry);
        if let Some(spec) = descriptor
            .models
            .iter()
            .find(|m| HINTS.iter().any(|h| m.id.to_ascii_lowercase().contains(h)))
        {
            return Some((id.clone(), spec.id.clone()));
        }
    }
    None
}

/// Convert stored rules back into the shape the UI edits.
pub fn rules_to_input(rules: &[Rule]) -> Vec<RuleInput> {
    rules
        .iter()
        .map(|r| RuleInput {
            kind: r.kind.clone(),
            provider: r.provider.clone(),
            model: r.model.clone(),
            effort: r.effort.clone(),
            prompt: r.prompt.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"# Zest configuration.
# This comment must survive.

[providers.codex]
kind = "gateway"
base_url = "http://127.0.0.1:8317"   # origin only
model = "gpt-5.6-sol"

[providers.claude]
kind = "gateway"
base_url = "http://127.0.0.1:8317"
model = "claude-opus-5"
models = ["claude-opus-5", "claude-sonnet-5"]

[routing]
default = { provider = "codex", model = "gpt-5.6-sol" }

[tools.bash]
enabled = true
"#;

    fn rule(kind: &str, provider: &str, model: Option<&str>) -> RuleInput {
        RuleInput {
            kind: kind.into(),
            provider: provider.into(),
            model: model.map(str::to_string),
            effort: None,
            prompt: None,
        }
    }

    #[test]
    fn comments_and_unrelated_sections_survive_an_edit() {
        let out = apply_routing(
            BASE,
            true,
            &[rule("frontend", "claude", Some("claude-sonnet-5"))],
        )
        .unwrap();

        assert!(out.contains("# Zest configuration."), "{out}");
        assert!(out.contains("# This comment must survive."), "{out}");
        assert!(
            out.contains("# origin only"),
            "comment on a key was lost:\n{out}"
        );
        assert!(out.contains("[tools.bash]"), "{out}");
        assert!(out.contains("[providers.claude]"), "{out}");
        // And the default target is untouched.
        assert!(out.contains(r#"default = { provider = "codex""#), "{out}");
    }

    #[test]
    fn writes_delegation_and_rules_that_parse_back() {
        let out = apply_routing(
            BASE,
            true,
            &[
                rule("frontend", "claude", Some("claude-sonnet-5")),
                rule("planning", "codex", None),
            ],
        )
        .unwrap();

        let parsed = Config::parse(&out).expect("round-trips");
        assert!(parsed.routing.delegation);
        assert_eq!(parsed.routing.rules.len(), 2);
        assert_eq!(parsed.routing.rules[0].kind, "frontend");
        assert_eq!(parsed.routing.rules[0].provider, "claude");
        assert_eq!(
            parsed.routing.rules[0].model.as_deref(),
            Some("claude-sonnet-5")
        );
        // Omitted model stays omitted rather than becoming an empty string.
        assert_eq!(parsed.routing.rules[1].model, None);
        assert_eq!(parsed.routing.kinds(), vec!["frontend", "planning"]);
    }

    #[test]
    fn clearing_the_rules_removes_the_key_entirely() {
        let with = apply_routing(BASE, true, &[rule("frontend", "claude", None)]).unwrap();
        let without = apply_routing(&with, false, &[]).unwrap();

        let parsed = Config::parse(&without).expect("valid");
        assert!(parsed.routing.rules.is_empty());
        assert!(!parsed.routing.delegation);
        assert!(
            !without.contains("rules"),
            "stale key left behind:\n{without}"
        );
    }

    #[test]
    fn editing_twice_does_not_accumulate_rules() {
        let once = apply_routing(BASE, true, &[rule("frontend", "claude", None)]).unwrap();
        let twice = apply_routing(&once, true, &[rule("planning", "codex", None)]).unwrap();
        let parsed = Config::parse(&twice).unwrap();
        assert_eq!(parsed.routing.rules.len(), 1);
        assert_eq!(parsed.routing.rules[0].kind, "planning");
    }

    #[test]
    fn works_on_a_config_with_no_routing_section() {
        let bare =
            "[providers.codex]\nkind = \"gateway\"\nbase_url = \"http://x\"\nmodel = \"m\"\n";
        let out = apply_routing(bare, true, &[rule("frontend", "codex", None)]).unwrap();
        let parsed = Config::parse(&out).unwrap();
        assert!(parsed.routing.delegation);
        assert_eq!(parsed.routing.rules.len(), 1);
    }

    #[test]
    fn validation_rejects_an_unknown_provider() {
        let config = Config::parse(BASE).unwrap();
        let err = validate_rules(&config, &[rule("frontend", "gemini", None)]).unwrap_err();
        assert!(err.contains("unknown provider"), "{err}");
        assert!(err.contains("gemini"), "{err}");
        // Names what *is* available, so the message is actionable.
        assert!(err.contains("claude"), "{err}");
    }

    #[test]
    fn validation_rejects_a_model_the_provider_does_not_offer() {
        let config = Config::parse(BASE).unwrap();
        let err = validate_rules(&config, &[rule("frontend", "claude", Some("gpt-5.6-luna"))])
            .unwrap_err();
        assert!(err.contains("does not offer"), "{err}");
        assert!(
            err.contains("claude-opus-5"),
            "lists the real options: {err}"
        );
    }

    #[test]
    fn validation_accepts_a_real_pair_and_an_omitted_model() {
        let config = Config::parse(BASE).unwrap();
        validate_rules(
            &config,
            &[
                rule("frontend", "claude", Some("claude-sonnet-5")),
                rule("planning", "codex", None),
            ],
        )
        .expect("valid");
    }

    #[test]
    fn validation_rejects_empty_and_malformed_kinds() {
        let config = Config::parse(BASE).unwrap();
        assert!(validate_rules(&config, &[rule("  ", "codex", None)]).is_err());
        // The kind becomes a JSON-schema enum value; keep it boring.
        let err = validate_rules(&config, &[rule("front end!", "codex", None)]).unwrap_err();
        assert!(err.contains("letters, digits"), "{err}");
    }

    #[test]
    fn effort_and_prompt_round_trip() {
        let mut r = rule("frontend", "claude", Some("claude-sonnet-5"));
        r.effort = Some("low".into());
        r.prompt = Some("You write React components.".into());

        let out = apply_routing(BASE, true, &[r]).unwrap();
        let parsed = Config::parse(&out).expect("round-trips");
        let saved = &parsed.routing.rules[0];
        assert_eq!(saved.effort.as_deref(), Some("low"));
        assert_eq!(saved.prompt.as_deref(), Some("You write React components."));
    }

    #[test]
    fn omitted_effort_and_prompt_stay_absent() {
        let out = apply_routing(BASE, true, &[rule("frontend", "claude", None)]).unwrap();
        assert!(
            !out.contains("effort"),
            "empty keys should not be written:\n{out}"
        );
        assert!(!out.contains("prompt"), "{out}");
    }

    #[test]
    fn validation_rejects_an_effort_the_model_does_not_accept() {
        // The gateway declares efforts per model; `nonsense` is not one.
        let config = Config::parse(&format!(
            "{BASE}\n[providers.limited]\nkind = \"gateway\"\nbase_url = \"http://x\"\nmodel = \"m\"\nefforts = [\"low\", \"high\"]\n"
        ))
        .unwrap();
        let mut r = rule("frontend", "limited", None);
        r.effort = Some("nonsense".into());
        let err = validate_rules(&config, &[r]).unwrap_err();
        assert!(err.contains("does not accept effort"), "{err}");
        assert!(err.contains("low, high"), "lists what is allowed: {err}");
    }

    #[test]
    fn validation_accepts_a_real_effort() {
        let config = Config::parse(BASE).unwrap();
        let mut r = rule("frontend", "claude", Some("claude-sonnet-5"));
        r.effort = Some("low".into());
        validate_rules(&config, &[r]).expect("low is in the standard effort set");
    }

    #[test]
    fn suggestions_always_cross_to_another_provider() {
        // The whole point of a rule is to reach a different account; suggesting
        // one that lands back on the default would be suggesting the mistake.
        let config = Config::parse(BASE).unwrap();
        let suggested = suggest_rules(&config);
        assert!(!suggested.is_empty());
        let default_provider = config.default_target().unwrap().provider;
        for rule in &suggested {
            if rule.kind == "mechanical" {
                continue; // cheap-model rule may legitimately stay on codex
            }
            assert_ne!(
                rule.provider, default_provider,
                "rule `{}` does not cross providers",
                rule.kind
            );
        }
    }

    #[test]
    fn suggestions_validate_against_the_real_config() {
        // A preset that cannot be saved is worse than no preset.
        let config = Config::parse(BASE).unwrap();
        validate_rules(&config, &suggest_rules(&config)).expect("presets must be saveable");
    }

    #[test]
    fn a_single_provider_gets_no_suggestions() {
        let config = Config::parse(
            "[providers.codex]\nkind = \"gateway\"\nbase_url = \"http://x\"\nmodel = \"m\"\n",
        )
        .unwrap();
        assert!(
            suggest_rules(&config).is_empty(),
            "nowhere to delegate to, so nothing to suggest"
        );
    }

    #[test]
    fn a_small_model_is_picked_for_mechanical_work_at_low_effort() {
        let config = Config::parse(&format!(
            "{BASE}\n[providers.small]\nkind = \"gateway\"\nbase_url = \"http://x\"\nmodel = \"gpt-5.4-mini\"\n"
        ))
        .unwrap();
        let mechanical = suggest_rules(&config)
            .into_iter()
            .find(|r| r.kind == "mechanical")
            .expect("a small-looking model should produce a mechanical rule");
        assert_eq!(mechanical.model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(
            mechanical.effort.as_deref(),
            Some("low"),
            "a cheap model at max effort is not cheap"
        );
    }

    #[test]
    fn validation_rejects_a_duplicate_kind_provider_pair() {
        let config = Config::parse(BASE).unwrap();
        let err = validate_rules(
            &config,
            &[
                rule("frontend", "claude", None),
                rule("frontend", "claude", Some("claude-sonnet-5")),
            ],
        )
        .unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn two_providers_for_one_kind_is_allowed_as_a_fallback_chain() {
        // Not a duplicate: first match wins, so the second is the fallback.
        let config = Config::parse(BASE).unwrap();
        validate_rules(
            &config,
            &[
                rule("frontend", "claude", None),
                rule("frontend", "codex", None),
            ],
        )
        .expect("an ordered fallback chain is legitimate");
    }
}
