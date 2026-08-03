# Download the CLIProxyAPI gateway binary Zest bundles as a Tauri sidecar.
#
# The gateway is MIT-licensed and redistributable, so Zest ships it rather than
# asking every user to install it. Run this before `cargo tauri build`; the
# binaries are gitignored because they are ~64MB each.
#
# Bundling for another platform is the same command with -Target, so a release
# box can fetch every sidecar it needs:
#
#   .\scripts\fetch-gateway.ps1
#   .\scripts\fetch-gateway.ps1 -Target aarch64-apple-darwin
#   .\scripts\fetch-gateway.ps1 -Version 7.2.116
#
# ASCII only, deliberately: Windows PowerShell 5.1 reads a BOM-less UTF-8 script
# as ANSI, and a stray non-ASCII byte in a comment breaks the parse.

[CmdletBinding()]
param(
    # Rust target triple to fetch for. Defaults to this machine's.
    [string]$Target,
    # Release to fetch. Defaults to the latest published release.
    [string]$Version,
    # Re-download even when the sidecar is already present.
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$repo = "router-for-me/CLIProxyAPI"
$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "crates\desktop\binaries"
$licenseDir = Join-Path $root "crates\desktop\licenses"

function Get-HostTarget {
    $arch = "x86_64"
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { $arch = "aarch64" }
    return "$arch-pc-windows-msvc"
}

# Rust target triple -> the release asset that carries that build.
function Get-AssetSuffix([string]$triple) {
    switch -Regex ($triple) {
        '^x86_64-pc-windows'     { return "windows_amd64.zip" }
        '^aarch64-pc-windows'    { return "windows_aarch64.zip" }
        '^x86_64-apple-darwin'   { return "darwin_amd64.tar.gz" }
        '^aarch64-apple-darwin'  { return "darwin_aarch64.tar.gz" }
        '^x86_64-unknown-linux'  { return "linux_amd64.tar.gz" }
        '^aarch64-unknown-linux' { return "linux_aarch64.tar.gz" }
        default { throw "No CLIProxyAPI release asset is published for '$triple'." }
    }
}

if (-not $Target) { $Target = Get-HostTarget }
$isWindowsTarget = $Target -match 'windows'
$binName = if ($isWindowsTarget) { "cli-proxy-api.exe" } else { "cli-proxy-api" }
# Tauri resolves a sidecar by appending the target triple, and strips the suffix
# again when bundling, so the installed name is plain `cli-proxy-api`.
$sidecar = if ($isWindowsTarget) { "cli-proxy-api-$Target.exe" } else { "cli-proxy-api-$Target" }
$sidecarPath = Join-Path $outDir $sidecar

if ((Test-Path $sidecarPath) -and (-not $Force)) {
    Write-Host "Already present: $sidecarPath (use -Force to re-download)"
    exit 0
}

if (-not $Version) {
    Write-Host "Resolving latest release of $repo ..."
    $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ "User-Agent" = "zest-build" }
    $Version = $latest.tag_name -replace '^v', ''
}

$asset = "CLIProxyAPI_${Version}_$(Get-AssetSuffix $Target)"
$base = "https://github.com/$repo/releases/download/v$Version"
$work = Join-Path ([System.IO.Path]::GetTempPath()) "zest-gateway-$Version-$Target"
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Path $work | Out-Null

$archive = Join-Path $work $asset
Write-Host "Downloading $asset ..."
Invoke-WebRequest -Uri "$base/$asset" -OutFile $archive -UseBasicParsing

# Verify before extracting. This binary is about to be signed into Zest's own
# installer, so an unverified download would make Zest the delivery vehicle for
# whatever it fetched.
$sumsPath = Join-Path $work "checksums.txt"
Invoke-WebRequest -Uri "$base/checksums.txt" -OutFile $sumsPath -UseBasicParsing
$expectedLine = Get-Content $sumsPath | Where-Object { $_ -match [regex]::Escape($asset) } | Select-Object -First 1
if (-not $expectedLine) { throw "checksums.txt has no entry for $asset" }
$expected = ($expectedLine -split '\s+')[0]
$actual = (Get-FileHash -Path $archive -Algorithm SHA256).Hash
if ($actual -ne $expected.ToUpper()) {
    throw "SHA256 mismatch for ${asset}: expected $expected, got $actual"
}
Write-Host "SHA256 verified."

$extract = Join-Path $work "x"
New-Item -ItemType Directory -Path $extract | Out-Null
if ($asset.EndsWith(".zip")) {
    Expand-Archive -Path $archive -DestinationPath $extract -Force
} else {
    tar -xzf $archive -C $extract
    if ($LASTEXITCODE -ne 0) { throw "tar failed to extract $asset" }
}

$found = Get-ChildItem -Path $extract -Recurse -File -Filter $binName | Select-Object -First 1
if (-not $found) { throw "No $binName inside $asset" }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Copy-Item $found.FullName $sidecarPath -Force

# MIT requires the copyright notice and licence text travel with the binary.
$licenseSrc = Get-ChildItem -Path $extract -Recurse -File | Where-Object { $_.Name -like "LICENSE*" } | Select-Object -First 1
if ($licenseSrc) {
    New-Item -ItemType Directory -Force -Path $licenseDir | Out-Null
    Copy-Item $licenseSrc.FullName (Join-Path $licenseDir "CLIProxyAPI-LICENSE.txt") -Force
} else {
    Write-Warning "No LICENSE in the archive. Add it to crates/desktop/licenses/ by hand before shipping."
}

Remove-Item $work -Recurse -Force
Write-Host "Sidecar ready: $sidecarPath (CLIProxyAPI v$Version)"
