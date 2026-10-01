//! Counterfactual replay: which single contribution move would fix a failed replay.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;

use super::blocks::{ContributionBlock, contribution_blocks};
use super::{Completeness, EffectiveDef, EffectiveInput, Stopper, compute};
use crate::patch_eval::PatchContribution;
use crate::tree::FieldTree;

/// The load-order move [`counterfactual`] found: moving the failing
/// operation's own mod past `other` makes that operation succeed.
///
/// `subject_loads_after` is the direction the *subject's* mod must move
/// relative to `other` — `true` means the subject has to move **after**
/// `other` (so `other` is the mod whose work the subject depends on),
/// `false` that it has to move **before** it (so `other` is the mod that
/// destroys what the subject needs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterfactualFix {
    /// The decisive mod — the block the subject had to cross for the
    /// *minimal* successful move, never merely the nearest one.
    pub other: ModId,
    /// Whether the subject must load after [`Self::other`].
    pub subject_loads_after: bool,
    /// Whether the winning permutation's own [`EffectiveDef::resolved`]
    /// is identical to `baseline`'s — `true` means this fix changes only
    /// *which* operation's own failure RimWorld would log, never the
    /// final def itself (the same "only the op that logs `failed`
    /// differs" shape `rim_analyzer::domain::EdgeKind::PatchRemovedNodeCosmetic`
    /// already names for the static edge case — this is that same fact,
    /// proven by replay instead of inferred from an edge). A caller
    /// (`rim_session::use_cases::verify_order`) reads this to decide
    /// whether the fix is worth offering as a `set-pair` rule at all: a
    /// cosmetic one changes nothing a user would notice in-game.
    pub final_def_unchanged: bool,
}

/// What [`counterfactual`] learned about one failing top-level operation.
///
/// Every counter is reported rather than silently folded away: a caller
/// (`rim_session::use_cases::verify_order`) sums them across a real
/// install so the experiment's own cost and refusal rate are measurable,
/// which is the whole reason a move is rejected instead of quietly
/// preferred.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterfactualOutcome {
    /// The strictly-better move, if any exists. `None` means every
    /// alternative either left the subject failing, broke another
    /// operation, or truncated the fold — the cause stays whatever it
    /// already was.
    pub fix: Option<CounterfactualFix>,
    /// How many alternative block positions were actually replayed
    /// (`K - 1` for a well-formed input, `0` when this function refused
    /// the input outright).
    pub attempts: usize,
    /// Alternatives that fixed the subject but broke an operation that
    /// succeeded in the baseline.
    pub rejected_for_regression: usize,
    /// Alternatives whose fold stopped at or before the subject, so the
    /// subject's own success could never be observed — never read as a
    /// success.
    pub rejected_for_truncation: usize,
    /// How many successful moves tied the winner's distance in the other
    /// direction (resolved later-move-first, deterministically, and
    /// counted so a real-install run can report how often it happens).
    pub ties: usize,
    /// Whether the input was refused outright because some mod's
    /// contributions are not contiguous (see [`contribution_blocks`]) —
    /// reported so a caller can tell that refusal apart from the far
    /// commoner "only one block, nothing to permute" case, which also
    /// yields `attempts: 0`.
    pub refused_non_contiguous: bool,
}

/// Asks the one question a static edge scan keeps getting wrong on real
/// installs: **would a different load order have made this operation
/// work?**
///
/// `input` is the same [`EffectiveInput`] [`compute`] was given and
/// `baseline` the [`EffectiveDef`] it produced; `subject` is the index
/// into `input.contributions` of the top-level operation whose
/// [`patch_eval::TopLevelOutcome::succeeded`](crate::patch_eval::TopLevelOutcome::succeeded)
/// is `false`. The subject's whole mod block is moved to every other
/// block position, one replay each, and an alternative counts as a fix
/// **only** when it is strictly better: the subject itself succeeds, the fold
/// reaches it, and no contribution that succeeded in `baseline` stops
/// succeeding. Attribution names the block the subject had to cross for the
/// *smallest* successful move — every shorter move failed, so that block is
/// the decisive one — with an equal-distance tie resolved later-move-first
/// and counted.
///
/// Pure and deterministic: no IO, no index, no randomness, and the
/// enumeration is `0..K` in order, so the same input always yields the
/// same outcome. The cost is `K - 1` calls to [`compute`], each of which
/// replays every contribution once — `(K - 1) * contributions.len()`
/// calls to [`patch_eval::replay`](crate::patch_eval::replay), **not**
/// `(K - 1) * K`, since a block can hold more than one operation — still
/// exactly `K - 1` even when a fix is found: [`CounterfactualFix::final_def_unchanged`]
/// is read off a resolved tree already produced by one of those `K - 1`
/// calls (kept alongside every successful alternative's own target
/// position while the search runs), never a fresh `compute` call of its
/// own for the winner. The caller is the one that must cap `K`.
///
/// **Scope, stated because the result is a load-order recommendation and
/// this function cannot check it**: the experiment holds
/// [`EffectiveInput::winner`], [`EffectiveInput::raw`] and
/// [`EffectiveInput::templates`] fixed, so it answers "would this
/// operation have worked under a different *patch* order", never "what
/// would the def look like if a different mod owned it". If the subject's
/// own mod is also an owner of this def alongside the winner, the reorder
/// this function recommends could change which owner wins, and the
/// prediction would no longer describe the order it recommends. Callers
/// must exclude such a subject themselves — `rim_session`'s own
/// `VerifyOrder` does, counting it as `skipped_co_owner`.
///
/// Refuses the input (returning a default outcome, `attempts: 0`) when
/// there is nothing to experiment on or the experiment would be
/// ill-defined: fewer than two blocks, a `subject` outside
/// `contributions`, a baseline that never reached the subject or in which
/// the subject *succeeded*, or **any** mod whose contributions are not
/// contiguous (see [`contribution_blocks`]; that last refusal is reported
/// as [`CounterfactualOutcome::refused_non_contiguous`]).
#[must_use]
pub fn counterfactual(
    input: &EffectiveInput<'_>,
    baseline: &EffectiveDef,
    subject: usize,
) -> CounterfactualOutcome {
    let mut outcome = CounterfactualOutcome::default();

    // The subject must be a genuinely-observed failure of this exact
    // baseline — otherwise "strictly better" has no meaning to measure
    // against.
    match baseline.top_level_outcomes.get(subject) {
        Some(subject_outcome) if !subject_outcome.succeeded => {}
        _ => return outcome,
    }

    let blocks = contribution_blocks(input.contributions);
    if blocks.len() < 2 {
        return outcome;
    }
    let Some(subject_block) = blocks
        .iter()
        .position(|block| subject >= block.start && subject < block.start + block.len)
    else {
        return outcome;
    };
    // The contiguity premise, checked rather than assumed, for
    // *every* mod and not just the subject's: two blocks naming one mod
    // make "move the mod's block" ambiguous for the subject, and make
    // "the decisive block at `q`" ambiguous for the attribution — a move
    // could name a mod whose other run sits on the far side of the
    // subject, which is not a relation any pair rule can express.
    let mut seen: BTreeSet<&ModId> = BTreeSet::new();
    if !blocks.iter().all(|block| seen.insert(&block.mod_id)) {
        outcome.refused_non_contiguous = true;
        return outcome;
    }

    // Successful alternatives, as original block positions paired with
    // that alternative's own resolved tree — captured here, not
    // recomputed for the winner afterward, so this function's own `(K -
    // 1)` `compute` calls stay exactly that even when a fix is found
    // (see this function's own "Cost" paragraph). Enumeration order
    // (`0..K` skipping `subject_block`) so the tie-break below reads off
    // a deterministic list.
    let mut successes: Vec<(usize, FieldTree)> = Vec::new();

    for target in 0..blocks.len() {
        if target == subject_block {
            continue;
        }
        outcome.attempts += 1;

        let permutation = permuted_contribution_indices(&blocks, subject_block, target);
        let mut position_of: Vec<usize> = vec![0; permutation.len()];
        for (new_position, &original) in permutation.iter().enumerate() {
            position_of[original] = new_position;
        }
        let contributions: Vec<PatchContribution<'_>> = permutation
            .iter()
            .map(|&original| input.contributions[original])
            .collect();

        let alternative = compute(EffectiveInput {
            winner: input.winner,
            raw: input.raw.clone(),
            contributions: &contributions,
            context: input.context,
            templates: input.templates,
            template_owners: input.template_owners,
        });

        let subject_position = position_of[subject];
        // A move that merely pushes the subject past a stopper must
        // never read as success. Both checks are deliberate — the length
        // check is what actually proves the subject was reached, and the
        // `Completeness` check states the rule directly, for the day a
        // stopped contribution starts emitting an outcome of its own.
        if alternative.top_level_outcomes.len() <= subject_position {
            outcome.rejected_for_truncation += 1;
            continue;
        }
        if let Completeness::Partial {
            stopped_at: Stopper::Replay { op_index, .. },
        } = &alternative.completeness
            && *op_index <= subject_position
        {
            outcome.rejected_for_truncation += 1;
            continue;
        }
        // The index-alignment invariant `position_of` exists to keep,
        // asserted rather than assumed — `compute` emits exactly one
        // `TopLevelOutcome` per contribution it reaches, in the order it
        // was given them, so the outcome at `subject_position` must be
        // the subject's own mod's. A mismatch means the alternative fold
        // and the permutation have disagreed, and reading a *different*
        // operation's success as the subject's would be the worst
        // possible failure mode here: silently confident, and wrong.
        let subject_outcome = &alternative.top_level_outcomes[subject_position];
        debug_assert_eq!(
            &subject_outcome.mod_id, input.contributions[subject].mod_id,
            "permutation/top_level_outcomes order mismatch"
        );
        if &subject_outcome.mod_id != input.contributions[subject].mod_id {
            outcome.rejected_for_truncation += 1;
            continue;
        }
        if !subject_outcome.succeeded {
            continue;
        }

        // Strictly better, mapped through the permutation rather
        // than compared positionally. A baseline success the alternative
        // either fails or never reaches counts as a regression — "no
        // longer observed to succeed" is not evidence of anything better.
        let regressed = baseline
            .top_level_outcomes
            .iter()
            .enumerate()
            .filter(|(original, base)| *original != subject && base.succeeded)
            .any(|(original, _)| {
                alternative
                    .top_level_outcomes
                    .get(position_of[original])
                    .is_none_or(|moved| !moved.succeeded)
            });
        if regressed {
            outcome.rejected_for_regression += 1;
            continue;
        }

        successes.push((target, alternative.resolved));
    }

    // Minimal move wins; on an equal-distance tie the later move
    // (the subject loading *after* the decisive block) wins, and the tie
    // is counted.
    let Some((best, best_resolved)) = successes.iter().min_by_key(|(target, _)| {
        (
            target.abs_diff(subject_block),
            usize::from(*target < subject_block),
        )
    }) else {
        return outcome;
    };
    let best_distance = best.abs_diff(subject_block);
    outcome.ties = successes
        .iter()
        .filter(|(target, _)| target.abs_diff(subject_block) == best_distance)
        .count()
        .saturating_sub(1);
    outcome.fix = Some(CounterfactualFix {
        other: blocks[*best].mod_id.clone(),
        subject_loads_after: *best > subject_block,
        final_def_unchanged: *best_resolved == baseline.resolved,
    });
    outcome
}

/// The contribution indices of `blocks` with the block at `from` moved to
/// position `to`, every other block's relative order untouched.
///
/// Removing `from` and inserting at `to` is what gives the minimal-move
/// rule its attribution: for `to > from` the moved block lands immediately
/// **after** the block originally at `to` (the ones at `from + 1 ..= to`
/// were all crossed); for `to < from` it lands immediately **before** it.
fn permuted_contribution_indices(
    blocks: &[ContributionBlock],
    from: usize,
    to: usize,
) -> Vec<usize> {
    let mut order: Vec<&ContributionBlock> = blocks.iter().collect();
    let moved = order.remove(from);
    order.insert(to, moved);
    order
        .into_iter()
        .flat_map(|block| block.start..block.start + block.len)
        .collect()
}
