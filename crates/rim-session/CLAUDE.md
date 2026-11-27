# rim-session

Application layer: the `Session` aggregate, one use case per struct with
constructor-injected ports, the `FindingIndex`, and the ports (traits)
that `rim-io` implements. No filesystem, no XML, no Tauri.

## Layer

Sits above `rim-merge` and below the two interface apps in the crate
graph; `rim-io` depends on this crate for its port trait definitions, not
the other way around.

## Invariants

- `Session` is single-writer and lives behind a lock in the desktop app;
  every *mutating* method takes `&mut self` — a plain getter (`sources`,
  `report`, ...) takes `&self`, same as any other type.
- Every mutating use case snapshots the state it changes, persists
  through its port, and restores the snapshot when the save fails. Keep
  that shape for new use cases and add the matching `fail_next_save()`
  test with the fakes in `test_support` (feature `test-support`). The
  one exception, and it must stay one: `Session.working: ActiveSet` (the
  in-memory, unsaved active-mod list — see "Activating and deactivating
  mods" below) is the only mutating state a use case in this crate
  carries without persisting immediately. Its edits also reach
  `orders.current` in memory, never on disk: an edit that leaves the same
  ids settles Current onto `working`'s sequence
  (`Session::settle_current_onto_working`, through
  `current_order_replaced`), so Current is the second thing a use case
  changes without persisting. `file_active_mods` stays what the file held (the live baseline, see
  "The file baseline is live" below).
- **`user_path::UserPath` is the one parser for a path from outside the process** (pure: text in,
  validated path out, no file-system call). Text starting with two separators is classified by
  our own rule before `std` is asked (`classify_double_separator`; `std` reads `\\?\C:/x` and
  `//?/C:/x` differently per toolchain): a `?` or `.` then a separator or the end opens a namespace
  in any separator spelling, only the exact `\\?\<letter>:\` is a drive path (a `/` anywhere
  or a `.` or `..` component after it, split on `\`, is `DevicePath`), a `UNC` name (any case) is `NetworkPath`,
  every other namespace form (`\\?\C:/x`, `//?/C:/x`, `\\?/C:\x`, bare `\\?\C:`, `\\.\pipe\x`,
  `\\?\GLOBALROOT\x`) is `DevicePath`; other double-separator text is a server name and `std`
  reads it (UNC and WebDAV are `NetworkPath`). Everything else is checked by its `std` prefix
  (`Disk` only). A legacy DOS device name as any component (`is_reserved_device_name`: `CON`,
  `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, superscript digits, `CONIN$`/`CONOUT$`, any
  case, extension or trailing spaces) is `DevicePath`, so refusing a share never contacts its host (see
  `docs/privacy-and-network.md`). It lives here, not in `rim-io`, because it is pure and every
  interface (and `rim-io`) may use it; the desktop applies it at every command boundary, the CLI
  deliberately does not (its paths come from the local user, there is no webview and no session
  lock to hold). Its Windows prefix tests are `cfg(windows)`: elsewhere `std` does not parse those
  prefixes. It cannot see a symbolic link that leads to a share.
- `CopyOrganization::read_source` + `preview_loaded`/`execute_loaded` split the source read from the
  planning, so the desktop reads outside the session lock (the composed `preview`/`execute` check
  that the organization is available before they read); `ImportGameLog::read(path, formats, kind)`
  + the free `attribute_game_log` do the same for a log, and `ImportRimSort::read` + `apply` for a
  RimSort import. `preview`/`execute` stay as the composed forms.
- Declared ids never carry `_steam`; compare through `ModId::base()`.
  `ActiveMods` is the single authority for "is this id active".
- Ports return typed errors; `Apply` writes `ModsConfig.xml` only when
  asked and always saves decisions and rules first. It writes the order
  named by `ApplyOptions::source`, never `Session::selected()`: the
  caller states which order the user confirmed, so a selection that has
  since moved cannot change what lands on disk.
- `Session::select` is the interface's choice, not a scan result, so
  `Rescan::execute` carries the old session's selection onto the
  brand-new `Session` it returns (`Session::new` itself still starts on
  `OrderSource::Current`, which the CLI's `ledger`/`defs`/`patch` rely
  on). A composition root that rebuilds a session some other way does the
  same.
- **A use case that reads many def sources reads them through one call
  view.** `DefSourceReader::call_view` (default `None`) lets a reader hand
  out a view for one use-case call; `call_reader::CallReader::open` takes
  that view or falls back to the reader itself. `VerifyOrder` (the whole
  pass, counterfactual included), `InspectDef` (a build on a cache miss),
  `RenderMergeMod` (its own planner) and `Session::prefetch_clean_merge_previews`
  (on the session's reader) each open one per call and drop it on return;
  the `Arc<T>`/`&T` forwarding impls must forward `call_view` too, or the
  desktop's `Arc<dyn DefSourceReader>` silently loses it. What a view may
  trust is the adapter's business (`rim-io`'s view checks each file on disk
  once per view, see `crates/rim-io/CLAUDE.md`): an edit made during a call
  may be seen only by the next call. Every other read (the lazy
  `redecide_*` passes behind `Session::resolution`, `PlanMerge::execute`,
  the assignment use cases) reads the reader directly and keeps its rule.
  `test_support::CallCountingReader` counts views and reads for tests.
- `PreflightApply` is read-only (no ports): it lists
  `rim_resolve::preflight` problems for the order named by `source`, through
  `Session::hard_problems` (the ledger cache is built lazily, hence `&mut`).
  `ApplyPreflight::requires_confirmation` is derived from the items and
  `outside_change`, never stored. `outside_change` is
  `Session::external_file_change()` (the live baseline against the acknowledged
  list) but `None` when `file_holds_exactly(source)` (the file already holds exactly that order, the merge mod included): it names only what writing
  `source` would overwrite. The use case reads the baseline as it is; a caller
  wanting the file now refreshes it first (the desktop does, see its CLAUDE.md).
  The CLI loads a fresh session, so its preflight is always `None` there. `Session::file_matches(source)` compares that order with the
  file's live baseline list, ignoring the generated merge mod's package id on
  both sides.
- **Module layout**: a facade file keeps the public API (and usually the
  use case itself) and re-exports its children from a same-named
  directory. `use_cases/inspect_def.rs` is a facade over
  `inspect_def/{inspection,targets,patchers}.rs`,
  `use_cases/plan_merge.rs` over `plan_merge/{context,owners,state}.rs`
  (it re-exports `stored_choices`/`build_owner_versions` as
  `pub(crate)`), `use_cases/export_assignment.rs` over
  `export_assignment/{outcome,validation,rendering}.rs`, and `changes.rs`
  (which keeps `query`/`search`) over `changes/{rows,conflicts,
  inventory}.rs`. `session.rs`, `def_conflict_view.rs`,
  `use_cases/verify_order.rs`, `ports.rs`, and `test_support.rs` have
  the same shape. A facade's former inline tests live in one sibling
  `<facade>_tests.rs`, declared with
  `#[cfg(test)] #[path = "…"] mod tests;`.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rim-session --all-targets --all-features -- -D warnings
cargo nextest run -p rim-session --all-features
```
Before reporting done, run the full gate block in the root `CLAUDE.md`.

## Cache invalidation — the two families

Every cache in `Session` falls into exactly one of two families, and
mixing them up is the most common way to introduce a stale-read bug
here. The one exception is `Session.def_index` (`Session::def_exists`, the
def index a replay's existence test reads): it is built from `sources`
alone, which never changes after construction, so it is built once on
first use and never invalidated.

- **The ledger family** (`Session.ledgers`, `MergeWorkspace`'s per-slot
  previews, `Session.scoped` patch ledgers): any decision, rule, tag,
  settings, or import change invalidates both profile ledgers and every
  merge preview via `invalidate_ledgers`; a decision only triggers a
  resort when `DecisionSet::sorter_overrides()` differs before/after.
  `Session.scoped` is a `HashMap` (`OrderSource` has no `Ord`), fine
  since it's a point-lookup cache, never iterated for output.
  `MergeWorkspace` caches per `(slot, key, choices, scope)`, not just
  `(slot, key)` — sound only because every *other* preview input already
  invalidates the whole workspace on change, so **never widen
  `PlanMerge::build_preview`'s own inputs without also widening what the
  cache records**. A `Merge`/`ShipAsset` decision invalidates only its
  own finding's cached preview (`invalidate_ledgers_for_merge_decision`),
  provably safe since such a decision can never change a *different*
  finding's diff and never touches a patch's own scoped previews (those
  invalidate independently, via `DecidePatchMerge`).
- **The selected-order family** (`Session.inspections`,
  `assignment_instances`, `assignment_coverage`, `def_graphics`): cleared only by
  `Session::recompute_sort` and `Session::current_order_replaced` (the
  one helper behind every replacement of `orders.current` after load:
  `set_current_order` and the edit settle `settle_current_onto_working`
  both call it) — **never** by
  `invalidate_ledgers`, since nothing these caches read (the selected
  order, active mods, raw XML on disk) is touched by a plain decision/
  rule/tag/settings/import change. **Exception**: `DefInspection.findings`
  — a finding's `ResolutionStatus` *does* change on a non-resorting
  decision, so `InspectDef::execute` always recomputes that one field
  fresh (`Session::refresh_inspection_findings`), cache hit or not.
  `Session.inspections` is bounded at `MAX_CACHED_INSPECTIONS` (256): the
  whole map clears once inserting would exceed it, rather than
  LRU-evicting (an inspection can be built for any def/template in the
  install, unlike a merge preview, which the ledger's own finding count
  already caps). `Session.def_graphics` (what a def's texture resolves to)
  is bounded the same way at `MAX_CACHED_DEF_GRAPHICS` (1,024), so a
  refetch stays cheap after the smaller inspection cache has been thrashed.
- **Working set == current set implies working sequence == current
  sequence, and the settle has two directions.** Which one runs depends on
  who is the truth:
  - **File truth: `working` takes Current's sequence**
    (`settle_working_onto_current`, private to `session/order.rs`). It runs
    at the end of `set_current_order`, i.e. for `Apply`, `ExportPatch`'s
    install and `RevertOrderToFile`. A rescan scans `working`'s sequence,
    so without it an `Apply` followed by a rescan scanned the pre-apply
    order and reverted Current.
  - **Edit truth: Current takes `working`'s sequence**
    (`Session::settle_current_onto_working`, `pub(crate)`). It runs after
    every edit made from the Mods page (`MoveMods`, `ActivateMods`,
    `DeactivateMods`), because the edit is the user's intent and must not be
    undone. It writes nothing: `file_active_mods` stays what the file held,
    so `file_matches(Current)` turns false until an `Apply`. It is
    `current_order_replaced`'s one caller besides `set_current_order`: both
    invalidate the ledger family and the selected-order caches and, under
    `PreserveCurrent`, re-sort (a settled sequence change always keeps the id
    set). Never duplicate that logic in a use case.

  Both directions do nothing unless the id multisets are equal (duplicates
  respected). A `working` with pending membership edits keeps its own
  sequence in both; the next rescan (`execute_with_active_set` over
  `working().ids()`) carries a staged move into Current, and a move made
  while a rescan runs makes that rescan refuse (the swap compares the
  sequence). `is_stale` stays set-based, which is safe now because a
  same-set reorder settles at once and so is never "pending but invisible".
- **`ActivateMods::execute` places a mod without an anchor at its Current
  slot** (`ActiveSet::activate_at_reference_slots`): right after its nearest
  predecessor in Current that is in `working`, or at the start; a mod Current
  never held goes to the end. That keeps "deactivate then reactivate puts
  it back and Current is unchanged", which the edit-direction settle would
  otherwise turn into an appended order. The one exception is the dependency
  floor (`after_declared_dependencies`): a reactivated mod lands right after
  any declared dependency that is active now and sits after its old slot, so
  the round trip can change Current in that case. `execute_at` takes an `Anchor`
  instead and inserts the whole plan (dependencies first) as one block. The
  un-anchored placement keeps dependencies first too: a mod never lands before
  a declared dependency that is in `working` by then, whether this same call
  inserted it or it was already active (for example after being moved behind
  the mod's old slot); it goes right after the dependency when its own slot
  would put it earlier.
- **`MoveMods` is the one reorder use case.** `ActiveSet::move_to(ids,
  anchor)` is a pure permutation with `Anchor::{Before, After, Start, End}`
  (ids, never indices, so it is exact under a filtered or sorted view); it
  validates everything before mutating (`MoveError`: a stale anchor, an
  anchor inside the moved set, an inactive or repeated id) and keeps the
  moved ids together in their current relative order. `MoveMods::execute`
  then settles (edit direction) and, when the working sequence really
  changed while Suggested is selected, `select(Current)`s and says so in
  `MoveOutcome::did_switch_to_current`: the same switch `ImportOrder::finish`
  makes, through the one `Session::switch_to_current_after_reorder`. An
  *anchored* `ActivateMods::execute_at` (an explicit drop position) switches
  the same way when it added something (`ActivateOutcome::did_switch_to_current`);
  an un-anchored `execute` never does. `DiscardPendingChanges` is `working := Current` (a settled reorder
  stays, it lives in Current); `RevertOrderToFile` is `set_current_order(file
  sequence)` and refuses (`RevertOrderError`) while `is_stale` or when the
  file and Current hold different ids. None persists anything, in keeping with
  the `working` exception above. `ModList::reorderable` is derived in
  `mod_lists` (the Active list in `ListSort::is_load_order`, never Inactive):
  a filter keeps it on, because a move names a neighbour by id.
- **"Mods that move" is derived on read.** `Session::moved_mods` measures
  `suggested` against the live `current`
  (`rim_resolve::sort::compute_disturbance`); `SortOutcome.stats` is the
  snapshot from the last sort and goes stale at `set_current_order`, so the
  desktop dashboard never reads it (the CLI's `sort` output still does).
- `set_current_order` re-sorts (`recompute_sort`) only when the id set is
  unchanged, Current's sequence changed and `tie_break` is `PreserveCurrent`
  (Suggested ranks by Current's positions there). A changed id set never
  re-sorts: a just-written id is not in the report yet.
- `Session::set_current_order` is the way anything that **wrote**
  `ModsConfig.xml` changes `orders.current` after load — `ExportPatch`'s
  own `install` and `Apply` both call it right after writing the file. The
  one other writer of `orders.current` is the edit settle
  (`settle_current_onto_working`: a Mods-page edit, nothing written);
  both go through `current_order_replaced` and neither may skip it.
  Skipping `set_current_order` after a write is a real bug: a later `Apply` with `selected() == OrderSource::Current`
  and no reload in between would read the stale `current` and silently
  overwrite the just-written file with it.

## Def inspection and merge preview (`InspectDef`, `PlanMerge`, `def_conflict_view`)

- `SourceIndex.defs`/`ops_by_mod` (`rim-analyzer`) key on the **raw**
  `ModsConfig.xml` id, never `ModId::base()` — resolve every raw id
  sharing a base (`changes::active_raw_ids`, reading `report.mods`)
  before indexing into either map. `ModId::base()` alone is only correct
  for comparing against `owners_by_def`/`templates`/`Report::conflicts`
  owner lists, which are keyed loosely enough to compare through
  `.base()` directly. Getting this backwards silently drops a
  `_steam`-suffixed mod's own changes/patch ops.
- `def_sources::template_chain` (used by `InspectDef`) is **deliberately
  separate** from `template_set` (used by `PlanMerge`): `template_chain`
  stops the walk silently on a gap, since `InspectDef`'s whole point is
  to *show* a def with a broken chain, not fail loudly on one; `template_set`
  errors, since `PlanMerge` cannot build a preview over a chain it
  can't fully resolve. **Do not refactor the two to share a walk** — that
  would flip `PlanMerge`'s already-tested error behaviour.
- `DefInspection.children: Vec<(ModId, DefRef)>` — never assume every
  child is a concrete `DefName`: a template's direct child can itself be
  another `Name`-only template with no `defName` of its own (a large
  share of Core's own `Name` declarations are exactly this shape).
  `inspect_def::child_ref(sources, key)` is the **one** place that
  decides: `owners_by_def` carrying the `(def_type, name)` key means some
  active mod registers it as a concrete def (`Selector::DefName`);
  otherwise it's `Selector::NameAttr`. Every site that builds a child
  `DefRef` goes through this helper — never reconstruct one by hand.
- `def_sources::nearest_owner`/`Asking` implement the real
  `Verse.XmlInheritance.GetBestParentFor` rule (ground-truthed against
  the decompiled engine): there is no registration-time winner — each
  child resolves to the nearest registration at or before its own load
  position (vanilla fallback, or an inverted rule for a vanilla child).
  `resolve_name_only_def_type` (a *different*, unrelated heuristic: which
  `def_type` an untyped `DefRef::name_only` ref should bucket under for
  display) picks whichever candidate type's earliest owner sits soonest
  overall — this is **this crate's own arbitrary tie-break**, not a claim
  about engine behaviour; don't conflate the two.
- `Patcher::ops`' per-op `class`/`xpath` come from the *shallowest*
  indexed op sharing a top-level `<Operation>`'s ancestor
  (`representative_op`) — exact for a bare mutating op, approximate for
  one wrapped in a `Sequence`/`FindMod`/`Conditional`;
  `PatchOpSummary::is_wrapped` flags the approximate case so a caller
  doesn't have to guess. **`Patcher::replay`/`reached`/`caveats` are
  *derived* from `DefInspection.effective`'s own fold, never a second,
  standalone `patch_eval::replay` call** — a standalone replay can
  disagree with the real, full-order one (an earlier mod's patch can
  change the tree an isolated replay never sees).
- `Session::def_conflict_view` is a **pure join over two already-cached
  results** (`DefInspection` and `MergePreview`) — never a fresh replay,
  never new IO. A caller must already have run `InspectDef::execute`
  (and, for `DefOverride`/`PatchCollision`, `PlanMerge::execute`) or it
  returns `NotInspected`/`NotPlanned`. Every field the collision's mods
  touch beyond the contested one is read straight off
  `EffectiveDef::provenance`, never a second replay; `values`/
  `FieldRowKind` come from `DiffClass`'s own `by` set; both use
  `PatchCollisionOutcome::fields` verbatim — `rim-merge` is the single
  source of a collision's per-mod candidate trees (see
  `crates/rim-merge/CLAUDE.md`), never rebuilt here.
- **`DecideMerge`/`DecidePatchMerge` reject any `MergeChoice::Drop` on a
  `FindingKey::PatchCollision`** before validating owners or persisting
  anything — RimWorld's own patches replay to a value; there's no "drop"
  a collision's replay can express. A `DefOverride`'s `Drop` is
  unaffected.
- `diff::structural_change` (mechanism in `crates/rim-merge/CLAUDE.md`)
  is wired into `PlanMerge::plan_def_override`: when it fires,
  `state_from_plan` forces `MergeState::NeedsFieldInput` **outright**,
  before the ordinary unresolved-fields check — never a per-`FieldDiff`
  reclassification, since a `ParentName`-only trigger is never a
  `FieldDiff` at all. The guard holds even over a fully-resolved
  `Action::Merge` decision, which then previews `NeedsFieldInput` and is
  never rendered by `RenderMergeMod`/`Apply`/`ExportPatch` (all three
  only render `Complete`). `MergePreview::structural_guard_field()` is
  what `redecide_clean_merge_at`/`merge_coverage` both read to phrase
  this without duplicating the predicate.

## `def_graphic` (what texture a def shows)

`use_cases/def_graphic.rs`: the core is pure (`plan_slots`,
`build_set`, `resolve_def_graphic`; no ports, no `Session`), and two use
cases wrap it. `ResolveDefGraphic` gathers the facts through `InspectDef`
(the effective tree of the def, and only the related defs its rules can
use: `BodyTypeDef`s in registration order, `ApparelLayerDef`s with
`isUtilityLayer`, `StuffAppearanceDef`s, and the `PawnKindDef`s listed in
`SourceIndex::kinds_by_race`) and caches the result on the session.
`ReadDefTexture` re-resolves the def and refuses any `TextureKey` its
resolution did not produce (`KeyNotInGraphic`), so a forged key reads
nothing; for a loose face it locates the file through `AssetLocator`, and
for a `.dds` it answers `UndecodableInGame` (a report conflict),
`Image { from: PngSibling }` (via `locate_non_dds_texture`) or
`DdsNotPreviewable`. Registration order is owner position in the selected
order, then file, then document position. `plan_slots`
reads a def's **effective** `FieldTree` into `SlotSpec`s by field path
(never by element tag), and `build_set` expands them over
`SourceIndex::textures` and the selected order into `GraphicSet`s. The
value types make bad states unbuildable (`TextureKey::parse`, a `Faces::Multi`
with four faces, a `GraphicSlot` with variants and an in-range default, a
`GraphicSet` with slots and an in-range default view). Rules and sources
are in `crates/rim-analyzer/CLAUDE.md` ("Graphic layout"). Facts from outside the def are inputs, not lookups: the body types
in registration order (the default body type is the first one with a loose
file), the
utility apparel layers, the `StuffAppearanceDef` list, and the pawn kinds
that may use a race (their effective trees; a kind whose `race` names
another def is ignored). A humanlike race (`race/intelligence` is
`Humanlike`) that yields no slot at all is `ComposedAtRuntime`; one that
yields a probe slot is shown. Probe slots are approximate and last, and are
never the default while an engine slot exists. At most `MAX_SLOTS` (16) slots
and 64 collection members are listed, with the rest counted.

## Rules, tags, and the generated merge mod

`Tagging::accepted(threshold)` feeds the sorter; the raw `Tagging` feeds
the ledger and the rules page — pass the wrong one and a tag either
fails to move a mod it should, or the rules page stops showing a
low-confidence tag worth reviewing. `Settings.tie_break`/
`use_imported_pairs`/`use_imported_placements` feed `compute` directly;
the two toggles run `settings::filter_imported_rules` (`pub`, so
`apps/cli` reuses the identical filter) over the rule set before the
synthetic merge-mod placement is merged in — `RuleOrigin::UserDecision`
(including a promoted copy) and `Rule::Incompatible` are never filtered
by either toggle. `Session::remove_rule_matching` is origin-aware:
`origin: None` removes every rule at a `RuleKey`, `Some(origin)`
restricts to that origin's row — needed once `promote_imported_rule`
leaves an imported rule and its promoted `UserDecision` copy sharing one
key, so deleting one doesn't wipe the other. `active_mods_by_base_id()`
(base id -> `Mod.workshop_id`) exists for `rim-io`'s SteamDB importer;
every other caller still uses the plain-id `active_base_ids()`.

`SetManualTags` (the Mods page's bulk Tags action) is the bulk sibling of
`SetManualTag`: one rules snapshot, one save, rollback on failure, and **no
`recompute_sort`** — a tag-only change cannot move a mod, so
`Session::set_manual_tags` re-infers the tagging through the same
`infer_session_tagging` the full `compute` uses and drops only the ledgers
(their `TagInferred` findings read the tagging). Its `Remove` acts only on mods
that currently *show* the tag (manual or inferred): it deletes a manual `Add`
override and records a manual `Remove` only if inference would still show the
tag, so no dead `Remove` override is written; the rest count as unchanged. The
rollback is `restore_manual_tags` (tagging refresh only), never
`restore_rules`, which would re-sort and could replace Suggested. Tests pin that
the sort outcome and both orders are unchanged.

The generated merge mod's id is derived from the profile hash
(`GeneratedModIdentity::for_profile`), **never stored** — `Session::compute`
adds its synthetic Bottom placement rule whenever the id is already in
the current order or any `Merge`/`ShipAsset` decision exists, so a
project with no merge activity never carries a placement rule for a mod
that doesn't exist yet.

## Mod knowledge is data on the `Session`

`crate::ModKnowledge` carries the precedence map, the patch-operation
behaviour table, the def-cache-carrier list, and the log shapes (the
game-log line formats mods print: `ports::LogShapes`, parsed into
`LogTemplate`s that are checked against their bounds and required captures
at construction, so a shape that exists is valid) — read once at load time
and hung off the aggregate via `Session::set_mod_knowledge` (set right
after construction, same pattern as `set_rule_load_warnings`).
`LoadProject`'s `ModKnowledgeStore` port is infallible: an unreadable
cache falls back to the vendored defaults with a warning;
`source_enabled` gates **consumption**, not just fetching.
`ModKnowledge::log_formats()` (carriers plus shapes as `ports::LogFormats`)
is what `ImportGameLog` hands the `GameLogReader` per read, never adapter
state. **Tag rules
are the one section this never loads** — a tag rule can move a mod, so
reading one from the fetched cache would break the guarantee that no
sort path touches the network or its cache; tags stay on the RimSort
import model instead. `NetworkPolicy::fetch_rimmerge_rules` (app-global,
`AppSettings`, not per-profile `Settings` — see the Notifications and
network policy section below) never joins `SortProvenance`, for the
same reason the other network settings don't: none of these sections
can change what a sort produces.

## Notifications and network policy

- **`AppSettings`** (`<base>/app-settings.json`, app-global, one per
  machine) carries `NetworkPolicy` (the master `allow_network` switch,
  `check_for_updates`, `auto_refresh_rule_databases`, and each source's
  own `fetch_*` toggle) and the staleness-reminder threshold — never
  part of the per-profile `Settings`/`rules.json`, since these switches
  act before or outside any one profile's own scope (a first-run notice
  shown once per machine, a background check at launch before a profile
  has even finished loading).
- **Fail-closed vs. fail-open, and why they differ**:
  `JsonAppSettingsStore::load` fails **closed** — a missing file loads
  as `AppSettings::default` (every switch on, safe: nothing automatic
  runs yet regardless), but a present-and-unreadable/corrupt one loads
  with **every network switch off**, since this file's own default
  means network on, and a corrupted privacy preference must never
  silently re-enable it. `JsonNotificationStateStore`/
  `JsonProfileNotificationStateStore` (`notifications.json`) are the
  opposite, deliberately: any problem (missing, corrupt, unrecognized
  version) loads as their own `::default`, since that's app-remembered
  state, not a preference — losing it costs at most re-showing an
  already-seen notice (including Welcome, which then safely re-closes
  every automatic-network gate below until answered again — a
  self-correcting failure mode, not a silent one).
- **The Welcome/first-run gate**: nothing automatic — no automatic
  update check, no automatic rule-database refresh — ever runs before
  `NotificationState::welcome_completed_at` is set
  (`crate::use_cases::CheckForUpdate::execute`'s `AwaitingFirstRun` skip
  reason, and `crate::use_cases::RunLaunchNetworkChecks::execute`'s own
  shared gate over both halves). Answering Welcome from any of its three
  buttons, **or closing it from the bell's own × with no button
  clicked**, all set this field — `crate::use_cases::DismissNotification`
  sets it too when the dismissed key's kind is `Welcome`, precisely so
  closing the card counts as answering it (see that use case's own doc
  comment) rather than leaving the automatic gates closed forever with
  no notice left to explain why. Both answer paths (`CompleteWelcome`
  and `DismissNotification`'s Welcome branch) also pin a **missing**
  `app-settings.json` with `AppSettings::default()` through the shared
  `pin_policy_if_missing` (before marking Welcome answered, so a failed
  write leaves it unanswered); a `Loaded` or `Recovered` file is never
  touched. The pin is one `AppSettingsStore::save_if_missing` call, never
  a `load` followed by a `save`: that pair would let the pin replace a
  file another writer created in between, and the user's choice with it.
  Losing the race is success; the other writer's file stands. `NetworkPolicy::default()` is built from
  `RuleDatabase::is_recommended` and `NetworkPolicy::fetches` is the one
  source-to-toggle mapping; Steam is recommended (on by default) but
  `is_auto_refresh_eligible` stays false for it.
- **`RecommendedSourcesIncomplete`** (lowest priority, Info, mutable)
  lists recommended sources that are `Off`, or `NotDownloaded` (on, no
  cache, `RefreshMode::Manual`). It shares `refresh_mode` with the stale
  reminder so the two **partition** the sources (a test sweeps the state
  grid); it is gated on Welcome answered and `allow_network`, so a
  `Recovered` file never prompts. `EnableRecommendedSources` turns the
  recommended toggles on via `with_recommended_sources`, never touches
  `allow_network`, and refuses a `Recovered` file; the download is the
  caller's separate manual refresh.
- **The Dashboard's "Get the recommended rules" step** is derived, never
  stored: `recommended_rules::recommended_rules_step` is one pure function
  over `RecommendedRulesFacts` (the policy, the `RuleDatabaseView`s, the
  first-run answer, `StepSkip`, the profile settings), shared by the
  desktop's status command and the notification evaluator. First match
  wins: Done (every recommended **and** importable source has an import
  record in this profile: "imported once", a newer cache is
  `ImportedRulesOutdated`'s job), then Skipped, then the per-source needs
  (`TurnOn`/`Download`/`Import`); `Unavailable` applies only when some need
  contacts the network and `allow_network` is off or Welcome is
  unanswered, so an import-only remainder is always `NeedsAction`. The
  skip flag is per profile: `ProfileNotificationState::recommended_rules_skipped_at`
  in `<profile>/notifications.json`, read into `StepSkip`.
- **`evaluate` dedupes against the step.** `sources_offered_by_step` runs the
  same derivation and is non-empty only while the step is `NeedsAction`;
  `ImportedRulesOutdated` and `RecommendedSourcesIncomplete` drop exactly
  those sources, so one call to action shows at a time. Done, Skipped and
  Unavailable leave both notices as they were before the step existed.
- **`GetRecommendedRules` has two phases, never one call.** Phase 1
  (`fetch`) takes no `Session`, so the network never runs under the session
  lock; it turns the recommended sources on if any need says `TurnOn`,
  downloads each source needing the network one call at a time, and
  returns a `FetchedRecommendedRules` (private fields) that only it can
  build. Phase 2 (`import`) takes that value and the `Session` and imports
  only the sources that had **no import record at phase 1** (`missing_import`),
  whatever the download outcomes, in one `ImportRimSort` call (one save, one
  resort). A source already imported is never re-imported by the step. It
  reports `ProfileChanged` instead of importing when another profile was
  loaded in between. A failed import crosses as `ImportStep::Failed` with
  an `ImportFailure` (`Import` or `Store` only); a failed import record is
  `ImportedManifestNotRecorded`, since the rules were saved.
- **The CLI never runs anything automatically.** `crate::use_cases::RunLaunchNetworkChecks`
  is desktop-only, called once per launch right after a project loads;
  `apps/cli` never constructs or calls it. Every CLI network action
  (`db refresh`, `check-update`) is an explicit, user-typed command — a
  click/keystroke is its own consent, so `CheckForUpdate::execute`'s
  `Manual` request kind and `RefreshRuleDatabases::execute` both ignore
  the rule-database cadence entirely (never `NotDue`); the automatic update
  check has no cadence of its own beyond once per launch, and no `NotDue`
  reason.

## Compat patches and the patch maker — two independent project kinds

Both mirror the same shape: a `BTreeMap<Id, Project>` on `Session`,
loaded by its own `LoadProject` port, and every mutating use case follows
the snapshot/persist/rollback pattern. Neither's decisions cross into
the other or into the profile's own `DecisionSet`.

- **Compat patches** (`Session.patches`): a patch's own ledger
  (`Session.scoped`, keyed `(PatchId, OrderSource)`) is *derived* from
  the profile's via `rim_resolve::ledger::scoped`, never built from
  scratch. `PlanMerge`'s scoped-participant rule only applies when the
  caller's `MergeContext.scope` is `Some` — build one via
  `Session::merge_context(slot, key)`, never by hand. `ExportPatch` is
  the one use case writing outside the profile directory
  (`MergeModWriter::write`, `Mods/` only when `install` is set) — gated
  by `read_marker`/`exists` refusing to overwrite a folder that isn't
  this patch's own previous export, checked before the first write.
  `out_dir` must be absolute; the inside-`Mods` check is lexical only
  (no filesystem read, no symlink resolution) — that, and the
  running-game refusal, stay the interface layer's job.
- **The patch maker** (`Session.assignments`): simpler —
  `upsert_assignment` invalidates no ledger cache of its own (an
  assignment project has no decisions/findings) except that it **does**
  invalidate its own cached `assignment_coverage` (which also reflects
  the project's own rows, unlike `assignment_instances`, which only
  reads the active install). `CreateAssignment` is **two-phase**:
  `list_candidates` is cheap (off `SourceIndex` alone, no def-source
  reads); `infer_candidate` does the real, expensive inference for
  exactly one chosen type, reading a `TargetKey` field's value back only
  when its owner is in the reference/target set — this split plus the
  owner-scoping is what makes interactive use possible at all (unscoped
  inference over a large reference set is orders of magnitude slower).
  A candidate with **no** `TargetKey` field is valid only when the
  target set is empty (a free-standing "new def" candidate).
  `export_folder.rs` is `ExportPatch`'s folder-safety core, shared
  verbatim by `ExportAssignment`. `AssignmentProject` is multi-section
  (`rim-resolve`'s own type); every mutating use case here loops
  `sections()` rather than assuming one. The **only** place an
  `ItemSlot` value's own liveness is checked at all is this crate's own
  `validate_and_clean_sections` (`export_assignment.rs`,
  `SessionKnownDefs`-backed) — `rim-merge` performs no such check itself
  (see `crates/rim-merge/CLAUDE.md`); don't add a second one.

## `ImportGameLog`

Attributes a parsed game log (a `Player.log` or a console snapshot)
against `session.report().mods`:
primary, longest-prefix match of a `file:`/block-trailer/texture path
against each active mod's path/loaded folders; fallback, the `[Tag]`/
dependency/timer-label display name, lowercased, against a display-name
map built here, then as a packageId (`_steam` base), then reduced to ASCII
letters and digits (`attribution::NameResolver`; the last two must match
exactly one mod, else a family is `Ambiguous` and a typed entry
`Unattributed`) — a small reimplementation of `rim-analyzer`'s own
"first mod to claim a name wins" resolution, not a call to that
function, since this only has `Vec<Mod>`, not scan-time `&[ScannedMod]`
data. `rim_resolve::domain::normalize_log_text` (whitespace-collapse)
lives in `rim-resolve`, not here or in `rim-io`, specifically so it's
reachable from both without either depending on the other.

The log may be Unity's (1.6) or the 1.7 engine's: the reader decides that by
content and this use case never sees which (the port types are the same; a 1.7
`Player.log` ends `CleanExit` on its `Log closed.` line, and a family's
severity is the log's own level). Frame attribution is unchanged, since a 1.7
frame names `Ns.Type.Method` and goes through the same `namespace_ownership`.

The parser hands this use case already pass-resolved lists (see
`crates/rim-io/CLAUDE.md`): `dependency_warnings` is the last startup
pass's only (the game repeats them in every pass), and each `RawTimer`
carries its `pass`, which `TimerSummary` passes through; nothing here
re-folds passes.

`GameLogSummary` also carries the parser's entry view: `classes`
(`EntryClass` -> `AttributedClass { tally, attribution }`),
`blank_separator_lines` and `sentinels`. `tally` is the parser's
`ClassTally` unchanged; `attribution` maps a `FamilyKey` to a
`FamilyAttribution` (`Mod`, `Unattributed { raw }`, `Ambiguous { raw,
candidates }`: a type or assembly several mods ship; the shared
`LogAttribution` of the typed lists and the desktop DTOs has no ambiguous
case and is unchanged) for each family whose first entry carried an
`AttributionInput` (the parser's per-family, bounded evidence, one variant
per kind: `DisplayName`, `DisplayNameAndPath`, `Path`, `TypeName`,
`Assembly`; a family with none has no row: cross-references and the
engine's own classes never carry evidence). `use_cases/import_game_log/attribution.rs`
resolves it, once per family: `PathIndex` (every mod's `path` and
`loaded_folders`, normalized once per import and sorted longest first, so
the typed lists and the families share it and a 4k-family log does not
re-normalize ~1000 mods per lookup; trailing separators are trimmed, since a
real `loaded_folders` entry can end in `/`), the display-name map, and, for
`TypeName`, `session.sources().namespace_ownership(type)`: the longest
namespace prefix that names an assembly some active mod ships, all its
owners (none, one, several; no fall-through to a shorter unique prefix, unlike
`dll_owner_of`, which stays as the sort edges need it). Only that one type is
asked (the innermost non-engine frame, or the type a type-load head names):
an unresolvable type is `Unattributed`, never re-blamed on a caller's frame,
so attribution is a function of the family key. A mod whose namespace differs
from its DLL name stays unattributed until the analyzer has a namespace ->
assembly index. `Assembly` reads `assembly_ownership(name)` directly. The
typed lists and the families share `ports::leading_bracket_tag`. The mod ids are the report's raw ids, so a `_steam`
suffix is kept and a lookup by `Mod::id` finds the mod; nothing compares a
declared id here. `totals()` (via `ports::totals_of`, the one place the
sums are written, taking the tallies as pairs) is the reconciliation
`lines_read == entry lines + blank separators`; `has_losses()` is a read bound that truncated something, a
class whose families overflowed their bound, a logging gap (the game stopped
writing), **or** a console snapshot at the 1,000-entry cap (a snapshot *over*
the cap is reported as its own state, `ConsoleFill::OverCap`, not as a known
loss; `has_losses()` has no production caller yet, only tests). `execute(session,
path, KindChoice)` passes the kind choice to the reader; the summary carries
`coverage` (a closed `LogCoverage`, one variant per `LogKind`, so the kind is
derived from it; a snapshot's `SnapshotCoverage` has `entries`,
`head_truncated` (the copy starts mid-entry) and the derived three-way
`fill()`: `ConsoleFill::{BelowCap, AtCap, OverCap}`; `startup_passes()` is
`Some` for a `Player.log` only, so a snapshot never states passes), `logging_gaps` (`LoggingGap { stop_line, resume: GapEnd }`)
and the derived `lower_bound_families()` (families whose line span overlaps a
gap, the two gap-message classes exempt; `lower_bound_entries()` is the
entries they hold). The port reads both kinds, so
a caller that wants one kind checks `summary.kind()` itself (the desktop
command accepts both and maps `coverage`, `logging_gaps` and the lower-bound
summary to its DTOs, so its UI labels what each file covers). The port types
(`EntryClass`, `Family`, `SentinelReport`, ...) live in
`ports/game_log.rs`; every `LogReadStats` counter is a `ReadLoss` variant
and an arm in the exhaustive `losses()`, and `apps/desktop`'s
`LogReadStatsDto::from` destructures `LogReadStats` exhaustively too, so a
new counter cannot go missing.

## `VerifyOrder` and its counterfactual phase

A live-session, on-demand-only replay of every active patch operation
against a chosen `OrderSource` (`rimmerge verify` is the only caller —
never wired into the cached `sort`/`ledger` path). Every failed
`TopLevelOutcome` becomes a `Finding::PatchWillFail`; this **never
reaches `FindingIndex`**/the ordinary inbox — it exists only for
`VerifyOrderReport`. The def keys are examined on the replay pool
(`replay_pool::install`, a 4-thread rayon pool, not the global one: it
leaves most cores to a running game; more threads now verify faster but at
no lower peak memory, see that module's measured figures) in chunks of `DEF_KEYS_PER_CHUNK`, each key into its own
`PassTally`, folded back in key order (`PassTally::absorb`), so the report
never depends on thread timing; `on_progress` fires once per key, all of a
chunk's calls before any key in it runs (so `checked` can lead the finished
work by up to one chunk). This needs `Reader: Sync`, which every reader passed in
already is. `RenderMergeMod`'s eager clean-merge pass first calls
`Session::prefetch_clean_merge_previews` on the same pool: it builds
exactly the previews `redecide_clean_merge_at` would (an undecided
candidate with **no** cached preview, whatever choices a cached one was
built from) and caches them in ledger order, so it changes timing only,
never which preview a redecision reads.
Every def with an active patcher is replayed, including one only its own
winner patches: an OR-ed head is one query that succeeds when any named def
matches, and a self-patched alternative is often the one that matches.
The per-def replay lives in `verify_order/replay_def.rs`. A def whose every
op is under a closed gate in this order (`active_top_level_operations` is
empty) is skipped before any of its sources are read: it is neither
replayed, nor counted in `defs_checked`, nor listed in `skipped`.
`VerifyOrderReport.skipped` must name every def
truncated by a `Completeness::Partial` stopper on **both** the replayed
and the zero-owner path — a truncated replay must never silently count
toward `defs_checked` as if fully checked. `active_top_level_operations`
gates on "any indexed op under this top-level node is active", never
just the shallowest child, or a `Sequence` whose first child has an
unsatisfied `MayRequire` while a later sibling doesn't is dropped
wholesale.

After reconciliation, each surviving `Unknown`/`DeadTarget`-caused row is
re-asked, via `rim_merge::effective::counterfactual`, whether a
different load order would have made it succeed (`VerifyOptions
{ counterfactual }`, default on), rewriting the cause in place when it
would. `def_sources::top_level_operations` re-interleaves each mod's own
operation *groups* by load order so a mod's contributions stay
contiguous — `counterfactual` refuses (and counts) a non-contiguous
input, so a non-zero `refused_non_contiguous` is a bug signal. Rows are
refused *before* the caps, never after, so a refusal never spends
budget. `execute_with_progress`'s `total` grows **once**, mid-stream,
from the candidate-key count to `keys + counterfactual jobs` — a
consumer must recompute its percentage from both fields.

Every `PatchWillFail` row also carries `reorder_kind:
Option<rim_resolve::domain::ReorderKind>` — `None` for
`DeadTarget`/`Unknown` (no reorder to classify), `Some(Content)` for the
edge-evidence route (`report::classify_cause`, which only ever tracks the
content `EdgeKind::PatchRemovedNode`, never the cosmetic
`PatchRemovedNodeCosmetic` kind), and `Some(Cosmetic { existing_merge })`
when the counterfactual phase's own fix has
`CounterfactualFix::final_def_unchanged: true` — the final resolved def
is identical whichever mod loads first, so no interface offers a
`set-pair` rule for it. `existing_merge` is looked up from
`report::PatchCollisionIndex` (built once per `execute_with_progress`
call, the same pattern `EdgeEvidence` already uses), keyed by the failing
op's own `(def_type, def_name, selector, sub_path)` via
`report::op_sub_path` — the same sub_path derivation `classify_cause`'s
own `DeadTarget` check already used, now shared rather than duplicated.

`use_cases::verify_order::reorder_conflicts(report, outcome, after,
before)` is this module's other shared export: the one answer to "does
this proposed `Action::Reorder` contradict something already in the
report", called by both interfaces rather than each re-deriving the
edge-direction comparison (see either interface's own `CLAUDE.md` for
the conflict-direction rules themselves). Status is evaluated fresh via
`rim_resolve::evaluate::edge_status`, never a scan-time-fixed
`EdgeReport::status`.

## Game launch

`game_launch.rs` is pure: `game_launch_status(game, route, order)` picks the
Launch RimWorld button's state, first match wins: `GameRunning`, then
`Unavailable`, then `NeedsApply` (with its `UnappliedReason`), else `Ready`.
`Session::order_on_disk()` reads the **selected** order (not Suggested) and
puts unscanned working-set changes (`is_stale`) ahead of
`!file_matches(selected)`, which ignores the generated merge mod.
`use_cases::LaunchGame::execute` takes no `Session`: the interface snapshots `game_dir` and `order_on_disk()` under the
session lock and calls it outside the lock. It re-reads `GameLauncher::route`
on every call (the button's status may be seconds old) and refuses
`GameRunning`/`Unavailable` always, `NotApplied` unless `IfNotApplied::
LaunchAnyway`. Whether the game is running is an input (`GameProcess`): the
probe trait stays in `rim-io`. `GameExecutable` has a private path and one
constructor (`of_install(game_dir, GameLayout)`), so a route can only name
`<game_dir>/RimWorldWin64.exe` (Unity layout) or `<game_dir>/RimWorld.exe`
(Waylith, 1.7). `GameLayout` is a `rim_analyzer::domain` type (pure); `rim-io`
detects the layout from the folder. Tests
use `test_support::FakeGameLauncher`; nothing here starts a process.

## The file baseline is live (`RefreshFileBaseline`)

`Session::file_active_mods` is the **live baseline**: the active list the
on-disk `ModsConfig.xml` held the last time it was read successfully, not only
what it held at load. It changes in four places: `Session::new`, the rescan
correction `set_file_active_mods`, `set_current_order` (the file now is this)
and `Session::observe_file`, which `use_cases::RefreshFileBaseline` drives. So
`file_matches`, `order_on_disk` (the launch button), the import diff and the Mods
page marks follow an outside edit once a refresh has seen it, while
`pending_changes()` does not (see "Own facts vs live facts" below).

- **Two phases, never one lock.** `RefreshFileBaseline::fetch(FileProbe)` takes
  no `Session` (the probe is `Session::file_probe()`: the path and an epoch) and
  reads through `ModsConfigStore`; `FileObservation::commit(&mut Session)` swaps
  the result in and returns `BaselineChange::{Unchanged, Changed}`. The desktop
  runs the read in a bare `spawn_blocking` between two short `with_session`
  calls, the same lock exception as `game_launch`. `execute` does both for a
  caller that already holds the session (tests).
- **A failure never empties the baseline.** A read that fails keeps the last
  valid list and records `FileBaseline::Stale { reason }` with
  `BaselineIssue::{Missing, Unreadable, Malformed}` (from
  `ConfigErrorKind`, the typed `ConfigError`; a truncated or partly written file
  is `Malformed`); the next good read sets `Fresh`. `Changed` means the list or
  the status differs, so a caller emits its change event only then.
- **A read overtaken by our own write is dropped.** `FileWatch` (a private
  field) holds an epoch that `set_file_active_mods` and
  `set_current_order` bump; `observe_file` ignores a probe from an older epoch,
  so a read that began before an `Apply` cannot put the pre-apply list back.
  A session built over an override (`execute_with_active_set`) starts at epoch 1,
  because `set_file_active_mods` resets the watch once; a plain load starts at 0.
  The epoch is never compared across sessions: a read probed from the old session
  can commit into the new one when their epochs match; the path check rejects
  another file, the window is negligible, and the next poll corrects it.
  `observe_file` ignores a probe for another path than the session's
  `mods_config`.
- **Outside change is derived, not stored**: `FileWatch` remembers the ids
  Rimmerge itself last loaded or wrote (the **acknowledged** list), and
  `Session::external_file_change()` is a closed `ExternalFileChange::{None,
  Membership { added, removed }, Reordered}` of the live baseline against it: the
  counts are **exact** ids gained or lost (a swap of `x` for `x_steam` is one of each,
  the same exact comparison `file_matches` makes, so Apply cannot overwrite a swap
  unasked), `Reordered` only when the membership is the same but the sequence differs.
  Rimmerge's own write resets it, so only someone else's edit shows. The row marks
  stay by base id (below), so a swap raises the note but marks no row.
  `Session::file_holds_exactly(source)` is the strict sibling of `file_matches`: the same
  ids in the same sequence with the generated merge mod **counted** (an outside edit that
  only turned it on or off is a difference, since an Apply would put it back).
  `file_holds_selected_exactly()` is it for the selected order, the live fact the
  desktop's baseline DTO carries so a note never says an Apply would overwrite what it
  would not; `PreflightApply` gates its `outside_change` on the same exact comparison.
- **Own facts vs live facts.** Anything said about the user's *own unsaved work*
  compares with the acknowledged list, never the live baseline: an outside edit alone
  must not read as their pending changes. `pending_changes().unapplied` and
  `order_unapplied` are own (`orders().current` vs the acknowledged list, merge mod
  ignored for the sequence); they feed the close and project-switch guard, the pending
  banner, "This isn't your active order yet", the export panels' note and the import
  preview's `replacesUnwritten`. `file_matches`/`order_on_disk` (the launch status),
  `DashboardDto.file_matches_*`, the import diff's file side and the Added/Removed
  marks are live. The outside-change note is the one explanation of an outside edit.
- **Marks** (`MembershipMark::{None, Added, Removed}` on `ModListRow`) compare
  `working` with the baseline by `ModId::base()` (`FileMembership`): `Added` on
  an Active row the file lacks, `Removed` on an Inactive row the file lists. A
  reorder alone never marks. A mod the file lists that is not installed has no
  row in either list, so it has no mark.
- The refresh only reads: it never touches `working`, the orders or the file,
  and its tests assert the fake store saw no write.

## Activating and deactivating mods

`Session.working: ActiveSet` is the working, in-memory, unsaved active-mod
list — the sole exception to "every mutating use case persists
immediately" (see Invariants). Activating/deactivating a mod
(`ActivateMods`/`DeactivateMods`, wrapping pure `active_set::{plan_activate,
plan_deactivate}` over a `ModInventory`) changes membership only: `orders`,
the ledgers and the sorter stay as they were until `use_cases::Rescan` (a
brand-new `Session` via `LoadProject::execute_with_active_set`) picks the
edit up. The one thing that reaches `orders.current` sooner is the edit
settle: once an edit leaves `working` with the same ids as Current, Current
takes `working`'s sequence (`settle_current_onto_working`), and
`Session::set_current_order` does the same for a written file. `pending_changes()`/
`is_stale()` are **two separate diffs**: `unscanned` (`working` vs. the
scanned `orders().current` — what a rescan would pick up) and
`unapplied` (`orders().current` vs. the acknowledged list, what Rimmerge
last loaded or wrote, so an outside edit of the file never shows here; a
membership diff, so a settled reorder leaves it empty and shows only in
`PendingActiveChanges::order_unapplied`). `Apply` refuses to write `ModsConfig.xml`
(`ApplyError::StaleActiveSet`) while `unscanned` is non-empty; decisions
and rules still save regardless. `ActiveSet::new` validates every id
resolves and rejects exact duplicates — **it does not require Core to
load first** (a real install can legitimately load a framework mod, such
as an early-loading hook, before Core); the only real Core invariant is "never
removable" (`ActiveSet::deactivate`'s own `is_core` check).
`set_current_order`'s own callers (`Apply`, `ExportPatch::install`)
append/remove a package id on `orders.current` directly, so it folds
that same delta into `working` too (`ActiveSet::force_activate`, no
inventory check — the caller just wrote this id to the real file) —
otherwise a plain install-then-apply would read as stale for no reason.

**The order and mod-detail read models are pure joins behind `Session`
methods** (`order_rows.rs`, `mod_detail.rs`, `placement.rs`, `mod_names.rs`, each with a
`session/` half): `order_rows(source)`, `mod_detail(id, source)`,
`explain_placement(id)` and `mod_names()`. A lookup by id tries the **exact id
first, then `ModId::base`** (`mod_detail`'s `find_active` over active mods,
`explain_placement`'s `find_placement` over the sorter's placements), so with
a plain and a `_steam` copy both active each request answers for its own copy;
`mod_info`'s `resolve_installed_mod` has the same two passes and also reads
inactive mods. `ReadTexture` (through `find_active`), the active mod info's cost
row and `ReadDefTexture`'s undecodable check (against the exactly resolved
owner) do the same. These lookups stay base-only on purpose: `is_own_export`
(`assignment_coverage.rs`) and the two copies of `is_core`
(`create_assignment.rs` and `export_assignment/validation.rs`). The fact each
reads (is this Core, is this our own export) is the same for either copy, so
which copy answers cannot change the result. A miss is `ModNotActive`, which the
desktop maps to `mod_not_found`. `mod_names` keeps an active id's own name over
a base-id alias or an inactive entry. The CLI has no equivalent lookup.

`Session::mods` flows through `mod_index::query` (the **scanned** tier, paged
and capped at `MAX_PAGE_SIZE`; patch scopes and rule pickers use it); it
`.chain()`s a synthetic row per `report.missing_mods` entry so a missing mod
is searchable through the same path as everything else, and
`ModSummary.source` is `None` only for a missing row (there's no `About.xml`
to have read a source from; a fabricated one would be a lie).

The Mods page reads the **working** tier through `mod_lists`:
`Session::working_mod_list` (the working set in its own order) and
`Session::inactive_mod_list` (every discovered mod whose base id is not in
it, by name A first by default; see the display sort below). Both take a
`ModListQuery` (search, tag, source, sort; no paging) and
return every matching row up to `MAX_MOD_LIST_ROWS` (5,000) with `total`
counted before the cut, so a cut is visible. A row's `RowState` is closed:
`Scanned { tags, hard_dependents }` (in `report.mods`), `PendingScan` (no
scan facts: in the Active list, activated since the last scan; in the
Inactive list, an ordinary inactive mod) or `Missing`. Filtering lives
there, never in a command or in Vue. The tag filter applies to scanned rows
of the Active list only; a specific source excludes a missing row.

**The display sort is `ModListQuery::sort: Option<ListSort>`** (`ListSortKey`
`LoadOrder | Name | PackageId | LastUpdated`, `SortDirection`); `None` is the
list's default (Active: working order; Inactive: Name ascending). Every sort is
total: the key's comparison runs in the chosen direction, then the exact
`ModId` ascending, in both directions. Names and ids compare Unicode-lowercased,
then as written (no locale collation). An unavailable update time sorts after
every known one in both directions. `LoadOrder` on the Inactive list means its
default. `ListSort::is_load_order()` (ascending `LoadOrder`) is the one fact
reordering must gate on. Sorting runs before the `MAX_MOD_LIST_ROWS` cut and
never touches `working`/`orders`.

**A search says where it matched, from the filter's own fold.** Each row
carries `ModListRow::search: SearchHighlight` (`mod_lists/search.rs`): byte
ranges into the row's original `name` and `mod_id`, empty without a search. It is
filled by `admit`, which is also the search filter: a row is listed exactly when
`SearchHighlight::of` finds a range in its folded name or id, so the
marks and the filter are one computation and cannot drift (an empty needle admits
everything unmarked). The fold is the crate-wide `search_fold` (`search_fold.rs`, shared by every search box:
`mod_lists`, `mod_index`, `changes`, `finding_index`, `merge_workspace`): it lowercases **one character
at a time** (`char::to_lowercase`) and writes `ς` as `σ`, for the needle and the searched text alike:
`str::to_lowercase` writes a word-final `Σ` as `ς`, so `ΑΣ` would never find `ΑΣΑ`, and the `ς`
mapping lets a typed `κοσμος` find `ΚΟΣΜΟΣ`. Sort keys keep `str::to_lowercase`. Every non-overlapping
occurrence is marked. A hit in the folded
text is widened to the whole original characters it touches (a fold can change
length: `İ` lowercases to `i` plus a combining dot), overlapping hits merge and
touching ones stay apart. The per-character spans add up to the folded text by
construction; if the lengths ever stopped agreeing (a `debug_assert` fails in tests)
the row is marked whole, so the filter's verdict never changes. The unit stays bytes here; an interface converts once at its own
boundary (the desktop to UTF-16). The proptests pin the ranges against
the fold's own hits (never wider than a hit touches).

**Update times are a session fact, not a report fact.** `ModUpdateTimes`
(`UpdateTime::Known(UnixSeconds) | Unavailable(ShippedWithGame | Unknown)`)
hangs off `Session` (`set_update_times`), filled by `LoadProject` when it was
given a `ModTimestampReader` (`with_timestamp_reader`, optional so the CLI and
most tests need none; without one every time is `Unknown`), once per load and
therefore refreshed by every rescan. It is deliberately outside
`rim_analyzer::Report`, whose JSON is a byte-identical contract. The port is
infallible: an unreadable source answers `Unavailable(Unknown)`, never an error
and never an epoch. Fake: `test_support::FakeModTimestampReader`.

## Library organization (`organization`, `EditOrganization`)

Nested mod groups and per-mod colours: a bounded context of its own, pure
in `organization.rs` (a facade over `organization/{ids,color,group,forest,
edit,resolve,state}.rs`), persisted to `<profile>/organization.json`
through `ports::OrganizationStore` (`rim-io`'s `JsonOrganizationStore`).

- **Never on the sort path.** `compute()`, the ledger and every cache in
  `Session` take no organization, so an organization edit invalidates no
  cache, changes no order, no `working`, no `ModsConfig.xml`. A test pins
  `working`, both orders, `file_active_mods`, the selection, the pending
  diff, the sort outcome, the rules, the decisions and the ledger as equal
  before and after every edit kind, and the sort and ledger equal with an
  empty and a large organization. Keep it that way: a new `Session` cache
  must not read it, and `mod_lists` is the only join (fresh per query, no
  cache).
- **`ModOrganization` is a forest with direct members only.** Ids are
  `OrganizedModId` (a base id: `x` and `x_steam` are one member; the
  constructor is fallible, bounded like the file reader), `GroupId`
  (non-zero, never reused within a session), `GroupName` (trimmed, 1 to 64
  characters, no control characters), `ColorToken` (a closed 12-name
  palette). Invariants live in the aggregate: no cycle, depth at most 8,
  at most 1,000 groups, sibling names unique ignoring case, bounded
  memberships and colours (`OrganizationLimits`, read by the `rim-io`
  reader too). A mutator that returns an error changed nothing (a proptest
  asserts it). "Parent views include descendants" is `scope()`, derived on
  every call; no ancestor membership is ever stored. A mod never inherits
  a group's colour.
- **`OrganizationState` is closed**: `NotLoaded` (built without a store:
  the CLI, most tests), `Loaded`, `Recovered { issue }` (the file is
  present but unreadable, too large, malformed, from a newer version or
  breaking a forest rule). Only `Loaded` can be read or edited
  (`OrganizationState::loaded`/`loaded_mut`, `OrganizationUnavailable`),
  so a session that could not read the file can never save an empty
  organization over it. `LoadProject::with_organization_store` (optional,
  desktop only) reads the file once per load, so every rescan and import
  rebuild reloads it: nothing is carried by hand. A damaged file never
  fails a project load. The desktop also calls
  `Session::reload_organization(&store)` on the new session **under the
  session write lock, just before the swap** (`replace_session` and the
  rescan swap): every organization edit saves while holding that lock, so
  an edit that landed after `LoadProject` read the file is on disk by then
  and the swap cannot drop it (and the next save cannot erase it). A
  `NotLoaded` session is left alone. The rules, decisions, patches and
  assignments are re-read the same way by `Session::reload_persisted` (the
  desktop calls it through `reload_persisted_state`, right after
  `reload_organization`): it sets what differs, calls `recompute_sort` once
  and only when rules or decisions changed, drops the scoped ledger and previews of
  only the patches that were added, removed or changed (an unchanged patch
  keeps its caches) or the assignment coverage cache. `rule_load_warnings()`
  chains three separate fields in a fixed order (the rules file's own
  warnings, the reload failures, the mod-knowledge notes), so a reload
  replaces its own group and no setter order can misfile one. A store that
  fails to re-read keeps the scanned copy and adds
  `RulesLoadWarning::PersistedReloadFailed`; a failed rules re-read also
  keeps the scan's rules-file warnings. `set_organization_state` is
  crate-private (fixtures in other crates use
  `test_support::set_organization_state`). Errors split by origin: an edit
  reports `OrganizationError`, reading a document reports
  `OrganizationDocumentError` (dangling parent, duplicate id, an id out of
  range, a `next_group_id` behind the groups, or a wrapped rule). The next
  group id is part of the document, so ids are never reused after a
  reload.
- **One use case, one closed command type.** `EditOrganization::execute`
  applies an `OrganizationEdit` (create, rename, reparent, delete, group
  colour, add/remove/move members, mod colours; `CreateGroup` carries the mods
  that join the new group, checked for room before the group exists, so
  "create and add" is one edit and one save), snapshots, saves once and
  restores on a failed save; an edit that changes nothing writes nothing.
  `ResetOrganization` ("start over") is the only way out of `Recovered`:
  `OrganizationStore::archive_unusable` keeps a copy
  (`organization.json.unusable-<seconds>`) before the empty save, and it
  refuses a loaded organization.
  `OrganizationStore::save` fails with `OrganizationSaveError`: `TooLarge`
  (the document would exceed the file's size limit; a limit the user can act
  on, `organization_limit` in the desktop) or `Failed`; `EditOrganizationError::Save`
  and `ResetOrganizationError::Store` carry it whole, so the desktop maps one
  type. "Remove mods that aren't installed" is the use case
  `RemoveUninstalledMembers` (an `OrganizationEdit::RemoveMembersFromGroups`
  over the scope's groups): the caller names the mods the user confirmed, and
  the use case removes only those still in `ResolvedScope::not_installed`
  when it runs, so a mod installed again since the confirmation keeps its
  memberships. The command in `apps/desktop` only parses and calls it.
- **Resolving against the install** is `resolve_scope` (the
  `find_by_base` rule, a copy already active wins, members found nowhere
  are counted, not dropped), reached through `Session::resolve_group_scope`
  for group-level activate/deactivate planning.
- **Copying another profile's organization** is `organization::plan_copy` (pure:
  `CopyMode::{Merge, Replace}`, a `CopyPlan` of the resulting `ModOrganization` and a
  `CopyReport`) and the use case `CopyOrganization` (`preview` is a read; `execute` re-plans from a
  fresh read of the source and saves once). The plan runs on a clone through the aggregate's own
  mutators, so every limit and rule of an edit guards the copy, and any `OrganizationError` refuses
  the whole copy (`CopyRefusal::Limit`) with nothing changed. The source is read through
  `OrganizationStore::load(source_dir)` (its size bound and validation apply; missing, unusable
  and empty are `CopyRefusal`s, and so is a source equal to this organization,
  `NothingToCopy`: an alias of the own folder or an identical copy), never written. Merge matches groups by path (names compared with
  `uniqueness_key`, siblings of one parent), keeps this profile's name spelling, id and colours
  (a differing group colour is a `GroupColorClash`, a differing mod colour a count, an unset colour
  is filled in); Replace starts from an empty organization that keeps this one's `next_group_id`
  so no id it ever handed out is reused. A merge that changes nothing writes nothing; a failed
  save leaves the session as it was and a `TooLarge` save is `CopyRefusal::TooLargeToSave`. The
  desktop answers "is this profile's own folder" before the use case runs (it needs the
  filesystem); the use case touches no order, working set, rule or `ModsConfig.xml`, and a test
  pins that.
- **`mod_lists`** gains `ModListQuery::group` (`GroupFilter::InScope`,
  descendants included and deduplicated, or `Ungrouped`), `ModListRow::groups`
  (direct labels in group-tree order: depth first, siblings by name, so a row's first coloured group is the tree's first) and `ModListRow::color`. Both list functions now
  return `Result<_, ModListError>`: a stale group id is `UnknownGroup`, and
  a filter with unavailable organization is refused, never read as "no
  filter". A group filter is a filter: `reorderable` stays on.
  A row's `RowState::PendingScan` carries the mod's `manual_tags` (an
  unscanned mod has no evidence, so every tag it holds was added by hand);
  the tag filter matches scanned tags and these, in both lists.
  **The Groups view is a second read model over the same rows**
  (`mod_lists/grouped.rs`): `Session::working_mod_groups` /
  `inactive_mod_groups` take a `GroupedListQuery` (the pane's `ModListQuery`
  plus the folded `FolderKey`s, at most `MAX_COLLAPSED_FOLDERS` = `OrganizationLimits::MAX_GROUPS` + 1;
  an unknown id is ignored) and return a `GroupedModList`: the List view's own
  `ModList` (same rows, `reorderable` always `false`) plus a flat, preorder
  `entries` sequence of folder headers (`depth`, distinct `matched` count,
  `is_collapsed`) and mod copies (`folder`, `depth`, `also_in`), capped at
  `MAX_GROUPED_ENTRIES` (10,000) with `total_entries` counted before the cut.
  It is built from `working_rows`/`inactive_rows` (the uncut, decorated,
  filtered and sorted rows the List view cuts), so placement uses the labels a
  row carries and the pane's sort is never repeated: a folder's subfolders
  come first, then its mods in pane order; "Ungrouped" is last and only
  when it holds a mod; a group scope walks only its subtree, rebased to level
  1; with no search, tag or source filter empty folders show at 0, under one a
  folder with no match is left out. Counts are merged bottom up over one
  `tree()` call (never `tree()`/`scope()` per node) and are taken before any
  cut; a row past `MAX_MOD_LIST_ROWS` is counted, never emitted. The view needs
  organization: `NotLoaded`/`Recovered` is `OrganizationUnavailable` even with
  no group filter (the List view still degrades to no labels). It reads no
  cache and changes none (organization is still never on the sort path).

## Sharing a load order (`mod_list`, `ExportOrder`, `PreviewOrderImport`, `ImportOrder`)

- **`mod_list` is pure**: value objects, the "Copy as text" codec
  (`render_text`/`parse_text`), the export builder
  (`SharedModList::from_active`) and the diff (`plan_import`). No file, no
  XML: the `.rml` and `ModsConfig.xml`-shaped readers and the `.rml` writer
  live in `rim-io` behind `ports::ModListFileStore` (fake:
  `test_support::InMemoryModListFileStore`). Input is untrusted, so every
  value object is correct by construction (`WorkshopId` is never `0`,
  `ListedPackageId` passes the lenient id grammar, names are bounded and
  stripped of control characters), and every bound is a
  `ModListLimits` constant that `rim-io`, the CLI's stdin reader and the
  desktop paste all read. A malformed entry is a `SkippedEntry` (at most
  `MAX_SKIPPED_REPORTED`, the rest only counted); only a whole-document
  failure is a `Rejection`, and a rejection is a preview **outcome**
  (`ImportPreview::Rejected`), not an error. A RimWorld save is one more
  source behind the same port (`ListFormat::SaveHeader`; the three
  save-specific rejections are `SaveHeaderTruncated`, `SaveHeaderMissing`
  and `SaveHeaderTooLarge`). `ParsedModList` carries its `ListFormat` and,
  per parallel list (names, Steam ids), a `ListAlignment` (`Absent`,
  `Aligned`, or `Misaligned { ids, values }`: the list was left out whole,
  never zipped short); `ReadyImport` passes both through with the listed game
  version text (`listed_game_version`, verbatim), while `ImportPlan` and
  `VersionCheck` (major.minor) are unchanged.
- **Export reads the file, not the selection.** `ExportOrder` re-reads
  `ModsConfig.xml` through `ModsConfigStore::read` on every call (the
  desktop opens on Suggested, an order the player may never have run),
  exports base ids once each, leaves the profile's own merge mod out, and
  returns ids that fail the grammar as `ExportedModList::unrepresentable`
  for the caller to report, never dropped silently.
  `ExportSource::from_session` takes `Session::game_version()`, the
  scanned major.minor.
- **`plan_import` diffs against the active list the user has now**:
  `ImportTarget::from_session` passes `working` (pending activations,
  deactivations and moves included, since that is what an import replaces),
  not the file's list; the CLI has no working set and passes `ModsConfig.xml`'s
  list, so its output is unchanged. "Active" below means in that list.
- **`plan_import` resolves an entry in a fixed order**: a sender's
  generated id is `MissingKind::RimmergeMergeMod` and never activated;
  then a copy of the same base id that is already active wins
  (`AlreadyActive`, or `MatchedOtherCopy` with `Activation::AlreadyActive`),
  so re-importing your own export is an empty diff and never swaps a
  `_steam` copy for a local one; then the exact id on disk (`Activated`);
  then `find_by_base` on disk (`MatchedOtherCopy`, `Activated`); else
  `NotInstalled`. A repeat by base id is `Duplicate`, first position wins.
  Core goes first when the list lacks it (`CorePlacement::AddedFirst`) and
  is never in `deactivated`. The profile's own merge mod is appended at the
  end when the file has it, after `moved` is counted, so it is neither
  deactivated nor counted as moved. `VersionCheck` compares major.minor.
- **`ImportContext::id_spelling`** says how the list's ids were written. `Shared` (every
  export, pasted text, a save) is the base-id rule above. `Exact` is for the game's own
  `ModsConfig.xml` (`ImportTarget::for_the_games_own_list`, set by the desktop when the previewed
  path is the session's own file): a listed copy that is installed wins before the
  already-active-copy rule, so an outside swap of `x` for `x_steam` previews as that swap
  (`x_steam` activated, `x` deactivated) instead of being undone. A listed copy that is not
  installed falls back to the base-id rules. The CLI always passes `Shared`.
- **`check_order` is the one import rule.** `ImportOrder::validate`
  (desktop, against the live session), `validate_order` (the CLI, against a
  discovered inventory) and `import_blocker` (why the preview's "Use this
  order" is disabled) all call it: at most `MAX_ENTRIES + 2` ids (Core and
  the own merge mod on top of a full list), every id installed and on disk,
  none repeated, Core present, and at least one mod besides Core and
  generated mods (`NothingInstalled`). `ImportBlocker` is
  `ImportOrderError` without `ScanDidNotMatch`, which only a scanned
  session can raise. Never re-derive the rule in an interface.
- **An import is validate → scan → finish, never a `working` edit.**
  `ImportOrder::validate` returns a `ValidatedImport` (only it can build
  one); the composition root scans with `ValidatedImport::order` through
  `LoadProject::execute_with_active_set`, which makes that order the new
  session's `orders.current` and its `working`; `ImportOrder::finish`
  checks the scanned Current equals the validated order
  (`ScanDidNotMatch` otherwise, selection untouched) and then selects
  `OrderSource::Current`. An import does not go through `MoveMods`: it is
  validated against the inventory first and may change membership, and
  staging it in `working` and rescanning separately would leave a failed or
  cancelled rescan with a half-applied order. (A manual same-set reorder is
  different: it settles into Current at once, so `is_stale` never has to
  see it.) Nothing here writes `ModsConfig.xml`; the
  ordinary `Apply` with `source: Current` does.
- **`ImportLoss::of(plan)`** is the CLI's `--yes` rule (something
  deactivated, or a listed mod other than the sender's merge mod not
  installed), derived from the plan and never stored.
- **The text format has a client-side twin.** The desktop's "Copy the
  missing list" (`missingListText` in `apps/desktop/src/utils/orderShare.ts`)
  writes the same line format with its own copy of the name escaper.
  `tests/fixtures/client_text_parity.json` is asserted by both
  `render_text`'s Rust test and that function's vitest: change the
  escaping in both places and the fixture together, or one suite fails.
