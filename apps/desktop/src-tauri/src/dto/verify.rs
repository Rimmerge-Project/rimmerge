//! DTOs for `verify_order`: the apply dialog's on-demand
//! `rim_session::use_cases::VerifyOrder` pass. Mirrors `apps/cli`'s own
//! `verify` command's grouped JSON shape (`apps/cli/src/commands/verify.rs`)
//! — camelCase here per this crate's own DTO convention, snake_case there
//! per that crate's — so the two surfaces report the identical facts.
//!
//! **Grouped by `(mod, operation identity)`, not one entry per
//! `Finding`**: `VerifyOrder`'s own
//! architecture is per-def — a top-level operation whose head matches
//! several defNames (a common `OR`-list idiom) is checked once per
//! matched def, producing one `Finding::PatchWillFail` each, even though
//! RimWorld's own log reports that operation's failure exactly once. On a
//! real install one sequence-wrapper mod's compatibility-patch operation
//! can account for many findings sharing one rendered identity, most of
//! them genuinely distinct real operations, not duplicates — see
//! `apps/cli/src/commands/verify.rs`'s own doc comment. Grouping happens
//! once here, in this DTO layer
//! (never inside `VerifyOrder` itself, which still checks and reports
//! per def — that data is kept in full under [`VerifyOperationDto::defs`],
//! genuinely useful for diagnosis) so the apply dialog's own Vue
//! component can iterate `operations` directly with no further grouping
//! of its own to get right.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Action, Finding, ReorderKind};
use rim_resolve::ledger::{SuggestContext, suggest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::DefKeyDto;
use super::common::OrderSourceDto;
use super::common::SelectorDto;
use super::finding::{PatchFailureCauseDto, RationaleDto};

/// Request shape for `verify_order`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyRequestDto {
    /// Which order to check.
    pub source: OrderSourceDto,
}

/// A candidate def this pass could not check at all, with why. Mirrors
/// one entry of [`rim_session::use_cases::VerifyOrderReport::skipped`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifySkippedDto {
    /// The def or template that couldn't be checked.
    pub def_key: DefKeyDto,
    /// Which attribute the target predicate matched on.
    pub selector: SelectorDto,
    /// Why, verbatim.
    pub reason: String,
}

/// The one load-order change that would make a predicted failure
/// succeed, as `rim_resolve::ledger::suggest` states it — the exact
/// `(after, before)` pair a
/// [`rim_session::use_cases::UpsertRule`] call needs, plus the
/// suggestion's own rationale text for the rule's `comment`.
///
/// **Computed here, in Rust, and never re-derived on the TypeScript
/// side.** The direction is the whole content of this type: a
/// `RemovedBy(remover)` cause means *this* mod must load **before** the
/// remover, while a `NotYetInjected(injector)` cause means it must load
/// **after** the injector — an inversion no test of the Vue component
/// would catch, because both shapes render the same. `suggest`'s own
/// `patch_will_fail` arms already carry it, so this reads it rather than restating it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyReorderDto {
    /// The mod that must load **after** [`Self::before`].
    pub after: String,
    /// The mod that must load **before** [`Self::after`].
    pub before: String,
    /// The suggestion's own plain-English reason, used verbatim as the
    /// created pair rule's `comment` so `rules.json` records why the row
    /// exists. See `docs/translating.md`: this field is never localized —
    /// the dialog renders [`Self::rationale_code`] instead and leaves this
    /// one as the English editable prefill.
    pub rationale: String,
    /// The same reason, structured for a localized rendering.
    pub rationale_code: RationaleDto,
    /// Every report edge this reorder contradicts — reversing an author
    /// declaration or an already-satisfied fact, or re-asserting a
    /// direction the sorter already tried and had to drop to break a
    /// real cycle. Empty when there are none. See
    /// [`rim_session::use_cases::reorder_conflicts`].
    pub conflicts: Vec<VerifyReorderConflictDto>,
}

/// One [`rim_session::use_cases::ReorderConflict`]. See
/// [`VerifyReorderDto::conflicts`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyReorderConflictDto {
    /// The contradicting edge's own kind.
    pub kind: super::common::EdgeKindDto,
    /// The contradicting edge's own detail text, verbatim.
    pub detail: String,
    /// The contradicting edge's own status, evaluated against the
    /// sorter's own suggested order.
    pub status: super::common::EdgeStatusDto,
    /// Which conflict shape this is — see
    /// [`rim_session::use_cases::ReorderConflictDirection`].
    pub direction: ReorderConflictDirectionDto,
}

/// Mirrors [`rim_session::use_cases::ReorderConflictDirection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ReorderConflictDirectionDto {
    /// See [`rim_session::use_cases::ReorderConflictDirection::Reverses`].
    Reverses,
    /// See [`rim_session::use_cases::ReorderConflictDirection::ReAsserts`].
    ReAsserts,
}

impl From<rim_session::use_cases::ReorderConflictDirection> for ReorderConflictDirectionDto {
    fn from(value: rim_session::use_cases::ReorderConflictDirection) -> Self {
        match value {
            rim_session::use_cases::ReorderConflictDirection::Reverses => Self::Reverses,
            rim_session::use_cases::ReorderConflictDirection::ReAsserts => Self::ReAsserts,
        }
    }
}

impl From<&rim_session::use_cases::ReorderConflict> for VerifyReorderConflictDto {
    fn from(value: &rim_session::use_cases::ReorderConflict) -> Self {
        Self {
            kind: value.kind.into(),
            detail: value.detail.clone(),
            status: value.status.into(),
            direction: value.direction.into(),
        }
    }
}

/// Mirrors [`rim_resolve::domain::ReorderKind`]. `None` on
/// [`VerifyDefTargetDto::reorder_kind`] itself covers the two causes
/// that offer no reorder at all (`DeadTarget`/`Unknown`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ReorderKindDto {
    /// See [`rim_resolve::domain::ReorderKind::Content`].
    Content,
    /// See [`rim_resolve::domain::ReorderKind::Cosmetic`]. No
    /// `set-pair` rule is offered for a row classified this way —
    /// [`VerifyDefTargetDto::reorder`] is always `None` alongside it.
    #[serde(rename_all = "camelCase")]
    Cosmetic {
        /// An existing `PatchCollision` finding's own canonical key
        /// text (`rimmerge merge plan --key`-ready), if the ledger has
        /// one at this def and sub_path.
        existing_merge_key: Option<String>,
    },
}

impl From<&ReorderKind> for ReorderKindDto {
    fn from(value: &ReorderKind) -> Self {
        match value {
            ReorderKind::Content => Self::Content,
            ReorderKind::Cosmetic { existing_merge } => Self::Cosmetic {
                existing_merge_key: existing_merge.as_ref().map(ToString::to_string),
            },
        }
    }
}

/// One def target a grouped operation's own failure was predicted
/// against. See this module's own doc comment for why grouping exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyDefTargetDto {
    /// The def or template this operation was predicted to fail on.
    pub def_key: DefKeyDto,
    /// Which attribute the operation's own target predicate matched on.
    pub selector: SelectorDto,
    /// The specific nested leaf's own xpath, when the failure traces to
    /// one identifiable leaf — diagnostic only.
    pub leaf_xpath: Option<String>,
    /// Why, classified from the replay.
    pub cause: PatchFailureCauseDto,
    /// Whether this row's own reorder-bearing cause would change the
    /// final resolved def — `None` for `DeadTarget`/`Unknown`. The Apply
    /// dialog greys out a `Cosmetic` row instead of offering `reorder`.
    pub reorder_kind: Option<ReorderKindDto>,
    /// The reorder that would fix this row, when the cause has one —
    /// `None` for `DeadTarget`/`Unknown`, which offer no alternative at
    /// all, and **also** `None` for a `Cosmetic` row: the fix wouldn't
    /// change anything RimWorld actually loads. See [`VerifyReorderDto`].
    pub reorder: Option<VerifyReorderDto>,
}

/// Every `Finding::PatchWillFail` sharing one real `(mod, operation
/// identity)` pair, collapsed to a single entry — matches what RimWorld's
/// own log reports (one line per real operation), with every affected
/// def target kept underneath for diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyOperationDto {
    /// The mod whose operation is predicted to fail.
    pub mod_id: String,
    /// The top-level operation's own RimWorld-log identity text —
    /// matches what a user sees in their own log.
    pub operation: String,
    /// Every def target this operation was predicted to fail against.
    pub defs: Vec<VerifyDefTargetDto>,
}

/// The counterfactual phase, summarized —
/// additive, and all zeroes when the phase found nothing to do.
/// Deliberately a *subset* of [`rim_session::use_cases::CounterfactualStats`]:
/// the full per-cap and per-refusal breakdown is a measurement surface
/// (`apps/cli`'s own `verify --json` carries it), while the dialog only
/// ever shows a summary line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyCounterfactualDto {
    /// How many failing operations the phase actually experimented on.
    pub jobs: usize,
    /// `Unknown`-caused rows that gained a `Reorder`-bearing cause.
    pub resolved: usize,
    /// `DeadTarget`-caused rows a counterfactual move contradicted.
    pub demoted_dead_targets: usize,
    /// Jobs skipped because their def has too many distinct patcher mods
    /// for the experiment's own cost bound.
    pub skipped_too_many_mods: usize,
    /// Jobs skipped because the failing mod also *owns* the def alongside
    /// its winner, so the reorder would change which owner wins — the
    /// one thing the experiment holds fixed.
    pub skipped_co_owner: usize,
    /// Moves refused because they fixed one operation and broke another.
    pub rejected_for_regression: usize,
}

impl From<&rim_session::use_cases::CounterfactualStats> for VerifyCounterfactualDto {
    fn from(value: &rim_session::use_cases::CounterfactualStats) -> Self {
        Self {
            jobs: value.jobs,
            resolved: value.resolved,
            demoted_dead_targets: value.demoted_dead_targets,
            skipped_too_many_mods: value.skipped_too_many_mods,
            skipped_co_owner: value.skipped_co_owner,
            rejected_for_regression: value.rejected_for_regression,
        }
    }
}

/// `verify_order`'s result. Mirrors
/// [`rim_session::use_cases::VerifyOrderReport`], grouped — see this
/// module's own doc comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyReportDto {
    /// Which order this was checked against.
    pub source: OrderSourceDto,
    /// How many defs this pass actually attempted to check.
    pub defs_checked: usize,
    /// `operations.len()` — how many distinct real operations are
    /// predicted to fail. Named explicitly (not left for the frontend to
    /// derive) so neither this number nor [`Self::def_targets_total`] is
    /// hidden behind the other.
    pub operations_total: usize,
    /// The total def-target count across every operation — the raw
    /// per-def `Finding` count `VerifyOrder` itself produced, before
    /// grouping.
    pub def_targets_total: usize,
    /// Every predicted-to-fail operation, grouped.
    pub operations: Vec<VerifyOperationDto>,
    /// Every candidate this pass couldn't check at all.
    pub skipped: Vec<VerifySkippedDto>,
    /// What the counterfactual phase did.
    pub counterfactual: VerifyCounterfactualDto,
}

/// Groups `findings` by `(mod_id, operation)` — see this module's own
/// doc comment for why. Key order (`BTreeMap<(ModId, &str), _>`) is
/// deterministic, matching this codebase's own determinism convention
/// (no `HashMap` reaching output).
fn group_by_operation(findings: &[Finding], ctx: &SuggestContext<'_>) -> Vec<VerifyOperationDto> {
    let mut groups: BTreeMap<(ModId, &str), VerifyOperationDto> = BTreeMap::new();
    for finding in findings {
        let Finding::PatchWillFail {
            mod_id,
            def_key,
            selector,
            operation,
            leaf_xpath,
            cause,
            reorder_kind,
        } = finding
        else {
            continue;
        };
        // A cosmetic row never offers `set-pair`: the fix wouldn't
        // change anything RimWorld actually loads, only which op's own
        // failure it would log.
        let reorder = match reorder_kind {
            Some(ReorderKind::Cosmetic { .. }) => None,
            _ => reorder_of(finding, ctx),
        };
        groups
            .entry((mod_id.clone(), operation.as_str()))
            .or_insert_with(|| VerifyOperationDto {
                mod_id: mod_id.as_str().to_string(),
                operation: operation.clone(),
                defs: Vec::new(),
            })
            .defs
            .push(VerifyDefTargetDto {
                def_key: def_key.clone().into(),
                selector: (*selector).into(),
                leaf_xpath: leaf_xpath.clone(),
                cause: cause.into(),
                reorder_kind: reorder_kind.as_ref().map(Into::into),
                reorder,
            });
    }
    groups.into_values().collect()
}

/// The one `Action::Reorder` alternative
/// `rim_resolve::ledger::suggest` offers for this finding, if any.
///
/// Reads the *first* `Reorder` alternative rather than searching for a
/// particular one: `patch_will_fail`'s `RemovedBy`/`NotYetInjected` arms
/// each carry exactly one, and its direction is already correct for the
/// cause. A `DeadTarget`/`Unknown` row has `alternatives: Vec::new()`,
/// so this is `None`.
///
/// `ctx` is required by [`suggest`]'s own signature, not by this finding
/// kind: `patch_will_fail` reads none of the report, sort outcome,
/// current order or `mods_by_id`. Building the full context here is the
/// price of going through the one public entry point rather than
/// duplicating its `RemovedBy`/`NotYetInjected` direction table — which
/// is exactly the duplication this field exists to prevent.
fn reorder_of(finding: &Finding, ctx: &SuggestContext<'_>) -> Option<VerifyReorderDto> {
    suggest(finding, ctx)
        .alternatives
        .into_iter()
        .find_map(|alternative| match alternative.action {
            Action::Reorder { after, before } => {
                let conflicts = rim_session::use_cases::reorder_conflicts(
                    ctx.report,
                    ctx.sort_outcome,
                    &after,
                    &before,
                );
                Some(VerifyReorderDto {
                    after: after.as_str().to_string(),
                    before: before.as_str().to_string(),
                    rationale: alternative.rationale.to_string(),
                    rationale_code: (&alternative.rationale).into(),
                    conflicts: conflicts.iter().map(Into::into).collect(),
                })
            }
            _ => None,
        })
}

impl VerifyReportDto {
    /// Builds the DTO from a finished pass.
    ///
    /// Takes a [`SuggestContext`] rather than being a plain `From` impl
    /// because the `reorder` field is computed by
    /// calling [`suggest`] per finding — the direction must never be
    /// re-derived on the TypeScript side (see [`VerifyReorderDto`]), and
    /// `suggest` needs the report/sort outcome/current order the session
    /// holds. Deliberately the only constructor: a second one (a plain `From`
    /// impl) that silently drops `reorder` is exactly the trap that would ship
    /// a dialog with no buttons and no failing test.
    #[must_use]
    pub(crate) fn from_report(
        value: &rim_session::use_cases::VerifyOrderReport,
        ctx: &SuggestContext<'_>,
    ) -> Self {
        let operations = group_by_operation(&value.findings, ctx);
        Self {
            source: value.source.into(),
            defs_checked: value.defs_checked,
            operations_total: operations.len(),
            def_targets_total: value.findings.len(),
            operations,
            skipped: value
                .skipped
                .iter()
                .map(|(def_key, selector, reason)| VerifySkippedDto {
                    def_key: def_key.clone().into(),
                    selector: (*selector).into(),
                    reason: reason.clone(),
                })
                .collect(),
            counterfactual: (&value.counterfactual).into(),
        }
    }
}

/// Payload of the `verify://progress` event, emitted while `verify_order`
/// runs — a real-install pass can take minutes per order source on a large
/// install, so the apply dialog's own progress bar needs genuine feedback,
/// not a frozen dialog; mirrors [`super::project::ProgressEventDto`]'s
/// own shape, fed by
/// [`rim_session::use_cases::VerifyOrder::execute_with_progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct VerifyProgressEventDto {
    /// How many *checks* have started — candidate defs in phase 1, then
    /// counterfactual jobs in phase 2. Named `checked` for wire compatibility;
    /// it counts two different kinds of unit, which is why the dialog's own
    /// caption says "checks" rather than "defs checked".
    pub checked: usize,
    /// The total for this pass. **Grows exactly once, mid-stream**: it
    /// is the candidate-def count until the counterfactual phase's own
    /// job count `J` is known, then `defs + J`. A consumer must
    /// recompute its percentage from both fields rather than caching the
    /// first `total` it sees.
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{Mod, ModId, Selector};
    use rim_resolve::domain::{DefKey, Finding, OrderSource, PatchFailureCause};
    use rim_session::Session;
    use rim_session::use_cases::VerifyOrderReport;

    use super::*;

    /// A real [`SuggestContext`] over a throwaway session, so these
    /// tests exercise `rim_resolve::ledger::suggest` itself rather than a
    /// stub — the direction on [`VerifyReorderDto`] is the one thing a
    /// stubbed suggestion could not prove.
    fn mods_by_id(session: &Session) -> BTreeMap<ModId, &Mod> {
        session
            .report()
            .mods
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect()
    }

    fn ctx<'a>(session: &'a Session, mods: &'a BTreeMap<ModId, &'a Mod>) -> SuggestContext<'a> {
        SuggestContext {
            report: session.report(),
            sort_outcome: session.sort_outcome(),
            current: &session.orders().current,
            mods_by_id: mods,
        }
    }

    fn patch_will_fail(mod_id: &str, def_name: &str, cause: PatchFailureCause) -> Finding {
        let reorder_kind = match &cause {
            PatchFailureCause::RemovedBy(_) | PatchFailureCause::NotYetInjected(_) => {
                Some(ReorderKind::Content)
            }
            PatchFailureCause::DeadTarget | PatchFailureCause::Unknown => None,
        };
        Finding::PatchWillFail {
            mod_id: ModId::new(mod_id),
            def_key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: def_name.to_string(),
            },
            selector: Selector::DefName,
            operation: format!(
                "Verse.PatchOperationAdd(Defs/ThingDef[defName=\"{def_name}\"]/comps)"
            ),
            leaf_xpath: None,
            cause,
            reorder_kind,
        }
    }

    fn report_of(findings: Vec<Finding>) -> VerifyOrderReport {
        VerifyOrderReport {
            source: OrderSource::Suggested,
            findings,
            defs_checked: 1,
            skipped: Vec::new(),
            counterfactual: rim_session::use_cases::CounterfactualStats::default(),
            suppressed_filter_head_ops: 0,
        }
    }

    /// Both causes that carry a `Reorder`
    /// alternative, asserted in **opposite** directions from one run —
    /// the inversion this field exists to prevent would flip exactly one
    /// of these two and leave the other passing.
    #[test]
    fn order_fixable_def_targets_carry_the_suggested_reorder_in_the_causes_own_direction() {
        let (_temp, session) = crate::test_support::session_fixture_with_temp_paths(&[
            "subject.mod",
            "remover.mod",
            "injector.mod",
        ]);
        let mods = mods_by_id(&session);
        let ctx = ctx(&session, &mods);
        let report = report_of(vec![
            patch_will_fail(
                "subject.mod",
                "Wall",
                PatchFailureCause::RemovedBy(ModId::new("remover.mod")),
            ),
            patch_will_fail(
                "subject.mod",
                "Door",
                PatchFailureCause::NotYetInjected(ModId::new("injector.mod")),
            ),
            patch_will_fail("subject.mod", "Lamp", PatchFailureCause::DeadTarget),
            patch_will_fail("subject.mod", "Table", PatchFailureCause::Unknown),
        ]);

        let dto = VerifyReportDto::from_report(&report, &ctx);

        let reorder_for = |def_name: &str| {
            dto.operations
                .iter()
                .flat_map(|operation| operation.defs.iter())
                .find(|def| def.def_key.def_name == def_name)
                .unwrap_or_else(|| panic!("{def_name} row"))
                .reorder
                .clone()
        };

        let removed = reorder_for("Wall").expect("RemovedBy offers a Reorder");
        assert_eq!(
            (removed.after.as_str(), removed.before.as_str()),
            ("remover.mod", "subject.mod"),
            "RemovedBy: the remover must load after the subject, so the node still exists"
        );
        assert!(
            removed.rationale.contains("subject.mod"),
            "the rationale becomes the created rule's comment: {}",
            removed.rationale
        );
        assert!(
            removed.conflicts.is_empty(),
            "this throwaway session's report has no edges to conflict with: {removed:?}"
        );

        let injected = reorder_for("Door").expect("NotYetInjected offers a Reorder");
        assert_eq!(
            (injected.after.as_str(), injected.before.as_str()),
            ("subject.mod", "injector.mod"),
            "NotYetInjected: the subject must load after the injector"
        );

        assert_eq!(
            reorder_for("Lamp"),
            None,
            "DeadTarget offers no alternative"
        );
        assert_eq!(reorder_for("Table"), None, "Unknown offers no alternative");
    }

    /// The test above only ever exercises the
    /// empty-`conflicts` path. A real session whose report carries a
    /// declared edge opposite the `RemovedBy` fix's own direction must
    /// carry that conflict all the way through the DTO mapping.
    #[test]
    fn a_reorder_that_reverses_a_declared_edge_carries_the_conflict_through_the_dto() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("subject.mod")
            .mod_("remover.mod")
            // Opposite direction from the `RemovedBy` fix's own
            // `after: remover.mod, before: subject.mod` — a declared
            // edge the reorder would reverse.
            .declared_edge("subject.mod", "remover.mod")
            .build();
        let (_temp, session) = crate::test_support::session_with_temp_paths(
            report,
            SourceIndex::default(),
            &["subject.mod", "remover.mod"],
        );
        let mods = mods_by_id(&session);
        let ctx = ctx(&session, &mods);
        let report = report_of(vec![patch_will_fail(
            "subject.mod",
            "Wall",
            PatchFailureCause::RemovedBy(ModId::new("remover.mod")),
        )]);

        let dto = VerifyReportDto::from_report(&report, &ctx);
        let reorder = dto.operations[0].defs[0]
            .reorder
            .clone()
            .expect("RemovedBy offers a Reorder");

        assert_eq!(reorder.conflicts.len(), 1);
        assert_eq!(
            reorder.conflicts[0].kind,
            super::super::common::EdgeKindDto::LoadAfter
        );
        assert_eq!(
            reorder.conflicts[0].direction,
            ReorderConflictDirectionDto::Reverses
        );
    }

    #[test]
    fn verify_report_dto_groups_findings_by_mod_and_operation() {
        let report = VerifyOrderReport {
            source: OrderSource::Suggested,
            findings: vec![
                Finding::PatchWillFail {
                    mod_id: ModId::new("x.mod"),
                    def_key: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Wall".to_string(),
                    },
                    selector: Selector::DefName,
                    operation: "Verse.PatchOperationAdd(Defs/ThingDef[defName=\"Wall\" or defName=\"Door\"]/comps)"
                        .to_string(),
                    leaf_xpath: None,
                    cause: PatchFailureCause::DeadTarget,
                    reorder_kind: None,
                },
                // Same mod, same operation identity, a different def
                // target — the real OR-list-inflation shape this DTO's
                // own grouping must collapse to one entry.
                Finding::PatchWillFail {
                    mod_id: ModId::new("x.mod"),
                    def_key: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Door".to_string(),
                    },
                    selector: Selector::DefName,
                    operation: "Verse.PatchOperationAdd(Defs/ThingDef[defName=\"Wall\" or defName=\"Door\"]/comps)"
                        .to_string(),
                    leaf_xpath: None,
                    cause: PatchFailureCause::DeadTarget,
                    reorder_kind: None,
                },
                // A different mod entirely — its own group.
                Finding::PatchWillFail {
                    mod_id: ModId::new("y.mod"),
                    def_key: DefKey {
                        def_type: "GeneDef".to_string(),
                        def_name: "Learning_Fast".to_string(),
                    },
                    selector: Selector::DefName,
                    operation: "Verse.PatchOperationRemove(Defs/GeneDef[defName=\"Learning_Fast\"]/statOffsets)"
                        .to_string(),
                    leaf_xpath: None,
                    cause: PatchFailureCause::Unknown,
                    reorder_kind: None,
                },
            ],
            defs_checked: 5,
            skipped: vec![(DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Injected".to_string(),
                },
                Selector::DefName,
                "owned only by a patch injection".to_string())],
            counterfactual: rim_session::use_cases::CounterfactualStats {
                jobs: 3,
                resolved: 1,
                demoted_dead_targets: 2,
                skipped_too_many_mods: 4,
                skipped_co_owner: 6,
                rejected_for_regression: 5,
                ..Default::default()
            },
            suppressed_filter_head_ops: 0,
        };

        let (_temp, session) =
            crate::test_support::session_fixture_with_temp_paths(&["x.mod", "y.mod"]);
        let mods = mods_by_id(&session);
        let dto = VerifyReportDto::from_report(&report, &ctx(&session, &mods));

        assert_eq!(dto.source, OrderSourceDto::Suggested);
        assert_eq!(dto.defs_checked, 5);
        assert_eq!(dto.def_targets_total, 3, "raw per-def finding count");
        assert_eq!(
            dto.operations_total, 2,
            "two distinct (mod, operation) pairs"
        );
        assert_eq!(dto.operations.len(), 2);

        let x_mod = dto
            .operations
            .iter()
            .find(|op| op.mod_id == "x.mod")
            .expect("x.mod group");
        assert_eq!(x_mod.defs.len(), 2, "both def targets kept under one entry");
        let def_names: Vec<&str> = x_mod
            .defs
            .iter()
            .map(|def| def.def_key.def_name.as_str())
            .collect();
        assert!(def_names.contains(&"Wall"));
        assert!(def_names.contains(&"Door"));

        let y_mod = dto
            .operations
            .iter()
            .find(|op| op.mod_id == "y.mod")
            .expect("y.mod group");
        assert_eq!(y_mod.defs.len(), 1);

        assert_eq!(dto.skipped.len(), 1);
        assert_eq!(dto.skipped[0].selector, SelectorDto::DefName);
        assert_eq!(dto.skipped[0].reason, "owned only by a patch injection");

        assert_eq!(
            dto.counterfactual,
            VerifyCounterfactualDto {
                jobs: 3,
                resolved: 1,
                demoted_dead_targets: 2,
                skipped_too_many_mods: 4,
                skipped_co_owner: 6,
                rejected_for_regression: 5,
            },
            "the counterfactual's additive stats, carried through field for field"
        );
    }
}
