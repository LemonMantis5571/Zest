# Plan 003: Front-ends use `DEFAULT_SYSTEM` (dedupe system prompts)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 2e0c60b..HEAD -- crates/core/src/prompt.rs crates/desktop/src/lib.rs crates/cli/src/main.rs`
> Compare live `const SYSTEM` blocks to the excerpts below.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `2e0c60b`, 2026-08-02

## Why this matters

Tool-list wording lives in three nearly identical string constants
(`DEFAULT_SYSTEM` in core, plus local `SYSTEM` in desktop and CLI). Every new
tool (e.g. `web_search`) requires three edits and they already drift. Core
already exports `DEFAULT_SYSTEM` and `RuntimeBuilder` uses it. Point desktop
and CLI at that constant so one edit updates all front-ends.

**Acceptable capability trade**: desktop/CLI-specific flavor sentences
(“running in a desktop app…”, “CLI currently auto-denies writes…”) go away.
That is intentional for this slim pass — behavior of tools/approvals does not
change; only the base prompt text unifies. Approval rules remain enforced in
code (`Approver`), not by prompt prose.

## Current state

Canonical (keep and edit here only when tools change):

```rust
// crates/core/src/prompt.rs
pub const DEFAULT_SYSTEM: &str = "\
You are Zest, a coding agent inside the user's project. You have project tools \
(list_dir, glob, grep, read_file, write_file) scoped to that project, plus \
web_search for public docs and current information. Explore and read the project \
before answering codebase questions. write_file requires user approval. Keep \
responses focused.";
```

Already re-exported from `zest_core` (`crates/core/src/lib.rs` exports
`DEFAULT_SYSTEM`).

Duplicates to remove:

```rust
// crates/desktop/src/lib.rs ~line 40
const SYSTEM: &str = "\
You are Zest, a coding agent running in a desktop app ...";
// used as .with_system(SYSTEM) ~line 859
```

```rust
// crates/cli/src/main.rs ~line 11
const SYSTEM: &str = "\
You are Zest, a coding agent running in a terminal ...";
// used as .with_system(SYSTEM) ~line 68
```

CLI already imports other symbols from `zest_core`; add `DEFAULT_SYSTEM` to
that import list. Desktop already imports `compose_system` from `zest_core` —
add `DEFAULT_SYSTEM` there too.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Check | `cargo check -p zest-cli -p zest-desktop -p zest-core` | exit 0 |
| Grep local SYSTEM | `rg -n "^const SYSTEM" crates/desktop/src/lib.rs crates/cli/src/main.rs` | no matches |
| Grep with_system | `rg -n "with_system\\(DEFAULT_SYSTEM\\)|with_system\\(SYSTEM\\)" crates/desktop crates/cli` | only `DEFAULT_SYSTEM` |

## Scope

**In scope**:
- `crates/desktop/src/lib.rs` — delete `SYSTEM`; use `DEFAULT_SYSTEM`
- `crates/cli/src/main.rs` — delete `SYSTEM`; use `DEFAULT_SYSTEM`
- `crates/core/src/prompt.rs` — **only if** needed to ensure `web_search` is
  mentioned once (it already is at plan time); do not invent desktop/CLI forks
- `plans/README.md`

**Out of scope**:
- Changing `compose_system` / `.zest/system.md` behavior
- Worker prompt in `tools/external_agent.rs` (`EXTERNAL_WORKER_SYSTEM`)
- UI copy in Settings that shows the effective prompt (it reads from session)

## Git workflow

- Branch: `advisor/003-dedupe-default-system`
- Commit: `refactor: use DEFAULT_SYSTEM in desktop and CLI`
- Do NOT push unless asked.

## Steps

### Step 1: Desktop

1. Add `DEFAULT_SYSTEM` to the `use zest_core::{...}` import in
   `crates/desktop/src/lib.rs`.
2. Delete the `const SYSTEM: &str = "...";` block.
3. Replace `.with_system(SYSTEM)` with `.with_system(DEFAULT_SYSTEM)`.

**Verify**: `rg -n "const SYSTEM" crates/desktop/src/lib.rs` → no matches;
`cargo check -p zest-desktop` → exit 0.

### Step 2: CLI

1. Add `DEFAULT_SYSTEM` to the `zest_core::{...}` import in
   `crates/cli/src/main.rs`.
2. Delete `const SYSTEM`.
3. Replace `.with_system(SYSTEM)` with `.with_system(DEFAULT_SYSTEM)`.

**Verify**: `rg -n "const SYSTEM" crates/cli/src/main.rs` → no matches;
`cargo check -p zest-cli` → exit 0.

### Step 3: Confirm single source of truth

Ensure `DEFAULT_SYSTEM` in `prompt.rs` still lists `web_search` and the core
tools. Do not reintroduce front-end-specific constants.

**Verify**:
`rg -n "DEFAULT_SYSTEM" crates/core/src/prompt.rs crates/desktop/src/lib.rs crates/cli/src/main.rs`  
→ definition in prompt.rs; uses in desktop + cli.

### Step 4: Update plan index → DONE

## Test plan

- No new tests required (string constant wiring).
- If any characterization test snapshots the exact desktop/CLI system string,
  update that test to expect `DEFAULT_SYSTEM` — search with
  `rg -n "coding agent running in" crates/` and fix hits.

## Done criteria

- [ ] No `const SYSTEM` in desktop or CLI
- [ ] Both call `.with_system(DEFAULT_SYSTEM)`
- [ ] `cargo check -p zest-core -p zest-cli -p zest-desktop` exits 0
- [ ] `rg -n "coding agent running in a (desktop|terminal)" crates/` → no matches
      in Rust sources
- [ ] Only in-scope files changed
- [ ] `plans/README.md` row 003 → DONE

## STOP conditions

- Desktop/CLI **must** keep divergent legal/policy text (unlikely) — stop and
  report rather than inventing a composition API.
- `DEFAULT_SYSTEM` is not exported from the `zest_core` crate root (it should
  be; fix export only if missing, still in-scope via `lib.rs` if needed — add
  `crates/core/src/lib.rs` to scope only for a missing re-export).

## Maintenance notes

- Future tool list changes: edit `crates/core/src/prompt.rs` only.
- Reviewers: confirm Settings “system prompt” base view still shows the composed
  prompt from the session (uses `base_system` from runtime, which will now be
  `DEFAULT_SYSTEM`).
