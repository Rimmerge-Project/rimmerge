# apps/cli (`rimmerge-cli`)

The `rimmerge` command-line binary: a `clap` CLI over the sorter,
ledger, merge generator, and mod-activation use cases. It is the
reference interface for every feature the desktop app also exposes, and
the only interface with a synthetic-install fixture generator and a
real-install test suite attached to it.

See [`docs/cli.md`](../../docs/cli.md) for the full command reference
and [`docs/testing.md`](../../docs/testing.md) for the real-install test
tier and the synthetic-fixture regeneration procedure — this file
covers only what changes about *working in this crate's code*, not what
each command does.

## Layer

A composition root: it wires `rim-io` adapters into `rim-session` use
cases and prints the result. If a command needs a computation, that
computation belongs in `rim-session` or `rim-resolve`, not here — a
`commands/*.rs` module parses args, calls a use case or free function,
and formats the output.

Two disclosed exceptions, both dev/test fixture tooling that writes
files rather than touching a live session or a real install:

- **`fixture trim`** shrinks a full JSON report into a smaller golden
  fixture.
- **`fixture gen --spec <spec.json> --out <dir>`** writes a complete,
  deterministic, synthetic RimWorld install (`Version.txt`, `Data/Core`
  and DLC folders, `Mods/`, a Steam workshop content tree,
  `ModsConfig.xml`) from a committed spec — no network, no real
  install, no real mod data. It copies pre-built assembly blobs from
  `crates/rim-resolve/tests/fixtures/synthetic/assemblies/bin/`
  (`scripts/build-synthetic-assemblies.ps1` builds them by hand; the
  generator never compiles anything at generation time) into generated
  mod folders for the assembly-reference and runtime-patch constructs
  the sorter reasons about. `generate` validates the spec's
  assembly-derived `planted` counts against that committed blob set
  before writing anything, and rejects a spec whose values disagree.

`tests/synthetic_install_e2e.rs` is hermetic and **not** `#[ignore]`d —
it runs the real pipeline (`load`/`sort`/`ledger`/`defs changes`/`merge
coverage`/`verify --json`/`apply --dry-run`) against `fixture gen`'s own
output, so it doubles as this generator's own regression test.

## Invariants

- **The real-install tier is `RIMMERGE_GAME_DIR`-driven and hermetic by
  default.** `tests/common/mod.rs` provides the guard helpers every
  `real_install_*.rs` test in this crate uses. `require_game_dir` is the
  three-state gate: **unset** → an honest `skipping:` line, nothing
  asserted; **set but not a real install** (per
  `rim_analyzer::infra::paths::is_game_dir` — `Version.txt` and
  `Data/Core/` both present) → panic, naming the variable and the rerun
  command; **set correctly** → the tier runs. `require_profile_dir`
  (needs `RIMMERGE_PERF_PROFILE_DIR` too) and `require_pin_var` (any
  other test-specific pin) both call through it and panic on their own
  variable when it's unset **inside** the tier, never a soft skip.
  `real_install_is_present` is a plain two-state boolean query — it
  collapses "unset" and "set but not an install" into one `false` and
  must never be used to gate a test; routing a three-state check through
  it is what let a typo'd path soft-skip a whole tier once already, so
  don't reintroduce that shape. `real_install_paths` also asserts a
  floor of 200 active mods (`MIN_ACTIVE_MODS`) before any test body
  runs, so a vanilla or reset `ModsConfig.xml` fails loudly instead of
  passing every cross-mod assertion vacuously. `cargo nextest run
  --workspace --all-features` (the default gate) opens no socket and
  touches no real install; only the `#[ignore]`d tests in this crate's
  `tests/real_install_*.rs` do, and only when explicitly run with
  `--run-ignored ignored-only`.
- **Every `apps/cli/tests/*.rs` test spawns the binary through
  `common::rimmerge()`**, never a bare `Command::cargo_bin("rimmerge")`:
  the helper points `RIMMERGE_PROFILE_DIR` at a fresh empty directory
  under `CARGO_TARGET_TMPDIR`, so a test never reads the developer's own
  `%LOCALAPPDATA%\rimmerge` (their network switches, a fetched rules
  cache, their profiles). A test that needs a specific base sets
  `RIMMERGE_PROFILE_DIR` itself afterwards (`db_cli.rs`, `network_cli.rs`,
  `config_cli.rs` and `real_install_databases.rs` do, and build their
  commands directly).
- **A default-gate test must never consult the real
  `SysinfoGameProcessProbe`.** Every composition-root command
  (`commands/mods.rs`, `commands/apply.rs`, `commands/patch.rs`,
  `commands/assign/export.rs`) wires `rim_io::SysinfoGameProcessProbe`
  directly, so a `#[cfg(test)]` unit test inside that same file can
  inject a fake probe (a local `FixedProbe`, e.g. `commands/mods.rs`'s
  and `commands/apply.rs`'s own copies — every probe-gated command's own
  file tests its refusal this way, not just one of them), but an
  `apps/cli/tests/*.rs` integration test spawns the compiled binary via
  `assert_cmd` and has no seam to inject anything into. Every such test
  that performs a real, non-`--dry-run` write (`mods activate`/
  `deactivate`, `order import`, `apply`, `patch`/`assign export
  --install`) passes
  `--force` instead — always safe there, since every one of those tests
  writes to a tempdir/scratch copy, never the game's own file (`--force`
  bypasses only the running-game probe, never any other safety check).
- **The real writers against a live install are `apply` (including
  `--write-merge-mod`, which writes the generated merge mod into
  `<game>/Mods`), `mods activate|deactivate`, `order import`, and `patch export
  --install` / `assign export --install` (each copies a mod folder into
  `<game>/Mods` and appends it to `ModsConfig.xml`)** — every one of
  them only on an explicit, non-`--dry-run` invocation. For a manual run
  against the real install, always pass a scratch `--profile-dir` and,
  for any of these, `--dry-run` or a temp copy of the install/config.
  Every one of them refuses to write while `RimWorldWin64.exe` is
  running unless `--force` is passed; the probe
  (`rim_io::GameProcessProbe`) is shared with the desktop app. Agents
  and tests use these only against temp copies; against a real install,
  run them only when a human asks.
- **`mods list|activate|deactivate` carry zero business logic.** Every
  subcommand goes through `rim_io::discover_inventory` — discovery
  only, no defs/patches/assemblies scan, seconds rather than the ~30 s
  `load`/`apply` pay — and the same `rim_session::{ActiveSet::new,
  plan_activate, plan_deactivate}` free functions the session use cases
  call; `commands/mods.rs` only parses args, prints the plan, and writes
  through `ModsConfigStore::write_with_backup`. `mods list --json`
  always emits the full `{active, inactive, missing}` shape regardless
  of `--inactive`/`--all` (those two flags are text-mode only).
  `activate` validates every requested id against the inventory before
  building or printing a plan (an input error); `deactivate`'s
  Core-refusal and dependents-without-`--yes` checks print the full plan
  first and refuse after (real facts about an otherwise-valid plan, not
  input errors) — `--yes` is required whenever a declared dependent
  (from `modDependencies`, since discovery has no report edges to see a
  `Hard`-strength one) is still active, and `--force` only ever bypasses
  the running-game probe. `--dry-run` writes nothing and takes no
  backup; a genuine refusal is a distinct `WriteOutcome` variant from a
  dry run, so the two never share output text. Core is never removable,
  but is not required to load first — a legitimate install can run a
  runtime-patching library or an early-loading hook ahead of Core (such a
  hook hooks the game's own assembly-loading process, which must run
  first), so nothing here enforces load position, only presence.
- **`order export|import` carry zero business logic either**
  (`commands/order.rs`). `export` reads `ModsConfig.xml` and
  `discover_inventory` (no scan), builds an `ExportSource` itself (the CLI
  has no `Session`), and runs `rim_session::use_cases::ExportOrder`: no
  `--out` prints the text format on stdout, `--out` writes an `.rml`
  through `RmlFileStore` and refuses an existing file unless
  `--overwrite` (never touches `ModsConfig.xml`). Ids that cannot be
  written into a list go to stderr as warnings, so stdout stays pasteable.
  `import <file|->` previews through `PreviewOrderImport`/
  `ImportPreview::from_read` (`-` reads stdin through
  `rim_io::read_mod_list_bounded` and parses it with
  `rim_io::parse_mod_list_bytes`, so it gets the same bound, lossy
  decoding and format detection as a file: text or a piped `.rml`) and
  prints the plan, then writes `<activeMods>` = the planned
  order through the same `mods::write_active_set` (backup, file's
  `version`/`knownExpansions` kept) and `mods::refuse_while_running`
  probe as `mods activate`. It is deliberately **not** routed through
  `apply` (no scan, no hard-problem list; it prints the `apply --dry-run
  --source current` hint instead). `--yes` is required exactly when
  `ImportLoss::of(&plan)` is `Some` (it deactivates something, or a
  listed mod other than the sender's merge mod is not installed); the
  plan prints first and the refusal after, as `mods deactivate` does.
  `has_pending_changes` is always `false` here. A plan whose order
  equals the file's writes nothing and takes no backup. A rejected input
  (`Rejection`) is an error exit with a reason; `--dry-run` writes
  nothing. The planned order goes through
  `ImportOrder::validate_order`, the same rule the desktop applies
  (every id installed, none repeated, Core present, and at least one mod
  besides Core and generated mods), after the plan
  prints and before the dry-run exit, `--yes` and the probe: an install
  with no Core on disk refuses the import (error exit, nothing written)
  rather than writing an order without Core. A list that omits Core gets
  it first (`core: not in the list; kept first`). The game version is
  `Option`al in the list; discovery itself still needs `Version.txt`, so
  a missing one fails here exactly as it does for `mods list`.
- **`db_cli.rs`'s disabled-sources seed is load-bearing for the whole
  workspace's hermetic-network gate.** It writes `<base>/app-settings.json`
  as a raw JSON string (`NetworkPolicy`'s own fetch toggles, not
  `rules.json`/`Settings` — the fetch toggles moved app-global), so a new
  `NetworkPolicy` toggle that defaults to enabled is not a compile error
  there — it silently reaches the real fetcher and opens a live
  connection under `cargo nextest run --workspace`. Any new
  rule-database source must be added to that seed (and to the
  refresh/status tests' per-source assertions) before it ships.
- Determinism, `BTreeMap`/`BTreeSet`, and denied `unwrap`/`expect`
  outside tests are workspace-wide (root `CLAUDE.md`) and apply here
  unchanged — nothing in this crate is exempt.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rimmerge-cli --all-targets --all-features -- -D warnings
cargo nextest run -p rimmerge-cli --all-features
cargo deny check
```

Full gate block (all workspace crates plus the desktop app) and the
real-install tier's exact invocation: root `CLAUDE.md`.

## Conventions

- **Never derive `Serialize` on a `rim-session`/`rim-merge` domain type
  for `--json`.** `commands/*.rs` transcribes fields into small, local
  `#[derive(Serialize)]` DTOs instead (`defs`, `merge coverage`, `log`,
  `mods` all follow this) — a CLI flag adds no serde dependency to the
  domain crates.
- **Text-mode output of mod-, report- or log-derived strings goes
  through `common::TerminalSafe`**, the CLI's one terminal-safe output
  boundary: it drops every control character (ESC, DEL, C1 such as
  U+009B) except tab, and `TerminalSafe::line` also drops newlines (use
  `block` only for text that is multi-line by nature, such as a mod
  description or a def's resolved XML). A mod id, name, def name, xpath,
  log line or free-text reason can all carry one, and a terminal may act
  on it. `--json` needs no wrapper (serde escapes), and `main` prints an
  error chain through `TerminalSafe` too, since error text quotes the same
  strings. A padded cell (`{:<40}`) must call `.to_string()` first:
  `TerminalSafe` does not pad. `tests/terminal_safety_cli.rs` runs the
  text-mode commands against a fixture carrying U+009B and ESC and asserts
  no control character leaves the process; a new command that prints
  such text adds a case there.
- **Never trust a domain type's derived `Debug`/bare `Display` to stay a
  stable table cell or `--json` field.** Each command routes through its
  own local `format_*` helper (`format_kind`, `format_status`,
  `format_gate`, `format_role`, `format_row_intent`, `format_row_key`,
  …) instead of `{value:?}` — a `Debug` output is not a contract, and a
  reader may `grep`/`jq` a printed value back into another flag.
- **An unresolvable id is re-worded to include the literal substring
  "not found"** (`defs inspect`, `assign`, `mods`) so a caller can
  `grep` CLI stderr for that string across command families. `patch` is
  the one outlier — its subcommands surface `UnknownPatch`'s own bare
  `Display` (`"no patch project with id {id}"`) or a hand-built `"no
  such patch: {id}"` unreworded. Don't assume the convention holds for
  `patch` without checking; a future pass touching `patch.rs` should
  bring it in line rather than read the inconsistency as intentional.
- **Row-editing flags share one tagged-value spec** across `assign
  set-row`/`copy-from` and `patch decide --choice`: `--value
  <path>=<spec>`, `spec` one of `names:a,b,c`, `numbers:1.0,2.0`,
  `text:...`, or `omit`.
- **A composition root has no business knowing an ordering rule.**
  `assign copy-from`'s named-instance lookup calls
  `rim_session::use_cases::AssignmentInstances::find_by_name` rather
  than recomputing that ordering here — a second copy would risk a
  silent desync from the one `rim-session` actually uses to build and
  evaluate the same set.
- **End-to-end tests run against temp copies of the analyzer's
  `sample_game` and `rim-io`'s `merge_game`/`assign_game` fixtures**,
  never the checked-in files in place. A test that needs a shape the
  fixture doesn't have writes the extra content into its own already-
  copied scratch directory instead of editing the fixture — those
  fixtures are shared with several other crates' own test suites, so a
  change to the checked-in copy has a wide blast radius. `assign_game`'s
  "five of everything" is load-bearing, not padding: it's the minimum
  instance count the reference-field classification gate needs to fire
  at all, so shrinking it silently changes what the suite covers.
- **Real-install `#[ignore]`d tests assert invariants and bands, never
  pin the real, drifting counts themselves** — e.g.
  `real_install_merge_coverage.rs` checks a bucket-partition identity
  and a monotonicity bound, `real_install_assign.rs` compares
  classification results against a real mod's actual field shapes read
  live off disk, never a bundled reference file, and
  `real_install_mods.rs` round-trips a real, active, non-Core mod
  (picked from the middle of the list — Core is not guaranteed to sit at
  index 0) and asserts the result on the **parsed** active-id list, not
  raw bytes (`write_with_backup` always re-renders in its own canonical
  shape, which need not match the game's own formatting). See
  [`docs/testing.md`](../../docs/testing.md) for the pins-file pattern
  that makes an *exact* real-install assertion possible without ever
  committing a real mod's identity.
- **An integration test can't reach the binary's own private session
  builder.** `tests/real_install_defs.rs` (and its siblings) build a
  small `Session` directly against `rim-session`/`rim-io` rather than
  reusing `common::build_session`, which is private to the binary crate
  and unreachable from a separate `tests/*.rs` compilation unit — accept
  the small duplication rather than making that builder `pub`.
- **`verify`'s printed fixes are single-quote–shell-safe by
  construction.** Every untrusted value it interpolates (a package id,
  a rule rationale) goes through `commands/verify.rs`'s own
  `shell_quote` helper, which wraps the value in single quotes —
  literal in both POSIX `sh` and PowerShell — and returns `None` for a
  value with no safe spelling in either shell (a literal single quote or
  control character), in which case the fix falls back to prose and
  `--json`'s `set_pair_command` is `null`. `shell_quote` lives only
  here; the desktop's own reorder-fix rendering (`dto/verify.rs`) is a
  separate concern (it never builds a shell command) and holds no copy
  of it.
- **`rule set-placement --mod-id`/`set-pair --after`/`--before` are
  validated against the session's own scan before the rule is saved**
  (`commands/rule.rs`'s `mod_is_active`, matched through the `_steam`
  suffix the same way the sorter later matches a stored rule against a
  mod's tier) — a typo'd id fails fast, before `rules.json` is ever
  written, instead of silently persisting a placement or pair that
  matches nothing.
- **`assign set-row`/`copy-from`'s `--target`/`--key-field` are
  `requires`-paired by clap** (each needs the other), and the key field
  is never inferred from the target — a schema can carry more than one
  `TargetKey` field of the same granularity, so which one a given
  `--target` resolves through always has to be named explicitly.
- **`assign items --assignment`'s "own" status comes from
  `Section::own_instance_names`, never from comparing an item's own
  owner id against the project's package id** — an active def's owner
  is incidentally never the project's own id, but that coincidence
  isn't the actual contract.
- **`rimmerge config`'s `--base` exists for tests only**, not a
  general-purpose knob — every other command derives the same base from
  `%LOCALAPPDATA%`/`RIMMERGE_PROFILE_DIR`, and `default_profile_base`
  honours `RIMMERGE_PROFILE_DIR` the same way the desktop's own
  `profile_base()` does, since the two interfaces read and write the
  same files.
- **`build_session` falls back to `FsModKnowledgeStore::vendored()`**
  when no profile base resolves (`common.rs`) — a missing
  `%LOCALAPPDATA%` degrades to shipped behaviour, never a failed load.
- **Module layout after the last file split**: `commands/assign.rs`
  keeps `AssignCommand`/`run` and the shared parsers/formatters;
  per-subcommand logic lives in
  `assign/{propose,project,rows,sections,items,coverage,export}.rs`
  (each `run_*` is `pub(super)`, `*Args` types re-exported).
  `commands/fixture_gen.rs` keeps `generate`/`GenSummary` and re-exports
  `Spec` from `fixture_gen/spec.rs`, with the mod tree in
  `fixture_gen/mods.rs` and the archetype/planted population in
  `fixture_gen/population.rs`. Former inline tests are sibling
  `<name>/<name>_tests.rs` files declared with `#[cfg(test)] #[path =
  …] mod tests;`. This is a binary crate, so an unused `pub use` is dead
  code — a tests file imports only what it needs from the child module,
  not through the facade.

## Traps

- `sort`/`ledger` accept `--tie-break`/`--use-imported-pairs`/
  `--no-imported-placements` for flag parity with `Settings`, but only
  `--tie-break` actually does anything on these two commands (it reaches
  `SortInput` directly) — `--use-imported-pairs`/`--no-imported-placements`
  are inert, since both commands are JSON-in only with no live rules
  file to filter in the first place. Don't assume the import-toggle
  flags change output for these two commands; `--tie-break` does.
- `verify` deliberately has **no** `--enforce-soft`/`--enforce-awareness`/
  `--no-inferred` override flags, unlike `sort`/`ledger`. It reads a live
  session's persisted `rules.json` settings; a local override would let
  `verify`'s own "suggested" order silently diverge from what `apply
  --source suggested` actually writes.
- `verify`'s findings print grouped by `(mod, operation identity)`
  (`group_by_operation`), not one row per `Finding`. A single
  RimWorld-log-line failure can produce many findings (one per matched
  def in an OR-list xpath) — printing the raw list would repeat the same
  real failure dozens of times over.
- The `conflicts with <kind>` line under a `verify` fix only fires for
  an edge the sorter actually recorded giving up (in the sort outcome's
  own dropped-edge list), never for an edge the current order merely
  fails to satisfy. An `Awareness`-strength edge is routinely violated
  by an ordinary, cycle-free real order — flagging every such violation
  produces false positives by the hundreds.
- `rule set-pair`'s `--override-declared` is not idempotent to omit: a
  second call for the same key that leaves the flag off *clears* it if
  the stored rule already carried it. The `fix:` line `verify` prints
  discloses this with a `note:` rather than silently reprinting the
  existing comment.
- `assign coverage` names no winner by default — there is no built-in
  precedence table; which def type has a verified rule is data the
  rules repository supplies, not something this crate hardcodes. A
  workspace with an empty or stale rules cache shows every candidate as
  "no winner named."
