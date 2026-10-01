//! Setting, clearing, and copying one row.

use rim_resolve::domain::{AssignmentRow, RowKey, TargetRef};
use rim_session::use_cases::{AssignmentInstances, ClearAssignmentRow, CopyFrom, SetAssignmentRow};

use super::projects::parse_assignment_id;
use super::sections::{resolve_section, resolve_section_type};
use crate::dto::assignment::{
    AssignmentRowResultDto, ClearAssignmentRowRequestDto, CopyAssignmentRowFromRequestDto,
    CopyAssignmentRowResultDto, SetAssignmentRowRequestDto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Validates and records one section's row — target-keyed (`request.target`
/// given) or free-standing (omitted, addressed by `request.row.defName`
/// alone) — persisting it before returning.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, [`CommandError::invalid_input`]
/// when `request.section`/`.target`/`.row` fails to resolve or parse, or
/// the aggregate rejects the row, or [`CommandError`] when persisting the
/// row fails.
pub(crate) async fn set_assignment_row_inner(
    state: &AppState,
    request: SetAssignmentRowRequestDto,
) -> Result<AssignmentRowResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let row: AssignmentRow = request.row.try_into()?;
        let key = match request.target {
            Some(target) => RowKey::Target(target.try_into()?),
            None => RowKey::Own(row.def_name.clone()),
        };
        let project = session
            .assignment(&id)
            .cloned()
            .ok_or_else(|| CommandError::assignment_not_found(&request.assignment_id))?;
        let def_type = resolve_section_type(&project, request.section.as_deref())?;
        let use_case = SetAssignmentRow::new(adapters.assignment_store);
        let replaced = use_case.execute(session, &id, &def_type, key, row)?;
        Ok(AssignmentRowResultDto {
            replaced: replaced.as_ref().map(Into::into),
        })
    })
    .await
}

/// Removes one section's row — target-keyed (`request.target` given) or
/// free-standing (`request.def_name` given) — if it has one, persisting
/// the change before returning.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, [`CommandError::invalid_input`]
/// when `request.section`/`.target` fails to resolve or parse, or neither/
/// both of `request.target`/`.defName` were given, or [`CommandError`]
/// when persisting the change fails.
pub(crate) async fn clear_assignment_row_inner(
    state: &AppState,
    request: ClearAssignmentRowRequestDto,
) -> Result<AssignmentRowResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let key = match (request.target, request.def_name) {
            (Some(target), None) => RowKey::Target(target.try_into()?),
            (None, Some(def_name)) => RowKey::Own(def_name),
            (None, None) => {
                return Err(CommandError::invalid_input(
                    "specify either target (a target-keyed row) or defName (a free-standing row)",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(CommandError::invalid_input(
                    "specify only one of target or defName, not both",
                ));
            }
        };
        let project = session
            .assignment(&id)
            .cloned()
            .ok_or_else(|| CommandError::assignment_not_found(&request.assignment_id))?;
        let def_type = resolve_section_type(&project, request.section.as_deref())?;
        let use_case = ClearAssignmentRow::new(adapters.assignment_store);
        let cleared = use_case.execute(session, &id, &def_type, &key)?;
        Ok(AssignmentRowResultDto {
            replaced: cleared.as_ref().map(Into::into),
        })
    })
    .await
}

/// Builds a fresh row for `request.target` from an existing instance's
/// own fields — a pure preview: never validated against `set_assignment_row`'s
/// own invariants and never saved. The caller (the row editor) shows the
/// result for further editing, then calls `set_assignment_row` itself to
/// persist it.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, [`CommandError::invalid_input`]
/// when `request.section`/`.target` fails to resolve or parse, the
/// resolved section is free-standing (no `TargetKey` field to match a
/// target through), or `request.source_def_name` doesn't name an active
/// instance, or a `MergeSourceFailed`/`internal` [`CommandError`] when
/// reading the instances fails.
pub(crate) async fn copy_assignment_row_from_inner(
    state: &AppState,
    request: CopyAssignmentRowFromRequestDto,
) -> Result<CopyAssignmentRowResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let target: TargetRef = request.target.try_into()?;
        let project = session
            .assignment(&id)
            .cloned()
            .ok_or_else(|| CommandError::assignment_not_found(&request.assignment_id))?;
        let section = resolve_section(&project, request.section.as_deref())?;
        if section.is_standalone() {
            return Err(CommandError::invalid_input(format!("copy-from builds a target-keyed row, but section {:?} is free-standing (no TargetKey field to match a target through)",
                section.schema.def_type
            )));
        }
        let def_type = section.schema.def_type.clone();
        let (_, source_instance) = AssignmentInstances::new(adapters.def_reader.clone())
            .find_by_name(session, &def_type, &request.source_def_name)?
            .ok_or_else(|| {
                CommandError::invalid_input(format!("no active instance named {:?}",
                    request.source_def_name
                ))
            })?;
        let outcome = CopyFrom::execute(&project, &def_type, &target, &source_instance, session)?;
        Ok((&outcome).into())
    })
    .await
}
