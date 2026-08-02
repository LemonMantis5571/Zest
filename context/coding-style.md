# Coding Style

## General

- Keep code readable. Prefer explicit names.
- Avoid unnecessary abstractions — but a trait that exists to keep two implementations
  interchangeable (`Tool`, and soon `Provider`) is not unnecessary.
- Preserve existing behavior unless asked to change it.
- Match the surrounding file's idiom, naming and comment density.

## Rust conventions in this project

- `rustfmt` defaults. No custom config.
- Modules are directories with `mod.rs` (`anthropic/`, `tools/`), re-exported from `lib.rs` so
  consumers write `zest_core::Agent` rather than a deep path.
- Public API takes `impl Into<String>` / `impl AsRef<Path>` where it costs nothing.
- Builder-style `with_*` methods that take and return `self` for optional configuration
  (`with_base_url`, `with_system`).
- `thiserror` for library errors (`HarnessError`), `anyhow` only in the binary.
- `async_trait` where a trait needs async methods; the ergonomic cost is worth avoiding hand-rolled
  futures here.

## Comments

- Comment **why**, not what. If the code is doing something that looks wrong but is deliberate,
  say why — the raw-JSON content blocks and the byte-level SSE buffering both exist for reasons
  that are invisible from the code alone.
- Never write a comment that narrates the next line, records where code came from, or argues that a
  change is correct. That is talking to a reviewer, and it is noise once merged.
- Module-level `//!` docs state what the module is for and what it deliberately does not do.

## Error Handling

- Handle missing or null values safely. Never index into a `Value` field that a malformed or
  future-version payload might omit — use `.get()` and `.and_then()`.
- Unknown enum variants, event types and content-block types must be skipped, not treated as
  errors. The API adds them over time and a strict parser breaks on a Tuesday.
- Distinguish failure kinds: a tool failing is a `tool_result` with `is_error: true` that goes back
  to the model; a stream or transport failure is a `HarnessError` that ends the turn.
- Check `stop_reason` before reading `content`. A refusal returns HTTP 200 with empty or partial
  content.
- `unwrap()` and `expect()` only where the invariant is established two lines above and stated in
  the message.

## Testing

- Unit-test the parsers against the nasty cases, not the happy path — the SSE tests cover a
  codepoint split across a chunk boundary and a line arriving in two pieces, because those are the
  bugs that actually happen.
- Do not claim behavior is verified because it compiles. "Builds", "unit tests pass" and "works
  against the live API" are three separate claims and should be reported separately.

## Formatting

- Keep indentation consistent with the existing file.
- Return complete replacement code when requested.
