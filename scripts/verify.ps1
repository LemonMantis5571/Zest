#Requires -Version 5.1
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

Step "cargo fmt --check" {
  cargo fmt --all -- --check
}

Step "cargo clippy (strict)" {
  cargo clippy --workspace --all-targets -- -D warnings
}

Step "cargo test" {
  cargo test --workspace
}

Step "npm install (workspaces)" {
  npm install --no-fund --no-audit
}

Step "ui lint" {
  npm run ui:lint
}

Step "ui build" {
  npm run ui:build
}

Step "cargo deny / audit (best-effort)" {
  if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    cargo deny check
  } elseif (Get-Command cargo-audit -ErrorAction SilentlyContinue) {
    cargo audit
  } else {
    Write-Host "cargo-deny/audit not installed — skipping (install for full alpha gate)"
  }
}

Step "git diff --check" {
  git diff --check
  git diff --cached --check
}

Write-Host ""
Write-Host "verify.ps1 passed" -ForegroundColor Green
