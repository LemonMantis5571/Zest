# Zest

A coding harness in Rust. Own the loop: model client, tool execution, permissions, sessions.

Zest is a **coding agent** — the Claude Code / Codex shape. It reads and edits a codebase, runs
commands, and asks permission before doing anything it can't take back.

It is **not** [LimeBot](https://github.com/LemonMantis5571/LimeBot-OS). LimeBot is a personal
assistant: long-running, multi-channel (Discord, Telegram, WhatsApp), memory-oriented, and it
lives in Python. Zest borrows LimeBot's design lessons — the skill layout, the persona and prompt
work, the config shape — but shares no code and no runtime with it.

## Status

The orchestration stack is written and unit-tested — 40 tests — and **none of it has served a live
turn.** No provider has yet been reachable with a working credential, so everything below is
"compiles and passes its tests", not "verified against a real model".

- Streaming Messages API client over raw HTTP + SSE, with the turn accumulator tested against the
  published event transcript
- `Provider` trait; `AnthropicProvider` serving both native and gateway backends
- Auth detection for Codex / Claude / Antigravity / BYOK — `zest auth`
- Usage ledger per provider, spend and headroom kept separate — `zest usage`
- `zest.toml` config, provider registry, routing policy, and a `delegate` tool that hands a subtask
  to whichever provider the policy picks

No GUI yet, deliberately — debugging an agent loop through a webview is miserable, and a desktop
front-end later becomes a thin view over a core that already works.

## Setup

```bash
cp .env.example .env
```

Add your key, then:

```bash
cargo run -p zest
```

`read_file` is scoped to the directory you launch from.

## Layout

```
crates/core/          zest-core — headless, no terminal assumptions
  anthropic/
    types.rs          Messages API wire types
    sse.rs            byte-level SSE line reader
    client.rs         streaming request + content-block accumulator
  tools/
    mod.rs            Tool trait + registry
    read_file.rs      first real tool, root-confined
  agent.rs            the loop
crates/cli/           zest — terminal front-end
```

Project knowledge for AI agents lives in `AGENTS.md`, `PROJECT_CONTEXT.md`, `context/`, and
`memory/` — scaffolded by [agentic-lemon](https://github.com/Ethereal-Lemons/agentic-lemon).

## Design notes

**Assistant content blocks are `serde_json::Value`, not a typed enum.** Thinking blocks carry a
`signature` that must be echoed back byte-for-byte, and the API adds block types over time. Raw
JSON round-trips losslessly; a typed enum silently drops what it doesn't know. Typed access is
through `tool_uses()`.

**Tool inputs arrive as partial JSON strings.** `input_json_delta` deltas are string fragments,
not JSON objects — accumulate per block index and parse once at `content_block_stop`.

**The SSE reader buffers bytes, not strings.** HTTP chunk boundaries land mid-codepoint often
enough that decoding per chunk corrupts multi-byte characters.

**No `temperature` / `top_p` / `top_k`.** Rejected with a 400 on Opus 5. Steering happens in the
prompt.

**`max_tokens` budgets thinking and text together.** Thinking is on by default on Opus 5, so a
value sized for the answer alone truncates mid-response.

**Tool failures aren't harness failures.** `Tool::run` returns `Result<String, String>`; the error
goes back as a `tool_result` with `is_error: true` so the model can adapt instead of the turn
aborting.

**Tool paths are model output, not user input.** `read_file` canonicalizes and checks against the
project root before touching the filesystem.

## Other backends

`ZEST_BASE_URL` points the client at any gateway that speaks the Messages API — e.g.
[CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) fronting a Codex or Gemini login. When
the host isn't Anthropic's, the Anthropic-only request fields (`thinking`, `output_config.effort`)
are dropped, since the backend has no use for them.

Dev-time convenience only. Shipping a proxy as a runtime dependency reintroduces the
process-supervision problem a single binary exists to avoid.

## Next

The permission layer — approval prompts, diffs before writes, a sandbox boundary for bash. That is
the part that makes a harness trustworthy rather than a demo, and it is much easier to design in
than to retrofit. Then sessions and compaction. Then a front-end.
