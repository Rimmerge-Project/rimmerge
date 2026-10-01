# Architecture

A Cargo workspace of five domain/infrastructure crates plus two
interface apps, laid out so the dependency rule is enforced by the
crate graph itself, not merely by convention.

## The crate graph

```
apps/cli, apps/desktop/src-tauri
        |
    rim-session
        |
    rim-merge
        |
   rim-resolve
        |
rim-analyzer::domain
```

`rim-io` implements `rim-session`'s ports (it depends on `rim-session`
for the trait definitions, not the other way around). `rim-resolve`,
`rim-merge`, and `rim-analyzer::extract` do no IO at all — every
filesystem/network/process operation lives in `rim-io` or the two
interface apps.

- **`rim-analyzer`** — reads a RimWorld install (mods, defs, patches,
  assemblies, textures) into a `Report`: the analysis/extraction layer.
  Its `domain` module (pure types: `Report`, `Edge`, `EdgeKind`, `ModId`,
  ...) has no IO of its own; `extract`/`analysis` build a `Report` from
  parsed XML/PE data, also pure; `infra` is the one part of this crate
  that touches the filesystem.
- **`rim-resolve`** — pure domain: the sorter, tag inference, the
  resolution ledger (findings, confidence, decisions), and the
  assignment domain (a patch-maker project's own schema, shape
  inference, coverage, precedence rules, and section model). No IO, no
  XML parsing, no Tauri. See [concepts/sorting.md](concepts/sorting.md)
  and [concepts/ledger.md](concepts/ledger.md).
- **`rim-merge`** — pure domain: field-level diffing, the merge
  evaluator, and patch replay (`verify`). For the patch maker, this
  crate only *renders* — turning an already-built `rim-resolve`
  assignment project into patch XML — it owns none of that feature's
  own schema/coverage logic. See [concepts/merge.md](concepts/merge.md)
  and [concepts/verify.md](concepts/verify.md).
- **`rim-session`** — application layer: orchestrates use cases over
  the three crates above, defines the ports (`trait`s) `rim-io`
  implements, and owns the `Session`/`Settings` types a profile is
  built from.
- **`rim-io`** — infrastructure: every real filesystem/network/process
  adapter — reading the install, writing `ModsConfig.xml`, the rule
  database cache and fetcher, profile/patch/assignment persistence. Its
  `mod_knowledge.rs` also embeds `rules/rimmerge-rules.json` at compile
  time (`build.rs`) — a snapshot of the sibling
  [`rimmerge-rules`](https://github.com/Rimmerge-Project/rimmerge-rules)
  repository, checked out here as a git submodule at `rules/`, outside
  the crate graph proper. See
  [concepts/rules-databases.md](concepts/rules-databases.md).
- **`apps/cli`** — a `clap` binary, a thin composition root: parses
  arguments, wires `rim-io` adapters into `rim-session`, calls one use
  case, prints the result. No business logic.
- **`apps/desktop`** — a Tauri v2 + Vue 3 app. `src-tauri` is the same
  kind of composition root as the CLI (Tauri commands call into
  `rim-session`, nothing more); `src` is the Vue frontend, page-routed
  (see [desktop.md](desktop.md)), talking to the backend only through
  generated, typed DTOs (`ts-rs` — see below).

## Where to add what

- A new ordering fact derived from install content → `rim-analyzer`
  (`extract`/`analysis`), producing a new or extended `Edge`/`EdgeKind`.
- A new way to resolve a conflict, or a new finding kind → `rim-resolve`
  (`ledger`) if it's about the sorter/ledger; `rim-merge` if it's about
  field-level merging or patch replay.
- A new kind of hard problem shown at Apply (something the written
  order breaks, or a mod it would drop) → a `HardProblem` variant in
  `rim-resolve::preflight`; the compiler then points at every interface
  that has to present it.
- How a `graphicClass` turns a `texPath` into texture files (the
  engine's faces, mirroring, and collection members) →
  `rim-analyzer::extract::graphics`, pure over a `TextureCatalog`;
  which texture keys exist and who ships them →
  `SourceIndex::textures`.
- What texture a def shows (its slots, and per face whether the game
  finds it) → `rim-session::use_cases::def_graphic`, pure over the
  effective tree and `SourceIndex::textures`.
- A new way to read or write something real (a file format, a network
  call) → a port `trait` in `rim-session`, implemented in `rim-io`.
- A new game-log message the engine writes → a class pattern in
  `rim-io`'s `game_log` (the exact decompiled wording). A log line format
  a *mod* prints → a `log-shapes` row in the
  [rules data](concepts/rules-databases.md), never code.
- A new user-facing action that orchestrates existing pieces → a use
  case in `rim-session`, then a thin CLI subcommand and/or Tauri
  command calling it.
- Nothing at the interface layer (`apps/*`) should contain a decision
  about *what* to do — only how to parse the request and how to render
  the result.
- A new notice → a `Notification` variant, its `NotificationKind` (the
  declaration order is display priority) and a rule in
  `rim-session::notifications::evaluate`; the kind's on-disk spelling in
  `rim-io`'s `notifications.rs`; the mirrored DTOs in `src-tauri`; and
  i18n keys, never a Rust sentence — the compiler and the frontend's
  `assertNever` switches point at each place. See `apps/desktop/CLAUDE.md`'s
  localization rules.
- A new outbound host → an `AllowedHost` variant in
  `crates/rim-io/src/net/allowlist.rs`, reviewed as a security-relevant
  change (root `CLAUDE.md`'s network hard rule), never a setting.

## Determinism

The analyzer's `Report` and the sorter's output are a tested contract:
byte-identical across runs given the same input. Every collection that
reaches either uses an ordered container (`BTreeMap`/`BTreeSet`), never
hash-iteration order.

## Frontend/backend boundary

DTOs crossing the Tauri boundary are generated from the Rust types that
define them (`ts-rs`) — the TypeScript types under
`apps/desktop/src/types/generated/` are never hand-edited; they're
regenerated from the Rust `#[derive(TS)]` structs whenever a DTO
changes.
