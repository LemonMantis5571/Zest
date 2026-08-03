# Plan 002: Drop Brave path from `web_search` (DuckDuckGo only)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 2e0c60b..HEAD -- crates/core/src/tools/web_search.rs crates/core/src/tools/mod.rs`
> Also read the live file (may be untracked WIP). If `search_brave` /
> `BRAVE_API_KEY` are already gone, mark DONE and stop.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `2e0c60b`, 2026-08-02

## Why this matters

`web_search` already defaults to DuckDuckGo HTML scraping (no API key). The
Brave JSON branch only runs when `BRAVE_API_KEY` is set, adds fallback noise
into results, and roughly doubles the tool’s code. Keep the **capability**
(agent can search the public web); remove the unused second provider until a
real config-backed search provider exists.

## Current state

- `crates/core/src/tools/web_search.rs` — `Tool` impl; `run` checks
  `BRAVE_API_KEY`, calls `search_brave`, falls back to `search_duckduckgo`.
- Module docstring mentions Brave.
- Registered in `register_read_tools` via `crates/core/src/tools/mod.rs`
  (leave registration alone).
- Unit tests cover DDG HTML parse / href decode only — keep those.

Excerpt of the dual path:

```rust
let results = if let Ok(key) = std::env::var("BRAVE_API_KEY") {
    // ... search_brave with DDG fallback inserting a synthetic hit ...
} else {
    search_duckduckgo(query, max).await?
};
```

Also present: `async fn search_brave(...)`, Brave URL
`https://api.search.brave.com/res/v1/web/search?...`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Unit tests | `cargo test -p zest-core web_search` | 2+ tests pass |
| Registry test | `cargo test -p zest-core tools::tests` | pass (includes `web_search` risk) |
| Grep | `rg -n "BRAVE|search_brave|brave" crates/core/src/tools/web_search.rs` | no matches |

## Scope

**In scope**:
- `crates/core/src/tools/web_search.rs`
- `plans/README.md` (status)

**Out of scope**:
- Adding a different search API
- Changing tool name, schema fields (`query`, `max_results`), or registration order
- Desktop UI
- `pdf-inspector`

## Git workflow

- Branch: `advisor/002-web-search-ddg-only`
- Commit message style: `refactor(core): keep web_search on DuckDuckGo only`
- Do NOT push unless asked.

## Steps

### Step 1: Simplify `run`

In `WebSearch::run`, after validating `query` / `max`:

```rust
let results = search_duckduckgo(query, max).await?;
```

Remove the entire `BRAVE_API_KEY` / Brave fallback block.

### Step 2: Delete Brave helpers

Delete `search_brave` and any helpers used only by Brave (e.g. `truncate` if
only Brave used it — check with `rg` inside the file before deleting shared
helpers). Keep DDG parse/decode/unescape/urlencoding and their tests.

### Step 3: Update module docs + tool description if needed

- Module doc: say DuckDuckGo HTML only; no API key; do not mention Brave.
- `description()` may stay focused on when to use search; remove any Brave mention
  if present.

**Verify**:
`rg -n "BRAVE|Brave|search_brave" crates/core/src/tools/web_search.rs` → no matches  
`cargo test -p zest-core web_search` → pass  
`cargo test -p zest-core register_read_tools -- --nocapture` or
`cargo test -p zest-core tools::` → `web_search` still registered as Read.

### Step 4: Update plan index

Status → DONE.

## Test plan

- Keep existing `parse_sample_ddg_html` and `decode_plain_https`.
- Do **not** add network-hitting tests.
- No need for a test that Brave is absent beyond `rg` in Done criteria.

## Done criteria

- [ ] `cargo test -p zest-core web_search` exits 0
- [ ] `web_search` still registered in `register_read_tools`
- [ ] No `BRAVE_API_KEY` / `search_brave` in `web_search.rs`
- [ ] Tool still returns titled links + snippets via DDG
- [ ] Only in-scope files changed
- [ ] `plans/README.md` row 002 → DONE

## STOP conditions

- DDG HTML endpoint shape already broken in live use and fixing it requires a
  new provider — report; do not reintroduce Brave as a silent default.
- `reqwest` is missing from `zest-core` dependencies (it should already be there
  for this tool).

## Maintenance notes

- If a paid search API is wanted later, add it behind explicit config (not a
  raw env branch inside the tool), with tests against a mocked HTTP client.
- Reviewers: confirm registration order in `mod.rs` was not shuffled (prompt
  cache prefixes).
