//! [`Constraint`]: a load-order requirement satisfiable by any of several
//! mods, rather than by one specific mod.
//!
//! An `AssemblyRef` naming an assembly shipped by more than one active
//! mod can't be pinned to a single "must load after" edge — any one of
//! the shipping mods could be the one that actually satisfies it at
//! runtime. Modeling that as one edge per candidate produced contradictory
//! per-candidate edges; a single any-of constraint says what's actually
//! true instead.

use serde::{Deserialize, Serialize};

use super::mod_id::ModId;

/// Whether the current load order satisfies a [`Constraint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintStatus {
    Satisfied,
    Violated,
}

/// A load-order requirement satisfied when *any one* of several candidate
/// mods loads before `after`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// `after` ships a DLL referencing `assembly`, an assembly name more
    /// than one active mod ships — `candidates` lists every owner, unless
    /// a designated provider among them could be inferred from the
    /// install's own declarations
    /// (`analysis::edges::assembly::designated_provider`), in which case
    /// it lists just that one owner.
    AnyOf {
        after: ModId,
        assembly: String,
        candidates: Vec<ModId>,
        /// Whether the reference is resolved at DLL-load time (see
        /// [`super::AssemblyReference`]) rather than lazily.
        load_time: bool,
        status: ConstraintStatus,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_of_constraint_serializes_with_its_variant_tag() {
        let constraint = Constraint::AnyOf {
            after: ModId::new("a"),
            assembly: "shared.dll".to_string(),
            candidates: vec![ModId::new("b"), ModId::new("c")],
            load_time: true,
            status: ConstraintStatus::Satisfied,
        };
        let json = serde_json::to_value(&constraint).expect("must serialize");
        assert_eq!(json["kind"], "any_of");
        assert_eq!(json["status"], "satisfied");
    }
}
