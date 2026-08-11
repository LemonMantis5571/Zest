# Codebase audit

Audit date: 2026-08-05

Scope: repository structure, Rust and UI source, build scripts, dependency
declarations, tests, release metadata, security boundaries, and public docs.
The audit was read-only except for the small clippy/verification fixes and
documentation updates in the same beta preparation pass.

The working tree also contained user-owned, uncommitted provider and external
worker entries in `zest.toml`. They were intentionally excluded from this
release patch and must not be staged as part of the beta documentation work.

## Verification baseline

The following checks passed after the audit fixes:

- `scripts/release-verify.ps1` (the full Windows beta verification gate);
- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace --lib`;
- `cargo check --workspace`;
- `npm run ui:lint`;
- `npm run ui:test` (120 tests at audit time); and
- `npm run ui:build`;
- `cargo audit` (18 allowed upstream-maintenance/advisory warnings at audit
  time); and
- `npm audit --omit=dev` (0 vulnerabilities at audit time).

The live provider doctor was not run as part of this audit because it requires
credentials and spends real quota.

## Findings and disposition

| Priority | Finding and evidence | Impact | Disposition |
| --- | --- | --- | --- |
| P1 | The repository declared MIT in `Cargo.toml:5-8` but had no root license file. | A source checkout and release had no single, obvious license grant. | Fixed in this pass with `LICENSE` and README/third-party links. |
| P1 | The README described a “stable Windows alpha” and said the license would be published, while the repository already had MSI/NSIS, signing, checksum, and verification scripts. | Users could not tell how to install a beta or build a release safely. | Fixed in this pass with the beta README, `docs/RELEASING.md`, `SECURITY.md`, and updated contributor guidance. |
| P1 | `crates/cli/src/main.rs` had three non-exhaustive `StreamEvent` matches after `ToolCallUpdate` was added in `crates/core/src/provider/mod.rs:268-278`. | The full clippy/build gate failed even though core and UI tests passed. | Fixed: JSONL emits the metadata event; interactive CLI paths ignore UI-only metadata. |
| P1 | The desktop picker only rendered detected sign-in slots plus OpenAI-compatible config rows, so a configured native Anthropic provider was invisible. | Existing Anthropic configuration could not be selected from the desktop. | Fixed: configured Anthropic rows now appear with environment-key status and no misleading keychain action. |
| P2 | Custom API-provider setup kept every custom credential account named `custom`, and the model allow-list hint said an empty list allowed any model even though core accepts only the default model. | Multiple custom providers could overwrite one another; users could configure a model the runtime would reject. | Fixed: custom credential names follow the provider id and the UI copy matches core validation. |
| P2 | Settings showed the CLI check action only for built-in ACP presets. | A manually declared `[agents.*]` worker could be enabled but not checked from the desktop. | Fixed: every configured worker now exposes the local CLI check. |
| P2 | Modal panels declared `aria-modal` but did not keep keyboard focus inside or restore the trigger. | Keyboard and assistive-technology users could tab into the page behind an open panel. | Fixed: Settings and provider-switch panels share a WebView-safe focus trap. |
| P2 | The release process named a CLIProxyAPI notice but had no dependency-license index for the source/build graph. | A public binary release had no single attribution map for maintainers to review. | Fixed for beta preparation with `THIRD_PARTY_NOTICES.md`; maintainers still attach any upstream license texts required by the resolved graph. |
| P2 | The ACP registration tests do not execute a real delegated worker through the parent agent and verify its persisted outcome. | CLI availability and delegation persistence can regress without a live worker fixture. | Deferred: add a mock ACP/headless worker fixture before expanding worker protocol behavior. |
| P2 | `crates/desktop/ui-legacy/index.html:1`, `styles.css`, and `app.js` are an older static UI tree. The active package entrypoint is `crates/desktop/ui/package.json:6-11`, and Tauri builds `crates/desktop/ui/dist` from `crates/desktop/tauri.conf.json:6-8`. | Two UI implementations increase maintenance and security-review surface; the legacy tree can confuse contributors. | Cleanup candidate. Do not delete until a maintainer confirms no downstream/manual use; remove it in a separate small commit. |
| P2 | `crates/desktop/ui/src/components/ui/dropdown-menu.tsx:1-35`, `popover.tsx`, and `tooltip.tsx` have no imports from the active UI source. `vite.svg`, `react.svg`, `hero.png`, and `icons.svg` are also unreferenced assets. | Generated leftovers add dead code and can reintroduce unsupported portal/menu patterns. | Cleanup candidate. Confirm dynamic imports or screenshots first, then delete unused files together with any now-unused package code. |
| P2 | `crates/desktop/src/lib.rs` is about 4,169 lines and contains Tauri commands, event projection, provider setup, workspace review, usage, and tests. | A single backend module is harder to review and raises merge/regression risk. | Deferred architecture debt. Split by boundary only when behavior is covered by focused tests; avoid a broad release refactor. |
| P2 | `crates/core/src/tools/external_agent.rs` is about 2,414 lines and combines CLI normalization, JSONL, ACP JSON-RPC, terminal sessions, worktrees, diffs, and usage parsing (`:800`, `:1147`, `:1444`, `:1794`). | External-worker changes have a wide blast radius. | Deferred architecture debt. Extract protocol and worktree modules behind the existing `ExternalAgent` boundary in a later slice. |
| P3 | `scripts/release-verify.ps1:84` runs `cargo test --workspace --lib` and intentionally skips Tauri binary harnesses because of Windows application-control restrictions. | Some binary-target integration behavior is outside the default gate. | Documented coverage gap. Add a separate opt-in desktop/CLI test job when the Windows runner policy allows it; do not weaken the current gate. |
| P3 | The direct `walkdir` dependency had no source references and was removed from the workspace and core manifests. | Unused dependency and lockfile surface. | Fixed in this pass; `cargo check --workspace` passed afterward. |
| P3 | `npm run ui:build` reports a chunk over 500 kB; the current build emitted a roughly 699 kB main bundle and a roughly 2.3 MB syntax-highlighting worker. The likely contributors are the broad Shiki language set in `crates/desktop/ui/src/lib/highlight-core.ts:16-38` and the Mermaid dependency declared in `crates/desktop/ui/package.json:22`. | Startup and update cost are higher than the rest of the lightweight shell suggests. | Deferred performance work. Measure startup on a clean Windows profile, then reduce grammar loading or split optional visualization code without removing supported transcript features. |
| P3 | `cargo audit` passes with 18 allowed warnings, including unmaintained GTK3/Unicode crates and the cross-platform `glib` advisory in the current dependency graph. The beta Windows path may not load all of those targets, but the advisory set should not be treated as zero risk. | Non-Windows builds and future dependency changes need a deliberate RustSec review. | Documented release risk. Keep the gate, review the warnings before broadening platform support, and prefer maintained replacements when they do not destabilize the Windows build. |

## Intentional complexity not marked as dead code

- Compatibility parsing for old configuration and thread formats is retained
  because existing user state must open safely.
- The gateway pin, sidecar verification, credential backends, ACP worker
  isolation, and approval hub are security/reliability boundaries, not generic
  abstraction for its own sake.
- The Vite/Tauri UI uses local primitives because portal-based components have
  caused WebView regressions in this project. Replacing them wholesale would be
  a product-risk refactor, not cleanup.

## Recommended follow-up order

1. Remove the confirmed legacy UI and generated leftovers after one maintainer
   confirms they are not part of an external workflow.
2. Add an opt-in Windows test job for binary targets and installer smoke tests.
3. Extract ACP/headless parsing from `external_agent.rs` with characterization
   fixtures before changing behavior.
4. Split desktop commands by feature boundary after the beta stabilizes.

These are intentionally separate from the beta documentation and verification
fixes so a release does not hide architectural changes inside a large cleanup.
