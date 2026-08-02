# Project Overview

## Summary

**Zest** is a multi-provider orchestration harness for coding agents, written in Rust.

It routes each task to the model that suits it across separately-authenticated providers — Gemini
via an Antigravity login, Claude via a Claude login, GPT via a Codex login — and keeps an account of
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
- Ship a single binary. No sidecars, no runtime interpreter.
- Gate anything irreversible behind an approval the user actually sees.

## Current Priorities

1. **Stable Windows Alpha (Milestone 0)** — core safety, real cancel, file/prompt hardening, and
   `scripts/verify.ps1` are in place. Run `cargo run -p zest -- doctor --live` only when a working
   gateway/login exists (manual; spends quota). Do not treat verify green as live-doctor green.
2. **Milestone 1 — usage/routing UX** — visible delegation provenance + honest ledger after M0.
3. **Second spendable provider** alongside Codex so delegated workers exercise real dual-account
   routing.
4. **OS-backed Windows sandbox** before any `bash` / exec tool ships.

## Known Risks

- **Quota visibility is largely not exposed.** Subscription CLI logins mostly have no documented
  "remaining" endpoint, so the ledger is partly estimation. If it is presented as authoritative it
  will be wrong at the worst moment. Labelling matters as much as the arithmetic.
- **Provider auth is fragile and not ours.** OAuth flows for Codex and Antigravity can change
  without notice. This is the single largest source of future breakage, and it is entirely upstream.
- **Terms of service.** Routing subscription credentials (Codex, Claude, Antigravity) through a
  third-party proxy is against the providers' terms; the realistic downside is account loss. Fine
  for local development, not for anything distributed.
- **Gateway dependency drifting into permanence.** CLIProxyAPI is the fast path to providers two and
  three. If it is still load-bearing at release, the single-binary goal is gone. Revisit it as a
  decision, not by default.
- **Scope.** Router, ledger, permissions, sessions and compaction are each substantial. The agent
  loop being done makes the project feel further along than it is.
- **Unverified code.** Everything currently builds and passes its unit tests; almost none of it has
  run against a live model.
