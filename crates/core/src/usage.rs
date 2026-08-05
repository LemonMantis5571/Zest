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
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::provider::{Completion, RateLimitSnapshot};

/// Days of per-day history to keep. Enough to draw a year-long heatmap with room
/// to spare; old buckets are dropped rather than growing the file forever.
pub const DAILY_RETENTION_DAYS: usize = 400;

/// Minutes east of UTC, for deciding which day a turn belongs to.
///
/// A process global because the ledger is written from deep inside the agent
/// loop, far from anything that knows about the user's clock. Zero (UTC) is the
/// default so the CLI stays deterministic; the desktop sets the real offset at
/// startup, because a streak that resets at 6pm is worse than no streak.
static LOCAL_OFFSET_MINUTES: AtomicI32 = AtomicI32::new(0);

pub fn set_local_offset_minutes(minutes: i32) {
    // Guard against a nonsense value from the front end: real zones span
    // UTC-12..UTC+14.
    if (-12 * 60..=14 * 60).contains(&minutes) {
        LOCAL_OFFSET_MINUTES.store(minutes, Ordering::Relaxed);
    }
}

pub fn local_offset_minutes() -> i32 {
    LOCAL_OFFSET_MINUTES.load(Ordering::Relaxed)
}

/// `YYYY-MM-DD` for a unix timestamp, in the configured local zone.
///
/// ISO order is lexicographic order, which is why the daily map can be a
/// `BTreeMap<String, _>` and still iterate chronologically.
pub fn day_key(unix_secs: u64) -> String {
    day_key_from_number(local_day_number(unix_secs))
}

/// Days since the epoch, in the configured local zone.
///
/// The form to compute with: "are these two days consecutive" is subtraction on
/// this, and calendar-string arithmetic would be a bug farm.
pub fn local_day_number(unix_secs: u64) -> i64 {
    let shifted = unix_secs as i64 + i64::from(local_offset_minutes()) * 60;
    shifted.div_euclid(86_400)
}

pub fn day_key_from_number(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse a `YYYY-MM-DD` key back to a day number. `None` if it is not one.
pub fn day_number_from_key(key: &str) -> Option<i64> {
    let mut parts = key.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Calendar date to days since the epoch. Inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64; // March = 0
    let doy = (153 * mp + 2) / 5 + u64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

/// Days since the unix epoch to a calendar date.
///
/// Hinnant's civil-from-days, valid for any date in the proleptic Gregorian
/// calendar. Written out rather than pulled in: a date crate would be a new
/// dependency for one function, and this one has no configuration to get wrong.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days land at the end of the cycle.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March = 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe as i64 + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// One day's measured spend, across every provider.
///
/// Not split per provider: the question this answers is "how much did I use Zest
/// that day", and a per-provider-per-day matrix would grow the file by the
/// number of providers for a breakdown nothing asks for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayUsage {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
}

impl DayUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_write_tokens + self.cache_read_tokens
    }
}

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
    /// Spend per local day, newest last, capped at [`DAILY_RETENTION_DAYS`].
    ///
    /// Added after the per-provider totals, so an existing ledger simply starts
    /// empty here — there is no history to backfill, and inventing one would be
    /// worse than an honest gap.
    #[serde(default)]
    daily: BTreeMap<String, DayUsage>,
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
        if completion.usage_available {
            entry.input_tokens += u64::from(completion.usage.input_tokens);
            entry.output_tokens += u64::from(completion.usage.output_tokens);
            entry.cache_write_tokens += u64::from(completion.usage.cache_creation_input_tokens);
            entry.cache_read_tokens += u64::from(completion.usage.cache_read_input_tokens);
        }

        // Only overwrite when the provider actually reported something, so a
        // gateway turn doesn't erase a real reading from a native one.
        if let Some(limits) = &completion.limits {
            entry.headroom = Some(limits.clone());
            entry.headroom_at = Some(now);
        }

        let day = self.daily.entry(day_key(now)).or_default();
        day.requests += 1;
        if completion.usage_available {
            day.input_tokens += u64::from(completion.usage.input_tokens);
            day.output_tokens += u64::from(completion.usage.output_tokens);
            day.cache_write_tokens += u64::from(completion.usage.cache_creation_input_tokens);
            day.cache_read_tokens += u64::from(completion.usage.cache_read_input_tokens);
        }
        self.trim_daily();

        let _ = self.save();
    }

    /// Drop the oldest buckets past the retention window.
    ///
    /// Keys are ISO dates, so `BTreeMap` order is chronological and the oldest
    /// are simply the first ones.
    fn trim_daily(&mut self) {
        while self.daily.len() > DAILY_RETENTION_DAYS {
            let Some(oldest) = self.daily.keys().next().cloned() else {
                break;
            };
            self.daily.remove(&oldest);
        }
    }

    /// Per-day spend, keyed by ISO date so iteration is chronological. Empty for
    /// a ledger written before daily buckets existed, until the next turn.
    pub fn daily(&self) -> &BTreeMap<String, DayUsage> {
        &self.daily
    }

    /// Lifetime totals across every provider, for the headline figures.
    pub fn lifetime(&self) -> (u64, u64) {
        self.providers
            .values()
            .fold((0, 0), |(tokens, requests), p| {
                (tokens + p.total_tokens(), requests + p.requests)
            })
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
        self.daily = reloaded.daily;
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
            usage_available: true,
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

#[cfg(test)]
mod daily_tests {
    use super::*;

    /// Dates the algorithm is most likely to get wrong: epoch, leap days, and
    /// century rules. Checked against known-good calendar values.
    #[test]
    fn civil_dates_are_correct_at_the_awkward_boundaries() {
        for (days, expected) in [
            (0i64, (1970, 1, 1)),
            (-1, (1969, 12, 31)),
            (59, (1970, 3, 1)),
            // 2000 was a leap year (divisible by 400); 1900 was not.
            (11016, (2000, 2, 29)),
            (11017, (2000, 3, 1)),
            // 2100 is not a leap year, so Feb 28 is followed by Mar 1.
            (47540, (2100, 2, 28)),
            (47541, (2100, 3, 1)),
            (19723, (2024, 1, 1)),
            (20543, (2026, 3, 31)),
        ] {
            assert_eq!(civil_from_days(days), expected, "days={days}");
        }
    }

    #[test]
    fn day_keys_sort_chronologically() {
        // The retention trim and the heatmap both rely on this.
        let mut keys = vec![
            day_key(1_760_000_000),
            day_key(1_700_000_000),
            day_key(1_780_000_000),
        ];
        let original = keys.clone();
        keys.sort();
        assert_eq!(keys[0], original[1]);
        assert_eq!(keys[2], original[2]);
    }

    #[test]
    fn the_local_offset_decides_which_day_a_turn_lands_on() {
        // 2026-01-01T02:00:00Z is still 2025-12-31 in UTC-6. A user's late
        // evening belongs to their day, not to tomorrow.
        let two_am_utc = 1_767_232_800;
        set_local_offset_minutes(0);
        assert_eq!(day_key(two_am_utc), "2026-01-01");
        set_local_offset_minutes(-6 * 60);
        assert_eq!(day_key(two_am_utc), "2025-12-31");
        set_local_offset_minutes(0);
    }

    #[test]
    fn an_absurd_offset_is_refused() {
        set_local_offset_minutes(0);
        set_local_offset_minutes(99_999);
        assert_eq!(local_offset_minutes(), 0, "kept the sane value");
    }

    #[test]
    fn daily_history_is_capped() {
        let mut ledger = Ledger::default();
        for day in 0..(DAILY_RETENTION_DAYS + 25) {
            ledger
                .daily
                .insert(format!("2020-01-{day:05}"), DayUsage::default());
        }
        ledger.trim_daily();
        assert_eq!(ledger.daily.len(), DAILY_RETENTION_DAYS);
        // The oldest went, not the newest.
        assert!(ledger
            .daily
            .contains_key(&format!("2020-01-{:05}", DAILY_RETENTION_DAYS + 24)));
        assert!(!ledger.daily.contains_key("2020-01-00000"));
    }

    #[test]
    fn recording_a_turn_fills_todays_bucket() {
        let mut ledger = Ledger::default();
        ledger.record("codex", &completion_with(10, 4));
        ledger.record("codex", &completion_with(6, 1));

        let days: Vec<_> = ledger.daily().values().collect();
        assert_eq!(days.len(), 1, "same day, one bucket");
        let usage = days[0];
        assert_eq!(usage.requests, 2);
        assert_eq!(usage.total_tokens(), 21);
        // The lifetime totals still agree with the day.
        assert_eq!(ledger.get("codex").unwrap().total_tokens(), 21);
    }

    /// A ledger written before daily buckets existed must still load.
    #[test]
    fn an_older_ledger_loads_with_no_daily_history() {
        let raw = r#"{"providers":{"codex":{"requests":3,"input_tokens":10,"output_tokens":5,
            "cache_write_tokens":0,"cache_read_tokens":0,"first_seen":1,"last_seen":2}}}"#;
        let ledger: Ledger = serde_json::from_str(raw).unwrap();
        assert_eq!(ledger.get("codex").unwrap().requests, 3);
        assert!(ledger.daily().is_empty());
        assert_eq!(ledger.lifetime(), (15, 3));
    }

    fn completion_with(input: u32, output: u32) -> Completion {
        Completion {
            content: vec![],
            stop_reason: None,
            usage: crate::anthropic::types::Usage {
                input_tokens: input,
                output_tokens: output,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            usage_available: true,
            limits: None,
        }
    }
}
