# Fails when a relative Markdown link or image in the public tree points
# at a file that doesn't exist. A doc move/rename that forgets to update
# its citations reads fine in a Markdown preview (the text is still
# there) and fails silently for every reader who actually clicks it —
# this is the hermetic backstop, the same relationship
# check-forbidden.ps1 has to real forbidden tokens.
#
# Scope: every `.md` file a contributor or a reader actually sees —
# `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, every `CLAUDE.md`, and
# everything under `docs/`, `apps/`, `crates/` — excluding build output
# and anything that never ships (`target/`, `node_modules/`, `.journal/`,
# `.claude/`, `rules/` — the `rimmerge-rules` submodule, a separate repo
# with its own tooling — and `dist/`).
#
# Only *relative* links/images are checked (an absolute `http(s)://`,
# `mailto:`, or bare `#anchor`-in-the-same-file link is out of scope: the
# first two need a real network fetch to verify and the fetch itself is
# what CLAUDE.md's no-network rule forbids in this workspace; the third
# is handled by the anchor check below without needing its own case). A
# link with a `#fragment` (`concepts/sorting.md#tiers`) is split on
# first `#`; the file part is resolved the same way and, when the target
# file was itself read as part of this scan (cheap — no extra file I/O),
# the fragment is checked against that file's own headings, slugified
# the way GitHub does (lowercase, spaces to `-`, strip everything but
# word chars/`-`/`_`). A fragment into a file outside the scanned set
# (e.g. into `apps/desktop/README.md` from a file elsewhere) is not
# checked — flagged in this header rather than silently wrong: cheap
# means "reuse content already read", not "read every file's headings
# twice over."
#
# Exits 0 when clean and throws (non-zero) when a broken link is found,
# so it composes with `&&` and with CI's exit-code check.

$scanRoots = @(
    'README.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CLAUDE.md',
    'docs', 'apps', 'crates'
) | Where-Object { Test-Path $_ }

$mdFiles = rg --files -g '*.md' `
    --glob '!target/**' `
    --glob '!**/node_modules/**' `
    --glob '!.journal/**' `
    --glob '!.claude/**' `
    --glob '!rules/**' `
    --glob '!dist/**' `
    @scanRoots
# rg's own exit codes: 0 = files listed, 1 = none found, 2+ = rg itself
# failed (a bad glob, an unreadable root). `--files` legitimately exits
# 1 on an empty result, so that alone isn't a failure here — but this
# script always has plenty of real Markdown to scan, so an empty result
# is treated the same as a hard rg failure below (a broken `$scanRoots`
# entry silently reading as "zero broken links because zero files" is
# exactly the false-clean this whole script exists to prevent).
if ($LASTEXITCODE -ge 2) {
    throw "rg failed while listing Markdown files (exit $LASTEXITCODE)"
}
if ($mdFiles.Count -eq 0) {
    throw "check-doc-links found 0 Markdown files to scan — scan root broken or filter too aggressive, not actually clean"
}

# Matches both `[text](target)` and `![alt](target)` inline links —
# the leading `!` doesn't change how a target resolves, so one pattern
# covers both. Reference-style (`[text][ref]`) links don't appear
# anywhere in this tree (confirmed by a `rg '\]\('` scan before writing
# this script), so they're out of scope rather than silently mishandled.
$linkPattern = '\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)'

function Get-HeadingSlugs([string[]]$lines) {
    $slugs = [System.Collections.Generic.HashSet[string]]::new()
    # GitHub's slugifier disambiguates a repeated heading by appending
    # `-1`, `-2`, ... in order of appearance — a link to the *second*
    # "## Overview" on a page really does resolve to `#overview-1`, not
    # `#overview`. Tracked per base slug so a duplicate heading's real
    # anchor is recognized instead of silently colliding with the first
    # one's entry in the set.
    $seenCounts = @{}
    foreach ($line in $lines) {
        if ($line -notmatch '^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$') { continue }
        $text = $Matches[1]
        # Strip Markdown delimiters while keeping their content — a
        # backtick/asterisk/underscore *pair* is markup, not heading
        # text, but GitHub slugifies the rendered heading, not the raw
        # source, so a literal underscore inside that content (as in
        # `` `xpath_expr` ``) survives: only `_..._`/`__..__` acting as
        # an emphasis delimiter (word-boundary-anchored) is stripped, a
        # bare mid-word underscore is not touched at all.
        $text = $text -replace '\[([^\]]*)\]\([^)]*\)', '$1'
        $text = $text -replace '`([^`]*)`', '$1'
        $text = $text -replace '\*\*([^*]+)\*\*', '$1'
        $text = $text -replace '\*([^*]+)\*', '$1'
        $text = $text -replace '__([^_]+)__', '$1'
        $text = $text -replace '(?<!\w)_([^_]+)_(?!\w)', '$1'
        $baseSlug = $text.ToLowerInvariant() -replace '[^\w\- ]', '' -replace '\s+', '-'

        if ($seenCounts.ContainsKey($baseSlug)) {
            $seenCounts[$baseSlug]++
            $slug = "$baseSlug-$($seenCounts[$baseSlug])"
        } else {
            $seenCounts[$baseSlug] = 0
            $slug = $baseSlug
        }
        [void]$slugs.Add($slug)
    }
    return $slugs
}

# Cache each scanned file's lines and heading slugs once, keyed by full
# path — link targets are resolved against these same files far more
# often than headings are checked, so this doubles as the "already read"
# set the header comment promises for the anchor check.
$fileLines = @{}
$headingSlugs = @{}
foreach ($file in $mdFiles) {
    $full = (Resolve-Path $file).Path
    $lines = Get-Content -LiteralPath $full
    $fileLines[$full] = $lines
    $headingSlugs[$full] = Get-HeadingSlugs $lines
}

$brokenLinks = New-Object System.Collections.Generic.List[string]
$skippedAnchors = New-Object System.Collections.Generic.List[string]

foreach ($file in $mdFiles) {
    $full = (Resolve-Path $file).Path
    $dir = Split-Path $full -Parent
    $lineNum = 0
    foreach ($line in $fileLines[$full]) {
        $lineNum++
        foreach ($match in [regex]::Matches($line, $linkPattern)) {
            $target = $match.Groups[1].Value

            # Out of scope by design (see header comment): absolute
            # URLs, mailto, and bare same-file anchors need no file
            # resolution at all.
            if ($target -match '^[a-zA-Z][a-zA-Z0-9+.\-]*:') { continue }
            if ($target.StartsWith('#')) {
                $slug = $target.Substring(1)
                if (-not $headingSlugs[$full].Contains($slug)) {
                    $brokenLinks.Add("${file}:${lineNum}: #$slug has no matching heading in $file")
                }
                continue
            }

            $anchor = $null
            $pathPart = $target
            $hashIndex = $target.IndexOf('#')
            if ($hashIndex -ge 0) {
                $pathPart = $target.Substring(0, $hashIndex)
                $anchor = $target.Substring($hashIndex + 1)
            }

            $resolved = Join-Path $dir $pathPart
            if (-not (Test-Path -LiteralPath $resolved)) {
                $brokenLinks.Add("${file}:${lineNum}: ${target} -> $pathPart does not exist")
                continue
            }

            if ($null -eq $anchor -or $anchor -eq '') { continue }

            $resolvedFull = (Resolve-Path -LiteralPath $resolved).Path
            if ($headingSlugs.ContainsKey($resolvedFull)) {
                if (-not $headingSlugs[$resolvedFull].Contains($anchor)) {
                    $brokenLinks.Add("${file}:${lineNum}: ${target} -> no heading '#$anchor' in $pathPart")
                }
            } else {
                # Target file exists but wasn't itself part of this
                # scan's Markdown set (e.g. a link into a file the scope
                # excludes) — the file-existence check above already
                # covers the target, so this is a known gap, not a
                # silent pass.
                $skippedAnchors.Add("${file}:${lineNum}: ${target} (anchor not checked, target file outside scan)")
            }
        }
    }
}

if ($skippedAnchors.Count -gt 0) {
    Write-Host "check-doc-links: anchors skipped (target file outside scanned set):"
    $skippedAnchors | ForEach-Object { Write-Host "  $_" }
}

if ($brokenLinks.Count -gt 0) {
    $brokenLinks | ForEach-Object { Write-Host $_ }
    throw "broken doc links found"
}

Write-Host "check-doc-links: clean ($($mdFiles.Count) files scanned)"
exit 0
