# Changelog

Track notable changes here.

## 2026-08-02

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
