//! The patch maker's domain: an [`AssignmentSchema`] learned (then
//! confirmed) from a reference framework's own def instances, a
//! [`TargetShape`] telling a real target apart from every other def of
//! the same type, an [`AssignmentProject`] holding both plus the user's
//! per-target [`AssignmentRow`]s, a [`PrecedenceRule`] table for the one
//! framework verified against real source, and pure [`coverage`] over an
//! already-read active list. See `docs/concepts/merge.md` for the patch
//! maker as a whole; this module is exactly its domain layer — no
//! filesystem, no XML, no `rim-merge` tree-walking. Read
//! [`AssignmentSchema::infer_fields`]'s doc comment, including the
//! real-install cases its rules encode, before touching its five rules.
//!
//! Split across `schema.rs` (`AssignmentSchema`/`FieldRole`/inference),
//! `shape.rs` (`TargetShape`), `project.rs`
//! (`AssignmentId`/`AssignmentProject`), `section.rs` (`RowKey`/`Section`),
//! `precedence.rs` (`PrecedenceRule`), and `coverage.rs` (`coverage`) —
//! this file only re-exports the public shape; the shared test suite
//! lives in `assignment/assignment_tests.rs`. The public names and their
//! paths from outside this module are the re-exports below.
//!
//! **Why inference lives here and not in `rim-merge`** (the engine): the
//! classification algorithm itself (the five rules) needs no
//! tree or XML at all — only flat per-field value lists keyed by
//! [`FieldPath`] (already a `rim-resolve` type) and a `ModId` per
//! instance — so it is hosted here instead, as
//! [`AssignmentSchema::infer_fields`], pure and directly testable against
//! hand-built instance sets with no engine dependency. `rim-merge`'s own
//! `assign::read_instance` is a thin
//! tree-to-[`InstanceValues`] flattener feeding this function, and the
//! crate graph stays one-directional (`rim-merge` depends on
//! `rim-resolve`, never the reverse). [`TargetShape`]'s own inference
//! is kept separate from field inference for the same
//! reason in the other direction: computing it needs each *referenced
//! target's* resolved tree, which is IO the session layer reads —
//! [`TargetShape::infer`] takes the already-extracted top-level child-tag
//! sets, so the read stays in the engine/session layer and the threshold
//! arithmetic stays here, pure and testable.
//!
//! Likewise, [`AssignmentProject`] does **not**
//! implement `set_scope`-shaped re-inference on an R change
//! or row-dropping on a T change (`SchemaChange`/a `ScopeChange`-shaped
//! result) — both need live report data (re-running inference, or
//! knowing which mod owns a target def) this pure crate has no way to
//! read. That behaviour belongs to `UpdateAssignment` (`rim-session`);
//! [`AssignmentProject::set_refs`]/
//! [`AssignmentProject::set_targets`] here are plain setters with no
//! reporting, exactly like [`super::patch::PatchProject::set_identity`]'s
//! own "just a setter" siblings.

mod coverage;
mod precedence;
mod project;
mod schema;
mod section;
mod shape;

// Only the sibling test suite (`assignment/assignment_tests.rs`, through
// `use super::*`) needs these directly — the real implementation lives in
// the submodules above, each with its own imports. Gated so a non-test
// build never sees them as unused.
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use rim_analyzer::domain::{LoadOrder, ModId};

#[cfg(test)]
use super::finding::DefKey;
#[cfg(test)]
use super::merge::{FieldPath, PathSegment};
#[cfg(test)]
use super::patch::PatchModIdentity;

pub use coverage::{Coverage, CoverageRow, ExistingInstance, RowIntent, coverage};
pub use precedence::{ExistingMatch, PrecedenceRule, Winner, top_level_field};
pub use project::{
    AssignmentId, AssignmentIdParseError, AssignmentProject, AssignmentRow, AssignmentRowError,
    KnownDefs, RowValue, SectionError, StoredAssignmentProject, TargetRef,
};
pub use schema::{
    AssignmentSchema, Cardinality, ENUM_MAX_DISTINCT, FieldAlreadyExistsError, FieldOccurrence,
    FieldRole, FieldSpec, INSIDE_MIN, InstanceValues, MIN_RESOLVED_DISTINCT, ScalarKind,
    TYPE_COVERAGE_MIN, UnknownFieldError, majority_owner, reconstruct_candidate_type,
};
// Measurement-only diagnostics (the Def-suffix exemption's blast radius)
// — re-exported only under the same `cfg` the type itself is defined
// behind.
#[cfg(any(test, feature = "test-support"))]
pub use schema::ReferenceFieldDiagnostics;
pub use section::{RowKey, Section};
pub use shape::{SHAPE_MIN, TargetShape};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "assignment/assignment_tests.rs"]
mod tests;
