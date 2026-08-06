# Releasing Zest

This checklist is for maintainers preparing a Windows beta. It separates
reproducible build evidence, installer integrity, signing, and live-provider
verification so a missing credential never becomes a fake release pass.

## Before the build

1. Start from a clean worktree, except for an intentional local `zest.toml`.
   Never stage provider keys, `.env` files, gateway credentials, signing
   overlays, or generated binaries.
2. Update the version in both `Cargo.toml` (`workspace.package.version`) and
   `crates/desktop/tauri.conf.json`.
3. Update the release notes and call out beta limitations or migrations.
4. Review the pinned CLIProxyAPI release and its license notice. If the pin
   changes, run the gateway verification before packaging.

## Verification gate

Run from the repository root on the pinned Windows toolchain:

```powershell
./scripts/fetch-gateway.ps1 -CheckPin
./scripts/verify.ps1
```

The verification script covers the UI, Rust, generated bindings, dependency
advisories, and Git whitespace. Keep its output with the release record.

The live doctor is separate and consumes real provider quota:

```powershell
cargo run -p zest -- doctor --live
```

Only run it with a test account and report it separately from compilation and
automated tests. Do not put live keys in CI or release notes.

## Build and sign

Fetch the exact sidecar, build both Windows installer formats, and emit hashes:

```powershell
./scripts/fetch-gateway.ps1 -Check
npm run desktop:build
./scripts/release-checksums.ps1 -OutFile SHA256SUMS.txt
```

For Authenticode signing, keep the private key in the certificate store or
signing service. Use the public certificate thumbprint only:

```powershell
./scripts/build-signed.ps1 -Thumbprint A1B2C3D4E5F6...
./scripts/release-checksums.ps1 -OutFile SHA256SUMS.txt
```

The signing overlay is ignored by Git. Do not commit it unless the repository
policy explicitly changes.

## Clean-machine acceptance

Test the exact MSI and NSIS artifacts on a Windows profile or VM that has no
Rust, Node.js, `tools/CLIProxyAPI`, old Zest state, or manually started gateway.

Confirm:

- install and uninstall complete without a console window;
- the bundled gateway provisions on first run and listens only on loopback;
- Codex sign-in through the gateway works after a restart;
- API-key setup stores presence in the OS credential manager without rendering
  the key again;
- a minimal chat can read a file, asks before a write, and recovers when the
  request is denied;
- provider status and model selection remain correct after reopening the app;
- an external worker reports a useful missing-CLI or authentication error;
- fork, automatic compaction, and JSONL/headless behavior match the beta notes;
- the installer runs on a machine with no source checkout or developer tools.

Do not claim live-provider verification from a compile, unit test, or mock
server result. Record provider name, model, date, and whether real quota was
used without recording credentials or response contents.

## Publish

Publish the MSI, NSIS installer, `SHA256SUMS.txt`, release notes,
`THIRD_PARTY_NOTICES.md`, the CLIProxyAPI license notice, and any additional
license files required by the resolved dependency graph. Mark the release as a
pre-release while the beta contract is still changing. Link the source commit
and the verification result.

After publication, install the uploaded artifacts once rather than only the
local copies, then record the final URLs and hashes.
