# Constraints

## Technical Constraints

- **Rust only in the runtime path.** No Python, no Node. The deliverable is a single binary.
- **One wire protocol.** The Messages API. Other backends go through a translating gateway, never
  a second client inside the harness.
- **No shipped runtime dependencies.** Proxies and sidecars are acceptable in development and
  never in a release.
- **Windows is the primary development platform.** MSVC toolchain, `rustls` rather than OpenSSL to
  avoid a C build dependency.
- **Streaming always.** Non-streaming requests risk HTTP timeouts at meaningful `max_tokens`, and
  the harness needs incremental output regardless.

## Messages API constraints (violating these returns a 400)

Current as of 2026-08-01, model `claude-opus-5`:

- **No `temperature`, `top_p`, or `top_k`.** Removed on Opus 5. Steer through the prompt.
- **No `thinking: {type: "enabled", budget_tokens: N}`.** Removed. Use `{type: "adaptive"}` and
  control depth with `output_config.effort`.
- **Thinking is on by default.** Omitting the `thinking` field runs adaptive — unlike Opus 4.8/4.7
  where omitting it meant no thinking.
- **`max_tokens` caps thinking *and* response text together.** A value sized for the answer alone
  truncates mid-response.
- **`thinking.display` defaults to `"omitted"`** — blocks arrive with empty text, which reads as a
  long stall in a streaming UI. Zest sets `"summarized"` explicitly.
- **Disabling thinking is capped at `effort: high`.** `{type: "disabled"}` with `xhigh` or `max`
  is a 400.
- **No assistant-turn prefill.**
- **Thinking blocks must be echoed back unmodified**, including the `signature`.

## Behavioral constraints

- **Tool results go back in a single user message.** Splitting them across messages trains the
  model out of parallel tool calls.
- **Tool definition order must be stable.** Tools render at the very front of the prompt;
  reordering invalidates the entire prompt cache.
- **`stop_reason` must be checked before reading content.** `refusal` returns HTTP 200 with empty
  or partial content — indexing `content[0]` unconditionally breaks.

## Security Constraints

- Do not expose credentials.
- Do not log secrets.
- Do not commit private keys or tokens. `.env` is gitignored; `.env.example` carries placeholders.
- **Tool inputs are model output, not user input.** Every path from a tool call is canonicalized
  and checked against the project root before reaching the filesystem — this closes `..`, absolute
  paths, and symlinks pointing outside the tree.
- No `bash` or `write` tool until the permission layer exists. Adding an unsandboxed shell to a
  harness with no approval gate is the one genuinely dangerous shortcut available here.

## Output Constraints

- Distinguish "compiles", "tests pass", and "verified against the live API". These are three
  different claims.
- Report tool and build failures with the actual output, not a summary of it.
