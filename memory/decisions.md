# Decisions

Track important decisions here.

## Format

### YYYY-MM-DD — Decision Title

Decision:

Reason:

Impact:

---

### 2026-08-02 — CSP `worker-src` relaxed from `'none'` to `'self'`

Decision: the production CSP in `tauri.conf.json` now allows `worker-src 'self'`
(was `'none'`); the dev config additionally allows `blob:` and the Vite origin.
Vite is configured with `worker: { format: "es" }` so the production build emits
the worker as a real file rather than a blob — `'self'` alone is enough, and
`blob:` stays out of the shipped policy.

Reason: Shiki on the main thread costs upwards of 100 ms per block, which no
amount of reveal pacing can hide. Moving it to a worker is the only fix that
addresses the cause, and it needs `worker-src`.

Impact: narrow. `'self'` permits only workers built from the bundled app origin,
and exploiting it presupposes script execution in the page — at which point the
attacker already has the main thread and a worker adds no capability (no IPC, no
filesystem, only code that already ships). The directive that would have mattered
is `blob:`/`data:`, which allows constructing worker source from a string at
runtime; that stays out of production and appears only in the dev config.
Production `script-src` remains a clean `'self'`, and `object-src`, `frame-src`,
and `form-action` remain `'none'`.

Note on the prior state: `worker-src 'none'` was not a considered rejection of
workers. It arrived in `f292263`, the initial Tauri shell commit, as one line of
a policy written deny-by-default in a single pass, and was never revisited. So
this is that same hygiene restated once a worker actually exists — not a reversal
of a weighed decision. **If the worker is ever dropped, put `'none'` back**, for
the same reason it was there to begin with.

The real injection surface to watch is `CodeBlock.tsx`, which feeds the
highlighter's output to `dangerouslySetInnerHTML`. That, not `worker-src`, is
where a markup-escaping bug would actually cost something.

---

### 2026-08-02 — Same-provider delegation is a feature, and two things are called "default"

Decision: a routing rule that targets the provider the open chat already uses is
**allowed and not flagged** — the panel notes it in muted text as "a fresh worker
with no shared context, not a different model". Disabling such a rule was
considered and rejected. The Routing panel states both "this chat runs on X" and
"unmatched kinds go to Y" side by side.

Reason: delegating to the same provider still produces a worker with **no
conversation history**, which is context isolation — a legitimate technique, not
a misconfiguration. Only the *expectation* of a different model can be wrong.
Disabling would also be incoherent: rules live in `~/.zest/zest.toml` and apply to
every project, so a rule that is useless in a Claude chat is correct in a Codex
one, and blocking it would mean switching providers just to author config.

The confusion it caused was mine twice over. The per-rule warning compared
against `[routing].default` instead of the session provider, so it was exactly
inverted for a chat started on the non-default provider. And a comment in the
config claimed `[routing].default` "decides where a new chat starts" — false for
the desktop, where the launcher picker decides and
[runtime.rs](crates/core/src/runtime.rs) only falls back to the config default
when no provider is passed (the CLI's path). Two distinct facts wearing one name,
with a warning built on the wrong one.

Impact: `RoutingSettings` takes `sessionProvider`. Anything session-scoped that
comments on machine-wide config has to name which it means.

### 2026-08-02 — A credentials file is not a working session

Decision: after a sign-in completes, send one minimal turn (`probe`, `max_tokens: 1`,
thinking off) and only then report success. And when a turn fails with an
auth-shaped error, the chat shows plain language plus a **Reconnect** button for
that provider rather than the raw error envelope.

Reason: `gateway_auth_for` decided "Signed in" from a filename prefix plus
JSON-parseability. CLIProxyAPI held a Claude account it had put in cooldown — the
file was present, parseable and unexpired, so the picker showed green and the
first real turn returned 503 `auth_unavailable`. Worse, because the status said
Ready, nothing in the failure pointed back at the fix: the picker's Reconnect
button existed but there was no reason to think it was needed.

The project already refuses to present a guess as a reading for usage numbers.
Auth status was doing exactly that, and this holds it to the same standard.

Impact: `HarnessError::is_auth_problem()` reads the *body*, not just the status —
a gateway reports a dead account as 503, which is indistinguishable from ordinary
overload otherwise. Deliberately narrow: rate limits and bad requests do not get
a Reconnect, because signing in again fixes neither. `ChatEvent::Error` gained
`reconnect_provider`, set only for that class. The probe costs a few tokens, so
it runs on an explicit sign-in and never on a render.

Not fixed: the picker still derives its resting status from the filesystem, so a
session that dies *after* a verified login shows green until the next turn fails.
Probing on every render would cost tokens for a status nobody asked for.

### 2026-08-02 — Slash commands are skills; the parent orchestrates

Decision: a slash command is a skill invoked by name — `/plan` expands
`.zest/skills/plan/SKILL.md` into the turn. Parsing is narrow (leading token
only, `[a-z0-9-_]`, `//` escapes); an unrecognised token is **sent as typed**
rather than rejected. The transcript stores the typed text, not the expansion.

Orchestration stays **flat**: the pinned parent delegates a piece, reads it, then
delegates the next. Nested delegation stays structurally impossible.

Reason: the composer already advertised "/ for commands" and nothing implemented
it. Skills already had discovery, precedence, a name and a description — a second
registry would have been a parallel mechanism for the same idea. Rejecting an
unknown command was considered and dropped: a typo swallowing the message is
worse than the model saying it did not understand.

Flat over nested because worker streams are discarded. A planner dispatching its
own subagents would spend accounts with no transcript row explaining the charge,
which contradicts the ledger's whole reason for existing. Flat gives the same
outcome with every hop visible.

Impact: `commands.rs` + `SkillSet::command`. `send_message` expands after
`begin_turn` (skills live on the session) and stores display text separately.
`DELEGATION_SYSTEM` is appended only when `delegate` is registered — one
`delegate_enabled` binding feeds both the registration and the prompt, with a
test pinning them together, because the failure mode is a prompt describing a
tool the model cannot see.

### 2026-08-02 — Delegation is spend, so it obeys the permission mode

Decision: `delegate` declares `ToolRisk::Exec`. The route resolves in
`prepare()` so the approval card names the provider and model; `run()` re-resolves
and aborts if the answer changed. `Rule` gained optional `effort` and `prompt`,
replacing the hardcoded `"high"` and the single `WORKER_SYSTEM`.

Reason: `delegate` had no `risk()` override, so it defaulted to `Read` and ran
unattended in *every* mode including Manual — a fan-out across three
subscriptions with no card, immediately after delegation was made opt-in
precisely because that spend is a decision. "Allow delegate?" without naming the
account is not an answerable question, which is why the route has to be resolved
before the card rather than after.

Impact: the preview's `path` is `provider/model`, which is also the session-grant
key — so "allow for session" covers that pair rather than all delegation.
Accepted cost: a three-way fan-out is three cards outside Auto and Bypass.
Effort is validated per-model, since the gateway declares efforts per model.

### 2026-08-02 — Delegation is opt-in; the rule list is the kind vocabulary

Decision: `[routing] delegation` defaults to **false**, and false means the
`delegate` tool is not registered — not merely discouraged in the prompt. Two
loaded providers, or even configured rules, are not the switch. Task kinds come
from the rules themselves (`Routing::kinds()`), exposed as an enum in the tool
schema, so the model must pick one the user defined or none. Rules are edited in
**Settings → Routing** and written to `~/.zest/zest.toml`.

Reason: handing a subtask to another provider spends a second subscription, and
that should not switch itself on as a side effect of configuring a second
account — which is exactly what happened the moment Claude was added. Absent
capability beats a flag, same reasoning as the worker registries that
structurally cannot contain `delegate`. Free-form kinds were rejected: a typo
would fall through to the default provider silently, and an open set gives the
Settings UI nothing to render.

Impact: `routing_edit.rs` uses `toml_edit` rather than serde round-tripping,
because `zest.toml` is a commented, hand-edited file and rewriting it through
`Serialize` would delete every comment the first time someone toggled a checkbox.
Rules are written as one inline array so an edit replaces them as a unit.
Validation (`validate_rules`) runs in Rust against live provider catalogues before
the write — an unroutable rule would otherwise surface mid-delegation on a turn
already paid for. Changes apply on the next session; the tool registry is built
once at session start and is not swapped mid-life. The old alpha proof
`runtime_registers_delegate_when_multiple_providers_load` asserted the previous
always-on behaviour and was rewritten to assert the gate.

Still missing: per-rule `effort` (workers are pinned to `high` in
`routing.rs`), and no Gemini/Antigravity provider — the gateway currently serves
only OpenAI and Anthropic models.

### 2026-08-02 — A remembered preference must never be able to strand a provider

Decision: `RuntimeBuilder` distinguishes an explicitly requested model/effort from
a *remembered* one (`with_remembered_options`). Explicit values still error when
they do not fit the provider. Remembered values — and `ZEST_MODEL` / `ZEST_EFFORT`,
which are global and cannot know which provider they land on — are dropped with a
warning surfaced on `RuntimeSession::warnings`. Separately, `migrate_legacy` no
longer reads the user-level `last-model` / `last-effort` / `last-thread-id`
scalars at all; only the project-level copies migrate.

Reason: `~/AppData/Roaming/zest/last-model` held `gpt-5.6-luna` from the
single-provider era. Nothing ever deleted it, and it was consulted as a fallback
for *every* provider in *every* project — so the first Claude session inherited a
Codex model. Selecting Claude then failed validation, and because the only way to
change the sticky model is to start a session, the provider was permanently
unreachable. A convenience preference had become a trap with no exit.

Gating the user-level read on `last-provider` was tried and rejected: that file
and `last-model` are written by different code at different times, so they are not
a matched pair and cannot attribute a model to a provider by inspection. Guessing
wrong strands a provider, so the guess is not worth making.

Impact: two independent layers now prevent the class — the source no longer leaks,
and the consumer no longer treats stale state as fatal. `SessionInfo.warning`
(already present for thread-load problems) carries the explanation so a silently
different model in the picker is never unexplained. The corollary for future work:
anything restored from disk gets the soft landing; anything the user just asked
for does not.

### 2026-08-02 — A provider is offered only when signed in *and* configured

Decision: `ProviderView::selectable` requires both a usable `AuthStatus` and a
config entry. A provider that is signed in but absent from config renders as
"Not configured" with the path to the file to add it to.

Reason: detection and configuration are separate facts and the picker only showed
the first. Claude appeared as green "Signed in", Continue was enabled, and the
turn then failed with "provider `claude` is not configured". The row was pointing
at the Claude login, which was fine; the missing thing was a `[providers.claude]`
entry. `configured` was already on the wire and simply unused.

Impact: adding a provider to `zest.toml` is now a visible prerequisite rather
than a post-click error. Note the two Claude paths are different accounts:
`[providers.claude]` as a gateway spends the CLIProxyAPI Claude OAuth, while
`[providers.anthropic]` with `ANTHROPIC_API_KEY` is direct API billing — they get
separate ids so the ledger keeps them apart.

### 2026-08-02 — Permission modes, and session grants scoped to a target

Decision: five modes — Manual (ask for everything), Accept edits (writes pass,
every command confirmed), Plan (writes and commands **refused**, not queued), Auto
(writes pass plus allowlisted commands; the desktop default), Bypass (nothing
asks). Core's own default is Manual. "Allow for session" grants on
`(tool_name, target)` — the file path, or the exact command string — never on the
tool alone. Changing mode clears all grants.

Reason: one card at a time with only "Allow once" makes a scaffold unusable — the
user clicks Allow a dozen times and stops reading, which is worse than not asking.
Per-tool grants were rejected for the same reason in reverse: you approve one diff
and silently authorise every future write.

Impact: `ApprovalPolicy` in `tools/approval.rs` is consulted *before* the
`Approver`, so there is still one path for "may this run". `bash` no longer
downgrades allowlisted commands to `ToolRisk::Read`; it keeps `Exec` and sets
`PreparedToolCall::auto_eligible`, so Manual really can ask about `cargo check`
while Auto still runs it silently. Plan mode returns `Block` with a reason the
model reads, so it writes a plan instead of stalling on a card. The desktop's
`DESKTOP_DEFAULT_MODE` is Auto and the policy outlives a project switch;
`ApprovalDecision` gained `AllowSession`, widening the desktop approval channel
from `bool` to the full decision.

Two safety properties worth keeping: core defaults to Manual because a bare
`Agent` also defaults to `DenyApprover` and a permissive policy would let writes
through *because* no gate was wired yet; and a poisoned policy lock resolves to
Ask, never Allow.

### 2026-08-02 — Long tool runs collapse in the transcript

Decision: five or more consecutive **finished** tool rows fold into one summary
line ("Ran 2 commands, edited 3 files +30 -12"), expandable. Running and
awaiting-approval rows always break the run and render on their own. A collapsed
run that contains a failure says so on the summary line.

Reason: a scaffold produced thirteen stacked rows and the actual state of the work
was unreadable. Folding is presentation only — the model's view is untouched.

Impact: `lib/toolRuns.ts` (pure, unit-tested) + `ToolRunGroup.tsx`. `edit_file`
diffs already carry the `+`/`-` lines, so counts need no new plumbing; the counter
skips `---`/`+++` headers, which would otherwise add two phantom lines per file.
Files are counted by distinct path, so three edits to one file is one file.

### 2026-08-02 — Providers may be configured per user, not only per project

Decision: `Config::find` looks for `zest.toml` in the project, then
`~/.zest/zest.toml`, then the Anthropic-from-env fallback. Project config
**replaces** user config; the two provider tables are not merged. `load_env()`
does the matching thing for credentials: project `.env` (upward search), then
`~/.zest/.env`, with dotenv's first-wins semantics so a real environment variable
still beats both. Both front-ends call it instead of `dotenvy::dotenv()` directly.

Reason: opening any folder without a `zest.toml` dropped to the env fallback, which
declares only `anthropic`. With `codex` selected in the picker, the folder simply
failed to open — "provider `codex` is configured but could not be loaded", a message
that was also false, since codex was not in that config at all. Which accounts you
are signed into is a property of the machine; it should not change because you
opened a different directory. `~/.zest/` was already the user-global dir
(`skills/`, `known-workspaces.json`, `avatar.jpg`).

Not merged, because a provider table assembled from two files makes "which account
is this turn about to spend" hard to answer, and that question has to stay easy.

Impact: `config::user_config_path()`. `RuntimeBuilder::build` stopped discarding the
registry's `Skipped` reasons — a provider that failed to load now quotes why
(usually the missing env var), and one that was never configured says so and names
where to put a config instead. Those were two different problems sharing one
misleading sentence.

### 2026-08-02 — `bash` ships behind the approval gate, not behind a sandbox

Decision: Add a `bash` tool now. Containment is the existing approval gate plus one narrow
auto-run path: a command runs unattended only if it matches a read-only prefix
(`cargo check|clippy|test`, `cargo fmt --check`, `git status|diff|log|show|branch|rev-parse`,
`npm test|run lint|run ui:build|run ui:test`, version probes) **and** contains no shell
metacharacter at all. Auto-run commands are spawned from an argv vector with no shell in the
process. Everything else renders the exact command line on the approval card and only then
reaches `cmd /C`. `[tools.bash]` in `zest.toml` carries `enabled`, `extra_allowlist`,
`denylist`, `timeout_ms`; the denylist is checked first and always wins. `bash` is registered
on the parent conversation only — never in a delegated worker's registry — and is off for
both `doctor --live` paths.

**Supersedes** the constraint that exec stays disabled until an OS-backed Windows sandbox
exists.

Reason: a harness that cannot run `cargo check` writes code it can never verify, which is the
single largest gap between Zest and the tools it is modelled on. The sandbox bar was not going
to be met soon, and the risk it addresses is already addressed by a gate that exists and works.
Prompting for `cargo check` on every iteration is worse than not prompting: it trains the user
to click Allow without reading.

Impact: the load-bearing check is the metacharacter rule, not the allowlist — `cargo check &&
rm -rf /` starts with an allowlisted token. Ordering (denylist → metacharacters → prefix match)
is a safety property, not a style choice; `tools/bash.rs` tests each escape route explicitly.
Timeouts kill the child rather than only ceasing to wait for it, and `kill_on_drop` covers
cancellation. CLI gained a stdin y/n `Approver` (anything that is not an explicit yes, EOF
included, denies).

### 2026-08-02 — `edit_file` beside `write_file`; reads are sliced and numbered

Decision: Add `edit_file` (exact string replace, unique match unless `replace_all`). It
computes the full new body at prepare time and returns `PreparedKind::WriteFile`, so it reuses
the BLAKE3 pre-image, the bounded diff preview, the approval card, and the atomic replace
unchanged. `read_file` gained `offset`/`limit`, `cat -n` line prefixes, and a 256 KiB cap
(from 64 KiB); `grep`'s per-file cap moved with it.

Reason: `write_file` charged the whole file in *output* tokens to change three lines, and
output is the serialized, expensive dimension of a turn. Separately, the old 64 KiB cap meant
Zest could not read `crates/desktop/src/lib.rs` — 67,806 bytes — at all.

Impact: the read/edit pair has one known failure mode, the model quoting a line-number prefix
back into `old_string`. Both tool descriptions say not to, and the not-found error names it
specifically. Registration order is `write_file` then `edit_file` so the cached prompt prefix
shifts once.

### 2026-08-02 — Ungated tool calls run concurrently; gated ones never do

Decision: `Agent` prepares every tool call in a batch up front, runs the ungated ones through
`join_all`, then runs approval-gated ones strictly sequentially afterwards. Results and
`ToolCallResult` events are emitted in **call order**, never completion order.

Reason: the model issues parallel `tool_use` blocks having seen none of their results, so they
are independent by construction; serializing them only cost wall-clock. Gated calls stay
sequential because the user must see one card at a time and two writes to one path must not
race. Running them after the concurrent batch also makes a same-batch read observe the
pre-write file deterministically.

Impact: `on_event` is `&mut dyn FnMut` and cannot cross into concurrent futures, so events are
emitted after the join. `execute_tool_call`'s four-tuple became `ToolCallOutcome`.

### 2026-08-02 — Retry only what produced no output

Decision: `AnthropicClient` retries up to 3 attempts on connect/timeout errors and on 408, 429,
500, 502, 503, 529 — but **only** before any byte of the response body has streamed. Honours
`retry-after` up to 60s, else ~1/2/4s with clock-derived jitter. The backoff sleep races the
cancel token. An exhausted retry says "failed after N attempts" in the error the user reads.

Reason: a single 529 previously discarded a whole staged turn, because history is transactional.
Retrying mid-stream would replay text the caller already saw, so that case is deliberately
excluded rather than made clever.

Impact: `HarnessError::is_transient()`. `Cancelled` is passed through un-annotated — flattening
it would report a deliberate Stop as a failure. Successful retries are not yet surfaced in the
UI; only exhausted ones are.

### 2026-08-02 — Desktop projects sidebar + shared DEFAULT_SYSTEM

Decision: Desktop sidebar groups chats **by project** (known workspace roots). Opening a folder
appends to `~/.zest/known-workspaces.json`. Switching project keeps the same provider and reloads
sticky/new thread via `open_project_chat`. Front-ends (desktop + CLI) use `zest_core::DEFAULT_SYSTEM`
instead of duplicated `SYSTEM` string constants. `web_search` ships DuckDuckGo-only (no Brave
env branch). Profile photos are optimized to ~128px JPEG on disk (`~/.zest/avatar.jpg`), not stored
as multi-MB data-URLs in JSON. Context meter shows used/window only — no fake “to compact” product
surface until compaction exists.

Reason: Multi-repo day-to-day use needs project grouping; tool-list prompt drift across three
constants was already biting; dual search providers and fat avatars were overengineering for alpha.

Impact: Sidebar APIs `list_chat_projects` / `open_project_chat`; path compares for delete must use
display-normalized roots on Windows. Prompt/tool-list edits land in `prompt.rs` only.

### 2026-08-02 — Minimal tool approval gate before writes

Decision: Tools declare `risk: read | write | exec`. Read tools auto-run; write/exec pause on an
`Approver` hook. Desktop shows an in-chat Allow once / Deny card with a short diff for
`write_file`. Decisions are session-scoped only (no forever-trust). CLI registers `write_file` but
auto-denies until it has a prompt. No bash/exec yet.

Reason: PROJECT_CONTEXT requires a permission layer before irreversible tools. A per-call desktop
gate unblocks the first write tool without building a trust store.

Impact: Agent loop owns the gate; front-ends supply Approver + UI. Exec remains blocked until a
sandbox story exists.

### 2026-08-01 — Build a harness, not a desktop shell over LimeBot

Decision: Own the agent loop, model client, tool layer, and permission model in Rust. LimeBot-OS
stays the Python reference implementation and is not a dependency.

Reason: The alternative considered was a Tauri window whose webview pointed at LimeBot's existing
FastAPI backend, with Rust supervising the Python process. That is days of work and reuses the
whole React UI — but the ceiling is Python's, and none of the harness is actually yours. The
explicit goal was to own it.

Impact: Weeks-to-months of work instead of days. Nothing is reusable from LimeBot's runtime. The
`skills/` layout, prompt design, and config shape are worth porting as concepts.

### 2026-08-01 — Zest is a coding agent, not an assistant

Decision: Zest targets the Claude Code / Codex shape — filesystem, tools, approvals, diffs, invoked
inside a project. It is not multi-channel, not persona-driven, not long-running.

Reason: LimeBot already occupies the personal-assistant space and does it well. Building a second
assistant would duplicate effort; building a coding harness covers the gap.

Impact: Rules out channels (Discord/Telegram/WhatsApp), persona bootstrapping, and conversational
memory as features. Rules **in** the permission model, diffing, and session/context management as
the hard problems worth solving.

### 2026-08-01 — Named Zest, separate from LimeBot

Decision: Repo `zest`, crates `zest-core` and `zest`, binary `zest`. Renamed from the initial
`limebot-harness` scaffold.

Reason: Keeping the LimeBot name on a differently-shaped project guarantees confusion in docs,
config keys, and conversation. `zest` is four characters for something typed constantly, stays in
the lemon family alongside Agentic Lemon, and collides with nothing in the AI/agent space.

Impact: The crates.io name `zest` is held by an abandoned zip library (v0.0.2). Irrelevant unless
Zest is ever published there; if it is, `zester` was free as of this date. Rejected `rig` and
`pith` for semantic collisions — `rig` is an LLM-app framework, `pith` is an LLM-context tool.

### 2026-08-01 — Headless core first, terminal front-end, GUI later

Decision: `zest-core` carries no UI or terminal assumptions. The CLI is one consumer.

Reason: Debugging an agent loop through a webview is miserable. Building the loop behind a plain
binary means iterating at `cargo run` speed and testing it properly.

Impact: A desktop front-end later is a thin view over a working core rather than a rewrite. Costs
a small amount of API design discipline now.

### 2026-08-01 — Hand-written Messages API client, no SDK

Decision: Talk to the API directly with `reqwest` + `serde_json` and a hand-rolled SSE reader.

Reason: There is no official Anthropic Rust SDK. Community crates (`async-anthropic` ~45k
downloads, `anthropic-sdk` ~76k) are thin wrappers with low adoption. The client is a few hundred
lines and full control over streaming and tool blocks is wanted anyway.

Impact: Wire-format changes have to be tracked manually. Offset by owning exactly the behavior
needed. The official `rmcp` crate is the counterexample — when MCP is added, use it rather than
hand-rolling.

### 2026-08-01 — Assistant content blocks stay as `serde_json::Value`

Decision: Do not model assistant content blocks as a typed Rust enum. Store the raw JSON and
extract typed views (`tool_uses()`) where needed.

Reason: Two forces. Thinking blocks carry a `signature` that must be echoed back byte-for-byte or
the next request is rejected. And the API adds block types over time (`server_tool_use`,
`fallback`, …). Raw JSON round-trips losslessly by construction; a typed enum silently drops
anything it does not know about, which is the worst possible failure mode — it looks fine and
corrupts history.

Impact: The tool layer is slightly stringly-typed at the boundary. Accepted deliberately.

### 2026-08-01 — One wire protocol; other backends via gateway *(superseded same day — see "Multi-provider orchestration is the product")*

Decision: Zest speaks the Messages API only. Other providers are reached through a gateway that
translates (e.g. CLIProxyAPI with a Codex login), configured with `ZEST_BASE_URL`. When the host
is not Anthropic's, `thinking` and `output_config.effort` are dropped.

Reason: Adding a second client (OpenAI Responses) means a second wire protocol to maintain
forever. Letting a proxy translate keeps the harness at one.

Impact: Gateways are a **development** convenience and must never become a shipped dependency —
bundling one reintroduces the process-supervision problem a single binary exists to avoid. Also
note that routing Codex subscription credentials through a third-party proxy is against OpenAI's
terms; the realistic downside is account loss.

### 2026-08-01 — Multi-provider orchestration is the product

Decision: Zest's defining capability is routing tasks across separately-authenticated providers —
Gemini via Antigravity, Claude via a Claude login, GPT via Codex — and accounting for what each has
left. A `Provider` trait is the central abstraction; the router and the usage ledger are the parts
worth getting right.

Reason: Stated directly as the main functionality. It is also the gap no single-vendor tool fills:
Claude Code speaks Anthropic, Codex speaks OpenAI. Neither can send a mechanical task to a cheap
fast model and the hard reasoning to an expensive one, and neither can tell you which subscription
you are about to exhaust.

Impact: **Supersedes "One wire protocol; other backends via gateway."** That decision assumed
non-Anthropic providers were an occasional convenience; they are now the point, so provider access
cannot be an env var. Two consequences:

1. `AnthropicClient` becomes one implementation of `Provider`, not *the* client. `Agent` must hold
   a `Provider`, not a concrete type.
2. Reaching a provider natively vs. through CLIProxyAPI becomes a per-provider implementation
   detail behind the trait. Bootstrap through the proxy, replace with native clients where it
   earns it, without the router noticing.

It also softens "no shipped runtime dependencies" from a rule to a target: if native OAuth for
three providers proves impractical, a bundled proxy may be the honest answer. Revisit deliberately
rather than by drift.

Resolved for v1 (see 2026-08-02 Stable Windows Alpha): the parent conversation stays pinned to the
selected provider; multi-provider routing is performed only by **delegated workers**
(`delegate` tool), never automatically per user turn.

### 2026-08-01 — Usage accounting is metered locally, not queried

Decision: Track consumption per provider in a local ledger. Read provider-reported headroom
(e.g. Anthropic's `anthropic-ratelimit-*` response headers) where it exists, and label everything
else as a local estimate against a configured budget.

Reason: Subscription-backed CLI logins — Claude, Codex, Antigravity — largely do not expose a
documented "remaining quota" endpoint. Their own clients show session windows and weekly caps from
internal state, not a public API. Building the ledger on a promise of queryable remaining usage
would build it on something that does not exist for most of the fleet.

Impact: The ledger must record what Zest itself spent (tokens and requests per provider per
window), which is exact for Zest's own traffic and blind to usage from other clients on the same
account. That limitation has to surface in the UI — a number labelled "remaining" that silently
excludes what Claude Code spent an hour ago is worse than no number. Where response headers give
real limits, prefer them and mark them as authoritative.

### 2026-08-01 — Detect vendor sign-ins, never implement OAuth

Decision: Zest reads whether a vendor CLI has already signed in (`~/.codex/auth.json` and friends)
rather than performing any OAuth flow itself.

Reason: Three vendor OAuth flows would be the most fragile code in the project and would break
upstream without notice — it is the reason CLIProxyAPI exists. LimeBot already proved the read-only
pattern in `scripts/codex-oauth.mjs`.

Impact: `AuthStatus` needs a fourth case. Codex is detectable on Windows; Claude and Antigravity
keep credentials somewhere unreadable, so they report `Unknown` — **selectable**, because claiming
"not logged in" when the truth is "can't tell" would push the user to re-authenticate for nothing.
Detection never reads a secret: existence and JSON-parseability only.

### 2026-08-01 — Delegation loops are prevented structurally, not by a counter

Decision: A delegated worker's tool registry cannot contain the `delegate` tool. Asserted at
construction.

Reason: A depth limit is a setting someone can get wrong. An absent capability cannot be. The
failure it prevents — unbounded recursive delegation across paid providers — is expensive in a way
that argues for the stronger guarantee.

Impact: `ToolRegistry` became `Clone` so a worker gets its own. Wiring a delegate into a worker's
toolset panics at startup rather than mid-conversation.

### 2026-08-01 — A provider is only "exhausted" on evidence

Decision: Routing skips a provider only when it *reported* no headroom (`requests_remaining == 0`,
or a `retry-after` that has not yet elapsed). A provider that reports nothing is treated as
available.

Reason: Gateways report no rate-limit headers at all. Treating silence as exhaustion would route
every task away from every gateway-backed provider — exactly the providers this harness exists to
use.

Impact: `Resolution` carries the list of providers passed over and why, so a fallback that spends a
different account than expected is visible rather than silent.

### 2026-08-01 — If a GUI is built, Tauri over GPUI

Decision: A future desktop front-end uses Tauri, not GPUI / gpui-component.

Reason: GPUI is consumed as a git dependency tracked against Zed's tree, with breaking changes
outside semver, thin docs, a small ecosystem, and its least-mature backend on Windows — which is
the development platform here. Tauri's real cost is WebKitGTK inconsistency on Linux, which
matters less.

Impact: Revisit only if Zest becomes latency-critical and text-dense enough that a webview is the
bottleneck. It is not today.

### 2026-08-01 — First live turns go through Codex via CLIProxyAPI

Decision: Accept the subscription-through-proxy tradeoff for local development. Default
`zest.toml` routes to `[providers.codex]` (CLIProxyAPI on `127.0.0.1:8317`, model
`gpt-5.6-sol`) authenticated with `ZEST_GATEWAY_KEY`. A paid `OPENAI_API_KEY` remains a valid
BYOK alternative; it is not required for this path.

Reason: No provider had yet served a live turn. Codex OAuth is already detectable on this
machine's platform, and Tibo (@thsottiaux, Codex) publicly recommended the same bootstrap —
CLIProxyAPI + Messages-API client pointed at GPT-5.6 Sol
(https://x.com/thsottiaux/status/2076119366647894371). That is exactly Zest's transitional
gateway shape. The ToS risk (account loss) is accepted by the author for personal local use;
nothing ships that embeds or depends on the proxy.

Impact: First end-to-end verification spends the Codex subscription through a local gateway.
Native Codex client remains the long-term swap behind the `Provider` trait. Do not distribute a
setup that requires third-party proxying of subscription credentials.

### 2026-08-01 — Desktop starts as a Tauri provider picker; Connect spawns vendor CLI

Decision: Ship `crates/desktop` (`zest-desktop`) as a Tauri 2 app whose first screen is the
provider picker. **Connect** calls `start_login` in `zest-core`, which spawns vendor/gateway
login silently and re-detects — it does not embed OAuth. **Continue** persists the chosen
provider id under the user config dir; agent session UI followed in the same Tauri shell.

Reason: The project already chose Tauri over GPUI and “detect, don’t implement OAuth.” The
screenshot-shaped picker is the smallest useful desktop surface: it answers “what can I spend?”
and “how do I sign in?” before the chat surface.

Impact: Three workspace members (`core`, `cli`, `desktop`). CLIProxyAPI remains a separate
local process.

### 2026-08-02 — Connect is a native shell over vendor OAuth, not OAuth inside Zest

Decision: Do not implement OAuth (client IDs, redirects, token exchange) in Zest. Make Connect
feel native instead: `CREATE_NO_WINDOW` spawn, in-app waiting/success UI, system browser for
ChatGPT/Claude. When `tools/CLIProxyAPI` (or `ZEST_CLIPROXY_PATH`) is installed, Codex Connect
prefers `cli-proxy-api -codex-login`, and Codex `AuthStatus::Ready` means a well-formed JSON
file exists under `~/.cli-proxy-api` (presence only).

Reason: The OpenAI success page saying “return to your terminal” is their page; replacing it
with a real in-process OAuth client would own a fragile vendor flow. Hiding the console and
owning the waiting chrome gives the product feel without that cost. Gateway credentials are
what live turns actually spend.

Impact: `resolve_login` / `LoginSpawn` in `auth.rs`; desktop waiting and success screens; README
Connect table updated. In-webview ChatGPT login remains out of scope.

### 2026-08-02 — Desktop chat UI is React + shadcn; agent stays in Rust

Decision: Keep the agent loop, providers, and tools in `zest-core`. Skin the Tauri webview with
Vite + React + shadcn chat primitives (`MessageScroller`, `Message`, `Bubble`, `Marker`,
`Attachment`) themed from `DESIGN.md` (Linear). Stream turns over existing Tauri `chat-event`s;
extend those with `tool_call_start` / `tool_call_result`. Do not adopt TanStack AI as the live
transport.

Reason: Rust owns the harness; the webview only needs a modern chat surface. shadcn’s chat
components cover scrolling/streaming chrome without rewriting the agent path.

Impact: `crates/desktop/ui/` is the Vite app; `ui-legacy/` holds the previous static HTML.
`npm install` is required for desktop builds. Offline UI smoke via `?fixture=1`.

### 2026-08-02 — Stable Windows Alpha: reliability before more tools

Decision: Prioritize a personal, local Windows alpha gate over new features. Sequence: toolchain
+ CI guardrails → tool/approval integrity (`PreparedToolCall`, real diffs, BLAKE3-bound
approvals, atomic writes, ignore-aware walk, secrets) → transactional turns + session controller
+ coalescing persistence → desktop contract (reducer, ts-rs DTOs, CSP) → `RuntimeBuilder`,
provider-owned `ModelSpec`, desktop `delegate`, fake-provider tests, and opt-in
`zest doctor --live`.

Routing (resolved v1): the main conversation remains pinned to the selected provider.
Multi-provider routing is performed only by **delegated workers** (`RuntimeBuilder` registers
`delegate` when `registry.len() > 1`), never automatically per user turn.

Alpha acceptance: automated checks (including deterministic fake-provider proofs for route
selection, selected model, tool round-trip, ledger attribution, fallback reasons, and thread
restoration) plus one manual `zest doctor --live` read-only README turn (streaming, tool
completion, usage delta, persistence). Doctor spends quota and must not run in CI.

Deferred: bash/exec (needs OS sandbox), native Codex transport, compaction, public signing,
automatic per-turn routing, accounts/cloud/telemetry.

Reason: Trustworthy writes and deterministic session state are the alpha bar; more tools without
those foundations widen the blast radius.

Impact: Pin Rust 1.97.1 / Node 24.16.0 / npm 11.13.0; root npm workspaces; `scripts/verify.ps1`
and Windows CI. Visual chat styling stays unless error/loading fixes require it. Gateway
`models` / `efforts` allow-lists are optional; when `models` is omitted, generic gateways
accept only the configured default. Provider `codex` uses the built-in Sol/Terra/Luna
(+ 5.5/5.4) catalogue so the desktop picker and sticky last-model validate without a
manual allow-list.

### 2026-08-02 — Custom system prompt + Cursor-style skills

Decision: Project custom instructions live in `.zest/system.md` (Settings editor). When
present they are **authoritative** (composed first; hardcoded “You are Zest…” is softened)
so persona overrides work. Skills use Cursor-compatible `SKILL.md` under `.zest/skills/*/`
and `~/.zest/skills/*/`; catalogue (+ small bodies) enter the system prompt; larger skills
load via `read_skill`. Threads/`system.md` stay gitignored; `.zest/skills/` may be committed.

Reason: Authors need project tone/rules without forking the harness prompt, and reusable
skill packs without inventing a new format.

Impact: `RuntimeBuilder` always composes custom → base capabilities → skills; desktop
hot-reloads on Save; Settings uses shadcn Collapsible sections; chat emits `assistant_start`
so Thinking… appears before the first token.

### 2026-08-02 — Antigravity 429 may be prompt fingerprint, not quota

Decision: Treat Antigravity/Gemini `429 RESOURCE_EXHAUSTED` as **ambiguous** until proven
otherwise. Do not automatically mark the provider exhausted or rewrite the ledger as “quota
spent” solely from that status when the same credential still serves neutral prompts.
When debugging Gemini failures through CLIProxyAPI (or later native Antigravity), A/B the
system identity sentence first — upstream has been observed filtering specific product
identity phrases while reporting quota exhaustion
([CLIProxyAPI#4696](https://github.com/router-for-me/CLIProxyAPI/issues/4696); also
reproduced on the native Antigravity endpoint without CPA).

Reason: Future-proofing. Mislabeling fingerprint rejects as quota would burn routing
fallback incorrectly and send operators on a wild goose chase.

Impact: Documented in `memory/recurring-corrections.md`. Prefer neutral coding-agent
system framing for Antigravity-backed models; avoid copying third-party agent identity
blocks into `.zest/system.md` without checking.
