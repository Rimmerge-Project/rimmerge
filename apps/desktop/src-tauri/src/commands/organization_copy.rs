//! Copying groups and colours from another profile: `preview_copy_organization`
//! and `copy_organization`.
//!
//! The user picks the other profile's folder with a folder picker. The path
//! is user input, so it is parsed once here (non-empty, absolute, no NUL)
//! and checked to be a folder other than this profile's own; the use case
//! then reads only `organization.json` inside it, through the same store
//! that reads this profile's file, and never writes there. Both commands
//! plan from a fresh read of the source: `copy_organization` never trusts
//! an earlier preview. A refusal is an answer ([`CopyAnswerDto::Refused`]),
//! not an error, and changes nothing. Neither command touches the working
//! set, the orders, the rules or `ModsConfig.xml`.

use std::path::{Component, Path, PathBuf, Prefix};

use rim_session::use_cases::{CopyOrganization, CopyOrganizationError, CopyRefusal};

use crate::commands::emit_session_changed;
use crate::dto::organization_copy::{
    CopyAnswerDto, CopyOrganizationRequestDto, CopyRefusalDto, CopyReportDto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Whether `raw` can only name a file-system folder: an allowlist on the
/// parsed Windows prefix (drive letter, `\\?\C:\`, UNC, `\\?\UNC\`). Every
/// other prefix (`\\.\pipe\x`, `\\?\pipe\x`, `\\?\CONIN$`,
/// `\\?\GLOBALROOT\x`, `\\?\PhysicalDrive0`) is a device namespace, and
/// opening one could block or reach a device while the session lock is
/// held. A path with no prefix (not Windows) is left to the absolute check.
fn is_file_system_path(raw: &str) -> bool {
    match Path::new(raw).components().next() {
        Some(Component::Prefix(prefix)) => matches!(
            prefix.kind(),
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(..) | Prefix::VerbatimUNC(..)
        ),
        _ => true,
    }
}

/// The folder the user picked, as a path: never empty, absolute, free of NUL
/// and a file-system path, not a device.
fn parse_source_dir(raw: &str) -> Result<PathBuf, CommandError> {
    if raw.trim().is_empty() {
        return Err(CommandError::invalid_input("the profile folder is empty"));
    }
    if raw.contains('\0') {
        return Err(CommandError::invalid_input(
            "the profile folder holds a character a path cannot",
        ));
    }
    if !is_file_system_path(raw) {
        return Err(CommandError::invalid_input(
            "the profile folder must be a folder, not a device path",
        ));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(CommandError::invalid_input(
            "the profile folder must be an absolute path",
        ));
    }
    Ok(path)
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

/// What copying the other profile's organization would do. A read.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] for an empty, relative or malformed path,
/// or the `organization_unavailable` code. Everything wrong with the source
/// is a [`CopyAnswerDto::Refused`].
pub(crate) async fn preview_copy_organization_inner(
    state: &AppState,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    let source = parse_source_dir(&request.source_dir)?;
    let mode = request.mode.into();
    let store = state.adapters.organization_store.clone();
    with_session(state, move |session| {
        if let Some(reason) = refuse_folder(&source, &session.paths().profile_dir) {
            return Ok(CopyAnswerDto::Refused { reason });
        }
        answer(CopyOrganization::new(store).preview(session, &source, mode))
    })
    .await
}

/// Copies the other profile's organization into this one in one save,
/// planning afresh from the source as it is now.
///
/// # Errors
///
/// See `preview_copy_organization_inner`; also `profile_io_failed` when the
/// save fails (nothing changed).
pub(crate) async fn copy_organization_inner(
    state: &AppState,
    request: CopyOrganizationRequestDto,
) -> Result<CopyAnswerDto, CommandError> {
    let source = parse_source_dir(&request.source_dir)?;
    let mode = request.mode.into();
    let store = state.adapters.organization_store.clone();
    with_session(state, move |session| {
        if let Some(reason) = refuse_folder(&source, &session.paths().profile_dir) {
            return Ok(CopyAnswerDto::Refused { reason });
        }
        answer(CopyOrganization::new(store).execute(session, &source, mode))
    })
    .await
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
