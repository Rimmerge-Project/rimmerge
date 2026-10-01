# Builds the release zips from already-built release binaries:
#   Rimmerge-<version>-windows-x64-portable.zip      desktop app + CLI + licenses
#   rimmerge-cli-<version>-x86_64-pc-windows-msvc.zip  the CLI exe alone
# Shared by .github/workflows/release.yml and a maintainer's local check,
# so a local run produces the same layout a release ships (not
# byte-identical zips: file timestamps and compression differ per run).
#
# Prerequisites (this script builds nothing):
#   cargo build --release -p rimmerge-cli
#   (cd apps/desktop && bun run tauri build)    # bundling is off: it only emits the exe
#
# Usage:
#   pwsh ./scripts/package-portable.ps1 -Version 1.0.0 [-OutDir <dir>] [-TargetDir <dir>]
#   -OutDir defaults to the repo root, -TargetDir to <repo>/target/release.
#
# Layout of the portable zip (one top-level folder):
#   Rimmerge-<version>-windows-x64-portable/
#     Rimmerge.exe       the desktop app (target/release/rimmerge-desktop.exe, renamed)
#     cli/rimmerge.exe   the CLI (own folder: Windows file names are case-insensitive,
#                        so it cannot sit beside Rimmerge.exe)
#     README.txt         how to run, where data lives, how to uninstall
#     LICENSE-MIT, LICENSE-APACHE
#
# Fails if the desktop exe's embedded ProductVersion differs from -Version,
# and removes any partial zip on failure. Prints the two zip paths. The
# release's SHA256SUMS.txt is written by the workflow, not here.

param(
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [string]$OutDir,
    [string]$TargetDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = $repoRoot }
if (-not $TargetDir) { $TargetDir = Join-Path $repoRoot 'target/release' }

$desktopExe = Join-Path $TargetDir 'rimmerge-desktop.exe'
$cliExe = Join-Path $TargetDir 'rimmerge.exe'
foreach ($required in @($desktopExe, $cliExe)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "missing $required - build it first (see this script's header)"
    }
}

$embeddedVersion = (Get-Item -LiteralPath $desktopExe).VersionInfo.ProductVersion
if ($embeddedVersion -ne $Version) {
    throw "$desktopExe carries ProductVersion '$embeddedVersion', not '$Version' - rebuild it or fix -Version"
}

$name = "Rimmerge-$Version-windows-x64-portable"
$cliName = "rimmerge-cli-$Version-x86_64-pc-windows-msvc"
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("rimmerge-portable-" + [guid]::NewGuid().ToString('N'))
$folder = Join-Path $stage $name
New-Item -ItemType Directory -Path $folder | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$resolvedOut = (Resolve-Path -LiteralPath $OutDir).Path
$zip = Join-Path $resolvedOut "$name.zip"
$cliZip = Join-Path $resolvedOut "$cliName.zip"

$readme = @'
Rimmerge - portable build

Run: double-click Rimmerge.exe (the desktop app), or run cli\rimmerge.exe from a
terminal (the command-line tool). Extract this folder anywhere you like.
To use the CLI from any terminal, add this folder's cli subfolder to your PATH.
The extracted folder is named after the version, so either rename it to
"Rimmerge" first or update the PATH entry each time you upgrade.

Needs the Microsoft Edge WebView2 runtime. It is usually preinstalled on
Windows 10 and 11, but not on every LTSC, N or unserviced build. If
Rimmerge.exe does not open a window, install the "Evergreen" runtime from
https://developer.microsoft.com/microsoft-edge/webview2/

These files are not code-signed, so Windows SmartScreen may warn on first
run ("More info", then "Run anyway"). To verify a download, compare the zip's
SHA-256 (PowerShell: (Get-FileHash .\<zip>).Hash.ToLower()) with the line in
SHA256SUMS.txt on the release page.

Rimmerge writes nothing to this folder, the registry or Program Files. Your
data lives in two places under %LOCALAPPDATA%:
  %LOCALAPPDATA%\rimmerge          settings, profiles, decisions, rule databases
  %LOCALAPPDATA%\dev.rimmerge.app  the WebView2 cache and the app's display
                                   preferences (language, mod-name display)
Moving or updating this folder keeps both. To update, extract a newer zip
beside or over this one.

To uninstall, delete this folder. To remove your data as well, also delete
both folders above.

Documentation: https://github.com/Rimmerge-Project/rimmerge
Licensed under MIT OR Apache-2.0 (see LICENSE-MIT and LICENSE-APACHE).
'@

try {
    Copy-Item -LiteralPath $desktopExe -Destination (Join-Path $folder 'Rimmerge.exe')
    New-Item -ItemType Directory -Path (Join-Path $folder 'cli') | Out-Null
    Copy-Item -LiteralPath $cliExe -Destination (Join-Path $folder 'cli/rimmerge.exe')
    foreach ($license in @('LICENSE-MIT', 'LICENSE-APACHE')) {
        Copy-Item -LiteralPath (Join-Path $repoRoot $license) -Destination $folder
    }
    Set-Content -LiteralPath (Join-Path $folder 'README.txt') -Value ($readme -replace "`r?`n", "`r`n") -Encoding utf8

    foreach ($stale in @($zip, $cliZip)) {
        if (Test-Path -LiteralPath $stale) { Remove-Item -LiteralPath $stale }
    }
    Compress-Archive -LiteralPath $folder -DestinationPath $zip
    Compress-Archive -LiteralPath $cliExe -DestinationPath $cliZip
    Write-Output $zip
    Write-Output $cliZip
}
catch {
    foreach ($partial in @($zip, $cliZip)) {
        if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
    }
    throw
}
finally {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
