# rim-merge

Pure merge engine: def XML -> field tree, `ParentName` inheritance,
three-way diff, patch-operation replay, merge plan, generated-mod emit.
The session hands it XML text; it never reads a file.

## Layer

Sits above `rim-resolve` and below `rim-session` in the crate graph. No
IO of any kind — every input is a string or an already-parsed struct
handed in by the caller.

## Invariants

- **Correctness rule**: a replay or plan that might be wrong must become
  `Unsupported`/`CannotMerge` with a reason. Never model an operation
  approximately; `docs/concepts/verify.md` (replay semantics table and
  its assumptions) is the RimWorld behaviour this crate could not verify
  directly against the game DLL — add a new assumption there when you
  rely on one.
- Every emitted xpath must parse back through
  `rim_analyzer::extract::xpath_expr` and replay to the intended tree;
  `tests/closure.rs` proves it for every `ItemId` variant. Extend that
  test when you add an xpath shape.
- **This crate is serde-free and stays that way.** The wire format for
  any data this crate consumes lives with the adapter that reads files
  (`rim_io::mod_knowledge`); `serde_json` is a **dev**-dependency only,
  for `tests/replay_parity.rs`.
- **Nothing under `src/` may name a real third-party class**, including
  the inline tests — use invented `Example.*` names.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rim-merge --all-targets --all-features -- -D warnings
cargo nextest run -p rim-merge --all-features
```
Snapshots use `insta`; read every `.snap.new` diff before accepting, never
accept blindly. Before reporting done, run the full gate block in the
root `CLAUDE.md`.

## Conventions

- Fixtures under `tests/fixtures/xml/` are neutral — some hand-built,
  some trimmed from a shape seen in the wild but rewritten with an
  `Example …` name in the header comment, never a real mod's own name.
  Keep them under 60 lines, and never trim a multi-def patch file down
  to one def when testing scoping: a real op-scoping bug can hide behind
  exactly that shortcut.
- Every `apply_*` helper threads one `&mut ReplayLog` (the flat caveat
  list plus per-replay counters) rather than a bare `&mut Vec<Caveat>` —
  add a new counter there, not as another function parameter.
- `depends_on` never contains Core and always contains the winner unless
  the winner *is* Core (then it can be empty — no gate needed). **Every
  emitted `<li>` in `<operations>` carries its own op's full `depends_on`
  as its own `MayRequire`; the wrapping `Operation` never does** — the
  game only reads `MayRequire` on a `<li>` list item
  (`DirectXmlToObject.ListFromXml`), never on a top-level `<Operation>`
  (`ObjectFromXml`), so a gate placed on the wrapper is silently never
  read. `emit::patches::render_plan_block` is the one place this is
  decided.
- **Module layout**: a facade file (`patch_eval.rs`, `effective.rs`,
  `plan.rs`) keeps the public API and re-exports its children in a
  same-named directory (`patch_eval/{identity,select,dispatch,
  standard_ops,custom_ops,tree}.rs`, `effective/{fold,blocks,
  counterfactual}.rs`, `plan/{fields,choices,rendering}.rs`). The
  facade's own former inline tests live in **one** sibling
  `<facade>_tests.rs` declared with `#[cfg(test)] #[path = "…"] mod
  tests;` — the split children have no test files of their own. An
  `insta` snapshot follows its test to that one test file's own
  `snapshots/` directory (insta resolves the directory from the *test's*
  source file).
- `assign.rs` (`assign/{read,mod_dependencies,render}.rs`), `emit.rs`
  (keeps `render` over `emit/{input,rendered,about,patches,
  rimmerge_json}.rs`), and `diff.rs` (`diff/{field_diff,structural,values,
  collisions}.rs`) have the same shape. `diff.rs` re-exports
  `field_value`/`is_keyed_map_at`/`is_confirmed_keyed_map` as
  `pub(crate)`. The `assign` and `emit` snapshots live in
  `assign/snapshots/` and `emit/snapshots/`. Don't give a child module
  the name of a function its facade re-exports (`mod_dependencies`, not
  `dependencies`): rustdoc then reports every link to that name as
  ambiguous.

## Custom operation classes are data, not code

`patch_behaviours.rs` holds the closed `CustomBehaviour`
(`SetModExtension`/`AddOrReplace`/`ReplaceResearchCoords`),
`GateBehaviour`, `ConditionalKind`, `ClassMatch`, and
`PatchOperationBehaviours` (class pattern -> behaviour, plus gates). Each
behaviour keeps its own handler in `patch_eval.rs`. **Which real class
has which behaviour is data**, loaded from the `rules` git submodule's
bundled snapshot (`rim-io`'s `build.rs` embeds
`rules/rimmerge-rules.json`, itself built from that repo's own
`data/patch-operations.json`) — or a fetched rimmerge-rules file
overriding it — into `rim_session::ModKnowledge` and threaded
through `ReplayContext::behaviours` and
`PatchCollisionInput::behaviours`; `PatchOperationBehaviours::none()` is
the empty set, the right value for a caller with no loaded data.
`CustomBehaviour::from_wire`/`GateBehaviour::from_wire`/
`ConditionalKind::from_wire`/`ClassMatch::from_wire` are the one piece of
wire vocabulary kept beside the enums, so adding a variant can't leave
its spelling behind. `tests/replay_parity.rs` **reads**
`rules/rimmerge-rules.json` directly (the same built, embedded bundle
`rim-io`'s `build.rs` compiles in) rather than restating its
`patch_operations` rows — identical output with the real map,
`Unsupported` with an empty one — with a floor on how many other
fixtures must still replay identically either way, so the comparison
can't go vacuous. **The structural detections stay name-free**:
`has_operations_child`, the `<match>`/`<nomatch>`/`<operation>` toggle
shape with no `<xpath>` of its own, and `STANDARD_CLASS_SUFFIXES` are
facts about RimWorld and XML shape, not about any specific mod — a doc
comment here may describe such a shape but must never name the class it
was measured on. A sequence-like class runs at its default toggle with
`Caveat::ModSettingDefault`; a class with fixed semantics dispatches to
its own handler; anything else is `Unsupported`. **`toggle_default`
itself lives in `rim-analyzer`** (`extract::patches::toggle_default`,
reading the first of `<enabled>`/`<defaultValue>`/`<default>` a class
declares) — `identity.rs` here re-exports it rather than keeping a
second copy, so the analyzer's own load-order edges
(`PatchOp::toggle_active`) and this replay never silently disagree about
which mods a default-off toggle actually runs.

## Xpath resolution and replay scoping

Ops are scoped by the replayed def's identity in `ReplayContext`: an op
targeting another def is treated as succeeded elsewhere; a Conditional or
Test on another def goes through `def_exists`.

`resolve_xpath`'s head-resolution chain is ordered, and the order is
**load-bearing** — strict `xpath_expr::parse` -> `head_content_predicate`
(Group B, child-value head) -> `head_filter_predicate` (`@ParentName=`,
`and`-composed, `not(...)`, nested child filters, `contains(text(), …)`,
bare-type heads) -> `xpath_target::parse_all`'s loose "names only other
defs" fallback -> `Err`. Group B's shape is a strict subset of the filter
query's; keeping it first stops a shipped, measured behaviour being
silently re-routed through newer code. All three call sites (a
mutation's own selection, a Conditional/Test's own test, and the branch
walk below) use the identical chain.

A Conditional whose own test can't be answered (another def with steps,
a root predicate/`text()`, an `@Name` head, or a rejected xpath) walks
its `<match>`/`<nomatch>` branches conservatively
(`branches_can_affect_this_def`) before falling back to "moot, treat as
succeeded elsewhere": an unmodelled class, an xpath naming this def, or a
non-additive document-root op all count as "could affect".

**Both head fallbacks evaluate with `root_predicate_matches`, never
`predicate_matches`.** `crate::xml::parse` lifts `Name`/`ParentName` off
the root element onto `FieldTree`, so a `[@ParentName="X"]` head
evaluated against `root.attrs` would answer `false` for every def on the
install — turning the largest skip shape into a silent no-op. Its
delegation to `predicate_matches` must stay an explicit, exhaustive
variant list, not a catch-all: a catch-all arm sends `Not` down the
wrong evaluator, so `not(@ParentName="X")` matches *every* def, the
exact inversion of the shape it's meant to exclude. Only a predicate
that applies to the def node itself belongs in `root_predicate_matches`;
a test against the root's children or text may be delegated.

**A condition is a claim about the whole document**, so a filter head
that selects nothing on this def is `Unsupported`, not `false`:
`condition_matches` can't answer it any more than `cross_def_existence`
can, and answering `false` would take a real Conditional's `<nomatch>`
branch or make a Test report `succeeded: false` — the exact prediction
`resolve_selection` refuses to emit for the same head. A **non-empty**
selection is still decisive. A head resolved by `head_filter_predicate`
**or by `head_content_predicate` (Group B)** never predicts a failure at
all, for the same reason one step further: both are *global* queries, so
an empty selection on the one def being replayed says nothing about the
operation's real outcome — such an op reports `Selected::Done(true)`,
pushes no `Caveat::FailedOp`, and increments
`ReplayOutcome::suppressed_filter_head_ops` (surfaced by `rimmerge
verify`) instead. **Group B used to predict from an empty selection**
(`non_predicting: false`); a real install's play-test log showed exactly
why that's wrong — a compat patch's `PatchOperationConditional` on a
child-value head decisively answered `false` on one def whose narrower
sub-path happened to be empty, and wrongly predicted its `nomatch`
branch would run, when the game's own whole-document query was `true`
elsewhere and never took that branch at all. A bare-type head is
additionally gated on `ReplayContext::this_def_present` so a
verify-pass zero-owner placeholder can never match it.

## `effective::compute`'s inheritance fold

Provenance is tracked via a "shadow" tree built and merged in lockstep
with the real one — never attribute an inherited field by diffing
rendered values before/after a merge step. A same-valued redeclaration
renders identically (nothing to diff), and a list item's identity can
only be known once its whole final sibling set exists, so any fix in
this area has to key off *position* in the real merge, not a path
computed early against a partial tree.

Three assumptions worth restating, since none are enforced by a type:
**templates are consumed unpatched** (a patch targeting a template by
`@Name` is not reflected in its children here, unlike the real game,
which patches the whole document before `XmlInheritance` resolves);
**losers' nodes are absent during replay** (`EffectiveInput::raw` is
always the winning owner's node alone, matching the real game, which
only ever has the winner's copy loaded); **agreeing patchers credit the
earlier mod** (two contributions setting the same field to the same
value produce no observable diff at the second one, so it stays credited
to whichever ran first — deliberately not reconciled with inheritance's
"declares always wins" rule, since a `PatchOperation` has no "not an
override" case to key a declare-based rule on).

**The list case**: when a new contribution's own `li` add collides with
an existing sibling's declared identity, the duplicate-identity fallback
renumbers *every* item sharing that identity to `ItemId::Position` —
including ones the new contribution never touched. `renamed_list_item_source`
looks for a `before`-only, no-longer-present `li` sibling whose node is
*exactly* the surviving one's and, if found, carries that sibling's own
prior provenance forward under the new path instead of misattributing it
to whoever merely triggered the renumbering.
`duplicate_of_earlier_sibling(effective, path)` (public, for callers like
`rim-session`'s conflict view) tells "this `Position`-identified item is
a genuine new addition" apart from "this is byte-identical to an earlier
sibling in its own collision group" — reads `effective.resolved`/
`provenance` directly, matching by real content equality, never a
candidate's own isolated snapshot.

## Keyed maps

`tree::ContainerKind`/`container_kind`/`keyed_map_entries` classify a
container `List`/`KeyedMap`/`Record` (a leaf-only record classifies
`KeyedMap` too, deliberately — there is no "known map tags" table).
`diff::EntryKind { Leaf, ListItem, MapEntry { container } }` tags each
compared field so an attributed keyed child (`MayRequire`, chiefly)
compares as `Value::Item` (attributes preserved) instead of silently
dropping them as a plain leaf. `diff::collision_fields` expands a
confirmed keyed map into one `FieldDiff` per key, or returns the single
whole-subtree field otherwise; `diff::is_keyed_map_at` returns `false`
for an empty path — a def's own root is never itself a keyed map.
`Caveat::ClobberedMapEntry { path, by }` fires when an auto-resolved
entry's chosen value is non-absent but the real, full-order replay's own
value is absent — a later mod's wholesale replace of the whole container
dropped an earlier mod's independent add — and restores it with an
`Add`. The mirror case (chosen resolves to absent while the real replay
has something real there) is a silent no-op: it's an isolated replay's
own attribution failing, not a genuine gap.

`plan_patch_collision`'s own per-mod candidate construction falls back
from an isolated per-mod replay to `move_mod_last` (this mod's own ops
replayed last, against the already-patched tree) whenever that mod's
isolated replay produces a `Caveat::FailedOp` naming it — an isolated
`Replace` of a key only another mod's earlier `Add` created can't
attribute correctly on its own, since it would read `Value::Absent`
regardless of what the op really does. After that fallback, the whole
per-mod candidate set is re-confirmed with `is_confirmed_keyed_map`
before being trusted: the isolated candidates just built might
themselves disagree about whether the container is still a keyed map
(e.g. one mod's own isolated replay removes the container's only key),
and if they do, the whole set falls back to `move_mod_last` instead
(never a partial mix of isolated and reordered trees). **`plan.rs`'s own
per-mod candidate-tree construction stays `pub(super)`, never `pub`** —
`plan_patch_collision` (`PatchCollisionOutcome { plan, fields }`) is the
only public surface; never re-expose a candidate-tree builder so a
caller could assemble its own, or a future correction to the
construction stops reaching every consumer by construction.

## The `TopLevelOutcome` channel

A second, parallel verdict alongside `Caveat`/`Completeness`, one per
top-level `<Operation>` a contribution replays (`succeeded`, an
`identity`, `failed_leaf_xpath: Option<String>`) — the *only* source for
`rimmerge verify`'s failure predictions, never surfaced through the
ordinary ledger/inbox path. `identity`/`failed_leaf_xpath` are tracked
**structurally**, via an out-parameter threaded through every
`apply_*` function, set once — the *first* node whose own
`apply_operation` call returns `false` after `apply_success_mode` is
applied — and never overwritten by a later sibling. **Never derive
`identity`/`leaf_xpath` by scanning `caveats`**: an earlier
`<success>Always</success>`-suppressed failure can still push its own
caveat (masking the real failure), and a bare `PatchOperationTest`
pushes no caveat at all, so a caveat-based scan disagrees with the real
stopping node. A future change to how a `<success>` mode swallows or
reports a child failure must keep naming the operation RimWorld itself
would report as `lastFailedOperation` in its own log.

`ReplayContext.this_def_present: bool` exists because `patch_eval::select`
seeds `candidates` with the root path unconditionally, so a bare
def-head xpath ("does this def merely exist") always "matches" — even
against a zero-owner synthetic placeholder that exists specifically to
represent a def with no active owner. `matched_paths` returns no match
when the resolved xpath has no steps **and** `this_def_present` is
false; every real caller sets it `true`, only the zero-owner placeholder
path sets it `false`. Getting this wrong silently inverts every
bare-def-head Conditional/Test against a def with no owner.

## `diff::structural_change` — a merge-safety guard, not a code path

Given a `ThreeWayDiff` plus its owner slice, reports whether any owner
differs from base in `thingClass`, `ParentName`, the root element's
`Class` attribute, or an *existing* `comps/li` entry's own `Class`
attribute — checked in that order, first hit wins. `ParentName`/root
`Class` are read directly off each owner's raw tree (neither is ever a
diffed field); a comp's `Class` is compared **by list position** on each
owner's resolved tree, never by `tree::ItemIdentity` — identity picks a
`li`'s own `Class` as its primary identity, so an identity-addressed
diff can never tell "this comp's class changed" apart from "this comp
was removed and an unrelated one was added"; position is the only
cross-owner correspondence left once identity itself is what changed. A
changed `ParentName` alone always triggers, with no
template-equivalence check — two owners' different `ParentName`s can
resolve to the same effective fields and still trip this.
`structural_change` returns `Result<Option<StructuralChange>,
BaseNotAnOwner>` and must fail *closed*, not open — `rim-session` uses
the return value to *gate* reclassification, and a silently-disabled
guard is the unsafe direction. This function only measures; it's
`rim-session`'s `PlanMerge` that decides what to do when it fires (see
`crates/rim-session/CLAUDE.md`). **Scope limit, documented not fixed**:
`StructuralField::CompClass` stays scoped to the literal `comps` path
only — `modExtensions/li[@Class=…]` is structurally the same failure
mode but is outside this guard's reach today.

## `assign.rs` is a thin adapter

The field-classification algorithm lives entirely in
`rim_resolve::domain::AssignmentSchema::infer_fields` (flat
`FieldPath`-keyed values, no tree) — `assign::infer` just calls it.
**Never re-implement classification here; extend `rim-resolve` instead**,
and use its public `majority_owner` rather than a local copy.
`assign::read_instance` is the only real tree-facing piece
(`FieldTree` -> `rim_resolve`'s `InstanceValues`); it never records a
top-level `defName` leaf as a field, since an instance's identity always
renders from `AssignmentRow::def_name`. `assign::render_rows`/
`dependencies` never approximate a field they can't safely render: a
field path only renders through every `Child` ancestor its `FieldPath`
names, a row's target-key field always renders from its own `TargetRef`,
a `Cardinality::Scalar` item slot renders as text only for exactly one
name, and a target with no `TargetGate` decision is skipped rather than
rendered ungated — every one of those is a recorded `SkippedField`, never
a guess. `build_node_for` takes an explicit `attrs: &BTreeMap<String,
String>` parameter (recovered by `attrs_for_entry` from the field's own
`base`/candidate `Value::Item` representations) so a free-text
`MergeChoice::Value` choice on an attributed keyed entry still keeps its
`MayRequire` — without it, only the `Value::Item`/`MergeChoice::From`
path preserved attributes and the free-text leaf arm silently dropped
them. `dependencies`/`standalone_dependencies` both exclude
`own_package_id` (`is_own_package`, base-normalised on both sides —
`ModId::base()` strips a `_steam` suffix independently of which side
carries it) from the owner set the exact same way they already exclude
Core: an `ItemSlot` value naming this project's own free-standing row
must never become a real external dependency on itself, however it
resolves. `emit::Dependencies::ExactlyWithLoadAfter { depends_on,
load_after_only }`: an assignment export's `modDependencies` is *exactly*
`depends_on`, and `loadAfter` is the **union** with `load_after_only` —
never the reverse, and `load_after_only` is never folded into
`depends_on` itself (a target gated only through a row's own
`MayRequire`, never a hard dependency, still needs a load-order
constraint but not a hard one). `render_sections(sections, gates)`
dispatches per row on its own `RowKey` (`Target` carries `MayRequire`
from `gates`; `Own` never consults `gates` at all) across a project's
whole `BTreeMap<String, Section>`, in map order — an `ItemSlot` value in
one section can legitimately name another section's own free-standing
row. **This engine performs no liveness check on an `ItemSlot` value** —
`rim-session`'s `validate_and_clean_sections` is the sole place that
checks a reference is still there; don't reintroduce a `KnownOwn`-shaped
guard here, since a type-scoped guard is *stricter* than the caller's
real "active def or current own instance" check and produces false
positives the moment a same-type section merely coexists.

## `effective::counterfactual` — the verify tool's "what-if" phase

`effective::counterfactual(input, baseline, subject)` answers "would a
different load order have made this operation work?" by re-running
`compute` with one mod's whole contribution block moved to each other
block position, accepting only strictly-better moves. Pure, no IO, no
index, deterministic — hermetically testable with no session and no
fixture profile.

- The unit moved is a **mod**, never an operation — a fix a user can
  apply is a pair rule between two mods; a per-operation shuffle models
  a reorder no rule can express.
- The verdict is per top-level operation, index-mapped through the
  permutation (never positionally). An alternative is refused, and
  counted, if it breaks a contribution that succeeded in the baseline or
  if the fold stops at or before the subject — a move that merely pushes
  the subject past a stopper must never read as success.
- **Scope, and it is the caller's job to respect it**: the experiment
  holds `EffectiveInput::winner`/`raw`/`templates` fixed, so it answers
  "under a different *patch* order", never "if a different mod owned
  this def" — a subject whose own mod co-owns the def alongside the
  winner must be excluded by the caller.
- Cost is `(K - 1)` calls to `compute`, i.e. `(K - 1) *
  contributions.len()` calls to `patch_eval::replay` (a block can hold
  more than one operation, so it is **not** `(K - 1) * K`). Capping `K`
  is the caller's job.
- Two invariants a caller leans on and must never quietly change:
  `compute` emits exactly one `top_level_outcomes` entry per contribution
  it actually reached, in the order it was given them; a stopped
  contribution contributes no outcome, so `top_level_outcomes.len()` is
  the count of contributions reached.
