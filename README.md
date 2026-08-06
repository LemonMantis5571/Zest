# Zest

Zest is a local-first coding workbench. It reads and edits a project, streams
tool calls, asks before risky actions, and keeps the desktop UI and terminal
front-end on the same Rust agent core.

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)

> **Beta**: the current release line is Windows-first and still evolving. The
> installer, configuration, and JSONL protocol may change before 1.0. Keep a
> backup of important workspaces and do not use the beta as your only recovery
> path.

## What is included

| Area | Beta behavior |
| --- | --- |
| Desktop | Tauri + React app with project chat, provider setup, Workbench activity, transcript outline, fork, and automatic context compaction. |
| Terminal | `zest` CLI using the same agent loop, tools, approvals, and thread format. |
| Codex | Local CLIProxyAPI gateway, bundled and pinned for Windows releases. |
| API-key providers | Anthropic through its configured environment variable, plus OpenAI-compatible endpoints such as DeepSeek, OpenAI, and local servers. OpenAI-compatible keys use the OS credential manager. |
| External workers | Claude Code and Gemini CLI through their already-authenticated CLI/ACP sessions. They are delegated workers, not Zest sign-in providers. |
| Automation | JSONL/headless turns for editors and CI. Interactive approval prompts are deny-only in headless mode. |

Zest is not a hosted service. Project files, conversation state, gateway
credentials, and API keys stay on the machine unless a configured provider or
external worker sends them to its service.

## Install the beta on Windows

For a published beta, download the `.msi` or `.exe` installer from
[GitHub Releases](https://github.com/LemonMantis5571/Zest/releases). End users do
not need Rust, Node.js, npm, or a separate CLIProxyAPI installation: the desktop
package carries the pinned gateway sidecar and provisions it on first run.

1. Download the installer from the release page.
2. Prefer the signed artifact when one is available. If a release publishes
   `SHA256SUMS.txt`, verify the file before running it:

   ~~~powershell
   Get-FileHash ./Zest_0.1.0_x64-setup.exe -Algorithm SHA256
   ~~~

3. Install Zest and open a project folder.
4. Choose Codex, or open Settings to configure an API-key provider.
5. If Windows asks for a firewall exception, allow local loopback access so
   the bundled gateway can talk to the desktop app. Zest does not need a public
   inbound listener.

If no beta artifact is published yet, use the source build below. An unsigned
developer build may produce a Windows SmartScreen warning; verify its origin
before allowing it.

## Build from source

### Development prerequisites

| Requirement | Version | Used for |
| --- | --- | --- |
| Windows | 10 or 11 | Primary desktop target |
| Rust | 1.97.1 | Pinned by `rust-toolchain.toml` |
| Node.js | 24.16.0 | Pinned by `.nvmrc` and `package.json` |
| npm | 11.13.0 | Installed with Node.js |
| Git | Current | Source checkout and isolated worker worktrees |
| WebView2 | Evergreen runtime | Tauri desktop webview; normally present on Windows 10/11 |
| Visual Studio Build Tools | C++ workload + Windows SDK | Native Rust/Tauri builds |

The repository also supports the Rust and Node toolchains on other platforms,
but the bundled gateway and installer workflow in this beta is Windows-first.

### Checkout and run

~~~powershell
git clone https://github.com/LemonMantis5571/Zest.git
cd Zest

# The repository pins Rust and Node versions.
rustup show
node --version
npm --version

# Install the exact JavaScript dependency graph.
npm ci

# Download and verify the pinned CLIProxyAPI sidecar for this target.
./scripts/fetch-gateway.ps1

# Build the static webview once.
npm run ui:build

# Run the desktop app with the built webview.
cargo run -p zest-desktop
~~~

For UI work with Vite hot reload:

~~~powershell
npm run desktop:dev
~~~

To run the terminal front-end from a project directory:

~~~powershell
cargo run -p zest -- --help
cargo run -p zest
~~~

The desktop shell loads `crates/desktop/ui/dist`. If the app shows a blank or
old screen after a UI change, run `npm run ui:build` before launching the Rust
desktop binary.

### Gateway setup for Codex

The source build needs the sidecar before it can package or run the normal
Codex path. `fetch-gateway.ps1` reads the exact release and SHA256 values from
`crates/desktop/gateway-release.json`; it never resolves `latest`.

~~~powershell
./scripts/fetch-gateway.ps1 -Check
./scripts/start-gateway.ps1
./scripts/codex-login-gateway.ps1
~~~

The desktop app can also start the bundled gateway and launch the Codex sign-in
flow. On first run, the bundled gateway creates a loopback-only configuration
under `%APPDATA%/zest/gateway/`. A hand-installed `tools/CLIProxyAPI/` tree or
an existing `ZEST_GATEWAY_KEY` remains the explicit override.

## Build installers

`npm run desktop:build` builds the UI, Tauri application, MSI, and NSIS bundle.
The gateway sidecar must already be present and pass its provenance check.

~~~powershell
./scripts/fetch-gateway.ps1 -Check
npm run desktop:build
./scripts/release-checksums.ps1 -OutFile SHA256SUMS.txt
~~~

Artifacts are written below:

~~~text
target/release/bundle/msi/
target/release/bundle/nsis/
~~~

For a signed release, use a certificate in the Windows certificate store. The
thumbprint is not a private key and the signing script never handles the key
material itself:

~~~powershell
./scripts/build-signed.ps1 -Thumbprint A1B2C3D4E5F6...
./scripts/release-checksums.ps1 -OutFile SHA256SUMS.txt
~~~

See [docs/RELEASING.md](docs/RELEASING.md) for the clean-machine test and
release checklist. Generated gateway binaries, UI output, and signing overlays
are intentionally ignored by Git.

## First-run provider setup

| Provider | How it authenticates | Where it is configured |
| --- | --- | --- |
| Codex | Codex sign-in through the local CLIProxyAPI gateway | `zest.toml` plus the gateway credential store |
| Anthropic | `ANTHROPIC_API_KEY` or the configured `api_key_env` variable | `zest.toml` plus the environment; no desktop keychain action |
| OpenAI-compatible | API key in the OS credential manager, with optional environment fallback | `zest.toml` for endpoint/model; Settings for the key |
| Claude Code / Gemini CLI | Sign in directly with the vendor CLI | `[agents.*]` in `zest.toml` or Settings > External workers |

### API keys

For an OpenAI-compatible provider:

1. Open Settings or the provider picker.
2. Choose **Add API provider** and select DeepSeek, OpenAI, or Custom.
3. Enter the key and save it.

The key is stored under the `zest` service in the OS credential manager. Zest
does not write it to `zest.toml`, return it to the UI, put it in logs, or pass it
through the model/tool context. The endpoint, model list, and credential name
are configuration, not secrets. Native Anthropic setup uses the environment
variable named by `api_key_env` and is not entered in this dialog.

### DeepSeek

The built-in DeepSeek example uses the two configured models:

~~~toml
[providers.deepseek]
kind = "openai_compatible"
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
models = ["deepseek-v4-flash", "deepseek-v4-pro"]
credential = "deepseek"
~~~

Zest appends `/chat/completions` to `base_url`. Model discovery is not
automatic; the configured model list is authoritative.

### OpenAI and local servers

~~~toml
[providers.openai]
kind = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-5"
credential = "openai"

[providers.local]
kind = "openai_compatible"
base_url = "http://localhost:11434/v1"
model = "qwen2.5-coder"
# No credential is required for a local server.
~~~

`credential` names the OS credential-manager entry. A provider may omit it for
an unauthenticated local endpoint. `api_key_env` remains available for CI and
headless environments; the value itself must never be committed.

## External workers

Claude Code and Gemini CLI are optional workers. Authenticate them in their own
CLIs first, then enable them in Settings > External workers or declare them in
`zest.toml`:

~~~toml
[agents.claude]
mode = "headless"
command = "claude"
args = ["--print", "--output-format", "stream-json", "--strict-mcp-config", "{prompt}"]
workspace = "isolated"

[agents.gemini]
mode = "acp"
command = "gemini"
args = ["--acp"]
workspace = "isolated"
~~~

Zest checks that the executable is available and lets the CLI own its session;
there is no Zest OAuth or provider connection flow for these workers. Delegated
work is bounded, approval-gated, and isolated in a Git worktree by default. The
worker returns an answer and diff for review. A non-Git folder can use
`workspace = "current"`, but that gives the worker access to the active
checkout after approval and should be chosen deliberately.

## Safety and recovery

- File writes, shell commands, and other risky tools show an approval request.
  Denying a request cancels that tool call; it does not corrupt the turn.
- Approval is not an operating-system sandbox. `bash` can still do anything the
  user account can do after approval. Use a disposable workspace for untrusted
  projects.
- Workbench keeps tool activity, a transcript outline, and recovery checkpoints.
  **Fork conversation** creates a new thread from the current conversation;
  it does not copy or roll back workspace files.
- Automatic compaction starts after a completed turn reaches 80% of the active
  model context window. A checkpoint is kept first. If usage is unavailable,
  the meter is an estimate and does not pretend to know exact token counts.
- The context meter is informational for provider usage; it is not a
  subscription balance or quota guarantee.

## Configuration

Zest reads configuration from the project and user locations. A project
`zest.toml` is authoritative for that project; it replaces rather than merges
the user provider table so the account that may be charged is explicit.

~~~text
<project>/zest.toml
%USERPROFILE%/.zest/zest.toml
~~~

The repository `zest.toml` is a safe starting template. Do not add credentials
or private tokens to it. Use the desktop credential setup or an ignored `.env`
file for local secrets.

Useful environment variables:

| Variable | Purpose |
| --- | --- |
| `ZEST_GATEWAY_KEY` | Key that matches a hand-installed CLIProxyAPI `api-keys` entry. |
| `ZEST_MODEL` | One-off model override. |
| `ZEST_EFFORT` | Optional effort override when the selected model supports it. |
| `ZEST_BASE_URL` | One-off gateway origin override; do not include `/v1/messages`. |
| `ANTHROPIC_API_KEY` | Native Anthropic provider fallback. |
| Provider `api_key_env` | Optional CI/headless key fallback for that provider. |

## JSONL and headless mode

Use JSONL for editor integrations and CI:

~~~powershell
zest run --jsonl -- "Review the changed files and list any risks"
"Fix the failing test" | zest run --jsonl
~~~

Every stdout line is a JSON object. The stream begins with a `session` object
using protocol `zest-jsonl-v1`, then emits text, thinking, tool-call,
approval, model-substitution, and final `done` or `error` events. Diagnostics
go to stderr so stdout stays machine-readable. Headless approval is deny-only:
gated actions are reported and denied instead of waiting for an interactive
window. `--json` remains a compatibility alias.

## Verification

Run the same checks used by Windows CI:

~~~powershell
./scripts/verify.ps1
~~~

The gate covers the gateway pin, reproducible npm install, UI tests/lint/build,
Rust formatting, clippy with warnings denied, workspace library tests, ts-rs
binding drift, npm audit, RustSec advisories, and Git whitespace checks.

The live doctor is separate because it needs credentials and spends real
provider quota:

~~~powershell
cargo run -p zest -- doctor --live
~~~

It is opt-in and should only be reported as passed after a real gateway/provider
run. It does not write files or run external workers.

## Troubleshooting

| Symptom | Action |
| --- | --- |
| Blank or old desktop UI | Run `npm ci` and `npm run ui:build`, then rebuild the desktop app. |
| `ZEST_GATEWAY_KEY is not set` | This applies to a hand-installed gateway. Match `.env` to its `api-keys`; the bundled gateway generates its own key. |
| Codex is not signed in | Run `./scripts/codex-login-gateway.ps1` or use **Connect** in the desktop picker. |
| Gateway connection refused | Run `./scripts/fetch-gateway.ps1 -Check`; close stale gateway processes and retry. |
| API-key provider says unconfigured | Confirm the provider block is in the active project/user config, then save the key again in Settings. The key is never displayed after saving. |
| Claude/Gemini worker unavailable | Sign in with that CLI directly and use **Check CLI** in Settings > External workers. |
| External worker requires Git | Initialize or open a Git repository, or deliberately choose `workspace = "current"` with the approval boundary in mind. |
| Model or effort control is missing | The selected model does not advertise that capability; Zest hides unsupported controls instead of sending an invalid request. |
| Desktop executable is locked | Close Zest before rebuilding on Windows. |
| Smart App Control blocks a developer build | Use a signed release, or follow the local Windows policy for development builds; then clean and rebuild. |

## Repository layout

~~~text
crates/core/       Agent loop, providers, tools, skills, threads, usage
crates/cli/        `zest` terminal front-end
crates/desktop/    Tauri shell and Rust commands
crates/desktop/ui/ React/Vite webview
scripts/           Gateway, verification, packaging, and checksum helpers
context/           Architecture and product decisions
memory/            Durable project decisions and corrections
docs/              Security, release, and audit documentation
~~~

Contributor workflow and source conventions are in
[CONTRIBUTING.md](CONTRIBUTING.md). The release process is in
[docs/RELEASING.md](docs/RELEASING.md), and the latest codebase audit is in
[docs/CODEBASE_AUDIT.md](docs/CODEBASE_AUDIT.md). User-facing release notes are
in [CHANGELOG.md](CHANGELOG.md).

## License and third-party notices

Zest is released under the [MIT License](LICENSE). The desktop bundle includes
CLIProxyAPI as a pinned sidecar; its separate license notice is kept at
[crates/desktop/licenses/CLIProxyAPI-LICENSE.txt](crates/desktop/licenses/CLIProxyAPI-LICENSE.txt).
Other dependencies retain their own upstream licenses.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the dependency index
and [SECURITY.md](SECURITY.md) for responsible disclosure guidance.
