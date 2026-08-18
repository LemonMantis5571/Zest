# ADR 0002: Measure the whole prompt, and trim before summarizing

- Status: Accepted
- Date: 2026-08-17

## Context

Auto-compaction decided context pressure from `last_usage.input_tokens`. That
field is the *uncached remainder* of a prompt: the Anthropic API reports
`cache_read_input_tokens` and `cache_creation_input_tokens` as separate columns,
and the OpenAI-compatible and Codex paths deliberately subtract their cached share
out of the prompt they were handed so that all three agree. Once the 1h
tools+system cache prefix landed, the uncached remainder became the *small* part
of a well-cached prompt — a session sitting at 83% of its window reported 1% full.
The 80% threshold therefore fired late or never, and a long session terminated in
a provider context-overflow error instead of a compaction.

The correct total was already computed twice elsewhere in the repo: inline in the
usage ledger's range report, and again in the UI's cache metrics. The meter was
the last reader of the bare field.

Separately, compaction had exactly one strategy. Crossing the threshold always
spent a model call and always replaced the conversation with a paraphrase, even
when the pressure came from three enormous tool results whose middles nobody would
miss. There was no cheaper first move.

## Decision

Define the prompt total once per type that carries these counters —
`Usage::prompt_tokens` and `TokenCounts::prompt_tokens`, both
`input + cache_read + cache_write` — and read the meter's occupancy from it. The
`source` label (`last_turn` / `estimate`) was never wrong and does not change; the
number behind it does. The meter additionally reports the three columns so a user
can see that "78% full" is mostly cache reads.

Move the budget arithmetic — char/4, the threshold, the due-ness predicate — from
the desktop meter into `zest-core::context_budget`, because compaction now needs
the identical estimator and two copies would drift into disagreeing about whether
a compaction helped.

Add a model-free pre-compaction stage. `zest-core::prune` rewrites over-long
`tool_result` bodies in a wire history to head + fixed marker + tail, in place,
touching only `content`. `compact_context` prunes a clone of the live history,
re-measures, and returns `CompactionOutcome::Pruned` without any model call when
that was enough; otherwise it redacts and summarizes exactly as before.

Pruning runs **only** inside compaction. It is not offered on ordinary turns.

## Invariants

- The prompt total is `input + cache_read + cache_write` on every provider.
- `can_compact` and `MIN_COMPACTION_CONVERSATION_TOKENS` are measured against the
  conversation *estimate*, never against the measured total. That floor is what
  stops a prompt whose bulk is system prompt and tool schemas from triggering a
  compaction that could not have shrunk it.
- `PRUNE_HEAD_CHARS + PRUNE_MARKER + PRUNE_TAIL_CHARS <= PRUNE_THRESHOLD_CHARS`,
  asserted at compile time. Without it a replacement could exceed the threshold
  and each pass would prune its own output forever.
- Every emitted replacement is at most the threshold and strictly smaller than the
  input that triggered it, so a second pass replaces nothing.
- Slicing counts Unicode code points, never bytes: byte-indexing a `str`
  mid-character panics rather than truncating.
- `tool_use_id` and `is_error` survive a rewrite. The API validates
  `tool_use`/`tool_result` pairing, so losing an id would invalidate the whole
  request, and redaction finds sensitive bodies by that same id afterwards.
- Redaction is the last transformation before the wire. Pruning is measured on the
  live-pruned copy, because that is what a resumed turn would actually resend.
- `report.replaced > 0` is a termination condition, not an optimization: a history
  with nothing left to prune must fall through to the summarizer.
- The prune path clears `provider_session` and `last_usage` but deliberately does
  **not** clear `sensitive_tool_ids` — those bodies are still in live history and
  still sensitive.
- The "Before compaction" checkpoint is written on both paths and never deleted.
  The UI transcript keeps only a 160-character summary per tool call, so that
  snapshot's `agent_messages` is the sole durable copy of the bodies a prune just
  shortened.

## Alternatives considered

- **Leave the meter and raise the threshold**: would not help. The reported number
  is not merely off by a factor; it varies with cache hit rate, so no fixed
  threshold against it means anything.
- **Switch `can_compact` to the measured total as well**: rejected. On a
  small-window model the system prompt and tool schemas alone can approach 80%,
  and compaction touches neither, so this would fire repeatedly and achieve
  nothing.
- **Prune on ordinary turns at a lower threshold**: rejected. Rewriting a result
  the model has already seen diverges the cached prefix from that point onward, so
  it would pay a full-price re-read of the conversation to save bytes that the
  cache was already serving at a tenth of the price — working directly against the
  cache fix in `6fae094`.
- **Prune only on the persistence path**: no cache effect at all, but no effect on
  context pressure either, which is the problem being solved.
- **Expose the prune budgets in `zest.toml`**: rejected. Three of the four are
  constrained by the compile-time idempotence assertion, so configuring them turns
  a build error into a startup error.
- **Keep `compact_context() -> Result<String>`**: rejected. An empty summary is
  already an error, so the type has no way to express "there is no summary because
  none was needed".

## Consequences

Long sessions that never auto-compacted will now start compacting. That is the
fix, not a side effect — the previous terminal state was a hard provider error.
Shipping the per-column breakdown in the same change is what makes the new
behavior legible rather than mysterious.

Some compactions now cost nothing and keep the conversation intact. Cache effects,
stated plainly:

- The measurement change moves no wire bytes and has no cache effect.
- The compaction request keeps a byte-identical tools+system prefix, so the
  expensive 1h entry still hits; only the conversation diverges, at the first
  pruned result, and that history is about to be discarded anyway.
- The prune-only path is the one real cost. It rewrites live history, so the next
  ordinary turn's message prefix misses from the first pruned message onward and
  pays one full-price read of the now-smaller conversation. `KEEP_RECENT_MESSAGES`
  does **not** rescue this: a cache lookup matches a prefix, so an earlier
  divergence invalidates the tail regardless of where the breakpoints sit. The
  trade is that one re-read against a 4,096-token model call plus a full-price
  read of a two-message history, and it is taken only when it avoids that call
  entirely.

One over-pressure streak costs at most one prune-only compaction followed by a
real one, bounded by the idempotence invariant.

`ContextUsageView` and the new `CompactionResultView` are hand-written TypeScript,
not ts-rs types, so `scripts/release-verify.ps1`'s binding-drift gate cannot catch
a mismatch between them and `types.ts`. Adding a field to either means editing
`types.ts`, `fixtureBackend.ts`, and the consuming component by hand.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets` — 583 core, 90 desktop, all passing
- `npm run ui:test` (225), `npm run ui:lint`, `npm run ui:build`

New tests pin each invariant above: `the_prompt_total_counts_both_cache_columns`,
`a_well_cached_prompt_is_measured_from_every_column` (with the old 1,200-token
reading recorded in a comment), `pruning_twice_emits_no_second_replacement`,
`slicing_counts_code_points_not_bytes`, `the_most_recent_messages_are_never_pruned`,
`pruning_alone_ends_compaction_without_a_model_call` (a provider that fails if
called at all), `a_second_compaction_summarizes_because_pruning_is_already_done`,
and `a_pruned_sensitive_result_still_reaches_the_provider_redacted`.

Two existing tests were updated: the compaction test now matches on
`CompactionOutcome::Summarized`, and the meter's threshold test moved to
`context_budget` verbatim.

Not verified:

- That char/4 predicts real token savings for any specific provider tokenizer. It
  is a stand-in, chosen to under-count so decisions err toward summarizing.
- That head 4,096 / tail 1,024 preserves enough of a large tool result for a
  summarizer to write a faithful checkpoint. This is a judgement about model
  behavior, not something a unit test can assert.
- The prompt-cache hit-rate effect of the prune-only path on a live session. Only
  the ledger's `served_from_cache_percent` over real turns can answer that.
- Behavior against a live provider. The compaction paths are exercised with fake
  providers only.
