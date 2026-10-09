//! [`Session`]'s merge editor state: cached previews and inspections, def-conflict views, the merge
//! context, and re-deciding clean merges after a change.

use std::collections::BTreeSet;

use rayon::prelude::*;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    Action, DefKey, DefRef, Finding, FindingKey, GeneratedModIdentity, MergeFindingKind,
    OrderSource, PatchId, Resolution, ResolutionStatus, redecide_for_clean_merge,
    redecide_for_identical_copies,
};
use rim_resolve::ledger::{self};

use super::Session;
use super::{UnknownPatch, adjust_stat, mods_by_id};
use crate::merge_workspace::{MergeFieldFilter, MergeFieldPage, MergePreview, PreviewSlot};
use crate::use_cases::{MergeContext, PlanMerge, read_owner_def_raw, stored_choices};

/// The def and kind of merge [`Session::redecide_clean_merge_at`] plans
/// for `entry`: an undecided `DefOverride`/`PatchCollision` finding whose
/// suggestion already offers `Merge` as an alternative. `None` for every
/// other entry.
fn clean_merge_target(entry: &Resolution) -> Option<(DefKey, MergeFindingKind)> {
    if entry.decision.is_some() {
        return None;
    }
    let (def_key, finding_kind) = match &entry.key {
        FindingKey::DefOverride { key, .. } => (key.clone(), MergeFindingKind::DefOverride),
        FindingKey::PatchCollision { key, .. } => (key.clone(), MergeFindingKind::PatchCollision),
        _ => return None,
    };
    let offers_merge = entry.suggestion.alternatives.iter().any(
        |alt| matches!(&alt.action, Action::Merge { key: alt_key, .. } if *alt_key == def_key),
    );
    offers_merge.then_some((def_key, finding_kind))
}

impl Session {
    /// Merge-first suggestions for clean previews: re-derives `source`'s cached
    /// ledger entry at `key`, provided it's a `DefOverride`/
    /// `PatchCollision` finding with no decision whose ledger-computed
    /// suggestion already offers `Merge` as an alternative — ensuring a
    /// merge preview exists first (via [`crate::use_cases::PlanMerge`],
    /// when [`Session::set_def_source_reader`] has been called; a no-op
    /// otherwise) and re-deriving the suggestion/status from it through
    /// [`rim_resolve::domain::redecide_for_clean_merge`]. `entry_index`
    /// is stable across the whole call: nothing here inserts or removes
    /// ledger entries, only mutates one in place.
    ///
    /// Deliberately lazy, per finding — not run for the whole ledger
    /// inside `ensure_ledger`. Eagerly running this across every finding
    /// on every ledger build of a large real install exceeds the ledger
    /// build's 2s budget (see `apps/desktop`'s
    /// `real_install_timing::ledger_build_with_and_without_the_clean_merge_pass`).
    /// Called instead from [`Session::resolution`] (which every displayed
    /// finding, page or single lookup, already goes through) for just the
    /// one finding being resolved, and from
    /// [`crate::use_cases::RenderMergeMod`] for every candidate at apply
    /// time — a heavier, deliberate one-shot action outside the inbox's
    /// own hot path, where paying the full cost once is acceptable.
    ///
    /// A preview failing (a stale scan, an unsupported xpath, a missing
    /// source entry, ...) never fails the call: it's simply skipped,
    /// keeping the entry's ledger-computed suggestion.
    ///
    /// [`crate::use_cases::PlanMerge::execute`] always builds and caches
    /// its preview under `session.selected()`, not an explicit
    /// [`OrderSource`] parameter — but this can be asked to redecide
    /// *either* source's entry regardless of which is currently selected
    /// (`RenderMergeMod` always matches; a direct `Session::resolution`
    /// call for the non-selected source, from `explain_placement`-style
    /// commands, might not). `selected` is therefore swapped to `source`
    /// for the duration of the preview computation and restored
    /// immediately after, rather than widening `PlanMerge`'s own
    /// contract for this one caller.
    ///
    /// Returns whether the entry actually changed — idempotent (a second
    /// call on an already-redecided entry always returns `false`), which
    /// [`Session::findings`] relies on to know when its own cached
    /// [`FindingIndex`] (`sorted`/`needs_input_by_mod`, computed once from
    /// whatever the ledger looked like *before* any redecision) has gone
    /// stale and needs rebuilding.
    pub(crate) fn redecide_clean_merge_at(
        &mut self,
        source: OrderSource,
        key: &FindingKey,
    ) -> bool {
        if !self.rules.settings.suggest_merge_when_clean {
            return false;
        }
        let Some(reader) = self.def_source_reader.clone() else {
            return false;
        };
        let slot = Self::slot(source);
        let Some(entry_index) = self.finding_index[slot]
            .as_ref()
            .and_then(|index| index.index_of(key))
        else {
            return false;
        };
        let Some((def_key, finding_kind)) = self.ledgers[slot]
            .as_ref()
            .and_then(|ledger| clean_merge_target(&ledger.entries[entry_index]))
        else {
            return false;
        };

        let preview_slot = PreviewSlot::profile(source);
        if self.merge_preview(&preview_slot, key).is_none() {
            let planner = PlanMerge::new(reader);
            let previous_selected = self.selected;
            self.selected = source;
            // Mapped to `()` immediately: holding onto the `Ok`
            // reference would tie this `Result` to a borrow of `self`,
            // which the very next line (restoring `selected`) needs
            // mutably.
            let plan_result = planner.execute(self, key).map(|_| ());
            self.selected = previous_selected;
            if plan_result.is_err() {
                return false;
            }
        }
        // "The preview assumed default
        // mod settings" is `MergePreview::assumed_mod_setting_defaults` —
        // one place shared with `use_cases::merge_coverage::promotes_to_merge_85`,
        // rather than each call site re-deriving it from
        // `rim_merge::plan::Caveat::ModSettingDefault` independently.
        let Some((state, assumed_mod_setting_defaults, structural_guard_field)) =
            self.merge_preview(&preview_slot, key).map(|preview| {
                (
                    preview.state.clone(),
                    preview.assumed_mod_setting_defaults(),
                    preview.structural_guard_field(),
                )
            })
        else {
            return false;
        };

        let Some(ledger) = self.ledgers[slot].as_mut() else {
            return false;
        };
        let entry = &mut ledger.entries[entry_index];
        let new_suggestion = redecide_for_clean_merge(
            entry.suggestion.clone(),
            &def_key,
            &state,
            assumed_mod_setting_defaults,
            structural_guard_field.as_deref(),
            finding_kind,
        );
        if new_suggestion == entry.suggestion {
            return false;
        }
        let old_status = entry.status;
        let new_status = if new_suggestion
            .confidence
            .meets(self.rules.settings.threshold)
        {
            ResolutionStatus::Auto
        } else {
            ResolutionStatus::NeedsInput
        };
        entry.status = new_status;
        entry.effective = new_suggestion.action.clone();
        // The state pill (`resolution.merge`, rendered off `Some` alone)
        // must show for an auto-suggested merge exactly like a decided
        // one — only reachable when the suggestion actually became
        // `Merge` (a `NeedsFieldInput` reorder never does; see
        // `redecide_for_clean_merge`'s own doc comment).
        if matches!(new_suggestion.action, Action::Merge { .. }) {
            entry.merge = Some(state);
        }
        entry.suggestion = new_suggestion;
        if new_status != old_status {
            adjust_stat(&mut ledger.stats, old_status, -1);
            adjust_stat(&mut ledger.stats, new_status, 1);
        }
        true
    }

    /// Builds, on the replay pool (`crate::replay_pool`), every merge preview
    /// [`Self::redecide_clean_merge_at`] would build for `source`'s
    /// cached ledger, and caches each one in ledger order — so a caller
    /// about to redecide every entry (`RenderMergeMod`) pays for the
    /// previews in parallel instead of one at a time. The same previews
    /// end up cached either way: each is a pure function of the session
    /// and its finding, and only the entries that call would plan for are
    /// planned here. A preview that fails to build is not cached, so the
    /// redecision skips it exactly as before: such a build is attempted twice
    /// (here in parallel, then again by the sequential pass) with the same
    /// outcome both times.
    pub(crate) fn prefetch_clean_merge_previews(&mut self, source: OrderSource) {
        if !self.rules.settings.suggest_merge_when_clean {
            return;
        }
        let Some(reader) = self.def_source_reader.clone() else {
            return;
        };
        let Some(ledger) = self.ledgers[Self::slot(source)].as_ref() else {
            return;
        };
        let slot = PreviewSlot::profile(source);
        // The same build condition `redecide_clean_merge_at` applies (no
        // preview cached at all, whatever its choices), so a stale preview
        // it would reuse is never replaced here.
        let pending: Vec<(MergeContext, &FindingKey)> = ledger
            .entries
            .iter()
            .filter(|entry| {
                clean_merge_target(entry).is_some()
                    && self.merge_preview(&slot, &entry.key).is_none()
            })
            .filter_map(|entry| {
                let ctx = self.merge_context(slot.clone(), &entry.key).ok()?;
                Some((ctx, &entry.key))
            })
            .collect();
        let planner = PlanMerge::new(reader);
        let session: &Session = self;
        let built: Vec<(MergeContext, MergePreview)> = crate::replay_pool::install(|| {
            pending
                .into_par_iter()
                .filter_map(|(ctx, key)| {
                    let preview = planner.build_preview(session, &ctx, key).ok()?;
                    Some((ctx, preview))
                })
                .collect()
        });
        for (ctx, preview) in built {
            self.cache_merge_preview(ctx, preview);
        }
    }

    /// Compares the copies before calling a def override contested:
    /// re-derives `source`'s cached
    /// `DefOverride` ledger entry at `key` when every active owner's own
    /// copy turns out to be structurally identical — order genuinely
    /// cannot change the outcome, so the suggestion becomes `Accept` 99
    /// with no alternatives (see [`rim_resolve::domain::redecide_for_identical_copies`]).
    ///
    /// Only attempted for a finding whose suggestion carries no direction
    /// of its own: [`rim_resolve::ledger::def_override_direction`]'s
    /// `SameAuthor`/`Unknown` cases (see that function's own doc comment
    /// for why confidence/shape alone can't be used to tell these apart
    /// from `LoneNonVanillaOwner`, which must **not** run the check even
    /// though it also renders `Accept` 90 with only `PreferWinner`
    /// alternatives). `WinnerDeclaresRelation`/`LoneNonVanillaOwner`/
    /// `ShadowsFramework` are left alone even when the copies do happen to
    /// agree — the ledger already has a stronger signal for those. The
    /// direction is judged with the entry's own `winner`, which is the
    /// winner under `source`'s order.
    ///
    /// Structural identity is `rim-merge`'s own canonical form: each
    /// owner's raw XML is read back through the [`DefSourceReader`] and
    /// parsed into a [`rim_merge::tree::FieldTree`] — whose derived
    /// `PartialEq` already ignores whitespace and attribute order (a
    /// `BTreeMap` for attributes, trimmed text) — rather than comparing raw
    /// text or building a second, new normalisation. A read/parse failure
    /// on *any* owner (a stale scan; a def that exists only as a patch
    /// injection and so has no `Defs/`-inline source to read at all —
    /// `use_cases::read_owner_def_raw`'s own doc comment) never fails the
    /// call: the entry's existing suggestion is simply kept, exactly like
    /// [`Session::redecide_clean_merge_at`]'s own preview-failure handling.
    ///
    /// Deliberately lazy, per finding, mirroring
    /// [`Session::redecide_clean_merge_at`] exactly — same call sites, same
    /// 2s eager-pass budget reasoning (that method's own doc comment).
    ///
    /// Returns whether the entry actually changed — idempotent, same
    /// contract as [`Session::redecide_clean_merge_at`].
    pub(crate) fn redecide_identical_copies_at(
        &mut self,
        source: OrderSource,
        key: &FindingKey,
    ) -> bool {
        let Some(reader) = self.def_source_reader.clone() else {
            return false;
        };
        let slot = Self::slot(source);
        let Some(entry_index) = self.finding_index[slot]
            .as_ref()
            .and_then(|index| index.index_of(key))
        else {
            return false;
        };
        let Some((def_key, owners)) = self.ledgers[slot].as_ref().and_then(|ledger| {
            let entry = &ledger.entries[entry_index];
            if entry.decision.is_some() {
                return None;
            }
            let (def_key, owners, winner) = match &entry.finding {
                Finding::DefOverride {
                    key,
                    owners,
                    winner,
                } => (key.clone(), owners.clone(), winner.clone()),
                _ => return None,
            };
            let mods_by_id = mods_by_id(&self.report);
            let direction = ledger::def_override_direction(
                &self.report,
                &def_key,
                &owners,
                &winner,
                &mods_by_id,
            );
            matches!(
                direction,
                ledger::DefOverrideDirection::SameAuthor | ledger::DefOverrideDirection::Unknown
            )
            .then_some((def_key, owners))
        }) else {
            return false;
        };

        let mut trees = Vec::with_capacity(owners.len());
        for owner in &owners {
            match read_owner_def_raw(&reader, self, &def_key.def_type, &def_key.def_name, owner) {
                Ok(tree) => trees.push(tree),
                Err(_) => return false,
            }
        }
        let Some((first, rest)) = trees.split_first() else {
            return false;
        };
        if !rest.iter().all(|tree| tree == first) {
            return false;
        }

        let Some(ledger) = self.ledgers[slot].as_mut() else {
            return false;
        };
        let entry = &mut ledger.entries[entry_index];
        let new_suggestion = redecide_for_identical_copies(entry.suggestion.clone());
        if new_suggestion == entry.suggestion {
            return false;
        }
        let old_status = entry.status;
        let new_status = if new_suggestion
            .confidence
            .meets(self.rules.settings.threshold)
        {
            ResolutionStatus::Auto
        } else {
            ResolutionStatus::NeedsInput
        };
        entry.status = new_status;
        entry.effective = new_suggestion.action.clone();
        entry.suggestion = new_suggestion;
        if new_status != old_status {
            adjust_stat(&mut ledger.stats, old_status, -1);
            adjust_stat(&mut ledger.stats, new_status, 1);
        }
        true
    }

    /// The cached merge preview for `key` under `slot`, if one has been
    /// computed since the last invalidation of that slot. Read-only —
    /// building or refreshing a preview is
    /// [`crate::use_cases::PlanMerge`]'s job, since only it holds the
    /// `DefSourceReader` port a preview needs.
    #[must_use]
    pub fn merge_preview(&self, slot: &PreviewSlot, key: &FindingKey) -> Option<&MergePreview> {
        self.merges.get(slot, key)
    }

    /// Filters and pages the cached merge preview's field diff for `key`
    /// under `slot` — `None` when no preview is cached yet (building one
    /// is [`crate::use_cases::PlanMerge`]'s job).
    #[must_use]
    pub fn merge_fields(
        &self,
        slot: &PreviewSlot,
        key: &FindingKey,
        filter: &MergeFieldFilter,
    ) -> Option<MergeFieldPage<'_>> {
        self.merge_preview(slot, key)
            .map(|preview| preview.field_page(filter))
    }

    /// Whether `slot`'s cached preview for `key` was already built from
    /// exactly `ctx.choices`/`ctx.scope` — a `true` result means
    /// [`Session::merge_preview`] can be trusted as-is for this context,
    /// with no rebuild (the
    /// per-`(slot, key, choices)` cache). `pub(crate)` for the same reason
    /// as [`Self::cache_merge_preview`].
    #[must_use]
    pub(crate) fn merge_preview_matches(&self, ctx: &MergeContext, key: &FindingKey) -> bool {
        self.merges
            .matches(&ctx.slot, key, &ctx.choices, ctx.scope.as_ref())
    }

    /// Caches `preview` under `ctx.slot`, remembering `ctx.choices`/`ctx.scope`
    /// as the fingerprint [`Self::merge_preview_matches`] compares against.
    /// `pub(crate)` since only [`crate::use_cases::PlanMerge`] (in the same
    /// crate) should ever write to this cache.
    pub(crate) fn cache_merge_preview(&mut self, ctx: MergeContext, preview: MergePreview) {
        self.merges.set(ctx.slot, preview, ctx.choices, ctx.scope);
    }

    /// The cached [`crate::use_cases::DefInspection`] for `def_ref` under
    /// `source`, if one has been computed since the last invalidation
    /// (see [`Self::inspections`]'s own doc comment for exactly when
    /// that is). Read-only — computing one is
    /// [`crate::use_cases::InspectDef`]'s job.
    ///
    /// `self.inspections` is keyed by the exact `DefRef` it was inspected
    /// under — a caller that has a [`rim_resolve::domain::FindingKey`]
    /// (e.g. [`Self::def_conflict_view`]) must pass `key.def_ref()`
    /// verbatim, not a ref it reconstructs by hand: a `DuplicateTemplateName`
    /// finding's own `def_ref()` is always name-only
    /// ([`rim_resolve::domain::DefRef::name_only`]), and a typed
    /// `Selector::NameAttr`/`DefName` ref built from the same finding's
    /// `key`/`selector` fields instead would miss the cache entirely.
    #[must_use]
    pub fn inspection(
        &self,
        source: OrderSource,
        def_ref: &DefRef,
    ) -> Option<&crate::use_cases::DefInspection> {
        self.inspections.get(&(source, def_ref.clone()))
    }

    /// Caches `inspection` under `(source, def_ref)`. `pub(crate)` since
    /// only [`crate::use_cases::InspectDef`] should ever write to this
    /// cache.
    ///
    /// Bounded at [`Self::MAX_CACHED_INSPECTIONS`] entries
    /// a session
    /// that stays open a long time and inspects many different defs would
    /// otherwise grow this cache without limit (unlike `merges`, which
    /// only ever holds one entry per finding key currently in the
    /// ledger — an inspection has no such natural ceiling, since any def
    /// or template in the install can be inspected). The policy is
    /// deliberately blunt, matching this cache's own simple point-lookup
    /// shape: once inserting `inspection` would exceed the cap, the
    /// *whole* cache is cleared first rather than evicting one entry
    /// (no LRU bookkeeping to maintain) — a full recompute of whatever
    /// gets inspected again next is cheap relative to the bookkeeping a
    /// finer eviction policy would add, and this path is rare in
    /// practice (256 distinct `(OrderSource, DefRef)` pairs in one
    /// session).
    pub(crate) fn cache_inspection(
        &mut self,
        source: OrderSource,
        def_ref: DefRef,
        inspection: crate::use_cases::DefInspection,
    ) {
        if self.inspections.len() >= Self::MAX_CACHED_INSPECTIONS {
            self.inspections.clear();
        }
        self.inspections.insert((source, def_ref), inspection);
    }

    /// The cached [`crate::use_cases::DefGraphic`] for `def_ref` under
    /// `source`, if one was computed since `orders` last changed.
    #[must_use]
    pub fn def_graphic(
        &self,
        source: OrderSource,
        def_ref: &DefRef,
    ) -> Option<&crate::use_cases::DefGraphic> {
        self.def_graphics.get(&(source, def_ref.clone()))
    }

    /// Caches `graphic`; once inserting would pass
    /// [`Self::MAX_CACHED_DEF_GRAPHICS`] the whole cache clears first (the
    /// same blunt policy as [`Self::cache_inspection`]). `pub(crate)`:
    /// only [`crate::use_cases::ResolveDefGraphic`] writes here.
    pub(crate) fn cache_def_graphic(
        &mut self,
        source: OrderSource,
        def_ref: DefRef,
        graphic: crate::use_cases::DefGraphic,
    ) {
        if self.def_graphics.len() >= Self::MAX_CACHED_DEF_GRAPHICS {
            self.def_graphics.clear();
        }
        self.def_graphics.insert((source, def_ref), graphic);
    }

    /// How many resolutions are cached, for a bound test.
    #[cfg(test)]
    pub(crate) fn cached_def_graphic_count(&self) -> usize {
        self.def_graphics.len()
    }

    /// Overwrites the cached inspection's own `findings` field in place —
    /// a no-op if there is no cache entry at `(source, def_ref)`. Every
    /// other field of a cached [`crate::use_cases::DefInspection`] is
    /// invariant under a non-resorting decision, but `findings`' own
    /// [`rim_resolve::domain::ResolutionStatus`] is exactly what such a
    /// decision changes;
    /// [`crate::use_cases::InspectDef::execute`] calls this on every
    /// cache hit rather than serving the entry's stale `findings`.
    /// `pub(crate)`, same as [`Self::cache_inspection`].
    pub(crate) fn refresh_inspection_findings(
        &mut self,
        source: OrderSource,
        def_ref: &DefRef,
        findings: Vec<(FindingKey, ResolutionStatus)>,
    ) {
        if let Some(inspection) = self.inspections.get_mut(&(source, def_ref.clone())) {
            inspection.findings = findings;
        }
    }

    /// Builds a field-by-field conflict view for one def-shaped finding
    /// (`DefOverride`/`PatchCollision`/`DuplicateTemplateName`). A pure join
    /// over an already-cached
    /// [`crate::use_cases::DefInspection`]/[`MergePreview`] under
    /// [`Self::selected`]'s order: it computes nothing new and reads no
    /// file, so a caller must have already run
    /// [`crate::use_cases::InspectDef::execute`] (and, for
    /// `DefOverride`/`PatchCollision`, [`crate::use_cases::PlanMerge::execute`])
    /// on `key`'s own def first.
    ///
    /// # Errors
    ///
    /// See [`crate::DefConflictViewError`].
    pub fn def_conflict_view(
        &self,
        key: &FindingKey,
    ) -> Result<crate::DefConflictView, crate::DefConflictViewError> {
        crate::def_conflict_view::build(self, key)
    }

    /// [`Self::def_conflict_view`], scoped to a compat patch's own
    /// preview — the same field-by-
    /// field view, but joining the [`MergePreview`] cached under that
    /// patch's own slot (`PreviewSlot::patch(self.selected(), patch_id)`)
    /// and that patch's own decisions, rather than always the profile's.
    /// Mirrors exactly how `get_merge_preview` scopes [`crate::use_cases::
    /// PlanMerge`] to a patch: build the context via
    /// [`Self::merge_context`], plan under it
    /// (`PlanMerge::execute_in`), then call this — the same precondition
    /// [`Self::def_conflict_view`] already has (must already be inspected/
    /// planned under this exact slot), scoped to the patch instead of the
    /// profile.
    ///
    /// # Errors
    ///
    /// See [`crate::DefConflictViewError`]. Never errors on an unknown
    /// `patch_id` itself here — a stale id reaching this call is already a
    /// caller bug the same way it would be for [`Self::merge_context`],
    /// which every real caller (`get_def_conflict_view`) already runs
    /// first and propagates [`UnknownPatch`] from.
    pub fn def_conflict_view_for_patch(
        &self,
        patch_id: &PatchId,
        key: &FindingKey,
    ) -> Result<crate::DefConflictView, crate::DefConflictViewError> {
        let slot = PreviewSlot::patch(self.selected(), patch_id.clone());
        crate::def_conflict_view::build_for_slot(self, key, slot)
    }

    /// [`Self::def_conflict_view`], but for a `DefOverride` whose own
    /// `PlanMerge::execute`/`execute_in` call the caller already ran and
    /// swallowed (see [`crate::Problem::PlanFailed`]'s own doc comment):
    /// appends
    /// the failure as a `Problem` rather than letting it vanish silently
    /// just because the *winning* owner's own chain (all of
    /// [`crate::use_cases::InspectDef`]'s own `Problem`s are derived
    /// from) resolved cleanly. `plan_failure: None` behaves identically
    /// to a plain [`Self::def_conflict_view`] call.
    ///
    /// # Errors
    ///
    /// See [`crate::DefConflictViewError`].
    pub fn def_conflict_view_with_plan_failure(
        &self,
        key: &FindingKey,
        plan_failure: Option<&crate::use_cases::PlanMergeError>,
    ) -> Result<crate::DefConflictView, crate::DefConflictViewError> {
        crate::def_conflict_view::build_with_plan_failure(
            self,
            key,
            PreviewSlot::profile(self.selected()),
            plan_failure,
        )
    }

    /// [`Self::def_conflict_view_for_patch`] combined with
    /// [`Self::def_conflict_view_with_plan_failure`] — see both for why
    /// each exists.
    ///
    /// # Errors
    ///
    /// See [`crate::DefConflictViewError`].
    pub fn def_conflict_view_for_patch_with_plan_failure(
        &self,
        patch_id: &PatchId,
        key: &FindingKey,
        plan_failure: Option<&crate::use_cases::PlanMergeError>,
    ) -> Result<crate::DefConflictView, crate::DefConflictViewError> {
        let slot = PreviewSlot::patch(self.selected(), patch_id.clone());
        crate::def_conflict_view::build_with_plan_failure(self, key, slot, plan_failure)
    }

    /// Builds the [`MergeContext`] a preview computed under `slot` should
    /// use: the profile's own stored choices out of [`Session::decisions`]
    /// when `slot.patch` is `None`, or that patch's own stored choices and
    /// scope when it names one.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when `slot.patch` names a patch this
    /// session doesn't have loaded — never silently degrades to an
    /// unscoped context under a patch slot. A stale patch id reaching
    /// here is always a caller bug (every patch use case already checks
    /// [`Session::patch`]/[`Session::patch_snapshot`] first), so surfacing
    /// it as an error rather than guessing an unrestricted preview keeps
    /// that bug from producing a plausible-looking but wrong plan.
    pub fn merge_context(
        &self,
        slot: PreviewSlot,
        key: &FindingKey,
    ) -> Result<MergeContext, UnknownPatch> {
        let Some(patch_id) = slot.patch.clone() else {
            let choices = stored_choices(self.decisions(), key);
            return Ok(MergeContext {
                slot,
                choices,
                scope: None,
            });
        };
        let project = self.patches.get(&patch_id).ok_or(UnknownPatch(patch_id))?;
        Ok(MergeContext {
            slot,
            choices: stored_choices(project.decisions(), key),
            scope: Some(project.scope().clone()),
        })
    }

    /// Every Rimmerge-generated mod id the report marks (by marker or the
    /// `rimmerge.merge.` prefix fallback), plus every loaded patch
    /// project's own package id — a project not yet exported still must
    /// not be picked as a new patch's scope member or collide with a new
    /// patch's package id.
    #[must_use]
    pub fn reserved_package_ids(&self) -> BTreeSet<ModId> {
        let mut ids: BTreeSet<ModId> = self
            .report
            .mods
            .iter()
            .filter(|m| m.generated.is_some() || GeneratedModIdentity::is_generated(&m.id))
            .map(|m| m.id.base())
            .collect();
        ids.extend(
            self.patches
                .values()
                .map(|project| project.identity().package_id().clone()),
        );
        ids
    }
}
