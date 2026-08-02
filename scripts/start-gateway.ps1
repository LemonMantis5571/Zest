# Start the local CLIProxyAPI gateway used by [providers.codex] in zest.toml.
# Requires a one-time install under tools/CLIProxyAPI (gitignored). See README.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$tools = Join-Path $root "tools\CLIProxyAPI"
$exe = Join-Path $tools "cli-proxy-api.exe"
$cfg = Join-Path $tools "config.yaml"

if (-not (Test-Path $exe)) {
    Write-Error "CLIProxyAPI not found at $exe. Download the Windows amd64 release into tools/CLIProxyAPI."
}
if (-not (Test-Path $cfg)) {
    Write-Error "Missing $cfg — copy config.example.yaml to config.yaml and set api-keys."
}

Write-Host "Starting CLIProxyAPI on http://127.0.0.1:8317 ..."
Start-Process -FilePath $exe -ArgumentList @("-config", $cfg) -WorkingDirectory $tools
Start-Sleep -Seconds 2
try {
    $code = (Invoke-WebRequest -Uri "http://127.0.0.1:8317/" -UseBasicParsing -TimeoutSec 3).StatusCode
    Write-Host "OK — HTTP $code"
} catch {
    Write-Warning "Proxy did not answer yet: $($_.Exception.Message)"
}
