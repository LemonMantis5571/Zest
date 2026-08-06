# Project Overview

## Summary

**Zest** is a provider-aware coding harness for Windows, written in Rust.

The parent conversation runs against one configured provider at a time. Zest can also delegate
bounded work to an already-authenticated external CLI through ACP or headless mode, such as Claude
Code or Gemini CLI. The harness reads and edits a codebase, runs commands, asks before risky
actions, and keeps the transcript and usage record local.

Provider configuration, the usage ledger, and the approval boundary are product surfaces. The
agent loop stays provider-independent, while external workers remain an explicit process boundary;
Zest does not perform automatic provider routing or run a second internal delegate loop.

## Goals

- Keep the parent conversation on the provider the user selected, with clear model and effort
  capabilities.
- Delegate bounded work to already-authenticated CLIs without duplicating their sign-in flows.
- Never silently spend through a different account; show honest provider and usage state before
  spending when the endpoint exposes it.
- Keep provider access behind one trait so a backend can move from gateway to native without the
  agent loop changing.
- Ship one installer with no manual runtime dependencies; Zest manages its pinned gateway sidecar.
- Gate anything irreversible behind an approval the user actually sees.

## Current Priorities

1. **Windows Beta** — core safety, cancellation, file/prompt hardening, and
   `scripts/verify.ps1`. Run `cargo run -p zest -- doctor --live` only with a working
   gateway/login; it is manual and spends quota.
2. **ACP worker UX** — configure external workers in the desktop, make delegation status and
   approvals clear, keep worker output reviewable without making workers look like parent
   providers, and make CLI-owned MCP explicitly opt-in.
3. **Provider setup** — maintain the provider picker, API-key setup, model capability controls,
   and OpenAI-compatible/DeepSeek configuration without reintroducing routing rules.
4. **Compaction and sandboxing** — keep context management honest, then harden the OS-backed
   Windows sandbox before expanding command execution.

## Known Risks

- **Quota visibility is largely not exposed.** Subscription and CLI logins often have no
  documented remaining-quota endpoint, so the ledger is partly an estimate. It must not be shown
  as authoritative when it is not.
- **Provider auth is fragile and not ours.** OAuth flows, local gateways, and vendor CLIs can
  change without notice. ACP workers additionally depend on the user's local CLI installation and
  sign-in state.
- **Terms of service.** CLIProxyAPI's MIT licence permits redistributing its code, not using
  vendor subscriptions. Review current vendor terms before any public or commercial release.
- **Bundled gateway supply chain.** CLIProxyAPI is an intentional runtime dependency. Pin every
  release and archive hash, ship its licence, and revisit the sidecar only for a documented native
  OAuth/API path, API-key billing, or a demonstrated security or reliability failure.
- **Scope.** ACP process boundaries, the ledger, permissions, sessions, and compaction are each
  substantial. The agent loop being built makes the project feel further along than it is.
- **Unverified code.** Compilation and automated tests do not replace live verification against a
  configured provider or external worker.
