//! A project's coverage and its item pages.

use std::collections::BTreeSet;

use rim_resolve::domain::{AssignmentProject, Section};
use rim_session::use_cases::{AssignmentCoverage, ListItems};

use super::projects::parse_assignment_id;
use super::sections::resolve_section_type;
use crate::dto::assignment::{
    CoverageDto, ListAssignmentItemsRequestDto, ListItemsPageDto, list_items_page_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Builds one assignment project's coverage work queue: every candidate
/// target of T, what already references it, and — under a known
/// precedence rule — who would win. No filter/paging — see
/// `dto::assignment`'s own module doc comment.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::assignment_not_found`] when `assignment_id`
/// isn't loaded, [`CommandError::invalid_input`] when `section` fails to
/// resolve, or a `MergeSourceFailed`/`internal` [`CommandError`] when
/// reading a candidate's or an existing instance's own XML fails.
pub(crate) async fn get_assignment_coverage_inner(
    state: &AppState,
    assignment_id: String,
    section: Option<String>,
) -> Result<CoverageDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&assignment_id)?;
        let project = session
            .assignment(&id)
            .cloned()
            .ok_or_else(|| CommandError::assignment_not_found(&assignment_id))?;
        let def_type = resolve_section_type(&project, section.as_deref())?;
        let use_case = AssignmentCoverage::new(adapters.def_reader.clone());
        let coverage = use_case.execute(session, &id, &def_type)?;
        Ok((&coverage).into())
    })
    .await
}

/// Searches and pages every active def of one item type — the row
/// editor's own item pickers. When `request.assignment_id` is given, that
/// project's own free-standing rows of this exact item type (if it has a
/// section for one) are prepended, labelled `own: true`.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::assignment_not_found`] when
/// `request.assignment_id` is given but isn't loaded, or a
/// `MergeSourceFailed`/`internal` [`CommandError`] when reading a page's
/// own hint text fails.
pub(crate) async fn list_assignment_items_inner(
    state: &AppState,
    request: ListAssignmentItemsRequestDto,
) -> Result<ListItemsPageDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let project = request
            .assignment_id
            .as_deref()
            .map(|raw| -> Result<AssignmentProject, CommandError> {
                let id = parse_assignment_id(raw)?;
                session
                    .assignment(&id)
                    .cloned()
                    .ok_or_else(|| CommandError::assignment_not_found(raw))
            })
            .transpose()?;
        let use_case = ListItems::new(adapters.def_reader.clone());
        let page = use_case.execute(
            session,
            project.as_ref(),
            &request.def_type,
            &request.filter.into(),
        )?;
        let own_names: BTreeSet<String> = project
            .as_ref()
            .and_then(|p| p.section(&request.def_type))
            .map(Section::own_instance_names)
            .unwrap_or_default();
        Ok(list_items_page_dto(&page, &own_names))
    })
    .await
}
