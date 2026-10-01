//! A single, shared hash function for the "does this node path exist inline"
//! index (`extract::defs` builds it, `analysis::edges` queries it) — both
//! sides must hash identically for membership to mean anything, so this
//! exists once rather than being reimplemented at each call site.
//!
//! Stores a 64-bit hash, not the path text itself (on a real install, ~16MB
//! of hashes vs. ~120MB+ of owned `String`s). This set is in-process only and
//! never serialized, so hasher stability across Rust versions or process runs
//! is irrelevant — the one consequence that *does* matter is a hash
//! collision, which can only ever make [`hash_node_path`] return a value
//! another, unrelated path also hashes to, i.e. a **false positive** for
//! "this node pre-exists inline". That falsely demotes what should have been
//! a `Hard` `PatchInjectedNode` edge to advisory `UsesType` — the safe
//! direction (a missed, weaker edge, never a wrong, stronger one) — which is
//! exactly why the space trade is acceptable here.

use std::hash::{Hash, Hasher};

/// Hashes `path` (the same `"{def_type}/{def_name}/{relative_path}"` text
/// [`crate::domain::PatchOp::injected_paths`] and the injected-node edge-path
/// matching use) with the standard library's default hasher. Deterministic
/// *within one process run* (the only guarantee this index needs — see this
/// module's own doc comment) regardless of which `SipHash` parameters a given
/// Rust/std version happens to seed it with.
#[must_use]
pub fn hash_node_path(path: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_the_same_path_identically() {
        assert_eq!(
            hash_node_path("ThingDef/Wall/comps"),
            hash_node_path("ThingDef/Wall/comps")
        );
    }

    #[test]
    fn hashes_different_paths_differently() {
        // Not a mathematical guarantee (hash collisions are possible by
        // design — see this module's own doc comment), but these two
        // real-shaped paths must not collide in practice.
        assert_ne!(
            hash_node_path("ThingDef/Wall/comps"),
            hash_node_path("ThingDef/Door/comps")
        );
    }
}
