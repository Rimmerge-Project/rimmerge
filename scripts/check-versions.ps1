# Fails when the version number isn't the same everywhere it's spelled
# out by hand. A Tauri desktop build has four independent places that
# each carry their own copy of "the version" (every workspace crate's
# own `[package] version`, the root workspace's own path-dependency
# pins on the internal crates, `apps/desktop/src-tauri/tauri.conf.json`,
# and `apps/desktop/package.json`) and nothing enforces they agree —
# `cargo build` doesn't care that a path dependency's declared
# `version` matches the crate it points at, and Tauri doesn't check its
# own `tauri.conf.json` against `Cargo.toml` either. A mismatched
# release is the first symptom anyone notices, well after the tag is
# pushed — this is the hermetic backstop, run locally before tagging
# and again in `release.yml` against the tag itself.
#
# Usage:
#   pwsh ./scripts/check-versions.ps1              # everything must agree with everything else
#   pwsh ./scripts/check-versions.ps1 -Tag v0.2.0   # also require that exact version (tag's leading 'v' stripped)
#
# Exits 0 when every source agrees (and matches -Tag, when given) and
# throws (non-zero) otherwise, so it composes with `&&` and with CI's
# exit-code check.

param(
    [string]$Tag
)

# Every crate/app Cargo.toml under this workspace, discovered from the
# workspace's own `members` list rather than hardcoded here — the same
# "one authoritative list" reasoning as CLAUDE.md's DRY rule: a new
# member added to `[workspace] members` is automatically covered by
# this check without a second edit.
$rootCargoToml = Get-Content Cargo.toml -Raw
if ($rootCargoToml -notmatch '(?s)members\s*=\s*\[(.*?)\]') {
    throw "could not find [workspace] members in Cargo.toml"
}
$members = [regex]::Matches($Matches[1], '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }

function Get-PackageVersion([string]$cargoTomlPath) {
    $content = Get-Content $cargoTomlPath -Raw
    # `[package]` always appears before `[dependencies]`/`[lib]`/etc. in
    # every Cargo.toml this workspace writes, so the first top-level
    # `version = "..."` line is the package's own version, not a
    # dependency's.
    if ($content -notmatch '(?m)^\[package\]\s*$[\s\S]*?^version\s*=\s*"([^"]+)"') {
        throw "no [package] version found in $cargoTomlPath"
    }
    return $Matches[1]
}

$versions = [ordered]@{}

foreach ($member in $members) {
    $cargoTomlPath = Join-Path $member "Cargo.toml"
    $versions["$cargoTomlPath [package]"] = Get-PackageVersion $cargoTomlPath
}

# The root workspace pins each internal crate's path dependency with its
# own `version` (see Cargo.toml's own comment on why: a bare `path` dep
# has no version at all, which cargo-deny's `bans.wildcards` flags) —
# that pin has to track the crate's own version by hand, so it's exactly
# as prone to drift as any of the files above and gets the same check.
foreach ($match in [regex]::Matches($rootCargoToml, '(?m)^(rim-\w+)\s*=\s*\{[^}]*\bpath\s*=\s*"[^"]+"[^}]*\bversion\s*=\s*"([^"]+)"')) {
    $crateName = $match.Groups[1].Value
    $pinnedVersion = $match.Groups[2].Value
    $versions["Cargo.toml [workspace.dependencies] $crateName pin"] = $pinnedVersion
}

$tauriConfPath = "apps/desktop/src-tauri/tauri.conf.json"
$tauriConf = Get-Content $tauriConfPath -Raw | ConvertFrom-Json
$versions[$tauriConfPath] = $tauriConf.version

$packageJsonPath = "apps/desktop/package.json"
$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$versions[$packageJsonPath] = $packageJson.version

# `@(...)` forces array semantics: PowerShell collapses a one-element
# pipeline result to a bare scalar string otherwise, and indexing that
# scalar with `[0]` below would silently return its first *character*
# ("0" of "0.1.0") instead of the version string — exactly the single-
# element case this script hits every time every source already agrees.
$distinctVersions = @($versions.Values | Select-Object -Unique)

if ($distinctVersions.Count -gt 1) {
    Write-Host "check-versions: mismatched versions found:"
    foreach ($key in $versions.Keys) {
        Write-Host "  $key -> $($versions[$key])"
    }
    throw "version mismatch across the tree"
}

$agreedVersion = $distinctVersions[0]

if ($Tag) {
    $tagVersion = $Tag -replace '^v', ''
    if ($tagVersion -ne $agreedVersion) {
        throw "tag '$Tag' (version '$tagVersion') does not match the tree's version '$agreedVersion'"
    }
}

$sourceCount = $versions.Count
$tagNote = if ($Tag) { " (matches tag $Tag)" } else { "" }
Write-Host "check-versions: clean, $sourceCount source(s) agree on $agreedVersion$tagNote"

# Exposes the agreed version to a GitHub Actions job that runs this
# script as a step, so a later job (`release.yml`'s `cli`/`desktop`) can
# name its own build artifacts from the one already-validated version
# instead of re-deriving it from `github.ref_name` — which is a branch
# name, not necessarily a version, on a `workflow_dispatch` run. A
# plain local run has no `$env:GITHUB_OUTPUT`, so this is a no-op
# outside Actions.
if ($env:GITHUB_OUTPUT) {
    "version=$agreedVersion" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}

exit 0
