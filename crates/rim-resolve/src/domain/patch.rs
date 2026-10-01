//! A compatibility patch: a named, user-identified subset of the load
//! order (a [`PatchScope`]) with its own [`DecisionSet`](crate::domain::decision::DecisionSet), independent of
//! the profile's. This module is exactly its domain layer — [`PatchId`],
//! [`PatchModIdentity`], [`PatchScope`] and its per-[`FindingKey`](crate::domain::finding::FindingKey)-variant
//! membership rule, and [`PatchProject`] itself. No filesystem, no XML:
//! persistence (`rim-io`) and the scoped participant/owner restriction
//! rules `rim-merge`/`rim-session` apply during planning live in those
//! crates.

mod identity;
mod project;
mod scope;

pub use identity::{PatchId, PatchIdParseError, PatchIdentityError, PatchModIdentity};
pub use project::{
    PatchDecisionError, PatchProject, ScopeChange, StoredPatchProject, UnpatchableAction,
};
pub use scope::{PatchScope, PatchScopeError, ScopeMembership};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "patch/patch_tests.rs"]
mod tests;
