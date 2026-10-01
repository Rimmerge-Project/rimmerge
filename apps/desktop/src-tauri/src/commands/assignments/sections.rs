//! Resolving the section a command addresses, and adding or removing sections.

use rim_resolve::domain::{AssignmentProject, Section};
use rim_session::use_cases::{AddAssignmentSection, RemoveAssignmentSection};

use super::projects::{detail_for, parse_assignment_id};
use crate::dto::assignment::{
    AddAssignmentSectionRequestDto, AssignmentDetailDto, RemoveAssignmentSectionRequestDto,
    RemoveAssignmentSectionResultDto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// A project's section def types, comma-joined in `BTreeMap` order —
/// [`resolve_section`]'s own error messages, mirroring `apps/cli`'s
/// `commands::assign::join_def_types`.
fn join_def_types<'a>(types: impl Iterator<Item = &'a String>) -> String {
    types.cloned().collect::<Vec<_>>().join(",")
}

/// Resolves which section a command addresses — `requested` (typically
/// `SetAssignmentRowRequestDto::section` or a sibling field) names a def
/// type explicitly; `None` falls back to the project's own single
/// section, erroring — naming every section def type — once it genuinely
/// has more than one, rather than silently picking one at random.
/// Mirrors `apps/cli`'s own `resolve_section`.
pub(super) fn resolve_section<'a>(
    project: &'a AssignmentProject,
    requested: Option<&str>,
) -> Result<&'a Section, CommandError> {
    if let Some(def_type) = requested {
        return project.section(def_type).ok_or_else(|| {
            CommandError::invalid_input(format!(
                "no section for def type {def_type:?}; this project has sections: {}",
                join_def_types(project.sections().keys())
            ))
        });
    }
    let mut sections = project.sections().values();
    match (sections.next(), sections.next()) {
        (None, _) => Err(CommandError::invalid_input(
            "this assignment project has no sections",
        )),
        (Some(only), None) => Ok(only),
        (Some(_), Some(_)) => Err(CommandError::invalid_input(format!(
            "this assignment project has {} sections ({}); specify a section",
            project.sections().len(),
            join_def_types(project.sections().keys())
        ))),
    }
}

/// [`resolve_section`], returning just the resolved section's own def
/// type — what every section-taking use-case call needs.
pub(super) fn resolve_section_type(
    project: &AssignmentProject,
    requested: Option<&str>,
) -> Result<String, CommandError> {
    resolve_section(project, requested).map(|section| section.schema.def_type.clone())
}

/// Infers and adds a new section to an existing assignment project — the
/// wizard's own "Add section" flow, reusing the same phase-1/phase-2
/// candidate inference `list_assignment_candidates`/
/// `infer_assignment_candidate` run for a brand-new project.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, [`CommandError::invalid_input`]
/// when `request.def_type` isn't owned by any member of the project's own
/// reference set, has no valid candidate under its current refs/targets,
/// or already has a section, or [`CommandError`] when inferring or
/// persisting the new section fails.
pub(crate) async fn add_assignment_section_inner(
    state: &AppState,
    request: AddAssignmentSectionRequestDto,
) -> Result<AssignmentDetailDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let use_case =
            AddAssignmentSection::new(adapters.assignment_store, adapters.def_reader.clone());
        use_case.execute(session, &id, &request.def_type)?;
        detail_for(session, &id)
    })
    .await
}

/// Removes a section from an assignment project. Idempotent (`removed:
/// false`, no error) when the project has no section for `request.def_type`.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, a
/// [`crate::error::CommandErrorCode::AssignmentSectionInUse`] error when
/// the section's own free-standing rows are still referenced by another
/// section's row and `request.force` is `false`, or [`CommandError`] when
/// persisting the removal fails.
pub(crate) async fn remove_assignment_section_inner(
    state: &AppState,
    request: RemoveAssignmentSectionRequestDto,
) -> Result<RemoveAssignmentSectionResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let use_case = RemoveAssignmentSection::new(adapters.assignment_store);
        let removed = use_case.execute(session, &id, &request.def_type, request.force)?;
        Ok(RemoveAssignmentSectionResultDto {
            removed: removed.is_some(),
            assignment: detail_for(session, &id)?,
        })
    })
    .await
}
