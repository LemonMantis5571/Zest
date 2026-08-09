#Requires -Version 5.1
# ASCII-safe Windows beta verify gate (PowerShell 5.1).
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

# The whole directory, not a hand-kept list. The list had drifted to 7 of 11
# files, so ThreadCheckpoint, TurnRecovery, WorkspaceReview and CommandView
# could change shape without the gate noticing — and nothing about adding a
# binding reminds you to add it here.
$BindingDir = "crates/desktop/ui/src/lib/generated"

function Normalize-BindingWhitespace {
  # ts-rs versions differ only in trailing spaces on a few generated lines.
  # Strip that generator noise everywhere it can appear rather than from three
  # named files, so a new binding cannot silently opt out. Trailing whitespace
  # is never meaningful in the generated TypeScript, so this can only remove
  # false failures, never mask a real change.
  $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  $dir = Join-Path $Root $BindingDir
  foreach ($file in Get-ChildItem -Path $dir -Filter *.ts -File) {
    $text = [System.IO.File]::ReadAllText($file.FullName)
    $clean = [regex]::Replace($text, '(?m)[ \t]+(?=\r?$)', '')
    if ($clean -ne $text) {
      [System.IO.File]::WriteAllText($file.FullName, $clean, $utf8NoBom)
    }
  }
}

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

Step "gateway release and sidecar" {
  # CI starts from a clean checkout. Fetch the pinned host sidecar so Tauri's
  # build script can validate its external binary during Clippy and tests.
  # Forward slashes keep Join-Path correct under pwsh on Linux (a literal
  # backslash would become part of the file name there).
  & (Join-Path $Root "scripts/fetch-gateway.ps1")
}

Step "npm ci" {
  npm ci --no-fund --no-audit
}

# Ahead of every UI step, on purpose. Generating after the UI had already
# compiled meant `ui build` type-checked against whatever was committed, so a
# Rust type change could pass the build and only be caught — if at all — by a
# gate several steps later.
Step "binding drift (ts-rs)" {
  cargo test -p zest-desktop --features export-bindings --lib export_bindings
  Normalize-BindingWhitespace
  git diff --exit-code -- $BindingDir
  if ($LASTEXITCODE -ne 0) { throw "Generated bindings are stale. Commit the regenerated files." }

  # `git diff` cannot see a file that has never been tracked, and a brand new
  # binding is exactly the drift most worth catching.
  $untracked = git ls-files --others --exclude-standard -- $BindingDir
  if ($untracked) {
    throw "New generated bindings are not committed:`n$($untracked -join "`n")"
  }
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
  git diff --check --ignore-space-at-eol
  if ($LASTEXITCODE -ne 0) { throw "Unstaged whitespace errors found." }
  git diff --cached --check --ignore-space-at-eol
  if ($LASTEXITCODE -ne 0) { throw "Staged whitespace errors found." }
}

Write-Host ""
Write-Host "verify.ps1 passed" -ForegroundColor Green
Write-Host "Live doctor is opt-in: cargo run -p zest -- doctor --live"
Write-Host "(requires gateway/creds; do not fake success without them)"
