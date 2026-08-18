# ADR 0004 — Remove the bundled CLIProxyAPI gateway

Status: Accepted
Date: 2026-08-18

## Context

Zest reached model providers five ways, and they disagreed on almost every axis.
Two went over HTTP (`anthropic` native, `gateway` → a bundled CLIProxyAPI
sidecar), one over a different HTTP dialect (`openai_compatible`), and two by
spawning a vendor runtime (`claude_code`, `codex_cli`). Credentials resolved
three ways with three unnamed failure policies, model catalogues were built
three ways, and capabilities were inferred from a provider's *name* —
`if id == "codex"` in `registry.rs` decided prompt caching, thinking, and effort
in one string match, so renaming a config section silently changed behaviour.

The sidecar was the most expensive part of that. Six platform binaries at
58–65 MiB each (~349 MiB) were fetched by `scripts/fetch-gateway.ps1`, pinned by
SHA256 in `gateway-release.json`, verified in `crates/desktop/build.rs` and again
in `scripts/verify-bundle.mjs`, staged by Tauri as an `externalBin`, shipped with
its own MIT notice, and supervised at runtime by `crates/core/src/gateway.rs`
(746 lines of install discovery, port probing, process spawn, and lease
ownership). Two CI jobs existed to keep that chain honest.

Meanwhile zest had already grown first-class clients for both subscriptions it
was using the proxy to reach: `ClaudeCodeProvider` spawns the `claude` CLI and
`CodexAppServerProvider` speaks the Codex app-server protocol. The proxy was
translating a subscription into the Messages API so that a provider we already
had a native path for could be reached over HTTP.

The direction, following [t3code](https://github.com/pingdotgg/t3code): drop the
proxy dependency and reach both subscriptions through the vendor runtimes.

## Decision

**Delete CLIProxyAPI entirely, and delete the `gateway` provider kind with it.**

Removed: `crates/core/src/gateway.rs`; the `cliproxy_*` install probes,
`adopt_bundled_gateway`, `gateway_auth_state` and its stub-file heuristic, and
`spawn_managed` from `auth.rs`; the CLI's `prepare_gateway`; the desktop's
`AppState.gateway`, `shutdown_gateway`, `ensure_gateway_ready`, and
`local_gateway_url`; `scripts/fetch-gateway.ps1`; `scripts/verify-bundle.mjs`;
`crates/desktop/gateway-release.json`; the six binaries and their provenance
stamps; the licence resource; the SHA256 validation in `build.rs` (and the `sha2`
build-dependency); the `externalBin`/`resources` entries in `tauri.conf.json`;
and the fetch steps in `release.yml`, `linux-verify.yml`, and
`release-verify.ps1`.

**Legacy configs migrate in memory, and nothing on disk is rewritten.**
`Config::parse` parses strict first and calls `config_migrate::migrate` only when
that fails:

| id | becomes | model / models | efforts |
|---|---|---|---|
| `codex` | `codex_cli`, `command = "codex"` | **carried** | **carried** |
| `claude` | `claude_code`, `command = "claude"` | **dropped** | dropped |
| anything else | removed, reported as unsupported | — | — |

The asymmetry is deliberate: a `codex` gateway's model ids *are* the strings the
Codex CLI accepts, while a `claude` gateway's are API ids like `claude-opus-5`,
which the Claude Code CLI does not take as aliases — carrying them would fill
the picker with entries that fail on use.

**`ZEST_BASE_URL` goes too.** Its only job was to synthesise a single-provider
`gateway` config from the environment, and there is no kind left for it to build.
`Config::from_provider_override` had no other caller.

**`AnthropicProvider` collapses to one case.** The `extensions` flag existed to
suppress `thinking` and `output_config.effort` for a proxy fronting a
non-Anthropic model. With one endpoint left, those fields are always sent and
`supports_prompt_cache` is unconditionally true. The `gateway` constructor and
`with_models` go with it.

## Invariants

- **The migration never touches the file on disk.** It rewrites a
  `toml_edit::DocumentMut` in memory and hands the string to serde. A user who
  never opens Settings keeps a working config; a user who does gets the new shape
  written by the normal `config_edit.rs` path.
- **A strict parse always runs first, and its error always wins.** Migrating for
  everyone would lose serde's span information, degrading every real typo for the
  sake of a legacy path. If the rewritten document still fails, the *original*
  error is reported — the migration is not a suspect worth naming when the real
  problem is elsewhere.
- **A `[default].model` that the migrated provider can no longer offer is
  stripped.** `RuntimeBuilder::build` runs `validate_selection` unconditionally,
  so leaving that pin in place would turn a migrated config into a hard startup
  failure rather than a warning. The provider choice itself is preserved — only
  the model is dropped, and only when that provider lost its catalogue.
- **Every migration is reported.** Notices go out through `Config::lint`, which
  all three front-ends already print, and dropped providers become `Skipped`
  entries so the picker can explain an absence rather than showing nothing.
- **Capability is decided from `kind`, never from an id.**
  `is_claude_subscription_provider` no longer accepts a provider merely *named*
  `claude`. (`catalogue_for_provider` still matches `id == "codex"` to inject the
  built-in Codex catalogue; that is the last name match, and ADR 0005's driver
  SPI is where it goes.)
- **No process listens on 8317, and no bundle contains a proxy binary.** There is
  no code path left that can start one.

## Alternatives

**Keep `ProviderConfig::Gateway` as an unmanaged endpoint.** This was the shape
after A1 and it survived one commit. It reads well — "bring your own LiteLLM" —
but it kept a fifth kind alive that overlapped `anthropic` almost entirely (same
`AnthropicProvider`, differing only in `base_url` and the extensions flag) while
carrying its own credential policy, its own catalogue path, and the `id ==
"codex"` capability match. Keeping it would have preserved exactly the divergence
this change exists to remove. If a Messages-API proxy is wanted again, the honest
shape is an optional `base_url` on the `anthropic` kind — one kind, one code
path — which is a separate, smaller change.

**Rewrite `zest.toml` on load instead of migrating in memory.** Rejected: a tool
that silently edits a committed, comment-heavy config file the first time it
starts is a bad trade for saving the user one edit. The in-memory path is
reversible by doing nothing.

**Two-stage parse for every document.** Rejected: see the invariant above.

**Migrate in `ProviderRegistry` rather than `Config::parse`.** Rejected: ~23
sites across 8 files match on `ProviderConfig` after their own `Config::find`, so
the registry is not a funnel. `Config::parse` is the single one.

## Consequences

**No subscription provider runs zest's own tool loop any more.** Both
`ClaudeCodeProvider` and `CodexAppServerProvider` return
`owns_agent_loop() == true`, and `RuntimeBuilder::build` gates the read/write/skill
tools, the browser tool, `bash`, `ask_user`, `delegate_external`, and
`delegate_feature` on `!provider_owns_agent_loop`. Before this change, a `codex`
gateway was an `AnthropicProvider` that did *not* own the loop, so the Codex
subscription drove zest's tools, approval policy, plan mode, spill store, and
delegation. After migration it drives none of them — verified live: a runtime
built from this repo's own `zest.toml` reports `tools=[]`.

That is the t3 model, and it yields a coherent story — **subscriptions run the
vendor's agent; API keys run zest's own loop** — but it is a real reduction in
what a subscription user gets, and it is the main cost of this decision.

The gain: ~349 MiB out of the installer, ~1,100 lines of supervision and
provenance code deleted, one runtime dependency and its supply-chain surface
gone, two CI steps removed, and the `context/constraints.md` "approved runtime
exception" retired rather than renegotiated. `ProviderConfig` is down to four
kinds, which is what makes the driver SPI in ADR 0005 tractable.

Claude Code approval parity had to land first (see the `claude_control.rs`
control-protocol work) — removing the proxy before that would have meant Claude
edits applying with no zest prompt and no diff.

## Verification

Automated, in `crates/core/src/config_migrate.rs`:
`a_legacy_gateway_codex_entry_migrates_to_the_codex_cli`,
`a_legacy_gateway_claude_entry_resets_its_model_list`,
`an_unrecognised_gateway_entry_is_skipped_with_a_reason_naming_the_replacement`,
`a_default_model_that_the_migrated_provider_lost_is_stripped`,
`a_legacy_routing_default_model_is_stripped_too`,
`a_codex_default_model_survives_because_codex_keeps_its_catalogue`,
`a_document_with_no_gateway_provider_is_left_for_the_strict_parser`,
`a_document_that_is_not_even_toml_is_left_for_the_strict_parser`,
`comments_and_unrelated_sections_survive_the_rewrite`.

End to end, in `crates/core/src/config.rs`:
`a_legacy_gateway_document_loads_through_parse_with_notices`,
`a_typo_still_reports_the_original_error_after_a_failed_migration`,
`a_current_document_records_no_migration`,
`every_kind_parses_to_exactly_its_own_variant`,
`codex_cli_may_list_supported_models_and_efforts`.

Manually, on a machine whose project *and* user config were both
`kind = "gateway"`: both loaded, both reported the expected notices, both built a
runtime with no warnings, and the Codex runtime registered no tools — the
consequence above, observed rather than assumed.
