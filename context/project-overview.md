# Project Overview

## Summary

**Zest** is a multi-provider orchestration harness for coding agents, written in Rust.

It routes each task to the model that suits it across separately-authenticated providers — Gemini
and Claude work can run through already-authenticated external CLIs, while GPT uses the Codex
login — and keeps an account of
what each provider has left. Around that sits a coding agent: reads and edits a codebase, runs
commands, asks before doing anything irreversible.

The routing policy and the usage ledger are the product. The agent loop is comparatively simple and
is already built.

## Goals

- Send mechanical work to cheap fast models and hard reasoning to expensive ones, deliberately
  rather than by accident.
- Never silently exhaust the wrong subscription — show what each provider has left before spending
  it.
- Keep provider access behind one trait so a backend can move from gateway to native without the
  router changing.
- Ship one installer with no manual runtime dependencies; Zest manages its pinned gateway sidecar.
- Gate anything irreversible behind an approval the user actually sees.

## Current Priorities

1. **Stable Windows Alpha (Milestone 0)** — core safety, real cancel, file/prompt hardening, and
   `scripts/verify.ps1` are in place. Run `cargo run -p zest -- doctor --live` only when a working
   gateway/login exists (manual; spends quota). Do not treat verify green as live-doctor green.
2. **Milestone 1 — usage/routing UX** — Rust-authoritative provider catalogue, `UsageSnapshot`,
   delegation provenance (thread v2), Settings Usage, and opt-in `doctor --live --dual` (landed).
   Desktop also gained projects-by-folder chat history, attachments, `web_search`, and context meter.
3. **Second spendable provider** alongside Codex so delegated workers exercise real dual-account
   routing in day-to-day use (dual doctor proves the path when configured).
4. **Compaction + OS-backed Windows sandbox** — honest context management next; sandbox before any
   `bash` / exec tool ships.

## Known Risks

- **Quota visibility is largely not exposed.** Subscription CLI logins mostly have no documented
  "remaining" endpoint, so the ledger is partly estimation. If it is presented as authoritative it
  will be wrong at the worst moment. Labelling matters as much as the arithmetic.
- **Provider auth is fragile and not ours.** OAuth flows for Codex and Antigravity can change
  without notice. This is the single largest source of future breakage, and it is entirely upstream.
- **Terms of service.** CLIProxyAPI's MIT licence permits redistributing its code, not using vendor
  subscriptions. Review current vendor terms separately before any public or commercial release;
  the local personal alpha does not establish that release permission.
- **Bundled gateway supply chain.** CLIProxyAPI is an intentional runtime dependency, not an open
  architecture question. Pin every release and archive hash, ship its licence, and revisit the
  sidecar only for a documented native OAuth/API path, API-key billing, or a demonstrated security
  or reliability failure.
- **Scope.** Router, ledger, permissions, sessions and compaction are each substantial. The agent
  loop being done makes the project feel further along than it is.
- **Unverified code.** Everything currently builds and passes its unit tests; almost none of it has
  run against a live model.
