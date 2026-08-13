# Plan 009: Make provider quota checks lazy, cached, and parallel

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatches you and maintains the index.
>
> **Drift check (run first)**: `git diff --stat 1e37803..HEAD -- crates/core/src/quota.rs crates/desktop/src/lib.rs crates/desktop/ui/src/components/AgentQuotaButton.tsx crates/desktop/ui/src/components/TopbarPanel.tsx`
> Compare the Current state excerpts with the live code before proceeding.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `1e37803`, 2026-08-13

## Why this matters

Quota data should be useful when the user opens the quota panel, not a hidden
startup task that can wait on several providers. The current implementation
queries every configured provider on mount, sequentially, with up to eight
seconds per external check. It also repeats the provider check when the chat
message count changes. Lazy loading, a short cache, and parallel independent
checks make the panel responsive while preserving the rule that Zest only
shows provider-reported values.

## Current state

- `crates/core/src/quota.rs:22-23` sets eight-second HTTP and Codex query
  timeouts.
- `crates/core/src/quota.rs:89-137` loops through `config.providers` and
  awaits each check before starting the next one. Codex uses a local
  `codex app-server`; Claude reads Claude Desktop's shared local snapshot;
  DeepSeek uses its official balance endpoint; unsupported providers return a
  deliberate unavailable view.
- `crates/desktop/src/lib.rs:1811-1814` exposes the provider quota command.
- `crates/desktop/ui/src/components/AgentQuotaButton.tsx:21-39` refreshes the
  local usage snapshot whenever `refreshKey` changes.
- `crates/desktop/ui/src/components/AgentQuotaButton.tsx:41-58` calls
  `providerQuota()` on component mount and whenever the refresh button nonce
  changes. It does not wait for the panel to open and has no TTL cache.
- `crates/desktop/ui/src/components/ChatScreen.tsx:942` passes
  `${threadId}:${messages.length}` as the refresh key, so every new message
  can cause another local usage request while the provider quota remains
  independently eager.
- `crates/desktop/ui/src/components/TopbarPanel.tsx:13-17` already provides an
  `onOpenChange` callback that can trigger an on-demand fetch.
- `docs/QUOTA.md` requires provider evidence, no guessed remaining number, no
  OAuth scraping, and an age for Claude Desktop's shared snapshot. Preserve
  those constraints.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core quota tests | `cargo test -p zest-core quota` | all quota tests pass |
| Desktop tests | `cargo test -p zest-desktop --lib` | exit 0 |
| UI tests | `npm run ui:test` | all tests pass |
| Lint/build | `npm run ui:lint && npm run ui:build` | both exit 0 |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Final verification | `npm run verify` | exit 0 |

## Scope

**In scope**:

- `crates/core/src/quota.rs`
- `crates/desktop/ui/src/components/AgentQuotaButton.tsx`
- A small pure quota-cache helper and tests under
  `crates/desktop/ui/src/lib/`, if useful.
- `crates/desktop/ui/src/components/TopbarPanel.tsx` only if its open-change
  contract needs a small correctness adjustment.
- `plans/README.md` status row.

**Out of scope**:

- Reading provider OAuth files, browser storage, or private web endpoints.
- Claiming quota for providers that do not report it.
- Changing the Claude Desktop cache path or its 24-hour staleness rule.
- Changing provider credentials or the configured provider model.
- Redesigning the quota panel; plan 011 owns topbar layout.

## Steps

### Step 1: Make independent provider checks concurrent

Refactor `fetch_provider_quotas` so independent provider checks can run at the
same time, while the returned `providers` vector remains in the deterministic
`config.providers` order. Use the repository's existing async/futures
dependencies; do not create unbounded tasks. Keep each provider timeout and
the existing official-endpoint allowlist intact.

If a provider is unsupported, return its unavailable view immediately without
starting network or child-process work. If one provider fails, retain the
results from the others.

**Verify**: `cargo test -p zest-core quota` → all existing quota parsing and
staleness tests pass.

### Step 2: Fetch provider quota only when requested

Change `AgentQuotaButton` so mounting the topbar does not call
`providerQuota()`. Use `TopbarPanel`'s `onOpenChange(true)` to fetch the first
snapshot, and keep the current snapshot visible while a refresh is running.
The manual refresh button must still force a new check.

Keep `usageSnapshot()` separate: its local usage refresh may continue to follow
`refreshKey`, but it must not trigger a provider network/process check.

**Verify**: `npm run ui:test` and `npm run ui:lint` → exit 0.

### Step 3: Add a bounded TTL cache and clear loading semantics

Add a small cache policy, preferably a five-minute TTL, for the provider
snapshot while the component remains mounted. A panel reopen inside the TTL
should reuse the last result; the refresh button bypasses the TTL. Store the
`checkedAt`/age already returned by the backend and retain the last good result
when a refresh fails, while showing a short error/status message.

Do not display a made-up remaining quota. Preserve distinctions between
reported data, unavailable checks, stale Claude Desktop data, and request
errors. Keep copy short and non-technical, following `memory/recurring-corrections.md`.

Extract pure cache decision logic if necessary so time-based behavior can be
tested without sleeps.

**Verify**: `npm run ui:build` → exit 0; the emitted large-chunk warning may
remain because it is already documented.

### Step 4: Test the new request policy

Add tests for:

1. mount without opening the panel makes no provider quota call;
2. first open makes one call;
3. reopening inside the TTL reuses the snapshot;
4. manual refresh bypasses the TTL;
5. a failed refresh preserves last good data and exposes the error state; and
6. unsupported providers remain unavailable rather than guessed.

For the Rust side, retain or extend parsing tests for Codex's `rateLimits` and
`rateLimitsByLimitId`, Claude Desktop staleness, and independent provider
failure. Do not add tests that require real credentials.

**Verify**: `cargo test -p zest-core quota`, `cargo test -p zest-desktop --lib`,
and `npm run ui:test` → all pass.

### Step 5: Run the full gate

Run clippy, UI lint/build, and repository verification.

**Verify**: `cargo clippy --workspace --all-targets -- -D warnings` and
`npm run verify` → exit 0.

## Test plan

- Model pure UI tests after `crates/desktop/ui/src/lib/sessionOptions.test.ts`.
- Keep fake backend call counters in the existing fixture backend pattern if a
  UI helper needs them.
- Assert provider ordering explicitly after concurrency refactoring.
- Avoid exposing or logging secrets while testing Codex, Claude, or DeepSeek.

## Done criteria

- [ ] No provider quota check runs merely because the chat topbar mounted.
- [ ] Independent provider checks run concurrently and preserve output order.
- [ ] A five-minute cache avoids repeated checks on panel reopen.
- [ ] Manual refresh bypasses the cache and preserves last good data on failure.
- [ ] No unsupported provider is represented with a fabricated number.
- [ ] Tests cover lazy load, TTL, refresh, errors, provider ordering, and stale
      provider data.
- [ ] All commands in the Commands table pass.
- [ ] no files outside Scope are modified.
- [ ] `plans/README.md` marks plan 009 `DONE` only after implementation.

## STOP conditions

- A provider requires a private endpoint, OAuth token, browser scraping, or a
  new credential source to become “reported.”
- Parallelization changes the public DTO shape or provider ordering.
- The backend needs global persistent quota storage beyond this plan's local
  TTL; stop and re-scope that storage decision.
- A verification command fails twice after a focused correction.

## Maintenance notes

- New provider adapters must declare whether they are official, local-cache,
  or unsupported; never infer a quota from Zest's own usage ledger.
- Revisit the five-minute TTL if a provider later supplies push updates.
- Reviewers should verify that Claude Desktop and Claude Code continue to be
  described as sharing the same Claude.ai allowance.
