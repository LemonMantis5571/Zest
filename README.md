# Zest

**Multi-provider coding harness in Rust** — read and edit a project, stream tool calls, approve writes, stay local.

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)

Zest reads and edits your project with tools, streams replies, and asks before writing files — a local coding agent you run in a repo.

| | |
|---|---|
| **Status** | Stable Windows **alpha** |
| **Live path** | Codex via [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) (local gateway) |
| **UI** | Desktop app (Tauri) |
| **CLI** | `zest` — same agent in the terminal |

---

## Requirements

| Tool | Version | Notes |
|------|---------|--------|
| Windows | 10/11 | Primary target |
| [Rust](https://rustup.rs/) | **1.97.1** | Pinned in `rust-toolchain.toml` |
| [Node.js](https://nodejs.org/) | **24.16.0** | `.nvmrc` / `package.json` engines |
| npm | **11.13.0** | Comes with Node |
| CLIProxyAPI | Bundled pinned release | Local Codex/Claude OAuth gateway |

Optional: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) if you later enable a native Anthropic provider.

---

## Fresh install (Windows)

Copy-paste path from an empty machine to a working desktop chat.

### 1. Clone

```powershell
git clone https://github.com/LemonMantis5571/Zest.git
cd Zest
```

### 2. Toolchain

```powershell
rustup show          # should resolve to 1.97.1 via rust-toolchain.toml
node -v              # v24.16.0 (nvm use / nvs use if needed)
npm -v               # 11.13.0
```

### 3. Gateway (Codex, Claude)

Zest bundles a pinned [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)
release (MIT) as a Tauri sidecar and starts it on demand, so a downloaded Zest
needs no separate install. Fetch the binary once before building:

```powershell
.\scripts\fetch-gateway.ps1
```

That reads `crates/desktop/gateway-release.json`, downloads exactly that release
for your target, verifies it against the SHA256 committed in the repository, and
drops it in `crates/desktop/binaries/` (gitignored — ~64MB). It never resolves
`latest`. Add `-Target <triple>` for another platform; use `-Check` to verify the
local binary and provenance without downloading, or `-CheckPin` to validate only
the committed release metadata and MIT notice.

On first run Zest writes a loopback-only `config.yaml` with a freshly generated
key into `%APPDATA%\zest\gateway\`, pointed at the same `~/.cli-proxy-api`
credential store the vendor CLIs use — so sign-ins that already happened still
work. Nothing to configure and no `.env` needed.

**Already have a hand-installed gateway?** It keeps winning. A
`tools/CLIProxyAPI/` checkout with its own `config.yaml` is used as-is, and an
existing `ZEST_GATEWAY_KEY` is never overwritten.

5. Start the gateway and complete Codex OAuth **through the proxy** (not only `codex login`):

```powershell
.\scripts\start-gateway.ps1
.\scripts\codex-login-gateway.ps1
```

Desktop **Connect** can also spawn this login when the gateway tree is present.

If Claude through the gateway keeps returning `auth_unavailable` after a rebuild,
that is the proxy cooling or dropping the session — not Zest wiping credentials.
Desktop now probes before opening a gateway chat. In `tools/CLIProxyAPI/config.yaml`
you can also set `disable-cooling: true` (and optionally
`transient-error-cooldown-seconds: -1`) so a transient 503 does not black-hole the
account until you Connect again.

### 4. Install JS deps and build the webview

Desktop loads `crates/desktop/ui/dist` — you must build it once after clone:

```powershell
npm install
npm run ui:build
```

### 5. Run

```powershell
# Desktop (picker → Connect → chat)
cargo run -p zest-desktop

# Or CLI harness from a project directory
cargo run -p zest
```

Default config (`zest.toml`): provider `codex`, model `gpt-5.6-sol`. Tools are scoped to the directory you launch from.

---

## First chat checklist

1. `.\scripts\fetch-gateway.ps1` has run (Zest starts the gateway itself)
2. Codex signed in via gateway (`~/.cli-proxy-api` session present)
3. `npm run ui:build` already succeeded
5. Open desktop → provider **Codex** shows Signed in → **Continue**
6. Ask something that needs a file (`What's in README.md?`) — you should see tools stream

Offline UI smoke (no gateway): open the webview with `?fixture=1` during UI-only work.

---

## Daily commands

```powershell
cargo run -p zest-desktop          # desktop app
cargo run -p zest                  # CLI chat
cargo run -p zest -- auth          # provider detection
cargo run -p zest -- usage         # local usage ledger
cargo run -p zest -- doctor --live # opt-in live acceptance (spends quota)

npm run ui:build                   # rebuild webview after UI changes
npm run desktop:dev                # Tauri + Vite HMR (dev)
.\scripts\verify.ps1               # fmt, clippy -D warnings, tests, UI lint/build
```

Set `CARGO_TARGET_DIR` if you want a fixed target dir (CI/scripts use `D:\…\target` or repo `target/`).

---

## Features you’ll touch

### Model & effort

Composer picker for Codex **Sol / Terra / Luna** (and older GPT-5.x ids). Effort: low → max. Sticky last choice is restored on launch.

### System prompt

**Settings → System prompt** → saves `.zest/system.md`.

When set, it is **authoritative** (persona wins over “You are Zest…”). Takes effect on the next message after **Save**. Personal; gitignored.

### Skills (Cursor-style)

```
.zest/skills/<name>/SKILL.md      # project (shareable)
~/.zest/skills/<name>/SKILL.md    # user-global
```

YAML frontmatter requires `name` and `description`. Catalogue is injected into the system prompt; bodies ≤4 KiB are inlined; larger skills load via the `read_skill` tool.

### Approvals & modes

The composer footer has a mode chip (keys **1-5** while open):

| Mode | Writes | Commands |
|------|--------|----------|
| Manual | ask | ask — including `cargo check` |
| Accept edits | apply | ask for every command |
| Plan | **refused** | **refused** — read-only research |
| Auto *(default)* | apply | allowlisted run; ask for the rest |
| Bypass permissions | apply | run |

Cards offer **Allow once / Allow for session / Deny**. A session grant covers that
tool and *that target only* — this file, or this exact command — because that is
what you were shown. Changing mode clears every grant, and nothing is persisted:
restarting is always a clean slate.

Plan mode refuses rather than queueing a card, so the agent writes up what it
would do instead of stalling.

CLI prompts y/N at the terminal.

### Long tool runs

Five or more consecutive finished tool rows collapse into one line — *Ran 2
commands, edited 3 files +30 -12* — which expands on click. Anything still running
or waiting on you stays on its own row, and a failure inside a collapsed run is
called out on the summary.

### Tools

Project-scoped: `list_dir`, `glob`, `grep`, `read_file`, `write_file`, `edit_file`.
Commands: `bash`. Network: `web_search` (DuckDuckGo HTML; no API key). Skills:
`read_skill`. Multi-provider: `delegate` when more than one provider is configured.

`read_file` numbers its output and takes `offset` / `limit` (2000 lines at a time,
256 KiB cap). `edit_file` replaces an exact string and is what the agent should
reach for on an existing file — `write_file` is for creating one.

### Running commands

`bash` runs in the project root. Read-only commands run unattended:

```
cargo check | clippy | test | fmt --check | tree | metadata
git status | diff | log | show | branch | rev-parse
npm test | run lint | run ui:build | run ui:test
```

A command qualifies only if it contains **no shell metacharacters** — `cargo check
&& rm -rf /` is not an allowlisted command, it is an approval prompt. Everything
else shows the exact command line and waits for Allow. Configure under `[tools.bash]`:

```toml
[tools.bash]
enabled = true
extra_allowlist = [["just", "lint"]]   # token lists; still no metacharacters
denylist = ["cargo publish"]           # checked first, always wins
timeout_ms = 120000                    # capped at 600000
```

Not enabled for delegated workers or for `doctor --live`.

### Slash commands

A command **is** a skill. Type `/` in the composer to see what is available;
`↑↓` to move, `Tab`/`Enter` to pick, `Esc` to dismiss. Anything after the
command is appended to the skill body as your instruction:

```
/plan add a health endpoint
```

Adding a command means adding `.zest/skills/<name>/SKILL.md` — no code, no
rebuild. `zest` ships with `/plan` (research, propose, change nothing).

The transcript keeps what you typed, not the expansion, so the sidebar stays
readable. An unknown `/token` is sent as-is rather than rejected — a typo should
not eat your message. Start a message with `//` for a literal slash.

### Routing (delegation)

**Off by default.** Turn it on in **Settings → Routing**, where you also map task
kinds to providers:

| Kind | Provider | Model | Effort |
|---|---|---|---|
| `planning` | codex | gpt-5.6-luna | high |
| `implementation` | claude | claude-opus-5 | high |
| `mechanical` | codex | gpt-5.4-mini | low |

**The parent chat does the orchestrating.** It delegates a piece, reads the
answer, then delegates the next — so every hop is a row in your transcript with
its provider, model and token delta. A *worker* can never delegate again: its
registry structurally cannot contain the tool, which is what bounds the fan-out
without a depth counter.

Delegation is gated like anything else that spends money. Outside Auto and
Bypass you get a card naming the exact provider and model before the call goes
out, and the route is re-checked at dispatch — if a rate limit changed the
answer between the card and the call, it aborts rather than quietly spending a
different account.

Off means the `delegate` tool is **not registered at all** — the model cannot see
it and cannot spend a second subscription. Configuring rules is not enough on its
own; the switch is separate.

Rules need two or more configured providers. Model dropdowns are fed by each
provider's real catalogue, and a rule is validated against it before saving.
Rules are consulted in order, first match wins, so listing two providers for one
kind gives you a fallback chain. A kind with no matching rule goes to
`[routing].default`. Saved to `~/.zest/zest.toml`.

After saving, hit **Apply now** — the tool registry is built once per session, so
New chat (which only swaps the thread) keeps the old routing. Applying rebuilds
the session and reloads the open chat from disk; nothing is lost.

**Never configured routing before?** With two providers loaded, **Suggest rules**
fills in a working starting point derived from what you actually have. It only
suggests rules that reach a *different* provider — see below for why that matters.

Each rule shows what it truly resolves to (`claude · claude-opus-5 · high`).
A rule pointing at the provider the open chat already uses is noted, not
flagged: that is still a real delegation — the worker starts with **no
conversation history** — it just isolates context rather than changing model.
Point the kind elsewhere if you wanted a different model.

Two things sound like "the default" and are not the same:

| | |
|---|---|
| **This chat runs on** | whichever provider you picked in the launcher |
| **`[routing].default`** | where a *delegated* task goes when no rule matches — and the chat provider for the CLI, which has no picker |

The Routing panel states both, because a note about one reads as a claim about
the other otherwise.

Equivalent TOML:

```toml
[routing]
delegation = true
default = { provider = "codex", model = "gpt-5.6-sol" }
rules = [
  { kind = "planning", provider = "codex", model = "gpt-5.6-luna" },
  { kind = "implementation", provider = "claude", model = "claude-opus-5" },
  # `effort` and `prompt` are optional. Worth setting both: routing a
  # mechanical task to a cheap model and then running it at max effort spends
  # most of what the routing saved.
  { kind = "mechanical", provider = "codex", model = "gpt-5.4-mini", effort = "low",
    prompt = "Make the smallest change that works. Do not refactor." },
]
```

Delegated workers get the read and write tools but never `bash`, and never
`delegate` itself — recursion is prevented by the capability being absent rather
than by a depth counter.

### Chat history (by project)

The left sidebar lists **Projects** (folders you have opened). Each project expands
to its chats. Threads are stored per project under `<project>/.zest/threads/`
(gitignored). Known project roots are remembered in `~/.zest/known-workspaces.json`.

**A chat belongs to one provider for its whole life.** Its stored history is raw
wire format — thinking-block signatures that must echo back byte for byte, tool
calls in that provider's shape — so it cannot be replayed to a different one.
Switching provider therefore shows that provider's chats; switching back brings
the others straight back. Nothing is deleted. When a project has chats under more
than one provider, each row is tagged so this is visible rather than surprising.

- Folder picker (sidebar or composer) adds/switches the active project
- **+** on a project → new chat there
- Trash → confirm, then delete (the open chat becomes an unsaved draft until the first message)
- Composer footer shows folder, git branch, and context usage estimate

### Attachments & images

Composer **+** → Upload files / Open folder. Paste images into the composer.
PDFs are extracted to text (no OCR). Images go to the model as Messages API image
blocks.

### User profile

Header avatar opens **Settings → User**. Display name + optional photo (resized to
a small JPEG under `~/.zest/avatar.jpg`).

### Context usage

Footer ring shows how full the context window looks (last API turn’s
`input_tokens` when available, otherwise a rough estimate). Soft “compact”
threshold is **not** shipped yet — the meter is informational only.

---

## Project layout

```
crates/core/       zest-core — agent loop, providers, tools, skills, threads
crates/cli/        zest — terminal front-end
crates/desktop/    zest-desktop — Tauri + React (ui/)
scripts/           gateway helpers + verify.ps1
zest.toml          providers + routing (safe to commit)
.env.example       template for ZEST_GATEWAY_KEY
```

Agent-facing docs for contributors: `AGENTS.md`, `PROJECT_CONTEXT.md`, `context/`, `memory/`.

---

## Configuration

`zest.toml` declares providers and routing. API keys are never stored there: API-key providers use
the operating system credential manager, with optional environment-variable fallbacks for CI.

Looked up in this order:

1. `zest.toml` in the folder you opened
2. `~/.zest/zest.toml` — **user-global**, applies to every project
3. Anthropic-from-environment fallback

Environment variables load the same way: project `.env` (searching upward), then
`~/.zest/.env`. First one wins, and a real environment variable beats both.

On first launch, Zest creates `~/.zest/zest.toml` from the safe starter config
embedded in the app. It never overwrites an existing user file. The optional
credential file can still be shared across projects:

```powershell
copy .env      $HOME\.zest\.env
```

Which accounts you are signed into, and the keys that reach them, are properties
of your machine rather than of one repository. This bootstrap means opening a
folder outside the Zest checkout still sees the bundled Codex provider.
A project `zest.toml` **replaces** the user one rather than merging — a merged
provider table makes "which account is this about to spend" ambiguous.

The desktop also carries the active provider configuration across a folder
switch when the destination has neither project nor user-global config. This
keeps an open Codex session usable in a new codebase without weakening the
explicit project-config boundary; create `~/.zest/zest.toml` to make the setup
persist across restarts as well.

| Variable | Purpose |
|----------|---------|
| `ZEST_GATEWAY_KEY` | Must match CLIProxyAPI `api-keys` |
| `ZEST_MODEL` | Optional model override |
| `ZEST_EFFORT` | Optional effort override |
| `ZEST_BASE_URL` | One-off gateway origin override (no `/v1/messages`) |
| `ANTHROPIC_API_KEY` | Native Anthropic when you uncomment that provider |

### OpenAI-compatible providers

Add a provider definition, then open Zest Settings and paste the key. The key is written only to
the OS credential manager and is never returned to the UI or written to the config file.

```toml
[providers.deepseek]
kind = "openai_compatible"
base_url = "https://api.deepseek.com"
model = "deepseek-chat"
models = ["deepseek-chat", "deepseek-reasoner"]
credential = "deepseek"
```

OpenAI and local OpenAI-compatible servers use the same shape. `base_url` is the API root; Zest
appends `/chat/completions`:

```toml
[providers.openai]
kind = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-5"
credential = "openai"

[providers.local]
kind = "openai_compatible"
base_url = "http://localhost:11434/v1"
model = "qwen2.5-coder"
```

---

## Verify & live doctor

```powershell
.\scripts\verify.ps1
```

ASCII-safe PowerShell 5.1 gate: gateway release pin → `npm ci` → UI test/lint/build → `cargo fmt` /
clippy `-D warnings` / tests → ts-rs binding drift → `npm audit` → RustSec (`cargo audit`) → `git diff --check`.
CI runs the same script on `windows-latest`. Root convenience: `npm run ui:test`.

```powershell
cargo run -p zest -- doctor --live
```

Opt-in. Requires a working gateway/login and spends real quota. Reloads the usage ledger from disk
before asserting success (no RAM-only fake pass). Checks streamed text, `read_file` on `README.md`,
ledger delta, thread reload. Writes/`delegate` disabled for this command. If creds are missing,
skip live doctor — do not invent a green result.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Desktop blank / old UI | `npm install && npm run ui:build`, then rebuild desktop |
| `ZEST_GATEWAY_KEY is not set` | Only for a hand-installed gateway: match `.env` to its `api-keys`. A bundled gateway generates its own. |
| Codex not Signed in | Run `.\scripts\codex-login-gateway.ps1` or Connect in the picker |
| Connection refused `:8317` | Zest starts the gateway itself; if it persists, check the binary exists (`.\scripts\fetch-gateway.ps1`) |
| Model rejected / Luna missing | Rebuild with current core (Codex has a built-in Sol/Terra/Luna catalogue) |
| Custom persona ignored | **Save** in Settings, then send a **new** message (or New chat) |
| Can't reach gateway / connect errors | Start CLIProxyAPI (`.\scripts\start-gateway.ps1`); empty system prompt is fine |
| Delete chat looks like a no-op | Deleting the open chat creates a fresh Untitled chat — look for the toast |
| Project missing from sidebar | Open that folder once (picker); it is then stored under known workspaces |
| Provider shows **Not configured** despite being signed in | Being signed in is only half of it — add a `[providers.<id>]` entry to the project or user config |
| `auth_unavailable` / 503 from the gateway mid-chat | That account's session died or is in cooldown. Click **Reconnect** on the error — it re-runs the gateway login, no terminal needed |
| `provider 'codex' is not configured for <folder>` | A project config without Codex intentionally replaces the user config; add `[providers.codex]` there or remove the project config |
| `provider 'codex' … could not be loaded: ZEST_GATEWAY_KEY is not set` | Copy `.env` to `~/.zest/.env`, or set the variable for your user account |
| Smart App Control blocks build scripts | Windows Security → Smart App Control Off → reboot → `cargo clean` / rebuild |
| `zest-desktop.exe` locked | Close the running app before `cargo run` / rebuild |
| File lock on Windows | Close the desktop process; retry |

---

## Notes

- The agent runs in **Rust**. Node is only used to build the desktop UI.
- The desktop installer also carries the pinned CLIProxyAPI executable; Zest provisions and
  supervises it, so users still install only Zest.
- One chat stays on one provider; other accounts are used through delegated workers when configured.
- `bash` has no OS sandbox — the approval gate is the containment. See `memory/decisions.md`.
- Usage in Settings shows what Zest used — not your full subscription remaining.

---

## License

Local-only alpha. See the repository license when published.
