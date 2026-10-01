//! DTOs for the patch maker (assignment) commands
//! `list_assignments`,
//! `get_assignment`, `list_assignment_candidates`,
//! `infer_assignment_candidate`, `create_assignment`,
//! `update_assignment`, `set_assignment_row`, `clear_assignment_row`,
//! `delete_assignment`, `get_assignment_coverage`,
//! `list_assignment_items`, `copy_assignment_row_from`,
//! `export_assignment`, `add_assignment_section`,
//! `remove_assignment_section`. `CreateAssignment` is two-phase
//! (`crates/rim-session/CLAUDE.md`), so there is no separate propose
//! command. Reuses [`super::patch::ModRefDto`] for every
//! `ModId` list (refs/targets/dependencies) — the same `{modId, name}`
//! shape a patch's own mod lists use.
//!
//! **Two deliberate narrowings of the command surface:**
//! - `get_assignment_coverage` takes no filter: nothing in this codebase
//!   (not even `apps/cli`'s own `assign coverage`, which prints the whole
//!   queue unfiltered) filters a [`Coverage`] result server-side, so
//!   adding one here would be interface-layer business logic with no
//!   precedent — the editor's own search/target-mod/key-type filters run
//!   client-side over the full (bounded by `|T|`) result instead.
//! - A field's role can only be reclassified in the wizard, before
//!   `create_assignment` ever runs (the client mutates its own draft
//!   [`AssignmentSchemaDto`] locally — [`FieldRole::confirm_role`]/
//!   [`AssignmentSchema::add_field`](rim_resolve::domain::AssignmentSchema::add_field)/[`AssignmentSchema::remove_field`](rim_resolve::domain::AssignmentSchema::remove_field)
//!   are pure enough to mirror in TypeScript). `update_assignment` has no
//!   schema-editing field of its own because
//!   [`rim_session::use_cases::UpdateAssignment`] doesn't either — an R
//!   change re-infers automatically and reports a [`SchemaChangeDto`];
//!   there is no session-layer use case for reclassifying a field on an
//!   already-created project.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{AssignmentProject, Coverage, FieldPath, RowIntent, RowKey};
use rim_session::use_cases::{AssignmentCandidate, SchemaChange};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::patch::ModRefDto;
use crate::error::format_row_key;
use rows::section_dto;

// Keeps `super::common` valid for the children.
use super::common;

mod coverage;
mod rows;
mod schema;

pub use coverage::*;
pub use rows::*;
pub use schema::*;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "assignment/assignment_tests.rs"]
mod tests;

/// One row of `list_assignments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentSummaryDto {
    /// The project's stable id.
    pub id: String,
    /// The project's label in lists.
    pub name: String,
    /// The published mod's package id.
    pub package_id: String,
    /// The published mod's display name.
    pub display_name: String,
    /// Every section's own def type, e.g. `example.PartAssignmentDef` — one per
    /// section, in `BTreeMap` order (a project can carry more than one).
    pub def_types: Vec<String>,
    /// The selected reference set (before the dependency closure).
    pub refs: Vec<ModRefDto>,
    /// The target set T.
    pub targets: Vec<ModRefDto>,
    /// How many rows this project has across every section.
    pub row_count: usize,
    /// Whether this project has no target-keyed section at all — every
    /// section is free-standing ("new def"), or it has none yet. See
    /// [`Self::uncovered_count`]'s own doc comment: the two are always
    /// consistent (`is_standalone == uncovered_count.is_none()`), so the
    /// frontend never has to infer one from the other.
    pub is_standalone: bool,
    /// Across every target-keyed section: how many of its own candidate
    /// defs neither this project nor any other active mod currently
    /// covers ([`RowIntent::Cover`] with no row of this project's own).
    /// `None` when this project has no target-keyed section at all —
    /// coverage isn't applicable (see [`CoverageDto::applicable`]), never
    /// expressible as `0` (which would misread as "fully covered").
    pub uncovered_count: Option<usize>,
    /// The last folder this project was exported to, if any.
    pub export_dir: Option<String>,
    /// When this project was last changed.
    pub updated_at: String,
}

/// `get_assignment`'s full detail: [`AssignmentSummaryDto`]'s own fields
/// plus the identity/section fields a list row doesn't need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentDetailDto {
    /// See [`AssignmentSummaryDto::id`].
    pub id: String,
    /// See [`AssignmentSummaryDto::name`].
    pub name: String,
    /// See [`AssignmentSummaryDto::package_id`].
    pub package_id: String,
    /// See [`AssignmentSummaryDto::display_name`].
    pub display_name: String,
    /// See [`AssignmentSummaryDto::refs`].
    pub refs: Vec<ModRefDto>,
    /// Closure members the user opted out of.
    pub excluded_refs: Vec<ModRefDto>,
    /// See [`AssignmentSummaryDto::targets`].
    pub targets: Vec<ModRefDto>,
    /// See [`AssignmentSummaryDto::export_dir`].
    pub export_dir: Option<String>,
    /// See [`AssignmentSummaryDto::updated_at`].
    pub updated_at: String,
    /// The `About.xml` `<author>` this project will export.
    pub author: String,
    /// The `About.xml` `<description>` this project will export.
    pub description: String,
    /// The derived folder name this project renders into.
    pub folder_name: String,
    /// Every section this project carries, in `BTreeMap` (def type) order
    /// — one tab per section in the editor.
    pub sections: Vec<AssignmentSectionDto>,
    /// When this project was created.
    pub created_at: String,
}

/// Builds an [`AssignmentSummaryDto`] from a loaded [`AssignmentProject`]
/// and one coverage work queue per target-keyed section, keyed by def
/// type (already computed by the caller — building it fresh per summary
/// would mean an O(n) coverage rebuild for every row of
/// `list_assignments`, and the session already caches it per project/def
/// type). A def type present in `project.sections()` but absent from
/// `coverages` is read as "not yet computed", not "zero" — every
/// target-keyed section's caller must supply one.
#[must_use]
pub fn assignment_summary_dto(
    project: &AssignmentProject,
    coverages: &BTreeMap<String, Coverage>,
    names: &BTreeMap<ModId, String>,
) -> AssignmentSummaryDto {
    let has_target_keyed_section = project.sections().values().any(|s| !s.is_standalone());
    AssignmentSummaryDto {
        id: project.id().to_string(),
        name: project.name().to_string(),
        package_id: project.identity().package_id().as_str().to_string(),
        display_name: project.identity().display_name().to_string(),
        def_types: project.sections().keys().cloned().collect(),
        refs: mod_ref_dtos(project.refs(), names),
        targets: mod_ref_dtos(project.targets(), names),
        row_count: project.sections().values().map(|s| s.rows.len()).sum(),
        is_standalone: !has_target_keyed_section,
        // `None` (not `0`) once this project has no target-keyed section
        // at all: `AssignmentListPage.vue` must render "n/a", never a
        // misleadingly "fully covered" `0`.
        uncovered_count: has_target_keyed_section
            .then(|| coverages.values().map(uncovered_rows).sum()),
        export_dir: project.export_dir().map(|dir| dir.display().to_string()),
        updated_at: project.updated_at().to_string(),
    }
}

/// How many coverage rows are still genuinely uncovered: no existing
/// instance references the target ([`RowIntent::Cover`]) *and* this
/// project hasn't made a row for it either — an `Override` row (some
/// other active mod already covers it) is never counted, even without a
/// row of this project's own.
#[must_use]
fn uncovered_rows(coverage: &Coverage) -> usize {
    coverage
        .rows
        .iter()
        .filter(|row| matches!(row.intent, RowIntent::Cover) && !row.has_row)
        .count()
}

/// Builds an [`AssignmentDetailDto`] from a loaded [`AssignmentProject`].
#[must_use]
pub fn assignment_detail_dto(
    project: &AssignmentProject,
    names: &BTreeMap<ModId, String>,
) -> AssignmentDetailDto {
    AssignmentDetailDto {
        id: project.id().to_string(),
        name: project.name().to_string(),
        package_id: project.identity().package_id().as_str().to_string(),
        display_name: project.identity().display_name().to_string(),
        refs: mod_ref_dtos(project.refs(), names),
        excluded_refs: mod_ref_dtos(project.excluded_refs(), names),
        targets: mod_ref_dtos(project.targets(), names),
        export_dir: project.export_dir().map(|dir| dir.display().to_string()),
        updated_at: project.updated_at().to_string(),
        author: project.author().to_string(),
        description: project.description().to_string(),
        folder_name: project.identity().folder_name().to_string(),
        sections: project
            .sections()
            .iter()
            .map(|(def_type, section)| section_dto(def_type, section))
            .collect(),
        created_at: project.created_at().to_string(),
    }
}

// -- list_assignment_candidates / infer_assignment_candidate -------------

/// Request shape for `list_assignment_candidates` — the wizard's own
/// phase-1 step, cheap and instance-read-free
/// (see [`rim_session::use_cases::CreateAssignment::list_candidates`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListAssignmentCandidatesRequestDto {
    /// The selected reference set (before the dependency closure).
    pub refs: Vec<String>,
    /// Closure members the user has already opted out of.
    pub excluded_refs: Vec<String>,
    /// The target set T (accepted, not yet read at this phase — only
    /// `infer_assignment_candidate` reads it, to filter/gate a
    /// `TargetKey` field's own shape sample and to admit a no-`TargetKey`
    /// candidate).
    pub targets: Vec<String>,
}

/// Mirrors [`rim_session::use_cases::CandidateSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CandidateSummaryDto {
    /// The candidate def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// How many active instances of this type exist across the whole
    /// install.
    pub instance_count: usize,
    /// Every mod that owns at least one instance of this type.
    pub owners: Vec<ModRefDto>,
}

/// What `list_assignment_candidates` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListAssignmentCandidatesResponseDto {
    /// The selected refs plus their declared-dependency closure (Core/DLC
    /// excluded), minus `excludedRefs` — shown in the wizard as "plus X
    /// (dependency of Y)" (without attributing *which* Y; see this
    /// module's own doc comment).
    pub effective_refs: Vec<ModRefDto>,
    /// Every assignment-def candidate's cheap summary.
    pub candidates: Vec<CandidateSummaryDto>,
}

/// Request shape for `infer_assignment_candidate` — the wizard's own
/// phase-2 step, for exactly one candidate the user picked from phase 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct InferAssignmentCandidateRequestDto {
    /// The selected reference set (before the dependency closure).
    pub refs: Vec<String>,
    /// Closure members the user has already opted out of.
    pub excluded_refs: Vec<String>,
    /// The target set T. May be empty only for a candidate with no
    /// `TargetKey` field (a "new def" candidate).
    pub targets: Vec<String>,
    /// Which candidate (from `list_assignment_candidates`'s own response)
    /// to infer.
    pub def_type: String,
}

/// One field a candidate's own target-shape inference couldn't read back.
/// Mirrors `UnreadableTarget` (a private type re-exported only
/// structurally through [`AssignmentCandidate::unreadable_targets`] — see
/// that field's own doc comment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct UnreadableTargetDto {
    /// The `TargetKey` field whose sample this target would have joined.
    pub field: String,
    /// The target's own def type.
    pub def_type: String,
    /// The target's own `defName`.
    pub def_name: String,
    /// Why the read failed.
    pub reason: String,
}

/// One referenced target `infer_assignment_candidate` never even tried
/// to read, because its own owner isn't a member of T or Core. Mirrors
/// `ExcludedTarget` (a private type re-exported only structurally through
/// [`AssignmentCandidate::excluded_targets`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExcludedTargetDto {
    /// The `TargetKey` field whose sample this target would have joined.
    pub field: String,
    /// The target's own def type.
    pub def_type: String,
    /// The target's own `defName`.
    pub def_name: String,
}

/// Mirrors [`AssignmentCandidate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentCandidateDto {
    /// The candidate def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// The inferred schema, `target_shapes` included.
    pub schema: AssignmentSchemaDto,
    /// Referenced targets excluded from a `TargetKey` field's own shape
    /// sample because they couldn't be read back.
    pub unreadable_targets: Vec<UnreadableTargetDto>,
    /// Referenced targets never even attempted, because their own owner
    /// isn't a member of T or Core.
    pub excluded_targets: Vec<ExcludedTargetDto>,
}

impl From<&AssignmentCandidate> for AssignmentCandidateDto {
    fn from(value: &AssignmentCandidate) -> Self {
        Self {
            def_type: value.def_type.clone(),
            schema: (&value.schema).into(),
            unreadable_targets: value
                .unreadable_targets
                .iter()
                .map(|target| UnreadableTargetDto {
                    field: target.field.to_string(),
                    def_type: target.def_type.clone(),
                    def_name: target.def_name.clone(),
                    reason: target.reason.clone(),
                })
                .collect(),
            excluded_targets: value
                .excluded_targets
                .iter()
                .map(|target| ExcludedTargetDto {
                    field: target.field.to_string(),
                    def_type: target.def_type.clone(),
                    def_name: target.def_name.clone(),
                })
                .collect(),
        }
    }
}

// -- create_assignment ---------------------------------------------------

/// Request shape for `create_assignment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CreateAssignmentRequestDto {
    /// The project's label in lists.
    pub name: String,
    /// The published mod's package id.
    pub package_id: String,
    /// The published mod's display name.
    pub display_name: String,
    /// The selected reference set (before the dependency closure).
    pub refs: Vec<String>,
    /// Closure members the user opted out of.
    pub excluded_refs: Vec<String>,
    /// The target set T.
    pub targets: Vec<String>,
    /// The user-confirmed schema, from `infer_assignment_candidate`.
    pub schema: AssignmentSchemaDto,
}

// -- update_assignment ---------------------------------------------------

/// Request shape for `update_assignment`. `None` on any field means
/// "leave unchanged"; `refs`/`excludedRefs` are re-inferred together
/// whenever either is given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct UpdateAssignmentRequestDto {
    /// Which project to update.
    pub assignment_id: String,
    /// A new list label.
    pub name: Option<String>,
    /// A new `About.xml` author.
    pub author: Option<String>,
    /// A new `About.xml` description.
    pub description: Option<String>,
    /// A new selected reference set.
    pub refs: Option<Vec<String>>,
    /// A new set of closure members opted out.
    pub excluded_refs: Option<Vec<String>>,
    /// A new target set.
    pub targets: Option<Vec<String>>,
}

/// Mirrors [`SchemaChange`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SchemaChangeDto {
    /// Fields the fresh inference observes that the schema didn't have before.
    pub added: Vec<String>,
    /// Fields the schema had that the fresh inference no longer observes at all.
    pub removed: Vec<String>,
    /// Fields present both before and after whose stored role actually changed.
    pub reclassified: Vec<String>,
}

impl From<&SchemaChange> for SchemaChangeDto {
    fn from(value: &SchemaChange) -> Self {
        Self {
            added: value.added.iter().map(ToString::to_string).collect(),
            removed: value.removed.iter().map(ToString::to_string).collect(),
            reclassified: value.reclassified.iter().map(ToString::to_string).collect(),
        }
    }
}

/// One row value [`UpdateAssignment::execute`](rim_session::use_cases::UpdateAssignment::execute)
/// stranded — `(section def type, row, field path)` — because a
/// `refs`/`excludedRefs` update reclassified the field's role out from
/// under the value's own stored shape. Shares [`SectionReferenceDto`](crate::error::SectionReferenceDto)'s
/// exact `{defType, row, path}` shape (both transcribe a `(String,
/// RowKey, FieldPath)` triple the identical human-readable way, via
/// `error::format_row_key`) but stays its own type: the two mean
/// different things — one names a row still referencing a section that's
/// about to be removed, this one names a row whose own value was just
/// dropped — and `error.rs`'s own DTO has no business being constructed
/// from this module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StrandedValueDto {
    /// The section (by def type) the stranded value belonged to.
    pub def_type: String,
    /// The row's own key: a target-keyed row's target def, a
    /// free-standing row's own bare `defName`.
    pub row: String,
    /// The field, by canonical path text, whose stored value no longer
    /// fits its (reclassified) role.
    pub path: String,
}

/// Builds one [`StrandedValueDto`] from an
/// [`UpdateAssignmentOutcome::stranded_values`](rim_session::use_cases::UpdateAssignmentOutcome::stranded_values)
/// entry — a free function (not a `From` impl: the source is a bare
/// tuple, not a named type this crate owns) so `update_assignment_inner`'s
/// own mapping is one call, not an inline closure, and so it has a name a
/// unit test can call directly without a full session/IO fixture.
pub(crate) fn stranded_value_dto(
    def_type: &str,
    key: &RowKey,
    path: &FieldPath,
) -> StrandedValueDto {
    StrandedValueDto {
        def_type: def_type.to_string(),
        row: format_row_key(key),
        path: path.to_string(),
    }
}

/// One row dropped by a `targets` shrink — mirrors one
/// `UpdateAssignmentOutcome::dropped_rows` entry (a bare `(String,
/// TargetRef)` tuple), keeping which section's own row it was, the same
/// way [`StrandedValueDto::def_type`] already does for a stranded value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DroppedRowDto {
    /// The section (by def type) the row belonged to.
    pub def_type: String,
    /// The target whose row was dropped.
    pub target: TargetRefDto,
}

/// What `update_assignment` returns: the project's fresh detail, the
/// schema change every re-inferred section's own update made, every row
/// value that update stranded, and every row dropped by a `targets`
/// shrink.
///
/// **`schema_changes`/`dropped_rows` are keyed/tagged per section, not a
/// single `Option<SchemaChangeDto>`/bare `Vec<TargetRefDto>`**: on a
/// multi-section project, `UpdateAssignment::execute` re-infers *every*
/// section whenever `refs`/`excludedRefs` is part of the update
/// (`rim_session::use_cases::UpdateAssignmentOutcome::schema_changes` is
/// a `BTreeMap<String, SchemaChange>`, one entry per section whose own
/// diff is non-empty), so a single field taking only
/// `outcome.schema_changes.values().next()` would silently drop every
/// section past the first one a `BTreeMap` iterator yields.
/// `dropped_rows` carries its section the same way:
/// `UpdateAssignmentOutcome::dropped_rows` is `Vec<(String, TargetRef)>`
/// (which section a dropped target's row belonged to), the same pairs
/// `apps/cli`'s own `assign update` prints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentUpdateResultDto {
    /// The project's fresh detail.
    pub assignment: AssignmentDetailDto,
    /// One entry per section whose own schema actually changed as part of
    /// this update (`refs`/`excludedRefs` was given) — empty when neither
    /// was part of the update, or every section's diff came back empty.
    pub schema_changes: BTreeMap<String, SchemaChangeDto>,
    /// Every row value dropped because a reclassified field's new role no
    /// longer accepts the shape it was stored under
    /// ([`SchemaChangeDto::reclassified`]'s own doc comment).
    pub stranded_values: Vec<StrandedValueDto>,
    /// Every row dropped because its target left T.
    pub dropped_rows: Vec<DroppedRowDto>,
}

// -- set_assignment_row / clear_assignment_row --------------------------
