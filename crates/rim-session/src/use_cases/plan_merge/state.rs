//! Stored per-field choices, the merge state a plan implies, and the cannot-merge preview.

use std::collections::BTreeMap;

use rim_analyzer::domain::{ModId, Selector};
use rim_merge::diff::{StructuralChange, ThreeWayDiff};
use rim_merge::plan::MergePlan;
use rim_merge::tree::FieldPath;
use rim_resolve::domain::{Action, DefKey, FindingKey, MergeChoice, MergeState};

use crate::merge_workspace::MergePreview;

/// `key`'s stored per-field choices under `decisions` — the profile's own
/// [`Session::decisions`](crate::Session::decisions), or a patch project's, whichever `decisions`
/// names. `pub(crate)` so [`render_merge_mod`](crate::use_cases::render_merge_mod) and
/// [`crate::Session::merge_context`] can build the same
/// [`MergeContext::choices`](crate::use_cases::plan_merge::context::MergeContext::choices) this module's own [`PlanMerge::execute`](crate::use_cases::plan_merge::PlanMerge::execute)
/// does, for every candidate key/context they build.
pub(crate) fn stored_choices(
    decisions: &rim_resolve::domain::DecisionSet,
    key: &FindingKey,
) -> BTreeMap<FieldPath, MergeChoice> {
    match decisions.get(key).map(|d| &d.action) {
        Some(Action::Merge { choices, .. }) => choices.clone(),
        _ => BTreeMap::new(),
    }
}

/// Folds `plan`'s own per-field resolution into the preview's overall
/// [`MergeState`] — and, for a `DefOverride`, the structural guard on top
/// (`structural_change` the caller already
/// evaluated via `rim_merge::diff::structural_change`; always `None` for a
/// `PatchCollision`, which the guard never applies to).
///
/// **Why the guard overrides the variant here, rather than reclassifying
/// `diff.fields`**: "every field of that def is
/// reclassified `Conflict`" can't be implemented as a per-`FieldDiff`
/// rewrite without lying twice over — a `ParentName` difference is never
/// a `FieldDiff` at all ([`FieldTree::parent_name`][ft], stripped before
/// `ThreeWayDiff::fields` is built), so a def whose owners differ *only*
/// in `ParentName` has nothing to reclassify; and even for a field that
/// does exist, `DiffClass::Conflict { by }` needs a non-empty "these
/// owners disagree" set — synthesizing an empty one to force a
/// reclassification would misrepresent agreement as conflict. So the
/// guard is instead a property of the *preview*, checked once here,
/// after the ordinary field-by-field fold: `structural_change.is_some()`
/// forces [`MergeState::NeedsFieldInput`] outright, regardless of
/// `plan.unresolved` — a fully-chosen merge on a structurally-guarded def
/// still can't safely auto-apply (the load-order winner's own class
/// hierarchy may not carry whatever another owner tried to add; no
/// per-field choice fixes that), so [`RenderMergeMod`](crate::use_cases::render_merge_mod::RenderMergeMod)
/// (which only ever renders a `Complete` preview) correctly never writes
/// one, no matter what the user chose per field.
///
/// **`unresolved`/`total` are both `total_fields` in the guarded case**,
/// not `plan.unresolved.len()`: `plan.unresolved` only counts genuine
/// per-field conflicts, which the guard's own trigger fields (`ParentName`
/// especially) are never one of — reporting the real, possibly-zero
/// `plan.unresolved.len()` here would read as "0 fields need a choice"
/// while the state itself says `NeedsFieldInput`, a self-contradictory
/// pair that would also feed `redecide_for_clean_merge`'s rationale text
/// ("Merge, 0 fields need a choice."). `total_fields` for both numbers
/// says instead "every field needs re-confirming" — the honest reading of
/// "every field... reclassified" once it's applied
/// at the state level instead of the per-field one. This is a distinct
/// number from [`MergePreview::field_page`]'s own per-field totals, which
/// still report the real, individual `DiffClass`es unchanged — the
/// editor's field list is never touched by this, only the top-level
/// state pill and the ledger's rationale text.
///
/// [ft]: rim_merge::tree::FieldTree
pub(super) fn state_from_plan(
    plan: &MergePlan,
    total_fields: usize,
    structural_change: Option<&StructuralChange>,
) -> MergeState {
    if structural_change.is_some() {
        return MergeState::NeedsFieldInput {
            unresolved: total_fields,
            total: total_fields,
        };
    }
    if plan.unresolved.is_empty() {
        MergeState::Complete {
            op_count: plan.ops.len(),
        }
    } else {
        MergeState::NeedsFieldInput {
            unresolved: plan.unresolved.len(),
            total: total_fields,
        }
    }
}

pub(super) fn cannot_merge_preview(
    finding_key: &FindingKey,
    def_key: &DefKey,
    selector: Selector,
    representative: ModId,
    reason: String,
) -> MergePreview {
    MergePreview {
        key: finding_key.clone(),
        owners: vec![representative.clone()],
        base: representative.clone(),
        winner: representative.clone(),
        diff: ThreeWayDiff {
            base: representative.clone(),
            fields: Vec::new(),
        },
        plan: MergePlan {
            key: def_key.clone(),
            selector,
            winner: representative.clone(),
            owners: vec![representative],
            ops: Vec::new(),
            unresolved: Vec::new(),
            caveats: Vec::new(),
        },
        state: MergeState::CannotMerge { reason },
        // No real `ThreeWayDiff`/owner data was ever built for this
        // preview (that's what `CannotMerge` here means) — nothing for
        // the structural guard to evaluate.
        structural_change: None,
        // Nor anything for the "final" value to read a field from.
        final_values: BTreeMap::new(),
    }
}
