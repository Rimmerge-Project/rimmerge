//! Copying groups and colours from another profile: `preview_copy_organization`
//! and `copy_organization`.
//!
//! The user picks the other profile's folder with a folder picker. The path
//! is user input, so it is parsed once here (`parse_user_path`: a drive-letter
//! path, never a network share or a device) and checked to be a folder other
//! than this profile's own; the use case then reads only `organization.json`
//! inside it, through the same store that reads this profile's file, and
//! never writes there. Both commands plan from a fresh read of the source:
//! `copy_organization` never trusts an earlier preview. A refusal is an
//! answer ([`CopyAnswerDto::Refused`]), not an error, and changes nothing.
//! Neither command touches the working set, the orders, the rules or
//! `ModsConfig.xml`.
//!
//! **Lock protocol** (the `file_baseline` shape): a short `with_session` for
//! this profile's folder, the folder checks and the source read on a bare
//! `spawn_blocking` with the lock released (the folder may be on a slow or
//! offline drive), then a short `with_session` that plans against this
//! profile as it is then and, for `copy_organization`, saves once. The plan is
//! made from the bytes that were read, never from the path, so a change to the
//! source between read and commit cannot make the commit act on something
//! other than what was read; a change to this profile in between is seen,
//! because the plan reads it under the lock.

use std::path::{Path, PathBuf};

use rim_session::organization::OrganizationLoad;
use rim_session::ports::OrganizationStore;
use rim_session::use_cases::{CopyOrganization, CopyOrganizationError, CopyRefusal};
use rim_session::user_path::UserPath;
use tauri::async_runtime::spawn_blocking;

use crate::commands::emit_session_changed;
use crate::dto::organization_copy::{
    CopyAnswerDto, CopyOrganizationRequestDto, CopyRefusalDto, CopyReportDto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::{CommandError, parse_user_path};
use crate::state::{AppState, with_session};

/// The folder the user picked, parsed once.
fn parse_source_dir(raw: &str) -> Result<UserPath, CommandError> {
    parse_user_path(raw, "the profile folder")
}

/// Whether `candidate` is the same folder as `profile_dir`: equal paths, or
/// both resolving to one folder.
fn is_same_folder(candidate: &Path, profile_dir: &Path) -> bool {
    if candidate == profile_dir {
        return true;
    }
    match (candidate.canonicalize(), profile_dir.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// The refusals only the boundary can make: not a folder, or this profile's own.
fn refuse_folder(source: &Path, profile_dir: &Path) -> Option<CopyRefusalDto> {
    if !source.is_dir() {
        return Some(CopyRefusalDto::SourceNotAFolder);
    }
    is_same_folder(source, profile_dir).then_some(CopyRefusalDto::SameProfile)
}

/// A report or a refusal is an answer; anything else is an error.
fn answer(
    result: Result<rim_session::organization::CopyReport, CopyOrganizationError>,
) -> Result<CopyAnswerDto, CommandError> {
    match result {
        Ok(report) => Ok(CopyAnswerDto::Report {
            report: CopyReportDto::from(&report),
        }),
        Err(CopyOrganizationError::Refused(refusal)) => Ok(refused(&refusal)),
        Err(CopyOrganizationError::Unavailable(inner)) => Err(inner.into()),
        Err(CopyOrganizationError::Save(inner)) => Err(inner.into()),
    }
}

fn refused(refusal: &CopyRefusal) -> CopyAnswerDto {
    CopyAnswerDto::Refused {
        reason: refusal.into(),
    }
}

/// What the unlocked read found: a refusal only the boundary can make, or the
/// source's organization file as it was read.
enum SourceRead {
    Refused(CopyRefusalDto),
    Loaded(OrganizationLoad),
}

/// A source read together with the profile folder its checks were made
/// against, for the commit to compare.
struct CheckedRead {
    profile_dir: PathBuf,
    read: SourceRead,
}

/// Phases 1 and 2 of both commands: a short lock for this profile's folder,
/// then the source folder's checks and its `organization.json` read on a bare
/// `spawn_blocking` with the lock released, so a slow or hung drive never
/// makes another session command wait.
async fn read_source_unlocked<S>(
    state: &AppState,
    source: UserPath,
    store: S,
) -> Result<CheckedRead, CommandError>
where
    S: OrganizationStore + Send + 'static,
{
    let profile_dir =
        with_session(state, |session| Ok(session.paths().profile_dir.clone())).await?;
    let checked_against = profile_dir.clone();
    let read = spawn_blocking(move || {
        if let Some(reason) = refuse_folder(source.as_path(), &profile_dir) {
            return SourceRead::Refused(reason);
        }
        SourceRead::Loaded(CopyOrganization::new(store).read_source(source.as_path()))
    })
    .await
    .map_err(|join_error| {
        tracing::warn!(error = %join_error, "copy source read task panicked");
        CommandError::internal("background task panicked")
    })?;
    Ok(CheckedRead {
        profile_dir: checked_against,
        read,
    })
}

/// Which use-case call the commit makes with the source it was handed.
#[derive(Clone, Copy)]
enum Commit {
    Preview,
    Copy,
}

/// Phase 3: under one short lock, checks the project is the one the source
/// was checked against, then plans (and for [`Commit::Copy`] saves once).
async fn commit<S>(
    state: &AppState,
    checked: CheckedRead,
    store: S,
    mode: rim_session::organization::CopyMode,
    step: Commit,
) -> Result<CopyAnswerDto, CommandError>
where
    S: OrganizationStore + Send + 'static,
{
    with_session(state, move |session| {
        if session.paths().profile_dir != checked.profile_dir {
            return Err(CommandError::project_changed());
        }
        let load = match checked.read {
            SourceRead::Refused(reason) => return Ok(CopyAnswerDto::Refused { reason }),
            SourceRead::Loaded(load) => load,
        };
        let use_case = CopyOrganization::new(store);
        match step {
            Commit::Preview => answer(use_case.preview_loaded(session, load, mode)),
            Commit::Copy => answer(use_case.execute_loaded(session, load, mode)),
        }
    })
    .await
}

/// What copying the other profile's organization would do. A read.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// `network_path_refused` for a network share,
/// [`CommandError::invalid_input`] for an empty, relative, device or
/// malformed path, or the `organization_unavailable` code. Everything wrong
/// with the source is a [`CopyAnswerDto::Refused`].
pub(crate) async fn preview_copy_organization_inner(
    state: &AppState,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    let store = state.adapters.organization_store.clone();
    preview_copy_organization_with(state, request, store).await
}

/// [`preview_copy_organization_inner`] over an explicit store (generic so a
/// test can slow the read down).
///
/// # Errors
///
/// See `preview_copy_organization_inner`.
pub(crate) async fn preview_copy_organization_with<S>(
    state: &AppState,
    request: CopyOrganizationRequestDto,
    store: S,
) -> Result<CopyAnswerDto, CommandError>
where
    S: OrganizationStore + Clone + Send + 'static,
{
    let source = parse_source_dir(&request.source_dir)?;
    let checked = read_source_unlocked(state, source, store.clone()).await?;
    commit(state, checked, store, request.mode.into(), Commit::Preview).await
}

/// Copies the other profile's organization into this one in one save,
/// planning afresh from the source as it is now.
///
/// "Afresh" is a new read of the source file made just before the commit,
/// never the preview's. The read happens with the session lock released; the
/// plan and the save happen under the lock against this profile as it is at
/// that moment, so an organization edit made while the source was being read
/// is merged with, never overwritten, and the save stays one atomic write.
///
/// # Errors
///
/// See `preview_copy_organization_inner`; also `profile_io_failed` when the
/// save fails (nothing changed).
pub(crate) async fn copy_organization_inner(
    state: &AppState,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    let store = state.adapters.organization_store.clone();
    copy_organization_with(state, request, store).await
}

/// [`copy_organization_inner`] over an explicit store (generic so a test can
/// slow the read down).
///
/// # Errors
///
/// See `copy_organization_inner`.
pub(crate) async fn copy_organization_with<S>(
    state: &AppState,
    request: CopyOrganizationRequestDto,
    store: S,
) -> Result<CopyAnswerDto, CommandError>
where
    S: OrganizationStore + Clone + Send + 'static,
{
    let source = parse_source_dir(&request.source_dir)?;
    let checked = read_source_unlocked(state, source, store.clone()).await?;
    commit(state, checked, store, request.mode.into(), Commit::Copy).await
}

/// See `preview_copy_organization_inner`.
///
/// # Errors
///
/// See `preview_copy_organization_inner`.
#[tauri::command]
pub async fn preview_copy_organization(
    state: tauri::State<'_, AppState>,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    preview_copy_organization_inner(&state, request).await
}

/// See `copy_organization_inner`; also emits `session://changed`
/// (`organizationChanged`) when the copy happened.
///
/// # Errors
///
/// See `copy_organization_inner`.
#[tauri::command]
pub async fn copy_organization(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    let result = copy_organization_inner(&state, request).await;
    if matches!(result, Ok(CopyAnswerDto::Report { .. })) {
        emit_session_changed(&app, SessionChangeReasonDto::OrganizationChanged);
    }
    result
}

#[cfg(test)]
#[path = "organization_copy_tests.rs"]
mod tests;
