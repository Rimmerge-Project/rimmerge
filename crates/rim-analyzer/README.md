# rim-analyzer

A **read-only** analyzer for RimWorld mod load orders. It scans a RimWorld
install, the Steam workshop folder, and the active `ModsConfig.xml`, and
reports load-order-relevant facts derived from the mod files themselves:

- **Hard dependencies** — a DLL's `AssemblyRef` naming an assembly another
  mod ships, *classified as load-time or lazy*: after loading a mod's DLL,
  RimWorld calls `Assembly.GetTypes()`, which throws (dropping the whole
  assembly) if a type's base type or implemented interface lives in a
  not-yet-loaded assembly. Only that case — a `TypeDef.Extends` or
  `InterfaceImpl.Interface` resolving into the referenced assembly — is a
  true load-order-breaking (`Hard`-strength) dependency; a reference that
  only appears in a method body, field, or attribute resolves lazily
  (`Soft`-strength) and tolerates any load order. `forceLoadAfter`/
  `forceLoadBefore` are always `Hard`.
- **Declared edges** — `loadAfter`/`loadBefore`, and `modDependencies`.
  RimWorld itself never sorts by `modDependencies` — it only warns at
  startup when the dependency is missing (see `missing_dependencies` for
  that check) — but a declared dependency is still the author's own
  explicit ordering statement, so it carries `Declared` strength, the
  same way community tooling treats it.
- **Awareness edges** — `PatchOperationFindMod` names, `LoadFolders.xml`
  `IfModActive` gates, mutating patches that target a def owned by
  exactly one other mod, and `MayRequire`/`MayRequireAnyOf` conditions
  naming another active mod.
- **Any-of constraints** — when an `AssemblyRef` names an assembly shipped
  by *more than one* active mod, that's not a hard edge onto any single
  one of them (which one actually satisfies it isn't certain) — it's
  modeled as one `Constraint::AnyOf` per (referencing mod, assembly)
  pair, satisfied when any one of the candidates loads first.
- **Conflicts** — overriding defs (categorized as a vanilla override,
  declared-intent, a framework def shadowed by a leaf mod, or
  unexplained), colliding patch targets (severity `Additive` vs.
  `Contested` — whether the mods' operations can actually change the
  outcome depending on order), overriding textures, duplicate assembly
  names, and likely duplicate/forked mods (a large `(def_type, def_name)`
  overlap between two mods that declare no relation to each other and
  share no author).
- **Violations** — which of the above the *current* load order actually
  satisfies or breaks, plus undeclared hard dependencies, missing mods,
  missing dependencies, incompatible-but-both-active pairs, and
  unsupported-game-version mods.
- **Framework candidates** — mods with three or more distinct active mods
  holding a Hard-strength edge on them, a proxy for "this is a shared
  library other mods build on, not leaf content." Each mod also reports
  its `soft_dependents` count (lazy `AssemblyRef` edges) alongside
  `hard_dependents` and `awareness_dependents`.

It never writes to the game install or its config. The JSON report is
consumed by rimmerge's own load-order sorter (`rim-resolve`) — see the
workspace root [README.md](../../README.md) for the crate graph this
fits into.

## Usage

```
rim-analyzer analyze [OPTIONS]
```

| Option | Default | Meaning |
|---|---|---|
| `--game-dir PATH` | auto-detected (Steam libraries from `libraryfolders.vdf`, then the standard Steam/GOG locations; an error listing every candidate when none has a `Version.txt` and a `Data/Core/`) | RimWorld install directory |
| `--workshop-dir PATH` | `<game-dir>/../../workshop/content/294100` | Steam workshop content folder |
| `--mods-config PATH` | `%USERPROFILE%\...\RimWorld by Ludeon Studios\Config\ModsConfig.xml` | Active mod list |
| `--game-version X.Y` | first two components of `<game-dir>/Version.txt` | Version used for `ByVersion`/`LoadFolders.xml` resolution |
| `--json PATH` | (none) | Write the full report as JSON to this path |
| `--all-folders` | off | Diagnostic mode: ignore `LoadFolders.xml`, scan every folder a mod ships |
| `--verbose` | off | List every item in the text summary instead of the top 20 |

The text summary always prints to stdout. Example:

```sh
cargo run --release -- analyze --json report.json
```

A full scan of a large, real-world install completes in a few seconds on
a release build — the ECMA-335 reader also walks each DLL's `TypeRef`/
`TypeDef`/`InterfaceImpl` tables to classify load-time vs. lazy
`AssemblyRef`s, and the duplicate-mod heuristic adds an `O(n²)` pairwise
comparison over mods with at least ten defs.

## Architecture

Hexagonal-lite:

- `domain/` — pure value types (`ModId`, `Mod`, `LoadOrder`, `Edge`,
  `Conflict`, `Report`, `ScanOutput`, ...). No filesystem, no XML.
- `extract/` — pure functions parsing one file's bytes into domain types:
  `About.xml`, `ExpansionDefs.xml`, `LoadFolders.xml`, `Defs/**/*.xml`,
  `Patches/**/*.xml`, texture path normalization, `ModsConfig.xml`, and a
  hand-rolled ECMA-335 metadata reader for `.dll` assembly
  identity/references. Filesystem existence checks (which folders and
  files a mod actually ships) live in `infra`, not here.
- `infra/` — filesystem discovery and parallel (rayon) scanning, turning
  a game install into `ScannedMod`s.
- `analysis/` — builds edges, detects conflicts, checks the load order,
  and assembles the final `Report`.
- `interface/` — renders a `Report` as the stdout text summary.
- `main.rs` — a thin CLI shell (clap) over the crate: wires the above
  together and writes the optional JSON report.

## Assumptions

- **`ByVersion` lists override, not merge.** Per the RimWorld wiki,
  `<loadAfterByVersion>` (and the `loadBefore`/`forceLoadAfter`/
  `forceLoadBefore`/`incompatibleWith`/`modDependencies` equivalents)
  entirely *replace* the base list for a matching game version rather
  than appending to it. A mod that wants both must repeat the base
  entries under its `ByVersion` block.
- **Core/DLC display names** come from
  `Data/Core/Defs/Misc/ExpansionDefs/ExpansionDefs.xml`'s `<label>` for
  the matching `<linkedMod>` packageId (every DLC's `ExpansionDef` ships
  in *Core's* `Defs/`, not its own — their own `About.xml` has no
  `<name>`), falling back to a fixed table of the six known Ludeon
  packageIds if that file is missing or doesn't cover one.
- **A `PatchOperationFindMod` name matching an active mod's packageId
  (not its display name) still counts as unresolved** — RimWorld's own
  `FindMod` matches on display name only — but is reported separately
  (`find_mod_names_using_package_id`) as a likely authoring mistake.
- **The likely-duplicate-mod heuristic excludes Core/DLC.** Comparing
  vanilla's (large) def set against every other mod's would be both
  meaningless — RimWorld's own content isn't a "duplicate" of anything —
  and, at `O(n²)` mod pairs, by far the most expensive comparisons in the
  set.

## The assembly metadata reader

`.dll` assembly identity, references, and load-time-vs-lazy
classification are read via a small hand-rolled ECMA-335 metadata parser
(`extract::pe_metadata`) rather than a third-party PE/CIL crate: the one
evaluated option pulls in roughly 150 transitive dependencies (crypto
libraries for strong-name/resource handling this tool never needs) for a
feature surface far beyond what's needed here. The hand-rolled reader
covers exactly the ECMA-335 II.24/II.22 structures needed — PE headers →
CLI header → metadata root → `#~` tables stream →
`Assembly`/`AssemblyRef`/`TypeRef`/`TypeDef`/`InterfaceImpl` row layout
(including correct coded-index and heap-index width computation) — every
read bounds-checked against the input slice, so malformed or hostile
`.dll` bytes produce a typed error instead of a panic.

Load-time classification walks every `TypeDef.Extends` and
`InterfaceImpl.Interface` field: when it names a `TypeRef` whose
`ResolutionScope` is (directly, or through a chain of nested-type
`TypeRef`s — a nested type's `TypeRef` names its enclosing type's
`TypeRef` as its scope) an `AssemblyRef`, that `AssemblyRef` is marked
load-time — this is exactly what RimWorld's post-load `Assembly.GetTypes()`
call resolves eagerly. A `TypeSpec` target (a generic instantiation) is
never unwrapped; it simply isn't treated as a load-time reference.

It's validated against `System.Reflection.Metadata` ground truth in
`tests/pe_metadata_ground_truth.rs` (`#[ignore]`d — requires a real
RimWorld install with a mod whose DLL subclasses a vanilla type, so its
load-time reference set is a genuine, non-empty subset of its full
reference set).

## Development

```sh
cargo build
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
cargo nextest run --all-features
```

Ground-truth PE-metadata tests compare against real DLLs and are
`#[ignore]`d; running them is part of the real-install test tier — see
[docs/testing.md](../../docs/testing.md) for how to run it.

## License

Licensed under either of Apache-2.0 or MIT at your option. Unless you
explicitly state otherwise, any contribution intentionally submitted for
inclusion shall be dual licensed as above, without additional terms or
conditions.
