# Changelog

Track notable changes here.

## 2026-08-02 — streaming: the two remaining stalls

Follow-on to the entry below, after checking how opencode handles bursts. The
pacing was already right; the frames it was pacing into were too expensive.

- **Markdown re-parsed the whole message on every frame.** `lib/markdownBlocks.ts`
  splits the text into top-level blocks and each is its own memoized component,
  so a growing message re-parses only its last block. The split is deliberately
  conservative — it never divides a fence, a loose list, or a multi-paragraph
  list item — because a wrong split is a visible reflow, while a missed one only
  costs a little work. Tested for append-only stability: appending a character
  must never change an earlier block's key.
- **Shiki ran on the main thread.** Highlighting one block costs upwards of
  100 ms, which is dropped frames no matter how well the reveal is paced. It now
  runs in `highlight.worker.ts`, with `highlight.ts` as a per-block
  latest-value queue in front of it — a superseded request is dropped, not
  queued. Failure (CSP, no worker support) leaves the plain-text fallback that
  was already rendering. Side benefit: the main bundle went 2,829 kB → 572 kB,
  because the grammars now load with the worker.

## 2026-08-02 — streaming reads as typing again

Three separate causes of "text just appears in lurches":

- **Every finished message re-parsed its markdown on every frame** of the
  streaming one. `Markdown` is now memoized; the reducer already preserves
  object identity for messages it did not touch, so unchanged ones are skipped
  outright. Biggest win, and it grows with conversation length.
- **A code block re-highlighted on every frame while it grew.** Highlighting now
  waits 120 ms for the block to stop changing — the colours are only correct
  once it closes anyway. Plain text renders immediately in the meantime.
- **The gateway does not stream token by token.** It hands over whatever it
  buffered, so a turn arrives as a few large bursts. `lib/reveal.ts` paces the
  reveal per frame, proportional to the backlog: the drain is exponential, so
  time-to-empty grows logarithmically and stays under a second even for a whole
  answer delivered in one event. A `done` event drains the buffer completely
  first, so text can never land after the event that ended the turn.

## 2026-08-02 — command output renders as a document

- **The answer to a slash command gets a card** — titled header, copy,
  save-as-`.md`, collapse — instead of blending into the chat stream. A plan is
  a document; it should not look like a reply.
- **Generic by command name**, so a new `.zest/skills/<name>/SKILL.md` gets the
  same treatment with no code. Same rule as commands themselves: markdown, not
  Rust.
- **Rust decides what counts.** `assistant_start` carries the command, because
  only Rust knows whether a leading `/token` matched a real skill — an unknown
  one is sent as plain text and must render as plain text.
- **Persisted on the thread**, so reopening a chat still frames an old plan as a
  plan rather than silently degrading to markdown. Optional field; older threads
  load unchanged.
- Tool rows stay outside the card: they are how the answer was reached, not part
  of it.

## 2026-08-02 — Settings is readable during a turn

- **Fix: opening Settings mid-turn blanked the panel** with
  `{"code":"busy"}`. `begin_turn` *takes* the session out of the controller, so
  anything reaching through it is unreadable while a turn streams — and
  `Promise.all` meant that one rejection wiped Usage and Providers too, which
  need no session at all.
- `list_skills` and `list_commands` now discover from the workspace instead of
  the session. Disk is readable whenever; the session is not.
- Settings loads its four sections with `allSettled`, so each reports its own
  failure and the others still render.
- The busy message says what to do — "the assistant is still working — stop it
  or wait" — rather than describing internal state.

## 2026-08-02 — routing panel says which "default" it means

- **Fix: the same-account note was inverted.** It compared each rule against
  `[routing].default` instead of the provider the open chat is pinned to, so a
  chat started on Claude saw the note on the wrong rule. Now uses the session
  provider.
- **It is a note, not a warning.** Delegating to the provider you are already on
  gives a worker with no conversation history — context isolation, not a
  misconfiguration. Only the expectation of a different model can be wrong.
  Disabling such rules was rejected: config is machine-wide, so a rule that is
  pointless in one chat is correct in another.
- **The panel states both defaults** — "this chat runs on X" and "unmatched
  kinds go to Y". A config comment claiming `[routing].default` decides the chat
  provider was false for the desktop: the launcher picker decides, and the
  config default is the delegation fallback plus the CLI's chat provider.

## 2026-08-02 — auth that tells the truth

- **Sign-ins are verified.** After Connect, one minimal turn goes through the
  provider; "Signed in" now means *served a request*, not "a parseable file
  exists". Caught nothing before because nothing checked — a Claude account in
  CLIProxyAPI cooldown looked green and 503'd on first use.
- **Reconnect from the failure.** Auth-shaped errors render plain language plus
  a Reconnect button for that provider, instead of a raw JSON envelope with no
  way out. `HarnessError::is_auth_problem()` reads the body, not just the status
  — a gateway reports a dead account as 503, same as ordinary overload. Narrow
  on purpose: rate limits and bad requests get no Reconnect, since signing in
  again fixes neither.

## 2026-08-02 — routing safeguards

- **Apply now** — saving routing used to take effect only after a restart, with
  no sign that it hadn't. New chat swaps the thread but keeps the agent, so the
  tool registry (and therefore `delegate`) was unchanged. The button rebuilds the
  session; the sticky thread reloads, so the open chat survives.
- **Suggest rules** — derives a starting set from the providers actually
  configured. It cannot know which model is best (there is no pricing or
  capability data, only ids), so what it *guarantees* is structural: every
  suggested rule reaches a different provider than the one chats start on.
  The `mechanical` rule picks a small-looking model by name and pins it to `low`
  effort, since a cheap model at max effort is not cheap.
- **Resolved preview + cross-provider warning** — each rule shows what it really
  uses (`claude · claude-opus-5 · high`) instead of "Provider default", and flags
  rules pointing at the provider chats already start on. That case spawns a fresh
  worker on the same model — it looks like cross-model routing and isn't.

## 2026-08-02 — slash commands and orchestrated fan-out

- **Slash commands are skills.** `/plan` runs `.zest/skills/plan/SKILL.md`
  against whatever follows it, so a new command is a markdown file rather than a
  code change. Palette in the composer (`↑↓`, `Tab`/`Enter`, `Esc`); the
  transcript keeps the typed text, not the expansion. An unknown `/token` is
  sent as-is, and `//` escapes to a literal slash. Ships with `/plan`.
- **`delegate` is `ToolRisk::Exec`** and obeys the permission mode — it was
  read-risk, so a fan-out across three accounts happened silently even in
  Manual. The route resolves in `prepare()` so the card names the provider and
  model that will be spent, and `run()` aborts if the route changed in between.
- **Per-rule `effort` and `prompt`** replace the hardcoded `high` and the single
  generic worker system prompt. Validated against the model's own effort list.
- **Delegation guidance** is added to the system prompt **only** when the tool
  is actually registered, so single-provider users carry no dead instructions in
  their cached prefix. It is what tells the model to batch independent
  delegations into one turn, which is what makes them run concurrently.

## 2026-08-02 — opt-in delegation with a routing editor

- **Delegation is now opt-in.** `[routing] delegation = false` by default, and
  off means the `delegate` tool is not registered at all. Previously it appeared
  automatically as soon as a second provider loaded — which is what happened the
  moment Claude was configured.
- **Settings → Routing** — toggle plus rule rows (kind → provider + model) with
  model dropdowns fed by each provider's real catalogue. Validated in Rust
  against live catalogues before saving; writes to `~/.zest/zest.toml` via
  `toml_edit` so hand-written comments survive. Applies to the next chat.
- **Task kinds come from the rules** and reach the model as a schema enum, so it
  picks one you defined or none.
- **Sidebar tags chats by provider** when a project has chats under more than
  one. Threads are provider-immutable by design (wire history carries
  provider-specific thinking signatures and tool shapes), so switching provider
  shows a different set — this makes that legible instead of looking like loss.

## 2026-08-02 — permission modes + transcript readability

- **Permission modes** — Manual / Accept edits / Plan / Auto / Bypass, picked from
  the composer footer (positioned panel, number keys 1-5). `ApprovalPolicy` is
  consulted before the `Approver`, so there is still one path for "may this run".
  Plan mode **refuses** writes and commands with a reason the model reads, rather
  than queueing a card nobody wants to click. Desktop opens in Auto; core defaults
  to Manual so an un-wired `Agent` can never be permissive.
- **Allow for session** — third button on the approval card. Grants
  `(tool, target)`: this file, or this exact command — never the whole tool.
  Changing mode drops every grant.
- **`bash` risk is now always `Exec`** — the allowlist sets `auto_eligible`
  instead of downgrading to `Read`, so Manual mode really can confirm
  `cargo check` while Auto still runs it silently. The metacharacter rule is
  unchanged and still tested.
- **Collapsed tool runs** — five or more consecutive finished rows fold into
  "Ran 2 commands, edited 3 files +30 -12", expandable. Running and
  awaiting-approval rows never fold; a hidden failure is surfaced on the summary.
- **Fix: a stale sticky model could permanently strand a provider** — the
  user-level `last-model` scalar (single-provider era, never deleted) was read as
  a fallback for every provider, so Claude inherited `gpt-5.6-luna` from Codex and
  then failed validation. Since the only way to change a sticky model is to start
  a session, the provider became unreachable. Fixed at both ends:
  `migrate_legacy` no longer reads user-level scalars, and `RuntimeBuilder` now
  distinguishes explicit from *remembered* options — explicit still errors,
  remembered is dropped with a warning on `SessionInfo.warning`. `ZEST_MODEL` /
  `ZEST_EFFORT` are treated as remembered for the same reason.
- **Fix: picker offered providers it could not use** — `selectable` now requires
  a config entry as well as a sign-in. A signed-in provider with no
  `[providers.<id>]` shows "Not configured" and names the file to add it to,
  instead of showing green "Signed in" and failing after Continue.
- **Fix: `****` in thinking output** — summarized thinking arrives as a run of
  `**Title**` blocks and `joinThinkingStream` welded one block's closing marker to
  the next block's opening one, producing invalid emphasis that rendered as
  literal asterisks.

## 2026-08-02 — speed floor + reliability

- **`edit_file`** — exact string replace on an existing file (`replace_all` opt-in,
  ambiguous match rejected with the count). Computes the new body at prepare time
  and returns `PreparedKind::WriteFile`, so the BLAKE3 pre-image, bounded diff,
  approval card, and atomic replace are reused unchanged. A three-line change no
  longer costs the whole file in output tokens.
- **`read_file` windows** — `offset` / `limit`, `cat -n` line prefixes, cap raised
  64 KiB → 256 KiB. Zest can now read its own `crates/desktop/src/lib.rs` (67,806
  bytes), which the old cap made impossible. Byte-truncated tails drop the sliced
  line rather than presenting a partial one as whole. `grep`'s per-file cap moved
  with it.
- **Concurrent tools** — ungated calls in one batch run through `join_all`;
  gated ones stay strictly sequential and run after. Results and events are
  emitted in **call order**, never completion order (proved by test).
- **`bash`** — behind the approval gate, no OS sandbox. Metacharacter-free
  read-only commands auto-run via argv spawn with no shell; everything else shows
  the exact command line. `[tools.bash]` config; timeouts kill the child;
  `kill_on_drop` covers cancellation. Not registered for delegated workers or
  `doctor --live`. CLI gained a stdin y/N approver.
- **Pre-stream retry** — 3 attempts on connect/timeout and 408/429/500/502/503/529,
  never once bytes have streamed. Honours `retry-after` ≤60s, else ~1/2/4s jitter;
  the sleep races cancel. Exhausted attempts say so in the error.
- **Prompt caching** — provider-gated on `supports_prompt_cache()` (native
  Anthropic only). Breakpoints at end of tools, end of system, and rolling on the
  second-to-last message. Thinking blocks are never annotated. Gateways see no
  `cache_control` field at all.
- **System prompt** — `DEFAULT_SYSTEM` now states tool-call batching, edit_file
  preference, and verify-with-bash. `AGENTS.md` / `CLAUDE.md` / `PROJECT_CONTEXT.md`
  are discovered from the project root (16 KiB shared budget) and composed after
  the user's `.zest/system.md`. `env_context` (cwd, platform, git branch, top-level
  listing) is appended **after** the cached region so a branch change does not
  cost a cache miss.
- **Fix: opening a folder with no `zest.toml`** — `Config::find` now falls back to
  `~/.zest/zest.toml` before the env fallback, and `load_env()` falls back to
  `~/.zest/.env`, so providers *and* their keys follow the machine rather than the
  repository. Previously any folder without its own config failed to open with
  "provider `codex` is configured but could not be loaded" — a message that was
  also wrong, since codex was not in that config at all. `RuntimeBuilder` stopped
  discarding the registry's skip reasons: a load failure now quotes the missing env
  var, and an unconfigured provider says so and names where to put a config.
- `display_path` / `display_path_str` moved from `desktop` into `zest_core::fsutil`
  (one copy, used by both) — a raw `\\?\D:\…` prefix was leaking into the error
  above. Regression test asserts the prefix never reaches user-facing copy.
- Docs: `PROJECT_CONTEXT.md` exec constraint amended; five new `decisions.md`
  entries; README tools/commands/configuration sections and two troubleshooting
  rows; `zest.toml` gained `[tools.bash]`.

## 2026-08-02

- Desktop chat shell (post–Milestone 1 UX):
  - **Projects sidebar** — chats grouped by known project folders
    (`~/.zest/known-workspaces.json`); open folder / switch project / nested
    delete with confirm; sticky provider when jumping projects.
  - **Attachments** — upload + paste images; PDFs via `pdf-inspector`; multimodal
    user turns (`Agent::send_blocks_cancellable`).
  - **Diff viewer** — click write-tool diffs for a full-panel view; path/diff kept
    after tool completion.
  - **Context meter** — used/window % (last-turn tokens or estimate); no fake
    “to compact” UI until compaction ships.
  - **User profile** — display name + optimized 128px JPEG avatar under
    `~/.zest/avatar.jpg` (not a fat data-URL in JSON).
  - **Chat polish** — blue linkified URLs in user bubbles; quieter tool rows;
    borderless grey thinking/clarifying text; sentence-aware thinking join.
- Core: `web_search` tool (DuckDuckGo HTML, no API key); registered with read
  tools. Desktop/CLI use shared `DEFAULT_SYSTEM` from `prompt.rs`.
- Docs: README features, architecture tools list, glossary, decisions for
  system-prompt dedupe / DDG-only search / known workspaces / avatar storage.
- Milestone 1 (usage/routing UX): Rust-authoritative `ProviderView` + session
  `models`/`defaultModel`; `usage_snapshot` with Measured by Zest / Provider
  reported / Not reported; delegation `ToolMetadata` side-channel and thread
  format v2; Settings Usage section; invalid delegated-model skip; opt-in
  `zest doctor --live --dual`.
- Milestone 0 (Stable Windows Alpha): provider-immutable threads + project-scoped sticky
  prefs map; typed thread load outcomes (refuse newer formats); centralized atomic
  persistence; async `CancelToken` with HTTP abort, `message_stop` requirement, idle/
  connect timeouts; sensitive direct-file grep approval + redacted persisted wire history;
  prompt/skill/read/write bounds; desktop Stop control, Strict Mode subscription dispose,
  delta merge, settings load failures, canonical project root; `verify.ps1` gate
  (`npm ci` → ui test/lint/build → fmt/clippy/test → binding drift → audit → RustSec);
  doctor `--live` reloads ledger from disk before success.
- Docs: fresh-install README for https://github.com/LemonMantis5571/Zest, CONTRIBUTING,
  expanded `.gitignore` (secrets, `.zest` state, `tools/`, Node, OS junk).
- System prompt: custom `.zest/system.md` is authoritative (placed first; softens
  “You are Zest…”). Settings sidebar uses shadcn Collapsible sections (Provider /
  System prompt / Skills / Chats).
- Chat UX: `assistant_start` event so Thinking… appears before the first token;
  Working… between tool rounds; rAF-coalesced text/thinking deltas.
- Custom system prompt: Settings editor → `.zest/system.md`, appended after the
  base Zest prompt; hot-reloads the live agent.
- Cursor-style skills: discover `.zest/skills/*/SKILL.md` and `~/.zest/skills/*/SKILL.md`,
  catalogue (+ small bodies inlined) in the system prompt, `read_skill` tool for the rest.
- `.gitignore`: ignore threads/system.md under `.zest/`, allow committing `.zest/skills/`.
- Fix: provider `codex` with omitted `models` now accepts the built-in Sol/Terra/Luna
  catalogue (was default-only, which rejected sticky `gpt-5.6-luna` on Continue).
- Alpha §4 desktop contract: injected Tauri/fixture backend, approval resolve promise +
  restore on failure, Rust-authoritative model/effort with rollback, chatReducer helpers/
  tests (no legacy `tool_call`), ts-rs `ChatEvent`/`SessionInfo` under `ui/src/lib/generated`,
  production CSP (bundled + IPC) with localhost only in `tauri.dev.conf.json`.
- Alpha §5 prove/routing: provider-owned `ModelSpec` / `ProviderDescriptor`; gateway optional
  `models`/`efforts` (omit models → default only); validate on `RuntimeBuilder` + desktop
  `update_session_options`; delegated workers via `RuntimeBuilder` when multi-provider;
  deterministic fake-provider proofs; opt-in `zest doctor --live` (README read-only turn).
- Alpha §3 deterministic turns/threads: transactional `Agent` wire history, desktop
  `SessionController` (monotonic session id, one turn, cancel), `cancel_turn`, required
  session/thread/turn ids on chat events (React drops stale), coalescing `PersistWorker`
  (≤250ms text checkpoints), versioned thread JSON + corrupt preserve + restart
  terminalize, CLI/desktop via `RuntimeBuilder` with desktop delegate when multi-provider.
- Alpha §2 tool/approval integrity: `PreparedToolCall`, BLAKE3-bound writes, `similar` hunks,
  atomic Windows replace, ignore-aware walker, sensitive-path gate, validated `ThreadId`,
  Unicode-safe grep clipping.
- Alpha guardrails: `rust-toolchain.toml` (1.97.1), `.nvmrc` / engines (Node 24.16.0, npm 11.13.0),
  root npm workspaces, `scripts/verify.ps1`, `.github/workflows/windows-verify.yml`.
- Desktop: chat history under `.zest/threads/`, settings sheet, model/effort picker, approval UI
  for `write_file`.
- Core tools: `list_dir`, `glob`, `grep`, `write_file` + session-scoped Approver; read tools via
  `register_read_tools`.
- Decision: Stable Windows Alpha — delegated workers as v1 multi-provider routing; reliability
  before more tools.
