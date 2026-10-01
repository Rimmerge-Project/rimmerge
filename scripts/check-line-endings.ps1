# Fails on any CRLF text file under the trees a contributor actually
# edits by hand. A scripted edit on Windows (Python's `Path.write_text`
# with the platform-default newline handling, for one) silently writes
# CRLF, and neither `cargo fmt` (normalizes `.rs` back to LF on every
# run regardless of what's on disk) nor `bun run lint` (Biome polices
# `.ts`/`.vue` formatting but not raw line endings) catches that outside
# `apps/desktop/src`/`src-tauri` — a crate/app `CLAUDE.md`, a
# `README.md`, or a `scripts/*.ps1` file. `.gitattributes`' own
# `* text=auto eol=lf` normalizes line endings at commit time; this
# script is the hermetic backstop that also checks the working tree, the
# same relationship `check-forbidden.ps1` has to
# `.github/forbidden-tokens.txt`.
#
# Scope mirrors check-forbidden.ps1: everything a contributor hand-edits,
# excluding target/node_modules/.journal/.claude and every binary format
# (images, DLLs) and the one file that is expected to carry whatever
# line ending proptest itself wrote (`sort_proptest.proptest-regressions`
# — a generated seed file, never hand-edited, not ours to reformat).

$paths = @(
    'crates', 'apps/cli', 'apps/desktop/src', 'apps/desktop/src-tauri',
    'apps/desktop/e2e', 'scripts', '.github', 'docs', 'README.md', 'CLAUDE.md',
    'CONTRIBUTING.md', 'SECURITY.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'Cargo.toml',
    'deny.toml', 'clippy.toml', 'rust-toolchain.toml', '.gitignore', '.gitattributes',
    '.editorconfig',
    'apps/desktop/CLAUDE.md',
    # apps/desktop's own root-level, hand-edited files — every one of
    # these is a config/manifest a contributor edits directly (not
    # generated, not vendored), so it's exactly as in-scope as
    # `apps/desktop/src`/`src-tauri` above; only `bun.lock` (generated)
    # and the build/output dirs (`dist`, `test-results`, `node_modules`)
    # are deliberately left out.
    'apps/desktop/README.md', 'apps/desktop/package.json',
    'apps/desktop/tsconfig.json', 'apps/desktop/tsconfig.app.json',
    'apps/desktop/tsconfig.node.json', 'apps/desktop/tsconfig.vitest.json',
    'apps/desktop/vite.config.ts', 'apps/desktop/vitest.config.ts',
    'apps/desktop/eslint.config.js', 'apps/desktop/biome.json',
    'apps/desktop/index.html', 'apps/desktop/scripts'
) | Where-Object { Test-Path $_ }

$hits = rg -l -U '\r' @paths `
    --glob '!**/node_modules/**' `
    --glob '!**/*.png' `
    --glob '!**/*.jpg' `
    --glob '!**/*.jpeg' `
    --glob '!**/*.dll' `
    --glob '!**/*.ico' `
    --glob '!**/*.proptest-regressions'
# rg's own exit codes: 0 = matches (CRLF found, handled below), 1 = no
# matches (the expected clean case), 2+ = rg itself failed (a bad glob,
# a permissions error, an unreadable path in $paths). Without this, a
# broken invocation reads exactly like "no CRLF anywhere" — both leave
# $hits empty — and this script would report clean without having
# scanned anything.
if ($LASTEXITCODE -ge 2) {
    throw "rg failed while scanning for CRLF line endings (exit $LASTEXITCODE)"
}

if ($hits) {
    $hits
    throw "CRLF line endings found (expected LF everywhere in scope)"
}

Write-Host "check-line-endings: clean"
# rg exits 1 when it finds nothing; without this the caller's $LASTEXITCODE
# (which GitHub's pwsh steps check) would report a clean run as a failure.
exit 0
