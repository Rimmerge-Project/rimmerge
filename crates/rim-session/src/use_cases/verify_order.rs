//! [`VerifyOrder`]: the on-demand patch-replay pass over a load order.
//!
//! Runs the same replay [`crate::use_cases::InspectDef`] drives per def —
//! `rim_merge::effective::compute`, never a second replay path — for
//! every def with at least one *active* patcher, and turns each `rim_merge::plan::Caveat::FailedOp` it produces
//! into a [`rim_resolve::domain::Finding::PatchWillFail`], classified by
//! cause.
//!
//! **Gating, applied before predicting anything**: an op nested inside an
//! unsatisfied `PatchOperationFindMod`/`MayRequire` never runs at all, so
//! it can never fail, so it must never be predicted as failing — the same
//! `rim_analyzer::analysis::indices::patch_op_active` gate the analyzer's
//! own edge producers already apply. Without it, predictions outnumber the
//! failures a real game log shows by orders of magnitude.
//! [`active_top_level_operations`] is the one place this pass ever asks
//! `def_sources::top_level_operations` for a def's own patchers; every
//! other use (the zero-owner fast path, what gets replayed, `classify_cause`'s own `other_patchers`) reads *only* its
//! result, never the raw, gating-unaware list.
//!
//! **Zero-owner means zero owners, not zero *raw* owners**:
//! `has_any_owner` consults both `session.sources().owners_by_def` (a
//! literal `Defs/**/*.xml` def) and `.injected_def_owners` (a whole-def
//! `<xpath>Defs</xpath>` patch injection, common on real installs). Such
//! a def has no owner in `Indices.def_owners`, and treating it as
//! zero-owner would wrongly fast-path it to `DeadTarget`. An injected-only
//! def is real, so it must not hit the fast path — but it also has no
//! raw source [`def_sources::def_owner_and_raw`] can read (RimWorld
//! synthesizes it dynamically, never as its own file on disk), so
//! `has_raw_owner` gates a *second*, narrower check right before the
//! replay attempt: an injected-only def with an active foreign patcher
//! is honestly [`VerifyOrderReport::skipped`], never guessed at.
//!
//! **One OR-xpath is one query, not N**: RimWorld evaluates a patch xpath
//! once across
//! the whole document, so an OR-ed defName head with a narrowing root
//! predicate is meant to hit whichever defs qualify — `patch_ops_by_def`
//! indexes it once per named def, but replaying it independently per def
//! key must not manufacture a failure out of one query that merely
//! didn't match *some* of the defs it names. [`execute_with_progress`]
//! buffers every top-level operation's outcome, across every def key it
//! is examined under (both the zero-owner and replayed paths) in
//! `pending`, keyed by [`top_level_op_key`] — see that function's own
//! doc comment for the aggregation key. The rule: one observed success
//! suppresses the whole group; a def
//! key never examined (injected-only, truncated by a stopper) is never
//! read as either success or failure. A def only its own mod patches is
//! examined too: an OR-ed head can name it, and its success is the
//! evidence that clears the others' failures.
//!
//! Explicit, on-demand only (never part of `Session::compute`): the CLI
//! `verify` command and the desktop apply dialog call
//! [`VerifyOrder::execute`] directly, never through a cached/ledger path.

use std::collections::BTreeMap;

use rim_analyzer::analysis::indices::ActiveMods;
use rim_analyzer::domain::ModId;

use crate::Session;
use crate::ports::DefSourceReader;
use counterfactual::CounterfactualEnvironment;
use replay_def::{DefTarget, PassEnvironment, PassTally};
use report::{EdgeEvidence, PatchCollisionIndex};

mod counterfactual;
mod predict;
mod replay_def;
mod report;

pub use counterfactual::CounterfactualStats;
pub(super) use predict::{
    ReplayEnvironment, active_top_level_operations, build_gate_name_map, has_any_owner,
    has_raw_owner, zero_owner_outcomes,
};
pub use report::{ReorderConflict, ReorderConflictDirection, VerifyOrderReport, reorder_conflicts};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "verify_order/verify_order_tests.rs"]
mod tests;

/// What [`VerifyOrder`] should do beyond the base pass.
///
/// Exists so `apps/cli`'s own `--no-counterfactual` can produce
/// before/after numbers from **one** binary — the desktop command
/// deliberately gets no flag: an order-fixable cause is exactly what its
/// apply dialog is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyOptions {
    /// Whether to run the post-reconciliation counterfactual phase.
    /// Default `true`.
    pub counterfactual: bool,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            counterfactual: true,
        }
    }
}

/// Runs the patch-replay verification pass.
pub struct VerifyOrder<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> VerifyOrder<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Checks every def with at least one *active* patcher
    /// (`patch_collision` targets plus `patch_targets_def` targets, gated
    /// — see this module's own doc comment) against `source`'s own order, and reports every predicted
    /// patch failure found. Never errors outright — a def this pass can't
    /// check at all is recorded in [`VerifyOrderReport::skipped`] instead
    /// of aborting the whole pass.
    #[must_use]
    pub fn execute(
        &self,
        session: &Session,
        source: rim_resolve::domain::OrderSource,
    ) -> VerifyOrderReport {
        self.execute_with_progress(session, source, &mut |_checked, _total| {})
    }

    /// [`Self::execute`] with an explicit [`VerifyOptions`] and no
    /// progress reporting — `apps/cli`'s own `--no-counterfactual`
    /// is the one caller that needs it, so before/after numbers
    /// come from a single binary.
    #[must_use]
    pub fn execute_with_options(
        &self,
        session: &Session,
        source: rim_resolve::domain::OrderSource,
        options: VerifyOptions,
    ) -> VerifyOrderReport {
        self.execute_with_progress_and_options(session, source, options, &mut |_checked, _total| {})
    }

    /// Same as [`Self::execute`], but calls `on_progress(checked, total)`
    /// as work starts on each unit of work. `apps/desktop`'s own
    /// `verify_order` command is the one real
    /// caller: a real-install pass runs for about a minute on a large
    /// install with no feedback at all, which reads as a frozen dialog —
    /// the same problem [`super::LoadProject`]'s own `on_progress`
    /// (`ScanProgress`) already solves for the initial scan.
    ///
    /// **Two phases, and `total` grows once, mid-stream**:
    ///
    /// 1. **Candidate keys.** One call per candidate key as it starts
    ///    being examined, reported as `(index, defs)` where `defs` is the
    ///    full candidate-key count (every key this pass will visit,
    ///    including the ones the gating/zero-owner/injected-only branches
    ///    skip trivially without incrementing
    ///    [`VerifyOrderReport::defs_checked`]).
    /// 2. **Counterfactual jobs.** Once the surviving rows have been
    ///    reconciled and the job count `J` is known, one call per job as
    ///    it starts, reported as `(defs + done, defs + J)`.
    ///
    /// So `checked` is still strictly non-decreasing across the whole
    /// pass and the last call is still exactly `(total, total)` — but
    /// `total` itself changes value exactly once, from `defs` to
    /// `defs + J`, at the phase boundary. A consumer must recompute its
    /// own percentage from **both** numbers on every call rather than
    /// caching the first `total` it is handed. When `J` is zero (the
    /// counterfactual phase is off, or nothing survived for it to do)
    /// `total` never changes.
    #[must_use]
    pub fn execute_with_progress(
        &self,
        session: &Session,
        source: rim_resolve::domain::OrderSource,
        on_progress: &mut dyn FnMut(usize, usize),
    ) -> VerifyOrderReport {
        self.execute_with_progress_and_options(
            session,
            source,
            VerifyOptions::default(),
            on_progress,
        )
    }

    /// [`Self::execute_with_progress`] with the counterfactual phase
    /// switchable — the one real implementation the three wrappers above
    /// delegate to.
    #[must_use]
    pub fn execute_with_progress_and_options(
        &self,
        session: &Session,
        source: rim_resolve::domain::OrderSource,
        options: VerifyOptions,
        on_progress: &mut dyn FnMut(usize, usize),
    ) -> VerifyOrderReport {
        let order = session.orders().get(source).clone();
        let edge_evidence = EdgeEvidence::build(session);
        let patch_collisions = PatchCollisionIndex::build(session);

        // Built once, not per def: the same gate the analyzer's own edge producers
        // already apply (`patch_op_active`) — an op inside an unsatisfied
        // `PatchOperationFindMod`/`MayRequire` never runs, so it can
        // never fail. `active_mods_gate`/`name_map` feed the gate check
        // itself; `active_mods`/`mod_names_by_display` are the unrelated,
        // already-established pair `ReplayContext` needs for its own
        // internal `PatchOperationFindMod` replay (ordinary `BTreeSet`/
        // `BTreeMap`, per this crate's own determinism convention — see
        // `patch_op_active`'s own `DisplayNameIndex` requirement below for
        // why that one pair is a distinct newtype instead).
        //
        // **`name_map` must go through [`build_gate_name_map`]**:
        // `indices::gate_open` looks up every `<match>`/`<nomatch>` name
        // lowercased, so a bare verbatim-keyed `.collect()` would never
        // match anything — `AnyActive` gates would always read closed,
        // `NoneActive` gates always open, and every op nested under a
        // `PatchOperationFindMod` would be silently excluded from this
        // whole pass (roughly a quarter of all indexed ops on a real
        // install). `DisplayNameIndex` makes the verbatim-`.collect()`
        // mistake a compile error — see that type's own doc comment.
        let active_mods_gate = ActiveMods::from_ids(session.active_base_ids());
        let name_map = build_gate_name_map(&session.report().mods);
        let active_mods = session.active_base_ids();
        let mod_names_by_display: BTreeMap<String, ModId> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.name.clone(), m.id.clone()))
            .collect();

        let pass = PassEnvironment {
            session,
            order: &order,
            edge_evidence: &edge_evidence,
            replay: ReplayEnvironment {
                active_mods: &active_mods,
                mod_names_by_display: &mod_names_by_display,
            },
        };
        let mut tally = PassTally::default();
        // Iterates `patch_ops_by_def` directly rather than collecting
        // every key into an owned `Vec` and re-looking each one up by a
        // freshly cloned tuple: `indexed` comes straight from the
        // iterator, so there is no lookup to fail, no panic path in
        // non-test code, and no redundant `String` clones per pass.
        let total_keys = session.sources().patch_ops_by_def.len();
        for (index, ((def_type, def_name, selector), indexed)) in
            session.sources().patch_ops_by_def.iter().enumerate()
        {
            on_progress(index, total_keys);
            let top_level =
                active_top_level_operations(indexed, &order, &active_mods_gate, &name_map);
            // Every op on this def sits under a gate that is closed in this
            // order (an unsatisfied `FindMod`/`MayRequire`): nothing runs, so
            // nothing can fail, and reading and folding the def's sources
            // would only cost time and count a def as "checked" that no
            // operation was checked against.
            if top_level.is_empty() {
                continue;
            }
            let target = DefTarget {
                def_type,
                def_name,
                selector: *selector,
                indexed,
                top_level: &top_level,
            };

            if !has_any_owner(session, def_type, def_name, *selector) {
                self.replay_zero_owner(&pass, &target, &mut tally);
                continue;
            }

            if !has_raw_owner(session, def_type, def_name, *selector) {
                // Real, but injected-only: the def genuinely exists
                // (`has_any_owner` said so, via `injected_def_owners`), so
                // it must not be predicted `DeadTarget` — but it also has
                // no raw `Defs/**/*.xml` source anywhere `def_owner_and_raw`
                // could read, since RimWorld synthesizes it dynamically
                // from the injecting mod's own `<value>` content. Replaying
                // a foreign patcher's op against content this pass can't
                // even read would be guessing, not predicting — honestly
                // skipped instead, same as any other candidate this pass
                // can't check (`VerifyOrderReport::skipped`'s doc comment).
                tally.skipped.push((
                    target.def_key(),
                    *selector,
                    format!("{def_type}/{def_name}: owned only by a patch injection (no raw Defs/ source) — not replayable this pass"),
                ));
                continue;
            }

            self.replay_owned_def(&pass, &target, &mut tally);
        }

        // Reconciled after every def key has been examined. An operation
        // that succeeded under at least one examined def key is suppressed
        // outright — positive evidence only: a def key never reached above
        // simply has no entry here and is never read as either success or
        // failure. One that failed under every def key it was examined
        // under emits every one of those rows (emission stays per def key
        // — aggregation removes whole groups, it never merges rows).
        // `pending` is a `BTreeMap` keyed `(ModId, Arc<Path>, u32, String)`,
        // so this iterates — and therefore emits — in that deterministic
        // key order.
        let mut findings = Vec::new();
        for outcomes in std::mem::take(&mut tally.pending).into_values() {
            if outcomes.iter().any(Option::is_none) {
                continue;
            }
            findings.extend(outcomes.into_iter().flatten());
        }

        // The two-phase progress contract: phase 1 reported
        // `(index, total_keys)`; now that the job count `J` is known,
        // phase 2 reports `(total_keys + done, total_keys + J)`. `total`
        // grows exactly once, mid-stream — `VerifyProgressEventDto` is
        // unchanged, and `ApplyDialog.vue` recomputes its own percentage
        // from both fields.
        let counterfactual = if options.counterfactual {
            self.run_counterfactual_phase(
                session,
                &CounterfactualEnvironment {
                    order: &order,
                    replay: &ReplayEnvironment {
                        active_mods: &active_mods,
                        mod_names_by_display: &mod_names_by_display,
                    },
                    active_gate: &active_mods_gate,
                    name_map: &name_map,
                    patch_collisions: &patch_collisions,
                },
                &mut findings,
                total_keys,
                on_progress,
            )
        } else {
            CounterfactualStats::default()
        };

        let total = total_keys + counterfactual.jobs;
        on_progress(total, total);

        VerifyOrderReport {
            source,
            findings,
            defs_checked: tally.defs_checked,
            skipped: tally.skipped,
            counterfactual,
            suppressed_filter_head_ops: tally.suppressed_filter_head_ops,
        }
    }
}
