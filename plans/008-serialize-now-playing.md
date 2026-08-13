# Plan 008: Serialize and de-stale Now Playing controls

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatches you and maintains the index.
>
> **Drift check (run first)**: `git diff --stat 1e37803..HEAD -- crates/desktop/ui/src/components/NowPlayingButton.tsx crates/desktop/ui/src/lib`
> Compare the Current state excerpts with the live code before proceeding.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `1e37803`, 2026-08-13

## Why this matters

Now Playing combines five-second polling, a delayed post-control refresh, and
manual controls. These async reads can finish out of order. A stale response
can replace the result of a newer previous/next/play action, which makes the
music controls look broken even when Windows accepted the command.

## Current state

- `crates/desktop/ui/src/components/NowPlayingButton.tsx:20-29`
  `readNowPlaying` writes `value` and clears `error` whenever its request
  resolves; it has no request generation or cancellation guard.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:59-65` starts a
  `setInterval` every five seconds while the plugin is enabled.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:95-107`
  `controlNowPlaying` writes the control response and schedules an additional
  `readNowPlaying` with `window.setTimeout(..., 300)`.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:110-118` performs
  volume changes through the same `busy` flag but does not coordinate with
  reads already in flight.
- `crates/desktop/ui/src/lib/backend.ts:65-77` exposes promise-based
  `nowPlaying`, control, and volume operations. There is no abort protocol in
  the backend, so the UI must guard stale results.
- The project uses pure Node tests under `crates/desktop/ui/src/lib/*.test.ts`
  and does not have a React DOM testing harness. Prefer a small pure request
  coordinator/state helper over adding a new UI testing framework.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| UI tests | `npm run ui:test` | all tests pass |
| UI lint | `npm run ui:lint` | exit 0, no warnings |
| UI build | `npm run ui:build` | exit 0; existing large-chunk warning may remain |
| Rust check | `cargo check --workspace --all-targets` | exit 0 |
| Final verification | `npm run verify` | exit 0 |

## Scope

**In scope**:

- `crates/desktop/ui/src/components/NowPlayingButton.tsx`
- One small pure coordinator/helper under
  `crates/desktop/ui/src/lib/` if needed.
- Tests for that helper under `crates/desktop/ui/src/lib/`.
- `plans/README.md` status row.

**Out of scope**:

- The Windows plugin process protocol; plan 007 owns that.
- Changing `NowPlayingCard` layout or copy.
- Adding cancellation support to Tauri commands.
- Changing polling frequency unless it is required to avoid overlap.
- Adding a React testing framework.

## Steps

### Step 1: Define one ordering rule for reads and actions

Introduce a small pure request coordinator or equivalent refs with one clear
rule: only the latest relevant operation may commit `value` or `error`.
Starting a control or volume operation must invalidate reads that started
before it. A read started after the action must not be allowed to overwrite a
newer action result with an older request generation.

Prefer serializing backend operations where practical. If a timer or polling
read is already pending, queue it or drop it instead of letting it race with a
control. Keep the coordinator independent of React and the Tauri backend so it
can be tested with deferred promises.

**Verify**: `npm run ui:test` → existing tests pass before the component is
rewired.

### Step 2: Rewire polling, delayed refresh, and controls

Use the coordinator in `readNowPlaying`, `controlNowPlaying`, and
`changeVolume`. Replace the raw delayed callback at
`NowPlayingButton.tsx:101` with a tracked timer that is cleared on unmount,
disable, and a newer control. Polling must not create a second concurrent
read. Preserve the current `busy` behavior and all existing user-facing error
copy.

Do not make the panel disappear while a request is pending. A failed refresh
must not erase a valid last-known track unless the plugin was disabled.

**Verify**: `npm run ui:lint` and `npm run ui:build` → both exit 0.

### Step 3: Test stale-result behavior

Add pure tests using deferred promises for:

1. an old poll resolving after a next/previous action;
2. the 300 ms refresh resolving after a second action;
3. a stale error not clearing a newer successful value;
4. duplicate poll ticks while a read is pending; and
5. disable/unmount invalidating pending work.

The assertions must prove that only the newest operation commits state. Do not
test by relying on wall-clock sleeps.

**Verify**: `npm run ui:test` → all tests pass, including the new race tests.

### Step 4: Run the full gate

Run UI tests, lint, build, Rust checks, and the repository verification command.

**Verify**: `npm run verify` and `cargo check --workspace --all-targets` → exit 0.

## Test plan

- Model the new pure tests after `crates/desktop/ui/src/lib/sessionOptions.test.ts`
  and `thinkingSummary.test.ts`.
- Use manually controlled promise resolvers to make completion order explicit.
- The regression test must fail with the old implementation and pass with the
  coordinator.

## Done criteria

- [ ] No untracked `setTimeout` or polling callback can commit stale music
      state.
- [ ] At most one read/control operation is active according to the chosen
      coordinator rule.
- [ ] A valid last-known track remains visible during a refresh failure.
- [ ] Tests cover out-of-order reads, actions, errors, duplicate polls, and
      teardown.
- [ ] `npm run ui:test`, `npm run ui:lint`, `npm run ui:build`, and
      `npm run verify` exit 0.
- [ ] no files outside Scope are modified.
- [ ] `plans/README.md` marks plan 008 `DONE` only after implementation.

## STOP conditions

- The backend API has changed to support cancellation or event subscriptions;
  stop and re-scope instead of inventing a second protocol.
- The fix requires changing Windows media behavior or plugin JSON.
- The coordinator needs a new test framework to be useful.
- A verification command fails twice after a focused correction.

## Maintenance notes

- Any future event-driven media session should feed the same coordinator or
  replace polling entirely; do not add another independent timer.
- Reviewers should inspect teardown and the action/read ordering, not just the
  visible happy path.
