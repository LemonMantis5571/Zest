#Requires -Version 5.1
# Emit SHA256 checksums for the built Windows installers.
#
# Until the installers are signed, this is what lets someone verify that the
# file they downloaded is the file you built. It is not a substitute for a
# signature - it proves integrity, not identity, and only if they get the
# checksum from somewhere you control rather than from beside the download.
#
#   .\scripts\release-checksums.ps1
#   .\scripts\release-checksums.ps1 -OutFile SHA256SUMS.txt
#
# ASCII only: Windows PowerShell 5.1 reads a BOM-less UTF-8 script as ANSI.

[CmdletBinding()]
param(
    [string]$OutFile
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$bundleDir = Join-Path $root "target\release\bundle"

$artifacts = @()
foreach ($sub in @("nsis", "msi")) {
    $dir = Join-Path $bundleDir $sub
    if (Test-Path -LiteralPath $dir) {
        $artifacts += Get-ChildItem -LiteralPath $dir -File |
            Where-Object { $_.Extension -in ".exe", ".msi" }
    }
}

if (-not $artifacts) {
    throw "No installers under $bundleDir. Run a release build first."
}

$lines = foreach ($item in $artifacts) {
    $hash = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $size = "{0:N1} MB" -f ($item.Length / 1MB)
    "{0}  {1}   ({2})" -f $hash, $item.Name, $size
}

$lines | ForEach-Object { Write-Host $_ }

if ($OutFile) {
    $path = if ([System.IO.Path]::IsPathRooted($OutFile)) { $OutFile } else { Join-Path $root $OutFile }
    $lines | Out-File -LiteralPath $path -Encoding ascii
    Write-Host "`nWritten to $path"
}

Write-Host "`nTell people to check with:"
Write-Host '  Get-FileHash .\Zest_0.1.0_x64-setup.exe -Algorithm SHA256'
