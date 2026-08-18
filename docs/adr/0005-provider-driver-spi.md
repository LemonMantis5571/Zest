# ADR 0005 — A driver SPI for provider construction

Status: Accepted
Date: 2026-08-18

## Context

After [ADR 0004](0004-remove-the-bundled-cliproxyapi-gateway.md) removed the
`gateway` kind, `ProviderConfig` was down to four variants — but the way they were
served was still spread out. Two independent exhaustive matches decided how a
provider behaved:

- `registry::build` constructed it, resolving credentials three different ways
  with three unnamed failure policies inline.
- `provider::descriptor_from_config` built the catalogue the picker offers.

Both read the same config entry and their answers had to agree. Nothing checked
that they did, and they had already stopped agreeing. For
`[providers.house] kind = "anthropic", model = "claude-haiku-5"`:

```
picker: [claude-haiku-5]
live:   [claude-haiku-5, claude-opus-5]
```

The descriptor built the catalogue from the configured model. The live path called
`AnthropicProvider::native(key)`, which built a catalogue for `DEFAULT_MODEL`, and
then `with_default_model("claude-haiku-5")`, which **prepended** rather than
replaced. So the picker showed one model and the runtime accepted two. This was
found by reading, then pinned as a failing test before anything was changed.

Capability was also still partly decided by a provider's *name*:
`catalogue_for_provider` matched `provider_id == "codex"` to inject the built-in
Codex catalogue. A `codex_cli` entry named anything else silently lost it; an
unrelated provider named `codex` silently gained it. Renaming a config section
changed what models were selectable.

The direction follows [t3code](https://github.com/pingdotgg/t3code)'s
`ProviderDriver` + `BUILT_IN_DRIVERS`: each driver owns its kind, its credential
requirements, its catalogue, and its construction, and a generic table dispatches.

## Decision

**One driver per kind, in `crates/core/src/provider/driver.rs`, dispatched by a
single `driver_for`.**

```rust
pub trait ProviderDriver: Send + Sync {
    fn kind(&self) -> DriverKind;                                        // == the serde tag
    fn display_name(&self) -> &'static str;
    fn credentials<'a>(&self, config: &'a ProviderConfig) -> CredentialRequest<'a>;
    fn descriptor(&self, id: &str, config: &ProviderConfig) -> ProviderDescriptor;
    fn create(&self, ctx: DriverContext<'_>, config: &ProviderConfig)
        -> Result<Arc<dyn Provider>, String>;
}
```

**`descriptor` and `create` are served by the same driver, and every
implementation derives both from one private `catalogue()` helper.** That is the
structural fix: a picker offering a model the provider rejects is no longer
expressible, rather than merely tested for.

**Credentials resolve in one place.** The driver states *where* a key lives
(`CredentialRequest { account, env, policy }`); `driver::resolve` does the reading,
credential-manager first and environment second; `resolve_required` enforces the
policy. A driver never reads a secret itself, so it cannot forget the ordering or
skip the blank-account filter. The three policies that used to be differently
shaped error handling in each match arm are now named:

| policy | meaning |
|---|---|
| `VendorOwned` | the vendor CLI owns the session; Zest never holds a key |
| `RequiredToLoad` | no key means the provider is skipped, and the reason names what to set |
| `OptionalToLoad` | a key is used when present; a keyless loopback server still loads |

**Three catalogue builders collapse into one.** `catalogue(default_model, models,
builtin, EffortPolicy)` replaces `catalogue_from_lists`,
`catalogue_without_efforts`, and `catalogue_for_provider`. `builtin` is passed in
rather than looked up, which is what removes the `provider_id == "codex"` match.

**`ProviderConfig` stays a serde-tagged enum.** t3code needs a runtime
`configSchema` because TypeScript's types erase; Rust's do not —
`#[serde(tag = "kind", deny_unknown_fields)]` already *is* that schema, checked at
the tag. Per-driver `toml::Value` decoding would reimplement it, trade compile
errors for runtime ones, and make strictness opt-in per driver: a second place to
forget, which is the failure mode this refactor exists to remove.

## Invariants

- **`driver_for` is the only match that decides construction or capability.** Its
  arms and `BUILT_IN_DRIVERS` are tied together by
  `driver_kinds_round_trip_through_the_config_tag`, since the type system cannot.
- **`DriverKind` equals the `kind = "…"` string**, so an error or label can name
  the thing the user typed.
- **A driver's `descriptor` and `create` produce the same catalogue.** Enforced
  structurally by the shared helper and checked by
  `the_picker_catalogue_matches_the_live_provider_catalogue`.
- **No capability is decided by a provider id.** The single remaining id-keyed
  function is `descriptor_for_picker_id`, which exists precisely for ids that have
  *no* config entry, hence no kind to consult.
- **A `VendorOwned` kind is never asked for a key.** A missing `claude login` is
  the CLI's to report, not a reason the provider fails to load.
- **`quota.rs` keeps its own exhaustive match, deliberately.** It produces a value
  nothing else must agree with, and being exhaustive it already fails to compile
  when a kind is added. The pair this ADR merges was different: two matches
  producing values that *had* to agree, with nothing checking.

## Alternatives

**A `DriverCapabilities` struct**, as originally planned, declaring
`owns_agent_loop`, `prompt_cache`, `anthropic_extensions`, and `resume`.
**Rejected.** Every caller of those already holds a constructed
`Arc<dyn Provider>` — `RuntimeBuilder::build` for the first, `stream_turn` for the
second — and `resume_support` has no caller at all outside its own definition. So
the struct would have been a second declaration of facts already on `Provider`,
with no reader to keep it honest: precisely the drift this ADR removes, reintroduced
in the same commit. If a caller ever needs a capability *before* construction, the
struct can be added then, with that caller as its test.

**Consolidating `quota.rs` into the driver.** Rejected: see the invariant. Moving
the match would relocate code without removing a risk, and would drag
`ProviderQuotaView` and async HTTP into a module whose job is construction.

**Per-driver config structs.** Rejected: see the tagged-enum rationale above.

## Consequences

Adding a provider is now: add a `ProviderConfig` variant, add a driver, add one
arm to `driver_for`, add a sample to the round-trip test. The honest cost is that
`config.rs` and the driver table are two files rather than one, tied by a test
rather than by the type system.

`AnthropicProvider` loses `native`, `with_id`, and `with_default_model` for a
single `new()` that takes the catalogue its driver already built. Each was used
exactly once and the sequence did wasted work — building a catalogue that was then
mutated into a different one.

A behaviour change worth naming: **a `codex_cli` provider under any id now gets the
built-in Codex catalogue.** Previously only an entry literally named `codex` did.
This is a fix, but it means a second Codex account under another name gains
selectable models it did not have before.

The desktop's `provider_method` no longer matches on `ProviderConfig` internals
from another crate; it derives its label from `display_name()` plus the credential
request. Its existing test passes unchanged, so the user-visible strings are
identical.

## Verification

`crates/core/src/provider/driver.rs`:
`driver_kinds_round_trip_through_the_config_tag`,
`the_codex_catalogue_follows_the_kind_not_the_provider_id`,
`a_required_key_names_itself_and_an_optional_one_is_allowed_to_be_absent`,
`a_vendor_owned_kind_is_never_asked_for_a_key`,
`a_blank_credential_account_falls_through_to_the_environment`.

`crates/core/src/provider/mod.rs`:
`the_picker_catalogue_matches_the_live_provider_catalogue` — written first, watched
fail with `["claude-haiku-5"]` against `["claude-haiku-5", "claude-opus-5"]`, then
made to pass by construction.

The pre-existing suites are the regression net for the rest: 605 core and 89
desktop tests pass, `cargo clippy --all-targets -- -D warnings` is clean, and
`configured_provider_methods_match_the_secret_source` was left unmodified on
purpose so the label refactor had to reproduce every string exactly.
