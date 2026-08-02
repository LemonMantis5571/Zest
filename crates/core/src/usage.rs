//! Per-provider usage accounting.
//!
//! Two numbers live here and they must never be merged, because they answer
//! different questions and have different reliability:
//!
//! | | Source | Reliability |
//! |---|---|---|
//! | **Spend** | Zest's own metering | Exact for Zest's traffic, blind to every other client on the same account |
//! | **Headroom** | The provider's response headers | Authoritative, but short-window throughput — not subscription quota |
//!
//! Subscription quota (a Claude plan, a ChatGPT plan) has no documented endpoint
//! on any of the CLI-login providers, so it is not represented here at all
//! rather than being guessed at. A figure labelled "remaining" that silently
//! excludes what another client spent an hour ago is worse than no figure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::provider::{Completion, RateLimitSnapshot};

/// What Zest itself has spent against one provider, plus the last headroom that
/// provider reported.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    /// Unix seconds. Zero means never used.
    pub first_seen: u64,
    pub last_seen: u64,
    /// Last figures the provider reported about itself. `None` means it reports
    /// nothing — which is information, not zero.
    #[serde(default)]
    pub headroom: Option<RateLimitSnapshot>,
    /// When `headroom` was captured. Throughput limits refill continuously, so a
    /// stale snapshot should be shown with its age rather than as current fact.
    #[serde(default)]
    pub headroom_at: Option<u64>,
}

impl ProviderUsage {
    /// Everything Zest sent and received. Cache reads are counted because they
    /// were still tokens the provider processed, even though they bill lower and
    /// mostly do not count against throughput limits.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_write_tokens + self.cache_read_tokens
    }

    pub fn ever_used(&self) -> bool {
        self.requests > 0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    providers: BTreeMap<String, ProviderUsage>,
    /// Where to persist. Not serialized — it is where the file is, not part of it.
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Ledger {
    /// `<data dir>/zest/usage.json`.
    ///
    /// Deliberately outside the project: an account's spend is the same account
    /// whichever repository you happen to be sitting in.
    pub fn default_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("zest").join("usage.json"))
    }

    /// Load from the default location, or start empty if there isn't one.
    pub fn load() -> Self {
        match Self::default_path() {
            Some(path) => Self::load_from(path),
            None => Self::default(),
        }
    }

    /// Load from an explicit path.
    ///
    /// A missing or unreadable file yields an empty ledger rather than an error.
    /// Usage accounting must never be the reason a session refuses to start.
    pub fn load_from(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut ledger = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Ledger>(&raw).ok())
            .unwrap_or_default();
        ledger.path = Some(path);
        ledger
    }

    /// Fold one completed turn into the running totals.
    ///
    /// Persists immediately, and ignores write failures — losing a usage figure
    /// is not worth failing a turn the user already paid for.
    pub fn record(&mut self, provider_id: &str, completion: &Completion) {
        let now = now_secs();
        let entry = self.providers.entry(provider_id.to_string()).or_default();

        if entry.first_seen == 0 {
            entry.first_seen = now;
        }
        entry.last_seen = now;
        entry.requests += 1;
        entry.input_tokens += u64::from(completion.usage.input_tokens);
        entry.output_tokens += u64::from(completion.usage.output_tokens);
        entry.cache_write_tokens += u64::from(completion.usage.cache_creation_input_tokens);
        entry.cache_read_tokens += u64::from(completion.usage.cache_read_input_tokens);

        // Only overwrite when the provider actually reported something, so a
        // gateway turn doesn't erase a real reading from a native one.
        if let Some(limits) = &completion.limits {
            entry.headroom = Some(limits.clone());
            entry.headroom_at = Some(now);
        }

        let _ = self.save();
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        fsutil::atomic_write_json(path, self)
    }

    /// Reload spend totals from disk (doctor / external writers).
    pub fn reload_from_disk(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let reloaded = Self::load_from(path);
        self.providers = reloaded.providers;
    }

    pub fn get(&self, provider_id: &str) -> Option<&ProviderUsage> {
        self.providers.get(provider_id)
    }

    /// Every provider with recorded spend, alphabetically.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &ProviderUsage)> {
        self.providers.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Snapshot for UI/CLI: measured spend vs provider-reported headroom, never merged.
    pub fn snapshot(&self) -> UsageSnapshot {
        let providers = self
            .providers
            .iter()
            .map(|(id, usage)| ProviderUsageView::from_entry(id, usage))
            .collect();
        UsageSnapshot {
            providers,
            path: self.path.as_ref().map(|p| p.display().to_string()),
        }
    }
}

/// Honest usage projection: Zest metering and provider headroom stay separate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub providers: Vec<ProviderUsageView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageView {
    pub provider_id: String,
    /// Exact for Zest's own traffic — label as "Measured by Zest".
    pub measured: MeasuredUsage,
    /// Authoritative short-window throughput when present — never a subscription balance.
    pub headroom: HeadroomView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredUsage {
    pub label: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeadroomView {
    /// Provider reported throughput headroom; `age_secs` is how stale the reading is.
    ProviderReported {
        label: String,
        age_secs: Option<u64>,
        requests_remaining: Option<u64>,
        input_tokens_remaining: Option<u64>,
        output_tokens_remaining: Option<u64>,
        retry_after_secs: Option<u64>,
    },
    NotReported {
        label: String,
    },
}

impl ProviderUsageView {
    fn from_entry(provider_id: &str, usage: &ProviderUsage) -> Self {
        let measured = MeasuredUsage {
            label: "Measured by Zest".into(),
            requests: usage.requests,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            total_tokens: usage.total_tokens(),
        };
        let headroom = match &usage.headroom {
            Some(h) if !h.is_empty() => {
                let age_secs = usage.headroom_at.map(|at| now_secs().saturating_sub(at));
                HeadroomView::ProviderReported {
                    label: "Provider reported".into(),
                    age_secs,
                    requests_remaining: h.requests_remaining,
                    input_tokens_remaining: h.input_tokens_remaining,
                    output_tokens_remaining: h.output_tokens_remaining,
                    retry_after_secs: h.retry_after_secs,
                }
            }
            _ => HeadroomView::NotReported {
                label: "Not reported".into(),
            },
        };
        Self {
            provider_id: provider_id.to_string(),
            measured,
            headroom,
        }
    }
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
    use crate::anthropic::types::Usage;

    fn completion(input: u32, output: u32, limits: Option<RateLimitSnapshot>) -> Completion {
        Completion {
            content: vec![],
            stop_reason: Some("end_turn".into()),
            usage: Usage {
                input_tokens: input,
                output_tokens: output,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            limits,
        }
    }

    #[test]
    fn accumulates_across_turns() {
        let mut ledger = Ledger::default();
        ledger.record("anthropic", &completion(100, 20, None));
        ledger.record("anthropic", &completion(50, 10, None));

        let usage = ledger.get("anthropic").expect("recorded");
        assert_eq!(usage.requests, 2);
        assert_eq!(usage.input_tokens, 150);
        assert_eq!(usage.output_tokens, 30);
        assert_eq!(usage.total_tokens(), 180);
    }

    #[test]
    fn keeps_providers_separate() {
        let mut ledger = Ledger::default();
        ledger.record("anthropic", &completion(100, 20, None));
        ledger.record("codex", &completion(7, 3, None));

        assert_eq!(ledger.get("anthropic").unwrap().input_tokens, 100);
        assert_eq!(ledger.get("codex").unwrap().input_tokens, 7);
        assert_eq!(ledger.entries().count(), 2);
    }

    #[test]
    fn a_silent_provider_does_not_erase_a_real_reading() {
        let mut ledger = Ledger::default();

        ledger.record(
            "anthropic",
            &completion(
                10,
                5,
                Some(RateLimitSnapshot {
                    requests_remaining: Some(3914),
                    ..Default::default()
                }),
            ),
        );
        // A turn through a gateway reports nothing. The earlier reading must survive.
        ledger.record("anthropic", &completion(10, 5, None));

        let usage = ledger.get("anthropic").unwrap();
        assert_eq!(
            usage.headroom.as_ref().unwrap().requests_remaining,
            Some(3914)
        );
        assert_eq!(usage.requests, 2, "spend still accumulated");
    }

    #[test]
    fn absent_headroom_stays_absent() {
        let mut ledger = Ledger::default();
        ledger.record("codex", &completion(10, 5, None));

        // None must not be flattened to a fabricated zero anywhere.
        assert!(ledger.get("codex").unwrap().headroom.is_none());
        assert!(ledger.get("codex").unwrap().headroom_at.is_none());
    }

    #[test]
    fn survives_a_save_load_round_trip() {
        let dir = std::env::temp_dir().join("zest-ledger-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("usage.json");

        let mut ledger = Ledger::load_from(&path);
        ledger.record("anthropic", &completion(100, 20, None));
        ledger.save().expect("write");

        let reloaded = Ledger::load_from(&path);
        assert_eq!(reloaded.get("anthropic").unwrap().input_tokens, 100);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_starts_empty_instead_of_failing() {
        let dir = std::env::temp_dir().join("zest-ledger-corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("usage.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        let ledger = Ledger::load_from(&path);
        assert!(ledger.is_empty());
        // ...and remains writable, so the next turn repairs it.
        assert!(ledger.path().is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_keeps_measured_and_headroom_separate() {
        let mut ledger = Ledger::default();
        ledger.record(
            "codex",
            &completion(
                10,
                5,
                Some(RateLimitSnapshot {
                    requests_remaining: Some(9),
                    ..Default::default()
                }),
            ),
        );
        ledger.record("anthropic", &completion(1, 1, None));

        let snap = ledger.snapshot();
        assert_eq!(snap.providers.len(), 2);
        let codex = snap
            .providers
            .iter()
            .find(|p| p.provider_id == "codex")
            .unwrap();
        assert_eq!(codex.measured.label, "Measured by Zest");
        assert_eq!(codex.measured.requests, 1);
        match &codex.headroom {
            HeadroomView::ProviderReported {
                label,
                requests_remaining,
                ..
            } => {
                assert_eq!(label, "Provider reported");
                assert_eq!(*requests_remaining, Some(9));
            }
            other => panic!("expected reported headroom, got {other:?}"),
        }
        let anth = snap
            .providers
            .iter()
            .find(|p| p.provider_id == "anthropic")
            .unwrap();
        match &anth.headroom {
            HeadroomView::NotReported { label } => assert_eq!(label, "Not reported"),
            other => panic!("expected not reported, got {other:?}"),
        }
    }
}
