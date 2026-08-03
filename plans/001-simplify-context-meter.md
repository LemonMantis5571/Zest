# Plan 001: Simplify context meter to used/window only

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 2e0c60b..HEAD -- crates/core/src/agent.rs crates/desktop/src/context_meter.rs crates/desktop/ui/src/components/ContextUsageButton.tsx crates/desktop/ui/src/lib/types.ts crates/desktop/ui/src/lib/backend.ts`
> Also open the live uncommitted copies of those paths — this work was planned
> against WIP that may still be uncommitted. If excerpts below do not match,
> STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `2e0c60b`, 2026-08-02

## Why this matters

The context chrome advertises “tokens to compact” and a soft 85% compact
threshold even though auto-compact is not shipped. It also builds a colored
three-part breakdown and forces `Agent::tools_wire_chars()` to stringify every
tool schema just for an estimate that is ignored when last-turn API usage
exists. Keep the **capability** (show how full the window is) but drop the
fiction and the extra Agent API.

## Current state

- `crates/desktop/src/context_meter.rs` — builds `ContextUsageView` with
  `parts`, `compact_at_tokens`, `remaining_before_compact`, and calls
  `agent.tools_wire_chars()`.
- `crates/core/src/agent.rs` — `pub last_usage: Option<Usage>` (keep) and
  `pub fn tools_wire_chars` (remove after meter stops using it).
- `crates/desktop/ui/src/components/ContextUsageButton.tsx` — ring +
  `"{pct}% · {formatTokens(usage.remainingBeforeCompact)} to compact"` and a
  popover listing `usage.parts`.
- `crates/desktop/ui/src/lib/types.ts` — `ContextPart` + compact fields on
  `ContextUsage`.
- `crates/desktop/ui/src/lib/backend.ts` — fixture `contextUsage()` returns the
  fat shape (~line 312).

Excerpts (WIP at plan time):

```rust
// crates/desktop/src/context_meter.rs
const COMPACT_FRACTION: f64 = 0.85;
// ...
pub struct ContextUsageView {
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub remaining_tokens: u64,
    pub percent_full: f64,
    pub compact_at_tokens: u64,
    pub remaining_before_compact: u64,
    pub source: String,
    pub parts: Vec<ContextPart>,
}
```

```rust
// crates/core/src/agent.rs
pub fn tools_wire_chars(&self) -> usize {
    self.tools
        .definitions()
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum()
}
```

**Conventions**: Serde views use `#[serde(rename_all = "camelCase")]`. UI types
mirror that in `types.ts`. Match existing desktop command style in
`crates/desktop/src/lib.rs` (`context_usage` command stays; only the view
shape changes).

**Product constraint** (from `PROJECT_CONTEXT.md`): usage accounting must be
honest — label estimates as estimates; never present a guess as a reading.
Keep `source: "last_turn" | "estimate"`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Core check | `cargo check -p zest-core` | exit 0 |
| Desktop check | `cargo check -p zest-desktop` | exit 0 |
| UI typecheck/build | `cd crates/desktop/ui && npm run build` | exit 0 |
| UI tests | `cd crates/desktop/ui && npm test` | all pass |
| UI lint | `cd crates/desktop/ui && npm run lint` | exit 0 |
| Grep dead API | `rg -n "tools_wire_chars|ContextPart|compactAtTokens|remainingBeforeCompact|to compact" crates/` | no matches (except this plan / changelog if any) |

On Windows PowerShell, `cd` then run npm; or
`npm --prefix crates/desktop/ui run build`.

## Scope

**In scope**:
- `crates/core/src/agent.rs` — delete `tools_wire_chars` only
- `crates/desktop/src/context_meter.rs` — slim `ContextUsageView` + `estimate_context`
- `crates/desktop/ui/src/components/ContextUsageButton.tsx`
- `crates/desktop/ui/src/lib/types.ts`
- `crates/desktop/ui/src/lib/backend.ts` (fixture shape)
- `plans/README.md` (status row)

**Out of scope**:
- Real compaction / summarization implementation
- Changing how `last_usage` is recorded in the agent loop
- `pdf-inspector`, attachments, DiffViewer, web_search
- Splitting `lib.rs`

## Git workflow

- Branch: `advisor/001-simplify-context-meter` (optional if operator works on WIP)
- Commit style (from recent log): conventional, e.g.
  `refactor(desktop): simplify context meter to used/window`
- Do NOT push or open a PR unless asked.

## Steps

### Step 1: Slim Rust `ContextUsageView`

In `crates/desktop/src/context_meter.rs`:

1. Delete `ContextPart` and `COMPACT_FRACTION`.
2. Change `ContextUsageView` to only:
   - `used_tokens`, `window_tokens`, `remaining_tokens`, `percent_full`, `source`
3. Rewrite `estimate_context`:
   - `window = context_window_for_model(&agent.model)` (keep existing helper)
   - If `agent.last_usage` has `input_tokens > 0` → `used = input_tokens`,
     `source = "last_turn"`
   - Else estimate: sum `chars/4` over `agent.system` (if any) and each
     message’s `content` serialized the same way as today
     (`b.to_string().len()`), **without** tool-definition sizing
   - Fill remaining / percent as today
4. Remove any call to `tools_wire_chars`.

**Verify**: `cargo check -p zest-desktop` → compiles (may fail until step 2 if
UI types still expect old fields — that is OK only if you do Rust+TS in one
sitting; prefer finishing step 2 before claiming green).

### Step 2: Remove `tools_wire_chars` from Agent

In `crates/core/src/agent.rs`, delete the entire `tools_wire_chars` method.
Keep `last_usage`.

**Verify**: `rg -n "tools_wire_chars" crates/` → no matches;
`cargo check -p zest-core -p zest-desktop` → exit 0.

### Step 3: Slim TS types + fixture

In `types.ts`, replace `ContextUsage` with:

```ts
export type ContextUsage = {
  usedTokens: number;
  windowTokens: number;
  remainingTokens: number;
  percentFull: number;
  source: string;
};
```

Delete `ContextPart`.

Update `backend.ts` fixture `contextUsage()` to return only those fields
(drop `compactAtTokens`, `remainingBeforeCompact`, `parts`).

**Verify**: `rg -n "ContextPart|compactAtTokens|remainingBeforeCompact" crates/desktop/ui` → no matches.

### Step 4: Simplify `ContextUsageButton`

- Button label: `{pct}% · {formatTokens(usage.remainingTokens)} left`
  (not “to compact”).
- Popover: show percent, `~used / window`, and one line for source
  (`last API turn` vs `estimate`).
- Delete the colored bar over `parts` and the compact-threshold footer copy.

Keep the ring visualization driven by `percentFull`.

**Verify**: `npm --prefix crates/desktop/ui run build` → exit 0;
`npm --prefix crates/desktop/ui test` → all pass;
`npm --prefix crates/desktop/ui run lint` → exit 0.

### Step 5: Update plan index

Set plan 001 status to DONE in `plans/README.md`.

## Test plan

- No new Rust unit test required if behavior is a pure field deletion; optional
  tiny test in `context_meter.rs` for “last_usage wins over estimate” is nice
  but not required.
- Existing UI tests should still pass (they do not assert context meter shape
  today — if any fail, update fixtures only).

## Done criteria

- [ ] `cargo check -p zest-core -p zest-desktop` exits 0
- [ ] `npm --prefix crates/desktop/ui run build` exits 0
- [ ] `npm --prefix crates/desktop/ui test` exits 0
- [ ] `rg -n "tools_wire_chars|ContextPart|to compact|compactAtTokens|remainingBeforeCompact" crates/` returns no code matches
- [ ] Context button still shows a % and remaining tokens; source label remains
- [ ] No files outside Scope modified
- [ ] `plans/README.md` row 001 → DONE

## STOP conditions

- DiffViewer, attachments, or web_search appear to need changes for this compile
  to succeed (they should not).
- `last_usage` / `Usage` types moved or renamed so the meter cannot read
  `input_tokens`.
- Compaction was implemented since this plan — then re-evaluate instead of
  deleting compact UX.

## Maintenance notes

- When real compaction ships, add a **separate** control that triggers it; do
  not revive a fake “to compact” countdown without a button that does something.
- Reviewers: ensure `source` still distinguishes estimate vs last turn.
