# Open CLIProxyAPI's Codex OAuth flow (browser). Credentials land in ~/.cli-proxy-api.
# This is separate from `codex login` used by the Zest desktop Connect button.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$tools = Join-Path $root "tools\CLIProxyAPI"
$exe = Join-Path $tools "cli-proxy-api.exe"
$cfg = Join-Path $tools "config.yaml"

if (-not (Test-Path $exe)) {
    Write-Error "CLIProxyAPI not found at $exe."
}

Write-Host "Launching Codex login for the gateway..."
Start-Process -FilePath $exe -ArgumentList @("-config", $cfg, "-codex-login") -WorkingDirectory $tools
Write-Host "Complete the browser sign-in, then: cargo run -p zest"
