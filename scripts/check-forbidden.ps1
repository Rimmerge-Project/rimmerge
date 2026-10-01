# Fails when a forbidden token appears anywhere in the tracked tree: a
# local path, a Steam library layout, a profile hash, or (when a
# maintainer keeps one) a pattern from the local list. CI runs the same
# script with the public list only.
#
# Lists: `.github/forbidden-tokens.txt` (public, shape-only) and, when
# present, `.journal/forbidden-tokens.local.txt` (maintainer-local,
# gitignored). Lines starting with `#` in either file are ignored.
#
# Excluded from the scan: the public list itself, build output
# (`target/`, `node_modules/`), local-only material that is never
# committed (`.journal/`, `.claude/`, `CLAUDE.local.md`), and `rules/` —
# a separate repository (the `rimmerge-rules` git submodule), which
# names real mods by design and is scanned by its own tooling, not this
# one.
#
# Exits 0 when clean and throws (non-zero) when a token is found, so it
# composes with `&&` and with CI's exit-code check.

$localTokenFile = ".journal/forbidden-tokens.local.txt"
$usingLocalList = Test-Path $localTokenFile

# Pattern files use '#'-prefixed header comments for humans, but `rg -f`
# treats every non-blank line as a literal pattern — so comments and
# blank lines are stripped here before either list reaches rg, not left
# for rg to (mis)interpret.
function Get-PatternLines([string]$path) {
    Get-Content $path | Where-Object { $_ -notmatch '^\s*#' -and $_.Trim() -ne '' }
}

$patterns = Get-PatternLines ".github/forbidden-tokens.txt"
if ($usingLocalList) {
    $patterns += Get-PatternLines $localTokenFile
}

$mergedTokenFile = Join-Path $env:TEMP "rimmerge-forbidden-tokens-merged.txt"
try {
    Set-Content -Path $mergedTokenFile -Value $patterns -Encoding utf8

    $hits = rg -i -f $mergedTokenFile `
        --glob '!.github/forbidden-tokens.txt' `
        --glob '!target/**' `
        --glob '!**/node_modules/**' `
        --glob '!.journal/**' `
        --glob '!.claude/**' `
        --glob '!CLAUDE.local.md' `
        --glob '!rules/**' `
        .
    # rg's own exit codes: 0 = matches found, 1 = no matches (the
    # expected clean case, checked below via `$hits`), 2+ = rg itself
    # failed (a malformed pattern in either token file, a bad glob, a
    # permissions error). Without this check a broken token file reads
    # as "no matches" — `$hits` is empty either way — and this script
    # would report clean while never having actually scanned anything.
    if ($LASTEXITCODE -ge 2) {
        throw "rg failed while scanning for forbidden tokens (exit $LASTEXITCODE) — check for a malformed pattern in .github/forbidden-tokens.txt or the local list"
    }

    if ($hits) {
        $hits
        throw "forbidden tokens found"
    }
} finally {
    Remove-Item -Path $mergedTokenFile -ErrorAction SilentlyContinue
}

$listsUsed = if ($usingLocalList) { "public + local" } else { "public only" }
Write-Host "check-forbidden: clean ($listsUsed)"
# rg exits 1 when it finds nothing; without this the caller's $LASTEXITCODE
# (which GitHub's pwsh steps check) would report a clean run as a failure.
exit 0
