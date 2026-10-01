//! The counterfactual phase: bounded replay of alternative orders for the defs whose replay failed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use rim_analyzer::analysis::indices::{ActiveMods, DisplayNameIndex};
use rim_analyzer::domain::{LoadOrder, ModId, Selector, XmlLocator};
use rim_merge::effective::{self, EffectiveInput};
use rim_merge::patch_eval::{PatchContribution, ReplayContext};
use rim_resolve::domain::{Finding, PatchFailureCause, ReorderKind};

use super::VerifyOrder;
use super::predict::{
    ReplayEnvironment, active_top_level_operations, has_any_owner, has_raw_owner,
    is_co_owner_with_another,
};
use super::report::{PatchCollisionIndex, op_sub_path};
use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources;

/// The OR-head aggregation key —
/// one top-level `<Operation>` ancestor (`(mod, file, first
/// element_path ordinal)`, the same identity [`rim_analyzer::analysis::SourceIndex::ops_by_mod`]/
/// `def_sources::top_level_operations` already dedupe on) **plus the
/// specific leaf xpath actually being evaluated**, from
/// [`def_sources::representative_op`].
///
/// **The fourth field is not optional narrowing — dropping it is a real
/// correctness bug.** A top-level ancestor is shared by two
/// structurally different shapes, and only the first is the aggregation's actual
/// target: (a) one leaf mutation whose own `<xpath>` genuinely is an
/// OR-ed defName head — the real target, where `representative_op`'s
/// `PatchOp::xpath` is the *same literal string* for every def key it's
/// indexed under, success or failure, since it names a single physical
/// XML node's own static text; (b) a `PatchOperationSequence`/`FindMod`
/// wrapper whose children each target a *different, single* def — not
/// one OR-ed query at all, just several queries sharing one gate — where
/// each child's own `representative_op` xpath *differs* per def key. Key
/// on `(mod, file, ordinal)` alone and (b) collapses into one group
/// exactly like (a), and a success on any one of its many unrelated
/// children wrongly suppresses every other child's own real failure —
/// the case a real content pack's `FindMod` wrapper hits. Keying on `representative_op`'s own
/// xpath text as well splits (b)'s children back into their own separate
/// groups (each with too few entries — usually one — to ever suppress
/// anything) while leaving (a) untouched (uniform text, so the intended
/// grouping is unaffected) — so aggregation cannot cost recall except
/// through a wrong success.
pub(super) type TopLevelOpKey = (ModId, Arc<Path>, u32, String);

/// Builds a [`TopLevelOpKey`] from one `(mod, locator)` pair as
/// [`active_top_level_operations`] returns it, plus `indexed` — the
/// current def key's own op list, the same slice [`classify_cause`]
/// already reads — to recover the specific leaf xpath via
/// [`def_sources::representative_op`] (see [`TopLevelOpKey`]'s own doc
/// comment for why that field is load-bearing, not decorative).
/// `locator.element_path` is always exactly one ordinal for a top-level
/// entry — `def_sources::top_level_operations` builds every such locator
/// as `XmlLocator::new(file, vec![first_ordinal])` — so the first
/// `unwrap_or` here never actually falls back; it exists only to keep
/// this function `unwrap`/`expect`-free, per this workspace's own ban on
/// both outside tests, without asserting an invariant this function has
/// no way to enforce itself. The second `unwrap_or` (an absent
/// `representative_op`/`xpath`) is likewise defensive, not expected in
/// practice: `mod_id`/`locator` are read off `top_level`, which
/// `def_sources::top_level_operations` itself builds *from* `indexed` —
/// so a matching entry is always there to find.
pub(super) fn top_level_op_key(
    mod_id: &ModId,
    locator: &XmlLocator,
    indexed: &[rim_analyzer::analysis::IndexedPatchOp],
) -> TopLevelOpKey {
    let leaf_xpath = def_sources::representative_op(indexed, mod_id, locator)
        .and_then(|op| op.xpath.clone())
        .unwrap_or_default();
    (
        mod_id.clone(),
        locator.file.clone(),
        locator.element_path.first().copied().unwrap_or(0),
        leaf_xpath,
    )
}

/// The counterfactual phase's first cap: a def with
/// more than this many distinct patcher mods (`K`) is skipped outright.
/// One subject costs `K - 1` folds, each replaying every contribution
/// once — `(K - 1) * contributions.len()` calls to
/// `rim_merge::patch_eval::replay`, **not** `(K - 1) * K`, since one
/// mod's block can hold several operations. At
/// `K = 12` that is 11 folds per subject, and 132 replays only in the
/// degenerate case where every block holds exactly one operation. Every
/// skip is counted in [`CounterfactualStats::skipped_too_many_mods`],
/// never silently applied.
const MAX_COUNTERFACTUAL_MODS: usize = 12;

/// The second cap: at most this many failing operations per def get the
/// experiment. A def with `J` failing subjects costs `J` times
/// [`MAX_COUNTERFACTUAL_MODS`]'s own per-subject figure, and the marginal
/// value of the fifth subject on one def is far below that of the first
/// subject on another.
pub(super) const MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF: usize = 4;

/// The third cap: a hard ceiling on the whole phase, whatever the
/// per-def caps let through. `Unknown` rows are enqueued ahead of
/// `DeadTarget` rows so this ceiling can never starve the
/// acceptance case.
const MAX_COUNTERFACTUAL_JOBS: usize = 1000;

/// What the counterfactual phase did — every number reported rather than
/// folded away, because each one is a falsifiable claim about the
/// classifier and because the phase's own cost has a stated
/// budget to be measured against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterfactualStats {
    /// How many `(finding, subject)` experiments survived the caps and
    /// were actually enqueued — the `J` the two-phase progress contract
    /// reports against.
    pub jobs: usize,
    /// `Unknown` rows that gained a `Reorder`-bearing cause.
    pub resolved: usize,
    /// `DeadTarget` rows a counterfactual move contradicted — a
    /// non-zero count is a finding about the classifier in its own right,
    /// not a silent fix.
    pub demoted_dead_targets: usize,
    /// Jobs dropped because their def has more than
    /// [`MAX_COUNTERFACTUAL_MODS`] distinct patcher mods.
    pub skipped_too_many_mods: usize,
    /// Jobs dropped by [`MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF`].
    pub skipped_too_many_subjects: usize,
    /// Jobs dropped by [`MAX_COUNTERFACTUAL_JOBS`].
    pub skipped_job_ceiling: usize,
    /// Jobs whose def could not be replayed again at all — a zero-owner
    /// or injected-only target (so there is no raw XML to fold), or a
    /// source read that failed this time round.
    pub skipped_not_replayable: usize,
    /// Jobs whose subject mod is also an owner of the def alongside the
    /// winner, so the reorder the experiment would recommend could change
    /// *which owner wins* — the one thing the experiment holds fixed. A
    /// prediction that no longer describes the order it recommends is
    /// worse than no prediction, so such a job is refused outright rather
    /// than answered approximately. See
    /// [`rim_merge::effective::counterfactual`]'s own "Scope" paragraph.
    pub skipped_co_owner: usize,
    /// Inputs the pure experiment refused because some mod's
    /// contributions were not contiguous
    /// ([`rim_merge::effective::CounterfactualOutcome::refused_non_contiguous`])
    /// — `def_sources::top_level_operations` never produces one, so a
    /// non-zero count here is a bug signal, not a steady state.
    pub refused_non_contiguous: usize,
    /// Jobs whose failing operation could not be located in the
    /// re-computed baseline's own outcomes (an identity that no longer
    /// matches — should not happen, counted rather than assumed away).
    pub skipped_subject_not_found: usize,
    /// Total alternative orders replayed across every job.
    pub attempts: usize,
    /// Alternatives refused because they broke another operation.
    pub rejected_for_regression: usize,
    /// Alternatives refused because the fold stopped at or before the
    /// subject.
    pub rejected_for_truncation: usize,
    /// Equal-distance ties resolved later-move-first.
    pub ties: usize,
    /// `K` (distinct patcher mods) -> how many candidate defs had it —
    /// measured so the per-def cap stays grounded in real installs. Covers every def a job reached
    /// the block-count stage on, including the ones
    /// [`MAX_COUNTERFACTUAL_MODS`] then rejected.
    pub mods_per_def: BTreeMap<usize, usize>,
}

/// One experiment: re-classify the failing operation behind
/// `findings[finding_index]` by replaying its mod's block at every other
/// position on that def.
#[derive(Debug, Clone)]
struct CounterfactualJob {
    finding_index: usize,
    def_type: String,
    def_name: String,
    selector: Selector,
    mod_id: ModId,
    operation: String,
    /// Whether this row's cause was `DeadTarget` (a *demotion* candidate)
    /// rather than `Unknown` (a *resolution* candidate).
    was_dead_target: bool,
}

/// The per-call constants the counterfactual phase needs — bundled
/// purely to keep [`VerifyOrder::run_counterfactual_phase`]'s own
/// parameter count within clippy's limit, exactly as
/// [`ReplayEnvironment`] already is for [`zero_owner_outcomes`].
pub(super) struct CounterfactualEnvironment<'a> {
    pub(super) order: &'a LoadOrder,
    pub(super) replay: &'a ReplayEnvironment<'a>,
    pub(super) active_gate: &'a ActiveMods,
    pub(super) name_map: &'a DisplayNameIndex,
    /// Every `PatchCollision` finding in the session's own report,
    /// indexed for a [`ReorderKind::Cosmetic`] row's own merge lookup —
    /// see [`PatchCollisionIndex`]'s own doc comment.
    pub(super) patch_collisions: &'a PatchCollisionIndex,
}

/// Builds the phase's job queue from the **reconciled** findings (an
/// operation the OR-head aggregation suppressed never costs a single
/// fold), applying the per-def and total caps.
///
/// `Unknown` rows are enqueued ahead of `DeadTarget` rows so
/// [`MAX_COUNTERFACTUAL_JOBS`] can never starve the acceptance case
/// Within each pass the order is `findings`' own, which the
/// reconciliation already made deterministic.
fn build_counterfactual_jobs(
    session: &Session,
    findings: &[Finding],
) -> (Vec<CounterfactualJob>, CounterfactualStats) {
    let mut stats = CounterfactualStats::default();
    let mut jobs: Vec<CounterfactualJob> = Vec::new();
    let mut subjects_per_def: BTreeMap<(String, String, Selector), usize> = BTreeMap::new();

    for dead_target_pass in [false, true] {
        for (finding_index, finding) in findings.iter().enumerate() {
            let Finding::PatchWillFail {
                mod_id,
                def_key,
                selector,
                operation,
                cause,
                ..
            } = finding
            else {
                continue;
            };
            let is_dead_target = match cause {
                PatchFailureCause::Unknown => false,
                PatchFailureCause::DeadTarget => true,
                // Already carries a `Reorder` — nothing to learn.
                PatchFailureCause::RemovedBy(_) | PatchFailureCause::NotYetInjected(_) => continue,
            };
            if is_dead_target != dead_target_pass {
                continue;
            }

            // Checked before the caps, not after: a zero-owner or
            // injected-only target has no raw XML to re-permute, so
            // letting one consume a slot under
            // [`MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF`] or
            // [`MAX_COUNTERFACTUAL_JOBS`] would spend the budget on an
            // experiment that cannot run — and `DeadTarget` rows, the
            // pass where every zero-owner row lands, are exactly the
            // ones the ceiling is meant to give up on last.
            if !has_any_owner(session, &def_key.def_type, &def_key.def_name, *selector)
                || !has_raw_owner(session, &def_key.def_type, &def_key.def_name, *selector)
            {
                stats.skipped_not_replayable += 1;
                continue;
            }

            // The counterfactual's scope limit, enforced here because the pure
            // experiment cannot see it (see
            // `rim_merge::effective::counterfactual`'s "Scope"
            // paragraph): the fold holds the winner, its raw tree and
            // its template chain fixed, so moving a mod that is *also*
            // an owner of this def — past another owner — would change
            // which owner wins, and the prediction would stop describing
            // the order it recommends. Refused and counted rather than
            // answered approximately.
            if is_co_owner_with_another(
                session,
                &def_key.def_type,
                &def_key.def_name,
                *selector,
                mod_id,
            ) {
                stats.skipped_co_owner += 1;
                continue;
            }

            let def_slot = (
                def_key.def_type.clone(),
                def_key.def_name.clone(),
                *selector,
            );
            let used = subjects_per_def.entry(def_slot).or_insert(0);
            if *used >= MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF {
                stats.skipped_too_many_subjects += 1;
                continue;
            }
            if jobs.len() >= MAX_COUNTERFACTUAL_JOBS {
                stats.skipped_job_ceiling += 1;
                continue;
            }
            *used += 1;
            jobs.push(CounterfactualJob {
                finding_index,
                def_type: def_key.def_type.clone(),
                def_name: def_key.def_name.clone(),
                selector: *selector,
                mod_id: mod_id.clone(),
                operation: operation.clone(),
                was_dead_target: is_dead_target,
            });
        }
    }

    stats.jobs = jobs.len();
    (jobs, stats)
}

impl<Reader: DefSourceReader> VerifyOrder<Reader> {
    /// The post-reconciliation counterfactual phase: asks
    /// [`rim_merge::effective::counterfactual`] whether a different load
    /// order would have made each surviving `Unknown`- or
    /// `DeadTarget`-caused row's own operation succeed, and rewrites the
    /// cause in place when it would.
    ///
    /// Runs **after** the OR-head reconciliation, over surviving rows
    /// only, and re-reads each candidate def's own inputs
    /// (`def_owner_and_raw`, `template_chain`, `load_operation_texts`)
    /// rather than retaining thousands of `FieldTree`s across the main loop
    /// on the off-chance — a handful of small element reads per candidate
    /// def, against a pass that already performs tens of thousands of them.
    ///
    /// A fix maps onto the **existing** causes — `subject_loads_after`
    /// to `NotYetInjected` (whose suggestion is
    /// `Reorder { after: mod_id, before: injector }`), otherwise
    /// `RemovedBy` (`Reorder { after: remover, before: mod_id }`). No new
    /// `PatchFailureCause` variant: both variants' own doc comments
    /// already describe the *fact* ("an earlier mod's operation destroys
    /// the node", "a later mod injects the target"), never the evidence
    /// route, and this phase demonstrates that fact by replay instead of
    /// inferring it from a pair-deduped static edge.
    pub(super) fn run_counterfactual_phase(
        &self,
        session: &Session,
        env: &CounterfactualEnvironment<'_>,
        findings: &mut [Finding],
        progress_base: usize,
        on_progress: &mut dyn FnMut(usize, usize),
    ) -> CounterfactualStats {
        let (jobs, mut stats) = build_counterfactual_jobs(session, findings);
        let total_jobs = stats.jobs;
        if total_jobs == 0 {
            return stats;
        }

        // Grouped so each def's raw XML, template chain and operation
        // texts are read once however many subjects it contributes. The
        // key order is a `BTreeMap`'s, so the phase's own emission order
        // is deterministic like every other output here.
        let mut by_def: BTreeMap<(String, String, Selector), Vec<CounterfactualJob>> =
            BTreeMap::new();
        for job in jobs {
            by_def
                .entry((job.def_type.clone(), job.def_name.clone(), job.selector))
                .or_default()
                .push(job);
        }

        let mut done = 0usize;
        for ((def_type, def_name, selector), def_jobs) in by_def {
            on_progress(progress_base + done, progress_base + total_jobs);
            done += def_jobs.len();

            let Some(indexed) = session.sources().patch_ops_by_def.get(&(
                def_type.clone(),
                def_name.clone(),
                selector,
            )) else {
                stats.skipped_not_replayable += def_jobs.len();
                continue;
            };
            let top_level =
                active_top_level_operations(indexed, env.order, env.active_gate, env.name_map);
            let Ok((winner_id, raw)) = def_sources::def_owner_and_raw(
                &self.reader,
                session,
                env.order,
                &def_type,
                &def_name,
                selector,
            ) else {
                stats.skipped_not_replayable += def_jobs.len();
                continue;
            };
            let Ok(template_chain) = def_sources::template_chain(
                &self.reader,
                session,
                env.order,
                &def_type,
                &winner_id,
                raw.parent_name.as_deref(),
            ) else {
                stats.skipped_not_replayable += def_jobs.len();
                continue;
            };
            let Ok(op_texts) = def_sources::load_operation_texts(&self.reader, &top_level) else {
                stats.skipped_not_replayable += def_jobs.len();
                continue;
            };
            let contributions: Vec<PatchContribution<'_>> = op_texts
                .iter()
                .map(|(id, text)| PatchContribution {
                    mod_id: id,
                    operation_xml: text.as_str(),
                })
                .collect();

            // The first cap, and the K measurement that keeps it grounded.
            let blocks = effective::contribution_blocks(&contributions);
            *stats.mods_per_def.entry(blocks.len()).or_insert(0) += 1;
            if blocks.len() > MAX_COUNTERFACTUAL_MODS {
                stats.skipped_too_many_mods += def_jobs.len();
                continue;
            }

            let def_index = def_sources::LazyDefExists::new(session);
            let def_exists = |dt: &str, dn: &str| def_index.get(dt, dn);
            let input = EffectiveInput {
                winner: &winner_id,
                raw,
                contributions: &contributions,
                context: ReplayContext {
                    active_mods: env.replay.active_mods,
                    mod_names_by_display: env.replay.mod_names_by_display,
                    def_type: &def_type,
                    def_name: &def_name,
                    selector,
                    def_exists: &def_exists,
                    this_def_present: true,
                    behaviours: session.mod_knowledge().patch_operations(),
                },
                templates: &template_chain.set,
                template_owners: &template_chain.owners,
            };
            let baseline = effective::compute(input.clone());

            // Two rows can share one mod and one rendered identity (the
            // same physical operation examined under two def keys never
            // reaches here twice, but two distinct operations *can*
            // render alike); claiming each subject index at most once
            // keeps them from collapsing onto the same experiment.
            let mut claimed: BTreeSet<usize> = BTreeSet::new();
            for job in def_jobs {
                let subject = baseline
                    .top_level_outcomes
                    .iter()
                    .enumerate()
                    .find(|(index, outcome)| {
                        !outcome.succeeded
                            && outcome.mod_id == job.mod_id
                            && outcome.identity == job.operation
                            && !claimed.contains(index)
                    })
                    .map(|(index, _)| index);
                let Some(subject) = subject else {
                    stats.skipped_subject_not_found += 1;
                    continue;
                };
                claimed.insert(subject);

                let outcome = effective::counterfactual(&input, &baseline, subject);
                stats.attempts += outcome.attempts;
                stats.rejected_for_regression += outcome.rejected_for_regression;
                stats.rejected_for_truncation += outcome.rejected_for_truncation;
                stats.ties += outcome.ties;
                stats.refused_non_contiguous += usize::from(outcome.refused_non_contiguous);

                let Some(fix) = outcome.fix else {
                    continue;
                };
                let cause = if fix.subject_loads_after {
                    PatchFailureCause::NotYetInjected(fix.other)
                } else {
                    PatchFailureCause::RemovedBy(fix.other)
                };
                // A cosmetic fix names the existing `PatchCollision`
                // finding at this exact target, if there is one — read
                // *before* the mutable borrow below overwrites the row,
                // off the row's own `mod_id`/`leaf_xpath` (the same
                // `indexed`-matching `op_sub_path` uses for the
                // `DeadTarget` check above).
                let reorder_kind = if fix.final_def_unchanged {
                    let existing_merge = findings.get(job.finding_index).and_then(|finding| {
                        let Finding::PatchWillFail {
                            mod_id, leaf_xpath, ..
                        } = finding
                        else {
                            return None;
                        };
                        let sub_path = leaf_xpath
                            .as_deref()
                            .and_then(|xpath| op_sub_path(mod_id, xpath, indexed));
                        env.patch_collisions
                            .get(&def_type, &def_name, selector, sub_path.as_deref())
                            .cloned()
                    });
                    ReorderKind::Cosmetic { existing_merge }
                } else {
                    ReorderKind::Content
                };
                if let Some(Finding::PatchWillFail {
                    cause: stored_cause,
                    reorder_kind: stored_reorder_kind,
                    ..
                }) = findings.get_mut(job.finding_index)
                {
                    *stored_cause = cause;
                    *stored_reorder_kind = Some(reorder_kind);
                    if job.was_dead_target {
                        stats.demoted_dead_targets += 1;
                    } else {
                        stats.resolved += 1;
                    }
                }
            }
        }

        stats
    }
}
