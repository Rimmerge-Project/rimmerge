//! Replaying one def key: the part of
//! [`super::VerifyOrder::execute_with_progress_and_options`] that folds a
//! def's patchers over its winner's XML (or, for a def no mod owns, over a
//! synthetic empty placeholder) and buffers each top-level operation's
//! outcome.
//!
//! Split out of the main loop so each path reads as one function.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::IndexedPatchOp;
use rim_analyzer::domain::{LoadOrder, ModId, Selector, XmlLocator};
use rim_merge::effective::{self, Completeness, EffectiveInput};
use rim_merge::patch_eval::{PatchContribution, ReplayContext};
use rim_resolve::domain::{DefKey, Finding};

use super::VerifyOrder;
use super::counterfactual::{TopLevelOpKey, top_level_op_key};
use super::predict::{ReplayEnvironment, zero_owner_outcomes};
use super::report::{EdgeEvidence, classify_cause, reorder_kind_for_cause};
use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources;

/// The def key under examination, with its gated top-level operations.
pub(super) struct DefTarget<'a> {
    pub(super) def_type: &'a str,
    pub(super) def_name: &'a str,
    pub(super) selector: Selector,
    pub(super) indexed: &'a [IndexedPatchOp],
    pub(super) top_level: &'a [(ModId, XmlLocator)],
}

impl DefTarget<'_> {
    pub(super) fn def_key(&self) -> DefKey {
        DefKey {
            def_type: self.def_type.to_string(),
            def_name: self.def_name.to_string(),
        }
    }
}

/// What never varies between the def keys of one pass.
pub(super) struct PassEnvironment<'a> {
    pub(super) session: &'a Session,
    pub(super) order: &'a LoadOrder,
    pub(super) edge_evidence: &'a EdgeEvidence,
    pub(super) replay: ReplayEnvironment<'a>,
}

/// What a pass accumulates across def keys.
#[derive(Default)]
pub(super) struct PassTally {
    /// Buffered per top-level operation, across every def key it is
    /// examined under; `None` records an examined-and-succeeded outcome,
    /// `Some` the finding an examined-and-failed one would produce.
    /// A `BTreeMap` so the reconciliation emits in a deterministic order.
    pub(super) pending: BTreeMap<TopLevelOpKey, Vec<Option<Finding>>>,
    pub(super) skipped: Vec<(DefKey, Selector, String)>,
    pub(super) defs_checked: usize,
    pub(super) suppressed_filter_head_ops: usize,
}

impl<Reader: DefSourceReader> VerifyOrder<Reader> {
    /// Replays `target`, which has a raw owner, against the winner's XML
    /// and buffers one outcome per top-level operation reached. Every
    /// def with an active patcher is replayed, including one only its own
    /// winner patches: an OR-ed head names defs of several mods and one
    /// query succeeds when any of them matches, so a def the winner alone
    /// patches can be the alternative that clears an op's failure on the
    /// others.
    pub(super) fn replay_owned_def(
        &self,
        env: &PassEnvironment<'_>,
        target: &DefTarget<'_>,
        tally: &mut PassTally,
    ) {
        let def_key = target.def_key();
        let (winner_id, raw) = match def_sources::def_owner_and_raw(
            &self.reader,
            env.session,
            env.order,
            target.def_type,
            target.def_name,
            target.selector,
        ) {
            Ok(v) => v,
            Err(error) => {
                tally
                    .skipped
                    .push((def_key, target.selector, error.to_string()));
                return;
            }
        };

        let template_chain = match def_sources::template_chain(
            &self.reader,
            env.session,
            env.order,
            target.def_type,
            &winner_id,
            raw.parent_name.as_deref(),
        ) {
            Ok(v) => v,
            Err(error) => {
                tally
                    .skipped
                    .push((def_key, target.selector, error.to_string()));
                return;
            }
        };

        let op_texts = match def_sources::load_operation_texts(&self.reader, target.top_level) {
            Ok(v) => v,
            Err(error) => {
                tally
                    .skipped
                    .push((def_key, target.selector, error.to_string()));
                return;
            }
        };
        let contributions: Vec<PatchContribution<'_>> = op_texts
            .iter()
            .map(|(id, text)| PatchContribution {
                mod_id: id,
                operation_xml: text.as_str(),
            })
            .collect();

        let def_index = def_sources::LazyDefExists::new(env.session);
        let def_exists = |dt: &str, dn: &str| def_index.get(dt, dn);
        let context = ReplayContext {
            active_mods: env.replay.active_mods,
            mod_names_by_display: env.replay.mod_names_by_display,
            def_type: target.def_type,
            def_name: target.def_name,
            selector: target.selector,
            def_exists: &def_exists,
            this_def_present: true,
            behaviours: env.session.mod_knowledge().patch_operations(),
        };

        let effective = effective::compute(EffectiveInput {
            winner: &winner_id,
            raw,
            contributions: &contributions,
            context,
            templates: &template_chain.set,
            template_owners: &template_chain.owners,
        });

        // A `Stopper::Replay`/`Stopper::Inherit` truncation is recorded:
        // a def whose fold stopped partway through never silently counts
        // as fully checked (`VerifyOrderReport::skipped`'s contract). The
        // outcomes reached *before* the stopper are still genuine
        // predictions and are buffered below.
        if let Completeness::Partial { stopped_at } = &effective.completeness {
            tally.skipped.push((
                def_key.clone(),
                target.selector,
                format!(
                    "{}/{}: replay stopped — {stopped_at}",
                    target.def_type, target.def_name
                ),
            ));
        }

        // Purely a measurement — see
        // `VerifyOrderReport::suppressed_filter_head_ops`.
        tally.suppressed_filter_head_ops += effective.suppressed_filter_head_ops;

        let other_patchers: BTreeSet<ModId> = target
            .top_level
            .iter()
            .map(|(mod_id, _)| mod_id.clone())
            .collect();

        // Success-suppression and top-level granularity: one buffered
        // outcome per top-level operation, at RimWorld's own
        // `PatchOperation.Apply()` boundary — never per leaf
        // `Caveat::FailedOp`, which over-predicts through a
        // `<success>Always` idiom swallowing a leaf's failure and through
        // a multi-defName OR-list head fanning one op into many caveats.
        //
        // A length-checked zip against `top_level`:
        // `effective.top_level_outcomes` is one entry per contribution
        // `compute` reached, in the order `top_level` fed it, truncated
        // like the `Completeness::Partial` push above. `zip` stops at the
        // shorter side, so an op not reached under this def key is simply
        // absent from `pending`, never guessed at.
        for ((mod_id, locator), outcome) in target
            .top_level
            .iter()
            .zip(effective.top_level_outcomes.iter())
        {
            debug_assert_eq!(
                mod_id, &outcome.mod_id,
                "top_level/top_level_outcomes order mismatch"
            );
            let finding = if outcome.succeeded {
                None
            } else {
                // The structurally-tracked leaf `identity`'s own
                // `lastFailedOperation=` is built from, never re-derived
                // from `outcome.caveats`, which can disagree with the
                // real failing leaf (a `<success>Always>` sibling that
                // matched nothing still leaves a caveat; a bare
                // `PatchOperationTest` failure leaves none).
                let cause = match outcome.failed_leaf_xpath.as_deref() {
                    Some(xpath) => classify_cause(
                        &outcome.mod_id,
                        xpath,
                        target.indexed,
                        &other_patchers,
                        env.order,
                        env.edge_evidence,
                        &effective.resolved,
                    ),
                    // A failing leaf with no `<xpath>` at all (a bare
                    // `PatchOperationFindMod`, rare): nothing to
                    // classify from.
                    None => rim_resolve::domain::PatchFailureCause::Unknown,
                };
                let reorder_kind = reorder_kind_for_cause(&cause);
                Some(Finding::PatchWillFail {
                    mod_id: outcome.mod_id.clone(),
                    def_key: def_key.clone(),
                    selector: target.selector,
                    operation: outcome.identity.clone(),
                    leaf_xpath: outcome.failed_leaf_xpath.clone(),
                    cause,
                    reorder_kind,
                })
            };
            tally
                .pending
                .entry(top_level_op_key(mod_id, locator, target.indexed))
                .or_default()
                .push(finding);
        }

        tally.defs_checked += 1;
    }

    /// The zero-owner fast path: `target` names a def with zero owners
    /// anywhere, so there is no winner's XML to replay against, but the
    /// *outcome* ("matches nothing") is certain regardless of content. The
    /// gated top-level operations are replayed against a synthetic, empty
    /// `<{def_type}></{def_type}>` placeholder rather than hand-summarized,
    /// so the same success-suppression and top-level-granularity rules
    /// apply as on a genuinely owned def: a `<success>Always` compat patch
    /// aimed at content its author expects might not exist is exactly this
    /// path's shape, and must not be predicted a failure either. A
    /// `PatchOperationConditional`'s own test against the placeholder is
    /// sound too: with truly zero content every "does X exist" check is
    /// false, so it takes the `nomatch` branch, as the real game would.
    ///
    /// Gated: `target.top_level` is already active-only, so a compat-patch
    /// shape ("if ModB is active, add to ModB's def" with ModB not
    /// installed) never reaches here at all.
    pub(super) fn replay_zero_owner(
        &self,
        env: &PassEnvironment<'_>,
        target: &DefTarget<'_>,
        tally: &mut PassTally,
    ) {
        let def_key = target.def_key();
        let replay_outcome = match zero_owner_outcomes(
            &self.reader,
            env.session,
            target.def_type,
            target.def_name,
            target.selector,
            target.top_level,
            &env.replay,
        ) {
            Ok(replay_outcome) => replay_outcome,
            Err(error) => {
                tally
                    .skipped
                    .push((def_key, target.selector, error.to_string()));
                return;
            }
        };
        // The suppressed-prediction count, zero-owner half: this path runs
        // its own `patch_eval::replay`, so its suppressions are summed
        // here too; the owned path reads them off `EffectiveDef`.
        tally.suppressed_filter_head_ops += replay_outcome.suppressed_filter_head_ops;
        // A malformed or unsupported operation stops `replay` early:
        // `top_level_outcomes` then only covers the contributions reached
        // before that, like the owned path's `Completeness::Partial`.
        // Recorded, never silently treated as a fully-checked def.
        if let Some(error) = &replay_outcome.error {
            tally.skipped.push((
                def_key.clone(),
                target.selector,
                format!(
                    "{}/{}: replay stopped — {error}",
                    target.def_type, target.def_name
                ),
            ));
        }
        // A length-checked zip against `top_level`; see the owned path.
        for ((mod_id, locator), outcome) in target
            .top_level
            .iter()
            .zip(replay_outcome.top_level_outcomes.iter())
        {
            debug_assert_eq!(
                mod_id, &outcome.mod_id,
                "top_level/top_level_outcomes order mismatch"
            );
            let finding = if outcome.succeeded {
                None
            } else {
                Some(Finding::PatchWillFail {
                    mod_id: outcome.mod_id.clone(),
                    def_key: def_key.clone(),
                    selector: target.selector,
                    operation: outcome.identity.clone(),
                    // The structurally-tracked leaf `identity`'s own
                    // `lastFailedOperation=` is built from; see
                    // `TopLevelOutcome::failed_leaf_xpath`.
                    leaf_xpath: outcome.failed_leaf_xpath.clone(),
                    cause: rim_resolve::domain::PatchFailureCause::DeadTarget,
                    // `DeadTarget` offers no reorder at all.
                    reorder_kind: None,
                })
            };
            tally
                .pending
                .entry(top_level_op_key(mod_id, locator, target.indexed))
                .or_default()
                .push(finding);
        }
        tally.defs_checked += 1;
    }
}
