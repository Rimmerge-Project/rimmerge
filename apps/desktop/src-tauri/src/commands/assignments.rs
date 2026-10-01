//! Patch-maker (assignment) commands, multi-section:
//! `list_assignments`, `get_assignment`, `propose_assignment`,
//! `create_assignment`, `update_assignment`, `set_assignment_row`,
//! `clear_assignment_row`, `add_assignment_section`,
//! `remove_assignment_section`, `delete_assignment`,
//! `get_assignment_coverage`, `list_assignment_items`,
//! `copy_assignment_row_from`, `export_assignment`.
//!
//! Every command follows `apps/desktop/CLAUDE.md`'s shape: thin, mapping
//! DTOs to `rim-session` calls inside [`with_session`](crate::state::with_session); filtering, paging,
//! and inference all live in `rim-session`/`rim-resolve`.

use crate::commands::emit_session_changed;
use crate::dto::assignment::{
    AddAssignmentSectionRequestDto, AssignmentDetailDto, AssignmentRowResultDto,
    AssignmentSummaryDto, AssignmentUpdateResultDto, ClearAssignmentRowRequestDto,
    CopyAssignmentRowFromRequestDto, CopyAssignmentRowResultDto, CoverageDto,
    CreateAssignmentRequestDto, ExportAssignmentReportDto, ExportAssignmentRequestDto,
    InferAssignmentCandidateRequestDto, ListAssignmentCandidatesRequestDto,
    ListAssignmentCandidatesResponseDto, ListAssignmentItemsRequestDto, ListItemsPageDto,
    RemoveAssignmentSectionRequestDto, RemoveAssignmentSectionResultDto,
    SetAssignmentRowRequestDto, UpdateAssignmentRequestDto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::AppState;
use coverage::{get_assignment_coverage_inner, list_assignment_items_inner};
use export::export_assignment_inner;
use projects::{
    create_assignment_inner, delete_assignment_inner, get_assignment_inner,
    infer_assignment_candidate_inner, list_assignment_candidates_inner, list_assignments_inner,
    update_assignment_inner,
};
use rows::{clear_assignment_row_inner, copy_assignment_row_from_inner, set_assignment_row_inner};
use sections::{add_assignment_section_inner, remove_assignment_section_inner};

mod coverage;
mod export;
mod projects;
mod rows;
mod sections;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "assignments/assignments_tests.rs"]
mod tests;

/// See [`list_assignments_inner`].
///
/// # Errors
///
/// See [`list_assignments_inner`].
#[tauri::command]
pub async fn list_assignments(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AssignmentSummaryDto>, CommandError> {
    list_assignments_inner(&state).await
}

/// See [`get_assignment_inner`].
///
/// # Errors
///
/// See [`get_assignment_inner`].
#[tauri::command]
pub async fn get_assignment(
    state: tauri::State<'_, AppState>,
    assignment_id: String,
) -> Result<AssignmentDetailDto, CommandError> {
    get_assignment_inner(&state, assignment_id).await
}

/// See [`list_assignment_candidates_inner`].
///
/// # Errors
///
/// See [`list_assignment_candidates_inner`].
#[tauri::command]
pub async fn list_assignment_candidates(
    state: tauri::State<'_, AppState>,
    request: ListAssignmentCandidatesRequestDto,
) -> Result<ListAssignmentCandidatesResponseDto, CommandError> {
    list_assignment_candidates_inner(&state, request).await
}

/// See [`infer_assignment_candidate_inner`].
///
/// # Errors
///
/// See [`infer_assignment_candidate_inner`].
#[tauri::command]
pub async fn infer_assignment_candidate(
    state: tauri::State<'_, AppState>,
    request: InferAssignmentCandidateRequestDto,
) -> Result<crate::dto::assignment::AssignmentCandidateDto, CommandError> {
    infer_assignment_candidate_inner(&state, request).await
}

/// See [`create_assignment_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`create_assignment_inner`].
#[tauri::command]
pub async fn create_assignment(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: CreateAssignmentRequestDto,
) -> Result<AssignmentDetailDto, CommandError> {
    let result = create_assignment_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentCreated);
    }
    result
}

/// See [`update_assignment_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`update_assignment_inner`].
#[tauri::command]
pub async fn update_assignment(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: UpdateAssignmentRequestDto,
) -> Result<AssignmentUpdateResultDto, CommandError> {
    let result = update_assignment_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentChanged);
    }
    result
}

/// See [`set_assignment_row_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`set_assignment_row_inner`].
#[tauri::command]
pub async fn set_assignment_row(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetAssignmentRowRequestDto,
) -> Result<AssignmentRowResultDto, CommandError> {
    let result = set_assignment_row_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentRowChanged);
    }
    result
}

/// See [`clear_assignment_row_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`clear_assignment_row_inner`].
#[tauri::command]
pub async fn clear_assignment_row(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ClearAssignmentRowRequestDto,
) -> Result<AssignmentRowResultDto, CommandError> {
    let result = clear_assignment_row_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentRowChanged);
    }
    result
}

/// See [`add_assignment_section_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`add_assignment_section_inner`].
#[tauri::command]
pub async fn add_assignment_section(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: AddAssignmentSectionRequestDto,
) -> Result<AssignmentDetailDto, CommandError> {
    let result = add_assignment_section_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentChanged);
    }
    result
}

/// See [`remove_assignment_section_inner`]; also emits `session://changed`
/// on success.
///
/// # Errors
///
/// See [`remove_assignment_section_inner`].
#[tauri::command]
pub async fn remove_assignment_section(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: RemoveAssignmentSectionRequestDto,
) -> Result<RemoveAssignmentSectionResultDto, CommandError> {
    let result = remove_assignment_section_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentChanged);
    }
    result
}

/// See [`delete_assignment_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`delete_assignment_inner`].
#[tauri::command]
pub async fn delete_assignment(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    assignment_id: String,
) -> Result<(), CommandError> {
    let result = delete_assignment_inner(&state, assignment_id).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentDeleted);
    }
    result
}

/// See [`get_assignment_coverage_inner`].
///
/// # Errors
///
/// See [`get_assignment_coverage_inner`].
#[tauri::command]
pub async fn get_assignment_coverage(
    state: tauri::State<'_, AppState>,
    assignment_id: String,
    section: Option<String>,
) -> Result<CoverageDto, CommandError> {
    get_assignment_coverage_inner(&state, assignment_id, section).await
}

/// See [`list_assignment_items_inner`].
///
/// # Errors
///
/// See [`list_assignment_items_inner`].
#[tauri::command]
pub async fn list_assignment_items(
    state: tauri::State<'_, AppState>,
    request: ListAssignmentItemsRequestDto,
) -> Result<ListItemsPageDto, CommandError> {
    list_assignment_items_inner(&state, request).await
}

/// See [`copy_assignment_row_from_inner`]. Never mutates the session — no
/// `session://changed` is emitted.
///
/// # Errors
///
/// See [`copy_assignment_row_from_inner`].
#[tauri::command]
pub async fn copy_assignment_row_from(
    state: tauri::State<'_, AppState>,
    request: CopyAssignmentRowFromRequestDto,
) -> Result<CopyAssignmentRowResultDto, CommandError> {
    copy_assignment_row_from_inner(&state, request).await
}

/// See [`export_assignment_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`export_assignment_inner`].
#[tauri::command]
pub async fn export_assignment(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ExportAssignmentRequestDto,
) -> Result<ExportAssignmentReportDto, CommandError> {
    let result = export_assignment_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::AssignmentExported);
    }
    result
}
