//! [`PlanMerge`]: builds (and caches) a contested def's merge preview
//! against [`Session::selected`]'s order.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{LoadOrder, ModId, Selector, XmlLocator};
use rim_merge::diff::ThreeWayDiff;
use rim_merge::patch_eval::PatchContribution;
use rim_merge::plan::{self, Caveat, PatchCollisionInput};
use rim_merge::tree::FieldPath;
use rim_resolve::domain::{DefKey, FindingKey, MergeChoice, PatchScope};

use crate::Session;
use crate::merge_workspace::{MergePreview, PreviewSlot};
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources::{self, field_path_from_sub_path};
use owners::{ordered, partition_by_scope};
use state::{cannot_merge_preview, state_from_plan};

mod context;
mod owners;
mod state;

pub use context::{MergeContext, PlanMergeError};
pub(crate) use owners::build_owner_versions;
pub(crate) use state::stored_choices;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "plan_merge/plan_merge_tests.rs"]
mod tests;

/// Builds (and caches) one finding's [`MergePreview`], against
/// [`Session::selected`]'s order.
pub struct PlanMerge<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> PlanMerge<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// This use case's own port, for a caller that needs to read a def's
    /// sources through the identical reader a preview was built with —
    /// [`super::merge_coverage::MergeCoverage`] is the one caller: it
    /// needs the same raw/resolved owner data
    /// [`Self::plan_def_override`] builds internally, to run
    /// [`rim_merge::diff::structural_change`] over it.
    pub(crate) fn reader(&self) -> &Reader {
        &self.reader
    }

    /// [`Self::execute_in`] against the profile's own context: the
    /// session's currently selected order, and `key`'s stored choices out
    /// of [`Session::decisions`]. Every profile-level caller goes
    /// through here.
    ///
    /// # Errors
    ///
    /// See [`PlanMergeError`].
    pub fn execute<'a>(
        &self,
        session: &'a mut Session,
        key: &FindingKey,
    ) -> Result<&'a MergePreview, PlanMergeError> {
        let slot = PreviewSlot::profile(session.selected());
        // A profile slot (`slot.patch: None`) always resolves —
        // `Session::merge_context` only ever fails for a patch slot
        // naming an id this session doesn't have loaded.
        let ctx = match session.merge_context(slot, key) {
            Ok(ctx) => ctx,
            Err(never) => unreachable!("a profile slot's context is always Ok: {never}"),
        };
        self.execute_in(session, ctx, key)
    }

    /// Computes `key`'s preview against `ctx` and caches it under
    /// `ctx.slot` — unless a cached preview already sits there built from
    /// this exact `(ctx.choices, ctx.scope)`, in which case that one is
    /// reused as-is with no rebuild (a
    /// per-`(slot, key, choices)` cache: everything else a preview
    /// depends on — the selected order, which mods are active, a def's
    /// source text — already invalidates this whole cache wholesale on
    /// change, per `Session::invalidate_ledgers`'s own doc comment, so
    /// comparing just these two fields is sufficient). A caller that
    /// genuinely needs the latest state after choices changed elsewhere
    /// (e.g. right after storing a new decision) must build a fresh `ctx`
    /// reflecting that change first — [`Session::merge_context`] always
    /// reads the current stored choices, so it never hands back a stale
    /// `ctx` on its own.
    ///
    /// # Errors
    ///
    /// See [`PlanMergeError`]. A finding whose xpath falls outside the
    /// supported xpath grammar is *not* an error here — it comes back as an
    /// `Ok` preview with [`MergeState::CannotMerge`](rim_resolve::domain::MergeState::CannotMerge).
    pub fn execute_in<'a>(
        &self,
        session: &'a mut Session,
        ctx: MergeContext,
        key: &FindingKey,
    ) -> Result<&'a MergePreview, PlanMergeError> {
        let slot = ctx.slot.clone();
        if !session.merge_preview_matches(&ctx, key) {
            let preview = self.build_preview(session, &ctx, key)?;
            session.cache_merge_preview(ctx, preview);
        }
        match session.merge_preview(&slot, key) {
            Some(preview) => Ok(preview),
            None => unreachable!("just verified fresh, or just cached, above"),
        }
    }

    fn build_preview(
        &self,
        session: &Session,
        ctx: &MergeContext,
        key: &FindingKey,
    ) -> Result<MergePreview, PlanMergeError> {
        let order = session.orders().get(ctx.slot.source).clone();
        let choices = &ctx.choices;

        match key {
            FindingKey::DefOverride {
                key: def_key,
                owners,
            } => self.plan_def_override(
                session,
                &order,
                key,
                def_key,
                owners,
                choices,
                ctx.scope.as_ref(),
            ),
            FindingKey::PatchCollision {
                key: def_key,
                selector,
                sub_path,
                mods,
            } => self.plan_patch_collision(
                session,
                &order,
                key,
                def_key,
                *selector,
                sub_path.as_deref(),
                mods,
                choices,
                ctx.scope.as_ref(),
            ),
            other => Err(PlanMergeError::UnsupportedFinding(other.clone())),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "grouping these into a struct would just move the same eight inputs one level down; every field is used directly, not threaded further"
    )]
    fn plan_def_override(
        &self,
        session: &Session,
        order: &LoadOrder,
        finding_key: &FindingKey,
        def_key: &DefKey,
        owners: &BTreeSet<ModId>,
        choices: &BTreeMap<FieldPath, MergeChoice>,
        scope: Option<&PatchScope>,
    ) -> Result<MergePreview, PlanMergeError> {
        let ordered_owners = ordered(owners, order);
        let Some(fallback_base) = ordered_owners.first().cloned() else {
            return Err(PlanMergeError::MissingSource(format!(
                "{def_key}: no active owner in the selected order"
            )));
        };

        // The scoped participant rule: a patch's diff is built
        // only from owners ∩ (scope ∪ {Core}), never every owner the
        // finding names. `scope: None` (the profile, or a preview with no
        // patch context) keeps every owner.
        let (participants, out_of_scope_owners) = match scope {
            Some(scope) => partition_by_scope(&ordered_owners, scope),
            None => (ordered_owners.clone(), Vec::new()),
        };
        if scope.is_some() && participants.len() < 2 {
            let reason = match participants.first() {
                Some(representative) => format!(
                    "only {representative} of this patch's scope owns the def in the selected order"
                ),
                None => {
                    "no member of this patch's scope owns the def in the selected order".to_string()
                }
            };
            let representative = participants.first().cloned().unwrap_or(fallback_base);
            return Ok(cannot_merge_preview(
                finding_key,
                def_key,
                Selector::DefName,
                representative,
                reason,
            ));
        }

        let base_id = participants
            .first()
            .cloned()
            .unwrap_or_else(|| fallback_base.clone());
        let winner_id = participants
            .last()
            .cloned()
            .unwrap_or_else(|| fallback_base.clone());

        let owner_versions = build_owner_versions(
            &self.reader,
            session,
            order,
            &def_key.def_type,
            &def_key.def_name,
            &participants,
        )?;

        let diff = rim_merge::diff::three_way(&owner_versions, &base_id).unwrap_or_else(|error| {
            unreachable!("base_id is always one of owner_versions: {error}")
        });
        let winner_version = owner_versions
            .iter()
            .find(|owner| owner.mod_id == winner_id)
            .unwrap_or_else(|| unreachable!("winner_id is always one of participants"));
        let mut merge_plan = plan::plan_def_override(&diff, winner_version, choices);
        if !out_of_scope_owners.is_empty() {
            merge_plan.caveats.push(Caveat::OutOfScopeOwners {
                mods: out_of_scope_owners,
            });
        }
        // The structural guard is evaluated over
        // the exact `owner_versions`/`diff` this preview was just built
        // from, so `diff.base` is always one of `owner_versions` by
        // construction — the same precondition `three_way` above already
        // relies on, hence the identical `unreachable!` shape.
        let structural_change = rim_merge::diff::structural_change(&diff, &owner_versions)
            .unwrap_or_else(|error| {
                unreachable!("base_id is always one of owner_versions: {error}")
            });
        let state = state_from_plan(&merge_plan, diff.fields.len(), structural_change.as_ref());
        // The merge's own
        // resolved value per field — computed against `choices` (the
        // exact map `merge_plan` itself was just folded from), never
        // `merge_plan.ops` alone, since a field whose resolved value
        // already matches what's on disk gets no op but still has a real
        // "final" value to show.
        let final_values = plan::resolved_field_values(&diff, winner_version, choices);

        Ok(MergePreview {
            key: finding_key.clone(),
            owners: participants,
            base: base_id,
            winner: winner_id,
            diff,
            plan: merge_plan,
            state,
            structural_change,
            final_values,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "each field is one distinct piece of a FindingKey::PatchCollision plus the choices map and the optional patch scope; grouping them into a struct would only rename this same list one level down"
    )]
    fn plan_patch_collision(
        &self,
        session: &Session,
        order: &LoadOrder,
        finding_key: &FindingKey,
        def_key: &DefKey,
        selector: Selector,
        sub_path_text: Option<&str>,
        mods: &BTreeSet<ModId>,
        choices: &BTreeMap<FieldPath, MergeChoice>,
        scope: Option<&PatchScope>,
    ) -> Result<MergePreview, PlanMergeError> {
        let (def_owner, target_raw) = def_sources::def_owner_and_raw(
            &self.reader,
            session,
            order,
            &def_key.def_type,
            &def_key.def_name,
            selector,
        )?;

        let sub_path = match field_path_from_sub_path(sub_path_text) {
            Ok(path) => path,
            Err(reason) => {
                return Ok(cannot_merge_preview(
                    finding_key,
                    def_key,
                    selector,
                    def_owner,
                    reason,
                ));
            }
        };

        let indexed = session
            .sources()
            .patch_ops_by_def
            .get(&(def_key.def_type.clone(), def_key.def_name.clone(), selector))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let top_level = def_sources::top_level_operations(indexed, order);
        // A patch collision restricts *contributions*, not
        // the target — the def's raw node still comes from
        // `def_owner_and_raw` above regardless of scope, but with a scope
        // only its own members' top-level operations are replayed.
        // Filtered *before* reading anything: a scoped preview must never
        // fail because an out-of-scope mod's own patch file happens to be
        // stale or unreadable — that mod's ops are never going to be
        // replayed here anyway.
        let scoped_top_level: Vec<&(ModId, XmlLocator)> = top_level
            .iter()
            .filter(|(mod_id, _)| scope.is_none_or(|scope| scope.contains(mod_id)))
            .collect();
        let op_texts = def_sources::load_operation_texts(&self.reader, scoped_top_level)?;
        let contributions: Vec<PatchContribution<'_>> = op_texts
            .iter()
            .map(|(id, text)| PatchContribution {
                mod_id: id,
                operation_xml: text.as_str(),
            })
            .collect();

        let active_mods = session.active_base_ids();
        let mod_names_by_display: BTreeMap<String, ModId> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.name.clone(), m.id.clone()))
            .collect();
        // Answers a `PatchOperationConditional`/`Test` whose xpath is a
        // bare existence test on another def, out of the scan's own def
        // index — see `def_sources::LazyDefExists`'s own doc comment.
        let def_index = def_sources::LazyDefExists::new(session);
        let def_exists = |def_type: &str, def_name: &str| def_index.get(def_type, def_name);

        let all_mods_in_order = ordered(mods, order);
        let (mods_in_order, out_of_scope_mods) = match scope {
            Some(scope) => {
                let inside: Vec<ModId> = all_mods_in_order
                    .iter()
                    .filter(|id| scope.contains(id))
                    .cloned()
                    .collect();
                let outside: Vec<ModId> = all_mods_in_order
                    .iter()
                    .filter(|id| !scope.contains(id))
                    .cloned()
                    .collect();
                (inside, outside)
            }
            None => (all_mods_in_order, Vec::new()),
        };
        if scope.is_some() && mods_in_order.len() < 2 {
            let reason = match mods_in_order.first() {
                Some(representative) => format!("only {representative} of this patch's scope has a contribution to this collision in the selected order"
                ),
                None => "no member of this patch's scope has a contribution to this collision in the selected order"
                    .to_string(),
            };
            let representative = mods_in_order.first().cloned().unwrap_or(def_owner);
            return Ok(cannot_merge_preview(
                finding_key,
                def_key,
                selector,
                representative,
                reason,
            ));
        }

        let plan_result = plan::plan_patch_collision(PatchCollisionInput {
            key: def_key.clone(),
            selector,
            sub_path: sub_path.clone(),
            def_owner: def_owner.clone(),
            target_raw: &target_raw,
            mods: &mods_in_order,
            contributions: &contributions,
            active_mods: &active_mods,
            mod_names_by_display: &mod_names_by_display,
            def_exists: &def_exists,
            choices,
            behaviours: session.mod_knowledge().patch_operations(),
        });
        // The engine returns the very `FieldDiff`s it planned from: the
        // per-mod candidate trees behind them — an isolated single-mod
        // replay per member for a confirmed keyed map, a
        // `move_mod_last`-reordered full replay otherwise, plus the
        // clobber/identity corrections on top — are built once, inside
        // `plan_patch_collision`, and never rebuilt here, so the display
        // path cannot disagree with the plan.
        let (mut merge_plan, fields, final_values) = match plan_result {
            Ok(outcome) => (outcome.plan, outcome.fields, outcome.final_values),
            Err(error) => {
                return Ok(cannot_merge_preview(
                    finding_key,
                    def_key,
                    selector,
                    def_owner,
                    error.to_string(),
                ));
            }
        };
        if !out_of_scope_mods.is_empty() {
            merge_plan.caveats.push(Caveat::OutOfScopeOwners {
                mods: out_of_scope_mods,
            });
        }

        let diff = ThreeWayDiff {
            base: def_owner.clone(),
            fields,
        };
        // The structural guard is a `DefOverride`-only guard —
        // "the load-order winner's own class hierarchy may not carry
        // whatever another owner tried to add" describes one mod's own
        // *def* shadowing another's, which has no counterpart for a
        // patch collision (every contributor's own `PatchOperation`
        // targets the same, single def; there's no second "owner's
        // class" in play at all).
        let state = state_from_plan(&merge_plan, diff.fields.len(), None);
        let base = mods_in_order
            .first()
            .cloned()
            .unwrap_or_else(|| def_owner.clone());
        let winner = mods_in_order
            .last()
            .cloned()
            .unwrap_or_else(|| def_owner.clone());

        Ok(MergePreview {
            key: finding_key.clone(),
            owners: mods_in_order,
            base,
            winner,
            diff,
            plan: merge_plan,
            state,
            structural_change: None,
            final_values,
        })
    }
}
