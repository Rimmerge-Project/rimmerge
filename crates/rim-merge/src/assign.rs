//! The assignment engine's tree-facing half. [`read_instance`] flattens
//! one assignment instance's [`FieldTree`](crate::tree::FieldTree) into
//! `rim_resolve::domain::InstanceValues`; [`render_rows`] turns one
//! confirmed [`Section`](rim_resolve::domain::Section) back into `Defs/`
//! XML, and [`render_sections`] does the same for every section of a
//! multi-section project at once; [`dependencies`] computes the hard
//! `modDependencies` set.
//! Pure, like the rest of this crate: no filesystem, no def-name index
//! type crosses this module's boundary — every lookup a caller's own
//! index (`rim_analyzer::analysis::source_index::SourceIndex::defs_by_name`,
//! and its DLL-ownership rule) can answer arrives as a closure.
//!
//! **The field classification rules live in `rim-resolve`, not here.**
//! `rim-resolve`'s `AssignmentSchema::infer_fields` needs no
//! `FieldTree`/XML at all — only flat, `FieldPath`-keyed values — so the
//! whole five-rule
//! classification is hosted there, pure and directly testable against
//! hand-built instance sets. [`infer`] here is a thin adapter: it calls
//! `AssignmentSchema::infer_fields` and wraps the result with the def
//! type and an empty `target_shapes` (populated separately, by a caller
//! that reads referenced targets' resolved trees and calls
//! `TargetShape::infer` — session-layer IO this pure engine doesn't do).
//! Do not re-implement the classification rules here; extend
//! `rim-resolve` instead, exactly like `read_instance`
//! extends `rim-resolve`'s `InstanceValues`/`FieldOccurrence` shape
//! rather than inventing a parallel one. `read_instance` also never
//! records a `defName` leaf as a field of its own:
//! an instance's identity is always rendered from
//! `AssignmentRow::def_name`, never a schema field, so recording it
//! would let inference misclassify these instances' own names as a
//! spurious reference field whenever they happen to double as
//! resolvable `defName`s of the same type.
//!
//! [`dependencies`] computes the "hard `modDependencies` on every item's
//! owner" rule as its own pure, independently testable step. **It does
//! not implement the `loadAfter` = that set ∪ T nuance on its own** —
//! [`load_after_only`] is that other half, unioned by the caller.
//!
//! [`render_rows`] never approximates a field it can't safely render: a
//! field path with a list-item segment anywhere among its ancestors, an
//! `ItemSlot` field whose `Cardinality::Scalar` value carries more than
//! one name, or any other role/value combination that doesn't match (e.g.
//! a stored row's value from before its field's
//! role was reclassified, reachable through `AssignmentProject::from_stored`,
//! which trusts a stored row completely) is skipped and recorded as a
//! [`SkippedField`] instead of guessing — the caller folds these into a
//! later export step's own `skipped` list, the same shape `ExportPatch`
//! already uses for a row that fails validation at render time. A section
//! with nothing left to render (every row skipped, e.g. a row with no
//! gate decision) gets no `Defs/` file
//! at all, mirroring `emit::render`'s own no-blocks-no-file convention
//! for `Patches/`.
//!
//! A project is a set of `Section`s (one per def type), each
//! independently target-keyed (`RowKey::Target`) or free-standing.
//! [`render_rows`] covers both: it takes one
//! [`Section`](rim_resolve::domain::Section) and dispatches per row on its
//! own [`RowKey`](rim_resolve::domain::RowKey), since a
//! `Section`'s rows are always homogeneous by construction
//! (`Section::is_standalone`) but nothing stops a hand-built or
//! `from_stored`-trusted one from mixing — the dispatch is genuinely
//! per-row, not a section-wide assumption. [`render_sections`] renders
//! every section of a project (typically `AssignmentProject::sections()`)
//! into its own `Defs/<DefType>.xml` file, in `BTreeMap` order.
//! [`dependencies`]/[`standalone_dependencies`] both take an
//! `own_package_id` parameter: an `ItemSlot` value naming *this project's
//! own* free-standing row resolves, per the caller's own index, to either
//! this project's own package id or nothing at all — either way it is
//! never a real external dependency, so both functions drop it from the
//! owner set the same way they already drop Core.
//!
//! [`render_rows`]/[`render_sections`] run no own-instance "dangling
//! reference" guard. See [`render_rows`]'s own doc comment for why: that
//! check belongs to `rim-session`'s `ExportAssignment::render_defs` (the
//! sole real caller), which runs `validate_and_clean_sections` first.

mod mod_dependencies;
mod read;
mod render;

pub use mod_dependencies::{dependencies, load_after_only, standalone_dependencies};
pub use read::{infer, read_instance};
pub use render::{SkippedField, TargetGate, render_rows, render_sections};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "assign/assign_tests.rs"]
mod tests;
