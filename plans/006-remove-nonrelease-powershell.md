# Plan 006: Remove non-release PowerShell noise

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report - do not improvise. When done, update the status row for this plan
> in `plans/README.md` unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Dependency**: Execute Plan 005 first. This plan removes manual gateway
> entrypoints only after Zest has an explicit provider-scoped gateway
> lifecycle.
>
> **Drift check (run first)**: `git diff --stat 81286c3..HEAD -- package.json README.md CONTRIBUTING.md context/project-overview.md docs/RELEASING.md docs/CODEBASE_AUDIT.md .github/workflows/windows-verify.yml .github/workflows/linux-verify.yml scripts crates/core/tests/fixtures crates/core/src/tools/external_agent.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. On a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/005-conditional-gateway-bootstrap.md`
- **Category**: tech-debt / dx / tests / docs
- **Planned at**: commit `81286c3`, 2026-08-11

## Why this matters

The repository currently mixes three different concerns in PowerShell files:
release/CI packaging, manual gateway operation, and test fixtures. The manual
gateway scripts are obsolete once Zest manages the gateway lifecycle, while
the fixture scripts and the root `npm run verify` wrapper make ordinary
development appear to require PowerShell. Removing only the obsolete helpers,
replacing test fixtures with a cross-platform Rust helper, and keeping release
PowerShell clearly named preserves release coverage without adding shell noise
to the runtime or local development path.

## Current state

### PowerShell files and their intended disposition

The tracked PowerShell inventory is:

- `scripts/build-signed.ps1` - release signing; keep.
- `scripts/fetch-gateway.ps1` - release sidecar fetch/provenance; keep.
- `scripts/make-installer-art.ps1` - release installer assets; keep.
- `scripts/release-checksums.ps1` - release artifact checksums; keep.
- `scripts/verify.ps1` - full CI/release gate; keep as a release-only script,
  but rename it to `scripts/release-verify.ps1` so its role is explicit.
- `scripts/start-gateway.ps1:1-47` - manually starts a gateway process and
  tells users to stop it by process name; remove after Plan 005.
- `scripts/codex-login-gateway.ps1:1-18` - manually launches the gateway's
  Codex OAuth flow; remove after the app/CLI auth path is authoritative.
- `crates/core/tests/fixtures/external_agent_smoke.ps1`,
  `external_agent_stream_smoke.ps1`, and `external_agent_acp_smoke.ps1` - test
  children, not product functionality; replace with one cross-platform Rust
  fixture helper rather than dropping test coverage.

The generated `node_modules/.bin/*.ps1` files are ignored dependencies, not
repository files, and are not part of this cleanup.

### Normal development currently invokes PowerShell

- `package.json:20` defines `npm run verify` as a Windows PowerShell command
  with a `pwsh` fallback.
- `README.md:166` lists PowerShell as a source-build prerequisite and
  `README.md:182-188` tells normal developers to fetch the sidecar and invoke
  PowerShell scripts.
- `CONTRIBUTING.md:21-42` uses PowerShell for local setup and PR verification.
- `context/project-overview.md:31` points the normal project workflow at
  `scripts/verify.ps1`.

The current `scripts/verify.ps1:1-99` is not a lightweight developer command:
it fetches the pinned gateway, runs `npm ci`, performs audits, and executes the
full CI gate. Those release/CI responsibilities must remain available, but
they should not be the default local command.

### CI and release references

- `.github/workflows/windows-verify.yml:33-36` runs the full gate with
  `shell: pwsh`.
- `.github/workflows/linux-verify.yml:44-49` does the same under `pwsh`.
- `docs/RELEASING.md:22-54` documents sidecar checks, verification, and
  checksums as release operations.
- `docs/CODEBASE_AUDIT.md:18,50-60` describes `scripts/verify.ps1` as the
  Windows beta verification gate.

These are release/CI uses and may keep PowerShell. Update their filename
references if the gate is renamed; do not weaken the gate merely to remove a
shell from local development.

### Test fixture boundary

`crates/core/src/tools/external_agent.rs` contains three Windows-only test
paths that launch `powershell.exe`:

- `:3054-3070` launches `external_agent_stream_smoke.ps1` and asserts partial
  text, tool-call, and final-result events.
- `:3168-3180` launches `external_agent_acp_smoke.ps1` and exercises ACP file,
  terminal, permission, usage, and final-message handling.
- `:3199-3212` launches `external_agent_smoke.ps1` for the basic headless result.

The non-Windows basic test currently uses `sh -c`, so behavior differs by OS.
The replacement should be a std-only Rust helper with explicit modes such as
`headless`, `stream`, and `acp`, compiled once for the test process with the
host `rustc` and launched by absolute path. This keeps the fixture portable,
avoids adding Node to the external-agent runtime path, and lets the same tests
run on Windows and Unix.

### Product PowerShell support is not script noise

Keep the runtime ability to execute PowerShell when a user explicitly chooses
it. In particular, do not remove the Windows PowerShell command allowance in
`crates/core/src/tools/bash.rs:875` or PowerShell syntax highlighting in
`crates/desktop/ui/src/lib/highlight-core.ts`. The cleanup targets repository
helpers and default workflows, not a user's shell capability.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Drift check | `git diff --stat 81286c3..HEAD -- package.json README.md CONTRIBUTING.md context/project-overview.md docs/RELEASING.md docs/CODEBASE_AUDIT.md .github/workflows/windows-verify.yml .github/workflows/linux-verify.yml scripts crates/core/tests/fixtures crates/core/src/tools/external_agent.rs` | No unexpected prior changes in the in-scope paths |
| PowerShell inventory | `rg --files -g '*.ps1' scripts crates/core/tests/fixtures` | Only the five release scripts remain after the change |
| Developer verification | `npm run verify` | Exit 0 without invoking `powershell.exe` or `pwsh` |
| Fixture tests | `cargo test -p zest-core --lib external_agent` | All external-agent tests pass on Windows and Unix |
| Rust tests | `cargo test --workspace --quiet` | Exit 0; all workspace tests pass |
| Rust formatting | `cargo fmt --all -- --check` | Exit 0; no formatting changes required |
| UI tests | `npm run ui:test` | Exit 0; all UI tests pass |
| UI lint | `npm run ui:lint` | Exit 0; no warnings/errors |
| UI build | `npm run ui:build` | Exit 0; production bundle builds |
| Diff hygiene | `git diff --check` | Exit 0; no whitespace errors |

## Scope

**In scope** (modify only these paths unless a STOP condition applies):

- `scripts/start-gateway.ps1` - delete; gateway startup belongs to Zest after
  Plan 005.
- `scripts/codex-login-gateway.ps1` - delete; gateway authentication belongs
  to the app/CLI auth flow after Plan 005.
- `scripts/verify.ps1` - rename to `scripts/release-verify.ps1` without
  removing its release/CI checks.
- `scripts/dev-verify.mjs` - create a cross-platform local verification
  command that excludes release-only sidecar fetching, packaging, and audits.
- `package.json` - point `npm run verify` at the developer command; do not
  leave a normal-development script that shells into PowerShell.
- `.github/workflows/windows-verify.yml`,
  `.github/workflows/linux-verify.yml`, `docs/RELEASING.md`, and
  `docs/CODEBASE_AUDIT.md` - update the release-gate filename only; keep their
  PowerShell-based release/CI behavior.
- `crates/core/tests/fixtures/external_agent_fixture.rs` - create one std-only
  helper with `headless`, `stream`, and `acp` modes.
- `crates/core/tests/fixtures/external_agent_smoke.ps1`,
  `external_agent_stream_smoke.ps1`, and `external_agent_acp_smoke.ps1` -
  delete after the Rust helper is wired in.
- `crates/core/src/tools/external_agent.rs` - update test-only fixture
  compilation/launch helpers and remove Windows-only branching where the new
  fixture makes behavior portable.
- `crates/core/src/provider/claude_code.rs` - resolve two pre-existing strict
  clippy warnings surfaced by the new cross-platform developer gate, without
  changing provider behavior.
- `README.md`, `CONTRIBUTING.md`, and `context/project-overview.md` - remove
  PowerShell and sidecar-fetch requirements from ordinary development steps;
  point release preparation to `docs/RELEASING.md` and the release gate.

**Out of scope** (do not touch):

- `scripts/build-signed.ps1`, `scripts/fetch-gateway.ps1`,
  `scripts/make-installer-art.ps1`, `scripts/release-checksums.ps1`, or the
  renamed `scripts/release-verify.ps1`; these are release/CI infrastructure.
- The bundled CLIProxyAPI binary, its release pin, license, packaging, or
  gateway configuration format.
- Runtime PowerShell execution support, command allowlists, or syntax
  highlighting.
- The gateway provider lifecycle itself; that work belongs to Plan 005.
- Replacing the external-agent ACP/headless protocol or reducing its test
  assertions.
- Deleting ignored `node_modules` files or changing global developer tooling.

## Steps

### Step 1: Make the release gate visibly release-only

Rename `scripts/verify.ps1` to `scripts/release-verify.ps1` and update only
release/CI references in `.github/workflows/windows-verify.yml`,
`.github/workflows/linux-verify.yml`, `docs/RELEASING.md`, and
`docs/CODEBASE_AUDIT.md`. Preserve the gate's sidecar fetch, binding-drift,
UI, Rust, audit, and Git checks exactly; changing its semantics is not part of
this cleanup. Update its final success text if it names `verify.ps1`.

**Verify**: `rg -n "scripts/(verify|release-verify)\\.ps1" .github docs README.md CONTRIBUTING.md context` -> only release/CI documentation and the explicitly marked release gate reference remain; no stale path points to a missing file.

### Step 2: Remove obsolete manual gateway helpers and normal-workflow references

Delete `scripts/start-gateway.ps1` and `scripts/codex-login-gateway.ps1` only
after Plan 005's gateway readiness and auth paths are present. Remove their
references from documentation and error/help text. Normal setup in `README.md`,
`CONTRIBUTING.md`, and `context/project-overview.md` must use `npm ci`, the
standard UI/Rust commands, and the app/CLI auth flow; it must not require a
manual gateway start, a gateway OAuth script, or a PowerShell installation.
Keep release sidecar preparation documented under `docs/RELEASING.md`.

**Verify**: `rg -n "start-gateway|codex-login-gateway|powershell|pwsh|PowerShell" README.md CONTRIBUTING.md context PROJECT_CONTEXT.md package.json` -> no manual-helper or PowerShell prerequisite remains in ordinary workflow text; any retained match is explicitly a release-only note.

### Step 3: Add a cross-platform developer verification command

Create `scripts/dev-verify.mjs` using Node's `child_process.spawnSync` (without
shell interpolation). On Windows, invoke npm through the current Node/npm CLI
entry point rather than a shell wrapper; use `npm` directly elsewhere. Run the
ordinary, non-release checks in a fixed fail-fast order: UI test, UI lint, UI
build, `cargo fmt --all -- --check`, strict workspace clippy, workspace library
tests, and `git diff --check`. Do not fetch the gateway, run `npm ci`, run
release audits, or require a release sidecar from this command. Keep those
checks in `scripts/release-verify.ps1` for CI/release use.

Change `package.json` so `npm run verify` invokes `node ./scripts/dev-verify.mjs`
and contains no PowerShell fallback. Document the distinction: `npm run
verify` is the local developer gate; the release gate is the PowerShell script
used by release/CI workflows.

**Verify**: `npm run verify` -> exit 0 on a checkout without a fetched release
sidecar, and the child-process list/command source contains no `pwsh` or
`powershell.exe` invocation.

### Step 4: Replace PowerShell external-agent fixtures with a Rust helper

Create `crates/core/tests/fixtures/external_agent_fixture.rs` as a standalone,
std-only executable. It must accept a mode argument and preserve the current
fixture contracts:

1. `headless` emits the same final JSON result with response `worker ok`.
2. `stream` emits the partial text `hello`, a `Read` tool-use start, a tool
   result event, and the final result response `hello`.
3. `acp` performs the same line-oriented ACP handshake, file read/write
   requests, terminal create/wait/output requests, two permission requests,
   usage update, streamed `acp ok` message, and final stop result. Use
   `rustc --version` for the simulated terminal command and assert the output
   contains `rustc`, so the protocol fixture is cross-platform and does not
   invoke PowerShell.

In the test-only code of `crates/core/src/tools/external_agent.rs`, compile the
helper once with the host `rustc` into a retained temporary path (including the
platform executable suffix), then launch that absolute executable for all
three configurations. Build command arguments with `Path::join`; do not embed
Windows-only separators. Remove the Windows-only guards around the stream and
ACP tests if they no longer have a platform dependency, and make the basic
headless configuration use the same helper on every OS. Preserve every current
assertion for text deltas, tool events, ACP file mutation, permissions, usage,
and final text. Delete the three `.ps1` fixtures only after these tests pass.

**Verify**: `cargo test -p zest-core --lib external_agent` -> the basic,
streaming, and ACP tests pass on the host; `rg -n "powershell\\.exe|\\.ps1"
crates/core/src/tools/external_agent.rs crates/core/tests/fixtures` -> no
external-agent test path invokes a PowerShell fixture.

### Step 5: Reconcile documentation and final script inventory

Make the normal-vs-release distinction consistent across README, contributor
guidance, project context, release documentation, and CI comments. The final
tracked `.ps1` inventory under `scripts/` must be the five release scripts
listed in Current state; no test fixture `.ps1` or manual runtime helper may
remain. Keep code comments that refer to `scripts/fetch-gateway.ps1` because
that release script still exists, but do not describe it as a normal runtime
prerequisite.

**Verify**: `rg --files -g '*.ps1' scripts crates/core/tests/fixtures` -> exactly
the five release files; `rg -n "start-gateway|codex-login-gateway|verify\\.ps1"
.` -> no stale deleted-helper path and no stale gate filename outside an
intentional release-history note.

### Step 6: Run the complete verification suite

Run the commands in the table above. Review `git diff --stat` and
`git status --short` to confirm the diff contains only the Scope paths and the
intended deletes/renames. Do not commit or push as part of this plan unless the
operator separately requests it.

**Verify**: all commands exit 0, and `git diff --check` reports no whitespace
errors.

## Test plan

- Unit/integration behavior: compile the std-only fixture and run headless,
  streaming, and ACP external-agent tests on Windows and Unix.
- Regression: retain the existing partial-event assertions, ACP filesystem
  writes, terminal lifecycle, permission outcomes, usage values, and final
  response assertions in `crates/core/src/tools/external_agent.rs`.
- Developer tooling: run `npm run verify` without a release sidecar and
  confirm it does not invoke PowerShell.
- Release tooling: confirm both CI workflows still call the renamed release
  gate and that `scripts/fetch-gateway.ps1` remains the sidecar provenance path.
- Inventory: confirm no non-release `.ps1` remains tracked under `scripts/` or
  the external-agent fixture directory.

## Done criteria

- [x] `scripts/start-gateway.ps1` and `scripts/codex-login-gateway.ps1` are
      deleted; no repository reference points to either file.
- [x] The five release PowerShell scripts remain available, with the full gate
      clearly named `scripts/release-verify.ps1`.
- [x] CI and release documentation use the renamed gate and retain their
      existing release checks.
- [x] `npm run verify` is cross-platform, does not invoke PowerShell, and does
      not require a fetched release sidecar.
- [x] External-agent tests use one cross-platform Rust helper and preserve all
      existing protocol/event assertions on Windows and Unix.
- [x] No `.ps1` fixture remains under `crates/core/tests/fixtures`.
- [x] Normal README/contributor/project-context instructions do not require
      PowerShell, manual gateway startup, or manual gateway OAuth.
- [x] Runtime PowerShell execution support and highlighting are unchanged.
- [x] `cargo test --workspace --quiet`, `cargo fmt --all -- --check`,
      `npm run ui:test`, `npm run ui:lint`, `npm run ui:build`, and
      `git diff --check` exit 0.
- [x] No files outside the Scope list are modified.
- [x] `plans/README.md` status row is updated when implementation completes.

## STOP conditions

Stop and report instead of improvising if:

- Plan 005 is not complete or the app/CLI still depends on either manual
  gateway script for normal startup, auth, or shutdown.
- A release workflow needs one of the deleted scripts or the renamed gate path
  in a way not covered by the Scope list.
- Replacing the fixture requires Node, Python, a third-party runtime, network
  access, or a change to the production external-agent protocol.
- The Rust helper cannot be compiled once and launched portably from the
  existing unit-test target without adding a runtime dependency; stop and
  propose a small test-only target rather than embedding shell-specific code.
- Removing a PowerShell branch would reduce a user's ability to explicitly run
  PowerShell as a Windows terminal command.
- `npm run verify` cannot run without a release sidecar after a scoped change;
  do not silently reintroduce sidecar fetching into the local command.
- The final `.ps1` inventory contains a file whose purpose cannot be classified
  as release/CI or obsolete runtime/test support.
- Any verification command fails twice after a reasonable, scoped fix attempt.

## Maintenance notes

- Keep release PowerShell scripts isolated from normal runtime code and label
  new release-only commands accordingly. A future release migration may move
  the gate to Rust or Node, but that is a separate plan; do not duplicate it in
  product startup code.
- If the external-agent protocol changes, update the single Rust fixture and
  its mode-specific assertions together. Do not restore per-platform shell
  fixtures merely to make a test pass.
- Review `package.json` whenever a new verification step is added: local checks
  must remain cross-platform and release checks must stay in the release gate.
- Keep the distinction between a bundled sidecar and a running gateway clear:
  fetching a release artifact is packaging work, while starting/stopping the
  gateway is Plan 005 runtime behavior.
- Revisit `crates/core/src/tools/bash.rs` only if product policy intentionally
  changes Windows shell support; this cleanup deliberately leaves that user
  capability intact.
