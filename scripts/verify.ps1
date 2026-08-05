#Requires -Version 5.1
# ASCII-safe Stable Windows Alpha verify gate (PowerShell 5.1).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Step($name, $scriptBlock) {
  Write-Host ""
  Write-Host "==> $name" -ForegroundColor Cyan
  & $scriptBlock
  if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
    throw "Step failed: $name (exit $LASTEXITCODE)"
  }
}

$env:CARGO_TARGET_DIR = Join-Path $Root "target"

Step "toolchain check" {
  $rustc = (rustc --version)
  if ($rustc -notmatch "1\.97\.1") {
    Write-Warning "Expected rustc 1.97.1, got: $rustc"
  }
  $node = (node --version)
  if ($node -ne "v24.16.0") {
    Write-Warning "Expected node v24.16.0, got: $node"
  }
}

Step "gateway release pin" {
  & (Join-Path $Root "scripts\fetch-gateway.ps1") -CheckPin
}

Step "npm ci" {
  npm ci --no-fund --no-audit
}

Step "ui test" {
  npm run ui:test
}

Step "ui lint (strict)" {
  npm run ui:lint
}

Step "ui build" {
  npm run ui:build
}

Step "cargo fmt --check" {
  cargo fmt --all -- --check
}

Step "cargo clippy (strict)" {
  cargo clippy --workspace --all-targets -- -D warnings
}

Step "cargo test" {
  # --lib: avoid executing Tauri bin test harnesses (WDAC/App Control can block them).
  cargo test --workspace --lib
}

Step "binding drift (ts-rs)" {
  cargo test -p zest-desktop --features export-bindings --lib export_bindings
  git diff --exit-code -- `
    "crates/desktop/ui/src/lib/generated/ChatEvent.ts" `
    "crates/desktop/ui/src/lib/generated/SessionInfo.ts" `
    "crates/desktop/ui/src/lib/generated/ProviderView.ts" `
    "crates/desktop/ui/src/lib/generated/ExternalAgentView.ts" `
    "crates/desktop/ui/src/lib/generated/ExternalAgentCheckView.ts" `
    "crates/desktop/ui/src/lib/generated/ModelCapability.ts" `
    "crates/desktop/ui/src/lib/generated/ToolMetaView.ts"
}

Step "npm audit" {
  npm audit --omit=dev
}

Step "RustSec (cargo audit)" {
  if (Get-Command cargo-audit -ErrorAction SilentlyContinue) {
    cargo audit
  } elseif (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    cargo deny check advisories
  } else {
    throw "cargo-audit (or cargo-deny) is required for the RustSec gate. Install: cargo install cargo-audit --locked"
  }
}

Step "git diff --check" {
  git diff --check
  git diff --cached --check
}

Write-Host ""
Write-Host "verify.ps1 passed" -ForegroundColor Green
Write-Host "Live doctor is opt-in: cargo run -p zest -- doctor --live"
Write-Host "(requires gateway/creds; do not fake success without them)"
