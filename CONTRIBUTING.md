# Contributing to Rimmerge

This is a technical contribution guide only. It covers how to build,
test, and submit a change — nothing else.

## Build

Cargo workspace (Rust 2024, MSVC) plus a Tauri v2 + Vue 3 desktop app.
Windows/MSVC only — see `docs/architecture.md` and
`docs/dependency-versions.md` for why.

This repository carries [`rimmerge-rules`](https://github.com/Rimmerge-Project/rimmerge-rules)
as a git submodule at `rules/`, embedded into `rim-io` at compile time
(`crates/rim-io/build.rs`). Clone with `--recursive`, or run
`git submodule update --init` in an existing clone, before building —
a build without it fails fast with a message telling you exactly that.

```sh
cargo build --workspace
```

Desktop app:

```sh
cd apps/desktop
bun install          # bun only — never npm/yarn/pnpm
bun run tauri dev
```

See `apps/desktop/README.md` for the day-to-day dev loop.

## Gates

Run all of these before opening a PR — they're what CI runs, and the PR
template checklist mirrors this list exactly:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo deny check
pwsh ./scripts/check-forbidden.ps1
pwsh ./scripts/check-line-endings.ps1
pwsh ./scripts/check-doc-links.ps1
cd apps/desktop && bun run types:check && bun run typecheck && bun run lint && bun run test && bun run e2e
```

`cargo nextest` requires `cargo-nextest` (`cargo install cargo-nextest --locked`);
`cargo deny check` requires `cargo-deny` (`cargo install cargo-deny --locked`).

`bun run types:check` regenerates the ts-rs TypeScript bindings under
`apps/desktop/src/types/generated/` by running `cargo test -p rimmerge-desktop`
and diffing the result — **never hand-edit a file under that directory**.
If you change a DTO's shape (add/remove/rename a field, change a type),
run `cargo test -p rimmerge-desktop` and commit the regenerated bindings
in the same change.

## Layering

```
apps/* -> rim-session -> rim-merge -> rim-resolve -> rim-analyzer::domain
```

`rim-io` implements `rim-session`'s ports (traits) — it sits beside
`rim-session` in the dependency graph, not below `rim-analyzer`, and
nothing below `rim-session` ever depends on it.

- `rim-analyzer` — read-only scanner of a RimWorld install; its `domain`
  module is the shared kernel every other crate depends on.
- `rim-resolve` — pure, IO-free domain logic: rules, tags, the sorter,
  conflict evaluation, the ledger.
- `rim-merge` — pure, IO-free merge-patch engine: XML field trees,
  inheritance, diff/plan, emission.
- `rim-session` — application layer: the `Session` aggregate and its use
  cases, each constructor-injected with the ports (traits) it needs.
- `rim-io` — infrastructure: implements every `rim-session` port
  (filesystem, `ModsConfig.xml`, RimSort import, network refresh).
- `apps/cli`, `apps/desktop/src-tauri` — interfaces. Composition roots
  only: wire `rim-io` adapters into `rim-session` use cases and render
  the result. No business logic here.

`rim-resolve`, `rim-merge`, and `rim-analyzer::extract` do no IO —
filesystem and process access live only in `rim-io` and
`rim-analyzer::infra`. A PR that adds a filesystem read, an HTTP call, or
a process spawn to a pure crate will be asked to move it behind a port
instead.

## Determinism

The analyzer's JSON report and the sorter's output must be
byte-identical across runs on the same input. Use `BTreeMap`/`BTreeSet`
for anything that reaches output — never iterate a `HashMap` into a
report, a snapshot, or a generated file. `cargo nextest run --workspace`
includes tests that build the same scan twice and diff the result; a
change that makes one of these tests flaky is a determinism regression,
not a flaky test to retry.

`unwrap`/`expect` are denied outside tests (workspace lints) — return a
typed error instead.

## Tests

Two tiers — see `docs/testing.md` for the full explanation, including
the pins-file pattern for a maintainer's own real-install exact-value
assertions:

- **The default tier** (`cargo nextest run --workspace --all-features`,
  the desktop `bun run` gates) is hermetic: no RimWorld install, no
  network access, runs everywhere including CI. `cargo nextest run
  --workspace --all-features` must open no socket — the only tests that
  really talk to GitHub are `#[ignore]`d
  (`crates/rim-io/tests/real_network_databases.rs`, and the real-install
  `db refresh` + `import --from-cache` test in `apps/cli`). It includes a
  committed, entirely synthetic golden fixture
  (`crates/rim-resolve/tests/golden/report.synthetic.json`) exercising
  the sorter and ledger against realistic but invented data.
- **The real-install tier** is a much larger set of `#[ignore]`d tests
  that read an actual RimWorld install with its actual mod list. These
  can never run in CI and can never assume any particular mod is
  installed — every exact-value assertion in this tier is gated behind
  an environment variable a maintainer sets locally, never a hardcoded
  mod id. Run it with:

  ```powershell
  $env:RIMMERGE_GAME_DIR = '<your RimWorld install>'
  $env:RIMMERGE_PERF_PROFILE_DIR = '<a scratch profile dir>'
  cargo nextest run -p rim-analyzer -p rim-resolve -p rimmerge-cli --all-features --release --run-ignored ignored-only
  cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only
  ```

  Both variables are required — a real, vanilla install with too few
  active mods fails loudly rather than silently passing every cross-mod
  assertion vacuously. See `docs/testing.md` for exactly what "in this
  tier" means, the three-state unset/wrong/correct rule every
  real-install test guard follows, and how to read a wall-clock time to
  tell whether the tier actually ran.

A bug fix needs a regression test that fails before the fix and passes
after. New business logic needs a test covering its happy path and at
least one failure case.

## Neutral vocabulary in fixtures

No fixture, test literal, or committed sample file may name a real mod,
a real author, or a real machine path. Use one of these namespaces for
invented mod ids and package ids in new tests:

- `example.*` — a generic invented mod (`example.core`, `example.framework`).
- `mypatch.*` — an invented user-authored patch/assignment/merge output.
- `synth.*` — content belonging to the synthetic install fixture
  (`crates/rim-resolve/tests/fixtures/synthetic-install.json` and its
  generated golden report).

`./scripts/check-forbidden.ps1` enforces this against
`.github/forbidden-tokens.txt` (real paths, a real author's name, and a
list of real third-party mod ids/classes) and is part of the gate list
above. The committed list is shape-only (generic path patterns, no mod
or author named); maintainers may keep a project-specific list of real
mod ids and author handles locally under `.journal/forbidden-tokens.local.txt`,
which the script merges in automatically when present.

## Proposing a load-order rule

If what you've learned is a fact about one specific mod rather than a
change to Rimmerge's own code, it almost never belongs in this repo:

- **A plain `loadAfter`/`loadBefore`/`loadTop`/`loadBottom`/
  `incompatibleWith` pair** — anything RimSort's own community-rules
  format can express — belongs in
  [RimSort's Community-Rules-Database](https://github.com/RimSort/Community-Rules-Database).
  Propose it there; Rimmerge picks it up automatically through
  `rimmerge db refresh`.
- **Anything RimSort's format cannot express** — a precedence rule
  between two def instances, a custom patch-operation behavior, a
  def-cache carrier, or a tag-inference rule — belongs in the sibling
  [`rimmerge-rules`](https://github.com/Rimmerge-Project/rimmerge-rules)
  repository instead, as data (JSON against a published schema), never
  as Rust code in this repo. See that repository's own `CONTRIBUTING.md`
  for its schemas and evidence requirements.

A PR to *this* repo that hardcodes a specific mod id, class name, or
def name outside a rules-database context will be asked to move that
knowledge to one of the two places above.

## Translating

The desktop app's UI ships in English, Simplified Chinese and Brazilian
Portuguese; the CLI stays English. [`docs/translating.md`](docs/translating.md)
covers the locale files, placeholders and plurals, what never gets
translated, the glossary
([`apps/desktop/src/locales/glossary.md`](apps/desktop/src/locales/glossary.md)),
and the checks a translation change runs: `bun run test` and
`bun run i18n:status`.

## Releasing

Maintainer-only. The version number is spelled out by hand in several
files, and all of them must agree before a tag is pushed:

1. Bump the version everywhere it's declared: every crate's own
   `[package] version` in `Cargo.toml` (`crates/*/Cargo.toml`,
   `apps/cli/Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`), the
   matching `version` on each internal crate's path-dependency pin under
   the root `Cargo.toml`'s `[workspace.dependencies]`,
   `apps/desktop/src-tauri/tauri.conf.json`, and
   `apps/desktop/package.json`.
2. Run `pwsh ./scripts/check-versions.ps1` and fix anything it reports
   before continuing — it fails on the first mismatch rather than
   guessing which file is right.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`. The tag
   triggers `.github/workflows/release.yml`, which re-runs
   `check-versions.ps1 -Tag vX.Y.Z` (so a tag that doesn't match the
   files above fails before anything builds), builds the CLI zip and
   the portable desktop zip (`scripts/package-portable.ps1`; Tauri
   bundling is off, so there is no installer), and opens a **draft**
   GitHub Release with both zips and a `SHA256SUMS.txt` attached.
4. Review the draft release (release notes, both zips and the checksum file present) and
   publish it by hand — the workflow never publishes on its own. There
   is no code signing for either zip; say so in the release notes
   if the generated ones don't already.
5. Publishing a release marks it **Set as the latest release** by
   default — leave that ticked for an ordinary release. Publishing a
   **backport** to an older line (a fix released after a newer version
   already went out) is the one case where you untick it: the desktop
   app's own update check and `rimmerge check-update` both read
   `GET /repos/Rimmerge-Project/rimmerge/releases/latest`, which is
   GitHub's own "latest" flag, not the highest version number — leaving
   it ticked on a backport would announce an older version as the newer
   one.

`/repos/Rimmerge-Project/rimmerge/releases/latest` is now a **runtime
dependency of every shipped binary** (see
[docs/privacy-and-network.md](docs/privacy-and-network.md)): the
desktop app's automatic update check and `rimmerge check-update` both
call it by this exact, hardcoded owner/repo path. This repository's
releases must stay public, and the repository must not be renamed or
transferred without updating that path first — either would silently
break the update check for every already-installed copy, with no way
for this project to notice.

## Contribution terms

Licensed under either of Apache-2.0 or MIT at your option. Unless you
explicitly state otherwise, any contribution intentionally submitted for
inclusion in this repository shall be dual licensed as above, without
any additional terms or conditions.
