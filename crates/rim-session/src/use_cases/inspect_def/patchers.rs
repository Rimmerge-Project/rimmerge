//! Each patcher's replay outcome and the caveats that concern it.

use rim_analyzer::domain::ModId;
use rim_merge::effective::{self};
use rim_merge::plan::Caveat;

/// Derives one patcher's own `(replay, reached)` pair
/// from the full
/// fold's own [`effective::Completeness`] — never a second, standalone
/// replay. `own_indices` are `mod_id`'s own contribution positions in
/// [`EffectiveInput::contributions`](rim_merge::effective::EffectiveInput::contributions), in load order; every mod's own
/// indices are contiguous there (`def_sources::top_level_operations`
/// groups per mod), so comparing just the first is enough to place the
/// whole span relative to a `Replay` stopper's single `op_index`.
pub(super) fn patcher_replay_outcome(
    mod_id: &ModId,
    own_indices: &[usize],
    completeness: &effective::Completeness,
) -> (Result<(), String>, bool) {
    let effective::Completeness::Partial {
        stopped_at:
            effective::Stopper::Replay {
                mod_id: stopper_mod,
                op_index,
                error,
            },
    } = completeness
    else {
        // `Complete`, or a `Partial { stopped_at: Stopper::Inherit(_) }`
        // — either way the patch stage itself ran to completion (an
        // `Inherit` stopper is only ever recorded when it did), so every
        // patcher's own ops were reached and replayed without error.
        return (Ok(()), true);
    };
    if stopper_mod == mod_id {
        return (Err(error.to_string()), true);
    }
    match own_indices.first() {
        Some(&first) if first < *op_index => (Ok(()), true),
        _ => (Ok(()), false),
    }
}

/// The mod one [`Caveat`] concerns, for the variants
/// [`rim_merge::patch_eval::replay`] ever actually produces
/// (`ModSettingDefault`/`FailedOp`/`MalformedOperation`/`DefRemoved`) —
/// `None` for every other
/// variant (raised elsewhere, by `rim_merge::plan`'s own diff/merge
/// machinery, never by a replay). Exhaustive so a new `Caveat` variant
/// is a compile error here rather than a silently dropped
/// [`Patcher::caveats`](crate::use_cases::inspect_def::inspection::Patcher::caveats) entry.
pub(super) fn caveat_mod_id(caveat: &Caveat) -> Option<ModId> {
    match caveat {
        Caveat::ModSettingDefault { mod_id, .. }
        | Caveat::FailedOp { mod_id, .. }
        | Caveat::UnscopedOps { mod_id, .. }
        | Caveat::MalformedOperation { mod_id }
        | Caveat::DefRemoved { mod_id }
        | Caveat::UnknownOwnerChoice { mod_id, .. } => Some(mod_id.clone()),
        Caveat::DuplicateTemplate { .. }
        | Caveat::PositionalItem { .. }
        | Caveat::UnsafeXpathValue { .. }
        | Caveat::InvalidValueFragment { .. }
        | Caveat::UnsupportedDrop { .. }
        | Caveat::UnreconstructableChain { .. }
        | Caveat::UnsettableLeaf { .. }
        | Caveat::OutOfScopeOwners { .. }
        // Raised by `rim_merge::plan`'s own merge machinery (a later
        // patch's wholesale replace of a keyed map dropping an earlier
        // mod's independent key-add), never by `rim_merge::patch_eval::replay`
        // itself — same bucket as `UnsupportedDrop`/`OutOfScopeOwners`
 // above.
        | Caveat::ClobberedMapEntry { .. } => None,
    }
}
