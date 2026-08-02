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

## Agent docs

Follow `AGENTS.md` → `PROJECT_CONTEXT.md` → `context/` → relevant `skills/`. Record durable corrections in the matching `learnings.md` or `memory/`.

## Commit style

[Conventional Commits](https://www.conventionalcommits.org/): `feat(scope):`, `fix:`, `docs:`, `chore:`, `ci:`.
