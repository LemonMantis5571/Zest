<div align="center">

<img src="assets/logo.png" alt="Zest Logo" width="128" height="128" />

# Zest Harness

**The provider-aware, local-first coding harness built in Rust.**

Zest gives coding AI models a focused, secure execution workspace on your machine. Stream responses, run terminal commands, view real-time diffs, and delegate complex tasks to external tools—all while keeping full control over your code, your secrets, and your quota.

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)

</div>

---

## Why Zest Harness?

Most AI coding tools lock you into proprietary SaaS subscriptions, hide token usage under opaque telemetry, or attempt to auto-route your prompts through middleman services.

**Zest is built differently:**

- ⚡ **Blazing-Fast Rust Core**: Powered by `zest-core`, an ultra-lightweight, multi-threaded agent engine that drives both our sleek desktop app and terminal CLI.
- 🔌 **Provider-Aware Freedom**: Connect directly to Anthropic, OpenAI, DeepSeek, local Ollama servers, or Codex subscriptions. You choose the exact model and provider for every parent session.
- 🤝 **Explicit Worker Delegation (ACP)**: Offload heavy subtasks to already-authenticated external CLIs (such as Claude Code or Gemini CLI). Workers run in isolated Git worktrees and return diffs for your review.
- 🔒 **Zero Lock-In & Local-First**: No remote Zest servers, no user accounts, and zero telemetry. Sensitive credentials stay in your Windows OS Credential Manager.
- 🛡️ **Human-in-the-Loop Safety**: Irreversible actions—like file writes, terminal executions, or subagent tasks—require your explicit approval with clear diff previews.
- 📊 **Honest Usage Ledger & Auto-Compaction**: Real-time context tracking and automatic compaction (at 80% context window) with persistent conversation checkpoints so you never lose context or spend quota blindly.

---

## Interfaces

| Interface | Description | Ideal For |
| --- | --- | --- |
| **Zest Desktop** | Sleek Tauri + React desktop shell with visual diffs, provider pickers, and transcript outlines. | Rich interactive pair-programming sessions. |
| **Zest CLI** | Lightweight REPL running directly in your terminal using the exact same Rust agent core. | Quick terminal runs, SSH sessions, and scriptable workflows. |

---

## How to Install from MSI or EXE

For pre-built releases on Windows:

1. Download the latest `.msi` or `.exe` installer from [GitHub Releases](https://github.com/LemonMantis5571/Zest/releases).
2. *(Optional)* Verify the installer checksum using PowerShell:

   ```powershell
   Get-FileHash ./Zest_0.1.0_x64-setup.exe -Algorithm SHA256
   ```

3. Run the installer and launch Zest.
4. If Windows Firewall prompts for permissions, allow local loopback access so the desktop application can communicate with its bundled local gateway.

---

## How to Build from Source

### Prerequisites

- **OS**: Windows 10 or 11
- **Rust**: 1.97.1 (managed via `rustup`)
- **Node.js**: 24.16.0+ & **npm**
- **Git**
- **WebView2**: Evergreen runtime (included with Windows 10/11)
- **Visual Studio Build Tools**: C++ workload with Windows SDK

### Build & Run Steps

1. **Clone the repository**:

   ```powershell
   git clone https://github.com/LemonMantis5571/Zest.git
   cd Zest
   ```

2. **Install JavaScript dependencies**:

   ```powershell
   npm ci
   ```

3. **Fetch required gateway sidecar**:

   ```powershell
   ./scripts/fetch-gateway.ps1
   ```

4. **Build the web UI**:

   ```powershell
   npm run ui:build
   ```

5. **Run Zest Desktop**:

   ```powershell
   cargo run -p zest-desktop
   ```

6. *(Optional)* **Run Zest CLI**:

   ```powershell
   cargo run -p zest
   ```

### Building Installers (`.msi` / `.exe`)

To package installers from source:

```powershell
./scripts/fetch-gateway.ps1 -Check
npm run desktop:build
```

Generated installer artifacts will be created in:
- `target/release/bundle/msi/`
- `target/release/bundle/nsis/`

---

## Quick Setup & Configuration

### 1. Provider Setup (API Keys)
Configure model providers via **Settings > Add API provider** in Zest Desktop. Advanced users can use a local `zest.toml`; the repository ships only `zest.toml.example`. Keys are stored securely in your OS Credential Manager.

Zest creates the user-level configuration at `%USERPROFILE%\.zest\zest.toml` on first launch. You do not need to create a project `zest.toml` unless you want provider or worker settings that apply only to that project.

```toml
[providers.deepseek]
kind = "openai_compatible"
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
credential = "deepseek"
```

### 2. External Workers (Claude Code & Gemini CLI)
Delegate bounded subtasks to external CLI tools without re-authenticating. Sign into the vendor CLI on your machine and declare it in your local `zest.toml` or Desktop Settings. MCP pass-through is off by default; enable it per worker only if you want that CLI to use its own configured MCP servers. Zest does not manage those servers or review individual MCP calls.

```toml
[agents.claude]
mode = "headless"
command = "claude"
args = ["--print", "--output-format", "stream-json", "--strict-mcp-config", "--model", "{model}", "{prompt}"]
model = "sonnet"
allow_mcp = false
workspace = "isolated"
```

When enabled, Claude uses its existing MCP configuration and Gemini uses its existing MCP server
configuration. The same settings are available from Settings > CLI delegation. The worker model is
independent from Zest's selected chat model; choose **CLI default** to let the vendor CLI decide.
The delegation approval still applies before the worker starts, but MCP calls remain controlled by
the external CLI.

---

## License

Zest is open source software released under the [MIT License](LICENSE).
