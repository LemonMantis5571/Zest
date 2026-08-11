# Plan 005: Make gateway bootstrap provider-scoped

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report - do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatched you and told you they maintain
> the index.
>
> **Drift check (run first)**: `git diff --stat 81286c3..HEAD -- crates/cli/src/main.rs crates/core/src/auth.rs crates/core/src/config.rs crates/core/src/gateway.rs crates/core/src/lib.rs crates/desktop/src/lib.rs README.md context/architecture.md`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. On a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug / tech-debt / dx
- **Planned at**: commit `81286c3`, 2026-08-11

## Why this matters

Zest currently discovers and initializes the bundled CLIProxyAPI gateway before
it has loaded the selected provider configuration. This creates gateway-side
effects for direct Claude Code sessions that do not use the gateway, and it
makes the provider boundary harder to reason about. Gateway-backed providers
must keep their current managed startup behavior, while direct providers must
not adopt the sidecar, provision gateway configuration, or be judged by gateway
credentials merely because a gateway binary exists on the machine.

After this plan, gateway preparation is lazy and provider-scoped: it happens
only when the effective provider is a `ProviderConfig::Gateway` entry or when a
user explicitly requests a gateway-backed operation. Direct Claude Code,
native Anthropic, and OpenAI-compatible providers remain free of gateway
startup work. When Zest starts the gateway, it also owns that child process for
the lifetime of the relevant app/CLI instance and shuts it down on normal exit;
it never kills a gateway it found already listening or one managed outside that
instance.

## Current state

### Startup order and gateway side effects

- `crates/cli/src/main.rs:22-29` calls `adopt_bundled_gateway()` and
  `gateway_runtime()` before loading the project/user config. The same binary
  then loads the effective config at `crates/cli/src/main.rs:74-83`.
- `crates/desktop/src/lib.rs:5855-5861` repeats the unconditional adoption and
  gateway runtime calls before the Tauri app starts.
- `crates/core/src/auth.rs:573-610` makes `adopt_bundled_gateway()` discover a
  sidecar and set `ZEST_CLIPROXY_PATH`; it does not spawn the gateway, but it is
  still global process state.
- `crates/core/src/gateway.rs:116-167` resolves a gateway executable,
  provisions its config when necessary, and reconciles the client key. This is
  the write-producing side effect that must not happen for a direct provider.

### Provider selection and direct Claude

- `crates/core/src/config.rs:135-208` defines the provider variants. A
  `ClaudeCode` entry is distinct from a `Gateway` entry.
- `crates/core/src/provider/claude_code.rs:127-169` marks Claude Code as an
  agent-loop owner and invokes the `claude` executable directly. It does not
  need CLIProxyAPI.
- `crates/core/src/runtime.rs:299-370` already uses `Provider::owns_agent_loop()`
  to omit Zest tools, browser, execution, questions, and `delegate_external`
  from a provider-owned parent session. This plan must preserve that behavior.
- `crates/desktop/src/lib.rs:472-560` partly handles the direct Claude case by
  using `detect_claude_code()` when the configured entry is `ClaudeCode`, but
  `selectable` is still computed from `slot.status` at line 544 rather than the
  selected `auth_status`. A gateway installation can therefore shadow the
  direct Claude status in the picker.

### Desktop gateway readiness

- `crates/desktop/src/lib.rs:1921-1931` already identifies local supervision by
  configuration kind: only `ProviderConfig::Gateway` returns a local URL.
- `crates/desktop/src/lib.rs:1879-1889` starts the gateway only after that
  configuration check, but it currently relies on startup adoption having
  happened earlier.
- `crates/desktop/src/lib.rs:2726-2730` gates session opening with
  `uses_gateway_auth(&id)`, while `crates/core/src/auth.rs:137-139` defines that
  predicate from provider id plus global sidecar presence. The session gate
  should use the loaded provider configuration instead; provider kind is the
  authoritative boundary.

### Gateway process ownership

- `crates/core/src/gateway.rs:43-75` starts a missing local gateway from
  `ensure_running()`.
- `crates/core/src/auth.rs:494-509` implements that start through
  `spawn_detached()`. Its documented contract is to outlive the Zest process,
  with no retained `Child` handle. Today a gateway Zest starts therefore keeps
  running after Zest closes.
- `crates/core/src/gateway.rs:48-53` treats an already-listening port as
  `GatewayState::Listening` without identifying who owns that process. The fix
  must preserve that distinction: a pre-existing or hand-started gateway is
  external/shared and must not be terminated by Zest.
- `crates/desktop/tauri.conf.json:27-31` confirms the CLIProxyAPI executable is
  bundled as a Tauri sidecar in desktop releases. Lazy activation changes when
  it is discovered/provisioned/started; it does not remove the binary from the
  installer.

### Repository conventions and constraints

- `PROJECT_CONTEXT.md` defines the gateway as an implementation detail of
  provider access and says provider-specific history must remain pinned to the
  provider selected at session start.
- `context/architecture.md:113-125` explicitly separates a direct Claude Code
  parent loop from gateway providers and says provider-owned tool events must
  not enter Zest's local executor.
- Gateway support is intentionally retained. Do not remove the sidecar,
  release pin, gateway provider, credentials, or maintenance scripts in this
  plan.
- Rust source follows `Result`-based error propagation and unit tests live
  beside the implementation in `#[cfg(test)]` modules. Desktop behavior is
  tested in `crates/desktop/src/lib.rs` rather than through a webview E2E
  harness.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Drift check | `git diff --stat 81286c3..HEAD -- crates/cli/src/main.rs crates/core/src/auth.rs crates/core/src/config.rs crates/core/src/gateway.rs crates/core/src/lib.rs crates/desktop/src/lib.rs README.md context/architecture.md` | No unexpected prior changes in the in-scope paths |
| Rust tests | `cargo test --workspace --quiet` | Exit 0; all workspace tests pass |
| Rust formatting | `cargo fmt --all -- --check` | Exit 0; no formatting changes required |
| UI tests | `npm run ui:test` | Exit 0; all UI tests pass |
| UI lint | `npm run ui:lint` | Exit 0; no warnings/errors |
| UI build | `npm run ui:build` | Exit 0; production bundle builds |
| Diff hygiene | `git diff --check` | Exit 0; no whitespace errors |

## Scope

**In scope** (modify only these files unless a STOP condition applies):

- `crates/core/src/config.rs` - add a pure provider-kind predicate or equivalent
  config helper, plus unit tests.
- `crates/core/src/auth.rs`, `crates/core/src/gateway.rs`, and
  `crates/core/src/lib.rs` - adjust or remove the
  global gateway-auth/session predicate only if the front ends no longer need
  it; add explicit owned-child lifetime handling; preserve explicit gateway
  login behavior.
- `crates/cli/src/main.rs` - load the effective config before gateway
  preparation and prepare the gateway only for the selected/default gateway
  provider. Cover interactive, `run`, and `doctor --live` paths.
- `crates/desktop/src/lib.rs` - remove unconditional startup preparation,
  lazily adopt/prepare the gateway inside the gateway-specific readiness path,
  use provider configuration for session gating, and fix configured direct
  provider status/selectability.
- `crates/core/src/config.rs` / `crates/core/src/auth.rs` tests and the existing
  desktop unit-test modules - characterization and regression coverage.
- `README.md` and `context/architecture.md` - document that gateway
  provisioning is lazy and applies only to gateway-backed providers.

**Out of scope** (do not touch):

- Removing or replacing CLIProxyAPI, its Tauri sidecar, release pin, license,
  or `scripts/fetch-gateway.ps1`.
- Implementing a direct Codex CLI parent provider. That is a separate design
  decision because it changes who owns Codex tools and approvals.
- Reworking Zest's local tool registry, approval policy, or
  `Provider::owns_agent_loop()` behavior.
- Changing provider-specific thread ownership, history format, usage ledger
  semantics, or UI event protocol.
- Removing non-release PowerShell helpers is tracked separately in Plan 006.
  Do not mix script deletion, fixture migration, or developer-command changes
  into this gateway lifecycle plan.

## Steps

### Step 1: Add a pure provider-kind decision boundary

Add a small, side-effect-free helper that answers whether a configured provider
entry is gateway-backed, for example an `is_gateway()` method on
`ProviderConfig` or an equivalent `Config` helper. It must inspect only the
TOML variant; it must not inspect `ZEST_CLIPROXY_PATH`, the filesystem, auth
files, or the gateway port.

Use this helper anywhere the front ends decide whether to prepare or supervise
the gateway. Keep `local_gateway_url()`'s existing behavior for the actual
base URL, including remote gateway URLs; do not use binary presence as a proxy
for provider kind.

Add unit coverage for at least `Gateway`, `ClaudeCode`, `Anthropic`, and
`OpenaiCompatible` entries.

**Verify**: `cargo test -p zest-core --lib config` -> all config tests pass.

### Step 2: Make CLI gateway preparation lazy

In `crates/cli/src/main.rs`:

1. Keep `ensure_user_config()` and `load_env()` early because they are not
   gateway-specific.
2. Remove the unconditional calls at lines 22 and 27.
3. After each command has built its effective config (`gateway_override()` or
   `Config::find()`), resolve the effective provider id from the explicit
   `--provider` value when present, otherwise the config default target.
4. Call `adopt_bundled_gateway()` and `gateway_runtime()` only when that
   effective provider entry is `ProviderConfig::Gateway`. Preserve the existing
   warning-and-continue behavior for a gateway preparation failure.
5. Apply the same decision to interactive mode, `run --jsonl`, and
   `doctor --live`; do not duplicate the decision logic in three subtly
   different forms.
6. Preserve `ZEST_BASE_URL` behavior: its synthetic config is a gateway config,
   so it remains eligible for gateway preparation when it points at a local
   managed gateway, while direct Claude configuration never triggers it.

If the command needs gateway auth discovery for `zest auth`, keep that command
read-only: it may discover an existing sidecar to report status, but it must
not provision a config or spawn a process merely to print the provider list.

Add a focused unit-testable helper for effective-provider/gateway preparation
decision rather than testing the whole CLI through brittle subprocess output.

**Verify**: `cargo test -p zest --lib` (or the repository's available CLI test
target) -> exit 0; `rg -n "adopt_bundled_gateway|gateway_runtime" crates/cli/src/main.rs`
shows no unconditional call before config resolution.

### Step 3: Make desktop gateway supervision configuration-driven

In `crates/desktop/src/lib.rs`:

1. Remove the unconditional `adopt_bundled_gateway()` and `gateway_runtime()`
   calls from `run()`.
2. In `ensure_gateway_ready()`, when `local_gateway_url(config, id)` returns a
   gateway URL, call `adopt_bundled_gateway()` immediately before
   `ensure_gateway_running()`. This preserves bundled-sidecar discovery for the
   gateway path without touching direct-provider sessions.
3. Replace the `uses_gateway_auth(&id)` session-opening gate with a check based
   on the loaded config entry / `local_gateway_url()`. A configured
   `ClaudeCode`, `Anthropic`, or `OpenaiCompatible` provider must not enter the
   gateway readiness path even if a sidecar is installed.
4. In `provider_view_from_slot()`, use the already-computed configured
   `auth_status` when calculating `selectable`. A configured direct Claude
   provider must be selectable from its Claude Code credentials even when the
   generic provider slot reports gateway status.
5. Keep provider-list rendering side-effect free with respect to provisioning
   and process startup. If bundled-sidecar discovery is required to render a
   gateway status row, use adoption only as discovery; do not call
   `gateway_runtime()` from picker/list code.

Preserve the existing `ensure_gateway_ready()` error contract: gateway setup
failures remain `ProbeFailure::Setup`, and native/direct providers remain a
no-op there.

**Verify**: `cargo test -p zest-desktop --lib` -> all desktop unit tests pass;
`rg -n "adopt_bundled_gateway|gateway_runtime" crates/desktop/src/lib.rs`
shows preparation only in the gateway-specific path, not in `run()`.

### Step 4: Make gateway process ownership explicit

Refactor the managed gateway start path so Zest can distinguish a child it
started from a gateway that was already running:

1. Replace the current detached-only start contract used by
   `ensure_running()` with an ownership-aware result. A suitable shape is a
   `GatewayLease`/manager that records whether the process is `Owned` by this
   Zest instance or `ExternalOrShared` because the port was already listening.
2. When Zest starts the gateway, retain the child handle (or an equivalent
   ownership token plus a controllable process handle) in the CLI runtime and
   desktop app state. Do not immediately drop it through `spawn_detached()`.
3. On normal CLI termination, drop/close the lease after the interactive or
   headless turn completes. On normal Tauri exit/window shutdown, run the same
   cleanup from the app lifecycle hook. Termination must be bounded: request a
   graceful child shutdown if CLIProxyAPI supports it, then kill and await the
   child after a short timeout.
4. Never terminate a process merely because it owns port `8317`. If the port
   was already open before this instance acquired a lease, leave it running;
   this covers a hand-installed gateway, another Zest instance, and a manually
   started development process.
5. If multiple Zest instances are supported by the existing product behavior,
   use a small per-user lease/refcount or equivalent coordination so one Zest
   instance cannot shut down a gateway still leased by another. If the current
   product intentionally supports only one instance, document and test that
   constraint instead of adding PID-only termination.
6. Do not implement an idle "sleep" mode in this change. A stopped owned
   process is deterministic and releases memory/port state; an idle process can
   be considered later only with an explicit lease and wake-up design.

Keep the existing `GatewayState::NotInstalled`, `NotLocal`, and `Unavailable`
semantics. Update comments that currently promise the daemon outlives Zest, and
keep the detached process path only for an explicitly external/manual command
if a caller still needs it.

**Verify**: add a lifecycle unit/integration test using a harmless test child or
test seam -> a Zest-owned child is terminated on lease drop, while a pre-existing
listener is never terminated; `cargo test -p zest-core --lib gateway` exits 0.

### Step 5: Add regression coverage for direct-provider isolation

Add tests following the existing tests around
`only_gateway_providers_are_supervised()` and the runtime provider tests:

- Gateway config returns a local URL and remains eligible for supervision.
- Claude Code, native Anthropic, and OpenAI-compatible configs return no local
  gateway URL.
- A configured Claude Code provider uses its direct auth status for picker
  selectability instead of the generic gateway-preferred slot status.
- The CLI effective-provider helper chooses gateway preparation for a default
  gateway, skips it for an explicit direct Claude provider, and skips it for a
  native/API provider.
- A provider-owned Claude runtime still has no local Zest tools or
  `delegate_external`; this guards against accidentally solving startup by
  reattaching tools.

Where process-wide environment variables are unavoidable, follow the existing
gateway `ENV_LOCK` pattern and restore every variable before assertions return.
Prefer pure decision helpers over spawning a real gateway in normal tests.

**Verify**: `cargo test --workspace --quiet` -> all existing and new tests pass;
the gateway-spawning tests remain ignored unless explicitly requested.

### Step 6: Update runtime documentation

Update `README.md` and `context/architecture.md` so they state:

- Gateway provisioning and sidecar supervision are lazy and scoped to a
  gateway-backed provider.
- Direct Claude Code sessions do not need the gateway or gateway credentials.
- The sidecar and its maintenance scripts remain release/build infrastructure,
  not a prerequisite for direct-provider runtime use.

Do not promise direct Codex CLI support in these edits; Codex remains the
current gateway-backed provider until a separate provider design is approved.

**Verify**: `rg -n "unconditional|at startup|gateway.*direct|Claude Code.*gateway|gateway.*Claude Code" README.md context/architecture.md`
and manually inspect the changed paragraphs for consistency with the code.

## Test plan

- Unit: provider-kind predicate and effective-provider preparation decision.
- Unit: desktop `local_gateway_url()` and direct-provider selectability.
- Regression: provider-owned Claude runtime still excludes Zest's local tool
  loop and delegation.
- Regression: gateway provider still enters the existing readiness/supervision
  path and retains its current config/key behavior.
- Lifecycle: an owned gateway child is cleaned up on normal Zest exit/lease
  release, while a pre-existing or externally managed gateway remains alive.
- Manual smoke, when a real Claude Code login is available: run a direct Claude
  turn with a gateway sidecar present and verify that the turn streams normally
  without creating or modifying the gateway config; then run a gateway-backed
  provider and verify the gateway readiness path still works.

Use the existing verification commands from the table above. The manual smoke
is supplemental and must not be replaced by a fixture-only claim for the
side-effect boundary.

## Done criteria

- [x] `cargo test --workspace --quiet` exits 0.
- [x] `cargo fmt --all -- --check` exits 0.
- [x] `npm run ui:test`, `npm run ui:lint`, and `npm run ui:build` exit 0.
- [x] No CLI or desktop entrypoint unconditionally calls gateway adoption or
      runtime provisioning before provider configuration is resolved.
- [x] Direct Claude Code, native Anthropic, and OpenAI-compatible sessions do
      not prepare or supervise the local gateway.
- [x] Gateway-backed providers retain lazy sidecar discovery, provisioning, and
      readiness behavior.
- [x] A gateway child started by Zest is owned and cleaned up on normal exit;
      pre-existing/external gateway processes are never killed by port alone.
- [x] Direct Claude picker/auth status is not shadowed by gateway presence.
- [x] Provider-owned tool-loop behavior remains unchanged.
- [x] `git diff --check` exits 0 and no files outside the Scope list are
      modified.
- [x] `plans/README.md` status row is updated when implementation completes.

## STOP conditions

Stop and report instead of improvising if:

- The startup call sites or provider variants no longer match the Current state
  excerpts.
- Moving gateway preparation reveals that CLI turns currently rely on an
  undocumented process-start side effect rather than an explicit readiness
  call; do not invent a new process lifecycle without documenting it.
- The gateway process cannot be safely associated with an owning Zest instance
  without a coordination mechanism; stop and present the options instead of
  killing by PID, executable name, or port.
- A direct Claude session cannot be made selectable without changing the
  provider identity or thread/history contract.
- Correctly rendering gateway auth status requires provisioning or starting the
  sidecar; stop and propose a separate read-only status API instead.
- The fix requires changing the sidecar release/bundling contract, removing
  PowerShell scripts, implementing direct Codex, or changing local tool
  permissions.
- Any verification command fails twice after a reasonable, scoped fix attempt.

## Maintenance notes

- Future direct CLI providers, including a possible direct Codex parent, should
  use the same provider-kind boundary and must not infer gateway use from a
  provider id or the presence of a sidecar binary.
- Keep provider auth detection separate from gateway process supervision:
  credentials may be inspected for status, but starting/provisioning a process
  is a selected-provider operation.
- Keep process ownership separate from provider authentication. A valid gateway
  credential does not mean Zest owns the process, and a listening port does not
  prove who started it.
- Review startup code whenever a new front end is added. The invariant is:
  load the effective provider configuration first; prepare only the selected
  gateway-backed provider.
- Non-release PowerShell cleanup and the cross-platform developer command are
  tracked in Plan 006. Execute that follow-up after this plan so deleting the
  manual gateway helpers cannot hide a missing runtime lifecycle path.
