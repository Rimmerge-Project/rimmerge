//! Contribution blocks: which contributions form one replayable block, and duplicate-sibling
//! detection.

use rim_analyzer::domain::ModId;

use super::{EffectiveDef, Provenance};
use crate::patch_eval::PatchContribution;
use crate::tree::{FieldPath, ItemId, PathSegment};

/// For a `li` item at `path` whose identity fell back to
/// [`ItemId::Position`] because it collides with a sibling's own declared
/// identity (`tree::identify_all_li`'s own duplicate-identity fallback),
/// the earliest sibling — by position — directly under the same container
/// whose node is byte-identical to this one's, along with the mod credited
/// with it. Two contributors that independently declared the exact same
/// item have nothing to contest; a caller (`rim-session`'s conflict view)
/// uses this to fold such a pair into one agreed-on row instead of showing
/// what looks like a spurious duplicate (the "list case").
///
/// `None` for anything but an [`ItemId::Position`]-identified path, when no
/// earlier sibling matches, or when the matching sibling has no
/// attributable mod ([`Provenance::UnattributedTemplate`]).
#[must_use]
pub fn duplicate_of_earlier_sibling<'a>(
    effective: &'a EffectiveDef,
    path: &FieldPath,
) -> Option<(&'a FieldPath, &'a ModId)> {
    let segments = path.segments();
    let (last, parent) = segments.split_last()?;
    let PathSegment::Item(ItemId::Position(position)) = last else {
        return None;
    };
    let this_node = effective.resolved.get(path)?;

    let mut earliest: Option<(u32, &FieldPath, &ModId)> = None;
    for (candidate_path, candidate_provenance) in &effective.provenance {
        let candidate_segments = candidate_path.segments();
        let Some((candidate_last, candidate_parent)) = candidate_segments.split_last() else {
            continue;
        };
        let PathSegment::Item(ItemId::Position(candidate_position)) = candidate_last else {
            continue;
        };
        if candidate_position >= position || candidate_parent != parent {
            continue;
        }
        let Some(candidate_node) = effective.resolved.get(candidate_path) else {
            continue;
        };
        if candidate_node != this_node {
            continue;
        }
        let mod_id = match candidate_provenance {
            Provenance::Owner(id) | Provenance::Patch { mod_id: id, .. } => id,
            Provenance::Inherited { owner, .. } => owner,
            Provenance::UnattributedTemplate { .. } => continue,
        };
        if earliest.is_none_or(|(current, _, _)| *candidate_position < current) {
            earliest = Some((*candidate_position, candidate_path, mod_id));
        }
    }
    earliest.map(|(_, earliest_path, mod_id)| (earliest_path, mod_id))
}

/// One mod's contiguous run of contributions within
/// [`EffectiveInput::contributions`](super::EffectiveInput::contributions).
/// `rim_session::use_cases::def_sources::top_level_operations` groups every
/// top-level operation per mod (in that mod's own file order) and
/// re-interleaves the groups by load order, so each mod's contributions
/// always arrive as one contiguous block; this type names that block so
/// [`counterfactual`](fn@super::counterfactual) can move it as a unit and
/// so a caller can read `K` (the block count) off the same definition
/// rather than re-deriving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributionBlock {
    /// The mod that ships every contribution in this block.
    pub mod_id: ModId,
    /// Index of the block's first contribution.
    pub start: usize,
    /// How many contributions the block holds (never zero).
    pub len: usize,
}

/// Splits `contributions` into one [`ContributionBlock`] per contiguous run
/// of the same mod — the unit [`counterfactual`](fn@super::counterfactual)
/// re-permutes (a patcher is a mod, and the fix a user can actually apply
/// is a pair rule between two mods, never a reorder of one mod's own
/// operations).
///
/// A mod whose contributions are *not* contiguous would produce two
/// blocks naming it; [`counterfactual`](fn@super::counterfactual) refuses such an input rather than
/// guessing which run "the mod's block" means.
#[must_use]
pub fn contribution_blocks(contributions: &[PatchContribution<'_>]) -> Vec<ContributionBlock> {
    let mut blocks: Vec<ContributionBlock> = Vec::new();
    for (index, contribution) in contributions.iter().enumerate() {
        match blocks.last_mut() {
            Some(block) if &block.mod_id == contribution.mod_id => block.len += 1,
            _ => blocks.push(ContributionBlock {
                mod_id: contribution.mod_id.clone(),
                start: index,
                len: 1,
            }),
        }
    }
    blocks
}
