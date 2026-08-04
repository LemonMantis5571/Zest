# Contributing

## Prerequisites

Match the pinned toolchain in the [README](README.md) (Rust 1.97.1, Node 24.16.0, npm 11.13.0).

## Verify before a PR

```powershell
.\scripts\verify.ps1
```

That runs `cargo fmt --check`, clippy `-D warnings`, workspace tests, UI lint, and UI build.

## Desktop UI

```powershell
npm install
npm run ui:build          # required before cargo run -p zest-desktop
npm run desktop:dev       # optional HMR
```

Regenerate ts-rs bindings after changing `ChatEvent` / `SessionInfo` in `crates/desktop/src/lib.rs` — see `crates/desktop/ui/src/lib/generated/README.md`.

## Package the desktop gateway

The package must contain exactly the CLIProxyAPI release pinned in
`crates/desktop/gateway-release.json`:

```powershell
.\scripts\fetch-gateway.ps1
.\scripts\fetch-gateway.ps1 -Check
npm run desktop:build
# npm run desktop:build also checks the configured target directory for the sidecar and MIT notice.
```

Before treating a release as ready, install it in a clean Windows VM or user profile with no
`tools/CLIProxyAPI`, `ZEST_CLIPROXY_PATH`, gateway config, or existing process. Confirm first-run
loopback provisioning, no console window, Codex and Claude Connect, chat after restart, useful
missing/corrupt-gateway errors, and that a separate hand-installed gateway still overrides the
bundle. Public or commercial distribution also requires a current vendor-terms review; the MIT
notice only covers redistribution of CLIProxyAPI itself.

## Agent docs

Follow `AGENTS.md` → `PROJECT_CONTEXT.md` → `context/` → relevant `skills/`. Record durable corrections in the matching `learnings.md` or `memory/`.

## Commit style

[Conventional Commits](https://www.conventionalcommits.org/): `feat(scope):`, `fix:`, `docs:`, `chore:`, `ci:`.
