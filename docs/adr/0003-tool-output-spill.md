# ADR 0003: Store oversized tool output instead of destroying it

- Status: Accepted
- Date: 2026-08-17

## Context

Nine tools each bounded their own output with a private constant and a private
clipper: `bash` at 30 KiB, `grep` at 64 KiB, `read_file` at 256 KiB, plus caps on
`glob`, `list_dir`, `web_search`, `browser`, and two on the external-agent path.
Past those constants the bytes were simply gone. The model was told some were
omitted and had no way to ever see them, so a large build log or a wide search
became permanently unrecoverable mid-conversation.

The clipping code had also multiplied. Seven independent clip functions existed
across the crate, two named `clip_chars` in different modules, and
`floor_char_boundary` / `ceil_char_boundary` were copied byte-identically in three
files. Two of the seven implemented head/tail clipping with different rules about
whether the truncation notice was inside the budget or added to it — `bash` added
it, so it quietly exceeded its own documented cap by the marker's length.

## Decision

Add one shared byte-budget primitive, `zest-core::bounded`, with `ends_within`:
head + tail inside a hard ceiling, with the notice's cost **reserved out of** the
ceiling rather than added to it. Migrate the two head/tail clippers onto it and
delete the three duplicated boundary helpers. The four char-budget clippers stay
where they are — they answer a display-width question, not a wire-cost one.

Add `zest-core::tools::spill`. When a model-facing result exceeds
`[tools] max_result_bytes` (default 32 KiB), the full text is written to
`.zest/spill/<chat-id>/` and the model receives a bounded head/tail preview plus a
project-relative locator and a retrieval hint naming `read_file` and `grep`.

Hook the policy as an `Option` field on `ToolRegistry`, applied inside
`execute_prepared`.

## Invariants

- A replacement never exceeds the cap, and is always strictly smaller than the
  original. When no replacement can honor the cap, the original stays inline and
  **nothing is written** — order is name, clip, write, replace, so a notice that
  cannot fit never leaves an unreferenced artifact behind.
- Clipping never splits a `char`.
- `read_file` is never spilled. The retrieval hint names it, so spilling its output
  would be a read that spills into a read that spills.
- A `ToolRisk::Sensitive` result is never spilled. Such a result already has its UI
  summary suppressed and is redacted out of the delegation handoff; a spill would
  write a second cleartext copy that neither mechanism knows about, and print its
  path into the model-facing body.
- A tool `Err` is never spilled: it is corrective feedback the model needs
  verbatim, and short by construction.
- A storage failure returns the original body. Spilling must never turn a
  successful tool call into a failed one, and must never hide a result.
- Only `body` changes; `ToolMetadata` rides through untouched.
- The locator is project-relative with forward slashes — the form `read_file` and
  `grep` accept, resolved against their own canonical root.
- The store creates no directory until something actually spills.
- A sweep never removes the file just written; a notice already points at it.
- An empty sibling directory is judged by its own age, not treated as residue.
  Another session creates its directory before writing into it, so the opposite
  rule would let one conversation delete another's store during that window.

## Alternatives considered

- **An `Arc<dyn Tool>` decorator instead of a registry field**: rejected. Dispatch
  is the one place both call paths meet — the concurrent batch awaits
  `execute_prepared` directly and never passes through the agent's gated wrapper —
  whereas a decorator must be applied at six `register_*` sites plus two
  concrete-type registrations, and the one you forget is silent. A decorator must
  also forward seven trait methods, where forgetting one fails invisibly: a missed
  `uses_context` quietly stops delegation from receiving conversation context, and
  a missed `input_schema` corrupts the tool list at the front of the cached prompt
  prefix. The honest cost of the chosen design is a policy living in an otherwise
  dumb dispatcher.
- **An absolute path in the locator**, as the design being borrowed from uses:
  rejected. Both read tools resolve through `ProjectRoot`, and an absolute Windows
  path inside a JSON tool argument is a needless quoting hazard.
- **`fsutil::atomic_write`**: rejected. The name is unique so nothing is ever
  replaced and there is no torn reader to protect against, and that helper's
  `sync_all` is precisely the cost `persist::write_off_runtime` exists to keep off
  the runtime — tool results are `PersistPriority::Immediate`, so this would add a
  disk sync to the turn loop for a file that is explicitly disposable.
- **`spawn_blocking` for the write**: rejected. A detached write completes after
  the future is dropped on cancellation and orphans an artifact for a result the
  model never saw. Writing inline with no await between naming and writing makes a
  dropped future *unable* to leave a half-written file.
- **Exempting tools that already bound their own output**: rejected. A policy keyed
  on size stays correct as tools are added; a name list is a second place to
  forget. At a 32 KiB cap only `grep` and `browser` can reach it in practice, and
  `bash`'s worst case is ~30.9 KiB, so the one tool whose markers are pinned by
  tests never also carries a spill notice. Note how narrow grep's reach is: it
  stops at 100 matches and clips each line to 400 characters, so it only crosses
  32 KiB when the matching lines are long — a minified bundle or a one-line JSON
  blob. The default cap therefore fires rarely today; it earns its place as tools
  grow and for the long-line cases where truncation hurts most.
- **Sharding artifacts into retrievable-sized parts**: rejected for now. Fully
  retrievable, but the notice would then have to list N paths, and a 10 MB body
  would produce 40 of them and blow the cap — a second bound for the bound.
- **A `ToolMetadata::Spill` variant for a UI affordance**: deferred. It breaks five
  exhaustive matches plus `ToolMetaView` and its ts-rs regeneration — a desktop and
  UI change inside a core-only slice — and buys little, since the card summary is
  derived from the body and so already carries the locator. It would also age
  badly: `ToolMetadata` is persisted, and a swept artifact leaves a dead button
  where prose degrades into ordinary history.

## Consequences

A truncation becomes a retrieval instead of a loss. `bash` now honors the cap it
documents, and every char-boundary walk in the crate lives in one place.

Two limits are worth stating plainly, because reading the notice as a stronger
promise than it makes would be reading it wrong.

**A dispatch-level policy cannot recover what a tool destroyed before returning.**
`bash` drops the middle of a stream while the command is still running, and `grep`
stops walking once its budget is spent. "Full result" means the complete string the
tool *returned*, which is exactly what the file holds; the earlier losses are
announced inside that string.

**Only the first 256 KiB of an artifact is reachable.** `read_file` counts its
budget from byte zero and applies any offset afterwards, so no offset addresses
content past that point, and `grep` bounds each file the same way *silently*. A
412 KB artifact is therefore ~40% unretrievable. The notice says so when it
applies. The follow-up that removes this — a byte-window mode for `read_file` and
per-file truncation reporting in `grep` — is the single change that would most
increase this feature's value.

Artifacts are disposable by design: no fsync, swept on age, count, and total size.
A locator can outlive its bytes after a crash or a later sweep, and the model gets
a failed read it can adapt to. Storage here is not durable state.

Cache effect: none beyond the ordinary. The spill notice is new content at the tail
of a tool result, so it follows the reusable prefix and invalidates nothing.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets` — 583 core, 90 desktop, all passing
- `npm run ui:test`, `npm run ui:lint`, `npm run ui:build`

The migration step's gate was the entire pre-existing suite passing **unmodified**,
which it did: both middle-omission markers keep their exact text, including the
deliberate `…` versus `...` difference between `bash` and the external-agent path.

New tests pin each invariant: `the_marker_is_paid_for_out_of_the_limit`,
`a_marker_that_cannot_fit_returns_none`, `clipping_never_splits_a_codepoint`,
`clipped_output_honors_the_documented_cap` (the `bash` overshoot this fixed),
`a_locator_is_project_relative_with_forward_slashes`,
`concurrent_stores_in_one_turn_do_not_collide` (32 concurrent writes),
`a_spill_write_failure_returns_the_original_body`,
`a_cap_too_small_for_the_notice_keeps_the_original_body`,
`opening_a_store_creates_no_directory`, `pruning_never_removes_the_file_just_written`,
`the_read_tool_is_never_spilled`, `dispatch_never_spills_a_sensitive_result`,
`a_tool_error_is_never_spilled`, `metadata_survives_a_spill`, and
`deleting_a_thread_removes_its_spilled_tool_output`.

`ThreadStore::delete` also gained a fix noted here rather than smuggled: side-file
cleanup now runs on the missing-file arm too, so re-deleting a thread whose JSON is
already gone still collects its checkpoints and spilled output. Previously it left
them behind for good.

`a_real_grep_spill_can_be_read_back_through_its_locator` closes the loop with the
real tools rather than fakes: a real `grep` over a generated tree spills, then the
real `read_file` retrieves the stored output through the locator, and a real `grep`
searches within it — both with no approval. That test is what established how
narrow grep's reach actually is, noted above.

Not verified:

- Behavior against a live provider. No test drives a real model turn; the
  round-trip above exercises the tools directly.
- Whether 32 KiB is the right default in practice. It is reasoned from bash's
  existing 30 KiB cap and a rough token cost, not measured against real sessions —
  and given how rarely grep crosses it, the default may deserve lowering once
  there is session data.
- The sweep's behavior under a system clock change. Both age arms are covered by
  backdating file times, but a clock that moves backwards makes `duration_since`
  fail, which this treats as "not stale" — artifacts are kept rather than
  wrongly collected.
- Concurrent sweeps within one conversation. Oldest-first ordering means a
  just-written sibling is the last candidate for eviction, so the race is
  reachable only when a directory is far over budget, and its outcome is a stale
  locator — already a documented state.
