#Requires -Version 5.1
# Download the pinned CLIProxyAPI gateway binary Zest bundles as a Tauri sidecar.
#
# The gateway is MIT-licensed and redistributable, so Zest ships it rather than
# asking every user to install it. Run this before `cargo tauri build`; the
# binaries are gitignored because they are ~64MB each.
#
# `crates/desktop/gateway-release.json` is the trust anchor. Updating the
# gateway means reviewing one pinned release and committing its exact archive
# hashes; this script never resolves "latest" and never trusts a remote checksum
# fetched beside the archive.
#
# Bundling for another platform is the same command with -Target:
#
#   .\scripts\fetch-gateway.ps1
#   .\scripts\fetch-gateway.ps1 -Target aarch64-apple-darwin
#   .\scripts\fetch-gateway.ps1 -Check
#   .\scripts\fetch-gateway.ps1 -CheckPin
#
# ASCII only, deliberately: Windows PowerShell 5.1 reads a BOM-less UTF-8 script
# as ANSI, and a stray non-ASCII byte in a comment breaks the parse.

[CmdletBinding()]
param(
    # Rust target triple to fetch for. Defaults to this machine's.
    [string]$Target,
    # Re-download even when the sidecar is already present.
    [switch]$Force,
    # Verify the current target's sidecar and provenance without downloading.
    [switch]$Check,
    # Verify only committed release metadata and the bundled licence.
    [switch]$CheckPin
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "crates\desktop\binaries"
$licenseDir = Join-Path $root "crates\desktop\licenses"
$licensePath = Join-Path $licenseDir "CLIProxyAPI-LICENSE.txt"
$pinPath = Join-Path $root "crates\desktop\gateway-release.json"
$requiredTargets = @(
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu"
)

function Get-HostTarget {
    try {
        # Do not pipe through Select-Object: closing that pipeline early can
        # leave PowerShell's LASTEXITCODE at -1 even when rustc succeeded.
        $tuple = & rustc --print host-tuple 2>$null
        $rustcExit = $LASTEXITCODE
        if ($rustcExit -eq 0 -and $tuple) { return ([string]$tuple).Trim() }
    } catch {
        # Fall through to the Windows-only compatibility path below.
    }
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
    return "$arch-pc-windows-msvc"
}

function Read-ReleasePin {
    if (-not (Test-Path -LiteralPath $pinPath -PathType Leaf)) {
        throw "Gateway release pin is missing: $pinPath"
    }
    try {
        $release = Get-Content -LiteralPath $pinPath -Raw | ConvertFrom-Json
    } catch {
        throw "Gateway release pin is not valid JSON: $($_.Exception.Message)"
    }
    if ($release.schema -ne 1) { throw "Unsupported gateway release pin schema '$($release.schema)'." }
    if ($release.repository -ne "router-for-me/CLIProxyAPI") {
        throw "Unexpected gateway repository '$($release.repository)'."
    }
    if ([string]::IsNullOrWhiteSpace([string]$release.version)) {
        throw "Gateway release pin has no version."
    }
    foreach ($triple in $requiredTargets) {
        $property = $release.targets.PSObject.Properties[$triple]
        if ($null -eq $property) { throw "Gateway release pin has no entry for '$triple'." }
        $entry = $property.Value
        if ([string]::IsNullOrWhiteSpace([string]$entry.asset)) {
            throw "Gateway release pin has no asset for '$triple'."
        }
        if ([System.IO.Path]::GetFileName([string]$entry.asset) -ne [string]$entry.asset) {
            throw "Gateway asset for '$triple' must be a file name, not a path."
        }
        $prefix = "CLIProxyAPI_$($release.version)_"
        if (-not ([string]$entry.asset).StartsWith($prefix)) {
            throw "Gateway asset '$($entry.asset)' does not match pinned version $($release.version)."
        }
        if ([string]$entry.sha256 -notmatch '^[0-9a-fA-F]{64}$') {
            throw "Gateway SHA256 for '$triple' is invalid."
        }
        if ([string]$entry.binary_sha256 -notmatch '^[0-9a-fA-F]{64}$') {
            throw "Gateway binary SHA256 for '$triple' is invalid."
        }
        $expectedArch = if ($triple.StartsWith("x86_64-")) { "_amd64" } else { "_aarch64" }
        if (-not ([string]$entry.asset).Contains($expectedArch)) {
            throw "Gateway asset '$($entry.asset)' does not match target architecture '$triple'."
        }
    }
    return $release
}

function Assert-License([string]$path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "CLIProxyAPI licence is missing: $path"
    }
    $text = Get-Content -LiteralPath $path -Raw
    if ($text -notmatch 'MIT License' -or
        $text -notmatch 'Copyright \(c\) 2025\.9-present Router-For\.ME' -or
        $text -notmatch 'Permission is hereby granted') {
        throw "CLIProxyAPI licence is incomplete or unexpected: $path"
    }
}

function Get-UnixMode([string]$path) {
    $stat = Get-Command stat -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $stat) { return $null }

    $mode = & $stat.Source -c "%a" -- $path 2>$null
    if ($LASTEXITCODE -ne 0) {
        $mode = & $stat.Source -f "%Lp" -- $path 2>$null
    }
    if ($LASTEXITCODE -ne 0) { return $null }

    $text = ($mode | Out-String).Trim()
    if ($text -notmatch '^[0-7]{3,4}$') { return $null }
    return [Convert]::ToInt32($text, 8)
}

function Assert-UnixExecutable([string]$path) {
    $mode = Get-UnixMode $path
    if ($null -eq $mode) {
        throw "Cannot inspect Unix executable permissions for $path. Run this target fetch on the packaging OS."
    }
    if (($mode -band 73) -eq 0) {
        throw "CLIProxyAPI sidecar is not executable: $path"
    }
}

function Set-UnixExecutable([string]$path) {
    $chmod = Get-Command chmod -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $chmod) {
        throw "Cannot set Unix executable permissions for $path. Run this target fetch on the packaging OS."
    }
    & $chmod.Source +x -- $path
    if ($LASTEXITCODE -ne 0) { throw "chmod failed for $path" }
    Assert-UnixExecutable $path
}

function Test-CurrentSidecar(
    [string]$path,
    [string]$stampPath,
    [object]$release,
    [object]$entry,
    [string]$triple,
    [bool]$isWindowsTarget
) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
    if (-not (Test-Path -LiteralPath $stampPath -PathType Leaf)) { return $false }
    try {
        $stamp = Get-Content -LiteralPath $stampPath -Raw | ConvertFrom-Json
    } catch {
        return $false
    }
    if ($stamp.schema -ne 1 -or
        $stamp.repository -ne $release.repository -or
        $stamp.version -ne $release.version -or
        $stamp.target -ne $triple -or
        $stamp.asset -ne $entry.asset -or
        ([string]$stamp.archive_sha256).ToLowerInvariant() -ne ([string]$entry.sha256).ToLowerInvariant() -or
        ([string]$stamp.binary_sha256).ToLowerInvariant() -ne ([string]$entry.binary_sha256).ToLowerInvariant() -or
        [string]$stamp.binary_sha256 -notmatch '^[0-9a-fA-F]{64}$') {
        return $false
    }
    $actualBinary = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($actualBinary -ne ([string]$stamp.binary_sha256).ToUpperInvariant()) { return $false }
    if (-not $isWindowsTarget) {
        try {
            Assert-UnixExecutable $path
        } catch {
            return $false
        }
    }
    return $true
}

$release = Read-ReleasePin
if ($Check -and $CheckPin) { throw "Use only one of -Check or -CheckPin." }
if ($CheckPin) {
    Assert-License $licensePath
    Write-Host "Gateway release pin OK: CLIProxyAPI v$($release.version), $($requiredTargets.Count) targets"
    return
}

if (-not $Target) { $Target = Get-HostTarget }
$targetProperty = $release.targets.PSObject.Properties[$Target]
if ($null -eq $targetProperty) {
    throw "No pinned CLIProxyAPI release asset for '$Target'."
}
$targetPin = $targetProperty.Value
$isWindowsTarget = $Target -match 'windows'
$binName = if ($isWindowsTarget) { "cli-proxy-api.exe" } else { "cli-proxy-api" }
# Tauri resolves a sidecar by appending the target triple, and strips the suffix
# again when bundling, so the installed name is plain `cli-proxy-api`.
$sidecar = if ($isWindowsTarget) { "cli-proxy-api-$Target.exe" } else { "cli-proxy-api-$Target" }
$sidecarPath = Join-Path $outDir $sidecar
$stampPath = "$sidecarPath.source.json"

if ($Check) {
    Assert-License $licensePath
    if (-not (Test-CurrentSidecar $sidecarPath $stampPath $release $targetPin $Target $isWindowsTarget)) {
        throw "Bundled gateway is missing, stale, or corrupt. Run .\scripts\fetch-gateway.ps1 -Target $Target"
    }
    Write-Host "Gateway sidecar OK: $sidecarPath (CLIProxyAPI v$($release.version))"
    return
}

if ((-not $Force) -and
    (Test-CurrentSidecar $sidecarPath $stampPath $release $targetPin $Target $isWindowsTarget)) {
    Assert-License $licensePath
    Write-Host "Already present and verified: $sidecarPath (CLIProxyAPI v$($release.version))"
    return
}

$repo = [string]$release.repository
$version = [string]$release.version
$asset = [string]$targetPin.asset
$expected = ([string]$targetPin.sha256).ToUpperInvariant()
$base = "https://github.com/$repo/releases/download/v$version"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("zest-gateway-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $work | Out-Null

try {
    $archive = Join-Path $work $asset
    Write-Host "Downloading pinned $asset ..."
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $archive -UseBasicParsing

    # The committed hash is the trust anchor. Fetching checksums from the same
    # release would let a compromised release replace both files together.
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actual -ne $expected) {
        throw "SHA256 mismatch for ${asset}: expected $expected, got $actual"
    }
    Write-Host "Pinned SHA256 verified."

    $extract = Join-Path $work "x"
    New-Item -ItemType Directory -Path $extract | Out-Null
    if ($asset.EndsWith(".zip")) {
        Expand-Archive -LiteralPath $archive -DestinationPath $extract -Force
    } else {
        tar -xzf $archive -C $extract
        if ($LASTEXITCODE -ne 0) { throw "tar failed to extract $asset" }
    }

    $found = Get-ChildItem -LiteralPath $extract -Recurse -File -Filter $binName | Select-Object -First 1
    if (-not $found) { throw "No $binName inside $asset" }

    # MIT requires the copyright notice and licence text travel with the binary.
    # Absence is fatal: Zest must never produce an installer without the notice.
    $licenseSrc = Get-ChildItem -LiteralPath $extract -Recurse -File |
        Where-Object {
            $licenseText = Get-Content -LiteralPath $_.FullName -Raw
            $_.Name -eq "LICENSE" -and
                $licenseText -match 'MIT License' -and
                $licenseText -match 'Copyright \(c\) 2025\.9-present Router-For\.ME' -and
                $licenseText -match 'Permission is hereby granted'
        } |
        Select-Object -First 1
    if (-not $licenseSrc) { throw "No CLIProxyAPI LICENSE inside pinned archive $asset." }
    Assert-License $licenseSrc.FullName

    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    New-Item -ItemType Directory -Force -Path $licenseDir | Out-Null
    Copy-Item -LiteralPath $found.FullName -Destination $sidecarPath -Force
    Copy-Item -LiteralPath $licenseSrc.FullName -Destination $licensePath -Force
    Assert-License $licensePath
    if (-not $isWindowsTarget) { Set-UnixExecutable $sidecarPath }

    $binaryHash = (Get-FileHash -LiteralPath $sidecarPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($binaryHash -ne ([string]$targetPin.binary_sha256).ToLowerInvariant()) {
        throw "Binary SHA256 mismatch for ${asset}: expected $($targetPin.binary_sha256), got $binaryHash"
    }
    $stamp = [ordered]@{
        schema = 1
        repository = $repo
        version = $version
        target = $Target
        asset = $asset
        archive_sha256 = $expected.ToLowerInvariant()
        binary_sha256 = $binaryHash
    }
    $stamp | ConvertTo-Json | Set-Content -LiteralPath $stampPath -Encoding Ascii
} finally {
    if (Test-Path -LiteralPath $work) {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
}

Write-Host "Sidecar ready: $sidecarPath (CLIProxyAPI v$version)"
