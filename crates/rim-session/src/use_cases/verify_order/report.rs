//! The verify report and the reorder-conflict classification built from its evidence.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{Conflict, EdgeKind, LoadOrder, ModId, PatchOp, Selector};
use rim_merge::tree::FieldTree;
use rim_resolve::domain::{DefKey, Finding, FindingKey, PatchFailureCause, ReorderKind};

use super::counterfactual::CounterfactualStats;
use crate::Session;
use crate::use_cases::def_sources;

/// [`VerifyOrder::execute`](super::VerifyOrder::execute)'s result: every predicted patch failure this
/// pass found, plus how many defs it actually examined (candidates with
/// at least one *active* patcher, whether or not any of their ops
/// turned out to fail — a gated-off patcher never makes a def a
/// candidate at all, see this module's own doc comment) and which
/// candidates it had to skip outright because a source read/parse failed
/// partway through (a stale scan, chiefly) — the same "never fail the
/// whole pass over one bad def" tolerance
/// `Session::redecide_identical_copies_at` already applies.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyOrderReport {
    /// Which order this was checked against.
    pub source: rim_resolve::domain::OrderSource,
    /// Every predicted failure, one [`Finding::PatchWillFail`] per failed
    /// operation.
    pub findings: Vec<Finding>,
    /// How many defs this pass actually attempted to check (the
    /// zero-owner fast path and the replayed path both count; a def with
    /// no *active* patcher — never a candidate at all, per this pass's
    /// own scope — does not).
    pub defs_checked: usize,
    /// A candidate def this pass could not check at all, with why —
    /// distinct from a def it checked and found nothing wrong with.
    pub skipped: Vec<(DefKey, Selector, String)>,
    /// What the counterfactual phase did — all zeroes when it was turned off
    /// ([`VerifyOptions::counterfactual`](super::VerifyOptions::counterfactual)) or when
    /// no surviving row qualified.
    pub counterfactual: CounterfactualStats,
    /// How many operations the replay deliberately declined to predict a
    /// failure for, summed over every def this pass replayed: their xpath
    /// uses one of the supported
    /// **filter heads** (`[@ParentName="X"]`, a bare-type
    /// `Defs/ThingDef/…`, an `and`-composed content filter), which is a
    /// global query whose empty selection *on one def* is no evidence at
    /// all about the operation's real outcome.
    ///
    /// Reported rather than acted on: it is the number that decides
    /// whether that conservatism is ever lifted. It is a
    /// count of *operations replayed*, not of distinct operations — one
    /// op examined under several def keys counts once per def.
    pub suppressed_filter_head_ops: usize,
}

/// Removed-node/injected-node edge evidence, indexed once per [`VerifyOrder::execute`] call rather
/// than per def: every `PatchRemovedNode`/`PatchInjectedNode` edge
/// `session.report()` already carries, as plain `(after, before)` pairs —
/// `Edge`'s own convention (`after` must load after `before`) applies
/// unchanged here. `Report`'s own `EdgeReport::status` is evaluated
/// against whichever order the *scan* ran under (effectively `Current`),
/// never against `source` — this pass compares each pair's own load
/// positions in `order` itself instead, so it's correct under either
/// `OrderSource`.
///
/// **Key-representability, checked**: `Edge::after`/`before` come from
/// the same raw-scanned-id space `LoadOrder`/`SourceIndex` do
/// (`ModsConfig.xml`'s own ids, `_steam` suffixes included) — the same
/// raw-id-space fact this crate's own `CLAUDE.md` already confirms for
/// `SourceIndex.defs`/`ops_by_mod`.
///
/// **Known imprecision, disclosed**: a `PatchRemovedNode`/`PatchInjectedNode`
/// edge is deduplicated per mod *pair* at scan time
/// (`push_edge_once`, keyed on `(after, before, kind)` alone —
/// `crates/rim-analyzer/src/analysis/edges.rs`), so one edge can stand in
/// for evidence about *any* def those two mods share, not only the one
/// currently being checked. [`classify_cause`] narrows this by also
/// requiring the named mod to be a genuine patcher of *this* def
/// (`other_patchers`), which rules out the case where the edge's real
/// evidence is about a completely different def neither mod touches
/// here — but it cannot rule out the case where both mods touch *this*
/// def **and** some other def, and the edge's own evidence was really
/// about the other one. Disclosed rather than fixed blind.
pub(super) struct EdgeEvidence {
    pub(super) removed_node: BTreeSet<(ModId, ModId)>,
    pub(super) injected_node: BTreeSet<(ModId, ModId)>,
}

impl EdgeEvidence {
    pub(super) fn build(session: &Session) -> Self {
        let mut removed_node = BTreeSet::new();
        let mut injected_node = BTreeSet::new();
        for edge_report in &session.report().edges {
            let edge = &edge_report.edge;
            match edge.kind {
                EdgeKind::PatchRemovedNode => {
                    removed_node.insert((edge.after.clone(), edge.before.clone()));
                }
                EdgeKind::PatchInjectedNode => {
                    injected_node.insert((edge.after.clone(), edge.before.clone()));
                }
                _ => {}
            }
        }
        Self {
            removed_node,
            injected_node,
        }
    }
}

/// Classifies one `Caveat::FailedOp`'s own cause — see
/// [`PatchFailureCause`]'s own doc comment for the four outcomes.
pub(super) fn classify_cause(
    mod_id: &ModId,
    xpath: &str,
    indexed: &[rim_analyzer::analysis::IndexedPatchOp],
    other_patchers: &BTreeSet<ModId>,
    order: &LoadOrder,
    edges: &EdgeEvidence,
    resolved: &FieldTree,
) -> PatchFailureCause {
    if let Some(mod_position) = order.position(mod_id) {
        // `RemovedBy`: some other patcher of this same def has a removed-node edge
        // naming it the remover against `mod_id`, and it actually loaded
        // before `mod_id` under `order` — violating the edge's own
        // requirement (remover after toucher), which is exactly why the
        // op failed.
        let removed_by = other_patchers.iter().find(|other| {
            *other != mod_id
                && edges
                    .removed_node
                    .contains(&((*other).clone(), mod_id.clone()))
                && order
                    .position(other)
                    .is_some_and(|other_position| other_position < mod_position)
        });
        if let Some(remover) = removed_by {
            return PatchFailureCause::RemovedBy(remover.clone());
        }

        // `NotYetInjected`: some other patcher of this same def has an injected-node
        // edge naming it the injector `mod_id` needs, and it actually
        // loads after `mod_id` under `order` — violating the edge's own
        // requirement (selector after injector).
        let not_yet_injected = other_patchers.iter().find(|other| {
            *other != mod_id
                && edges
                    .injected_node
                    .contains(&(mod_id.clone(), (*other).clone()))
                && order
                    .position(other)
                    .is_some_and(|other_position| mod_position < other_position)
        });
        if let Some(injector) = not_yet_injected {
            return PatchFailureCause::NotYetInjected(injector.clone());
        }
    }

    // `DeadTarget`: the failed op's own target path (recovered from the
    // matching `IndexedPatchOp`'s already-parsed `DefTarget`, never
    // re-parsed from `xpath` here) is absent from the winner's own final,
    // fully-resolved tree — the node genuinely never existed for this
    // def, under any order, or was unconditionally stripped by something
    // removed-node/injected-node edges can't structurally see (an
    // attribute-predicate remover, chiefly). A root-level op (no sub_path) or
    // a sub_path outside the replayable grammar can't be safely checked
    // this way and falls through to `Unknown` instead of guessing.
    let target_sub_path = op_sub_path(mod_id, xpath, indexed);
    if let Some(sub_path) = &target_sub_path
        && let Ok(Some(field_path)) = def_sources::field_path_from_sub_path(Some(sub_path))
        && resolved.get(&field_path).is_none()
    {
        return PatchFailureCause::DeadTarget;
    }

    PatchFailureCause::Unknown
}

/// The normalized sub_path of the one indexed op whose own xpath text
/// (as `Caveat::FailedOp::xpath` recorded it) matches `xpath`, for
/// `mod_id` — the same value `Conflict::PatchCollision::sub_path`/
/// `FindingKey::PatchCollision::sub_path` carry for the same target, so a
/// caller can look one up by it. Shared between [`classify_cause`]'s own
/// `DeadTarget` check and [`super::counterfactual`]'s cosmetic-row merge
/// lookup, rather than each re-deriving it.
pub(super) fn op_sub_path(
    mod_id: &ModId,
    xpath: &str,
    indexed: &[rim_analyzer::analysis::IndexedPatchOp],
) -> Option<String> {
    indexed
        .iter()
        .find(|entry| &entry.mod_id == mod_id && op_xpath_matches(&entry.op, xpath))
        .and_then(|entry| entry.op.target.as_ref())
        .and_then(|target| target.sub_path.clone())
}

/// Every `Conflict::PatchCollision` in the session's own report, indexed
/// by `(def_type, def_name, selector, sub_path)` — built once per
/// [`VerifyOrder::execute`](super::VerifyOrder::execute) call, the same
/// pattern [`EdgeEvidence`] already uses, so a [`ReorderKind::Cosmetic`]
/// row can name the existing merge that keeps the losing mod's intent —
/// the fallback offered since no `set-pair` rule is offered for a
/// cosmetic row — without re-scanning `session.report().conflicts` per
/// row.
pub(super) struct PatchCollisionIndex {
    by_target: BTreeMap<(String, String, Selector, Option<String>), FindingKey>,
}

impl PatchCollisionIndex {
    pub(super) fn build(session: &Session) -> Self {
        let mut by_target = BTreeMap::new();
        for conflict in &session.report().conflicts {
            let Conflict::PatchCollision(collision) = conflict else {
                continue;
            };
            let mods: BTreeSet<ModId> = collision
                .mods
                .iter()
                .map(|entry| entry.mod_id.clone())
                .collect();
            let key = FindingKey::PatchCollision {
                key: DefKey {
                    def_type: collision.def_type.clone(),
                    def_name: collision.def_name.clone(),
                },
                selector: collision.selector,
                sub_path: collision.sub_path.clone(),
                mods,
            };
            by_target.insert(
                (
                    collision.def_type.clone(),
                    collision.def_name.clone(),
                    collision.selector,
                    collision.sub_path.clone(),
                ),
                key,
            );
        }
        Self { by_target }
    }

    /// The `PatchCollision` finding at this exact `(def, selector,
    /// sub_path)`, if the ledger has one. `sub_path: None` is itself a
    /// meaningful key (a whole-def-level collision), not "unknown" — a
    /// caller with no resolvable sub_path passes it through unchanged
    /// rather than guessing.
    pub(super) fn get(
        &self,
        def_type: &str,
        def_name: &str,
        selector: Selector,
        sub_path: Option<&str>,
    ) -> Option<&FindingKey> {
        self.by_target.get(&(
            def_type.to_string(),
            def_name.to_string(),
            selector,
            sub_path.map(str::to_string),
        ))
    }
}

/// The [`ReorderKind`] a freshly-classified `cause` starts at, before the
/// counterfactual phase might rewrite either — `Some(ReorderKind::Content)`
/// for the two order-fixable causes, `None` for the two that aren't.
/// **Never `Cosmetic` here**: [`EdgeEvidence`] only ever tracks the
/// content `EdgeKind::PatchRemovedNode`, never `PatchRemovedNodeCosmetic`
/// (see its own doc comment), so a `RemovedBy`/`NotYetInjected` this
/// function classifies is always the content case — the cosmetic case
/// only ever comes from `rim_merge::effective::counterfactual`'s own
/// `final_def_unchanged`, in `super::counterfactual`.
pub(super) fn reorder_kind_for_cause(cause: &PatchFailureCause) -> Option<ReorderKind> {
    match cause {
        PatchFailureCause::RemovedBy(_) | PatchFailureCause::NotYetInjected(_) => {
            Some(ReorderKind::Content)
        }
        PatchFailureCause::DeadTarget | PatchFailureCause::Unknown => None,
    }
}

/// Whether `op`'s own xpath text is exactly `xpath` — [`Caveat::FailedOp::xpath`]
/// is the failed op's own literal text, so this is the correct (and only
/// sound) way back to the [`rim_analyzer::domain::PatchOp`] that produced
/// it, never a fuzzy match.
fn op_xpath_matches(op: &PatchOp, xpath: &str) -> bool {
    op.xpath.as_deref() == Some(xpath)
}

/// One edge in the session's own report that a pair rule enforcing
/// `after` after `before` (an `Action::Reorder { after, before }`, the
/// same shape `rim_resolve::ledger::suggest` already offers a
/// `PatchWillFail` finding) would contradict: a Reorder row that reverses
/// a satisfied edge in the report, or re-breaks a cycle the sorter already
/// resolved, must say so.
///
/// No `Reorder` is attached to a row in this module — it classifies a
/// `PatchFailureCause` (`RemovedBy`/`NotYetInjected`/`DeadTarget`/`Unknown`)
/// per row, and the `Action::Reorder` a "fix" actually offers is computed
/// downstream, on demand, by `rim_resolve::ledger::suggest` at each
/// interface's own DTO layer (`apps/cli/src/commands/verify.rs`'s own
/// `Reorder` struct, `apps/desktop/src-tauri/src/dto/verify.rs`'s
/// `reorder_of`). [`reorder_conflicts`] lives here as the one pure,
/// testable function both of those call sites share, rather than each
/// re-deriving the same edge-direction lookup independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderConflictDirection {
    /// The contradicting edge is the opposite direction from the
    /// proposed reorder — enforcing the proposal would reverse it.
    Reverses,
    /// The contradicting edge is the *same* direction as the proposed
    /// reorder, and the sorter already dropped it to break a real
    /// cycle — enforcing the proposal would just re-assert the exact
    /// direction the sorter tried and gave up on.
    ReAsserts,
}

/// One report edge that contradicts a proposed `Action::Reorder` — see
/// [`reorder_conflicts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorderConflict {
    /// The contradicting edge's own kind.
    pub kind: EdgeKind,
    /// The contradicting edge's own detail text, verbatim.
    pub detail: String,
    /// The contradicting edge's own status, evaluated against the
    /// sorter's own suggested order (`outcome.order`).
    pub status: rim_analyzer::domain::EdgeStatus,
    /// Which of the two conflict shapes this is — see
    /// [`ReorderConflictDirection`].
    pub direction: ReorderConflictDirection,
}

/// Every edge in `report` that contradicts enforcing `after` after
/// `before` — every edge whose own `(after, before)` is the **opposite**
/// of the proposed pair, at any status (the proposal would reverse an
/// author declaration or an already-satisfied inferred/awareness fact),
/// plus every edge in the **same** direction that `outcome` itself
/// actually **dropped** while breaking a cycle (`outcome.dropped`) — the
/// proposal would just re-assert a direction the sorter already tried
/// and had to drop. Empty when the proposed pair contradicts nothing in
/// the report.
///
/// **Only `outcome.dropped` counts for same-direction edges**: a
/// same-direction edge `outcome.order` merely violates was not necessarily
/// dropped by the sorter. An `Awareness`-strength edge (e.g.
/// `PatchSelectsInjectedNode`) is advisory and never even enters the
/// graph unless enforcement is on, so it is violated by the hundreds
/// under any real order regardless of any cycle; flagging one as a
/// conflict would falsely tell the user a free pair rule was contested.
/// Only `outcome.dropped` — the sorter's own record of edges it genuinely
/// had to give up to keep the graph acyclic — proves "re-breaks a cycle
/// the sorter already resolved". Opposite-direction edges are unaffected:
/// reversing a declaration or an already-satisfied fact is a conflict
/// regardless of whether the graph ever cycled over it.
///
/// Status is still evaluated fresh via `rim_resolve::evaluate::edge_status`
/// against `outcome.order`, never against `EdgeReport::status` — that
/// field is fixed to whichever order was active at scan time (see
/// `rim_resolve::evaluate`'s own doc comment).
#[must_use]
pub fn reorder_conflicts(
    report: &rim_analyzer::domain::Report,
    outcome: &rim_resolve::sort::SortOutcome,
    after: &ModId,
    before: &ModId,
) -> Vec<ReorderConflict> {
    let order = &outcome.order;
    let was_dropped = |edge_after: &ModId, edge_before: &ModId| {
        outcome
            .dropped
            .iter()
            .any(|dropped| &dropped.edge.after == edge_after && &dropped.edge.before == edge_before)
    };
    report
        .edges
        .iter()
        .map(|edge_report| &edge_report.edge)
        .filter_map(|edge| {
            if &edge.after == before && &edge.before == after {
                return Some((edge, ReorderConflictDirection::Reverses));
            }
            if &edge.after == after && &edge.before == before && was_dropped(after, before) {
                return Some((edge, ReorderConflictDirection::ReAsserts));
            }
            None
        })
        .map(|(edge, direction)| ReorderConflict {
            kind: edge.kind,
            detail: edge.detail.clone(),
            status: rim_resolve::evaluate::edge_status(edge, order),
            direction,
        })
        .collect()
}
