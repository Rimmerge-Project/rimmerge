//! Listing, reading, proposing, creating, updating, and deleting assignment projects.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::AssignmentId;
use rim_session::Session;
use rim_session::use_cases::{
    AssignmentCoverage, CreateAssignment, CreateAssignmentInput, DeleteAssignment,
    UpdateAssignment, UpdateAssignmentInput,
};

use crate::commands::mod_names;
use crate::dto::assignment::{
    AssignmentDetailDto, AssignmentSummaryDto, AssignmentUpdateResultDto, CandidateSummaryDto,
    CreateAssignmentRequestDto, DroppedRowDto, InferAssignmentCandidateRequestDto,
    ListAssignmentCandidatesRequestDto, ListAssignmentCandidatesResponseDto,
    UpdateAssignmentRequestDto, assignment_detail_dto, assignment_summary_dto, mod_ref_dtos,
    stranded_value_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Parses `text` as an [`AssignmentId`], mapping a malformed id to
/// [`CommandError::invalid_input`] — the same shape
/// `commands::patch::parse_patch_id` uses for [`rim_resolve::domain::PatchId`].
pub(crate) fn parse_assignment_id(text: &str) -> Result<AssignmentId, CommandError> {
    text.parse()
        .map_err(|error: rim_resolve::domain::AssignmentIdParseError| {
            CommandError::invalid_input(error.to_string())
        })
}

/// Builds `id`'s [`AssignmentDetailDto`].
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when `id` isn't loaded.
pub(super) fn detail_for(
    session: &Session,
    id: &AssignmentId,
) -> Result<AssignmentDetailDto, CommandError> {
    let names = mod_names(session);
    let project = session
        .assignment(id)
        .ok_or_else(|| CommandError::assignment_not_found(id.as_str()))?;
    Ok(assignment_detail_dto(project, &names))
}

/// Every loaded assignment project, summarized (coverage included, so
/// `uncoveredCount` is accurate — each project's coverage is cached on
/// the session after its first build).
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or a coverage-build error (a stale scan) for any one project.
pub(crate) async fn list_assignments_inner(
    state: &AppState,
) -> Result<Vec<AssignmentSummaryDto>, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let coverage_uc = AssignmentCoverage::new(adapters.def_reader.clone());
        let ids: Vec<AssignmentId> = session.assignments().map(|p| p.id().clone()).collect();
        let names = mod_names(session);
        let mut summaries = Vec::with_capacity(ids.len());
        for id in ids {
            let project = session
                .assignment(&id)
                .cloned()
                .unwrap_or_else(|| unreachable!("id came from session.assignments() itself"));
            let target_keyed_types: Vec<String> = project
                .sections()
                .iter()
                .filter(|(_, section)| !section.is_standalone())
                .map(|(def_type, _)| def_type.clone())
                .collect();
            let mut coverages = std::collections::BTreeMap::new();
            for def_type in target_keyed_types {
                let coverage = coverage_uc.execute(session, &id, &def_type)?;
                coverages.insert(def_type, coverage);
            }
            summaries.push(assignment_summary_dto(&project, &coverages, &names));
        }
        Ok(summaries)
    })
    .await
}

/// One assignment project's full detail.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::assignment_not_found`] when `assignment_id` isn't
/// loaded.
pub(crate) async fn get_assignment_inner(
    state: &AppState,
    assignment_id: String,
) -> Result<AssignmentDetailDto, CommandError> {
    with_session(state, move |session| {
        let id = parse_assignment_id(&assignment_id)?;
        detail_for(session, &id)
    })
    .await
}

/// Phase 1 of the wizard's own first step: every assignment-def
/// candidate's cheap summary (type, instance count, owners) for a
/// reference/target selection — no instance reads at all, run before
/// anything is persisted.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_assignment_candidates_inner(
    state: &AppState,
    request: ListAssignmentCandidatesRequestDto,
) -> Result<ListAssignmentCandidatesResponseDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let refs: BTreeSet<ModId> = request.refs.into_iter().map(ModId::new).collect();
        let excluded_refs: BTreeSet<ModId> =
            request.excluded_refs.into_iter().map(ModId::new).collect();
        let targets: BTreeSet<ModId> = request.targets.into_iter().map(ModId::new).collect();

        let use_case =
            CreateAssignment::new(adapters.assignment_store, adapters.def_reader.clone());
        let summaries = use_case.list_candidates(session, &refs, &excluded_refs, &targets);
        let effective_refs = use_case.effective_refs(session, &refs, &excluded_refs);

        let names = mod_names(session);
        Ok(ListAssignmentCandidatesResponseDto {
            effective_refs: mod_ref_dtos(&effective_refs, &names),
            candidates: summaries
                .into_iter()
                .map(|summary| CandidateSummaryDto {
                    def_type: summary.def_type,
                    instance_count: summary.instance_count,
                    owners: mod_ref_dtos(&summary.owners, &names),
                })
                .collect(),
        })
    })
    .await
}

/// Phase 2 of the wizard's own first step: the real (expensive)
/// per-type inference for exactly one candidate the user picked from
/// [`list_assignment_candidates`](crate::commands::assignments::list_assignment_candidates)'s own response.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `request.defType` isn't a valid
/// candidate for this reference/target selection, or a
/// `MergeSourceFailed`/`internal` [`CommandError`] when reading the
/// candidate's own instances fails.
pub(crate) async fn infer_assignment_candidate_inner(
    state: &AppState,
    request: InferAssignmentCandidateRequestDto,
) -> Result<crate::dto::assignment::AssignmentCandidateDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let refs: BTreeSet<ModId> = request.refs.into_iter().map(ModId::new).collect();
        let excluded_refs: BTreeSet<ModId> =
            request.excluded_refs.into_iter().map(ModId::new).collect();
        let targets: BTreeSet<ModId> = request.targets.into_iter().map(ModId::new).collect();

        let use_case =
            CreateAssignment::new(adapters.assignment_store, adapters.def_reader.clone());
        let candidate = use_case
            .infer_candidate(session, &refs, &excluded_refs, &targets, &request.def_type)?
            .ok_or_else(|| {
                CommandError::invalid_input(format!(
                    "{:?} is not a valid candidate for this reference/target selection",
                    request.def_type
                ))
            })?;
        Ok((&candidate).into())
    })
    .await
}

/// Validates and creates a new assignment project from the wizard's
/// confirmed schema, persisting it before making it visible on the
/// session.
///
/// # Errors
///
/// Returns [`CommandError::patch_identity_invalid`] when the proposed
/// package id/display name is invalid or already taken,
/// [`CommandError::invalid_input`] when `refs`/`targets` is empty or the
/// schema fails to parse, or [`CommandError`] when persisting the new
/// project fails.
pub(crate) async fn create_assignment_inner(
    state: &AppState,
    request: CreateAssignmentRequestDto,
) -> Result<AssignmentDetailDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let schema = request.schema.try_into()?;
        let input = CreateAssignmentInput {
            name: request.name,
            package_id: request.package_id,
            display_name: request.display_name,
            refs: request.refs.into_iter().map(ModId::new).collect(),
            excluded_refs: request.excluded_refs.into_iter().map(ModId::new).collect(),
            targets: request.targets.into_iter().map(ModId::new).collect(),
            schema,
        };
        let use_case =
            CreateAssignment::new(adapters.assignment_store, adapters.def_reader.clone());
        let id = use_case.execute(session, input)?;
        detail_for(session, &id)
    })
    .await
}

/// Applies any of `request`'s given fields to one assignment project.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when
/// `request.assignment_id` isn't loaded, a `MergeSourceFailed`/`internal`
/// [`CommandError`] when re-inferring the schema fails, or
/// [`CommandError`] when persisting the update fails.
pub(crate) async fn update_assignment_inner(
    state: &AppState,
    request: UpdateAssignmentRequestDto,
) -> Result<AssignmentUpdateResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&request.assignment_id)?;
        let use_case =
            UpdateAssignment::new(adapters.assignment_store, adapters.def_reader.clone());
        let input = UpdateAssignmentInput {
            name: request.name,
            author: request.author,
            description: request.description,
            refs: request
                .refs
                .map(|ids| ids.into_iter().map(ModId::new).collect()),
            excluded_refs: request
                .excluded_refs
                .map(|ids| ids.into_iter().map(ModId::new).collect()),
            targets: request
                .targets
                .map(|ids| ids.into_iter().map(ModId::new).collect()),
        };
        let outcome = use_case.execute(session, &id, input)?;
        Ok(AssignmentUpdateResultDto {
            assignment: detail_for(session, &id)?,
            schema_changes: outcome
                .schema_changes
                .iter()
                .map(|(def_type, change)| (def_type.clone(), change.into()))
                .collect(),
            stranded_values: outcome
                .stranded_values
                .iter()
                .map(|(def_type, key, path)| stranded_value_dto(def_type, key, path))
                .collect(),
            dropped_rows: outcome
                .dropped_rows
                .iter()
                .map(|(def_type, target)| DroppedRowDto {
                    def_type: def_type.clone(),
                    target: target.into(),
                })
                .collect(),
        })
    })
    .await
}

/// Deletes an assignment project. Never touches a previously exported
/// folder.
///
/// # Errors
///
/// Returns [`CommandError::assignment_not_found`] when `assignment_id`
/// isn't loaded, or [`CommandError`] when removing the persisted file
/// fails.
pub(crate) async fn delete_assignment_inner(
    state: &AppState,
    assignment_id: String,
) -> Result<(), CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_assignment_id(&assignment_id)?;
        let use_case = DeleteAssignment::new(adapters.assignment_store);
        use_case.execute(session, &id)?;
        Ok(())
    })
    .await
}
