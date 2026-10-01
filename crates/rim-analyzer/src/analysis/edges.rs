//! Builds every [`Edge`] between active mods: hard assembly-reference
//! dependencies, author-declared ordering, and soft "awareness" evidence.

// Keeps `super::patch_op_targets` valid for the children.
use super::patch_op_targets;

mod assembly;
mod declared;
mod defs;
mod manifest;
mod names;
mod patches;
mod textures;

pub use assembly::*;
pub use declared::*;
pub use defs::*;
pub use manifest::*;
pub use names::*;
pub use patches::*;
pub use textures::*;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "edges/edges_tests.rs"]
mod tests;
