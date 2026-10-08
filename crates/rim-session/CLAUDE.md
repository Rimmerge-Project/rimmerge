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
  carries without persisting immediately.
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
- `PreflightApply` is read-only (no ports): it lists
  `rim_resolve::preflight` problems for the order named by `source`, through
  `Session::hard_problems` (the ledger cache is built lazily, hence `&mut`).
  `ApplyPreflight::requires_confirmation` is derived from the items, never
  stored. `Session::file_matches(source)` compares that order with the
  file's last-known list, ignoring the generated merge mod's package id on
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
here.

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
  `Session::recompute_sort` and `Session::set_current_order` — the only
  two places `orders` gets a new value — **never** by
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
- `Session::set_current_order` is the **one** way anything should change
  `orders.current` after load — `ExportPatch`'s own `install` and `Apply`
  both call it right after writing `ModsConfig.xml`. Skipping it is a
  real bug: a later `Apply` with `selected() == OrderSource::Current`
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
`VerifyOrderReport`. Every def with an active patcher is replayed, including one only its own
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
constructor, so a route can only name `<game_dir>/RimWorldWin64.exe`. Tests
use `test_support::FakeGameLauncher`; nothing here starts a process.

## Activating and deactivating mods

`Session.working: ActiveSet` is the working, in-memory, unsaved active-mod
list — the sole exception to "every mutating use case persists
immediately" (see Invariants). Activating/deactivating a mod
(`ActivateMods`/`DeactivateMods`, wrapping pure `active_set::{plan_activate,
plan_deactivate}` over a `ModInventory`) never touches `orders`, the
ledgers, or the sorter — only `Session::set_current_order` and
`use_cases::Rescan` (a brand-new `Session` via
`LoadProject::execute_with_active_set`) do. `pending_changes()`/
`is_stale()` are **two separate diffs**: `unscanned` (`working` vs. the
scanned `orders().current` — what a rescan would pick up) and
`unapplied` (`orders().current` vs. `file_active_mods` — what an
`apply` would write). `Apply` refuses to write `ModsConfig.xml`
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

`Session::mods`/`inactive_mods` flow through `mod_index::query`/
`query_inactive`; `query` also `.chain()`s a synthetic row per
`report.missing_mods` entry onto the ordinary rows so a missing mod is
searchable through the same path as everything else —
`ModSummary.source` is `None` only for a missing row (there's no
`About.xml` to have read a source from; a fabricated one would be a
lie), and it only ever matches an unrestricted source filter.

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
  (`ImportPreview::Rejected`), not an error.
- **Export reads the file, not the selection.** `ExportOrder` re-reads
  `ModsConfig.xml` through `ModsConfigStore::read` on every call (the
  desktop opens on Suggested, an order the player may never have run),
  exports base ids once each, leaves the profile's own merge mod out, and
  returns ids that fail the grammar as `ExportedModList::unrepresentable`
  for the caller to report, never dropped silently.
  `ExportSource::from_session` takes `Session::game_version()`, the
  scanned major.minor.
- **`plan_import` resolves an entry in a fixed order**: a sender's
  generated id is `MissingKind::RimmergeMergeMod` and never activated;
  then a copy of the same base id that is already active in the file wins
  (`AlreadyActive`, or `MatchedOtherCopy` with `Activation::AlreadyActive`),
  so re-importing your own export is an empty diff and never swaps a
  `_steam` copy for a local one; then the exact id on disk (`Activated`);
  then `find_by_base` on disk (`MatchedOtherCopy`, `Activated`); else
  `NotInstalled`. A repeat by base id is `Duplicate`, first position wins.
  Core goes first when the list lacks it (`CorePlacement::AddedFirst`) and
  is never in `deactivated`. The profile's own merge mod is appended at the
  end when the file has it, after `moved` is counted, so it is neither
  deactivated nor counted as moved. `VersionCheck` compares major.minor.
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
  `OrderSource::Current`. Staging a same-set reorder in `working` and
  rescanning separately would be wrong: `is_stale` is set-based, so a
  failed or cancelled rescan would leave `Apply` free to write the old
  order with no warning. Nothing here writes `ModsConfig.xml`; the
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
