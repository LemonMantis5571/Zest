# Contributing

Thanks for helping improve Zest. Small, focused changes are easier to review
and safer to release than broad refactors.

## Before you start

Read the [README](README.md) for the product workflow and supported platforms.
For release work, also read [docs/RELEASING.md](docs/RELEASING.md).

Use the toolchain versions pinned in `rust-toolchain.toml`, `.nvmrc`, and
`package.json`:

- Rust 1.97.1
- Node.js 24.16.0+
- npm 11.13.0+

## Local development

From the repository root:

```powershell
npm ci
./scripts/fetch-gateway.ps1
npm run ui:build
npm run desktop:dev
```

Run the terminal client with:

```powershell
cargo run -p zest
```

The shared Rust library is in `crates/core`, the terminal client is in
`crates/cli`, and the desktop application is in `crates/desktop`. The desktop
web UI lives in `crates/desktop/ui`.

When changing desktop data types, regenerate TypeScript bindings using the
existing ts-rs test workflow rather than editing generated files by hand.

## Verification

Run the repository verification gate before opening a pull request:

```powershell
./scripts/release-verify.ps1
```

The gate checks formatting, linting, Rust and UI tests, generated bindings,
dependency advisories, and Git whitespace. Live provider checks are separate:
they require credentials and may consume real quota.

Add a focused regression test for behavior changes. For UI changes, update the
relevant characterization tests under `crates/desktop/ui/src`.

## Keep out of commits

Do not commit:

- API keys, gateway keys, `.env` files, credential-manager exports, or signing
  keys;
- downloaded gateway binaries or generated `ui/dist` output; or
- local signing configuration or personal `zest.toml` files.

Use [`zest.toml.example`](zest.toml.example) for shareable configuration
documentation.

## Code and UI conventions

- Keep provider-independent behavior in `crates/core`.
- Preserve approval and credential boundaries when changing execution paths.
- Keep user-facing copy actionable and free of debugging details.
- Reuse the existing design tokens and local UI primitives.
- Explain why non-obvious code exists, especially around process and security
  boundaries.

## Pull requests

Use [Conventional Commits](https://www.conventionalcommits.org/), such as:

```text
feat(provider): add compatible endpoint setup
fix(cli): handle streamed tool metadata
docs(release): explain beta installers
chore(deps): remove unused direct dependency
```

Keep each pull request scoped. Describe user-visible behavior, verification
performed, and any migration or release impact.

## Security reports

Do not open a public issue for a vulnerability. Follow the private reporting
process in [SECURITY.md](SECURITY.md).
