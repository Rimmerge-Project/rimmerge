# Runs `cargo doc --workspace --no-deps` and counts individual
# `warning:` lines against a fixed budget — the counting half of
# ci.yml's rustdoc-warning-budget gate, pulled out into its own script
# so it's testable locally without a real `cargo doc` run (see
# `-InputFile` below) and so the counting logic isn't duplicated between
# a local run and the CI step.
#
# **Why `$env:CARGO_TERM_COLOR` is forced to `never` here, not inherited
# from the caller's environment:** this workspace's other CI steps run
# with `CARGO_TERM_COLOR: always` (ci.yml's workflow-level `env:`, kept
# for readable logs on every other step), and cargo honors that by
# ANSI-coloring `warning:` too — `warning:` becomes
# `\x1b[1m\x1b[33mwarning\x1b[0m:` or similar, so a plain `^warning:`
# regex against that inherited setting matches nothing and the budget
# check silently passes no matter how many warnings exist. Setting it to
# `never` for this one invocation (not the whole job) fixes that without
# touching the readability of every other step's own output. The ANSI
# strip below is a second, independent backstop in case any warning
# line still carries escape codes from a source this script doesn't
# control (e.g. a future cargo version's own color heuristics).
#
# Usage:
#   pwsh ./scripts/count-doc-warnings.ps1                      # runs cargo doc for real
#   pwsh ./scripts/count-doc-warnings.ps1 -Budget 300           # override the budget
#   pwsh ./scripts/count-doc-warnings.ps1 -InputFile out.txt    # count captured output instead of running cargo (testing)
#
# Exits 0 when the count is at or under budget, throws (non-zero)
# otherwise.

param(
    [int]$Budget = 270,
    [string]$InputFile
)

# Strips SGR ANSI escape sequences (`\x1b[...m`) — defensive, on top of
# forcing `CARGO_TERM_COLOR=never` below, rather than instead of it: a
# color override can be overridden again by whatever calls this script,
# but a line that's already plain text can't un-match a regex.
function Strip-Ansi([string]$text) {
    return $text -replace "`e\[[0-9;]*m", ""
}

if ($InputFile) {
    $lines = Get-Content -LiteralPath $InputFile
} else {
    $previousColor = $env:CARGO_TERM_COLOR
    $env:CARGO_TERM_COLOR = "never"
    try {
        $lines = cargo doc --workspace --no-deps 2>&1
    } finally {
        $env:CARGO_TERM_COLOR = $previousColor
    }
}

$warningCount = 0
foreach ($rawLine in $lines) {
    $line = Strip-Ansi([string]$rawLine)
    if ($line -notmatch '^warning:') { continue }
    # Each crate's own trailing "generated N warnings" line also starts
    # with `warning:` but is a summary, not a distinct diagnostic — the
    # same exclusion the budget check has always used.
    if ($line -match 'generated \d+ warnings?$') { continue }
    $warningCount++
}

Write-Host "count-doc-warnings: $warningCount warning(s) (budget: $Budget)"
if ($warningCount -gt $Budget) {
    throw "cargo doc warning count ($warningCount) exceeds budget ($Budget)"
}

exit 0
