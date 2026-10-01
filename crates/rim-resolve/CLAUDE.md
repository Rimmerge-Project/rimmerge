# rim-resolve

Pure domain: rules, tag inference, the sorter, the merge-evaluator's
assignment domain (a patch-maker project's schema, shape inference,
coverage), and the resolution ledger. No IO, no XML parsing, no Tauri.

## Layer

Sits directly above `rim-analyzer::domain` and below `rim-merge` in the
crate graph. May depend on `rim-analyzer::domain` only.

## Invariants

- Every collection that reaches `SortOutcome` or `Ledger` is
  BTree-ordered; the proptests compare whole `SortOutcome`s across input
  shuffles, in both `TieBreak` modes.
- The confidence table **is** the code in `ledger/suggest.rs` —
  `docs/concepts/ledger.md` describes it at a higher level and must stay
  in sync, but the code is the source of truth.
- `FindingKey` and `FieldPath` have canonical text forms persisted in
  `decisions.json`; `PatchId`'s text form (12 lowercase hex chars) is a
  patch project's file name. Changing either type's `Display`/`FromStr`
  is a file-format change: bump the store version in `rim-io` and add a
  migration. The same rule extends to `Action`/`MergeChoice`'s
  `Serialize`/`Deserialize` — `PatchProject::decisions_sha256` hashes
  `Action`'s `serde_json` wire format directly, so a renamed field or
  reordered variant changes every already-exported compat patch's
  `decisionsSha256` too, not just `decisions.json`'s shape.
- `PatchScope::membership` and `ledger::findings::hidden_by_generated`
  are both exhaustive matches over every `FindingKey` variant, no `_`
  arm — a new variant is a compile error at both sites on purpose, so
  neither rule can silently default wrong for it.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rim-resolve --all-targets --all-features -- -D warnings
cargo nextest run -p rim-resolve --all-features
```
Test helpers (fixtures, proptest generators) are behind the
`test-support` feature, already on with `--all-features`. Snapshots use
`insta`; read every `.snap.new` diff before accepting, never accept
blindly. Before reporting done, run the full gate block in the root
`CLAUDE.md`.

## Conventions

- **Golden fixture**: `tests/golden/report.synthetic.json`, produced
  entirely by the real analyzer pipeline against a synthetic, invented
  install (`tests/fixtures/synthetic-install.json`, built by `rimmerge
  fixture gen`) — never real mod data. Regeneration procedure lives in
  `sort_golden.rs`'s own `# regenerate` doc comment; read that before
  touching this fixture, and run `rg -i -f .github/forbidden-tokens.txt`
  over the regenerated file before committing it (see `docs/testing.md`).
  `the_synthetic_golden_is_large_and_nontrivial` is the anti-vacuous-green
  floor for this fixture specifically.
- **Structural equivalence proof**: `examples/anonymize_report.rs` and
  `sort_equivalence.rs` give an independent, name-free proof that
  renaming every identifier in a report (mod ids, names, authors, def
  names, ...) cannot change what the sorter produces —
  `anonymize_report` deterministically maps every identifier to an
  invented token; `sort_equivalence` sorts both the real and anonymized
  reports and asserts the anonymized order is exactly the mapped image
  of the real one, structurally, edge by edge and conflict-kind by
  conflict-kind. Both are safe to run against a real report (their
  output is gitignored, never committed) and neither ships any data of
  its own.
- **Module layout**: `ledger/suggest.rs` is a facade over
  `suggest/{edges,defs,assets,rules,mods}.rs`; `domain/finding.rs` is a
  facade re-exporting `DefKey`/`FindingKey` from `finding/key.rs` and
  `FindingKeyParseError` from `finding/key_text.rs`. Each facade's own
  former inline tests live in **one** sibling `<facade>_tests.rs`
  (`suggest/suggest_tests.rs`, `finding/finding_tests.rs`) declared with
  `#[cfg(test)] #[path = "…"] mod tests;` — the split children have no
  test files of their own. Follow this shape for any new split.
- `domain/patch.rs` is a facade over `patch/{identity,scope,project}.rs`,
  and `ledger/findings.rs` keeps `extract` over
  `findings/{provenance,generated,conflicts}.rs`; their tests are
  `patch/patch_tests.rs` and `findings/findings_tests.rs`.
  `domain/assignment.rs` re-exports
  `assignment/{schema,shape,project,section,precedence,coverage}.rs` and
  keeps its tests in `assignment/assignment_tests.rs`. The facade's
  `#[cfg(test)]` imports serve those tests through `use super::*`.

## The sorter

`EnforcedLayers` decides which edge layers the sorter treats as hard
constraints versus advisories: `Hard`/`Declared` edges, rules, and any-of
constraints are enforced unconditionally; `Soft`/`Awareness` are
advisory by default; `Layer::Inferred` (heuristic edges — currently
`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/
`PatchInvalidatesPredicate`/`PatchRemovedNodeCosmetic`/
`ReplaceDiscardsAddition`) is enforced by
default too — a heuristic
edge is the analyzer's own conclusion, not a mere presence signal, so
leaving it advisory-only would discard the evidence it exists to act on.
There is no cluster layer or cluster anchor: a shared-tag contiguity
rule never earned its own complexity against what it actually caught.
`ClusterRuleId` and `Action::ExcludeFromCluster` exist only so an old
`rules.json`/`decisions.json` with cluster data still deserializes;
current code never produces either.

`EdgeKind::ParentTemplate` is `EdgeStrength::Hard`, not `Awareness` —
ground-truthed against decompiled `Verse.XmlInheritance`: inheritance
resolution is a proven load-time failure mode, not something that
"resolves after every mod's defs load regardless of order". Because it's
`Hard`, it's never advisory, so `ledger::suggest::relation_explanation`'s
old `ParentTemplate` arm (explaining why an advisory relation needs no
order) is unreachable for it.

`SortInput.tie_break: TieBreak` picks the base key unconstrained mods
emit in: `PreserveCurrent` uses the current order's position; `Rebuild`
(the default) ranks by `sort::graph::normalized_rebuild_name`
(case-insensitive, a leading bracketed/parenthesised tag or version
prefix stripped first), then `ModId`. Membership (which mods become
nodes at all) is the active list either way; `DisturbanceStats` is
always measured against the real current order in both modes.
**`Rebuild` does not propagate at all** (`sort::emit::non_propagated_keys`).
Emission is minimal-disturbance **in `PreserveCurrent` only, and only
through an edge carrying an author's or the user's own order statement, or
an `Inferred` heuristic edge** (`sort::emit::propagates`) — never through
a `Soft`/`Awareness` edge, evidence with no author or user behind it at
all. There is no Framework tier (tiers are Core/Dlc/Top/Body/Bottom).

Within `PreserveCurrent`, a violated edge's key can move in either
direction: `sort::emit::pull_keys` (pull a late prerequisite forward to
its earliest dependent) and `sort::emit::push_keys` (push a dependent
back to its prerequisite) are both tried, gated per pair by
`sort::direction::edge_directions` — computed once for the whole graph,
not per edge. For a pair already satisfied in the base order, the
direction is always Pull: a no-op unless the dependent itself was pulled
by something else in turn, in which case the pull chain carries the
prerequisite along too. For a pair violated in the base order, the
direction is whichever side's *position-aware* cost is strictly
smaller — the sum, over every mod that option would actually move, of
how far it has to travel, read off rank-indexed `fixedbitset` ancestor/
descendant closures over the same propagating subgraph `propagates`
selects; a mod already on the correct side of the target costs nothing.
Counting distance rather than mod count is what tells "one mod moving
700 slots" apart from "a dozen mods each moving 5". On an exact tie, an
edge carrying an author's or the user's own order statement resolves to
Pull; an `Inferred`-only pair resolves to Push instead, so a bare
heuristic edge whose two options genuinely cost the same still moves the
dependent, never the prerequisite. Correctness never depends on which
direction wins, since the Kahn loop still enforces every accepted edge
strictly via in-degree tracking — only how well the result minimizes
disturbance is at stake. The two results combine as **saturating signed
deltas from `base`**
(`base.saturating_sub(pull_delta).saturating_add(push_delta)`), never a
direct sum of the two raw keys: `base` is `usize::MAX` for a tier
sentinel (never itself pulled or pushed, so both its own deltas are
always `0`), and a raw `pull.key + push_key` sum would overflow long
before reaching one.

An `Inferred` edge propagates because it is still the analyzer's own
conclusion, not a mere presence signal — the same reasoning
`EnforcedLayers::inferred`'s own default rests on. Position-aware cost
is what makes that safe: a heuristic edge only ever pulls a late
prerequisite forward when doing so is *provably* cheaper than pushing its
dependent back, counting every mod either option would actually drag
along, never merely "the dependent waits". A bare "the dependent simply
waits" rule, with no cost comparison at all, can leave real disturbance
on the table whenever the dependent itself has a further declared
downstream of its own — pushing it back then drags that whole chain
too, which a cost comparison catches and a fixed rule cannot.

An explicit `Top`/`Bottom` placement always sorts to the extreme edge of
its tier via `placement_bias`, but a node ready in an earlier Kahn round
can still jump ahead of a different pin's dependent chain within the
same round — `sort::emit::extremize_region` is a second, bounded pass
after the main Kahn loop that moves each pin's whole *closure* (itself
plus every region member transitively reachable through accepted edges)
to the region's extreme as one block, largest closure first, ties on
base-order position. `Top`'s region is bounded on its inner side by one
past the last `Core`/`Dlc`-tagged mod (never by the first `Top`-tagged
position, which degenerates to a no-op region when only one `Top` pin
exists); **`Bottom` is asymmetric on purpose**: its own inner bound
stays the true end of the order, since `Bottom` is the terminal tier and
a mod forced to follow a `Bottom` pin can legitimately land past the
last `Bottom`-tagged member. The invariant this buys: within a region,
every mod positioned beyond any explicit pin of that placement is itself
such a pin or lies in some pin's closure. `placement_bias`/`bias_of` are
`0` for a **promoted** mod (`TierReason::PromotedBy`) — they only match
an explicit `TierReason::Placement`, so a mod dependency-promoted into a
pinned region never gets that region's own extreme-edge bias; only an
author/user-declared pin does.

Two findings disclose placement facts the reasoning above can produce,
neither with an alternative to offer: `FindingKey::PlacementOrderingOverridden`
(per colliding mod: a specific mod's accepted edge crosses a *different*,
still-holding pin's own extreme edge — the edge always wins, this is
disclosure of a correct fact, not a decision point) and
`FindingKey::PlacementPromotesDependents` (per pin: how many other mods
are attributed to a holding placement's own promotion past its tier
boundary, direct and transitive alike, read off every mod whose own
`TierReason::PromotedBy` names this pin — that field is set by
`sort::cycles::find_promotion_cause`'s own nearest-placed-node BFS during
cycle-breaking, an **attribution, not an exhaustive reach count**: the
walk stops at the first placed node it reaches, so a mod promoted past
two pins' own boundaries is attributed to only one of them).

## Hard problems (`preflight`)

`preflight::hard_problems(report, order, ledger)` re-evaluates the report
against the order that will be written (never the report's scan-time
`EdgeReport::status`) and returns a sorted, de-duplicated
`Vec<PreflightItem>`. The set is the closed `HardProblem` enum; `Ord` on it
is the display order, so a new variant is a compile error in
`finding_key` and in every interface that renders it. The ledger is read
only to set `acknowledged` (`ResolutionStatus::UserOverridden` on the
problem's own `FindingKey`); it never adds or removes a problem. An
any-of problem has no finding key and is never acknowledged. A missing
dependency compares through `ModId::base()` and a mod that is kept in the
order but not on disk does not satisfy a requirement.

## The ledger

`ledger::suggest`'s `winner_confidence` scores an `EdgeDropped` finding
by *both* the winner's layer and the dropped edge's own strength, not
the winner's layer alone: a `Hard`/`AnyOf` winner scores 95 regardless of
the loser; a `Declared`/`UserDecision` winner scores 95 over a strictly
weaker loser but falls through to the base table's "needs input" row
over an equally-`Declared` loser (a same-layer tie broken only by the
drop tie-break is not one author's word beating another's); a db-origin
winner (RimSortUser/RimSortCommunity/SteamDb) scores 70 over a
`Declared` loser but falls through over a `Soft`/`Awareness` loser; a
`Soft`/`Awareness` winner always falls through. `EdgeDropped`'s and
`DeclarationQuestioned`'s `Reorder` alternative is gated to a **db-layer**
winner/declaration only — never offered against `Hard`, `Declared`, or a
`UserDecision`, so a one-click override never sits next to a fact or a
declaration as if it were as casual as picking between two databases. A
dropped `Rule { origin: UserDecision }` edge always emits
`SortWarning::UserDecisionOverruled`, so it cannot vanish silently.
`FindingKey::RuleOverruled`/`PlacementOverruled`/`PlacementQuestioned`
mirror this shape for a dropped *rule*-origin edge and an overruled
placement pin — the loser there is always a rule with no `EdgeStrength`
of its own, so `rule_overruled_confidence` scores by the winner's layer
and the loser's own origin instead: `Hard`/`AnyOf` winner 95;
`DeclaredOverride` winner over a loser that itself overrides a
declaration 55 (one override beating another isn't a clean win);
`DeclaredOverride`/`Declared` winner otherwise 90; `UserDecision` winner
over a `UserDecision`-origin loser 55 (the tie-break decided it, not one
decision outranking another); `UserDecision` winner otherwise 90;
`RimSortUser`/`RimSortCommunity`/`SteamDb` winner 70;
`Soft`/`Awareness`/`Inferred` winner 55.

`domain::redecide_for_clean_merge` (`domain/resolution.rs`) is a
*second* pass over `suggest.rs`'s own output, applied by `rim-session`
(never here — this crate has no XML to build a merge preview from), once
a `DefOverride`/`PatchCollision` suggestion already offers `Merge` as an
alternative. Its own sibling, `redecide_for_identical_copies`, always
promotes straight to `Action::Accept` at confidence 99 with no
alternatives — no `MergeState` branching at all, since two byte-identical
copies leave nothing for load order to affect; `ledger::def_override_direction`
exists specifically because `SameAuthor` and `LoneNonVanillaOwner`
render as identical `Suggestion` shapes, so `rim-session`'s own gate for
which redecision to run can't tell them apart from the suggestion alone
and needs this classifier instead. **Neither redecision reaches every
suggestion**: the ordinary confidence-90-and-above `DefOverride` rows
(`WinnerDeclaresRelation`/`SameAuthor`/`LoneNonVanillaOwner`) and an
`Additive` patch collision never offer `Merge` as an alternative in the
first place, so they're never candidates for either pass; `rim-session`
also only runs `redecide_for_clean_merge` at all when
`suggestMergeWhenClean` is on and a `DefSourceReader` is wired, which is
never true for `apps/cli`'s `ledger --report` (it builds no preview).
Whether a redecided `Accept` actually shows as `Auto` still depends on
the profile's own threshold — `Confidence::meets` is `>=`, so a
threshold above 95 still leaves a zero-op `PatchCollision` promotion at
`NeedsInput`.

`redecide_for_clean_merge` itself branches on the merge preview's own
`MergeState`:

- `Complete { op_count > 0 }`: promotes `Merge` to the suggestion itself
  for a `PatchCollision` (confidence 85, mirroring RimWorld's own
  sequential patch composition), but for a `DefOverride` only ever
  **leads with** `Merge` in the alternatives (`lead_with_merge`) —
  `action`/`confidence` untouched — since a field-merged def override is
  a def copy no author shipped, unlike a patch-collision merge, which
  only mirrors patches every active mod already runs.
- `Complete { op_count == 0 }` (nothing to merge — the game applies the
  contributions in sequence to one value): promotes to `Action::Accept`
  at confidence 95, or 80 when the caller's `assumed_mod_setting_defaults`
  argument is `true` (the preview carried a mod-setting-default caveat);
  `Merge` is removed from the alternatives outright, `PreferWinner`
  alternatives are kept.
- `NeedsFieldInput`: reorders `Merge` to the front of the alternatives
  rather than replacing the action. A fifth parameter,
  `structural_guard_field: Option<&str>`, additionally strips `Merge`
  from the alternatives outright (never leads with it) here and replaces
  the rationale with one naming the guard's own field, whenever
  `rim_merge::diff::structural_change` fired for this def — that state
  can never become `Complete` however the fields are chosen, so offering
  `Merge` there is advice with no action the user could take.
- `CannotMerge`: changes nothing.

Every arm above is gated by `suggestion.action` already being
`Action::Accept`: a suggestion whose direction lives in the *action*
itself (e.g. a `PreferWinner`-based warning) only ever has its no-op
`Merge` alternative stripped, never its action/confidence overwritten.

`DecisionSet::sorter_overrides(&self, template_children)` is what turns
a stored decision back into the overrides the sorter consumes;
`template_children` is a second input the caller must supply (`rim-resolve`
has no XML/analyzer access of its own to derive it) — every non-vanilla,
non-registrant mod with a child referencing a duplicated template
`Name`, keyed by that `Name`. A `PreferWinner` decision on a
`DuplicateTemplateName` finding orders the winner after every other
registrant (as it does for every other finding kind this loop serves)
**and**, when `template_children` has an entry for that name, before
every one of that template's own genuinely-eligible children — the
opposite direction from the "after every registrant" rule, and the
correct one: a registration placed after a child is invisible to it. An
empty `template_children` map is always safe: the finding then simply
gets no children-side reorders.

## Rule import and classification

`Settings.use_imported_pairs`/`use_imported_placements` (owned by
`rim-session`) gate which imported rules the sorter sees; `domain::rule::
PairRuleEvidence`/`classify_pair_rule(&PairRule, &Report)` (pure,
IO-free) classifies an imported pair rule against every edge between the
same two mods: `Redundant` when a covering edge agrees (in either
direction, a rule agreeing with *any* derived evidence isn't "novel"),
`Contradicting` when an opposing edge exists. A db-origin pair rule
opposed by an `Awareness` edge already produces `DeclarationQuestioned`
through the ordinary layer match — there is no separate finding kind for
a contradicting import.
`FindingKey::RuntimePatchCollision` is grouped **per target**
(`{target_type, target_method, owners}`), not per mod pair — a single
popular runtime-patch target's owner count fans out quadratically into pairs,
so grouping by pair produces thousands of near-duplicate rows for one
real collision; the ledger's finding count is the analyzer's own
`Conflict` count 1:1 under this shape. `EdgeKind::ModDependency` is
`Declared`-strength and does feed the sorter: RimWorld itself never
orders by `modDependencies`, but RimSort treats a declared dependency as
an implicit `loadAfter`, and so does this engine.

## `DefRef` and def-name resolution

`DefRef` has three text forms: `<def_type>/<def_name>`,
`<def_type>/@<def_name>` (a template `Name`), and `name_only`/`@<name>`
with no `def_type` at all (`is_name_only()`) — the last is what
`FindingKey::DuplicateTemplateName::def_ref()` returns, since
`rim-analyzer`'s template index is keyed by `Name` alone. `FromStr`
rejects leading/trailing whitespace on every part
(`DefRefParseError::Whitespace`) rather than trimming it — a real
template `Name` can contain interior spaces, and silently trimming risks
matching a mistyped ref to a real one. A caller crossing a URL must
percent-encode the text form itself; `DefRef` does neither for you.

## Assignment domain (the patch maker)

`AssignmentSchema::infer_fields`'s cardinality rule: a field is `List`
the moment a **majority** of its own occurrences are `li`-wrapped, or at
least two are — never a bare majority-of-one-instance vote, and never
"any instance", which one malformed instance could flip. RimWorld's own
`List<T>` deserializer accepts a bare unwrapped scalar as a one-element
list shorthand, so a sparse but genuinely list-shaped field still needs
`List` to be usable at all.

The vote that decides which type owns an `ItemSlot`'s namespace has a
suffix exemption: a field whose leaf tag ends `Def`/`Defs` drops the
resolved-value count floor to `resolved >= 1` instead of the plain
`MIN_RESOLVED_DISTINCT` (5) — a field can have many distinct raw values
where only one resolves, when the rest belong to inactive mods. A field
rescued *only* by this exemption skips the inside/outside vote entirely
and classifies `ItemSlot` unconditionally, never `TargetKey` — a
one-value vote is too thin to trust for who owns the namespace, but a
low-evidence `<fooDef>` field is always a value the user picks, never
the row's identity. The exemption applies for either cardinality; a
field that already clears the floor on its own is untouched and still
goes through the ordinary vote. `has_def_suffixed_leaf_tag` is
deliberately case-sensitive — a conservative miss judged safer than a
case-insensitive match that might rescue an unrelated field.

**The `chance<Slot>` pairing convention** (`FieldRole::Chances`): a
float-list field whose tag is `chance` plus another top-level field's own
tag, case-insensitively (`chancePrimaryTool` <-> `primaryTool`), pairs to
that sibling `ItemSlot` field by name — classified only in a second pass,
after every reference field's role is already known, so the item slot it
pairs to is available to match against. A `Chances` field is edited
alongside its paired slot and `AssignmentProject::set_row` enforces they
stay the same length; a float list whose name doesn't pair to a
classified `ItemSlot` falls back to `Opaque` rather than being guessed
at.

`AssignmentProject.sections: BTreeMap<String, Section>`, one `{schema,
rows: BTreeMap<RowKey, AssignmentRow>}` per def type; `RowKey::Target(TargetRef)`
vs `RowKey::Own(String)` (free-standing rows) is decided structurally by
`Section::is_standalone()` (`!schema.has_target_key()`), never a separate
flag. A `RowKey::Own(name)` key's own string **must** equal
`row.def_name` exactly (`AssignmentRowError::OwnKeyDefNameMismatch`) —
otherwise `Section::own_instance_names()` and the row's own rendered
`defName` could diverge into a dangling reference nothing downstream
catches. `coverage()` takes an explicit `def_type`; a standalone section
has no meaningful coverage (no `TargetKey` field to match a candidate
through). A scalar `ItemSlot` must hold **exactly one** name —
`AssignmentRowError::ScalarItemSlotWrongCount`, checked in
`AssignmentProject::set_row`'s own `validate_value` — mirroring the same
invariant `rim-merge`'s `render_leaf` and the desktop's `ItemPicker.vue`
single-select UI both already enforce on their own sides; without this
check a caller with no picker of its own (the CLI) could set multiple
names on a scalar slot and only fail, silently, at export.
`AssignmentProject::section_mut` is a **raw, unvalidated** mutable
accessor (`Option<&mut Section>`) — removing a value through it is
always safe, an absent field behaves like `RowValue::Omit`, but prefer
`set_row` for anything that adds or changes a value, since `section_mut`
does none of `set_row`'s own shape/role checks.

## Compat patches

`domain/patch.rs`'s `PatchScope::membership` decides Full/Partial/Outside
per `FindingKey` (see Invariants — exhaustive, no `_` arm).
`PatchProject`'s own `DecisionSet` is independent of the profile's;
nothing in this crate looks one up from the other.
