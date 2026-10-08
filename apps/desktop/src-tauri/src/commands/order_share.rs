//! Sharing a load order: `export_order_file`, `export_order_text`,
//! `suggested_mod_list_path`, `preview_order_import_file`,
//! `preview_order_import_text`, `import_order` and `open_workshop_page`.
//!
//! Every rule lives in `rim-session` (`ExportOrder`, `PreviewOrderImport`,
//! `ImportOrder`, `plan_import`); these commands map DTOs in and out.
//!
//! - Export reads `ModsConfig.xml` as it is *now* (not the selected order)
//!   and writes only where the frontend's save dialog pointed. It never
//!   touches `ModsConfig.xml`, so it is not probe-gated.
//! - Previews are read-only.
//! - `import_order` is `rescan_with` over `ScanRequest::Import`: the
//!   order is validated, scanned and swapped in under one `load_lock`, and
//!   never staged in the working set. It emits `session://changed` with
//!   [`SessionChangeReasonDto::Rescanned`], since the whole session was
//!   replaced by a scan, exactly as for `rescan_project`.
//! - `open_workshop_page` takes an id, never a URL.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_session::mod_info::workshop_url;
use rim_session::mod_list::WorkshopId;
use rim_session::use_cases::{
    ExportOrder, ExportSource, ImportPreview, ImportTarget, PreviewOrderImport,
};

use crate::commands::active_set::{ScanRequest, rescan_with};
use crate::commands::{EVENT_PROJECT_PROGRESS, emit_session_changed};
use crate::dto::order_share::{
    ExportOrderFileRequestDto, ExportResultDto, ExportTextDto, ImportOrderRequestDto,
    OpenWorkshopPageRequestDto, OrderImportOutcomeDto, PreviewOrderImportFileRequestDto,
    PreviewOrderImportTextRequestDto,
};
use crate::dto::project::{ProgressEventDto, ProjectSummaryDto, SessionChangeReasonDto};
use crate::error::{CommandError, CommandErrorCode};
use crate::state::{AppState, with_session};

/// The file name the save dialog is offered inside RimWorld's own
/// `ModLists` folder, so the game's "Load list" finds it.
const SUGGESTED_FILE_NAME: &str = "rimmerge-load-order.rml";

/// The `ModLists` folder name beside RimWorld's `Config` folder.
const MOD_LISTS_FOLDER: &str = "ModLists";

fn parse_path(raw: &str) -> Result<PathBuf, CommandError> {
    if raw.trim().is_empty() {
        return Err(CommandError::invalid_input("the file path is empty"));
    }
    Ok(PathBuf::from(raw))
}

/// The one extension an export may write: RimWorld's own mod-list files.
/// An allowlist rather than a blocklist of `ModsConfig.xml`: `canonicalize`
/// cannot see through every alias of that file (an administrative
/// share path, a mapped drive), so nothing but a `.rml` name is ever written.
const EXPORT_EXTENSION: &str = "rml";

/// Refuses a target whose extension is not `.rml` (ASCII case-insensitive).
fn require_rml_extension(path: &Path) -> Result<(), CommandError> {
    let is_rml = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXPORT_EXTENSION));
    // A colon in the file name is an NTFS alternate data stream
    // (`ModsConfig.xml:x.rml`): it passes the extension test and would leave
    // a stream on the file it names.
    let has_stream_separator = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().contains(':'));
    if is_rml && !has_stream_separator {
        return Ok(());
    }
    Err(CommandError::invalid_input(
        "a mod list is saved as a .rml file",
    ))
}

/// Refuses a path that is not a regular file, so a named pipe or device
/// can never block the session lock while it is opened.
fn require_regular_file(path: &Path) -> Result<(), CommandError> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        CommandError::new(
            CommandErrorCode::ModListIoFailed,
            format!("{}: {error}", path.display()),
        )
    })?;
    if metadata.is_file() {
        return Ok(());
    }
    Err(CommandError::new(
        CommandErrorCode::ModListIoFailed,
        format!("{} is not a regular file", path.display()),
    ))
}

/// Whether `candidate` names the same file as `protected`: equal paths, or
/// both resolving to one file. A second check behind
/// [`require_rml_extension`], which is the one that cannot be aliased.
fn is_same_file(candidate: &Path, protected: &Path) -> bool {
    if candidate == protected {
        return true;
    }
    match (candidate.canonicalize(), protected.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Writes the order in `ModsConfig.xml`, re-read now, to `request.path` as
/// a RimWorld mod list (`.rml`), replacing a file there.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded;
/// [`CommandError::invalid_input`] for an empty path, a target that is not a
/// `.rml` file, or the project's own `ModsConfig.xml` (an export must never
/// overwrite it); `mods_config_io_failed`
/// when `ModsConfig.xml` can't be read; `nothing_to_export` when no active
/// mod can be listed; `mod_list_io_failed` when the file can't be written.
pub(crate) async fn export_order_file_inner(
    state: &AppState,
    request: ExportOrderFileRequestDto,
) -> Result<ExportResultDto, CommandError> {
    let path = parse_path(&request.path)?;
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        require_rml_extension(&path)?;
        if is_same_file(&path, &session.paths().mods_config) {
            return Err(CommandError::invalid_input(
                "a mod list cannot be written over ModsConfig.xml",
            ));
        }
        let source = ExportSource::from_session(session);
        let exported = ExportOrder::new(adapters.config_store, adapters.mod_list_store)
            .write_file(&source, &path)?;
        Ok((&exported).into())
    })
    .await
}

/// See `export_order_file_inner`.
///
/// # Errors
///
/// See `export_order_file_inner`.
#[tauri::command]
pub async fn export_order_file(
    state: tauri::State<'_, AppState>,
    request: ExportOrderFileRequestDto,
) -> Result<ExportResultDto, CommandError> {
    export_order_file_inner(&state, request).await
}

/// Renders the order in `ModsConfig.xml`, re-read now, in the text format
/// for the clipboard. Writes nothing.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// `mods_config_io_failed` when `ModsConfig.xml` can't be read, or
/// `nothing_to_export` when no active mod can be listed.
pub(crate) async fn export_order_text_inner(
    state: &AppState,
) -> Result<ExportTextDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let source = ExportSource::from_session(session);
        let exported =
            ExportOrder::new(adapters.config_store, adapters.mod_list_store).build(&source)?;
        Ok((&exported).into())
    })
    .await
}

/// See `export_order_text_inner`.
///
/// # Errors
///
/// See `export_order_text_inner`.
#[tauri::command]
pub async fn export_order_text(
    state: tauri::State<'_, AppState>,
) -> Result<ExportTextDto, CommandError> {
    export_order_text_inner(&state).await
}

/// Where the save dialog should start: a file inside RimWorld's own
/// `ModLists` folder (beside the `Config` folder holding `ModsConfig.xml`),
/// so the exported list shows up in the game's "Load list". `None` when
/// that folder doesn't exist: this never creates it.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn suggested_mod_list_path_inner(
    state: &AppState,
) -> Result<Option<String>, CommandError> {
    with_session(state, |session| {
        let mod_lists = session
            .paths()
            .mods_config
            .parent()
            .and_then(Path::parent)
            .map(|config_root| config_root.join(MOD_LISTS_FOLDER))
            .filter(|folder| folder.is_dir());
        Ok(mod_lists.map(|folder| folder.join(SUGGESTED_FILE_NAME).display().to_string()))
    })
    .await
}

/// See `suggested_mod_list_path_inner`.
///
/// # Errors
///
/// See `suggested_mod_list_path_inner`.
#[tauri::command]
pub async fn suggested_mod_list_path(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    suggested_mod_list_path_inner(&state).await
}

fn outcome(preview: &ImportPreview, target: &ImportTarget) -> OrderImportOutcomeDto {
    OrderImportOutcomeDto::from_preview(preview, &target.inventory)
}

/// Previews importing the list in `request.path` (a `.rml`, a
/// `ModsConfig.xml`-shaped list, or text; detected by content). Read-only.
/// A document that isn't an importable list is a `rejected` outcome, not an
/// error.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] for an empty path, or `mod_list_io_failed`
/// when the path is not a regular file or can't be read.
pub(crate) async fn preview_order_import_file_inner(
    state: &AppState,
    request: PreviewOrderImportFileRequestDto,
) -> Result<OrderImportOutcomeDto, CommandError> {
    let path = parse_path(&request.path)?;
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        require_regular_file(&path)?;
        let target = ImportTarget::from_session(session);
        let preview = PreviewOrderImport::new(adapters.mod_list_store).from_file(&path, &target)?;
        Ok(outcome(&preview, &target))
    })
    .await
}

/// See `preview_order_import_file_inner`.
///
/// # Errors
///
/// See `preview_order_import_file_inner`.
#[tauri::command]
pub async fn preview_order_import_file(
    state: tauri::State<'_, AppState>,
    request: PreviewOrderImportFileRequestDto,
) -> Result<OrderImportOutcomeDto, CommandError> {
    preview_order_import_file_inner(&state, request).await
}

/// Previews importing pasted text (the "Copy as text" format, or one
/// package id per line), bounded by the same size limit as a file.
/// Read-only.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn preview_order_import_text_inner(
    state: &AppState,
    request: PreviewOrderImportTextRequestDto,
) -> Result<OrderImportOutcomeDto, CommandError> {
    with_session(state, move |session| {
        let target = ImportTarget::from_session(session);
        let preview = ImportPreview::from_text(&request.text, &target);
        Ok(outcome(&preview, &target))
    })
    .await
}

/// See `preview_order_import_text_inner`.
///
/// # Errors
///
/// See `preview_order_import_text_inner`.
#[tauri::command]
pub async fn preview_order_import_text(
    state: tauri::State<'_, AppState>,
    request: PreviewOrderImportTextRequestDto,
) -> Result<OrderImportOutcomeDto, CommandError> {
    preview_order_import_text_inner(&state, request).await
}

/// Imports a previewed order: validates it against the live inventory,
/// scans with it and swaps the result in with Current selected, all under
/// one `load_lock` (`rescan_with`). Either the whole import lands or the
/// session is unchanged. Nothing is written to `ModsConfig.xml`; the
/// normal Apply does that.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when the order is oversized, names an
/// unknown or repeated mod, lacks Core, or the working set changed while
/// the scan ran, or the scan's own error (`scan_failed`, ...).
pub(crate) async fn import_order_inner(
    state: &AppState,
    request: ImportOrderRequestDto,
    on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<ProjectSummaryDto, CommandError> {
    let order = request.order.iter().map(ModId::new).collect();
    rescan_with(state, ScanRequest::Import(order), on_progress).await
}

/// See `import_order_inner`; the [`tauri::AppHandle`] here only exists to
/// emit [`EVENT_PROJECT_PROGRESS`] while the scan runs, then
/// `session://changed` with [`SessionChangeReasonDto::Rescanned`] on
/// success.
///
/// # Errors
///
/// See `import_order_inner`.
#[tauri::command]
pub async fn import_order(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ImportOrderRequestDto,
) -> Result<ProjectSummaryDto, CommandError> {
    use tauri::Emitter;

    let progress_app = app.clone();
    let result = import_order_inner(&state, request, move |progress| {
        let _ = progress_app.emit(EVENT_PROJECT_PROGRESS, progress);
    })
    .await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Rescanned);
    }
    result
}

/// Opens a mod's Steam Workshop page. The request is an id as digits: the
/// backend builds the fixed URL, so a URL (or any non-id text) is refused.
/// Needs no project.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request.workshop_id` isn't
/// a non-zero id, or [`CommandError::internal`] when the OS couldn't open
/// the link.
pub(crate) async fn open_workshop_page_inner(
    state: &AppState,
    request: OpenWorkshopPageRequestDto,
) -> Result<(), CommandError> {
    let id = WorkshopId::try_from(request.workshop_id.as_str())
        .map_err(|_| CommandError::invalid_input("not a Steam Workshop item id"))?;
    state
        .link_opener
        .open(&workshop_url(id.get()))
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// See `open_workshop_page_inner`.
///
/// # Errors
///
/// See `open_workshop_page_inner`.
#[tauri::command]
pub async fn open_workshop_page(
    state: tauri::State<'_, AppState>,
    request: OpenWorkshopPageRequestDto,
) -> Result<(), CommandError> {
    open_workshop_page_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use rim_resolve::domain::OrderSource;
    use rim_session::mod_list::ModListLimits;
    use rim_session::ports::{ModListFileStore, ModListRead};
    use rim_session::use_cases::ActivateMods;

    use super::*;
    use crate::dto::active_set::ActivateRequestDto;
    use crate::dto::common::OrderSourceDto;
    use crate::dto::order_share::{
        CorePlacementDto, ImportBlockedDto, ImportRejectionDto, ImportedEntryDto, MissingKindDto,
        OrderImportPreviewDto,
    };
    use crate::dto::project::ProjectPathsDto;
    use crate::error::CommandErrorCode;
    use crate::test_support::RecordingLinkOpener;

    const CORE: &str = "ludeon.rimworld";

    /// A scratch copy of the sample game with a Core stub added and
    /// `ModsConfig.xml` rewritten to `[Core, sample.mod]`; `aaa.mod` and
    /// `zzz.mod` are installed but inactive. Everything lives in the temp
    /// dir.
    fn scratch_game_with_core() -> (tempfile::TempDir, ProjectPathsDto) {
        let (scratch, paths) = crate::test_support::scratch_sample_game();
        let core_about = Path::new(&paths.game_dir)
            .join("Data")
            .join("Core")
            .join("About");
        std::fs::create_dir_all(&core_about).expect("create the Core stub folder");
        std::fs::write(
            core_about.join("About.xml"),
            "<ModMetaData><packageId>ludeon.rimworld</packageId><name>Core</name></ModMetaData>",
        )
        .expect("write the Core stub");
        std::fs::write(
            &paths.mods_config,
            "<ModsConfigData><version>1.6.4871 rev590</version><activeMods>\
             <li>ludeon.rimworld</li><li>sample.mod</li></activeMods></ModsConfigData>",
        )
        .expect("rewrite ModsConfig.xml");
        (scratch, paths)
    }

    async fn loaded_game() -> (tempfile::TempDir, AppState, ProjectPathsDto) {
        let (scratch, paths) = scratch_game_with_core();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths.clone(), |_| {})
            .await
            .expect("loading the scratch game must succeed");
        (scratch, state, paths)
    }

    fn mod_ids(raws: &[&str]) -> Vec<String> {
        raws.iter().map(|raw| (*raw).to_string()).collect()
    }

    fn import_request(raws: &[&str]) -> ImportOrderRequestDto {
        ImportOrderRequestDto {
            order: mod_ids(raws),
        }
    }

    fn text_request(text: &str) -> PreviewOrderImportTextRequestDto {
        PreviewOrderImportTextRequestDto {
            text: text.to_string(),
        }
    }

    fn file_request(path: &Path) -> PreviewOrderImportFileRequestDto {
        PreviewOrderImportFileRequestDto {
            path: path.display().to_string(),
        }
    }

    fn ready(outcome: OrderImportOutcomeDto) -> OrderImportPreviewDto {
        match outcome {
            OrderImportOutcomeDto::Ready { preview } => preview,
            OrderImportOutcomeDto::Rejected { reason } => panic!("rejected: {reason:?}"),
        }
    }

    /// The live session's Current order, selection and working ids.
    #[derive(Debug, PartialEq, Eq)]
    struct SessionFacts {
        current: Vec<String>,
        selected: OrderSource,
        working: Vec<String>,
    }

    fn session_facts(state: &AppState) -> SessionFacts {
        let guard = state.session.read().expect("lock");
        let session = guard.as_ref().expect("a session is loaded");
        let texts = |ids: &[ModId]| ids.iter().map(|id| id.as_str().to_string()).collect();
        SessionFacts {
            current: texts(session.orders().current.as_slice()),
            selected: session.selected(),
            working: texts(session.working().ids()),
        }
    }

    async fn nothing_is_staged_or_unapplied(state: &AppState) -> bool {
        let pending = crate::commands::active_set::get_pending_active_changes_inner(state)
            .await
            .expect("a project is loaded");
        pending.unscanned.added.is_empty()
            && pending.unscanned.removed.is_empty()
            && pending.unapplied.added.is_empty()
            && pending.unapplied.removed.is_empty()
    }

    // --- export ---

    #[tokio::test]
    async fn export_order_text_lists_the_file_order_with_names() {
        let (_scratch, state, _paths) = loaded_game().await;

        let exported = export_order_text_inner(&state).await.expect("exports");

        assert_eq!(exported.count, 2);
        assert!(exported.unrepresentable.is_empty());
        let lines: Vec<&str> = exported
            .text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect();
        assert_eq!(lines.len(), 2, "{}", exported.text);
        assert!(lines[0].contains("[ludeon.rimworld]"), "{}", lines[0]);
        assert!(lines[1].contains("Sample Mod") && lines[1].contains("[sample.mod]"));
    }

    #[tokio::test]
    async fn export_order_text_reports_nothing_to_export_for_an_empty_file_list() {
        let (_scratch, state, paths) = loaded_game().await;
        std::fs::write(
            &paths.mods_config,
            "<ModsConfigData><version>1.6</version><activeMods></activeMods></ModsConfigData>",
        )
        .expect("empty the active list");

        let error = export_order_text_inner(&state)
            .await
            .expect_err("nothing is active");

        assert_eq!(error.code, CommandErrorCode::NothingToExport);
    }

    #[tokio::test]
    async fn export_order_text_exports_the_file_as_it_is_now_not_as_scanned() {
        let (_scratch, state, paths) = loaded_game().await;
        std::fs::write(
            &paths.mods_config,
            "<ModsConfigData><version>1.6</version><activeMods><li>ludeon.rimworld</li>\
             <li>zzz.mod</li><li>sample.mod</li></activeMods></ModsConfigData>",
        )
        .expect("edit ModsConfig.xml after the scan");

        let exported = export_order_text_inner(&state).await.expect("exports");

        assert_eq!(exported.count, 3);
    }

    #[tokio::test]
    async fn export_order_file_writes_a_mod_list_that_reads_back_in_file_order() {
        let (scratch, state, _paths) = loaded_game().await;
        let target = scratch.path().join("shared.rml");

        let result = export_order_file_inner(
            &state,
            ExportOrderFileRequestDto {
                path: target.display().to_string(),
            },
        )
        .await
        .expect("exports");

        assert_eq!(result.count, 2);
        let read = rim_io::RmlFileStore::new().read(&target).expect("reads");
        let ModListRead::Parsed(parsed) = read else {
            panic!("the exported file must parse: {read:?}");
        };
        let ids: Vec<&str> = parsed
            .list
            .entries()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect();
        assert_eq!(ids, ["ludeon.rimworld", "sample.mod"]);
    }

    #[tokio::test]
    async fn export_order_file_refuses_to_overwrite_modsconfig_xml() {
        let (_scratch, state, paths) = loaded_game().await;
        let before = std::fs::read(&paths.mods_config).expect("read ModsConfig.xml");

        let error = export_order_file_inner(
            &state,
            ExportOrderFileRequestDto {
                path: paths.mods_config.clone(),
            },
        )
        .await
        .expect_err("the project's own ModsConfig.xml is protected");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert_eq!(std::fs::read(&paths.mods_config).expect("read"), before);
    }

    fn export_request(path: &Path) -> ExportOrderFileRequestDto {
        ExportOrderFileRequestDto {
            path: path.display().to_string(),
        }
    }

    #[tokio::test]
    async fn export_order_file_accepts_the_rml_extension_in_any_ascii_case() {
        let (scratch, state, _paths) = loaded_game().await;
        let target = scratch.path().join("shared.RML");

        export_order_file_inner(&state, export_request(&target))
            .await
            .expect("an upper-case extension is still a .rml file");

        assert!(target.is_file());
    }

    #[tokio::test]
    async fn export_order_file_refuses_any_target_that_is_not_a_rml_file() {
        let (scratch, state, _paths) = loaded_game().await;

        for name in ["shared.xml", "shared", "shared.rml.xml", "shared.txt"] {
            let target = scratch.path().join(name);

            let error = export_order_file_inner(&state, export_request(&target))
                .await
                .expect_err(name);

            assert_eq!(error.code, CommandErrorCode::InvalidInput, "{name}");
            assert!(!target.exists(), "{name} must not be written");
        }
    }

    /// A project whose `ModsConfig.xml` is named `ModsConfig.rml`, so the
    /// extension rule cannot be what refuses an export over it: only the
    /// same-file check stands between the export and the file.
    async fn loaded_game_with_rml_named_config() -> (tempfile::TempDir, AppState, PathBuf) {
        let (scratch, mut paths) = scratch_game_with_core();
        let renamed = Path::new(&paths.mods_config).with_file_name("ModsConfig.rml");
        std::fs::rename(&paths.mods_config, &renamed).expect("rename the config");
        paths.mods_config = renamed.display().to_string();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch game must succeed");
        (scratch, state, renamed)
    }

    async fn assert_export_refused_and_config_untouched(
        state: &AppState,
        alias: &Path,
        config: &Path,
    ) {
        let before = std::fs::read(config).expect("read the config");

        let error = export_order_file_inner(state, export_request(alias))
            .await
            .expect_err("an alias of the project's own config is protected");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert!(
            error.message.contains("ModsConfig.xml"),
            "{}",
            error.message
        );
        assert_eq!(std::fs::read(config).expect("read"), before);
    }

    #[tokio::test]
    async fn export_order_file_refuses_a_dot_dot_alias_of_the_config() {
        let (_scratch, state, config) = loaded_game_with_rml_named_config().await;
        let folder = config.parent().expect("the config has a folder");
        std::fs::create_dir_all(folder.join("sub")).expect("create sub");
        let alias = folder.join("sub").join("..").join("ModsConfig.rml");

        assert_export_refused_and_config_untouched(&state, &alias, &config).await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn export_order_file_refuses_an_upper_cased_alias_of_the_config() {
        let (_scratch, state, config) = loaded_game_with_rml_named_config().await;
        let alias = config.with_file_name("MODSCONFIG.RML");

        assert_export_refused_and_config_untouched(&state, &alias, &config).await;
    }

    #[tokio::test]
    async fn export_order_file_refuses_a_file_name_with_a_colon() {
        let (_scratch, state, config) = loaded_game_with_rml_named_config().await;
        let before = std::fs::read(&config).expect("read the config");
        let stream = config.with_file_name("ModsConfig.rml:x.rml");

        let error = export_order_file_inner(&state, export_request(&stream))
            .await
            .expect_err("a colon names an alternate data stream");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert_eq!(std::fs::read(&config).expect("read"), before);
    }

    #[tokio::test]
    async fn export_order_file_refuses_an_empty_path() {
        let (_scratch, state, _paths) = loaded_game().await;

        let error = export_order_file_inner(
            &state,
            ExportOrderFileRequestDto {
                path: "  ".to_string(),
            },
        )
        .await
        .expect_err("an empty path");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn export_order_file_reports_a_failed_write_as_mod_list_io_failed() {
        let (scratch, state, _paths) = loaded_game().await;
        let blocker = scratch.path().join("blocker");
        std::fs::write(&blocker, "a file, not a folder").expect("write blocker");

        let error = export_order_file_inner(
            &state,
            ExportOrderFileRequestDto {
                path: blocker.join("shared.rml").display().to_string(),
            },
        )
        .await
        .expect_err("the parent is a file");

        assert_eq!(error.code, CommandErrorCode::ModListIoFailed);
    }

    // --- the suggested path ---

    #[tokio::test]
    async fn suggested_path_is_none_when_the_mod_lists_folder_is_absent() {
        let (_scratch, state, _paths) = loaded_game().await;

        let path = suggested_mod_list_path_inner(&state)
            .await
            .expect("a project is loaded");

        assert_eq!(path, None);
    }

    #[tokio::test]
    async fn suggested_path_points_into_the_mod_lists_folder_beside_config() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_dir = temp.path().join("Config");
        let mod_lists = temp.path().join("ModLists");
        std::fs::create_dir_all(&config_dir).expect("create Config");
        std::fs::create_dir_all(&mod_lists).expect("create ModLists");
        let session = rim_session::Session::new(
            rim_session::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: config_dir.join("ModsConfig.xml"),
                profile_dir: "profile".into(),
            },
            rim_session::test_support::report_fixture(&["a"]),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            rim_session::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let path = suggested_mod_list_path_inner(&state)
            .await
            .expect("a project is loaded")
            .expect("the folder exists");

        assert_eq!(
            PathBuf::from(path),
            mod_lists.join("rimmerge-load-order.rml")
        );
    }

    // --- previews ---

    #[tokio::test]
    async fn preview_text_diffs_pasted_text_against_the_install() {
        let (_scratch, state, _paths) = loaded_game().await;
        let text = "zzz.mod\nludeon.rimworld\n\
                    Ghost [ghost.mod] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>\n";

        let preview = ready(
            preview_order_import_text_inner(&state, text_request(text))
                .await
                .expect("previews"),
        );

        assert_eq!(preview.order, mod_ids(&["zzz.mod", "ludeon.rimworld"]));
        assert_eq!(preview.listed, 3);
        assert_eq!(preview.deactivated.len(), 1);
        assert_eq!(preview.deactivated[0].mod_id, "sample.mod");
        assert_eq!(preview.deactivated[0].name, "Sample Mod");
        assert!(matches!(
            &preview.entries[0],
            ImportedEntryDto::Activated { id, name } if id == "zzz.mod" && name == "Zzz Mod"
        ));
        assert!(matches!(
            &preview.entries[2],
            ImportedEntryDto::NotInstalled {
                missing: MissingKindDto::Workshop {
                    workshop_id: 1_234_567_890
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn import_is_blocked_as_nothing_installed_when_only_core_would_be_in_the_order() {
        let (_scratch, state, _paths) = loaded_game().await;
        let installed = ready(
            preview_order_import_text_inner(&state, text_request("ludeon.rimworld\nsample.mod\n"))
                .await
                .expect("previews"),
        );
        let nothing_installed = ready(
            preview_order_import_text_inner(&state, text_request("ghost.mod\n"))
                .await
                .expect("previews"),
        );

        assert_eq!(installed.import_blocked, None);
        // Core is added first, but importing only Core would deactivate
        // every other mod.
        assert_eq!(
            nothing_installed.import_blocked,
            Some(ImportBlockedDto::NothingInstalled)
        );
        assert_eq!(nothing_installed.order, mod_ids(&[CORE]));
    }

    #[tokio::test]
    async fn import_is_blocked_as_core_missing_when_the_install_has_no_core() {
        let (_scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch sample game must succeed");

        let preview = ready(
            preview_order_import_text_inner(&state, text_request("sample.mod\n"))
                .await
                .expect("previews"),
        );

        assert_eq!(preview.core, CorePlacementDto::Missing);
        assert_eq!(preview.import_blocked, Some(ImportBlockedDto::CoreMissing));
    }

    #[tokio::test]
    async fn preview_text_turns_an_empty_paste_into_a_rejected_outcome() {
        let (_scratch, state, _paths) = loaded_game().await;

        let outcome = preview_order_import_text_inner(&state, text_request(""))
            .await
            .expect("a rejection is an outcome, not an error");

        assert!(matches!(
            outcome,
            OrderImportOutcomeDto::Rejected {
                reason: ImportRejectionDto::NoEntries
            }
        ));
    }

    #[tokio::test]
    async fn preview_text_rejects_an_oversized_paste() {
        let (_scratch, state, _paths) = loaded_game().await;
        let text = "a".repeat(ModListLimits::MAX_INPUT_BYTES + 1);

        let outcome = preview_order_import_text_inner(&state, text_request(&text))
            .await
            .expect("a rejection is an outcome");

        assert!(matches!(
            outcome,
            OrderImportOutcomeDto::Rejected {
                reason: ImportRejectionDto::TooLarge { .. }
            }
        ));
    }

    #[tokio::test]
    async fn preview_file_reads_a_text_file_and_rejects_a_dtd() {
        let (scratch, state, _paths) = loaded_game().await;
        let listed = scratch.path().join("list.txt");
        std::fs::write(&listed, "ludeon.rimworld\nsample.mod\n").expect("write list");
        let evil = scratch.path().join("evil.xml");
        std::fs::write(
            &evil,
            "<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///nowhere\">]><savedModList/>",
        )
        .expect("write dtd file");

        let from_text_file = ready(
            preview_order_import_file_inner(&state, file_request(&listed))
                .await
                .expect("previews"),
        );
        let from_dtd_file = preview_order_import_file_inner(&state, file_request(&evil))
            .await
            .expect("a rejection is an outcome");

        assert_eq!(
            from_text_file.order,
            mod_ids(&["ludeon.rimworld", "sample.mod"])
        );
        assert!(matches!(
            from_dtd_file,
            OrderImportOutcomeDto::Rejected {
                reason: ImportRejectionDto::DtdNotAllowed
            }
        ));
    }

    #[tokio::test]
    async fn preview_file_reports_a_missing_file_as_mod_list_io_failed() {
        let (scratch, state, _paths) = loaded_game().await;

        let error = preview_order_import_file_inner(
            &state,
            file_request(&scratch.path().join("absent.rml")),
        )
        .await
        .expect_err("nothing there");

        assert_eq!(error.code, CommandErrorCode::ModListIoFailed);
    }

    #[tokio::test]
    async fn preview_file_refuses_a_path_that_is_not_a_regular_file() {
        let (scratch, state, _paths) = loaded_game().await;

        let error = preview_order_import_file_inner(&state, file_request(scratch.path()))
            .await
            .expect_err("a directory is not a mod list");

        assert_eq!(error.code, CommandErrorCode::ModListIoFailed);
        assert!(
            error.message.contains("not a regular file"),
            "{}",
            error.message
        );
    }

    #[tokio::test]
    async fn preview_reports_pending_activation_edits_as_replaced() {
        let (_scratch, state, _paths) = loaded_game().await;
        crate::commands::active_set::activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: mod_ids(&["zzz.mod"]),
                with_dependencies: false,
            },
        )
        .await
        .expect("activates");

        let preview = ready(
            preview_order_import_text_inner(&state, text_request("ludeon.rimworld\n"))
                .await
                .expect("previews"),
        );

        assert!(preview.replaces_pending_changes);
    }

    // --- import ---

    #[tokio::test]
    async fn import_order_swaps_in_a_session_whose_current_and_selection_are_the_order() {
        let (_scratch, state, _paths) = loaded_game().await;
        let order = ["zzz.mod", CORE, "sample.mod"];

        let summary = import_order_inner(&state, import_request(&order), |_| {})
            .await
            .expect("imports");

        assert_eq!(summary.selected, OrderSourceDto::Current);
        let facts = session_facts(&state);
        assert_eq!(facts.current, mod_ids(&order));
        assert_eq!(facts.selected, OrderSource::Current);
        assert_eq!(facts.working, mod_ids(&order));
        let guard = state.session.read().expect("lock");
        assert!(
            !guard.as_ref().expect("a session").is_stale(),
            "the imported order is not a staged activation edit"
        );
    }

    #[tokio::test]
    async fn an_imported_order_is_not_in_the_file_until_apply() {
        let (_scratch, state, _paths) = loaded_game().await;
        import_order_inner(
            &state,
            import_request(&["zzz.mod", CORE, "sample.mod"]),
            |_| {},
        )
        .await
        .expect("imports");

        let dashboard = crate::commands::dashboard::get_dashboard_inner(&state)
            .await
            .expect("a project is loaded");

        assert!(!dashboard.file_matches_current);
    }

    #[tokio::test]
    async fn import_order_refuses_an_unknown_id_before_any_scan() {
        let (_scratch, state, _paths) = loaded_game().await;
        let before = session_facts(&state);
        let has_scanned = Arc::new(AtomicBool::new(false));
        let scanned_flag = Arc::clone(&has_scanned);

        let error = import_order_inner(&state, import_request(&[CORE, "ghost.mod"]), move |_| {
            scanned_flag.store(true, Ordering::SeqCst);
        })
        .await
        .expect_err("ghost.mod is not installed");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert!(
            !has_scanned.load(Ordering::SeqCst),
            "validation must fail before the scan starts"
        );
        assert_eq!(session_facts(&state), before);
    }

    #[tokio::test]
    async fn import_order_refuses_an_order_without_core_and_a_repeated_id() {
        let (_scratch, state, _paths) = loaded_game().await;

        let without_core = import_order_inner(&state, import_request(&["sample.mod"]), |_| {})
            .await
            .expect_err("no Core");
        let repeated = import_order_inner(
            &state,
            import_request(&[CORE, "sample.mod", "sample.mod"]),
            |_| {},
        )
        .await
        .expect_err("repeats");

        assert_eq!(without_core.code, CommandErrorCode::InvalidInput);
        assert_eq!(repeated.code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn import_order_refuses_an_order_one_over_the_bound() {
        let (_scratch, state, _paths) = loaded_game().await;
        let order = vec![CORE.to_string(); ModListLimits::MAX_ENTRIES + 3];

        let error = import_order_inner(&state, ImportOrderRequestDto { order }, |_| {})
            .await
            .expect_err("too many ids");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert!(
            error
                .message
                .contains(&format!("at most {} mods", ModListLimits::MAX_ENTRIES + 2)),
            "refused for its size, not for a repeated id: {}",
            error.message
        );
    }

    /// Core, the receiver's own merge mod (active in the file) and a list of
    /// `MAX_ENTRIES` other mods: the largest order a plan can emit.
    ///
    /// Every path is absolute inside the returned temp dir and none exists,
    /// so the scan's failure cannot depend on the working directory.
    fn state_with_the_largest_plannable_order() -> (tempfile::TempDir, AppState, String) {
        let scratch = tempfile::tempdir().expect("tempdir");
        let paths = rim_session::ProjectPaths {
            game_dir: scratch.path().join("game"),
            workshop_dir: scratch.path().join("workshop"),
            mods_config: scratch.path().join("Config").join("ModsConfig.xml"),
            profile_dir: scratch.path().join("profile"),
        };
        let own =
            rim_resolve::domain::GeneratedModIdentity::for_profile(paths.profile_hash()).package_id;
        let listed: Vec<String> = (0..ModListLimits::MAX_ENTRIES)
            .map(|index| format!("example.m{index}"))
            .collect();
        let active: Vec<&str> = [CORE, own.as_str()]
            .into_iter()
            .chain(listed.iter().map(String::as_str))
            .collect();
        let session = rim_session::Session::new(
            paths,
            rim_session::test_support::report_fixture(&active),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            rim_session::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: active.iter().map(ModId::new).collect(),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        (scratch, state, listed.join("\n"))
    }

    #[tokio::test]
    async fn the_preview_and_import_order_agree_at_the_largest_plannable_order() {
        let (_scratch, state, text) = state_with_the_largest_plannable_order();

        let preview = ready(
            preview_order_import_text_inner(&state, text_request(&text))
                .await
                .expect("previews"),
        );
        let outcome = import_order_inner(
            &state,
            ImportOrderRequestDto {
                order: preview.order.clone(),
            },
            |_| {},
        )
        .await;

        assert_eq!(preview.order.len(), ModListLimits::MAX_ENTRIES + 2);
        assert_eq!(preview.import_blocked, None);
        // The order passes validation and reaches the scan, which then fails
        // reading the ModsConfig.xml this fixture session does not have on
        // disk: an error of the scan, not a refusal of the order itself.
        let error = outcome.expect_err("there is no game to scan");
        assert_eq!(
            error.code,
            CommandErrorCode::ModsConfigIoFailed,
            "{}",
            error.message
        );
    }

    #[tokio::test]
    async fn import_order_refuses_when_the_working_set_changed_mid_scan() {
        let (_scratch, state, _paths) = loaded_game().await;
        let live_session = Arc::clone(&state.session);
        let mut has_edited = false;

        let error = import_order_inner(&state, import_request(&[CORE, "sample.mod"]), move |_| {
            if has_edited {
                return;
            }
            has_edited = true;
            let mut guard = live_session.write().expect("lock");
            let session = guard.as_mut().expect("the live session is still loaded");
            let plan = ActivateMods::plan(session, &[ModId::new("zzz.mod")], false);
            ActivateMods::execute(session, &plan).expect("activating zzz.mod must succeed");
        })
        .await
        .expect_err("a mid-scan edit must refuse the swap");

        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        assert_eq!(
            error.message,
            "the working active-mod set changed during the import; import again"
        );
        let facts = session_facts(&state);
        assert!(
            facts.working.iter().any(|id| id == "zzz.mod"),
            "the mid-scan edit survives the refusal"
        );
        assert_eq!(facts.selected, OrderSource::Suggested);
    }

    #[tokio::test]
    async fn the_session_is_unchanged_when_the_import_scan_fails() {
        let (_scratch, state, paths) = loaded_game().await;
        let before = session_facts(&state);
        // A real failure rather than a seam: the scanner needs the game's
        // version file, so removing it after the load makes the scan fail.
        std::fs::remove_file(Path::new(&paths.game_dir).join("Version.txt"))
            .expect("remove Version.txt");

        let error = import_order_inner(
            &state,
            import_request(&["zzz.mod", CORE, "sample.mod"]),
            |_| {},
        )
        .await
        .expect_err("the scan cannot run");

        assert_eq!(error.code, CommandErrorCode::ScanFailed);
        assert_eq!(session_facts(&state), before);
    }

    // --- the round trip through the commands ---

    #[tokio::test]
    async fn exporting_then_importing_your_own_list_is_an_empty_diff_and_stages_nothing() {
        let (scratch, state, _paths) = loaded_game().await;
        let target = scratch.path().join("mine.rml");
        export_order_file_inner(
            &state,
            ExportOrderFileRequestDto {
                path: target.display().to_string(),
            },
        )
        .await
        .expect("exports");

        let preview = ready(
            preview_order_import_file_inner(&state, file_request(&target))
                .await
                .expect("previews"),
        );

        assert_eq!(preview.order, mod_ids(&[CORE, "sample.mod"]));
        assert!(preview.deactivated.is_empty());
        assert_eq!(preview.moved, 0);
        assert!(!preview.replaces_pending_changes);
        assert!(
            preview
                .entries
                .iter()
                .all(|entry| matches!(entry, ImportedEntryDto::AlreadyActive { .. })),
            "{:?}",
            preview.entries
        );
        import_order_inner(
            &state,
            ImportOrderRequestDto {
                order: preview.order,
            },
            |_| {},
        )
        .await
        .expect("imports");
        assert!(
            nothing_is_staged_or_unapplied(&state).await,
            "an import of your own list stages and leaves unapplied nothing"
        );
        let dashboard = crate::commands::dashboard::get_dashboard_inner(&state)
            .await
            .expect("a project is loaded");
        assert!(dashboard.file_matches_current);
    }

    // --- the Workshop link ---

    fn state_with_recording_opener() -> (AppState, Arc<RecordingLinkOpener>) {
        let opener = Arc::new(RecordingLinkOpener::default());
        let state = AppState {
            link_opener: opener.clone(),
            ..AppState::default()
        };
        (state, opener)
    }

    #[tokio::test]
    async fn open_workshop_page_opens_the_fixed_url_for_an_id() {
        let (state, opener) = state_with_recording_opener();

        open_workshop_page_inner(
            &state,
            OpenWorkshopPageRequestDto {
                workshop_id: "1234567890".to_string(),
            },
        )
        .await
        .expect("opens");

        assert_eq!(
            opener.calls(),
            vec!["https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890".to_string()]
        );
    }

    #[tokio::test]
    async fn open_workshop_page_refuses_zero_and_urls_and_opens_nothing() {
        let (state, opener) = state_with_recording_opener();

        for bad in ["0", "", "https://example.invalid/", "12a", "-5"] {
            let error = open_workshop_page_inner(
                &state,
                OpenWorkshopPageRequestDto {
                    workshop_id: bad.to_string(),
                },
            )
            .await
            .expect_err(bad);
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "{bad}");
        }

        assert!(opener.calls().is_empty());
    }

    // --- no project ---

    #[tokio::test]
    async fn every_session_command_reports_no_project_loaded() {
        let state = AppState::default();
        let export = ExportOrderFileRequestDto {
            path: "a.rml".to_string(),
        };

        let codes = [
            export_order_text_inner(&state).await.unwrap_err().code,
            suggested_mod_list_path_inner(&state)
                .await
                .unwrap_err()
                .code,
            preview_order_import_text_inner(&state, text_request("a.b"))
                .await
                .unwrap_err()
                .code,
            preview_order_import_file_inner(&state, file_request(Path::new("a.rml")))
                .await
                .unwrap_err()
                .code,
            export_order_file_inner(&state, export)
                .await
                .unwrap_err()
                .code,
            import_order_inner(&state, import_request(&[CORE]), |_| {})
                .await
                .unwrap_err()
                .code,
        ];

        assert!(
            codes
                .iter()
                .all(|code| *code == CommandErrorCode::NoProjectLoaded)
        );
    }
}
