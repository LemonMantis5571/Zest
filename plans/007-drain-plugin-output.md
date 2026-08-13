# Plan 007: Drain plugin output without blocking the host

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatches you and maintains the index.
>
> **Drift check (run first)**: `git diff --stat 1e37803..HEAD -- crates/desktop/src/plugins.rs crates/desktop/Cargo.toml docs/PLUGINS.md`
> If any in-scope file changed since this plan was written, compare the
> Current state excerpts with the live code before proceeding.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `1e37803`, 2026-08-13

## Why this matters

The plugin host waits for an external plugin to exit before it reads the
plugin's stdout. A valid plugin response can fill the Windows pipe and make
the child wait forever, so Zest reports a timeout even though the plugin was
working. This is especially likely for artwork because the protocol permits a
512 KiB response and the sample music plugin can produce large artwork data.

## Current state

- `crates/desktop/src/plugins.rs` owns plugin discovery, manifest validation,
  process invocation, timeout handling, and untrusted metadata filtering.
- `crates/desktop/src/plugins.rs:328-404` spawns the child with piped stdin and
  stdout, writes the request, polls `child.try_wait()` until three seconds,
  and only then calls `read_to_end` on stdout.
- `crates/desktop/src/plugins.rs:26-27` defines a 512 KiB output cap and a
  three-second timeout.
- `docs/PLUGINS.md:299-314` promises a response body of at most 512 KiB and
  says plugin output is limited to that amount.
- `crates/plugins/now-playing/src/main.rs:298-320` reads artwork up to 1.5 MB
  before embedding it in the JSON response. The host still owns the final
  protocol limit; do not silently raise that limit in this plan.
- Existing tests in `crates/desktop/src/plugins.rs:481-590` cover metadata,
  path safety, manifest validation, and unsupported kinds. They do not launch
  a real child process through `invoke`.
- The process is intentionally not sandboxed. That is documented in
  `docs/PLUGINS.md` and is out of scope here.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0 |
| Focused Rust tests | `cargo test -p zest-desktop plugins` | all plugin tests pass |
| Workspace tests | `cargo test --workspace --all-targets` | exit 0, no failures |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Final verification | `npm run verify` | exit 0; existing bundle-size warning is allowed |

## Scope

**In scope**:

- `crates/desktop/src/plugins.rs`
- `crates/desktop/Cargo.toml` only if a test-only process fixture requires a
  dependency; `tempfile` is already available as a dev dependency.
- `docs/PLUGINS.md` only if the implementation changes the documented output
  or timeout semantics.
- `plans/README.md` status row.

**Out of scope**:

- Raising `MAX_PLUGIN_OUTPUT_BYTES`.
- Adding a plugin sandbox, signature system, or permission model.
- Changing the plugin protocol JSON shape.
- Changing the sample plugin's Windows media behavior.
- Refactoring unrelated functions in `crates/desktop/src/lib.rs`.

## Steps

### Step 1: Extract a bounded, concurrent stdout reader

Refactor the process code so stdout draining starts immediately after the
child is spawned, while the parent is waiting for process completion. Keep the
reader bounded to `MAX_PLUGIN_OUTPUT_BYTES + 1`; the extra byte is required to
distinguish an exactly-at-limit response from an oversized response. The
reader must not use an unbounded `read_to_end`.

Use a platform-neutral Rust thread or an equivalent non-blocking mechanism
that works in the Tauri desktop runtime. Keep the child handle available for
timeout cleanup. If the timeout fires, kill the child, wait for it, and join or
otherwise finish the reader before returning. Do not leave a child or reader
running after an error.

**Verify**: `cargo fmt --all -- --check` → exit 0.

### Step 2: Preserve validation and error behavior

Keep the existing request write, cleared environment, Windows hidden-window
flag, non-zero exit rejection, output-size rejection, JSON parsing, and
`response.ok` validation. Only change the ordering and lifecycle needed to
drain output concurrently. The host must still return the short existing
user-facing errors for timeout, invalid output, and failed startup.

**Verify**: `cargo test -p zest-desktop plugins` → all existing plugin tests pass.

### Step 3: Add process-boundary regression tests

Add a platform-appropriate test fixture or test-only child-process helper that
can exercise `invoke` without depending on an installed third-party plugin.
Cover at least:

1. a valid response that is large enough to fill a pipe but is at or below the
   documented cap;
2. output larger than the cap, which must be rejected;
3. a child that keeps stdout open past the timeout, which must be killed and
   return the timeout error;
4. non-zero exit with output, which must be rejected; and
5. a normal small response, which must parse as `NowPlayingView`.

Keep the fixture local to tests and make it work on the repository's Windows
development environment. If the fixture cannot be made cross-platform,
isolate the platform-specific test and retain a platform-neutral bounded
reader test; do not skip all coverage of the blocking behavior.

**Verify**: `cargo test -p zest-desktop plugins -- --nocapture` → all new and
existing plugin tests pass without orphaned child processes.

### Step 4: Run the full gate

Run the workspace tests, clippy, and the repository verification command.

**Verify**: `cargo test --workspace --all-targets`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `npm run verify` →
all exit 0.

## Test plan

- Model process tests after the existing inline tests in
  `crates/desktop/src/plugins.rs`; keep fixtures deterministic and local.
- Assert both successful parsing and exact user-facing rejection classes.
- The critical regression is that a child writing a pipe-filling response can
  complete successfully instead of timing out.

## Done criteria

- [ ] stdout is drained while the child is still running.
- [ ] output is capped at 512 KiB plus one detection byte.
- [ ] timeout always kills and waits for the child and does not leak a reader.
- [ ] tests cover normal, maximum-size, oversized, timeout, and non-zero exit
      responses.
- [ ] `cargo test --workspace --all-targets` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] no files outside Scope are modified.
- [ ] `plans/README.md` marks plan 007 `DONE` only after implementation.

## STOP conditions

- The `invoke` excerpt no longer matches because another agent changed its
  process lifecycle.
- The fix requires changing the public plugin protocol or increasing the
  output limit.
- A child-process test would require network access, credentials, or an
  installed plugin.
- A verification command fails twice after a focused correction.

## Maintenance notes

- Reviewers should inspect timeout cleanup and the exact boundary between
  `512 KiB` and `512 KiB + 1`.
- Any future streaming plugin protocol must not reuse this one-shot JSON
  runner without re-evaluating backpressure and cancellation.
- The no-sandbox decision remains a separate product/security direction.
