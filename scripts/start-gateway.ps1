# Start the local CLIProxyAPI gateway by hand.
#
# Rarely needed: Zest starts the gateway itself when a turn needs one. This is
# for poking at the gateway without launching the app.
#
# It resolves the same binary and config Zest would, in the same order. Starting
# it from some other config is what used to cause `401 Invalid API key` - the
# gateway would run with one key while Zest sent another.
#
# ASCII only: Windows PowerShell 5.1 reads a BOM-less UTF-8 script as ANSI.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

# 1. A hand-installed gateway keeps its own config, and Zest defers to it.
$exe = Join-Path $root "tools\CLIProxyAPI\cli-proxy-api.exe"
$cfg = Join-Path $root "tools\CLIProxyAPI\config.yaml"

# 2. Otherwise the bundled sidecar plus the config Zest provisions for it.
if (-not ((Test-Path $exe) -and (Test-Path $cfg))) {
    $exe = Join-Path $root "crates\desktop\binaries\cli-proxy-api-x86_64-pc-windows-msvc.exe"
    $cfg = Join-Path $env:APPDATA "zest\gateway\config.yaml"
}

if (-not (Test-Path $exe)) {
    Write-Error "No gateway binary. Run .\scripts\fetch-gateway.ps1 first."
}
if (-not (Test-Path $cfg)) {
    Write-Error "No gateway config at $cfg. Launch Zest once to provision one."
}

if (Get-Process -Name "cli-proxy*" -ErrorAction SilentlyContinue) {
    Write-Host "A gateway is already running. Stop it first to start a different config:"
    Write-Host "  Stop-Process -Name cli-proxy-api -Force"
    exit 0
}

Write-Host "Starting $(Split-Path -Leaf $exe) on http://127.0.0.1:8317"
Write-Host "  config: $cfg"
# Hidden, to match how Zest spawns it. Without this the gateway pops a console
# window that outlives the script and looks like Zest opened a terminal.
Start-Process -FilePath $exe -ArgumentList @("-config", $cfg) -WindowStyle Hidden

for ($i = 0; $i -lt 20; $i++) {
    Start-Sleep -Milliseconds 300
    $tcp = Test-NetConnection -ComputerName 127.0.0.1 -Port 8317 -InformationLevel Quiet -WarningAction SilentlyContinue
    if ($tcp) { Write-Host "OK - accepting on 127.0.0.1:8317"; exit 0 }
}
Write-Warning "Gateway did not accept within 6s. Check $cfg."
