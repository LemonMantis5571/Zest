# Project Context

## Project Name

**Zest** - a provider-aware coding harness with explicit ACP delegation, written in Rust.

## Purpose

Zest gives a coding agent a focused parent session: it reads and edits a project, runs commands,
asks before risky work, shows diffs, and keeps the transcript recoverable. The parent can use a
native API provider, an OpenAI-compatible endpoint, or an authenticated Claude Code or Codex
subscription through its own CLI.

When a task is better handled by another tool, Zest can delegate a bounded subtask to an external
worker that is already authenticated in its own CLI. Claude Code and Gemini CLI are the first
workers. Zest uses ACP or a non-interactive CLI, keeps the worker in an isolated workspace by
default, requires approval before execution, and returns the answer and diff for review. Claude
Code can also be selected as the parent provider; that path runs directly in the current project
and does not create a delegated worker.

Zest does not implement vendor OAuth for workers, embed their SDKs, or route individual tasks
between Zest providers. External workers may opt in to their own CLI-managed MCP servers; Zest
does not manage or individually approve those MCP calls. The selected Zest provider owns the parent
conversation.

## Main Users

The author, working in local repositories on Windows. Not a product; no multi-tenancy, no hosted
component, no accounts or telemetry.

## Main Systems

- **zest-core** - headless library: provider layer, agent loop, tools, native provider workers,
  ACP/headless workers, persistence, usage ledger, and approvals.
- **zest** - terminal front-end. One consumer of the core.
- **zest-desktop** - Tauri shell: provider picker, Codex Connect, API-key setup, ACP worker setup,
  project/chat history, attachments, approvals/diffs, context meter, and recovery controls.
- **Provider layer** - one Provider per configured parent backend. Anthropic, OpenAI-compatible,
  Claude Code, and Codex CLI parent providers share the abstraction but do not imply task routing.
  The two subscription kinds own their own agent loop, so Zest's tools are not registered on them.
- **ACP workers** - configured under [agents.<id>]; invoked only through delegate_external and
  kept separate from the Claude Code parent provider.
- **Usage ledger** - records Zest traffic honestly per provider. External CLI usage is not invented.
- **Coordinator** - the project-local delegation state machine and scheduler. It owns feature
  cards, worker/reviewer targets, approval fingerprints, queueing, retry records, artifacts, and
  apply/review transitions; it is not a second parent agent loop.
- **Feature card** - the bounded, versioned delegation request containing objective, scope,
  selected context, dependencies, acceptance checks, worker target, and reviewer target.
- **Two lanes** - native provider workers run through Zest's provider/runtime boundary; external
  workers run through an explicitly configured ACP or headless CLI. The lanes share coordinator
  records and review rules but do not share credential ownership or parent transcript state.

## Important Constraints

- **Rust agent path.** No Python or Node in the agent/runtime path. The desktop UI may use a
  prebuilt React webview; the agent loop stays in zest-core.
- **ACP stays explicit.** Workers must be configured and already signed in. No hidden provider
  switching or automatic delegation.
- **Parent boundary is provider-owned.** A parent chat stays with its selected provider. Native
  delegation creates a bounded worker/reviewer runtime; it does not reuse the parent transcript,
  fall back to another provider/model, or register a provider-owned parent's tools as Zest tools.
- **Delegation is approval-gated.** A job cannot enter `queued` without approval fingerprints for
  both worker and reviewer targets. Target changes invalidate approval and require a new approval.
- **Review is fresh and read-only.** Reviewers receive the worker diff in a fresh isolated
  workspace. Reviewer edits are discarded; only a validated review report can make a job ready to
  apply.
- **Secrets stay out of config.** API keys use the OS credential manager with an environment
  fallback for CI. Worker CLI sessions remain owned by their CLIs.
- **Usage accounting must be honest.** Mark provider-reported usage separately from unavailable
  usage. Never fabricate exact external-worker token counts.
- **One install, no bundled runtime.** The desktop installer ships no third-party executable; do
  not add one.
- **The permission layer gates dangerous tools.** Writes and commands use the approval policy.
  External worker execution is also approval-gated.
- **Provider-specific history is immutable.** A chat stays with its selected provider because
  wire history can contain provider-specific thinking signatures and tool shapes.
- **Usage has two lanes too.** Native provider worker usage is recorded with the provider-owned
  ledger. External worker usage is an optional per-worker projection and is never merged into the
  parent's provider spend or represented as an account balance.

## Preferred Style

- Direct and concrete. Lead with the outcome, then the reasoning.
- Say what is verified and what is not. Compilation and live API success are different claims.
- Comments explain why, especially around wire formats, process boundaries, and security.
- Preserve the lightweight desktop identity: sleek, quiet UI with useful state and visible diffs.

## Things the AI Should Avoid

- Treating Zest as a general assistant or adding channels, personas, or hosted accounts.
- Reintroducing a Zest-to-Zest routing policy or the removed internal delegate tool.
- Implementing OAuth or copying secrets for Claude Code/Gemini CLI.
- Silently attaching or managing MCP servers for external workers.
- Adding an SDK when the existing hand-written provider clients are sufficient.
- Guessing at provider APIs, model IDs, or streaming shapes.
- Claiming the loop works until it has been tested against the relevant live path.
