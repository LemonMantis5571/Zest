# Project Context

## Project Name

**Zest** - a provider-aware coding harness with explicit ACP delegation, written in Rust.

## Purpose

Zest gives a coding agent a focused parent session: it reads and edits a project, runs commands,
asks before risky work, shows diffs, and keeps the transcript recoverable. The parent can use the
bundled gateway, a native API provider, an OpenAI-compatible endpoint, or an authenticated Claude
Code subscription directly.

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

- **zest-core** - headless library: provider layer, agent loop, tools, ACP/headless workers,
  persistence, usage ledger, and approvals.
- **zest** - terminal front-end. One consumer of the core.
- **zest-desktop** - Tauri shell: provider picker, Codex Connect, API-key setup, ACP worker setup,
  project/chat history, attachments, approvals/diffs, context meter, and recovery controls.
- **Provider layer** - one Provider per configured parent backend. Anthropic, gateway,
  OpenAI-compatible, and Claude Code parent providers share the abstraction but do not imply task
  routing.
- **ACP workers** - configured under [agents.<id>]; invoked only through delegate_external and
  kept separate from the Claude Code parent provider.
- **Usage ledger** - records Zest traffic honestly per provider. External CLI usage is not invented.
- **Gateway** - bundled CLIProxyAPI sidecar for the supported subscription bootstrap. It is an
  implementation detail of provider access, not an external worker.

## Important Constraints

- **Rust agent path.** No Python or Node in the agent/runtime path. The desktop UI may use a
  prebuilt React webview; the agent loop stays in zest-core.
- **ACP stays explicit.** Workers must be configured and already signed in. No hidden provider
  switching or automatic delegation.
- **Secrets stay out of config.** API keys use the OS credential manager with an environment
  fallback for CI. Worker CLI sessions remain owned by their CLIs.
- **Usage accounting must be honest.** Mark provider-reported usage separately from unavailable
  usage. Never fabricate exact external-worker token counts.
- **One install, managed runtime.** The desktop installer includes the pinned gateway sidecar; do
  not add a second monolithic runtime.
- **The permission layer gates dangerous tools.** Writes and commands use the approval policy.
  External worker execution is also approval-gated.
- **Provider-specific history is immutable.** A chat stays with its selected provider because
  wire history can contain provider-specific thinking signatures and tool shapes.

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
