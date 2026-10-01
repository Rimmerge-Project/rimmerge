# rim-analyzer

Read-only scanner of a RimWorld install. Its JSON `Report` is a contract
consumed by `rim-resolve`, the golden fixtures, and the desktop app.

## Layer

Base of the crate graph — nothing in this workspace is below it.
`rim-resolve` depends on `rim-analyzer::domain` only; `rim-session`,
`rim-merge`, and every interface sit above that. `extract/` and
`analysis/` are pure (bytes in, domain types out); filesystem walking and
`rayon` live only in `infra/`. Do not add IO to `extract/` or `analysis/`.

## Invariants

- The report must stay byte-identical across runs
  (`report_builder::building_the_same_scan_twice_produces_byte_identical_json`).
- Adding a field to `Report` is a contract change: bump
  `REPORT_SCHEMA_VERSION` (a human tripwire, not a read-time guard — it
  signals "something changed", it doesn't gate loading), update
  `crates/rim-resolve/tests/golden/` and the desktop DTOs in the same
  change, and re-measure `analyze --json` before/after with `metadata`
  dropped to confirm nothing else moved. A safe-empty default
  (`#[serde(default)]`, e.g. `mod_costs`, `inactive_mods`) is the normal
  shape; `Edge.subject: Option<String>` is the one field-level exception
  that skips serializing when absent instead — an older cached report's
  edges simply read back with `subject: None`, the right value for an
  edge that predates the field.
- `unwrap`/`expect` denied outside tests (workspace lint); every read in
  `extract/pe_metadata.rs` is bounds-checked and every arithmetic op is
  checked — it is a hand-written ECMA-335 reader over untrusted bytes.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rim-analyzer --all-targets --all-features -- -D warnings
cargo nextest run -p rim-analyzer --all-features
```
Ground-truth PE-metadata tests compare against real DLLs and are
`#[ignore]`d (`--run-ignored ignored-only`); the real-install tier
(`tests/common/mod.rs`, all six `real_install_*.rs` files plus
`pe_metadata_ground_truth.rs`) is described in the root `CLAUDE.md` and
`docs/testing.md`. Before reporting done, run the full gate block in the
root `CLAUDE.md`.

## Conventions

- Module layout: a facade file (`analysis/edges.rs`, `extract/xpath_expr.rs`,
  `extract/pe_metadata.rs`) keeps the public API and re-exports its
  children in a same-named directory (`edges/{assembly,declared,names,
  patches,defs,textures,manifest}.rs`, `xpath_expr/{lex,parse,
  predicates}.rs`, `pe_metadata/{headers,tables,refs}.rs`). The facade's
  own former inline tests live in **one** sibling `<facade>_tests.rs`
  (`edges_tests.rs`, `xpath_expr_tests.rs`, `pe_metadata_tests.rs`)
  declared with `#[cfg(test)] #[path = "…"] mod tests;` — the split
  children have no test files of their own. Follow this shape for any
  new split; no public path should change as a result of one.
  `analysis/edges.rs`'s own `use super::patch_op_targets;` exists so its
  children can still reach that sibling item through their own inline
  `super::patch_op_targets` path one level down — remove it and every
  child's own `super::` reference breaks.
- `analysis/conflicts.rs` has the same shape over
  `conflicts/{defs,patches,textures,assemblies,assets,duplicate_mods}.rs`
  and tests in `conflicts/conflicts_tests.rs`. It keeps
  `sorted_by_load_order` itself, because `analysis/mod_cost.rs` imports it
  as `super::conflicts::sorted_by_load_order`. Like `edges.rs`, it carries
  a `use super::patch_op_targets;` so `conflicts/patches.rs` can keep its
  inline `super::patch_op_targets` path.
- `analysis::edges` and `analysis::indices` are `pub`, not `pub(crate)`,
  specifically so a downstream crate's *tests* can build a real `Edge`
  through a function like `edges::uses_type_edges` instead of a
  hand-built literal. Normal callers stay on `analysis::build`/`build_ref`.
- Fixture trees under `tests/fixtures/` are tiny scratch games;
  `Version.txt` may be absent — scan tests pass the version explicitly.

## Two xpath parsers, and `xpath_expr`'s three entry points

`extract/xpath_target.rs` is a cheap, `pub` head-only scan (`parse_all`)
used for collision detection and as a fallback when the strict grammar
rejects a head outright — a head naming only other defs is `Elsewhere`
("succeeded elsewhere"), never a lenient parse of the real semantics.
`extract/xpath_expr.rs` is the strict grammar used for replay; anything
it cannot model exactly returns `Unsupported` with a reason naming the
token, never a lenient guess.

`xpath_expr` exposes three entry points, and which one a caller uses is a
design decision: `parse` answers "*which* defs does this name" (still
`Unsupported` for anything it can't enumerate); `head_content_predicate`
(Group B) reports an `or`-only child-value head as `alternatives`, the
shape `analysis::edges::child_value_targets` indexes; `head_filter_predicate`
reports everything else the grammar can parse but not enumerate
(`@ParentName=`, `and`-composed terms, `not(...)`, nested child filters,
`contains(text(), …)`, a bare-type head with no bracket at all) as a
`Predicate` for a caller holding one def's tree to evaluate. Try them in
that order — Group B's shape is a strict subset of the filter query's,
and it's the one with shipped, measured indexing behaviour.

The predicate grammar's `Not` takes a whole `Predicate` (not a bare child
name); `Child(name, pred)` is "some child named `name` satisfies `pred`"
at arbitrary depth; `Contains` is `contains(text(), "v")` and nothing
else. `MAX_PREDICATE_DEPTH` is 256, spent per *term* (a flat `or` chain
costs one level per term; the real install ships a 103-term head). The
budget is not one unit per level: a bracket filter costs
`FILTER_DEPTH_COST` (8), `not(…)` costs `NOT_DEPTH_COST` (4), a relative
path costs one per segment. If you add a construct that re-enters
`parse_boolean`, measure its own cost on a 1 MiB debug stack the same way
— `a_pathological_predicate_of_every_shape_fails_cleanly_on_a_small_stack`
is the proof, not the arithmetic.

## Install detection

`infra::paths::is_game_dir(&Path)` (`Version.txt` **and** `Data/Core/`
both present) is the single definition of "this is an install" — used by
detection and by every real-install test guard, so it can never mean two
things. `default_game_dir_candidates()`/`detect_game_dir()` read only env
vars plus `steamapps/libraryfolders.vdf` (a ~60-line hand-rolled reader,
no new dependency; a malformed file degrades to "no libraries found"
rather than erroring — unit-tested against a fixture with invented drive
letters, never a real machine's layout). The Windows registry
(`HKCU\...\SteamPath`) is deliberately not read — the two
`%ProgramFiles*%\Steam` probes plus `libraryfolders.vdf` already cover
every library. Per-OS candidates are emitted so a non-Windows user gets a
sane error, but that is not a claim of non-Windows support: `deny.toml`
pins `x86_64-pc-windows-msvc` and CI is `windows-latest`.

## Engine facts this crate encodes

- **Loaded-folder rules follow RimWorld exactly**: `LoadFolders.xml`'s own
  descending-version search (the highest block key `<=` the game version,
  `<default>` only once nothing there is eligible — never "exact version
  then default"), gated by `IfModActive`/`IfModActiveAll`/`IfModNotActive`;
  else highest `<major.minor>` folder `<=` game version, then `Common/`,
  then root. `_steam` suffixes compare through `ModId::base()`.
- **`Mod::loaded_folders` is priority order, highest-priority folder
  first** — `ModContentPack.foldersToLoadDescendingOrder`'s own order,
  which is **not** always `LoadFolders.xml`'s document order:
  `InitLoadFolders`'s local `AddFolders` function walks a matched
  `<vX.Y>`/`<default>` block's parsed `<li>` list from its *last* entry
  down to its first, so the **last-listed** `<li>` ends up
  highest-priority, not the first-listed one. The default folder rule (no
  `LoadFolders.xml`, or no entry for the running version) needs no such
  reversal — `InitLoadFolders`'s fallback branch adds the version folder,
  then `Common/`, then root directly, in that priority order already.
  `mod_scan.rs::resolve_loaded_folders` returns exactly this one order —
  it *is* the order `scan_one_mod` walks folders in, so it decides the
  order of this mod's own `defs`/`templates`/`patch_ops`/`assemblies`
  too, not just which folder wins a same-relative-path collision.
- **Same relative path across loaded folders**: RimWorld's own
  `DirectXmlLoader.XmlAssetsInModFolder` (and
  `ModContentPack.GetAllFilesForMod`, the same shape for `Textures/`/
  `Sounds/`/`Strings/`/`Assemblies/`) reads every loaded folder's file
  tree into a `Dictionary<string, FileInfo>` keyed on the file's path
  relative to its own loaded folder (`TryAdd`/`ContainsKey`-then-`Add` —
  first occurrence wins, case-sensitive, ordinal — no folding), walked in
  priority order. A mod shipping the same relative path in two loaded
  folders (e.g. `1.6/Defs/X.xml` and `Defs/X.xml`) therefore has its
  **lower-priority** copy silently unread — the engine never opens it, so
  it produces no log line, either. `mod_scan.rs::shadowed_paths` applies
  this rule to `Defs/`, `Patches/`, and `Assemblies/` (`Textures/`/
  `Sounds/`/`Languages/` stay on plain sorted order, since only
  determinism matters there); `rim-io::asset_locator::FileAssetLocator
  ::locate_texture` walks `Mod::loaded_folders` in this same priority
  order.
- **File order within one loaded folder is breadth-first, NTFS-collation
  ordered — not a plain alphabetical sort.** `DirectoryInfo
  .GetFiles(pattern, AllDirectories)`'s own Windows implementation
  (`System.IO.Enumeration.FileSystemEnumerator`) yields one directory's
  own matching files before descending into any of its subdirectories,
  and visits subdirectories in the order it met them (FIFO), not
  depth-first; siblings are ordered by NTFS's own collation, emulated
  here (NTFS enumeration order isn't in any DLL) as a case-insensitive
  ordinal comparison over UTF-16 code units, simple-uppercase-folded
  (`extract::file_order::ntfs_collation_key`). A `.`-prefixed file name
  (including a macOS `._` resource-fork leftover) is never read.
  `infra::mod_scan::engine_enumeration_order` implements this for
  `Defs/`/`Patches/`; `rim-io`'s asset locator reuses it for `Textures/`.
  The exception is `Assemblies/`: `ModContentPack
  .GetAllFilesForModPreserveOrder` sorts by file `Name` within each
  folder, walking folders lowest-priority first — this crate's own
  `walk_files` (plain sorted order, unaffected) is enough, since only the
  relative-path dedup above is ever consumed downstream, never a mod's
  own DLL load order.
- **Patch operations within one mod apply grouped by loaded folder, in
  priority order**: every non-shadowed op from the highest-priority
  folder, in that folder's own breadth-first enumeration order, then the
  next folder's — `ModContentPack.LoadPatches` iterates the very same
  per-folder-first-wins `Dictionary`'s `Values.ToList()`
  (`Dictionary<TKey,TValue>` preserves insertion order when only
  `Add`/`TryAdd` are ever called, as they are here), so a file's
  processing order is entirely a function of which folder produced it —
  and `scan_one_mod` now walks folders in that identical priority order,
  so this crate's own `patch_ops`/`defs` order (and therefore
  `rim-merge`'s replay order, which trusts it) matches. Confirmed against
  a real case: a mod ships an `Example Tribals`-named folder
  and an `Example Security`-named folder side by side; one folder's patch
  renames an element the other folder's own later-in-that-folder op
  selects by its old name, and priority order decides whether that op
  still matches.
- **Duplicates within one mod: first wins, not last** (`DefDatabase<T>
  .AddAllInMods` logs `Mod X has multiple <T>s named Y. Skipping.` on the
  second one). `SourceIndex.defs`'/`templates`' own entry lists are in
  scan order, so a caller picking a winner among same-mod duplicates
  reads `.first()`, never `.last()` —
  `rim-session::use_cases::def_sources::{def_owner_and_raw,
  read_owner_def_raw}` are the two callers that do.
- Core and DLC `About.xml` files have no `<name>`; display names come
  from `ExpansionDefs`. Vanilla assemblies come from
  `RimWorldWin64_Data/Managed`.
- `analysis::source_index::SourceIndex`'s inverse maps have two dedup
  rules that are easy to get wrong: `ops_by_mod` must **not** filter to
  `is_top_level() == true` entries (that predicate is the top-level
  *leaf* shape only — a mutating op nested under a
  `PatchOperationSequence`/`FindMod`/`Conditional` has a longer path and
  would be silently dropped); dedup on each entry's own top-level
  ancestor, `(file, element_path.first())`, counting distinct ancestors
  per key. `children_by_template` must dedup on `(parent key, mod,
  locator)` — a concrete def that also carries a `Name` attribute is
  indexed once in `defs` and once in `templates`, same physical node,
  same locator; without the dedup it double-counts as two children.
- **Side-loaded assemblies**: a mod that ships a loader in `Assemblies/`
  which side-loads its real engine from elsewhere at runtime is invisible
  to RimWorld's own assembly resolution, so this analyzer's runtime-patch
  and assembly-reference data for it is silently incomplete.
  `extract::side_loaded_assemblies::classify` walks a `.dll` path into
  `Loaded` (any path component named `assemblies`, case-insensitive),
  `Ignored` (a dev directory — `Source`/`obj`/`bin`/`.git`/`.vs`/
  `packages`/`lib`/`nuget`/`.nuget` — or an inert-looking one), or
  `SideLoaded` (neither); the dev/inert check runs **before** the
  `Assemblies`-component check (a vendored `Source/**/Assemblies/**` copy
  must never inflate the "loaded" denominator and hide genuine
  side-loading). The scan-note fires only when `side_loaded_count >
  loaded_count`, worded informationally, never as a fault. This detector
  walks with its own `WalkDir`, never the shared `walk_files` the
  ordinary XML/def scan uses — **`walk_files` itself must never be
  pruned this way**, since `Languages/` genuinely holds relevant `.xml`
  there and pruning it there would be a real correctness bug, not a
  speedup; the dev/inert-directory pruning this detector does is only
  safe here because it runs its own separate walk. Scanning is
  active-mods-only by design (`infra::resolve_active_mods` filters
  before `scan_one_mod` ever runs) — an inactive Workshop subscription
  can carry the same shape and simply hasn't been scanned yet.
- `analysis::edges::dll_owner_of` is `pub` and takes
  `&BTreeMap<String, Vec<ModId>>` directly rather than the whole
  `Indices`, so a caller holding only a `SourceIndex` (never the whole
  `Indices`, which needs a raw `&[ScannedMod]` no session-layer caller
  holds) can still ask it; `SourceIndex::dll_owner_of` is the mirroring
  method for exactly that caller. `dll_owner_of` falls through an ambiguous
  longer prefix to a shorter unique one (a `UsesType` edge only needs an
  ordering hint) and returns one mod or none. A caller that must tell "no
  owner" from "several owners" (a log's stack frame) uses the strict
  `namespace_ownership` (`AssemblyOwnership::{Unowned, Sole, Shared}`): the
  longest prefix matching any assembly name decides, with all its owners and no
  fall-through, and keeps the engine roots (`ENGINE_NAMESPACE_ROOTS`)
  unowned; `assembly_ownership` is the exact-name lookup.
- `infra::ScanConfig.active_mods: Option<Vec<ModId>>` — when `Some`,
  scans exactly that list instead of reading `ModsConfig.xml`'s own
  `<activeMods>`. The port that calls into this from `rim-session`
  (`ports::ModScanner::scan_and_analyze`'s `active_override` parameter)
  is deliberately not a defaulted trait method: a default that silently
  ignored the override would be exactly the kind of bug this parameter
  exists to make impossible for an implementor to skip.
- **Manifest.xml entry resolution** (`analysis::edges::ManifestFallbackIndex`):
  after an exact packageId and an exact display-name lookup both fail, an
  entry matches against every active mod's *normalized* display name
  (lowercased, non-alphanumerics dropped) and, when the entry is all
  ASCII digits, against each mod's own content-folder name (a bare Steam
  Workshop id, keyed on the folder — not `Mod::workshop_id`, which is
  `None` for a local copy). A digit-only entry tries the folder index
  first. Both indexes key every mod under `ModId::base()`; two genuinely
  different active mods claiming one key resolve to **neither**, with a
  warning naming both. Never loosen this: RimWorld itself never reads
  `Manifest.xml`, so these entries are author statements emitted at
  `Declared` strength, and a wrong resolution fabricates a declaration
  nobody made.
- **`ParentName` is a load-order fact, and `analysis::inheritance` is the
  only place that decides who registers a `Name`.** Registering a
  duplicate `Name` across mods is never an error at registration time and
  picks no single winner; each child independently resolves to the
  nearest registration at or before its own load position (vanilla
  fallback, or the lowest-loaded registrant if none) — ground-truthed
  against decompiled `Verse.XmlInheritance.GetBestParentFor`. Consequences:
  `EdgeKind::ParentTemplate` is `EdgeStrength::Hard` (a proven load-time
  failure, not an author's declaration); `parent_template_edges` emits
  nothing when the lookup cannot fail (the child's own registration, a
  vanilla one, or a patch-added one all count); two or more foreign
  owners is an any-of and currently produces **no edge** (a template
  any-of needs a new `Constraint` variant that doesn't exist yet).
  `PatchOp::injected_template_names` deliberately over-collects (any
  `Name` attribute in a patch op's `<value>` subtree, or an
  attribute-writing op whose `<attribute>` is `Name`) — every consumer
  only uses the set to suppress a false "unresolved parent" warning, so
  over-collecting can cost a true positive but can never fabricate a
  false `Hard` edge. An inline `Name` registration is gated on
  `MayRequire` **only** — `TryRegister` never reads `MayRequireAnyOf`,
  which gates whether the *def* loads, not whether the name registers —
  and `TemplateEntry` deliberately has no `may_require_any_of` sibling
  field to prevent a caller from reaching for the wrong gate.
- **`texPath` folder resolution mirrors `Graphic_Collection.Init` exactly**,
  which is stricter than "any folder path resolves recursively":
  `ContentFinder.GetAllInFolder` enumerates recursively, but `Init`
  rebuilds each sub-graphic's path as folder-plus-*file-name*, so a file
  in a subfolder is enumerated and then looked up at a path that doesn't
  exist. The collection model applies only to a candidate whose
  `<graphicClass>` is one of the 14 vanilla classes that inherit `Init`
  unoverridden (`extract::graphics::COLLECTION_GRAPHIC_CLASSES`); an unknown class falls
  back to the lenient rule. The class is resolved through the
  `ParentName` chain, not read off the sibling element — `XmlInheritance`
  merges a template's `<graphicData>` into its children, so most concrete
  defs inherit `Graphic_Random` from a template that declares it once.
  `collection_graphic_rows` reports the *flattened* path `Init` actually
  attempts, one row per distinct path, evaluated before the exact/direct
  short-circuits. It enumerates the union of `Indices.texture_owners`
  (loose files) and `Indices.non_loose_textures` (asset bundles plus
  Core's own resource-container index — see the `texPath`/non-loose-index
  bullet below): a subfolder file served only by a bundle still counts
  towards the folder being non-empty, and gets the identical file-name
  flattening. An enumeration empty in *every* source now genuinely
  reports a `MissingTexturePath` row — before the non-loose index
  existed, Core and the DLCs shipped no loose texture files at all
  (vanilla art comes from `Resources.LoadAll`), so every vanilla-provided
  collection folder looked empty here and reporting that would have been
  a false positive; the non-loose index closes exactly that gap, so an
  empty result now means the folder is genuinely empty everywhere this
  analyzer can see. A vanilla child never produces a `ParentTemplate`
  edge either: "mod before Core" isn't an ordering the sorter can
  express.
- **Graphic layout** (`extract::graphics`, pure; decompiled from
  `Verse.Graphic_Multi`/`Graphic_Collection`/`Graphic_StackCount`/
  `Graphic_Appearances`/`GraphicData`). `Graphic_Multi.Init` reads
  `{p}_north|_east|_south|_west`; north falls back to south, east, west,
  then the exact `{p}`; south falls back to north; east to west
  (mirrored), else north; west to the resolved east (mirrored). A
  substituted face is mirrored only when `GraphicData.allowFlip` (default
  true) allows. `Graphic_Collection.Init` takes every non-`_m` file under
  the folder (recursively, then flattened to the file name), orders it by
  name, groups by the text before the first `_`, and folds each group's
  direction-named files (a `Contains` test) into one Multi appended after
  that group's single members. `Graphic_Random` previews member 0;
  `Graphic_StackCount` member 0 is the one-item graphic (its icon uses the
  last); `Graphic_Appearances` picks, per `StuffAppearanceDef`, the first
  file ending in its `defName` (under `pathPrefix/lastSegment` when a
  prefix is set), else `Smooth`'s. Two approximations: the engine walks
  mods in load order and, within a mod, in directory order, while the
  catalog is a deduplicated, name-sorted union, so the *first* match is
  the first by name (identical when exactly one file ends in the
  `defName`); and the collection sort compares
  lowercased names where the engine's culture comparer sees the original
  case, which only changes which member 0 is shown. A null `graphicClass` on a
  `GraphicData` is no graphic at all, except a `PawnKindLifeStage`'s body
  data, which defaults to Multi. Apparel adds `_{BodyTypeDef.defName}`
  unless the *last* layer is `Overhead`/`EyeCover`, the item renders as a
  pack, or the path is a placeholder. The catalog it reads is
  `SourceIndex::textures` (loose owners in load order, plus bundle and
  Core-resource keys); `SourceIndex::kinds_by_race` is the raw
  `PawnKindDef` -> `race` declaration, so a `race` set only by a patch is
  missed.
- **The non-loose texture index** (`Indices.non_loose_textures`,
  `analysis::indices::MIN_CORE_RESOURCE_TEXTURES`) closes the texture
  existence check's biggest blind spot: a `texPath` served only by an
  active mod's own compiled asset bundle, a DLC's bundle, or one of
  Core's built-in `Resources.Load` textures. Ground-truthed against
  `Verse.ContentFinder<T>.TryFindAssetInModBundles`/
  `ModAssetBundlesHandler`: a bundle
  asset resolves at `Assets/Data/<FolderName>/Textures/<path><ext>`, or
  (non-vanilla only) `Assets/Data/<PackageId>/Textures/<path><ext>`; only
  extensionless files with no OS suffix or a `_win` suffix are bundles,
  deduplicated across a mod's own loaded folders by the identical
  same-relative-path, highest-priority-folder-wins rule `Defs/`/
  `Patches/`/`Assemblies/` already use. `extract::asset_index::resource_container_paths`
  bounded-scans `RimWorldWin64_Data/globalgamemanagers` for Core's own
  `textures/...` container-path strings — a heuristic byte scan, not a
  real asset-bundle parse, pinned by a real-install count floor
  (`MIN_CORE_RESOURCE_TEXTURES`, 1,000, well under the ~3,455 measured):
  below that floor `conflicts::textures::missing_texture_paths` disables
  itself for the whole scan rather than run against a near-empty or
  wrong-file index, and
  `analysis::checks::core_resource_index_warning` surfaces that as a
  `Warning`. `non_loose_textures` is deliberately **not** folded into
  `texture_owners`: a bundle/Core texture is never a `TextureOverride` of
  a loose file — a loose file always wins the lookup — so mixing the two
  would fabricate overrides the engine never produces. A `texPath`
  value containing `{` or `[` is a format string or encoded data, never a
  path, and is skipped as a candidate before either index is consulted.
- **`.dds` decodability** (`extract::textures::classify_dds`,
  `Conflict::UndecodableTexture`): ground-truthed against decompiled
  `Verse.ModDdsLoader.TryLoadDds`/`DdsPixelFormat.ToTextureFormat`/
  `.IsUnsupportedCompressedFormat` — a bad magic or header size throws
  `InvalidDataException`; a compressed (`DDPF_FOURCC`) format whose FourCC
  isn't one of `DXT1`/`DXT5`/`DX10` (the only three `ToTextureFormat`
  recognizes — `DX10` is the extended-header marker, which this
  decompiled engine maps unconditionally to `TextureFormat.BC7`, never
  reading the DXGI format field the extended header actually carries)
  throws `NotSupportedException` immediately, undecodable regardless of
  the image's own dimensions — confirmed directly from the decompiled
  property, not inferred; one of those three FourCCs still fails,
  separately, when its width or height isn't a multiple of 4, inside
  Unity's own `Texture2D` constructor (Unity engine behaviour,
  unconfirmable from the managed DLL alone, but matched 68-for-68 against
  a real play-test's own game log). An uncompressed format is always fine
  regardless of dimensions. The header read
  (`infra::mod_scan`, during the existing `Textures/` walk) happens only
  for a `.dds` file's own winning copy after the identical
  same-relative-path shadowing rule every other file type gets;
  `has_png_sibling` records whether this mod also ships a non-`.dds` file
  at the same normalized key, evidence only — a `.dds` shadows any
  same-key file across the whole mod regardless of whether it decodes, so
  the base game never actually falls back to that sibling. The
  "`Loading from png instead`" log line some installs show comes from a
  third-party graphics mod (the strings sit in that mod's own assembly, not the
  game's), not the base game — this finding's own wording says only that the base game shows the
  bad-texture placeholder.
- **`EdgeKind::UsesType` vs `PatchSelectsInjectedNode`**: a patch op that
  merely *selects* an injected node by path (never names a type) must
  never be labelled `UsesType` — `rim-resolve`'s `UndeclaredTypeDependency`
  finding takes every `UsesType` edge at face value as "uses a type from
  X's assembly". `PatchSelectsInjectedNode` (`Awareness`) is what a
  node-path selection producer emits instead; the class-string producer
  (`emit_patch_injected_node_edge`) is unaffected and still emits
  `UsesType`. New `EdgeKind`/`FindingKey` variants are always appended at
  the enum's end (never inserted), so no existing variant's `Ord`
  position moves, and always paired with a `REPORT_SCHEMA_VERSION`
  bump plus updates to `Edge::subject`'s own doc comment, the proptest's
  `arb_edge_kind`, and the desktop's `edgeKind.test.ts` `ALL_EDGE_KINDS`.
  A new `EdgeKind` variant also needs `rim-io`'s `decisions.rs`
  `SCHEMA_VERSION` bumped separately: an `EdgeKind`'s text form can
  appear inside a persisted `decisions.json` key (e.g.
  `edge_dropped:`/`declaration_questioned:`), and `load` treats an
  unrecognized key string as a hard, whole-file parse error — never a
  silently-ignored unknown field — so a new variant's text is exactly
  the condition that forces that bump too, not just `REPORT_SCHEMA_VERSION`.
- **`EdgeKind::PatchInvalidatesPredicate`** fires when one mod's
  `Replace`/`Remove` op deletes or rewrites a predicate step
  (`P/S[K="v"]` or that step's key child) that another mod's own op's
  `sub_path` depends on still resolving — RimWorld combines every mod's
  Defs into one document and runs every op in load order against its
  *current* state, so the dependent op must run first. A candidate `A`
  is only eligible when it does **not** itself tolerate the predicate's
  absence (`tolerates_absence`, the same "conditional guards its own
  no-op case" shape `is_genuinely_conditional_remove` checks for `B`) —
  without that check, two mods sharing the same "if it exists, replace
  it" idiom on the same predicate fabricate a mutual false cycle against
  each other. A predicate step's bracket may carry several predicates
  (`li[label="x"][foo="y"]`); a candidate `K` matches any of them, but a
  composed predicate (`and`/`or`, `contains(`, `not(`, `@attr=`) is out
  of scope for equality matching on that bracket.
- **`EdgeKind::PatchRemovedNodeCosmetic`** is `patch_removed_node_edges`'s
  own classification of a candidate `PatchRemovedNode` pair (remover `R`,
  toucher `A`) as content-preserving rather than content-changing.
  Coverage is a *filter*, not a per-op veto (a real-install correction:
  the original rule required *every* op of `A` on the shared key,
  in-region or not, to individually satisfy every condition, so a mod's
  own unrelated op elsewhere on the same def wrongly forced
  `PatchRemovedNode` on an otherwise cosmetic pair — a real install ships
  two mods both removing the identical node, one of which also has an
  unrelated `Conditional`/`Add` elsewhere on that same def): `A`'s own
  active mutating ops on the shared `(def_type, def_name, selector)` key
  are first filtered down to the ones that actually cover `R`'s own
  removed region ([`sub_path_covers`]), and the pair is cosmetic exactly
  when *every one of those covering ops* also satisfies
  `PatchOp::sequence_tail` (no later sibling in any enclosing
  `PatchOperationSequence`, seen straight through `FindMod`/Conditional
  wrappers — the abort in `Verse.PatchOperationSequence.ApplyWorker` is what
  could otherwise skip real, later work depending on the order), isn't
  reached through a `NoMatch` branch whose own Conditional tests a
  location genuinely outside `R`'s region
  (`PatchOp::conditional_branch`/`conditional_xpath`), and names exactly
  one def (`PatchOp::names_single_def`). One *covering* op of `A` failing
  any of the three keeps the whole pair `PatchRemovedNode`; a
  non-covering op of `A` neither justifies the edge nor counts against
  it at all. Three exclusions precede classification, all "never emit an
  edge at all for this remover target, cosmetic or not": a toucher
  tolerant of the remover (`analysis::edges::patches::is_tolerant_toucher`
  — a `Match`-branch op whose enclosing Conditional's own tested xpath
  equals the op's own xpath, or resolves to a site at or beneath `R`'s
  own removed region, is tolerant on its own, ground-truthed against the
  decompiled engine: `Verse.PatchOperationConditional.ApplyWorker`
  returns success for a match-only Conditional whenever the tested node
  is simply absent, `<nomatch>` present or not. A narrower third case —
  the Conditional tests the op's own xpath's *parent*, a site the
  removal doesn't itself make vanish — stays gated on
  `PatchOp::conditional_nomatch_creates` (the "if exists, replace it / if
  not, add it" idiom: the toucher's own author only planned for the
  remover running first because the sibling `<nomatch>` recreates the
  site)), a toucher whose
  own loaded folder is gated `IfModActive`/`IfModActiveAll` on the
  remover's own mod (by `ModId::base()`, `PatchOp::load_folder_gate`,
  stamped by `infra::mod_scan`) — that folder was written specifically
  for the remover's own end state, so its op already anticipates the
  removal rather than merely being disturbed by it, and — on the
  *remover's own*
  side, checked once per remover target before any toucher is even
  considered — the remover's own mod recreating the removed region in a
  *later* op of its own
  (`analysis::edges::patches::remover_recreates_region`: an exact-site
  recreate, or a parent-targeting creating op whose own `<value>` names
  the removed child, `PatchOp::value_child_names` — a real compat mod
  ships exactly this idiom, `RR.PatchOperationAddOrReplace` targeting
  the def root right after a sibling `Remove` of one of its children).
  `PatchRemovedNodeCosmetic` is `Inferred` strength like
  `PatchRemovedNode`, but ranks first for cycle-breaking within
  `Layer::Inferred` (`rim-resolve`'s `sort::cycles::kind_rank`) — the
  owner's own decision: enforced when free, dropped first when not.
- **Mod-setting-gated custom toggle classes** (`PatchOp::toggle_active`,
  `analysis::indices::patch_op_active`) are honoured the same way the
  replay already honours them, from one shared function both layers call:
  `extract::patches::toggle_default` reads the first of `<enabled>`,
  `<defaultValue>`, `<default>` a custom operation class declares (`true`
  when it declares none), and `rim_merge::patch_eval::identity::toggle_default`
  re-exports this exact function rather than keeping a second copy — the
  crate graph is analyzer -> resolve -> merge, so the shared rule lives at
  the lowest layer both can reach. Detection is purely structural, never
  by class name: `extract::patches::ToggleShape` recognizes an
  unrecognized custom class (not a built-in, not a rules-data-registered
  `CustomBehaviour`) as sequence-shaped when it carries its own
  `<operations>` child, or Conditional-shaped when it carries no
  `<xpath>` of its own alongside a `<match>`/`<nomatch>`/`<operation>`
  child — this mirrors `rim-merge`'s own dispatch order (built-in control
  flow first, this structural fallback only once none of them match; see
  `rim-merge/CLAUDE.md`'s "Custom operation classes are data, not code")
  and needs no rules-data row of its own, since the mechanism reads
  RimWorld's own patch grammar rather than any specific mod's class name.
  `WalkContext::toggle_active` narrows the same way `sequence_tail` does
  (starts `true`, only ever narrowed, never re-widened) but can narrow at
  more than one nesting level, since a toggle can sit inside another
  toggle. **Known limitation, shared with the replay**: a player who
  flipped a toggle on in-game away from its XML-declared default
  diverges from both the analyzer's edges and the replay's predictions —
  neither layer reads runtime mod settings, only the XML-declared
  default.
- **`SourceIndex::patch_ops_by_def` indexes a whole-`<Defs>`-root
  injection** (`<xpath>Defs</xpath>` or `/Defs`, no `op.target` of its
  own) under every def its own `injected_paths` names, as a fourth
  fallback after `patch_op_targets`/`parent_name_targets`/
  `child_value_targets` all come back empty — parsing each
  `"{element_tag}/{defName_text}"` entry straight off the op
  (`injected_paths_of`'s own whole-def shape) rather than re-deriving it.
  Without this, a same-mod "Remove the def, then re-`Add xpath=Defs` it"
  idiom's own recreating `Add` is invisible to this def's own replay
  contributions — `rim_merge::patch_eval::replay`'s own def-recreate
  handling (`patch_eval::standard_ops::try_recreate_def_from_document_root_add`)
  never even gets called with it, since `verify_order.rs`/`inspect_def`
  both build their contribution list by iterating `patch_ops_by_def`
  directly, not by re-scanning every op for a match — a real install
  ships exactly this idiom (a mod removes a def, then re-adds it via a
  whole-`Defs` injection) and a later, unrelated op on the same def was
  predicting a false `DeadTarget` against a tree the game itself never
  leaves empty. This is a second, deeper layer under the same real-install
  case the replay fix targets — the replay fix alone is necessary but not
  sufficient, since the recreating op has to actually reach `replay` as a
  contribution first.
- **`Conflict::BrokenInheritance`** (`analysis::inheritance::broken_inheritance`)
  replaces the old `unresolved_parent_templates` scan warning: for every
  `ParentName` reference, `resolve_parent` mirrors the decompiled
  `Verse.XmlInheritance.GetBestParentFor` exactly — a non-vanilla asking
  mod gets the registrant with the greatest load position at or before
  its own, falling back to a vanilla registrant when none qualifies; a
  vanilla asking mod gets a vanilla registrant first (any one, since
  every vanilla mod's own position precedes every real mod's), else the
  lowest-positioned registrant of any kind; a patch-injected (`mod ==
  null`) registration is the final fallback, and its own element type is
  never reported as a mismatch — `PatchOp::injected_template_names`
  deliberately over-collects, so its type is unknowable. `MissingParent`
  fires when nothing resolves at all. `affected` (every active, gate-open
  concrete def that loses inherited content) is a `children_by_template`
  walk shared with `SourceIndex::build` via
  `source_index::build_children_index` — never a second,
  independently-written index.
- **`ParentTypeMismatch` precision** (`inheritance::TypeHierarchyIndex`):
  `XmlInheritance` keys templates by `Name` alone and merges across
  element types constantly — a differing tag on the resolved parent is
  weak evidence on its own, ground-truthed against the real install (47
  of 48 real `ParentTypeMismatch` rows a naive "tags differ" rule found
  were false positives: vanilla/DLC pairs like `FleckDef`/`ThingDef` in
  Royalty, a case-only `BackstoryDef`/`BackStoryDef`, and dozens of mod
  framework subclasses like `Example.Weapons.ExpandableProjectileDef :
  ThingDef`). Three checks run before a mismatch is even considered: a
  case-insensitive tag match is never a mismatch (`TryRegister` merges by
  `Name`, not tag spelling); the asking child's own mod being vanilla
  (Core/DLC) is ground truth that the shape is fine, unconditionally; and
  `TypeHierarchyIndex::is_subclass` walks the child's own `.Extends`
  chain (built from every scanned assembly's `type_hierarchy` — see
  `extract::pe_metadata::AssemblyMetadata::type_hierarchy` and
  `ScanOutput::vanilla_type_hierarchy`, the latter read from
  `Assembly-CSharp.dll` specifically, the one `Managed/` assembly that
  declares every vanilla def-type class) to confirm or rule out a
  subclass relationship. Three outcomes: `Some(true)` (child genuinely
  extends parent) suppresses the finding entirely, safe by construction;
  `Some(false)` (the chain resolves in full, to a known BCL root —
  `System.Object` and friends — without ever matching) keeps it as a full
  `Conflict`, a confirmed, resolvable mismatch; `None` (any hop's own
  base can't be named — an inactive or unparsed mod's type, a generic
  `TypeSpec` base, or the child's own type not found in any scanned
  assembly at all) downgrades it to an informational `Warning` rather
  than fabricating a `Hard`-adjacent finding from a guess — "if you can't
  resolve a type's base, don't flag it as broken" applies literally. On
  the real install this leaves 6 (of the original 47) `ParentTypeMismatch`
  rows, every one a *confirmed* (`Some(false)`) cross-wiring, not a
  guess — none landed in the `None`/warning bucket. `TypeHierarchyIndex`
  resolves a bare XML tag (`"ThingDef"`) against a fully-qualified
  metadata name (`"Verse.ThingDef"`) by simple name when the tag itself
  has no dot, and exactly when it does (a mod's own disambiguating
  `"Example.Weapons.ExpandableProjectileDef"` tag) — see
  `xml_type_matches`'s own doc comment.
- **`Conflict::NearMissModReference`** (`analysis::checks::near_miss_mod_references`,
  `analysis::name_similarity`) flags a `PatchOperationFindMod` name or a
  `MayRequire`/`MayRequireAnyOf` id that resolves to no active mod and to
  no installed-but-inactive mod either, but closely resembles exactly one
  active mod. `MayRequire` sites are gated to the shapes the game actually
  reads (`LoadedModManager.ParseAndProcessXML`, `DirectXmlToObject.ListFromXml`):
  a def or template node directly under `<Defs>`, a `<li>` list item
  (`PatchOp::is_list_item`), and a keyed element nested anywhere inside a
  def/template's own field tree (`DefsFile::nested_may_require`) — never
  a top-level `<Operation>`'s own
  attribute or a `<match>`/`<nomatch>` node's, which the engine ignores
  for its own built-in operations (`ModContentPack.LoadPatches` never
  reads `MayRequire` off either); a custom third-party `Operation`
  subclass's own `<match>`/`<nomatch>` child is a genuinely unverifiable
  case this crate doesn't chase (it would need decompiling that specific
  mod's own bytecode to know whether its bespoke `ApplyWorker` re-reads
  that attribute itself). A def-typed scalar cross-reference field (a
  third honoured shape, via `WantedRefForObject.BadCrossRefAllowed`) is
  not modeled — this crate has no generic cross-reference field
  extractor. `FindMod` names are matched exactly and case-sensitively (a
  case-only difference is its own rule, since RimWorld's own
  `DisplayNameIndex` resolves leniently by design); `MayRequire` ids are
  matched case-insensitively and suffix-free already, so a case
  difference there is never a miss. A written value that already equals
  some active mod's own raw package id (not its display name) is never a
  near miss of that same mod — `PatchOperationFindMod` genuinely never
  matches an id against `ModContentPack.Name`, but there is no "closer
  name" to suggest for a value that already unambiguously names one mod.
- **Precision fixes for `name_similarity`, ground-truthed against a
  real-install false-positive sweep** (rules (c)/(d) in `classify`'s own
  doc comment): rule (d) (`LeadingToken`) requires the *candidate*'s own
  token count to clear `MIN_LEADING_TOKEN_CANDIDATE_LEN` (3) — a one- or
  two-word candidate (`"Core"`, or a two-word `"More X"`/`"Big X"`-family
  name) sitting at the end of an unrelated compound name is no evidence
  at all (22 of 27 real `LeadingToken` rows shared one generic trailing
  word; a further two shared only a two-word tail that was itself
  plausibly a distinct real mod, not a typo of the shorter one — every
  real confirmed typo this rule needs to keep has a candidate at least
  three words long). `tokens` drops any token with no alphanumeric
  content (a lone `"-"`), which is what let a fork-marker pair like
  `"... - Forked"` fail its own length check before this fix. Rule (c)
  (`NearMiss`) scores a written/candidate pair by edit distance over
  `near_miss_scope`'s own *scoped* segment (shared leading/trailing
  `.`/whitespace/bracket-delimited segments trimmed from both sides), not
  the whole string — a long, mostly-identical dotted id or multi-word
  name otherwise dilutes a real difference into a falsely high
  similarity ratio (a real mod-framework module id, 50 characters,
  differing only in an 8-character final word, read as 0.85+ similar at
  the whole-string level).
  The `MIN_NEAR_MISS_LEN` floor still reads the *full* id/name's own
  length, not the scoped segment's, so a short scoped difference inside a
  long, mostly-shared id (`"roaylty"`/`"royalty"`, 7 characters, inside a
  21-character id) still reaches the distance check. Differing, non-empty
  bracket content on both sides (`bracket_content`) is real identity
  information, not decoration — `normalize`'s own bracket-dropping
  otherwise made `"[AA] Example Traits"`/`"[BB] Example Traits"`-shaped
  pairs (a real author-initials-tag pair on the real install) compare
  equal under rule (b).
- **`EdgeKind::ReplaceDiscardsAddition`** (`analysis::edges::patches::replace_discards_addition`)
  closes the silent half of `PatchOperationReplace`: unlike a `Remove`,
  which at least fails visibly when the node it targets is already
  gone, a later `Replace` swaps in a whole new node and discards
  whatever an earlier active mod's own `Add`/`Insert`/
  `AddModExtension`/`AttributeAdd`/`AttributeSet` wrote into it —
  **both operations report success**, so nothing is logged either way.
  Four conditions gate the edge, all checked against `PatchOp::value_root_names`/
  `PatchOp::value_digest` (bounded structural digests recorded at scan
  time, `MAX_VALUE_DIGEST_ENTRIES`/`MAX_VALUE_DIGEST_DEPTH`): (1) the
  replacement value keeps the replaced node's own tag — a rename or
  split instead makes the addition *fail* visibly in the fixed order,
  a different, already-logged problem, so no edge or finding applies;
  (2) the earlier addition's own content isn't already present in the
  replacement at the matching relative path (a truncated digest can't
  prove that, so it's treated conservatively as "keep the edge"); (3)
  the replacer's own `About.xml` doesn't already declare `loadAfter`/
  `forceLoadAfter`/`modDependencies` on the adder
  (`DeclaredOrder::declares_after_or_dependency`) — when it does, this
  is the replacer's own deliberate choice, surfaced only as
  `Conflict::DiscardedAddition`, never an edge; (4) the two mods
  differ. Covers both the same-node case (`A`'s own sub_path equals
  `P`) and the ancestor case (`P` is a real path-segment ancestor of
  `A`'s own sub_path, via the same [`sub_path_covers`] test
  `patch_removed_node_edges` already uses for its own remover/toucher
  region) — `Edge.after` is the **adder's** own mod (it must load
  *after* the replacer, the opposite direction from `PatchRemovedNode`,
  since the addition has to land inside the fresh replacement rather
  than being replaced along with the old subtree).
- **The predicate-keyed `<li>` shape closes a real gap in the
  injected-node passes** (`extract::patches::injected_li_predicate_paths_of`,
  `analysis::edges::patches::li_predicate_lookup_keys`): `injected_paths_of`'s
  own element-injection shape drops any path ending in an unaddressable
  bare `li` segment, and never reads a `PatchOperationReplace` at all —
  so a predicate-selecting op (`li[@Class="…"]`, the common "typed list
  item" idiom, or a bare-text `li[text()="…"]`, the "plain defName
  reference" idiom) whose target a *different* mod's `Add` or
  node-keeping `Replace` creates got no edge from either shape. Two
  producer cases, each recording a *normalized* key (`li[@Attr="v"]`/
  `li[text()="v"]`, always double-quoted regardless of the author's own
  XML) so quote-style differences between the two mods' own files never
  hide a real match: a direct `<li>` child of an `Add`/`AddModExtension`'s
  own `<value>`, or a `<li>` one level below a `PatchOperationReplace`'s
  own kept child (the same "value keeps the node" rule
  `EdgeKind::ReplaceDiscardsAddition` above uses).
  The consumer side (`patch_injected_node_edges`'s own second pass)
  tries the normalized predicate key *in addition to*, never instead
  of, the ordinary `target.match_key()` lookup, so it costs nothing when
  a selecting op's own sub_path doesn't end in a recognized predicate
  shape. Deliberately narrow (direct children only, one level, `Class`
  attribute or bare text only) to keep this precise rather than
  flooding the index with every nested list in a large injected
  subtree — a missed match only costs a missed `Hard`/`Awareness` edge,
  never a fabricated one.
  The pre-exists-inline guard has the matching inline-side keys:
  `extract::defs::collect_inline_node_paths` also records every inline
  `<li>` under `{prefix}/li[@Class="X"]` or `{prefix}/li[text()="v"]`,
  built with the same `li_predicate_identity`/`li_predicate_suffix` the
  patch side uses, so a predicate-li edge whose node a Def already ships
  demotes to `PatchSelectsInjectedNode` instead of staying `Hard`. The
  inline key ignores `MayRequire` (extract has no active-mod set); a
  gated item counts as present, which only loses a `Hard` edge. The
  inline index is consulted only for a key carrying
  `InlineCheck::Applicable` — the selecting op's **last** segment with
  exactly one predicate bracket; a multi-predicate step (the key keeps only
  the first recognized predicate) and a read-through step stay `Hard`
  (`InlineCheck::NotApplicable`). Text identity is verbatim (no trim) on
  both sides, like the game's `text()="X"`. Known gap: a `<li>` nested in
  an inline `<li>` is keyed under a plain `li`, so a patch-side
  `li[@Class="X"]/things/li[...]` never meets it.
- **A node-keeping `PatchOperationReplace` is also a remover, of
  everything strictly below the node it keeps** (`analysis::edges::patches::patch_removed_node_edges`'s
  own second, `Replace`-remover pass, sharing every piece of the
  original `Remove`-remover pass's machinery — the touchers index, the
  cosmetic/content split via `toucher_op_is_cosmetic`, the tolerant-toucher
  and folder-gate exclusions, `remover_recreates_region`, and
  `patch_op_active`'s own `toggle_active` gating — rather than a second,
  parallel mechanism). Ground-truthed against the decompiled engine
  (`Verse.PatchOperationRemove`/`Replace`/`Insert.ApplyWorker`: all three
  are an `xml.SelectNodes(xpath)` over a node that must already be
  there, and `PatchOperation.Complete` logs `Log.Error` when an op never
  once resolved it) that a `Remove`/`Replace`/`Insert` reading or
  rewriting something that only existed in the replaced node's *old*
  subtree fails, visibly, exactly the way it would against an outright
  `PatchOperationRemove` — the real-install case this closes: mod B
  replaces a stove's whole `researchPrerequisites` list with its own
  content; mod A's `Remove` of a specific, predicate-keyed prerequisite
  from the *original* list only succeeds when A runs first. Candidate
  touchers here are restricted to `READING_CLASS_SUFFIXES`
  (`Remove`/`Replace`/`Insert`), never the `Remove`-remover pass's own
  unrestricted "any mutating op" set, which is also what keeps this pass
  class-disjoint from `EdgeKind::ReplaceDiscardsAddition`'s own pass
  (`is_additive_class`/`ADDITIVE_CLASS_SUFFIXES`): `Add`/`AddModExtension`/
  `AttributeAdd`/`AttributeSet` stay that pass's own domain (their real-install
  usage is overwhelmingly the *same-node* case, where the node they
  require is the one the replace keeps, always present regardless of
  order), while `PatchOperationInsert` moved here entirely — unlike
  those four, an `Insert`'s own required xpath is inherently a sibling
  *inside* the container, never the container itself, so it is exactly
  as exposed to the old subtree being wiped as a `Remove`/`Replace`
  toucher is. No op class is ever a candidate for both passes, so the
  same op can never source edges in opposite directions for the same
  pair (a *different* op of the same two mods still can, and that's a
  genuine, both-true ambiguity `Layer::Inferred`'s own cycle-breaking
  already resolves, same as any other same-layer disagreement). The
  opposite direction — the replacer's own new value recreating the
  exact predicate-`<li>` a toucher needs — is excluded outright
  (`replace_recreates_touchers_target`, reusing `li_predicate_lookup_keys`
  against the replacer's own `PatchOp::injected_paths`, the identical
  normalized identity the predicate-`<li>` shape above already computes
  for the opposite-direction case), so the two mechanisms never emit
  contradictory edges for a pair where both could apply.
- **`Conflict::DanglingDefReference`** (`analysis::references::dangling_def_references`,
  the pure half; `infra::explain_dangling_references`, the lazy IO half)
  is a vote-based check, not a hand-written field table — RimWorld's own
  Def-typed field vocabulary is open and per-mod-extensible, so a fixed
  table can't cover it. `extract::ref_sites::collect` (shared by
  `extract::defs` for a def/template's own field tree and
  `extract::patches` for a mutating op's own `<value>`) walks every
  field, recording a candidate under **two** shapes at once for a
  non-`li` leaf — `RefSiteShape::Scalar` under the leaf's own full path,
  and `RefSiteShape::KeyedElement` under the *parent* container's path
  with the leaf's own tag as the value — since which one (if either) is
  a real reference field is a vote, never something the structural walk
  alone can decide; a `<li>` leaf is `RefSiteShape::ListItem` only.
  `descriptionHyperlinks` is a fourth, fixed shape (confirmed
  engine-typed, `DescriptionHyperlink`'s own XML loader): every child's
  own tag names the def type, its text the def name, no vote needed.
  `analysis::references::is_reference_field` trusts a `(def_type,
  field_path, shape)` group once at least `MIN_RESOLVED_DISTINCT` (5,
  shared with `rim_resolve::domain::assignment`'s identical constant —
  see [`crate::domain::MIN_RESOLVED_DISTINCT`]'s own doc comment for why
  the lower layer owns it) of its own distinct values resolve, at 90% or
  more of every distinct value seen — except a `List`/`Scalar` field
  whose own leaf tag ends `Defs`/`Def` respectively, where the whole
  rule collapses to "at least one resolved value", no floor and no
  ratio (the identical full-bypass shape the assignment domain's own
  `Def`-suffix exemption already uses, not merely a lowered floor still
  gated by the ratio).
  A dangling name's cause: **`RemovedBy`** is decided purely, from an
  active, unconditional-or-"if exists" whole-def
  `PatchOperationRemove`/`Conditional` (`is_genuinely_conditional_remove`,
  reused from `analysis::edges::patches`) — collapsed to name-only, since
  `PatchOperationRemove.ApplyWorker`'s own `xml.SelectNodes(xpath)`
  removes every node the xpath matches in the whole combined document,
  not just one mod's own copy. Every other cause needs the lazy pass:
  `OnlyInUnloadedFolder` (an active mod's own directory listing minus its
  `Mod::loaded_folders`) and `OnlyInInactiveMod` (`ScanOutput.inactive_mods`)
  both byte-search a candidate `Defs/**/*.xml` file for `>NAME<` before
  ever parsing it, confirming a hit with a real `defName` parse;
  `DefinedNowhere` is what's left once the search completes in full;
  `Unexplained` is what's left when `MAX_EXPLAIN_BYTES` (512 MiB, shared
  across the whole pass, not per name) runs out first — a materially
  different claim ("not proven either way", not "proven absent"), so the
  two must never collapse into one. Case-agnostic on purpose (typing a
  field from ECMA-335 metadata is a later, larger tier): a name that
  exists under the *wrong* def type reads as resolved here.
  **The implied-def filter** (`analysis::references::is_implied_name`,
  `RimWorld.DefGenerator.GenerateImpliedDefs_PreResolve`) checks a
  candidate name against the active-name set by prefix/suffix before
  ever calling it dangling: `Blueprint_`/`Blueprint_Install_`/`Frame_`
  (an active ThingDef/TerrainDef), `Corpse_`/`Meat_`/`Administer_`/`Make_`
  (an active ThingDef), `Techprint_` (an active ResearchProjectDef — not
  separately type-checked in this tier, matching the type-agnostic
  design elsewhere), `_Rough`/`_RoughHewn`/`_Smooth` (stone terrain, an
  active def),
  `GeneticChemicalDependency_`/`Trainable_`/`Psytrainer_`/`Neurotrainer_`
  (an active def), the generated-gene shape (`A_B` where `A` is an active
  `GeneTemplateDef`, or a mod's subclass of it, and `B` an active def,
  tried at every `_` position) and
  the generated-carpet shape (an active `TerrainTemplateDef`, or a subclass
  of it, name directly followed by an active `ColorDef` name, no separator:
  vanilla data references `CarpetSandstone`). A subclass is any tag whose
  last `.`-segment ends with the base type (`MorphGeneTemplateDef`,
  `Example.MorphGeneTemplateDef`), and a template or colour removed by a
  whole-def `PatchOperationRemove` generates nothing. The gene and carpet shapes are typed so two
  unrelated defs sharing a prefix do not read as generated. A
  `descriptionHyperlinks` site keeps its owner's `def_type` like every other
  shape (the child tag, the target's type, is not recorded: resolution is
  type-agnostic). `RefSiteOwner::Patch` sites are self-contained
  (`extract::patches::PatchValueRefSite`) rather than a field on
  `PatchOp` itself — like `ScanOutput::child_value_hashes_by_mod`,
  `PatchOp` is a `pub` domain type well over a hundred call sites across
  this workspace construct as full struct literals, and a required field
  there would ripple into every one of them for a feature most have no
  reason to touch; `infra::mod_scan` is the one place that knows the
  owning mod id and combines a site's own relative `field_path` with its
  op's resolved `sub_path` (`combine_sub_path`) into the same
  def-relative convention a def-tree `RefSite::field_path` already uses.
