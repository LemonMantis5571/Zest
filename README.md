<div align="center">

<img src="./assets/logo.png" alt="Zest Logo" width="128" height="128" />

# Zest Harness

**The provider-aware, local-first coding harness built in Rust.**

Zest gives coding AI models a focused, secure execution workspace on your machine. Stream responses, run terminal commands, view real-time diffs, and delegate complex tasks to external tools—all while keeping full control over your code, your secrets, and your quota.

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)
[![Linux verify](https://github.com/LemonMantis5571/Zest/actions/workflows/linux-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/linux-verify.yml)

</div>

---

## Why Zest Harness?

Most AI coding tools lock you into proprietary SaaS subscriptions, hide token usage under opaque telemetry, or attempt to auto-route your prompts through middleman services.

**Zest is built differently:**

- ⚡ **Blazing-Fast Rust Core**: Powered by `zest-core`, an ultra-lightweight, multi-threaded agent engine that drives both our sleek desktop app and terminal CLI.
- 🔌 **Provider-Aware Freedom**: Bring your own parent session: Codex or Claude subscriptions, the Anthropic API, or any OpenAI-compatible endpoint (OpenAI, DeepSeek, local Ollama, …). You choose the exact model and provider for every session.
- 🤝 **Explicit Worker Delegation (ACP)**: Offload heavy subtasks to already-authenticated external CLIs (such as Claude Code or Gemini CLI). Workers run in isolated Git worktrees and return diffs for your review.
- 🔒 **Zero Lock-In & Local-First**: No remote Zest servers, no user accounts, and zero telemetry. Sensitive credentials stay in your OS credential manager — Windows Credential Manager, macOS Keychain, or Linux Secret Service (GNOME Keyring / KWallet).
- 🛡️ **Human-in-the-Loop Safety**: Irreversible actions—like file writes, terminal executions, or subagent tasks—require your explicit approval with clear diff previews.
- 📊 **Honest Usage Ledger & Auto-Compaction**: Real-time context tracking and automatic compaction (at 80% context window) with persistent conversation checkpoints so you never lose context or spend quota blindly.

---

## Supported Platforms

| Platform | Status |
| --- | --- |
| **Windows 10/11** (x64, ARM64) | Primary target. Verified on every push. |
| **Linux** (x64, ARM64) | Supported. Verified on every push. Requires the system packages listed under [Building from source](#how-to-build-from-source). |
| **macOS** | Keychain and file-open code paths exist, but not yet covered by CI. |

---

## Interfaces

| Interface | Description | Ideal For |
| --- | --- | --- |
| **Zest Desktop** | Sleek Tauri + React desktop shell with visual diffs, provider pickers, and transcript outlines. | Rich interactive pair-programming sessions. |
| **Zest CLI** | Lightweight REPL running directly in your terminal using the exact same Rust agent core. | Quick terminal runs, SSH sessions, and scriptable workflows. |

---

## How to Install

### Windows

1. Download the latest `.msi` or `.exe` installer from [GitHub Releases](https://github.com/LemonMantis5571/Zest/releases).
2. *(Optional)* Verify the installer checksum using PowerShell:

   ```powershell
   Get-FileHash ./Zest_0.1.0_x64-setup.exe -Algorithm SHA256
   ```

3. Run the installer and launch Zest.
4. If Windows Firewall prompts for permissions, allow local loopback access so the desktop application can communicate with its bundled local gateway.

### Linux

1. Download the `.deb`, `.rpm`, or AppImage package for your distribution from [GitHub Releases](https://github.com/LemonMantis5571/Zest/releases).
2. Install with your distribution's tooling:

   ```bash
   sudo dpkg -i ./zest_0.1.0_amd64.deb    # Debian / Ubuntu
   sudo rpm -i ./zest-0.1.0-1.x86_64.rpm  # Fedora / openSUSE
   chmod +x ./Zest_0.1.0_amd64.AppImage && ./Zest_0.1.0_amd64.AppImage
   ```

3. *(Optional)* Verify the package checksum with `sha256sum`.

---

## How to Build from Source

### Prerequisites

- **Rust**: 1.97.1 (managed via `rustup`; pinned in `rust-toolchain.toml`)
- **Node.js**: 24.16.0+ & **npm** (pinned in `.nvmrc` / `package.json`)
- **Git**
- **PowerShell** to run the gateway fetch and verify scripts: Windows PowerShell 5.1+ on Windows, PowerShell 7+ (`pwsh`) on Linux.

**Windows**

- **WebView2**: Evergreen runtime (included with Windows 10/11)
- **Visual Studio Build Tools**: C++ workload with Windows SDK

**Linux** (Debian/Ubuntu package names; other distributions ship the same libraries under different names)

```bash
sudo apt install build-essential pkg-config libssl-dev cmake \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libdbus-1-dev libxdo-dev patchelf tar
```

- `libwebkit2gtk-4.1-dev` — the WebView used by Tauri (required to build the desktop app).
- `libdbus-1-dev` + `pkg-config` — required by `keyring`'s Secret Service backend, which stores API keys in GNOME Keyring / KWallet.
- `cmake` — required to build `aws-lc-rs` (TLS).
- `patchelf` — used by Tauri when bundling AppImages.

### Build & Run Steps

1. **Clone the repository**:

   ```bash
   git clone https://github.com/LemonMantis5571/Zest.git
   cd Zest
   ```

2. **Install JavaScript dependencies**:

   ```bash
   npm ci
   ```

3. **Fetch the pinned gateway sidecar** (the bundled local gateway binary Zest ships for provider sign-in):

   ```bash
   ./scripts/fetch-gateway.ps1        # Windows
   pwsh ./scripts/fetch-gateway.ps1   # Linux / macOS
   ```

   Use `-Target <rust-target-triple>` to fetch the sidecar for a platform other than the host, and `-Check` to verify an existing one.

4. **Build the web UI**:

   ```bash
   npm run ui:build
   ```

5. **Run Zest Desktop**:

   ```bash
   cargo run -p zest-desktop
   ```

6. *(Optional)* **Run Zest CLI**:

   ```bash
   cargo run -p zest
   ```

### Building Installers

```bash
npm run desktop:build
```

Generated artifacts:

- Windows: `target/release/bundle/msi/` and `target/release/bundle/nsis/`
- Linux: `target/release/bundle/deb/`, `target/release/bundle/rpm/`, and `target/release/bundle/appimage/`

Release builds require the pinned gateway sidecar for the target platform; `scripts/fetch-gateway.ps1 -Target <triple>` fetches it.

---

## Quick Setup & Configuration

### 1. Provider Setup (API Keys)

Configure model providers via **Settings > Add API provider** in Zest Desktop. Advanced users can use a local `zest.toml`; the repository ships only `zest.toml.example`. Keys are stored securely in your OS credential manager — Windows Credential Manager, macOS Keychain, or Linux Secret Service (GNOME Keyring / KWallet).

Zest creates the user-level configuration at `~/.zest/zest.toml` on first launch. You do not need to create a project `zest.toml` unless you want provider or worker settings that apply only to that project.

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

## Verifying a Build

The same gate Windows and Linux CI run is available locally:

```bash
npm run verify
```

It checks the gateway release pin and sidecar, installs npm dependencies, runs UI tests/lint/build, strict Rust clippy, workspace library tests, generated-binding drift, npm audit, RustSec, and whitespace hygiene.

---

## License

Zest is open source software released under the [MIT License](LICENSE).
