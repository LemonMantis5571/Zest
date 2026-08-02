# Architecture

## Overview

Three crates in a Cargo workspace. `zest-core` holds everything that isn't a user interface;
`zest` is the terminal front-end; `zest-desktop` is a Tauri launch picker over the same auth APIs.

The split exists so the agent loop can be developed and debugged at `cargo run -p zest` speed,
without a webview in the path. The desktop app is a thin view over core auth today; the agent
session UI comes after the loop is proven live.

```
crates/core/     zest-core
  anthropic/     Messages API client + SSE
  auth.rs        detect sign-ins; start_login spawns vendor CLI
  tools/         Tool trait + ToolRegistry
  agent.rs       the loop
crates/cli/      zest — REPL + ANSI renderer
crates/desktop/  zest-desktop — Tauri provider picker (ui/ + commands)
```

## Components

| Component | Purpose | Notes |
|---|---|---|
| `SseParser` (`sse.rs`) | Split the SSE byte stream into `data:` payloads | Buffers **bytes**, not strings — chunk boundaries land mid-codepoint. Ignores `event:` lines; every frame repeats its type inside the JSON. Unit-tested against a split codepoint. |
| `AnthropicClient` (`client.rs`) | Issue the streaming request, rebuild the assistant turn | Accumulates blocks keyed by the stream's `index`, never by arrival order. Owns `base_url` so a gateway can be substituted. |
| `Tool` / `ToolRegistry` (`tools/`) | Declare and dispatch client-side tools | `run` returns `Result<String, String>`; the `Err` becomes a `tool_result` with `is_error: true` rather than aborting the turn. Registry order is stable because tools render at the front of the prompt and reordering invalidates the cache. |
| `Agent` (`agent.rs`) | The loop | Request → execute tools → feed results back → repeat until the model stops asking. Owns message history. |
| `Renderer` (`cli/main.rs`) | Stream thinking/text/tool calls to the terminal | Tracks a mode so it can print a header when output switches kind. |

## Data Flow

One turn:

1. `Agent::send` pushes a user message and builds a `Request` (`stream: true`).
2. `AnthropicClient::stream` POSTs to `{base_url}/v1/messages`.
3. SSE frames arrive. Each is decoded and dispatched on its `type`:
   - `content_block_start` — record the block at its `index`; if it is a tool call, open a JSON accumulator.
   - `content_block_delta` — `text_delta` and `thinking_delta` append to the block **and** fire a `StreamEvent` for rendering; `signature_delta` sets the thinking signature; `input_json_delta` appends a *partial JSON string* to the accumulator.
   - `content_block_stop` — parse the accumulated JSON into the block's `input`.
   - `message_delta` — record `stop_reason` and cumulative usage.
   - `error` — abort with `HarnessError::Stream`.
4. Blocks are returned in index order as a `Completion` and pushed into history **verbatim**.
5. `stop_reason` decides what happens next:
   - `end_turn` → done
   - `tool_use` → run every requested tool, push **all** results in a single user message, loop
   - `pause_turn` → resend unchanged; the server resumes
   - `max_tokens` / `refusal` / anything else → `HarnessError::StoppedEarly`

The single-user-message rule for tool results is not cosmetic: splitting results across messages
trains the model out of making parallel tool calls.

## Provider layer, router, usage ledger

The part that makes Zest worth building. All of it is now written and unit-tested; none of it has
served a live turn, because no provider has ever been reachable with a working credential.

```
        task
          │
          ▼
    ┌───────────┐   policy    ┌──────────────┐
    │  Router   │────────────▶│ Usage ledger │  (can this provider still serve it?)
    └─────┬─────┘             └──────────────┘
          │ picks provider + model
          ▼
    ┌──────────────────── Provider (trait) ────────────────────┐
    │  Anthropic          Codex             Antigravity        │
    │  native client      via gateway       via gateway        │
    │  Claude login       Codex login       Google login       │
    └──────────────────────────────────────────────────────────┘
          │ streams back
          ▼
    ┌───────────┐
    │   Agent   │  tool calls, permissions, history
    └───────────┘
```

**`Provider` trait** (`provider/mod.rs`). One per authenticated backend. Owns its credentials, its
model catalogue, how it streams a turn, and how it reports auth state. `AnthropicProvider` is the
first implementation, serving both native and gateway cases; `Agent` holds `Arc<dyn Provider>`.

The callback is `&mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send)` rather than plain `FnMut` —
`async_trait` boxes futures as `Send`, and delegated sub-agents need to spawn on the runtime. The
explicit `for<'a>` is required because inside an `async_trait` method an elided lifetime binds
instead of staying higher-ranked.

**Auth detection** (`auth.rs`). Zest performs no OAuth. Vendor CLIs / the local gateway already
sign in and write credentials; Zest reads whether that happened. `AuthStatus` distinguishes
`NotLoggedIn` from `Unknown` — Claude and Antigravity keep credentials somewhere unreadable on
Windows, and reporting those as logged-out would push the user to re-authenticate for nothing.
The desktop **Connect** button calls `start_login` / `resolve_login`: silent spawn (no console on
Windows), system browser for ChatGPT/Claude, then re-detect. Codex prefers CLIProxyAPI
`-codex-login` when that binary is present under `tools/CLIProxyAPI`.

Crucially, *how* a provider is reached is an implementation detail behind the trait. Anthropic is
native today. Codex and Antigravity can be reached through CLIProxyAPI to get working quickly, then
swapped to native clients later without the router noticing.

**Router + delegated workers (v1).** The parent conversation stays pinned to the provider chosen at
session start. Multi-provider routing runs only through the `delegate` tool: the worker is resolved
by `Router` against `[routing]` rules / default / fallback, with exhaustion reasons surfaced in the
tool result. `RuntimeBuilder` registers `delegate` when more than one provider loads. Automatic
per-turn routing is deferred. See `memory/decisions.md`.

**ModelSpec.** Each `Provider` owns a catalogue (`ModelSpec` / `ProviderDescriptor`). Gateway
config may list `models` and `efforts`; when `models` is omitted, only the configured default is
accepted. `RuntimeBuilder` and `update_session_options` validate before spending.

**Usage ledger.** Per-provider consumption and remaining headroom, persisted across runs. The
honest constraint: most subscription-backed CLI logins expose no "remaining quota" endpoint, so the
ledger meters what Zest itself spends and compares it to a configured budget. That is exact for
Zest's own traffic and blind to what other clients spent on the same account. Where a provider
returns real limits in response headers (Anthropic's `anthropic-ratelimit-*`), prefer those and
mark them authoritative. A number labelled "remaining" that silently excludes other clients' usage
is worse than no number.

## Integrations

- **Anthropic Messages API** — `https://api.anthropic.com/v1/messages`, `anthropic-version: 2023-06-01`.
  Auth via `x-api-key`; an `Authorization: Bearer` header is sent alongside because gateways differ
  on which they read, and the real API ignores the extra one.
- **Messages-API gateways** — `ZEST_BASE_URL` swaps the origin. When the host is not Anthropic's,
  `thinking` and `output_config.effort` are omitted, since a GPT or Gemini backend has no use for
  them. Development convenience only; never a shipped dependency.
- **agentic-lemon** — generated `AGENTS.md`, `PROJECT_CONTEXT.md`, `context/`, `memory/`,
  `skills/`, `references/`. Documentation only, no runtime role.

### Dependencies

`tokio`, `reqwest` (rustls, no OpenSSL), `serde` / `serde_json`, `futures-util`, `async-trait`,
`thiserror`. Deliberately no Anthropic SDK — none exists officially for Rust, and the hand-written
client is a few hundred lines with full control over streaming.
