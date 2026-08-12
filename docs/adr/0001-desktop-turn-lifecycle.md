# ADR 0001: Deepen the desktop turn lifecycle

- Status: Accepted
- Date: 2026-08-12

## Context

The desktop command that handles a user message currently coordinates input
normalization, turn ownership, transcript events, provider execution,
approval/question routing, delegation, cancellation, and durable persistence.
Those responsibilities form one coherent domain operation, but they are
implemented in the Tauri command module. That makes the lifecycle difficult to
test without constructing Tauri state and makes changes to event ordering or
cleanup easy to miss.

The architecture review identified the active turn path as the highest-leverage
seam. Session opening and recovery, the provider adapter, and external worker
execution remain later candidates and are intentionally outside this change.

## Decision

Extract the active turn lifecycle behind a private `zest-desktop` turn module
with one high-level operation. The existing Tauri command remains a thin
adapter. `SessionController` remains the authority for session and active-turn
ownership; the extracted operation consumes that authority rather than creating
a second state model.

The module will project provider activity into the existing typed `ChatEvent`
surface through a small event-sink seam. Production uses a Tauri-backed sink;
tests use an in-memory recorder. The module constructs `ChatEvent` values and
does not expose raw provider stream events.

The module owns the lifecycle ordering: transcript state is persisted before a
successful terminal lifecycle is recorded, and every exit clears approval and
question state and returns ownership through `SessionController`. Existing
warning, cancellation, approval/question cleanup, provider-error, delegation,
event-name, payload, and persisted-thread behavior remain compatible.

## Invariants

- A turn has one owner from `begin_turn` through `finish_turn`.
- User and assistant-start events are emitted before provider progress events.
- Provider events are projected into typed chat events at the desktop boundary.
- Approval and question requests are resolved or cleared for the same turn.
- Transcript persistence precedes a successful terminal lifecycle mark.
- Cancellation and provider failure still produce the existing terminal events.

## Alternatives considered

- Leave the command monolithic: lowest immediate change, but preserves the
  shallow interface and makes lifecycle behavior hard to test.
- Split into several lifecycle methods: rejected because it would expose
  intermediate states and make ownership/cleanup the caller's responsibility.
- Change `zest-core` or the public desktop API: rejected because the current
  seam is sufficient and compatibility is an explicit constraint.

## Consequences

The desktop module gains a deeper interface around one meaningful operation,
with a replaceable event sink and focused tests. The command becomes easier to
read and future lifecycle changes become local. The implementation still
reuses the existing persistence and session authorities, so this refactor does
not change the underlying domain model or storage format.

## Verification

Focused tests cover the new event seam and the existing desktop
characterization tests remain in place for transcript projection,
approval/question resolution, and cancellation behavior. The implementation
was checked with:

- `cargo fmt --all -- --check`
- `cargo check -p zest-desktop --lib -j 1`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --lib --no-run`
- `npm run verify` through UI tests, UI lint, UI build, Rust formatting, and
  Rust clippy

The Rust test executables compile but cannot be launched in the current
Windows environment: the host returns `Access is denied (os error 5)` for the
generated `.exe` files. This is an execution-policy limitation, not a test
assertion failure; the final handoff keeps it separate from the passing build
and static checks.
