//! [`MergeWorkspace`]: per-[`PreviewSlot`] cache of computed
//! [`MergePreview`]s — the selected order changes a contested def's
//! winner, so a preview computed under `Current` can't be reused for
//! `Suggested` (or vice versa); stored choices (mod ids, never positions)
//! still apply under either, which is what makes "decisions carry between
//! orders" hold even though previews themselves don't. A patch's own
//! previews (`PreviewSlot::patch`) never share a slot with the profile's
//! (`PreviewSlot::profile`) or with another patch's, even for the same
//! finding key under the same order — each patch decides independently.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_merge::diff::{DiffClass, FieldDiff, StructuralChange, ThreeWayDiff, Value};
use rim_merge::plan::{Caveat, MergePlan};
use rim_resolve::domain::{
    FieldPath, FindingKey, MergeChoice, MergeState, OrderSource, PatchId, PatchScope,
};

/// One preview cache bucket: the selected order plus, for a patch's own
/// preview, that patch's id. `patch: None` is the profile's own slot —
/// [`PreviewSlot::profile`] builds it; [`PreviewSlot::patch`] builds a
/// patch's.
///
/// Deliberately hand-rolled `PartialOrd`/`Ord` rather than derived:
/// [`OrderSource`] carries no ordering of its own (nothing besides this
/// slot ever needed one), so ranking it by declaration order here —
/// `Current` before `Suggested` — is this module's own concern, not
/// something `rim_resolve::domain::order` should grow just to satisfy a
/// `BTreeMap` key on this side of the crate boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSlot {
    /// Which load order this preview was computed against.
    pub source: OrderSource,
    /// `None` for the profile's own preview; `Some(id)` for that patch's.
    pub patch: Option<PatchId>,
}

impl PreviewSlot {
    /// The profile's own slot under `source`.
    #[must_use]
    pub fn profile(source: OrderSource) -> Self {
        Self {
            source,
            patch: None,
        }
    }

    /// The patch `id`'s own slot under `source` — never the same bucket as
    /// the profile's or another patch's, even for an identical finding key
    /// and order.
    #[must_use]
    pub fn patch(source: OrderSource, id: PatchId) -> Self {
        Self {
            source,
            patch: Some(id),
        }
    }

    fn source_rank(source: OrderSource) -> u8 {
        match source {
            OrderSource::Current => 0,
            OrderSource::Suggested => 1,
        }
    }
}

impl PartialOrd for PreviewSlot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PreviewSlot {
    fn cmp(&self, other: &Self) -> Ordering {
        Self::source_rank(self.source)
            .cmp(&Self::source_rank(other.source))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

/// One finding's computed merge preview: the owners in the selected
/// order, the base/winner, the field-level diff, the plan folding in
/// whatever choices are currently stored, and the resulting state.
#[derive(Debug, Clone)]
pub struct MergePreview {
    /// The finding this preview is for.
    pub key: FindingKey,
    /// Every owner, in the selected order.
    pub owners: Vec<ModId>,
    /// The earliest owner in the selected order.
    pub base: ModId,
    /// Whose raw node the emitted xpaths would target.
    pub winner: ModId,
    /// The field-level diff (or, for a patch collision, a single-field
    /// diff shaped the same way — see
    /// `crate::use_cases::plan_merge` for why that one is
    /// reconstructed rather than produced by `rim_merge` directly).
    pub diff: ThreeWayDiff,
    /// The plan computed from `diff` plus the decision's stored choices.
    pub plan: MergePlan,
    /// How much of `diff` still needs the user's input.
    pub state: MergeState,
    /// The structural guard (`rim_merge::diff::structural_change`), for a `DefOverride` only —
    /// always `None` for a `PatchCollision` (the guard never applies to one; see
    /// `PlanMerge::plan_patch_collision`'s own doc comment) and for a
    /// `CannotMerge` preview (no real `ThreeWayDiff`/owner data to
    /// evaluate the guard against).
    ///
    /// `Some` forces [`Self::state`] to [`MergeState::NeedsFieldInput`]
    /// regardless of `diff`/`plan` — see `crate::use_cases::plan_merge`'s
    /// own (private) `state_from_plan` for why that's a property of the
    /// preview as a whole rather than a per-field
    /// [`DiffClass`] reclassification. Carried here, not just returned
    /// from the planning call, so the merge editor can show one banner
    /// naming the triggering field/owner without recomputing the guard
    /// itself.
    pub structural_change: Option<StructuralChange>,
    /// The "final" value per field — the full-order replay's outcome for a
    /// `PatchCollision`
    /// ([`rim_merge::plan::PatchCollisionOutcome::final_values`], verbatim)
    /// or the merge's own resolved value for a `DefOverride`
    /// ([`rim_merge::plan::resolved_field_values`]) — distinct from
    /// `diff.fields`' own `candidates`, which never carries the whole
    /// picture and must not be shown as the outcome (see `rim_merge::plan`'s
    /// own doc comments for why the two kinds mean genuinely different
    /// things). A path absent from this map has no
    /// meaningful final value at all (an unresolved conflict, an invalid
    /// choice) — a caller must print a clearly-empty marker for it, never
    /// silently fall back to `candidates`. Always empty for a
    /// [`MergeState::CannotMerge`] preview (no real diff/plan to derive
    /// one from).
    pub final_values: BTreeMap<FieldPath, Value>,
}

impl MergePreview {
    /// Whether the replay answered at
    /// least one contributing op's mod-setting toggle from its own
    /// declared default rather than the user's real game — the one piece
    /// of knowledge both `Session::redecide_clean_merge_at` and
    /// `use_cases::merge_coverage::promotes_to_merge_85` need to pass
    /// `rim_resolve::domain::redecide_for_clean_merge`'s
    /// `assumed_mod_setting_defaults` argument. Lives here, not
    /// duplicated at each call site, so a change to what counts (or a
    /// future second `Caveat` variant that should also count) is made
    /// once.
    #[must_use]
    pub fn assumed_mod_setting_defaults(&self) -> bool {
        self.plan
            .caveats
            .iter()
            .any(|caveat| matches!(caveat, Caveat::ModSettingDefault { .. }))
    }

    /// The structural guard's own reduction of [`Self::structural_change`] to
    /// the plain value `rim_resolve::domain::redecide_for_clean_merge`'s
    /// `structural_guard_field` argument needs — that crate sits below
    /// `rim-merge` in the workspace's dependency graph
    /// (`rim-session -> rim-merge -> rim-resolve`) and can't import
    /// `rim_merge::diff::StructuralField` to format it itself, the same
    /// layering reason [`Self::assumed_mod_setting_defaults`] exists for
    /// `Caveat::ModSettingDefault`. Uses `StructuralField`'s own `Display`
    /// impl rather than a second, hand-written rendering.
    #[must_use]
    pub fn structural_guard_field(&self) -> Option<String> {
        self.structural_change
            .as_ref()
            .map(|change| change.field.to_string())
    }

    /// Filters and pages this preview's field diff for the merge editor —
    /// pure computation over already-cached data, factored out of
    /// `apps/desktop`'s DTO layer per `apps/desktop/CLAUDE.md` ("filtering,
    /// paging, and aggregation live in rim-session").
    #[must_use]
    pub fn field_page(&self, filter: &MergeFieldFilter) -> MergeFieldPage<'_> {
        let mut totals = MergeFieldTotals {
            fields: self.diff.fields.len(),
            unresolved: self.plan.unresolved.len(),
            ..MergeFieldTotals::default()
        };
        for field in &self.diff.fields {
            match field.class {
                DiffClass::Unchanged => totals.unchanged += 1,
                DiffClass::OneSided { .. } | DiffClass::Agreeing { .. } => totals.auto += 1,
                DiffClass::Conflict { .. } => totals.conflicts += 1,
            }
        }

        let search = filter.search.as_deref().map(str::to_lowercase);
        let matching: Vec<&FieldDiff> = self
            .diff
            .fields
            .iter()
            .filter(|field| {
                !filter.only_conflicts || matches!(field.class, DiffClass::Conflict { .. })
            })
            .filter(|field| match &search {
                Some(needle) if !needle.is_empty() => {
                    field.path.to_string().to_lowercase().contains(needle)
                }
                _ => true,
            })
            .collect();
        let total = matching.len();
        let limit = filter.limit.min(crate::MAX_PAGE_SIZE);
        let fields = matching
            .into_iter()
            .skip(filter.offset)
            .take(limit)
            .collect();

        MergeFieldPage {
            total,
            totals,
            fields,
        }
    }
}

/// Filters and pages [`MergePreview::field_page`]'s field list. `limit` is
/// capped at [`crate::MAX_PAGE_SIZE`] regardless of the requested value —
/// the same cap [`crate::FindingFilter`] applies to its own `limit`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeFieldFilter {
    /// Keep only `Conflict`-classified rows.
    pub only_conflicts: bool,
    /// Keep only rows whose field path text contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to return, capped at [`crate::MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// Row/field counts across a [`MergePreview`]'s whole diff, before
/// filtering or paging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeFieldTotals {
    /// Every field the diff carries.
    pub fields: usize,
    /// `Unchanged` fields.
    pub unchanged: usize,
    /// `OneSided`/`Agreeing` fields — resolve automatically.
    pub auto: usize,
    /// `Conflict` fields.
    pub conflicts: usize,
    /// Fields the plan still lists as unresolved.
    pub unresolved: usize,
}

/// One page of [`MergePreview::field_page`]'s field list: the totals
/// across the whole diff (before filtering), the number of rows matching
/// the filter (before paging), and the page of rows itself, in the diff's
/// own order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeFieldPage<'a> {
    /// Rows matching the filter, before paging.
    pub total: usize,
    /// Field counts across the whole diff, before filtering/paging.
    pub totals: MergeFieldTotals,
    /// The requested page of field rows, in the diff's own order.
    pub fields: Vec<&'a FieldDiff>,
}

/// A cached [`MergePreview`] plus the exact inputs
/// ([`super::use_cases::MergeContext::choices`]/`scope`) it was built
/// from — [`MergeWorkspace::matches`] compares against these to tell a
/// still-fresh cache entry from a stale one without recomputing anything,
/// per (slot, key, choices).
/// Anything *else* a preview depends on (the selected order, which mods
/// are active, a def's source text) is covered by this workspace's own
/// wholesale invalidation (`clear_all`/`clear_patch`/`clear_profile_finding`)
/// on every change that could move it — see `crate::Session::invalidate_ledgers`'s
/// own doc comment — so comparing just these two fields is sufficient.
#[derive(Debug, Clone)]
struct CachedPreview {
    preview: MergePreview,
    choices: BTreeMap<FieldPath, MergeChoice>,
    scope: Option<PatchScope>,
}

/// Per-[`PreviewSlot`] merge previews. An implementation detail of
/// [`crate::Session`]'s own caching — never part of this crate's public
/// port surface. A plain map (no fixed slot count) since the number of
/// port surface. A plain map (no fixed slot count) since the number of
/// live slots is unbounded: the profile's two (one per
#[derive(Debug, Clone, Default)]
pub(crate) struct MergeWorkspace {
    previews: BTreeMap<PreviewSlot, BTreeMap<FindingKey, CachedPreview>>,
}

impl MergeWorkspace {
    /// The cached preview for `key` under `slot`, if one has been computed
    /// since the last invalidation.
    #[must_use]
    pub fn get(&self, slot: &PreviewSlot, key: &FindingKey) -> Option<&MergePreview> {
        self.previews
            .get(slot)?
            .get(key)
            .map(|cached| &cached.preview)
    }

    /// Whether `slot`'s cached preview for `key` was built from exactly
    /// `choices`/`scope` — a true result means [`Self::get`] can be
    /// trusted as-is, with no rebuild, for a caller about to plan with
    /// this same context (`crate::use_cases::PlanMerge::execute_in`, the
    /// per-`(slot, key, choices)` preview cache). `false`
    /// covers both "nothing cached yet" and "cached under different
    /// inputs".
    #[must_use]
    pub fn matches(
        &self,
        slot: &PreviewSlot,
        key: &FindingKey,
        choices: &BTreeMap<FieldPath, MergeChoice>,
        scope: Option<&PatchScope>,
    ) -> bool {
        self.previews
            .get(slot)
            .and_then(|by_key| by_key.get(key))
            .is_some_and(|cached| cached.choices == *choices && cached.scope.as_ref() == scope)
    }

    /// Caches `preview` under `slot`, keyed by its own
    /// [`MergePreview::key`], remembering the `choices`/`scope` it was
    /// built from for [`Self::matches`]. There is no per-key removal on a
    /// *profile-wide* change (a decision, rule, or order change can shift
    /// which fields are conflicts for any contested def, so `clear_all`
    /// drops the whole workspace instead) — [`Self::clear_profile_finding`]
    /// is the one exception, for the one decision kind (`Merge`/`ShipAsset`)
    /// that provably never affects any other finding's own preview.
    pub fn set(
        &mut self,
        slot: PreviewSlot,
        preview: MergePreview,
        choices: BTreeMap<FieldPath, MergeChoice>,
        scope: Option<PatchScope>,
    ) {
        self.previews.entry(slot).or_default().insert(
            preview.key.clone(),
            CachedPreview {
                preview,
                choices,
                scope,
            },
        );
    }

    /// Drops every cached preview in every slot — a decision, rule, or
    /// order change can shift any contested def's winner or diff, so
    /// nothing survives it.
    pub fn clear_all(&mut self) {
        self.previews.clear();
    }

    /// Drops both of `id`'s own preview slots (one per [`OrderSource`]) —
    /// used when that patch's decisions or scope change. The profile's own
    /// slots and every other patch's are untouched.
    pub fn clear_patch(&mut self, id: &PatchId) {
        self.previews
            .retain(|slot, _| slot.patch.as_ref() != Some(id));
    }

    /// Drops `key`'s own cached preview in both of the profile's slots
    /// (one per [`OrderSource`]) — used by a profile `Merge`/`ShipAsset`
    /// decision, whose effect never reaches beyond its own finding: every
    /// other finding's own diff is built from that finding's own stored
    /// choices and the (unchanged) def sources, never from `key`'s. Every
    /// patch's own previews are untouched too, for the same reason
    /// [`Self::clear_patch`] never touches the profile's. This is
    /// `clear_all`'s narrower sibling.
    pub fn clear_profile_finding(&mut self, key: &FindingKey) {
        for source in [OrderSource::Current, OrderSource::Suggested] {
            if let Some(by_key) = self.previews.get_mut(&PreviewSlot::profile(source)) {
                by_key.remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_analyzer::domain::Selector;
    use rim_merge::diff::{EntryKind, Value};
    use rim_merge::plan::MergePlan;
    use rim_resolve::domain::{DefKey, PathSegment};

    use super::*;

    /// A hand-built preview with `count` `OneSided` fields named
    /// `field0..fieldN` — the real fixtures never have this many
    /// contested fields, so the page-size clamp needs a synthetic diff to
    /// exercise it.
    fn preview_with_fields(count: usize) -> MergePreview {
        let winner = ModId::new("winner.mod");
        let base = ModId::new("base.mod");
        let fields = (0..count)
            .map(|i| {
                // `entry` is the one field that actually says what kind
                // of field this is; `is_list_item` is a redundant,
                // hand-maintained twin of it (`FieldDiff` itself does
                // nothing to keep the two in
                // sync) — set `entry` first and derive `is_list_item`
                // from it, rather than picking two independent literals
                // that could silently disagree.
                let entry = EntryKind::Leaf;
                FieldDiff {
                    path: rim_merge::tree::FieldPath::new(vec![PathSegment::Child(format!(
                        "field{i}"
                    ))]),
                    base: Value::Leaf("base".to_string()),
                    candidates: BTreeMap::from([
                        (base.clone(), Value::Leaf("base".to_string())),
                        (winner.clone(), Value::Leaf("winner".to_string())),
                    ]),
                    class: DiffClass::OneSided { by: winner.clone() },
                    is_list_item: matches!(entry, EntryKind::ListItem),
                    entry,
                }
            })
            .collect();

        MergePreview {
            key: FindingKey::DefOverride {
                key: DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                },
                owners: [base.clone(), winner.clone()].into_iter().collect(),
            },
            owners: vec![base.clone(), winner.clone()],
            base: base.clone(),
            winner: winner.clone(),
            diff: ThreeWayDiff {
                base: base.clone(),
                fields,
            },
            plan: MergePlan {
                key: DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                },
                selector: Selector::DefName,
                winner: winner.clone(),
                owners: vec![base, winner],
                ops: Vec::new(),
                unresolved: Vec::new(),
                caveats: Vec::new(),
            },
            state: MergeState::Complete { op_count: 0 },
            structural_change: None,
            final_values: BTreeMap::new(),
        }
    }

    /// Pins the
    /// predicate directly, so swapping which `Caveat` variant it matches
    /// (e.g. `FailedOp` instead of `ModSettingDefault`) fails a test
    /// instead of silently changing `redecide_for_clean_merge`'s
    /// confidence at both call sites with no coverage anywhere.
    #[test]
    fn assumed_mod_setting_defaults_is_true_only_when_that_caveat_is_present() {
        let mut preview = preview_with_fields(0);
        preview.plan.caveats = vec![Caveat::FailedOp {
            mod_id: ModId::new("a.mod"),
            xpath: "/Defs/ThingDef[defName=\"Wall\"]/label".to_string(),
        }];
        assert!(!preview.assumed_mod_setting_defaults());

        preview.plan.caveats.push(Caveat::ModSettingDefault {
            mod_id: ModId::new("b.mod"),
            class: "SomeMod.PatchOperationCustomSequence".to_string(),
        });
        assert!(
            preview.assumed_mod_setting_defaults(),
            "must find ModSettingDefault even when it isn't the first caveat"
        );
    }

    #[test]
    fn structural_guard_field_is_none_until_a_structural_change_is_set() {
        let mut preview = preview_with_fields(0);
        assert_eq!(preview.structural_guard_field(), None);

        preview.structural_change = Some(rim_merge::diff::StructuralChange {
            field: rim_merge::diff::StructuralField::ThingClass,
            by: ModId::new("winner.mod"),
        });
        assert_eq!(
            preview.structural_guard_field().as_deref(),
            Some("thingClass"),
            "must render through StructuralField's own Display, not a second hand-written string"
        );
    }

    fn default_filter() -> MergeFieldFilter {
        MergeFieldFilter {
            only_conflicts: false,
            search: None,
            offset: 0,
            limit: 200,
        }
    }

    #[test]
    fn the_field_page_is_clamped_to_max_page_size_regardless_of_the_requested_limit() {
        let preview = preview_with_fields(crate::MAX_PAGE_SIZE + 50);

        let page = preview.field_page(&MergeFieldFilter {
            limit: 10_000,
            ..default_filter()
        });

        assert_eq!(page.total, crate::MAX_PAGE_SIZE + 50);
        assert_eq!(page.fields.len(), crate::MAX_PAGE_SIZE);
    }

    #[test]
    fn only_conflicts_keeps_conflict_rows_only() {
        let preview = preview_with_fields(5);

        let page = preview.field_page(&MergeFieldFilter {
            only_conflicts: true,
            ..default_filter()
        });

        assert_eq!(
            page.total, 0,
            "every synthetic field is OneSided, never Conflict"
        );
        assert_eq!(page.totals.fields, 5);
    }

    #[test]
    fn search_matches_the_field_path_case_insensitively_before_paging() {
        let preview = preview_with_fields(5);

        let page = preview.field_page(&MergeFieldFilter {
            search: Some("FIELD2".to_string()),
            ..default_filter()
        });

        assert_eq!(page.total, 1);
        assert_eq!(page.fields[0].path.to_string(), "field2");
    }

    #[test]
    fn offset_skips_leading_matches() {
        let preview = preview_with_fields(5);

        let page = preview.field_page(&MergeFieldFilter {
            offset: 3,
            limit: 10,
            ..default_filter()
        });

        assert_eq!(page.fields.len(), 2);
        assert_eq!(page.fields[0].path.to_string(), "field3");
    }

    #[test]
    fn totals_count_the_whole_diff_regardless_of_the_page() {
        let preview = preview_with_fields(5);

        let page = preview.field_page(&MergeFieldFilter {
            limit: 2,
            ..default_filter()
        });

        assert_eq!(page.totals.fields, 5);
        assert_eq!(page.totals.auto, 5, "every synthetic field is OneSided");
        assert_eq!(page.totals.conflicts, 0);
        assert_eq!(page.fields.len(), 2, "the page itself is still limited");
    }

    fn patch_id() -> PatchId {
        "abcdef012345".parse().expect("12 lowercase hex chars")
    }

    /// Caches `preview` with an empty choices map and no scope — every
    /// pre-existing test in this module only cares about slot/key
    /// scoping, not the `(choices, scope)` fingerprint `matches` compares.
    fn set_default(workspace: &mut MergeWorkspace, slot: PreviewSlot, preview: MergePreview) {
        workspace.set(slot, preview, BTreeMap::new(), None);
    }

    /// A preview cached under the profile's own slot (`patch: None`) is
    /// invisible under a patch's slot for the same order and key, and vice
    /// versa — the whole point of keying [`MergeWorkspace`] by
    /// [`PreviewSlot`] rather than just [`OrderSource`].
    #[test]
    fn a_preview_is_scoped_to_its_own_slot_profile_and_patch_never_share_a_bucket() {
        let mut workspace = MergeWorkspace::default();
        let preview = preview_with_fields(1);
        let key = preview.key.clone();
        let profile_slot = PreviewSlot::profile(OrderSource::Current);
        let patch_slot = PreviewSlot::patch(OrderSource::Current, patch_id());

        set_default(&mut workspace, profile_slot.clone(), preview.clone());
        assert!(
            workspace.get(&patch_slot, &key).is_none(),
            "a profile preview must not leak into a patch's own slot"
        );

        set_default(&mut workspace, patch_slot.clone(), preview);
        assert!(
            workspace.get(&profile_slot, &key).is_some(),
            "caching under the patch slot must not disturb the profile's own cached preview"
        );
        assert!(workspace.get(&patch_slot, &key).is_some());
    }

    /// Two patches (or a patch and the profile) never share a bucket even
    /// under the same [`OrderSource`] — each id gets its own slot.
    #[test]
    fn two_different_patch_ids_under_the_same_order_get_separate_slots() {
        let mut workspace = MergeWorkspace::default();
        let preview = preview_with_fields(1);
        let key = preview.key.clone();
        let first: PatchId = "abcdef012345".parse().expect("valid id");
        let second: PatchId = "012345abcdef".parse().expect("valid id");

        set_default(
            &mut workspace,
            PreviewSlot::patch(OrderSource::Current, first.clone()),
            preview,
        );

        assert!(
            workspace
                .get(&PreviewSlot::patch(OrderSource::Current, second), &key)
                .is_none()
        );
        assert!(
            workspace
                .get(&PreviewSlot::patch(OrderSource::Current, first), &key)
                .is_some()
        );
    }

    /// [`MergeWorkspace::clear_patch`] drops only the named patch's own two
    /// slots — the profile's and another patch's both survive.
    #[test]
    fn clear_patch_drops_only_that_patchs_own_slots() {
        let mut workspace = MergeWorkspace::default();
        let preview = preview_with_fields(1);
        let key = preview.key.clone();
        let target: PatchId = "abcdef012345".parse().expect("valid id");
        let other: PatchId = "012345abcdef".parse().expect("valid id");
        let profile_slot = PreviewSlot::profile(OrderSource::Current);
        let target_current = PreviewSlot::patch(OrderSource::Current, target.clone());
        let target_suggested = PreviewSlot::patch(OrderSource::Suggested, target.clone());
        let other_slot = PreviewSlot::patch(OrderSource::Current, other.clone());
        set_default(&mut workspace, profile_slot.clone(), preview.clone());
        set_default(&mut workspace, target_current.clone(), preview.clone());
        set_default(&mut workspace, target_suggested.clone(), preview.clone());
        set_default(&mut workspace, other_slot.clone(), preview);

        workspace.clear_patch(&target);

        assert!(workspace.get(&target_current, &key).is_none());
        assert!(workspace.get(&target_suggested, &key).is_none());
        assert!(
            workspace.get(&profile_slot, &key).is_some(),
            "the profile's own slot must survive clearing a patch"
        );
        assert!(
            workspace.get(&other_slot, &key).is_some(),
            "another patch's slot must survive clearing a different one"
        );
    }

    /// [`MergeWorkspace::clear_all`] drops every slot, profile and patch
    /// alike — what [`crate::Session::invalidate_ledgers`] relies on.
    #[test]
    fn clear_all_drops_every_slot() {
        let mut workspace = MergeWorkspace::default();
        let preview = preview_with_fields(1);
        let key = preview.key.clone();
        let profile_slot = PreviewSlot::profile(OrderSource::Current);
        let patch_slot = PreviewSlot::patch(OrderSource::Suggested, patch_id());
        set_default(&mut workspace, profile_slot.clone(), preview.clone());
        set_default(&mut workspace, patch_slot.clone(), preview);

        workspace.clear_all();

        assert!(workspace.get(&profile_slot, &key).is_none());
        assert!(workspace.get(&patch_slot, &key).is_none());
    }

    /// [`MergeWorkspace::matches`] is `true` only when a preview is cached
    /// under the exact `(slot, key, choices, scope)` tuple — a different
    /// choices map, a different scope, or nothing cached at all must all
    /// report `false`.
    #[test]
    fn matches_is_true_only_for_the_exact_choices_and_scope_it_was_cached_with() {
        let mut workspace = MergeWorkspace::default();
        let preview = preview_with_fields(1);
        let key = preview.key.clone();
        let slot = PreviewSlot::profile(OrderSource::Current);
        let mut choices = BTreeMap::new();
        choices.insert(
            preview.diff.fields[0].path.clone(),
            MergeChoice::From {
                mod_id: preview.winner.clone(),
            },
        );

        assert!(
            !workspace.matches(&slot, &key, &choices, None),
            "nothing cached yet must never match"
        );

        workspace.set(slot.clone(), preview.clone(), choices.clone(), None);

        assert!(
            workspace.matches(&slot, &key, &choices, None),
            "the exact choices/scope it was cached with must match"
        );
        assert!(
            !workspace.matches(&slot, &key, &BTreeMap::new(), None),
            "a different choices map must not match"
        );
        let scope = PatchScope::new([preview.base.clone(), preview.winner.clone()])
            .expect("two distinct members");
        assert!(
            !workspace.matches(&slot, &key, &choices, Some(&scope)),
            "a different scope must not match"
        );
    }

    /// [`MergeWorkspace::clear_profile_finding`] drops only `key`'s own
    /// preview, in both of the profile's own order slots — a different
    /// finding key and every patch slot survive untouched.
    #[test]
    fn clear_profile_finding_drops_only_that_findings_own_profile_slots() {
        let mut workspace = MergeWorkspace::default();
        let decided = preview_with_fields(1);
        let key = decided.key.clone();
        let mut other = preview_with_fields(1);
        other.key = FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "OtherDef".to_string(),
            },
            owners: other.owners.iter().cloned().collect(),
        };
        let other_key = other.key.clone();
        let current = PreviewSlot::profile(OrderSource::Current);
        let suggested = PreviewSlot::profile(OrderSource::Suggested);
        let patch_slot = PreviewSlot::patch(OrderSource::Current, patch_id());
        set_default(&mut workspace, current.clone(), decided.clone());
        set_default(&mut workspace, suggested.clone(), decided.clone());
        set_default(&mut workspace, current.clone(), other.clone());
        set_default(&mut workspace, patch_slot.clone(), decided);

        workspace.clear_profile_finding(&key);

        assert!(workspace.get(&current, &key).is_none());
        assert!(workspace.get(&suggested, &key).is_none());
        assert!(
            workspace.get(&current, &other_key).is_some(),
            "a different finding's own profile preview must survive"
        );
        assert!(
            workspace.get(&patch_slot, &key).is_some(),
            "a patch's own preview for the same key must survive a profile-only clear"
        );
    }
}
