# Decisions

Track important decisions here.

## Format

### YYYY-MM-DD — Decision Title

Decision:

Reason:

Impact:

---

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
