<div align="center">

<img src="./assets/logo.png" alt="Zest logo" width="512" height="512" />

# Zest

[![Windows verify](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/windows-verify.yml)
[![Linux verify](https://github.com/LemonMantis5571/Zest/actions/workflows/linux-verify.yml/badge.svg)](https://github.com/LemonMantis5571/Zest/actions/workflows/linux-verify.yml)
[![Latest release](https://img.shields.io/github/v/release/LemonMantis5571/Zest?include_prereleases&label=latest%20beta)](https://github.com/LemonMantis5571/Zest/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**🧭 A local-first coding workspace with approvals, diffs, and optional model delegation. 🛠️**

Zest helps you use AI in real projects while keeping control of your files,
commands, credentials, and model accounts.

> **Beta:** Zest is ready for early adopters. Keep backups and review every
> approval before accepting it.

[Quick start](#quick-start) · [Build from source](#build-from-source) · [Contributing](CONTRIBUTING.md)

</div>

## Table of contents

- [Why Zest](#why-zest)
- [Features](#features)
- [Quick start](#quick-start)
- [Build from source](#build-from-source)
- [Plugins](#plugins)
- [Configuration](#configuration)
- [Supported platforms](#supported-platforms)
- [Documentation](#documentation)
- [Contributing](#contributing)

## Why Zest

AI coding tools are most useful when they can make progress without making
unreviewed changes. Zest gives you a focused workspace for that loop:

1. Choose the model provider you want to use.
2. Open a project and describe the change.
3. Review proposed writes and commands before they run.
4. Inspect the diff, continue the conversation, or reject the change.

Zest runs locally, keeps credentials in your operating system's credential
store, and does not require a Zest account or send telemetry to a Zest server.
You can use one model for planning and hand focused tasks to an already
configured specialist CLI when that suits the work.

## Features

- **Optional plugins** - add local integrations without rebuilding the Zest
  desktop app.
- **Approvals with diff previews** — review file changes and shell commands
  before accepting them.
- **Desktop and terminal clients** — use a focused desktop workspace or the
  `zest` terminal client.
- **Bring your own provider** — connect supported sign-ins, native APIs, or
  OpenAI-compatible endpoints.
- **Usage and quota** — keep local usage separate from live provider limits and
  official balances.
- **Resumable sessions** — keep project chats, checkpoints, and context
  handling available across restarts.
- **Optional task delegation** — send bounded work to a configured external
  coding CLI and review the result before accepting it.

## Quick start

### Install a release

Download the latest beta installer or package from
[GitHub Releases](https://github.com/LemonMantis5571/Zest/releases).

- **Windows** — install the `.msi` or `.exe` package.
- **Linux** — install the `.deb` or `.rpm` package, or run the AppImage.

Each release includes a platform checksum file. Verify the download before
installing it.

Launch Zest, choose a provider in Settings, open a project folder, and start a
chat. The first time Zest is about to write a file or run a command, it shows
you the proposed action and waits for your approval.

### Terminal client

```bash
zest
```

## Build from source

You need Rust 1.97.1, Node.js 24.16.0+, npm, Git, and PowerShell. Linux builds
also need the desktop libraries listed in
[`CONTRIBUTING.md`](CONTRIBUTING.md).

```powershell
npm ci
./scripts/fetch-gateway.ps1
npm run ui:build
cargo run -p zest-desktop
```

To run the terminal client instead:

```powershell
cargo run -p zest
```

On Linux or macOS, run the PowerShell scripts with `pwsh`.

## Plugins

Plugins are optional add-ons. They are not bundled with official releases and
there is no automatic plugin download yet. Install them separately by copying
their folder into the Zest plugin folder, then use **Settings > Extras >
Refresh > Turn on**.

On Windows, the folder is:

```text
%LOCALAPPDATA%\Zest\plugins
```

For the included Windows music add-on, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-now-playing-plugin.ps1
```

To build it without installing:

```powershell
cargo build -p zest-now-playing-plugin --release
```

`npm run dev` starts Zest. Cargo builds Rust plugins. The full install guide,
plugin standard, protocol, security rules, and review checklist are in
[`docs/PLUGINS.md`](docs/PLUGINS.md).

## Configuration

Zest creates a user-level configuration at `~/.zest/zest.toml`. You can add a
project-local override by copying [`zest.toml.example`](zest.toml.example) to
`zest.toml`.

Keep secrets out of configuration files. Zest stores supported API credentials
in the operating system's credential manager; sign-ins owned by a provider
remain with that provider.

## Supported platforms

| Platform | Status |
| --- | --- |
| Windows 10/11 (x64) | Primary target and verified in CI |
| Linux (x64) | Supported and verified in CI |
| Windows/Linux ARM64 | Source paths exist; beta installers are not published yet |
| macOS | Supported code paths; not yet CI-verified |

## Documentation

- [Plugins](docs/PLUGINS.md) - install, build, and develop optional add-ons
- [Skills](docs/SKILLS.md) - personal skills and install locations
- [Provider quota](docs/QUOTA.md) - live limits, balances, and provider limits
- [Contributing](CONTRIBUTING.md) — development, verification, and pull requests
- [Design notes](DESIGN.md) — product and architecture context
- [Releasing](docs/RELEASING.md) — maintainer release checklist
- [Beta release notes](docs/releases/0.1.0.md) — scope and known limits
- [Changelog](CHANGELOG.md) — user-facing changes
- [Security policy](SECURITY.md) — vulnerability reporting
- [Third-party notices](THIRD_PARTY_NOTICES.md) — dependency attribution

## Contributing

Issues and pull requests are welcome. Start with
[`CONTRIBUTING.md`](CONTRIBUTING.md), keep changes focused, and include tests
for behavior changes. Please report security vulnerabilities privately using
the process in [`SECURITY.md`](SECURITY.md).

Zest is released under the [MIT License](LICENSE).
