# Contributing

Zest is a Windows-first beta. Small, focused changes are easier to review than
large refactors, especially around the provider, approval, gateway, and ACP
boundaries.

## Before you start

Read:

1. [`AGENTS.md`](AGENTS.md) and [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md);
2. the relevant files under [`context/`](context/); and
3. the matching skill and durable corrections under [`skills/`](skills/) and
   [`memory/`](memory/).

Use Rust 1.97.1, Node 24.16.0, and npm 11.13.0. The pinned versions are in
`rust-toolchain.toml`, `.nvmrc`, and `package.json`.

## Local development

~~~powershell
npm ci
./scripts/fetch-gateway.ps1
npm run ui:build
npm run desktop:dev       # Tauri + Vite hot reload
cargo run -p zest          # terminal front-end
~~~

The desktop UI is in `crates/desktop/ui`. The Rust command layer is in
`crates/desktop/src`, and provider/tool behavior belongs in `crates/core`.
Regenerate TypeScript bindings only through the documented ts-rs test when
changing the DTOs in `crates/desktop/src/lib.rs`.

## Verification before a PR

Run the same gate as Windows CI:

~~~powershell
./scripts/verify.ps1
~~~

It runs the gateway pin check, `npm ci`, UI test/lint/build, Rust formatting,
strict clippy, workspace library tests, binding drift, npm audit, RustSec, and
Git whitespace checks. Keep live-provider verification separate: it requires
credentials and consumes real quota.

Do not commit:

- API keys, gateway keys, `.env` files, credential-manager exports, or private
  signing keys;
- downloaded gateway binaries or generated `ui/dist` output; or
- signing overlays containing local certificate configuration.

`zest.toml` is ignored because it is normally a local/project override. Use
`zest.toml.example` for shareable configuration documentation, and never force-
add a personal config or put a secret value in either file.

## Packaging

The exact sidecar and release process are documented in
[docs/RELEASING.md](docs/RELEASING.md). The short path is:

~~~powershell
./scripts/fetch-gateway.ps1 -Check
npm run desktop:build
./scripts/release-checksums.ps1 -OutFile SHA256SUMS.txt
~~~

The package must contain the CLIProxyAPI release pinned in
`crates/desktop/gateway-release.json` and the matching
`crates/desktop/licenses/CLIProxyAPI-LICENSE.txt` notice. Public or commercial
distribution requires a current review of the upstream vendor terms; the
notice covers the bundled software license, not every provider service.

Before calling an installer ready, use a clean Windows profile with no source
checkout, Rust, Node.js, hand-installed gateway, or existing Zest state. Test
first-run gateway provisioning, Codex sign-in, API-key presence without key
rendering, a denied approval, restart persistence, and useful missing-CLI
errors.

## Code and UI conventions

- Keep provider-independent agent behavior in `crates/core`.
- Treat approval, credential, gateway, and worktree boundaries as security
  boundaries; do not broaden them for convenience.
- Keep user-facing copy actionable and free of internal debugging language.
- Use the existing design tokens and local UI primitives. Avoid adding portal
  components that regress in the Tauri WebView.
- Add a focused regression test for behavior changes. For UI state, add or
  update the characterization tests under `crates/desktop/ui/src`.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/), for example:

~~~text
feat(provider): add compatible endpoint setup
fix(cli): handle streamed tool metadata
docs(release): explain beta installers
chore(deps): remove unused direct dependency
~~~

Keep commits scoped and explain any user-visible migration in the body.

## Security reports

Do not use a public issue for a vulnerability. Follow
[SECURITY.md](SECURITY.md) and use a private GitHub Security Advisory.
