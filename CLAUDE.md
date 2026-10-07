# Rimmerge

RimWorld mod load-order sorter, conflict ledger, and merge-patch generator.
Cargo workspace (Rust 2024, MSVC) plus a Tauri v2 + Vue 3 desktop app.

Public docs are binding for behavior: `docs/concepts/sorting.md` (the
sorter, tiers, edge strengths, tie-breaks), `docs/concepts/ledger.md`
(findings, confidence, decisions), `docs/concepts/merge.md` (the merge
evaluator, compatibility patches, the patch maker), `docs/concepts/verify.md`
(patch replay, the counterfactual, the verify -> pair-rule loop),
`docs/concepts/load-order.md`, `docs/concepts/rules-databases.md`, and
`docs/architecture.md` (crate graph, where to add what) describe current
behavior and must stay accurate — read the relevant page before changing
the sorter, the ledger's confidence table, or the merge evaluator. A
maintainer's own `.journal/` checkout (gitignored, never published) holds
the historical design docs and past reviews behind each decision; it's
readable background, not a citable authority for new work.

## Layering (enforced by the crate graph, keep it that way)

`apps/*` -> `rim-session` -> `rim-merge` -> `rim-resolve` -> `rim-analyzer::domain`;
`rim-io` implements `rim-session`'s ports. `rim-resolve`, `rim-merge`, and
`rim-analyzer::extract` do no IO. Interfaces (`apps/cli`, `src-tauri`) are
composition roots with no business logic.

## Hard rules

- IMPORTANT: agents and tests never write under a real game install
  (wherever `RIMMERGE_GAME_DIR`/detection points), the real
  `ModsConfig.xml`, or the real RimSort folder — every test and manual
  run that writes uses a temp copy (`tempfile`, or the scratchpad). This
  is a rule for how *this workspace* is developed, not a claim that the
  product never writes there: the product's own writers are `apply`
  (including `--write-merge-mod`), `mods activate|deactivate`, and
  `patch|assign export --install`. Run any of them against the real
  install only when a human asks for it.
- IMPORTANT: agents and tests never start the real game — no call to
  the real `SystemGameLauncher::launch`, no `steam://` URL opened, no
  `RimWorldWin64.exe` spawned. Tests inject a fake launcher
  (`FakeGameLauncher`, `RecordingGameLauncher`); only a manual check a
  human asks for starts RimWorld.
- IMPORTANT: the only network access this workspace may make goes to
  the closed host list in `crates/rim-io/src/net/allowlist.rs`
  (`raw.githubusercontent.com` for the three rule databases,
  `api.github.com` for the release check only) — see
  `docs/privacy-and-network.md`. The desktop app's automatic
  checks (the release check once per launch, the rule-database refresh
  at most once a day) are on by default but never run before the
  first-run notice is answered, never from the CLI, and never when
  `AppSettings::network.allow_network` is `false`; the URLs are
  constants, not settings. Every default gate stays hermetic:
  `cargo nextest run --workspace --all-features` opens no socket, and
  the only tests that really talk to GitHub are `#[ignore]`d
  (`crates/rim-io/tests/real_network_databases.rs`,
  `crates/rim-io/tests/real_network_release_feed.rs`, and the
  real-install `db refresh` + `import --from-cache` test in
  `apps/cli`). A new host is a design change, not a fix.
- Do not commit, push, or rewrite history unless asked.
- Determinism is a tested contract: the analyzer's JSON report and the
  sorter's output must be byte-identical across runs. Use `BTreeMap`/
  `BTreeSet` for anything that reaches output; no `HashMap` iteration.
- `unwrap`/`expect` are denied outside tests (workspace lints).

## Gates (run all before reporting a change as done)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo deny check
pwsh ./scripts/check-forbidden.ps1
pwsh ./scripts/check-line-endings.ps1
pwsh ./scripts/check-doc-links.ps1
pwsh ./scripts/count-doc-warnings.ps1   # rustdoc warning budget; CI runs the same script
cd apps/desktop && bun run types:check && bun run typecheck && bun run lint && bun run test && bun run e2e
```

## The real-install tier

A second, larger test tier reads an actual RimWorld install; see
`docs/testing.md` for the full design (the three-state rule, the pins-file
pattern, and why each part exists). Short version:

```sh
cargo nextest run -p rim-analyzer -p rim-resolve -p rimmerge-cli --all-features --release --run-ignored ignored-only
cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only
```

`-p rimmerge-cli` is required in the first command: the real-install
`assign` tests live there, not in `rim-analyzer`/`rim-resolve`.
`-p rimmerge-desktop` is its own, separate invocation — its real-install
tests are never covered by the first command. A real-install test that
isn't part of one of these two documented commands is worse than an
`#[ignore]`d test with no assertions at all: it never runs until someone
thinks to run it by hand. Run the two commands one after the other,
never concurrently: both are CPU-heavy release builds of large scans,
and contention pushes the tier's timing budgets (sort under 500 ms,
several 2 s command budgets) into spurious failures. Check that no other
`cargo`/`nextest` process is running before starting either.

- `RIMMERGE_GAME_DIR` puts a machine in this tier at all, and is
  three-state: unset skips every test with an honest `skipping:` line;
  set to a path that isn't a real install (no `Version.txt`/`Data/Core/`)
  makes every test **panic**, so a typo can never silently read as a
  pass; set correctly, the tier runs. This three-state check, plus a
  floor of 200 active mods (so a vanilla or near-empty `ModsConfig.xml`
  fails loudly instead of passing every cross-mod assertion vacuously),
  live in three shared guard modules — one each for the analyzer, CLI,
  and desktop real-install tests — not reimplemented per test file. A
  real-install tier that finishes in seconds has not run — the wall
  clock is the tell.
- **One test is outside those guard modules**:
  `crates/rim-resolve/tests/sort_golden.rs`'s full-size `#[ignore]`d
  tests read a separate file, `.journal/local/report.json` (regenerate
  with `cargo run -p rim-analyzer --release -- analyze --json
  .journal/local/report.json`) — without it they soft-skip even on a
  machine that's otherwise correctly in the tier, since the guard
  modules' `RIMMERGE_GAME_DIR` check has nothing to say about whether
  that separate file exists.
- **A second test has its own, separate command, run on its own**:
  `crates/rim-io/tests/real_game_log.rs` reads a real `Player.log` named
  by `RIMMERGE_REAL_GAME_LOG` (unrelated to `RIMMERGE_GAME_DIR`) and
  can't join either command above, since `-p rim-io` would also pull in
  `tests/real_network_databases.rs`/`tests/real_network_release_feed.rs`,
  both `#[ignore]`d for a different reason (they hit the real network)
  and must stay outside every default and every documented offline tier
  command:

  ```sh
  cargo nextest run -p rim-io --all-features --release --run-ignored ignored-only -E 'binary(real_game_log)'
  ```

  The same test binary has a directory-wide test, driven by
  `RIMMERGE_REAL_GAME_LOG_DIR` (a folder of real logs and console copies,
  searched recursively; same three-state rule: unset skips, not a directory
  or no `.log`/`.txt` file panics, otherwise every file is parsed under
  both framings and its detected kind (checked against an independent
  oracle), line conservation, sentinel leaks, logging-gap count and
  console-copy entry count are checked — see `docs/testing.md`). The same
  command above runs it.

  `crates/rim-io/tests/real_network_release_feed.rs` is, likewise, run
  on its own — it hits `api.github.com`'s `/releases/latest`:

  ```sh
  cargo nextest run -p rim-io --all-features --run-ignored ignored-only -E 'binary(real_network_release_feed)'
  ```
- `RIMMERGE_PERF_PROFILE_DIR` must point at a temp/scratch dir, never the
  real RimSort profile; the tests read the real install and
  `ModsConfig.xml` read-only and export with `install: false`.
- `RIMMERGE_MODS_CONFIG`/`RIMMERGE_WORKSHOP_DIR` (optional) point the
  tier at a specific `ModsConfig.xml`/workshop content folder instead of
  the default-relative paths.
- A real-install test's *exact* assertions (a specific mod id, count, or
  pair) only run when a matching pin variable is set, because a real
  mod's identity must never appear in a committed test file — see
  `docs/testing.md`'s "pins file pattern" for the full list and for which
  ones fall back to a shape/band check versus panic when unset. Each
  maintainer keeps their own values in a small, gitignored, dot-sourced
  script (see a maintainer's own gitignored `CLAUDE.local.md`).

## Contributor environment

A stable Rust toolchain on an MSVC host (Tauri on Windows needs MSVC),
`cargo-nextest`, `cargo-deny`, `bun`/`bunx` for all JS tooling, and
Playwright's Chromium installed. `target/` can exceed 30 GB with repeated
Tauri builds; `cargo clean` is fine, nothing persistent lives there.

## Workflow

- Plan a non-trivial structural change in `docs/` first, implement it,
  then get an independent review.
- ts-rs bindings are generated: after any DTO change run
  `cargo test -p rimmerge-desktop`, then `bun run types:check`. Never
  hand-edit `apps/desktop/src/types/generated/`.
- Subagent memory lives in `.claude/agent-memory/` at the workspace root
  (gitignored). An agent whose cwd is `apps/desktop` must not create
  `apps/desktop/.claude`.
