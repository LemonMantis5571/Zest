# Zest

**Multi-provider coding harness in Rust** — read and edit a project, stream tool calls, approve writes, stay local.

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)

Zest is the Claude Code / Codex shape: filesystem tools, permissions, sessions. It is **not** a chatbot platform.

> Not [LimeBot](https://github.com/LemonMantis5571/LimeBot-OS). LimeBot is a long-running personal assistant (Python, Discord/Telegram/etc.). Zest borrows design lessons only — no shared code or runtime.

| | |
|---|---|
| **Status** | Stable Windows **alpha** — reliability first |
| **Live path** | Codex via [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) (local gateway) |
| **UI** | Tauri desktop + React/shadcn webview |
| **CLI** | `zest` — same agent loop, terminal front-end |

---

## Requirements

| Tool | Version | Notes |
|------|---------|--------|
| Windows | 10/11 | Primary target |
| [Rust](https://rustup.rs/) | **1.97.1** | Pinned in `rust-toolchain.toml` |
| [Node.js](https://nodejs.org/) | **24.16.0** | `.nvmrc` / `package.json` engines |
| npm | **11.13.0** | Comes with Node |
| CLIProxyAPI | latest Windows amd64 | Local Codex OAuth gateway |

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

### 3. Gateway (Codex)

1. Download the **Windows amd64** [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) release.
2. Extract to `tools/CLIProxyAPI/` (gitignored — not in the repo).
3. Configure `tools/CLIProxyAPI/config.yaml`:
   - listen on `127.0.0.1:8317`
   - add an `api-keys` entry (any strong random string)
4. Put the **same** key in a repo-root `.env`:

```powershell
copy .env.example .env
# Edit .env → ZEST_GATEWAY_KEY=<same key as api-keys>
```

5. Start the gateway and complete Codex OAuth **through the proxy** (not only `codex login`):

```powershell
.\scripts\start-gateway.ps1
.\scripts\codex-login-gateway.ps1
```

Desktop **Connect** can also spawn this login when the gateway tree is present.

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

1. Gateway running on `127.0.0.1:8317`
2. `.env` has `ZEST_GATEWAY_KEY`
3. Codex signed in via gateway (`~/.cli-proxy-api` session present)
4. `npm run ui:build` already succeeded
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

### Approvals

`write_file` asks **Allow once / Deny** in the desktop UI. CLI denies writes by default.

### Chat history

Threads live under `.zest/threads/` (gitignored). New chat / recent threads are in Settings.

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

`zest.toml` declares providers and routing. Keys are never stored there — only env var **names**.

| Variable | Purpose |
|----------|---------|
| `ZEST_GATEWAY_KEY` | Must match CLIProxyAPI `api-keys` |
| `ZEST_MODEL` | Optional model override |
| `ZEST_EFFORT` | Optional effort override |
| `ZEST_BASE_URL` | One-off gateway origin override (no `/v1/messages`) |
| `ANTHROPIC_API_KEY` | Native Anthropic when you uncomment that provider |

---

## Verify & live doctor

```powershell
.\scripts\verify.ps1
```

ASCII-safe PowerShell 5.1 gate: `npm ci` → UI test/lint/build → `cargo fmt` / clippy `-D warnings` /
tests → ts-rs binding drift → `npm audit` → RustSec (`cargo audit`) → `git diff --check`.
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
| `ZEST_GATEWAY_KEY is not set` | Copy `.env.example` → `.env` and match gateway `api-keys` |
| Codex not Signed in | Run `.\scripts\codex-login-gateway.ps1` or Connect in the picker |
| Connection refused `:8317` | `.\scripts\start-gateway.ps1` |
| Model rejected / Luna missing | Rebuild with current core (Codex has a built-in Sol/Terra/Luna catalogue) |
| Custom persona ignored | **Save** in Settings, then send a **new** message (or New chat) |
| `zest-desktop.exe` locked | Close the running app before `cargo run` / rebuild |
| File lock on Windows | Close the desktop process; retry |

---

## Design constraints (short)

- Agent path is **Rust** — Node is build/dev for the webview only.
- Providers are first-class; gateway vs native is an implementation detail.
- Parent chat is **provider-pinned**; multi-provider work uses `delegate` workers.
- No bash/exec until an OS sandbox exists.
- Subscription headroom is estimated when providers don’t expose a remaining quota API — never labeled as a live reading.

Details: `memory/decisions.md`.

---

## License

See repository license when published. Local-only alpha; gateway OAuth ToS risk is accepted by the operator — see `memory/decisions.md`.
