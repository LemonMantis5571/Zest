# Glossary

| Term | Meaning | Notes |
|---|---|---|
| Harness | The code around the model: agent loop, tool execution, permissions, context and session management | What Zest *is*. Distinct from the model and from the UI |
| Agent loop | Request → model asks for tools → execute → feed results back → repeat until it stops asking | `crates/core/src/agent.rs`. The easy part; permissions are the hard part |
| Turn | One user message and everything the model does in response, including any number of tool round-trips | Ends when `stop_reason` is `end_turn` |
| Content block | One element of a message's `content` array — `text`, `thinking`, `tool_use`, `tool_result` | Stored as raw `serde_json::Value` so unknown types round-trip losslessly |
| `tool_use` | Assistant block requesting a client-side tool call, with `id`, `name`, `input` | Zest must answer each one with a matching `tool_result` |
| `tool_result` | User block answering a `tool_use`, keyed by `tool_use_id` | All results for one turn go in a **single** user message |
| `input_json_delta` | Streaming delta carrying a *fragment of a JSON string*, not a JSON object | Accumulate per block index, parse once at `content_block_stop` |
| Thinking block | Reasoning content, carrying a `signature` | Must be echoed back byte-for-byte or the next request is rejected |
| `signature_delta` | Streaming event delivering the thinking block's integrity signature | Arrives just before `content_block_stop` |
| Effort | `output_config.effort` — `low`/`medium`/`high`/`xhigh`/`max`, controls reasoning depth and token spend | Replaces the removed `budget_tokens`. Zest defaults to `high` |
| `stop_reason` | Why a turn ended: `end_turn`, `tool_use`, `pause_turn`, `max_tokens`, `refusal` | Must be checked before reading content |
| `pause_turn` | A server-side tool hit its iteration cap | Resend unchanged; the server resumes |
| SSE | Server-sent events, the streaming wire format | `crates/core/src/anthropic/sse.rs` |
| Gateway | A local server that translates between provider protocols | CLIProxyAPI is pinned and bundled; `ZEST_BASE_URL` can override its origin |
| Anthropic extensions | Request fields only Anthropic understands: `thinking`, `output_config.effort` | Dropped automatically when pointed at a non-Anthropic gateway |
| Sidecar | A second process shipped alongside and supervised by the app | CLIProxyAPI is the approved sidecar; users still receive one installer |
| Known workspace | A project folder the desktop has opened and remembered for the Projects sidebar | `~/.zest/known-workspaces.json` (MRU). Threads stay under each project's `.zest/threads/` |
| Context meter | UI estimate of how full the model context window is | Prefers last-turn `input_tokens`; else char/4 estimate. Compaction not shipped |
| LimeBot | The author's separate Python personal-assistant project | Design ancestor, not a dependency |
| Agentic Lemon | The author's scaffolder that generated this documentation structure | `AGENTS.md`, `context/`, `memory/`, `skills/` |
