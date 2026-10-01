# Testing (for contributors)

This page is for anyone building or reviewing changes to Rimmerge
itself, not for someone just running the app. See the root `CLAUDE.md`
for the exact commands; this page explains the two things that command
list assumes you already understand: the real-install test tier, and
the synthetic fixture.

## Two tiers

**The default tier** (`cargo fmt`/`clippy`/`nextest run --workspace`/
`cargo deny`, the desktop `bun run` gates) runs everywhere, on any
machine, with no RimWorld install and no network access. It's what CI
runs, and it's what "the tests pass" means for a normal contribution.
It includes a committed, synthetic golden fixture (below) exercising
the sorter and ledger against realistic, but entirely invented, data.

**The real-install tier** is a second, much larger set of `#[ignore]`d
tests that read an actual RimWorld install with its actual (large,
personal) mod list. These exist because some bugs only show up at real
scale, against real, messy third-party content — but they can never run
in CI, can never assume any particular mod is installed, and must never
leak a real mod's name into a committed assertion.

## The three-state rule

A real-install test is never allowed to silently report nothing as a
pass. Every real-install test guard checks `RIMMERGE_GAME_DIR` and
behaves one of three ways:

1. **Unset** — the machine isn't in this tier. The test prints an
   honest `skipping: ...` line and asserts nothing.
2. **Set, but not a real install** (no `Version.txt`/`Data/Core/` at
   that path) — the test **panics**. A typo that silently skipped the
   whole tier would be indistinguishable from a real pass, which is
   exactly the failure mode this rule exists to prevent.
3. **Set correctly** — the tier actually runs, against the real data.

A second guard sits behind the first: every real-install test also
requires a floor of active mods (200) before it proceeds, so a
vanilla or near-empty `ModsConfig.xml` fails loudly instead of passing
every cross-mod assertion vacuously.

Never run the two real-install tier commands concurrently against the
same install — several of these tests assert against a timing budget
(a sort finishing under 500 ms, a handful of commands under 2 s), and
two release-profile test binaries competing for the same CPU pushes
those budgets into spurious failures that have nothing to do with a
real regression. Run one tier to completion before starting the other.

`crates/rim-io/tests/real_game_log.rs` is a third, standalone
real-install test with its own command, never folded into the two
above: `-p rim-io` also carries `tests/real_network_databases.rs` and
`tests/real_network_release_feed.rs`, both of which hit the real
network and must stay out of every offline tier command. It reads its
own `RIMMERGE_REAL_GAME_LOG` env var (a log file path, unrelated to
`RIMMERGE_GAME_DIR`) and follows the same three-state rule on its own.
It reads the file through the product's `FileGameLogReader` (kind
detected by content, formats from the embedded rules bundle) and always
asserts what holds on any real log: parsing twice gives an identical
result, every stack-trace block pairs with a terse failure (no
`extra_stack_traces`), every terse failure has its `file:` line, and no
stack trace's leaf is an enclosing wrapper's `Error in ...` line. Its
exact counts are pins (see the list below). Run it with:

```sh
cargo nextest run -p rim-io --all-features --release --run-ignored ignored-only -E 'binary(real_game_log)'
```

The same binary holds a second, directory-wide test driven by
`RIMMERGE_REAL_GAME_LOG_DIR` (a folder of real `Player.log`s and console
copies, searched recursively for `.log`/`.txt` files; the same command
runs it). It follows the same three-state rule: **unset** prints a
`skipping:` line; **set to something that is not a directory**, or to a
directory with no `.log`/`.txt` file, **panics**; **set to a directory**
parses every file under it twice: through `FileGameLogReader` (the
product's path, which detects the kind by content and must agree with an
independent oracle and with `parse_as` under the detected framing), and,
for the other framing, through `parse_as` on the file read as bytes and
decoded lossily. The oracle never uses the product's detection rule: a
file holding a copy trace-start line anywhere must be detected as a
console snapshot, and a file whose first line starts with Unity's
`Mono path[0] = ` as a `Player.log` (a file with neither, a hand-written
note, is only parsed and conserved, and listed as undecided). The snapshot
checks below run for every file the reader detected as a snapshot, not for
those the tier itself guessed to be one. Per file it asserts that the
number of logging gaps equals the number of `Reached max messages limit`
lines, P1 conservation (every line is either in exactly one entry or a
blank separator) and P2 (no loose-sentinel leak in the file's own
framing; every file's leaks are collected and reported together), that
parsing twice gives an equal result, and that the other framing still
conserves its lines. For every snapshot it asserts that the coverage
entry count equals the entries parsed and that `console_fill` matches
that count against 1,000; for one holding any
`UnityEngine.StackTraceUtility:ExtractStackTrace ()` trace-start line
(a real console copy), also at most 1,000 entries (the engine's console
cap), equal to its count of those lines. It prints
the totals (Player.logs, snapshots, the most entries, how many are at the
cap, how many are `head_truncated`, how many the oracle left undecided),
each file's line, entry and family counts, its coverage line (kind, passes,
end state, gap count) and its top ten `Unclassified`
families, so new shapes stay visible (`--no-capture` shows them):

```sh
$env:RIMMERGE_REAL_GAME_LOG_DIR = '<a folder of real logs>'
cargo nextest run -p rim-io --all-features --release --run-ignored ignored-only -E 'binary(real_game_log)'
```

`tests/real_network_release_feed.rs` is, likewise, run on its own, not
folded into either of the two documented tier commands above (nor into
`real_game_log`'s own command) — it hits the real network
(`api.github.com`'s `/releases/latest`), the one thing every offline
tier must stay clear of:

```sh
cargo nextest run -p rim-io --all-features --run-ignored ignored-only -E 'binary(real_network_release_feed)'
```

Before this project's first published release, its first test fails
with `NotPublished` (GitHub's own `404` for a repository with no
releases yet) — expected, not a bug.

## What a def shows, against the real install

`apps/desktop/src-tauri/src/def_graphics_real_install.rs` (part of the
desktop real-install command, `-p rimmerge-desktop`) runs the
"what texture does this def show" use cases over a deterministic stride
sample of about 800 of the install's own graphic-bearing defs (things,
pawn kinds, terrain, hair, beards, head and body types, tattoos). It has no
pin variable: it asserts only shape and bands, never a def or mod identity.
Per run it checks that:

- no def fails to inspect (at most 2 %);
- every key a resolved graphic produces passes the reader's own key check
  unchanged, and reading a def's default face never reports a key outside
  the graphic;
- at least half of the resolved defs reach a located image, and at least
  one comes back as built-in, DDS-only, undecodable or missing;
- at most 2 % of located images are unreadable;
- two passes, the second from a cleared session cache, give identical results;
- resolve plus the default read stays under 50 ms median and 250 ms p95
  per def.

It prints the sample, the counts per outcome and the timings
(`--no-capture` shows them). The smoke tier's `def-graphics.spec.ts`
checks the same path through the real UI over a scratch game tree whose
target mod ships a four-direction texture set.

## Large real-install runs may need fewer test threads

On a memory-constrained machine, running either real-install tier
command at nextest's default parallelism can abort mid-run with
`STATUS_STACK_BUFFER_OVERRUN`/`0xc0000409` — several release-profile
test binaries, each holding a full real-install scan in memory, running
concurrently. Add `--test-threads 4` (nextest's own test-parallelism
flag, distinct from `--build-jobs`, which controls compile
parallelism) to either real-install command if you see this; it trades
wall-clock time for headroom.

## The pins file pattern

A real-install test's *exact* assertions — naming a specific mod, a
specific count, a specific pair — only run when a matching environment
variable is set, because a real mod's identity must never appear in a
committed test file. Each maintainer keeps their own values in a small,
gitignored PowerShell script that sets these variables and is
dot-sourced before the real-install run — the path and invocation live
in that maintainer's own gitignored notes, never in a public doc. Unset,
one of two
things happens, and which one is a property of the specific test, not a
single universal rule:

- **A test with a real shape/band fallback** (e.g. "this flagged set is
  non-empty, every member is active, at most N of them, and stable
  across two runs") skips only its own exact-value comparison, with a
  message naming the variable and the shape check it falls back to
  instead — the test still passes, and still asserts something real.
- **A test with no meaningful shape to fall back to** (an exact mod
  pair, an exact def case, an exact three-mod order tail — there's no
  honest weaker claim to make about "some mod pair" or "some def") calls
  a shared `require_pin_var` helper, which **panics**, naming the
  missing variable and the exact rerun command, the moment the machine
  is already confirmed to be in the tier (the same three-state
  reasoning the tier-detection rule above uses, applied per pin instead
  of per tier) — never a silent skip once the machine is known to be
  real. This is the more common shape: most of the pins in a
  maintainer's own file are this kind, so a missing or stale one is a
  loud, specific test failure, not a quietly-thinner pass.

**A third shape exists for a number, not an identity**: some
assertions compare a real install's own finding/edge *counts* against a
band, where the band itself was calibrated against one install and
would be either too loose or too tight on another. These take an
optional pin too, but never panic when it's unset — there's no third
state to close, since the count itself isn't sensitive the way a mod id
is:

- `RIMMERGE_EXPECTED_KENDALL_BASELINE=<inversions>,<positions>`
  (`sort_golden.rs`) — unset, only the structural relationship that
  holds on any install (`PreserveCurrent` never disturbs more than a
  from-scratch `Rebuild` would) is asserted; set, the pinned baseline's
  own `<= base * 3 + 4` headroom is.
- `RIMMERGE_EXPECTED_SORT_BANDS=<name>:<max>,...` (`sort_golden.rs`) —
  per-finding-kind upper bounds (`declaration_questioned`,
  `duplicate_template_name`, `keyed_translation_collision`,
  `sound_override`, `runtime_patch_collision`,
  `undeclared_type_dependency`, `placement_questioned`,
  `placement_ordering_overridden`, `placement_promotes_dependents`).
  Unset, every measured count is still printed (the eprintln candidate
  line for a future pin), just not compared against a ceiling.
- `RIMMERGE_EXPECTED_DB_COUNTS=user:<n>,community:<n>,steam:<n>`
  (`real_install_databases.rs`) — a real `db refresh`/`import` run's own
  rule counts, checked within ±50% once pinned.
- `RIMMERGE_EXPECTED_PAIR_RULE_SPLIT=<min redundant %>,<max novel %>`
  (`real_install_pair_rules.rs`) — how much of an imported pair-rule set
  is redundant with a derived edge versus genuinely novel community
  knowledge.
- `RIMMERGE_EXPECTED_GAME_LOG_COUNTS=patch_failures:<n>,cross_references:<n>,
  dds_failures:<n>,dependency_warnings:<n>,load_events:<n>,load_order:<n>,timers:<n>,
  failures_with_stack_trace:<n>` (`real_game_log.rs`) — one real
  `Player.log`'s own measured counts (`load_events` is the number of
  load blocks; `load_order` is the mod count of the last one, and
  `load_events` keeps a log with no block from passing as
  `load_order:0`; `failures_with_stack_trace` is how many terse failures
  paired with a stack-trace block, a property of the file rather than a
  structural fact). A key outside this list panics, naming the valid
  set, since a typo would otherwise never be asserted. Each metric left
  out falls back to the shape (that count is non-zero); the same file
  parsing identically twice is asserted either way. Paired with `RIMMERGE_REAL_GAME_LOG`,
  the path to that log file — this one follows the ordinary three-state
  rule on its own (see this file's own doc comment for its command),
  since there's no honest way to fall back further than "some log path
  was given" once it's set.

A handful of thresholds stay hardcoded on purpose rather than joining
this list: a genuine structural floor (`>= 200` active mods, `>= 1`
dropped edge) is a fact about *any* real install, not this one's own
history, and a couple of counts (`PatchInjectedNode` violations `<= 2`)
are close enough to a structural floor — "this should basically never
happen" — that a maintainer-specific pin would add ceremony without
adding real protection.

Losing this file is recoverable, if tediously: every panic names its own
variable and rerun command, several pins can be read straight off a
test's own diagnostic `eprintln!` before it panics (an order tail, a
count), and one class needs real investigation with no shortcut (a
per-key merge attribution, a specific contradiction pair) — see a
maintainer's own pins file for which pins in *this* codebase currently
fall into which category.

## The synthetic fixture

`crates/rim-resolve/tests/golden/report.synthetic.json` is a committed
golden fixture produced entirely by the real analyzer pipeline against
an invented install — never real mod data — so it's safe to publish
and never drifts with a real install's own churn. It's generated from
a spec (`crates/rim-resolve/tests/fixtures/synthetic-install.json`) by
`rimmerge fixture gen`, which builds a complete synthetic RimWorld
install (defs, patches, compiled stub assemblies, textures) on disk,
then trimmed by `rimmerge fixture trim` into the smaller committed
shape.

To regenerate it (PowerShell, from the workspace root):

```powershell
$S = "synthetic-install"   # relative, at the workspace root: the report embeds it,
                                     # so it must stay neutral. Never $env:TEMP
                                     # ($env:TEMP embeds your Windows username,
                                     # which the generated report's own
                                     # game_dir/workshop_dir/mods_config fields
                                     # would then carry into the committed file)
cargo run -p rimmerge-cli --release -- fixture gen `
  --spec crates/rim-resolve/tests/fixtures/synthetic-install.json --out $S
cargo run -p rim-analyzer --release -- analyze `
  --game-dir "$S/game" --workshop-dir "$S/workshop/content/294100" `
  --mods-config "$S/game/ModsConfig.xml" --json "$S/report.json"
cargo run -p rimmerge-cli --release -- fixture trim `
  --in "$S/report.json" --out crates/rim-resolve/tests/golden/report.synthetic.json
rg -i -f .github/forbidden-tokens.txt crates/rim-resolve/tests/golden/report.synthetic.json
# must print nothing
cargo nextest run -p rim-resolve --test sort_golden      # writes .snap.new
Remove-Item -Recurse -Force $S                           # the scratch install
```

That `rg` line checks the committed, shape-only public list; maintainers keeping a
project-specific list under `.journal/forbidden-tokens.local.txt` should also
run `./scripts/check-forbidden.ps1`, which merges both automatically.

Then **read** each `tests/snapshots/*.snap.new` file by hand before
renaming it over the matching `.snap` to accept — never accept a
snapshot update blindly.

## Anonymized structural comparison

`crates/rim-resolve/examples/anonymize_report.rs` and `sort_equivalence.rs`
give an independent proof that renaming every identifier in a report
(mod ids, names, authors, def names, ...) to invented tokens can't
change what the sorter produces: `anonymize_report` builds a
deterministic name→token mapping and applies it; `sort_equivalence`
sorts both the real and anonymized reports and checks the anonymized
order is exactly the mapped image of the real one, structurally, edge
by edge. Both are safe to run against your own real report (their
output is gitignored, never committed) and neither ships any data of
its own.
