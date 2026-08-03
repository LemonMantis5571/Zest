# Project Context

## Project Name

**Zest** — a coding harness written in Rust.

## Purpose

Zest is a **multi-provider orchestration harness** for coding agents.

The defining capability is routing: send each task to the model that suits it, across providers
that are authenticated separately — Gemini 3.5 Flash via an Antigravity login, Claude Opus 5 via a
Claude login, GPT-5.6 Luna via a Codex login — and keep a running account of what each provider has
left. Cheap, fast models take mechanical work; expensive models take the hard reasoning; the
harness decides which is which and never silently burns the wrong budget.

Around that sits the coding-agent shape (Claude Code / Codex): it reads and edits a codebase, runs
commands, and asks permission before doing anything it cannot take back.

What makes this worth building is that no single-vendor tool does it. Claude Code speaks Anthropic,
Codex speaks OpenAI. Zest is provider-agnostic by construction, which means the routing policy and
the usage ledger are the product — not the agent loop, which is comparatively simple.

It is **not LimeBot**, and that distinction matters for every decision here:

| | LimeBot (LimeBot-OS) | Zest |
|---|---|---|
| Shape | Personal AI assistant | Coding agent |
| Interaction | Long-running, multi-channel (Discord, Telegram, WhatsApp, web) | Invoked in a project directory, one session at a time |
| Emphasis | Memory, persona, conversation | Filesystem, tools, approvals, diffs |
| Stack | Python + Node CLI + React web UI | Rust, single binary |
| Relationship | Reference implementation | Shares design lessons, no code and no runtime |

Zest borrows LimeBot's accumulated design — skill layout, prompt work, config shape — as
*concepts*. Nothing is imported.

## Main Users

The author, working in local repositories on Windows. Not a product; no multi-tenancy, no hosted
component, no accounts.

## Main Systems

- **`zest-core`** — headless library: model client, agent loop, tool registry. No terminal or UI
  assumptions.
- **`zest`** — terminal front-end. One consumer of the core.
- **`zest-desktop`** — Tauri shell: provider picker, Connect (vendor OAuth spawn), and chat session
  UI (projects sidebar, attachments, approvals/diffs, context meter). Webview is a Vite + React +
  shadcn build under `crates/desktop/ui/` (Node is build/dev only).
- **Anthropic Messages API** — the wire protocol implemented natively today, over raw HTTP + SSE.
  No SDK.

Planned, and the actual point of the project:

- **Provider layer** — one `Provider` per authenticated backend (Anthropic, Codex, Antigravity/
  Gemini, …). Each owns its credentials, its model catalogue, and how it reports usage.
- **Router + delegated workers** — parent chat stays provider-pinned; multi-provider work goes
  through `delegate` workers resolved by routing policy + ledger fallback (v1 decision).
- **Usage ledger** — per-provider consumption and remaining headroom, persisted across runs.
- **Gateway (transitional)** — [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) already
  holds OAuth logins for Codex, Claude, Gemini/Antigravity, Grok and Kimi and re-exposes them as
  the Messages API. Fastest path to a second and third provider; see `memory/decisions.md` for why
  it is transitional rather than permanent.

## Important Constraints

- **Rust agent path.** No Python, no Node in the agent/runtime path. Desktop UI may use a React
  webview built ahead of time; the agent loop stays in `zest-core`.
- **Providers are a first-class abstraction**, not configuration. Whether a given provider is
  reached natively or through a gateway must be an implementation detail behind one trait, so
  either can be swapped without touching the router.
- **Model: `claude-opus-5`** is the default for Anthropic. See `context/constraints.md` for the API
  rules that follow — several cause a 400 if violated.
- **Usage accounting must be honest.** Subscription-backed CLI logins mostly do not expose a
  "remaining quota" endpoint. Where a provider reports headroom, read it; where it does not, meter
  locally against a configured budget and label the number as an estimate. Never present a guess
  as a reading.
- **No shipped runtime dependencies, long term.** A proxy is acceptable while bootstrapping
  providers and should not survive into a release; supervising a second process is the problem a
  single binary exists to avoid.
- **The permission layer gates dangerous tools.** `write_file` and `edit_file` ship with an
  approval gate and atomic replace. `bash` ships behind the same gate: a small set of
  genuinely read-only, metacharacter-free commands (`cargo check`, `git status`, …) runs
  unattended; everything else shows the exact command line and waits. There is no OS sandbox
  — see `memory/decisions.md` for why that bar was dropped rather than waited on.

## Preferred Style

- Direct and concrete. Lead with the outcome, then the reasoning.
- Say what is verified and what is not. "It compiles" and "it works against the live API" are
  different claims and should never be blurred.
- Comments explain *why*, especially where the code looks wrong but isn't (see the raw-JSON
  content blocks, or the byte-level SSE buffering). Never comment what the next line does.
- Prose over bullet fragments when explaining a decision.

## Things the AI Should Avoid

- **Treating Zest as LimeBot.** Different project, different shape. Do not add channels, personas,
  or assistant features.
- **Adding an SDK.** There is no official Anthropic Rust SDK; community ones are thin. The
  hand-written client is deliberate and gives full control over streaming and tool blocks.
- **Guessing at the Messages API.** Model IDs, streaming event shapes, and which parameters 400
  have all changed recently. Check the reference; do not answer from memory.
- **Typing the assistant content blocks.** They are `serde_json::Value` on purpose — see
  `memory/decisions.md`.
- **Sampling parameters.** `temperature`, `top_p`, `top_k` are rejected on Opus 5.
- **Claiming the loop works** until it has been run against a live key end to end with a real tool
  call.
