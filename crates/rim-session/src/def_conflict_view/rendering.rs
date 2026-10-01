//! Assembling the view: touchers, problems, injected-node relations, and the `build*` entry points.

use rim_analyzer::domain::{EdgeKind, ModId, Report};
use rim_merge::effective::{Completeness, EffectiveDef, Stopper};
use rim_merge::inherit::InheritError;
use rim_resolve::domain::{DefRef, FindingKey, PatchProject};

use super::rows::{build_def_override_fields, build_patch_collision_fields, parse_sub_path};
use super::{
    DefConflictKind, DefConflictView, DefConflictViewError, InjectedNodeRelation, Problem, Toucher,
    ToucherRole, problem_from_plan_failure,
};
use crate::Session;
use crate::merge_workspace::PreviewSlot;
use crate::use_cases::{DefInspection, PatchOpSummary, Patcher};

fn build_touchers(inspection: &DefInspection) -> Vec<Toucher> {
    let mut touchers: Vec<Toucher> = inspection
        .owners
        .iter()
        .map(|owner| Toucher {
            mod_id: owner.mod_id.clone(),
            position: owner.position,
            is_generated: owner.is_generated,
            role: ToucherRole::Owner,
            op_count: 0,
        })
        .chain(inspection.patchers.iter().map(|patcher| Toucher {
            mod_id: patcher.mod_id.clone(),
            position: patcher.position,
            is_generated: patcher.is_generated,
            role: ToucherRole::Patcher,
            op_count: patcher.ops.len(),
        }))
        .collect();
    touchers.sort_by(|a, b| {
        a.position
            .cmp(&b.position)
            .then_with(|| role_rank(a.role).cmp(&role_rank(b.role)))
    });
    touchers
}

fn role_rank(role: ToucherRole) -> u8 {
    match role {
        ToucherRole::Owner => 0,
        ToucherRole::Patcher => 1,
    }
}

/// The [`PatchOpSummary`] behind one fold [`Stopper::Replay`]: `patchers`
/// are in load order with each mod's own ops contiguous and in the same
/// relative order the fold's own `contributions` used (see
/// `crate::use_cases::inspect_def`'s own construction) — so summing op
/// counts of every patcher before `stopper_mod` gives that mod's own
/// first global index, and `op_index` minus that is its local index into
/// `stopper_mod`'s own `ops`.
fn representative_stopper_op<'a>(
    patchers: &'a [Patcher],
    stopper_mod: &ModId,
    op_index: usize,
) -> Option<&'a PatchOpSummary> {
    let mut global_start = 0usize;
    for patcher in patchers {
        if &patcher.mod_id == stopper_mod {
            // `checked_sub`, not a bare subtraction: `op_index` is a
            // global index across every active patcher's ops, so a caller
            // handed a `stopper_mod`/`op_index` pair the fold itself never
            // produced (a future refactor mismatching the two) must not
            // panic on underflow — it should just find nothing here,
            // same as any other index this local lookup can't resolve.
            return op_index
                .checked_sub(global_start)
                .and_then(|local_index| patcher.ops.get(local_index));
        }
        global_start += patcher.ops.len();
    }
    None
}

/// [`DefConflictView::problems`] from the effective def's own
/// [`Completeness`] — the only source (`Stopper`/`Unsupported` ops,
/// missing templates, cycles).
fn problems_from_completeness(effective: &EffectiveDef, patchers: &[Patcher]) -> Vec<Problem> {
    match &effective.completeness {
        Completeness::Complete => Vec::new(),
        Completeness::Partial {
            stopped_at:
                Stopper::Replay {
                    mod_id,
                    op_index,
                    error,
                },
        } => {
            let (class, xpath) = representative_stopper_op(patchers, mod_id, *op_index)
                .map(|op| (op.class.clone(), op.xpath.clone()))
                .unwrap_or_else(|| ("<unknown>".to_string(), None));
            vec![Problem::UnsupportedOp {
                mod_id: mod_id.clone(),
                op_index: *op_index,
                class,
                xpath,
                reason: error.to_string(),
            }]
        }
        Completeness::Partial {
            stopped_at: Stopper::Inherit(InheritError::MissingParent { def_type, name }),
        } => vec![Problem::MissingTemplate {
            def_type: def_type.clone(),
            name: name.clone(),
        }],
        Completeness::Partial {
            stopped_at: Stopper::Inherit(InheritError::Cycle { chain }),
        } => vec![Problem::Cycle {
            chain: chain.clone(),
        }],
    }
}

/// [`DefConflictView::injected_node_relations`]'s own builder: every
/// `EdgeKind::PatchSelectsInjectedNode` edge in `report.edges` whose own
/// `subject` names `def_ref` or a path beneath it. `Edge.subject` for
/// this kind is `DefTarget::display_path()`
/// (`"{def_type}/{def_name}[/{sub_path}]"`, `rim_analyzer::domain::patch`'s
/// own doc comment) — **not** selector-aware (a `[@Name="X"]` template
/// and a `[defName="X"]` def sharing one literal name render identically
/// there), so this matches on `def_ref.key` alone, ignoring
/// `def_ref.selector`, the same "display text, not an exact match key"
/// choice the edge's own producer already made. A [`DefRef::name_only`]
/// ref (`key.def_type` empty, `DuplicateTemplateName`'s own case) can
/// never match anything here — no real `subject` is ever just a bare
/// name with no def type — which is correct: that finding kind has no
/// merge preview either, and there is no sub-path selection to report
/// evidence about. Deterministically ordered
/// (`(selector, injector, subject)`, all three already `Ord`) so the view
/// stays byte-identical across runs, the same contract every other
/// `DefConflictView` field has.
fn injected_node_relations(report: &Report, def_ref: &DefRef) -> Vec<InjectedNodeRelation> {
    let prefix = format!("{}/{}", def_ref.key.def_type, def_ref.key.def_name);
    let nested_prefix = format!("{prefix}/");
    let mut relations: Vec<InjectedNodeRelation> = report
        .edges
        .iter()
        .filter(|edge_report| edge_report.edge.kind == EdgeKind::PatchSelectsInjectedNode)
        .filter_map(|edge_report| {
            let subject = edge_report.edge.subject.as_deref()?;
            if subject != prefix && !subject.starts_with(&nested_prefix) {
                return None;
            }
            Some(InjectedNodeRelation {
                selector: edge_report.edge.after.clone(),
                injector: edge_report.edge.before.clone(),
                subject: subject.to_string(),
            })
        })
        .collect();
    relations.sort_by(|a, b| {
        a.selector
            .cmp(&b.selector)
            .then_with(|| a.injector.cmp(&b.injector))
            .then_with(|| a.subject.cmp(&b.subject))
    });
    relations
}

/// [`Session::def_conflict_view`]'s own implementation — `pub(crate)` so
/// only that one-line accessor calls it (mirrors `crate::changes`' own
/// query/method split). Delegates to [`build_for_slot`] under the
/// profile's own slot; kept as its own function (rather than a call-site
/// `PreviewSlot::profile(session.selected())` at every caller) so this
/// module's own test suite — which calls `session.def_conflict_view`
/// through `Session`, never `build`/`build_for_slot` directly — is
/// unaffected by the patch-scoped sibling below.
pub(crate) fn build(
    session: &Session,
    key: &FindingKey,
) -> Result<DefConflictView, DefConflictViewError> {
    build_for_slot(session, key, PreviewSlot::profile(session.selected()))
}

/// [`build`]'s general form, and [`Session::def_conflict_view_for_patch`]'s
/// own implementation: joins the same
/// cached [`DefInspection`] — always looked up under `slot.source`, and
/// always the *unscoped* one, since a compat patch only restricts a
/// merge's own participants,
/// never which mods own or patch a def in the first place — with the
/// [`MergePreview`] cached under `slot` specifically, rather than always
/// [`PreviewSlot::profile`]. `&Session`, not `&mut`: every source this
/// joins (`Session::inspection`/
/// `merge_preview`/`decisions`) is already a `&self` reader, so nothing
/// here ever needed exclusive access — a caller holding only a shared
/// borrow (or a `&mut Session` reborrowed as one, which every existing
/// `&mut`-holding caller already does automatically) can call this.
pub(crate) fn build_for_slot(
    session: &Session,
    key: &FindingKey,
    slot: PreviewSlot,
) -> Result<DefConflictView, DefConflictViewError> {
    let Some(def_ref) = key.def_ref() else {
        return Err(DefConflictViewError::UnsupportedFinding(key.clone()));
    };
    let inspection = session
        .inspection(slot.source, &def_ref)
        .cloned()
        .ok_or_else(|| DefConflictViewError::NotInspected(def_ref.clone()))?;
    let touchers = build_touchers(&inspection);
    // The `Preference::Decision` lookup (`preference_for`, below) must
    // check *this* slot's own decision set — a patch's own `PreferWinner`
    // decision, when `slot.patch` names one, never the profile's. Falls
    // back to the profile's own only if the patch has since vanished from
    // under an already-built slot (defensive, not expected in practice:
    // `Session::def_conflict_view_for_patch` builds the slot unconditionally,
    // same as `Session::merge_context` does — neither checks the patch
    // exists itself — so this only ever matters for a caller that skips
    // the real entry point's own precondition; `get_def_conflict_view`
    // never does, since it already runs `session.patch_resolution`
    // first).
    let decisions = slot
        .patch
        .as_ref()
        .and_then(|id| session.patch(id))
        .map(PatchProject::decisions)
        .unwrap_or_else(|| session.decisions());

    let (kind, fields) = match key {
        FindingKey::DefOverride { .. } => {
            // Never `NotPlanned` for a
            // `DefOverride` — unlike `PatchCollision` (whose own
            // `PlanMerge::execute` never hard-errors; a stopper always
            // becomes `MergeState::CannotMerge` instead), a `DefOverride`
            // whose owner has a broken `ParentName` chain makes
            // `plan_def_override` itself return `Err`, which caches
            // nothing at all — so a missing preview here means "planning
            // couldn't even be attempted," not "the caller forgot to
            // plan," and `build_def_override_fields` already degrades
            // gracefully for exactly that (see its own doc comment).
            let preview = session.merge_preview(&slot, key).cloned();
            let fields = build_def_override_fields(key, preview.as_ref(), &inspection, decisions);
            (DefConflictKind::DefOverride, fields)
        }
        FindingKey::PatchCollision {
            key: def_key,
            sub_path,
            mods,
            ..
        } => {
            let preview = session
                .merge_preview(&slot, key)
                .cloned()
                .ok_or_else(|| DefConflictViewError::NotPlanned(key.clone()))?;
            let parsed_sub_path = sub_path
                .as_deref()
                .and_then(|text| parse_sub_path(text).ok());
            let fields = build_patch_collision_fields(
                key,
                &preview,
                &inspection,
                def_key,
                parsed_sub_path.as_ref(),
                mods,
                decisions,
            );
            (
                DefConflictKind::PatchCollision {
                    sub_path: parsed_sub_path,
                },
                fields,
            )
        }
        FindingKey::DuplicateTemplateName { .. } => {
            (DefConflictKind::DuplicateTemplateName, Vec::new())
        }
        // `key.def_ref()` returning `Some` above means `key` *should* be
        // one of the three arms above per `FindingKey::def_ref`'s own
        // exhaustive match — but relying on that from here would mean a
        // future def-ref-bearing variant this module hasn't been taught
        // about panics instead of erroring: matching on `key` here rather
        // than `unreachable!`-ing across a module boundary neither type
        // proves to the compiler.
        _ => return Err(DefConflictViewError::UnsupportedFinding(key.clone())),
    };

    let problems = problems_from_completeness(&inspection.effective, &inspection.patchers);
    let injected_node_relations = injected_node_relations(session.report(), &def_ref);

    Ok(DefConflictView {
        def_ref,
        kind,
        touchers,
        fields,
        problems,
        effective_completeness: inspection.effective.completeness.clone(),
        injected_node_relations,
    })
}

/// [`build_for_slot`], but for a `DefOverride` whose own
/// `PlanMerge::execute`/`execute_in` call the caller already ran and
/// swallowed (`MissingSource`/a structured `Inherit` gap — see
/// [`Problem::PlanFailed`]'s own doc comment for why this exists):
/// appends [`problem_from_plan_failure`]'s own `Problem` when
/// `plan_failure` is `Some`, so a losing owner's own broken chain is
/// never invisible just because the *winning* owner's own chain (all of
/// [`InspectDef`]'s own `Problem`s are derived from) resolved cleanly.
/// `None` behaves identically to a plain [`build_for_slot`] call — this
/// is the one function [`Session::def_conflict_view_with_plan_failure`]/
/// [`Session::def_conflict_view_for_patch_with_plan_failure`] both share,
/// the unscoped/scoped split staying exactly [`build`]/[`build_for_slot`]'s
/// own.
///
/// [`InspectDef`]: crate::use_cases::InspectDef
pub(crate) fn build_with_plan_failure(
    session: &Session,
    key: &FindingKey,
    slot: PreviewSlot,
    plan_failure: Option<&crate::use_cases::PlanMergeError>,
) -> Result<DefConflictView, DefConflictViewError> {
    let mut view = build_for_slot(session, key, slot)?;
    if let Some(error) = plan_failure {
        view.problems.push(problem_from_plan_failure(error));
    }
    Ok(view)
}
