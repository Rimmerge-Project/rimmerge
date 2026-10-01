//! Detects file-level collisions between active mods: overriding defs,
//! colliding patch targets, overriding textures, duplicate assemblies,
//! and likely duplicate/forked mods.

use crate::domain::{LoadOrder, ModId};

// Keeps `super::patch_op_targets` valid for the children.
use super::patch_op_targets;

mod assemblies;
mod assets;
mod defs;
mod duplicate_mods;
mod patches;
mod textures;

pub use assemblies::{duplicate_assemblies, runtime_patch_collisions, transpiler_collisions};
pub use assets::{duplicate_template_names, keyed_translation_collisions, sound_overrides};
pub use defs::def_overrides;
pub use duplicate_mods::likely_duplicate_mods;
pub use patches::patch_collisions;
pub use textures::{missing_texture_paths, texture_overrides, undecodable_textures};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "conflicts/conflicts_tests.rs"]
mod tests;

/// `pub(super)`: [`super::mod_cost`] reuses this exact ordering rule for
/// `overridden_texture_bytes` rather than duplicating it.
pub(super) fn sorted_by_load_order(owners: &[ModId], load_order: &LoadOrder) -> Vec<ModId> {
    let mut sorted = owners.to_vec();
    sorted.sort_by_key(|id| load_order.position(id).unwrap_or(usize::MAX));
    sorted
}
