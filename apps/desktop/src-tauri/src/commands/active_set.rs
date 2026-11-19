//! `list_inactive_mods`/`plan_activate_mods`/`activate_mods`/
//! `plan_deactivate_mods`/`deactivate_mods`/`move_mods`/
//! `discard_pending_changes`/`revert_order_to_file`/
//! `get_pending_active_changes`/`rescan_project`: the Mods page's Inactive
//! list, the working active-mod set, and re-scanning it.
//!
//! Every rule lives in `rim-session` (`ActiveSet`, `ActivateMods`,
//! `DeactivateMods`, `MoveMods`, `DiscardPendingChanges`,
//! `RevertOrderToFile`, `Rescan`) — these commands only map DTOs in and out
//! and, for the mutators plus `rescan_project`, emit `session://changed`.

use std::time::Instant;

use rim_analyzer::domain::ModId;
use rim_session::use_cases::{
    ActivateMods, DeactivateMods, DiscardPendingChanges, ImportOrder, LoadProject, MoveMods,
    RevertOrderToFile, ValidatedImport,
};
use rim_session::{Anchor, ProjectPaths, Session};
use tauri::async_runtime::spawn_blocking;

use crate::commands::project::finish_loaded_session;
use crate::commands::{EVENT_PROJECT_PROGRESS, emit_session_changed};
use crate::dto::active_set::{
    ActivateModsResultDto, ActivatePlanDto, ActivateRequestDto, DeactivatePlanDto,
    DeactivateRequestDto, MoveModsResultDto, MoveRequestDto, PendingActiveChangesDto,
};
use crate::dto::mod_lists::{ModListDto, ModListQueryDto};
use crate::dto::project::{ProgressEventDto, ProjectSummaryDto, SessionChangeReasonDto};
use crate::error::CommandError;
use crate::state::{Adapters, AppState, with_session};

/// The Mods page's Inactive list: every mod on disk whose base id is not
/// in the working set, so a mod deactivated since the last scan appears at
/// once. Filtering lives in [`rim_session::Session::inactive_mod_list`];
/// this only maps the DTO in and out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::invalid_input`] when `query.tag` isn't a valid tag
/// slug (accepted for symmetry with the Active list's query, though the
/// Inactive list ignores it).
pub(crate) async fn list_inactive_mods_inner(
    state: &AppState,
    query: ModListQueryDto,
) -> Result<ModListDto, CommandError> {
    with_session(state, move |session| {
        let query: rim_session::ModListQuery = query.try_into()?;
        Ok(session.inactive_mod_list(&query).into())
    })
    .await
}

/// See [`list_inactive_mods_inner`].
///
/// # Errors
///
/// See [`list_inactive_mods_inner`].
#[tauri::command]
pub async fn list_inactive_mods(
    state: tauri::State<'_, AppState>,
    query: ModListQueryDto,
) -> Result<ModListDto, CommandError> {
    list_inactive_mods_inner(&state, query).await
}

/// Plans what `activate_mods` would do — the confirm dialog's own read,
/// before anything is mutated. A stale anchor is refused here too, so the
/// dialog never offers a commit that is certain to fail.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded, or
/// `move_anchor_not_active` when the request's anchor is not active.
pub(crate) async fn plan_activate_mods_inner(
    state: &AppState,
    request: ActivateRequestDto,
) -> Result<ActivatePlanDto, CommandError> {
    with_session(state, move |session| {
        if let Some(anchor) = request.anchor.clone() {
            session.working().check_anchor(&Anchor::from(anchor))?;
        }
        let plan = ActivateMods::plan(session, &request.mod_ids(), request.with_dependencies);
        Ok((&plan).into())
    })
    .await
}

/// See [`plan_activate_mods_inner`].
///
/// # Errors
///
/// See [`plan_activate_mods_inner`].
#[tauri::command]
pub async fn plan_activate_mods(
    state: tauri::State<'_, AppState>,
    request: ActivateRequestDto,
) -> Result<ActivatePlanDto, CommandError> {
    plan_activate_mods_inner(&state, request).await
}

/// Commits an activation onto the working set. Re-plans against the
/// session's current report rather than trusting a plan the caller might
/// have shown the user earlier — [`ActivateMods::execute`]'s own contract
/// is exactly this: a plan reused after the report has moved on leaves
/// the working set untouched instead of partially applied, and
/// re-planning here means the two can never actually disagree.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when an id in `request.ids` isn't known
/// to the current report, or `move_anchor_not_active` for a stale anchor.
/// An anchored activation also switches a selected Suggested to Current
/// (the result says so); an un-anchored one never changes the selection.
pub(crate) async fn activate_mods_inner(
    state: &AppState,
    request: ActivateRequestDto,
) -> Result<ActivateModsResultDto, CommandError> {
    with_session(state, move |session| {
        let plan = ActivateMods::plan(session, &request.mod_ids(), request.with_dependencies);
        let outcome = match request.anchor {
            Some(anchor) => Some(ActivateMods::execute_at(session, &plan, &anchor.into())?),
            None => {
                ActivateMods::execute(session, &plan)?;
                None
            }
        };
        Ok(ActivateModsResultDto::new(
            &session.pending_changes(),
            outcome.as_ref(),
            session.selected(),
        ))
    })
    .await
}

/// See [`activate_mods_inner`]; also emits `session://changed` with
/// [`SessionChangeReasonDto::ActiveSetChanged`] on success.
///
/// # Errors
///
/// See [`activate_mods_inner`].
#[tauri::command]
pub async fn activate_mods(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ActivateRequestDto,
) -> Result<ActivateModsResultDto, CommandError> {
    let result = activate_mods_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::ActiveSetChanged);
    }
    result
}

/// Plans what `deactivate_mods` would do.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn plan_deactivate_mods_inner(
    state: &AppState,
    request: DeactivateRequestDto,
) -> Result<DeactivatePlanDto, CommandError> {
    with_session(state, move |session| {
        let plan = DeactivateMods::plan(session, &request.mod_ids());
        Ok((&plan).into())
    })
    .await
}

/// See [`plan_deactivate_mods_inner`].
///
/// # Errors
///
/// See [`plan_deactivate_mods_inner`].
#[tauri::command]
pub async fn plan_deactivate_mods(
    state: tauri::State<'_, AppState>,
    request: DeactivateRequestDto,
) -> Result<DeactivatePlanDto, CommandError> {
    plan_deactivate_mods_inner(&state, request).await
}

/// Commits a deactivation onto the working set — re-plans first, the same
/// as [`activate_mods_inner`]; unlike activation this is infallible
/// (`DeactivateMods::execute` returns no `Result`), since a since-become-
/// unknown id simply isn't in the working set to remove.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn deactivate_mods_inner(
    state: &AppState,
    request: DeactivateRequestDto,
) -> Result<PendingActiveChangesDto, CommandError> {
    with_session(state, move |session| {
        let plan = DeactivateMods::plan(session, &request.mod_ids());
        DeactivateMods::execute(session, &plan);
        Ok((&session.pending_changes()).into())
    })
    .await
}

/// See [`deactivate_mods_inner`]; also emits `session://changed` with
/// [`SessionChangeReasonDto::ActiveSetChanged`] on success.
///
/// # Errors
///
/// See [`deactivate_mods_inner`].
#[tauri::command]
pub async fn deactivate_mods(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DeactivateRequestDto,
) -> Result<PendingActiveChangesDto, CommandError> {
    let result = deactivate_mods_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::ActiveSetChanged);
    }
    result
}

/// Moves active mods within the working set. When the ids then equal
/// Current's, Current takes the new sequence (nothing is written); the first
/// reorder while Suggested is selected switches the selection to Current,
/// which the result reports.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// `move_anchor_not_active` for a stale anchor, or
/// [`CommandError::invalid_input`] for an id that is not active, repeated, or
/// the anchor itself.
pub(crate) async fn move_mods_inner(
    state: &AppState,
    request: MoveRequestDto,
) -> Result<MoveModsResultDto, CommandError> {
    with_session(state, move |session| {
        let outcome = MoveMods::execute(session, &request.mod_ids(), &request.anchor.into())?;
        Ok(MoveModsResultDto::new(outcome, session.selected()))
    })
    .await
}

/// See `move_mods_inner`; also emits `session://changed` with
/// [`SessionChangeReasonDto::ActiveSetChanged`] on success.
///
/// # Errors
///
/// See `move_mods_inner`.
#[tauri::command]
pub async fn move_mods(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: MoveRequestDto,
) -> Result<MoveModsResultDto, CommandError> {
    let result = move_mods_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::ActiveSetChanged);
    }
    result
}

/// Throws away every unscanned edit: the working set becomes Current again,
/// with no rescan. A reorder that already settled into Current stays (that
/// is `revert_order_to_file`).
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn discard_pending_changes_inner(
    state: &AppState,
) -> Result<PendingActiveChangesDto, CommandError> {
    with_session(state, |session| {
        DiscardPendingChanges::execute(session);
        Ok((&session.pending_changes()).into())
    })
    .await
}

/// See `discard_pending_changes_inner`; also emits `session://changed`
/// with [`SessionChangeReasonDto::ActiveSetChanged`] on success.
///
/// # Errors
///
/// See `discard_pending_changes_inner`.
#[tauri::command]
pub async fn discard_pending_changes(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PendingActiveChangesDto, CommandError> {
    let result = discard_pending_changes_inner(&state).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::ActiveSetChanged);
    }
    result
}

/// Puts Current's sequence back to what `ModsConfig.xml` holds, when both
/// list the same mods. Writes nothing.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// `revert_order_pending_changes` while the active list has unscanned edits,
/// or `revert_order_membership_differs` when the file lists other mods.
pub(crate) async fn revert_order_to_file_inner(
    state: &AppState,
) -> Result<PendingActiveChangesDto, CommandError> {
    with_session(state, |session| {
        RevertOrderToFile::execute(session)?;
        Ok((&session.pending_changes()).into())
    })
    .await
}

/// See `revert_order_to_file_inner`; also emits `session://changed` with
/// [`SessionChangeReasonDto::ActiveSetChanged`] on success.
///
/// # Errors
///
/// See `revert_order_to_file_inner`.
#[tauri::command]
pub async fn revert_order_to_file(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PendingActiveChangesDto, CommandError> {
    let result = revert_order_to_file_inner(&state).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::ActiveSetChanged);
    }
    result
}

/// The working set's own pending-change summary — what
/// [`crate::commands::mods`] the Mods page's Rescan banner and the apply
/// dialog's own stale/unapplied warnings both read.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_pending_active_changes_inner(
    state: &AppState,
) -> Result<PendingActiveChangesDto, CommandError> {
    with_session(state, |session| Ok((&session.pending_changes()).into())).await
}

/// See [`get_pending_active_changes_inner`].
///
/// # Errors
///
/// See [`get_pending_active_changes_inner`].
#[tauri::command]
pub async fn get_pending_active_changes(
    state: tauri::State<'_, AppState>,
) -> Result<PendingActiveChangesDto, CommandError> {
    get_pending_active_changes_inner(&state).await
}

/// What [`rescan_with`] scans, and so what the swapped-in session selects.
///
/// One enum rather than a list choice and a selection choice: the two only
/// make sense in these pairs (a rescan keeps the user's selection, an
/// import selects the imported order as Current), and two independent
/// enums would allow the mixed pairs that mean nothing.
#[derive(Debug)]
pub(crate) enum ScanRequest {
    /// Scans the live session's own working set and keeps the user's
    /// selection — what `rescan_project` does.
    Rescan,
    /// Scans an imported order, unvalidated until [`rescan_with`] checks
    /// it inside the lock, and selects Current — what `import_order` does.
    Import(Vec<ModId>),
}

/// A [`ScanRequest`] after phase 1: an import's order has passed
/// [`ImportOrder::validate`], so the swap can prove the scan matched it.
#[derive(Debug)]
enum ValidatedScan {
    Rescan,
    Import(ValidatedImport),
}

impl ValidatedScan {
    /// Validates `request` against the live `session`.
    fn prepare(request: ScanRequest, session: &Session) -> Result<Self, CommandError> {
        match request {
            ScanRequest::Rescan => Ok(Self::Rescan),
            ScanRequest::Import(order) => Ok(Self::Import(ImportOrder::validate(session, order)?)),
        }
    }

    /// The active list to scan: the working set, or the validated order.
    fn scan_ids(&self, working: &[ModId]) -> Vec<ModId> {
        match self {
            Self::Rescan => working.to_vec(),
            Self::Import(validated) => validated.order().to_vec(),
        }
    }

    /// The refusal for a working set that changed while the scan ran.
    fn working_changed_error(&self) -> CommandError {
        CommandError::invalid_input(match self {
            Self::Rescan => "the working active-mod set changed during the rescan; rescan again",
            Self::Import(_) => "the working active-mod set changed during the import; import again",
        })
    }

    /// Sets the selection of `new_session`, the scan's result, before it
    /// replaces `live`. A rescan carries `live`'s selection over: it is
    /// read here, at the swap, not snapshotted before the scan, so a
    /// `select_order` made mid-scan survives. An import selects Current
    /// through [`ImportOrder::finish`], which also checks that the scan
    /// really produced the validated order.
    fn select_on(self, new_session: &mut Session, live: &Session) -> Result<(), CommandError> {
        match self {
            Self::Rescan => new_session.select(live.selected()),
            Self::Import(validated) => ImportOrder::finish(new_session, &validated)?,
        }
        Ok(())
    }
}

/// Re-scans the session's own working set: [`rescan_with`] over
/// [`ScanRequest::Rescan`], keeping the user's selection.
///
/// # Errors
///
/// See [`rescan_with`].
pub(crate) async fn rescan_project_inner(
    state: &AppState,
    on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<ProjectSummaryDto, CommandError> {
    rescan_with(state, ScanRequest::Rescan, on_progress).await
}

/// Scans `request` and swaps the result in as the live session:
/// [`LoadProject::execute_with_active_set`] over the request's ids, which
/// re-reads decisions/rules/patches/assignments from disk exactly like an
/// ordinary [`LoadProject::execute`] would (safe, because the working set
/// is this crate's only in-memory-only state).
///
/// **Three phases, not one `with_session` call** — running the whole scan
/// inside `with_session`'s own closure would have two real costs a rescan
/// (multi-second, unlike every other command here) can't absorb:
/// `with_session` wraps `f` in `catch_unwind` specifically so a panic
/// discards only that call's session, but a scan panic there would discard
/// the *already-loaded* session — including its working set — for a rescan
/// that never even needs to touch it beyond reading two fields up front;
/// and nothing would serialize two concurrent rescans (or a rescan racing
/// `load_project`) the way
/// [`AppState::load_lock`][crate::state::AppState::load_lock] already
/// does for an ordinary load. Instead: (1) a short `with_session` call
/// validates the request, snapshots `(paths, working ids)` and releases
/// the lock immediately; (2) the real scan runs in a bare
/// [`spawn_blocking`], under `load_lock`, touching no live `Session` at
/// all — a panic there can only fail this call, never poison anything
/// already loaded; (3) a second short `with_session` call swaps the new
/// session in, but only if `session.working().ids()` still matches the
/// phase-1 snapshot — otherwise something else
/// (`activate_mods`/`deactivate_mods`) changed the working set while the
/// scan was running, and swapping in a session scanned against the *old*
/// set would silently discard that change, so this refuses instead and
/// asks for a retry.
///
/// An import's validation happens in phase 1, with `load_lock` already
/// held, so validate, scan and swap all happen under one `load_lock`: no
/// other load or rescan can change the inventory the order was checked
/// against. A validation error returns before any scan.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when an imported order fails validation
/// or the working set changed while the scan was running, or
/// [`CommandError`] when the scan itself fails (see
/// [`rim_session::use_cases::LoadProjectError`]).
pub(crate) async fn rescan_with(
    state: &AppState,
    request: ScanRequest,
    on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<ProjectSummaryDto, CommandError> {
    let _permit = state.load_lock.lock().await;

    let (paths, working_ids, scan) = with_session(state, move |session| {
        let scan = ValidatedScan::prepare(request, session)?;
        Ok((
            session.paths().clone(),
            session.working().ids().to_vec(),
            scan,
        ))
    })
    .await?;
    let scan_ids = scan.scan_ids(&working_ids);

    let (new_session, mut summary) =
        scan_detached(state.adapters.clone(), paths, scan_ids, on_progress).await?;

    let selected = with_session(state, move |session| {
        if session.working().ids() != working_ids.as_slice() {
            return Err(scan.working_changed_error());
        }
        let mut new_session = new_session;
        scan.select_on(&mut new_session, session)?;
        let selected = new_session.selected();
        *session = new_session;
        Ok(selected)
    })
    .await?;
    summary.selected = selected.into();

    Ok(summary)
}

/// Phase 2 of [`rescan_with`]: scans `active` in a bare [`spawn_blocking`]
/// that touches no live `Session`, so a panic there only fails this call.
///
/// # Errors
///
/// Returns [`CommandError`] when the scan fails, or
/// [`CommandError::internal`] when the scan task panicked.
async fn scan_detached(
    adapters: Adapters,
    paths: ProjectPaths,
    active: Vec<ModId>,
    mut on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<(Session, ProjectSummaryDto), CommandError> {
    spawn_blocking(move || {
        let use_case = LoadProject::new(
            adapters.scanner,
            adapters.config_store,
            adapters.decision_store,
            adapters.rule_store,
            adapters.patch_store,
            adapters.assignment_store,
            adapters.mod_knowledge_store.clone(),
            crate::commands::rules_databases::network_policy().fetch_rimmerge_rules,
        )
        .with_timestamp_reader(adapters.mod_timestamp_reader.clone());

        let start = Instant::now();
        let def_reader = adapters.def_reader.clone();
        let mut session = use_case.execute_with_active_set(paths, active, &mut |progress| {
            on_progress(progress.into());
        })?;
        let summary = finish_loaded_session(&mut session, def_reader, start);

        Ok::<_, CommandError>((session, summary))
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "session scan task panicked");
        Err(CommandError::internal("background task panicked"))
    })
}

/// See [`rescan_project_inner`]; the [`tauri::AppHandle`] here only
/// exists to emit [`EVENT_PROJECT_PROGRESS`] while the scan runs, then
/// `session://changed` with [`SessionChangeReasonDto::Rescanned`] on
/// success — same shape as [`crate::commands::project::load_project`].
///
/// # Errors
///
/// See [`rescan_project_inner`].
#[tauri::command]
pub async fn rescan_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectSummaryDto, CommandError> {
    use tauri::Emitter;

    let progress_app = app.clone();
    let result = rescan_project_inner(&state, move |progress| {
        let _ = progress_app.emit(EVENT_PROJECT_PROGRESS, progress);
    })
    .await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Rescanned);
    }
    result
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_session::test_support::report_fixture_with_inactive;

    use super::*;
    use crate::dto::mod_lists::{
        ListSortDto, ListSortKeyDto, ModListQueryDto, SortDirectionDto, UpdateTimeDto,
    };
    use crate::dto::mods::ModFilterDto;

    fn state_with(active: &[&str], inactive: &[&str]) -> AppState {
        let state = AppState::default();
        let report = report_fixture_with_inactive(active, inactive);
        let session = rim_session::Session::new(
            rim_session::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            rim_session::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: active.iter().map(|id| ModId::new(*id)).collect(),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        *state.session.write().expect("lock") = Some(session);
        state
    }

    #[tokio::test]
    async fn list_inactive_mods_lists_only_inactive_mods_sorted_by_id() {
        let state = state_with(&["a"], &["zzz.mod", "aaa.mod"]);

        let list = list_inactive_mods_inner(&state, ModListQueryDto::default())
            .await
            .expect("must succeed");

        assert_eq!(list.total, 2);
        assert_eq!(list.items[0].mod_id, "aaa.mod");
        assert_eq!(list.items[1].mod_id, "zzz.mod");
    }

    /// Sets the modification time of a sample game local mod's `About.xml`.
    fn set_about_modified(game_dir: &str, folder: &str, seconds: u64) {
        use std::time::{Duration, SystemTime};

        let about = std::path::Path::new(game_dir)
            .join("Mods")
            .join(folder)
            .join("About")
            .join("About.xml");
        std::fs::File::options()
            .write(true)
            .open(about)
            .expect("sample About.xml")
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("set mtime");
    }

    /// Loads the sample game with `ZzzMod`'s `About.xml` older than
    /// `AaaMod`'s, then returns the scratch folder, its paths and the state.
    async fn state_with_dated_local_mods() -> (
        tempfile::TempDir,
        crate::dto::project::ProjectPathsDto,
        AppState,
    ) {
        let (scratch, paths) = crate::test_support::scratch_sample_game();
        for (folder, seconds) in [("ZzzMod", 1_000), ("AaaMod", 2_000)] {
            set_about_modified(&paths.game_dir, folder, seconds);
        }
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths.clone(), |_| {})
            .await
            .expect("loading the scratch sample game must succeed");
        (scratch, paths, state)
    }

    fn last_updated(direction: SortDirectionDto) -> ModListQueryDto {
        ModListQueryDto {
            sort: Some(ListSortDto {
                key: ListSortKeyDto::LastUpdated,
                direction,
            }),
            ..ModListQueryDto::default()
        }
    }

    #[tokio::test]
    async fn list_inactive_mods_sorts_by_the_about_xml_modification_time() {
        let (_scratch, _paths, state) = state_with_dated_local_mods().await;

        let ascending = list_inactive_mods_inner(&state, last_updated(SortDirectionDto::Ascending))
            .await
            .expect("must succeed");
        let descending =
            list_inactive_mods_inner(&state, last_updated(SortDirectionDto::Descending))
                .await
                .expect("must succeed");

        let position = |list: &ModListDto, id: &str| {
            list.items
                .iter()
                .position(|row| row.mod_id == id)
                .expect("the mod is inactive in the sample game")
        };
        assert!(position(&ascending, "zzz.mod") < position(&ascending, "aaa.mod"));
        assert!(position(&descending, "aaa.mod") < position(&descending, "zzz.mod"));
        let aaa = &descending.items[position(&descending, "aaa.mod")];
        assert_eq!(aaa.updated, UpdateTimeDto::Known { seconds: 2_000 });
    }

    #[tokio::test]
    async fn a_rescan_refreshes_the_update_times() {
        let (_scratch, paths, state) = state_with_dated_local_mods().await;
        set_about_modified(&paths.game_dir, "AaaMod", 5_000);

        rescan_project_inner(&state, |_| {})
            .await
            .expect("the rescan must succeed");

        let list = list_inactive_mods_inner(&state, last_updated(SortDirectionDto::Descending))
            .await
            .expect("must succeed");
        let aaa = list
            .items
            .iter()
            .find(|row| row.mod_id == "aaa.mod")
            .expect("the mod is inactive in the sample game");
        assert_eq!(aaa.updated, UpdateTimeDto::Known { seconds: 5_000 });
    }

    #[tokio::test]
    async fn plan_activate_mods_plans_without_mutating_the_working_set() {
        let state = state_with(&["a"], &["b"]);

        let plan = plan_activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
                anchor: None,
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(plan.to_add, vec!["b".to_string()]);
        let pending = get_pending_active_changes_inner(&state)
            .await
            .expect("must succeed");
        assert!(
            pending.unscanned.added.is_empty(),
            "plan must not mutate the working set"
        );
    }

    #[tokio::test]
    async fn activate_mods_commits_and_reports_it_as_unscanned() {
        let state = state_with(&["a"], &["b"]);

        let result = activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
                anchor: None,
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(result.pending.unscanned.added, vec!["b".to_string()]);
    }

    #[tokio::test]
    async fn activate_mods_rejects_an_unknown_id() {
        let state = state_with(&["a"], &[]);

        let result = activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["ghost".to_string()],
                with_dependencies: false,
                anchor: None,
            },
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn plan_deactivate_mods_refuses_core() {
        let state = state_with(&["ludeon.rimworld", "a"], &[]);

        let plan = plan_deactivate_mods_inner(
            &state,
            DeactivateRequestDto {
                ids: vec!["ludeon.rimworld".to_string()],
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(plan.refused, vec!["ludeon.rimworld".to_string()]);
        assert!(plan.to_remove.is_empty());
    }

    #[tokio::test]
    async fn deactivate_mods_commits_and_reports_it_as_unscanned() {
        let state = state_with(&["a", "b"], &[]);

        let pending = deactivate_mods_inner(
            &state,
            DeactivateRequestDto {
                ids: vec!["b".to_string()],
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(pending.unscanned.removed, vec!["b".to_string()]);
    }

    #[tokio::test]
    async fn get_pending_active_changes_reflects_the_working_set() {
        let state = state_with(&["a"], &["b"]);
        activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
                anchor: None,
            },
        )
        .await
        .expect("must succeed");

        let pending = get_pending_active_changes_inner(&state)
            .await
            .expect("must succeed");

        assert_eq!(pending.unscanned.added, vec!["b".to_string()]);
        assert!(pending.unapplied.added.is_empty());
    }

    /// Once `activate_mods` has made
    /// the working set diverge from the last scan, `apply` (a sibling
    /// command, `commands::apply`) refuses to write `ModsConfig.xml` with
    /// [`crate::error::CommandErrorCode::StaleActiveSet`] until a rescan.
    /// Exercised through the real command surface, not by constructing
    /// `ApplyError` by hand, since what matters here is
    /// the *wiring* from one command's mutation to another's refusal.
    #[tokio::test]
    async fn activating_a_mod_makes_a_later_apply_refuse_as_stale() {
        let (_temp_dir, session) = crate::test_support::session_with_temp_paths(
            report_fixture_with_inactive(&["a"], &["b"]),
            rim_analyzer::analysis::SourceIndex::default(),
            &["a"],
        );
        // This test asserts `StaleActiveSet` specifically, which
        // `apply_inner` only reaches once its own `RimworldRunning` check
        // has already passed — `AppState::default`'s real
        // `SysinfoGameProcessProbe` would otherwise make this test's
        // outcome depend on whether RimWorld happens to be running on
        // whatever machine runs the default gate (`commands::apply`'s own
        // tests use the identical fixed-probe pattern for the same
        // reason).
        struct FixedProbe;
        impl crate::state::GameProcessProbe for FixedProbe {
            fn is_running(&self) -> bool {
                false
            }
        }
        let state = AppState {
            game_process_probe: std::sync::Arc::new(FixedProbe),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);

        activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
                anchor: None,
            },
        )
        .await
        .expect("activation must succeed");

        let result = crate::commands::apply::apply_inner(
            &state,
            crate::dto::settings::ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::StaleActiveSet
        );
    }

    #[tokio::test]
    async fn rescan_keeps_the_suggested_selection_in_the_session_and_the_summary() {
        let (_scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch sample game must succeed");

        let summary = rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");

        assert_eq!(
            summary.selected,
            crate::dto::common::OrderSourceDto::Suggested
        );
        let selected = state
            .session
            .read()
            .expect("lock")
            .as_ref()
            .map(rim_session::Session::selected);
        assert_eq!(
            selected,
            Some(rim_resolve::domain::OrderSource::Suggested),
            "the swapped-in session must not fall back to the current order"
        );
    }

    #[tokio::test]
    async fn rescan_keeps_a_current_selection_the_user_picked() {
        let (_scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch sample game must succeed");
        crate::commands::order::select_order_inner(
            &state,
            crate::dto::common::OrderSourceDto::Current,
        )
        .await
        .expect("a project is loaded");

        let summary = rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");

        assert_eq!(
            summary.selected,
            crate::dto::common::OrderSourceDto::Current
        );
    }

    async fn loaded_sample_state() -> (tempfile::TempDir, AppState) {
        let (scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState::default();
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch sample game must succeed");
        (scratch, state)
    }

    fn activate_request(id: &str) -> ActivateRequestDto {
        ActivateRequestDto {
            ids: vec![id.to_string()],
            with_dependencies: false,
            anchor: None,
        }
    }

    fn working_ids_of_session(session: &rim_session::Session) -> Vec<String> {
        session
            .working()
            .ids()
            .iter()
            .map(|id| id.as_str().to_string())
            .collect()
    }

    fn working_ids_of(state: &AppState) -> Vec<String> {
        let guard = state.session.read().expect("lock");
        working_ids_of_session(guard.as_ref().expect("a session is loaded"))
    }

    #[tokio::test]
    async fn rescan_swaps_in_a_session_scanned_from_the_working_set() {
        let (_scratch, state) = loaded_sample_state().await;
        activate_mods_inner(&state, activate_request("zzz.mod"))
            .await
            .expect("zzz.mod is installed but inactive, so it can be activated");

        rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");

        let pending = get_pending_active_changes_inner(&state)
            .await
            .expect("a project is loaded");
        assert!(
            pending.unscanned.added.is_empty(),
            "the swapped-in session was scanned from the working set"
        );
        let scanned = crate::commands::mods::list_mods_inner(
            &state,
            ModFilterDto {
                limit: 50,
                ..ModFilterDto::default()
            },
        )
        .await
        .expect("a project is loaded");
        assert!(
            scanned.items.iter().any(|item| item.mod_id == "zzz.mod"),
            "the scanned tier now includes the activated mod"
        );
    }

    /// Regression for the apply-then-rescan revert: `Apply` ends in
    /// `set_current_order`, and a rescan scans the working set's own
    /// sequence, so that sequence must have followed the written order.
    #[tokio::test]
    async fn rescan_after_the_current_order_changed_keeps_that_order() {
        let (_scratch, state) = loaded_sample_state().await;
        let reversed = {
            let mut guard = state.session.write().expect("lock");
            let session = guard.as_mut().expect("a session is loaded");
            let mut ids = session.orders().current.as_slice().to_vec();
            ids.reverse();
            session.set_current_order(rim_analyzer::domain::LoadOrder::new(ids.clone()));
            ids
        };

        rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");

        let guard = state.session.read().expect("lock");
        let rescanned = guard.as_ref().expect("a session is loaded");
        assert_eq!(rescanned.orders().current.as_slice(), reversed);
    }

    #[tokio::test]
    async fn list_working_mods_returns_the_working_set_in_its_own_order() {
        let (_scratch, state) = loaded_sample_state().await;
        activate_mods_inner(&state, activate_request("zzz.mod"))
            .await
            .expect("zzz.mod is installed but inactive, so it can be activated");

        let list =
            crate::commands::mods::list_working_mods_inner(&state, ModListQueryDto::default())
                .await
                .expect("a project is loaded");

        let listed: Vec<String> = list.items.iter().map(|row| row.mod_id.clone()).collect();
        assert_eq!(listed, working_ids_of(&state));
        let activated = list
            .items
            .iter()
            .find(|row| row.mod_id == "zzz.mod")
            .expect("the activated mod is listed at once");
        assert_eq!(
            activated.state,
            crate::dto::mod_lists::ModRowStateDto::PendingScan
        );
    }

    #[tokio::test]
    async fn rescan_refuses_when_the_working_set_changed_mid_scan() {
        let (_scratch, state) = loaded_sample_state().await;
        let live_session = std::sync::Arc::clone(&state.session);
        let mut has_edited = false;

        let result = rescan_project_inner(&state, move |_| {
            if has_edited {
                return;
            }
            has_edited = true;
            // Taking the write lock here also proves the scan holds no
            // session lock: a held lock would deadlock this blocking thread.
            let mut guard = live_session.write().expect("lock");
            let session = guard.as_mut().expect("the live session is still loaded");
            let plan = ActivateMods::plan(session, &[ModId::new("zzz.mod")], false);
            ActivateMods::execute(session, &plan).expect("activating zzz.mod must succeed");
        })
        .await;

        let error = result.expect_err("a mid-scan edit must refuse the swap");
        assert_eq!(error.code, crate::error::CommandErrorCode::InvalidInput);
        assert_eq!(
            error.message,
            "the working active-mod set changed during the rescan; rescan again"
        );
        assert!(
            working_ids_of(&state).iter().any(|id| id == "zzz.mod"),
            "the mid-scan edit must survive the refusal"
        );
    }

    #[tokio::test]
    async fn rescan_reads_the_selection_at_the_swap_not_at_the_start() {
        let (_scratch, state) = loaded_sample_state().await;
        assert_eq!(
            state
                .session
                .read()
                .expect("lock")
                .as_ref()
                .map(rim_session::Session::selected),
            Some(rim_resolve::domain::OrderSource::Suggested),
            "the loaded session starts on the suggested order"
        );
        let live_session = std::sync::Arc::clone(&state.session);
        let mut has_selected = false;

        let summary = rescan_project_inner(&state, move |_| {
            if has_selected {
                return;
            }
            has_selected = true;
            let mut guard = live_session.write().expect("lock");
            guard
                .as_mut()
                .expect("the live session is still loaded")
                .select(rim_resolve::domain::OrderSource::Current);
        })
        .await
        .expect("a mid-scan selection change must not refuse the rescan");

        assert_eq!(
            summary.selected,
            crate::dto::common::OrderSourceDto::Current
        );
        let selected = state
            .session
            .read()
            .expect("lock")
            .as_ref()
            .map(rim_session::Session::selected);
        assert_eq!(selected, Some(rim_resolve::domain::OrderSource::Current));
    }

    #[tokio::test]
    async fn rescan_holds_load_lock_for_the_whole_scan() {
        let (_scratch, state) = loaded_sample_state().await;
        let load_lock = std::sync::Arc::clone(&state.load_lock);
        let is_lock_free_during_scan =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let was_observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (lock_flag, observed_flag) = (
            std::sync::Arc::clone(&is_lock_free_during_scan),
            std::sync::Arc::clone(&was_observed),
        );

        rescan_project_inner(&state, move |_| {
            observed_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            if load_lock.try_lock().is_ok() {
                lock_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        })
        .await
        .expect("rescanning the scratch sample game must succeed");

        assert!(
            was_observed.load(std::sync::atomic::Ordering::SeqCst),
            "the scan must report progress, or this test proves nothing"
        );
        assert!(
            !is_lock_free_during_scan.load(std::sync::atomic::Ordering::SeqCst),
            "load_lock must be held for the whole scan"
        );
    }

    #[tokio::test]
    async fn a_scan_panic_fails_the_rescan_but_keeps_the_loaded_session() {
        let (_scratch, state) = loaded_sample_state().await;
        activate_mods_inner(&state, activate_request("zzz.mod"))
            .await
            .expect("zzz.mod is installed but inactive, so it can be activated");
        let working_before = working_ids_of(&state);

        let result = rescan_project_inner(&state, |_| panic!("scan blew up")).await;

        assert_eq!(
            result.expect_err("a scan panic must fail the rescan").code,
            crate::error::CommandErrorCode::Internal
        );
        let guard = state.session.read().expect("lock");
        let session = guard
            .as_ref()
            .expect("a scan panic must not discard the loaded session");
        assert_eq!(working_ids_of_session(session), working_before);
        assert_eq!(
            session.selected(),
            rim_resolve::domain::OrderSource::Suggested
        );
    }

    #[tokio::test]
    async fn no_project_loaded_is_reported_for_every_command() {
        let state = AppState::default();

        assert_eq!(
            list_inactive_mods_inner(&state, ModListQueryDto::default())
                .await
                .unwrap_err()
                .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            plan_activate_mods_inner(
                &state,
                ActivateRequestDto {
                    ids: vec![],
                    with_dependencies: false,
                    anchor: None,
                }
            )
            .await
            .unwrap_err()
            .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            activate_mods_inner(
                &state,
                ActivateRequestDto {
                    ids: vec![],
                    with_dependencies: false,
                    anchor: None,
                }
            )
            .await
            .unwrap_err()
            .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            plan_deactivate_mods_inner(&state, DeactivateRequestDto { ids: vec![] })
                .await
                .unwrap_err()
                .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            deactivate_mods_inner(&state, DeactivateRequestDto { ids: vec![] })
                .await
                .unwrap_err()
                .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            get_pending_active_changes_inner(&state)
                .await
                .unwrap_err()
                .code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
        assert_eq!(
            rescan_project_inner(&state, |_| {}).await.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    use crate::dto::active_set::AnchorDto;

    fn move_request(ids: &[&str], anchor: AnchorDto) -> MoveRequestDto {
        MoveRequestDto {
            ids: ids.iter().map(ToString::to_string).collect(),
            anchor,
        }
    }

    fn current_ids_of(state: &AppState) -> Vec<String> {
        let guard = state.session.read().expect("lock");
        guard
            .as_ref()
            .expect("a session is loaded")
            .orders()
            .current
            .as_slice()
            .iter()
            .map(|id| id.as_str().to_string())
            .collect()
    }

    #[tokio::test]
    async fn move_mods_settles_into_current_and_reports_no_pending_change() {
        let state = state_with(&["a", "b", "c"], &[]);

        let result = move_mods_inner(&state, move_request(&["c"], AnchorDto::Start))
            .await
            .expect("must succeed");

        assert_eq!(current_ids_of(&state), ["c", "a", "b"]);
        assert_eq!(working_ids_of(&state), ["c", "a", "b"]);
        assert!(
            !result.did_switch_to_current,
            "Current was already selected"
        );
        let pending = get_pending_active_changes_inner(&state)
            .await
            .expect("must succeed");
        assert!(pending.unscanned.added.is_empty() && pending.unscanned.removed.is_empty());
    }

    #[tokio::test]
    async fn the_first_move_while_suggested_is_selected_reports_the_switch() {
        let state = state_with(&["a", "b", "c"], &[]);
        crate::commands::order::select_order_inner(
            &state,
            crate::dto::common::OrderSourceDto::Suggested,
        )
        .await
        .expect("a project is loaded");

        let first = move_mods_inner(&state, move_request(&["c"], AnchorDto::Start))
            .await
            .expect("must succeed");
        let second = move_mods_inner(&state, move_request(&["b"], AnchorDto::Start))
            .await
            .expect("must succeed");

        assert!(first.did_switch_to_current);
        assert_eq!(first.selected, crate::dto::common::OrderSourceDto::Current);
        assert!(!second.did_switch_to_current);
    }

    #[tokio::test]
    async fn move_mods_refuses_a_stale_anchor_with_its_own_code() {
        let state = state_with(&["a", "b"], &[]);

        let error = move_mods_inner(
            &state,
            move_request(
                &["a"],
                AnchorDto::After {
                    mod_id: "gone".to_string(),
                },
            ),
        )
        .await
        .expect_err("gone is not active");

        assert_eq!(
            error.code,
            crate::error::CommandErrorCode::MoveAnchorNotActive
        );
        assert_eq!(working_ids_of(&state), ["a", "b"]);
    }

    #[tokio::test]
    async fn move_mods_refuses_an_inactive_mover_as_invalid_input() {
        let state = state_with(&["a"], &["x"]);

        let error = move_mods_inner(&state, move_request(&["x"], AnchorDto::Start))
            .await
            .expect_err("x is not active");

        assert_eq!(error.code, crate::error::CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn activate_mods_with_an_anchor_places_the_mod_there() {
        let state = state_with(&["a", "b"], &["x"]);

        let mut request = activate_request("x");
        request.anchor = Some(AnchorDto::Before {
            mod_id: "b".to_string(),
        });
        activate_mods_inner(&state, request)
            .await
            .expect("must succeed");

        assert_eq!(working_ids_of(&state), ["a", "x", "b"]);
    }

    async fn select_suggested(state: &AppState) {
        crate::commands::order::select_order_inner(
            state,
            crate::dto::common::OrderSourceDto::Suggested,
        )
        .await
        .expect("a project is loaded");
    }

    #[tokio::test]
    async fn an_anchored_activation_while_suggested_is_selected_switches_to_current() {
        let state = state_with(&["a", "b"], &["x"]);
        select_suggested(&state).await;
        let mut request = activate_request("x");
        request.anchor = Some(AnchorDto::Start);

        let result = activate_mods_inner(&state, request)
            .await
            .expect("must succeed");

        assert!(result.did_switch_to_current);
        assert_eq!(result.selected, crate::dto::common::OrderSourceDto::Current);
        assert_eq!(result.pending.unscanned.added, vec!["x".to_string()]);
    }

    #[tokio::test]
    async fn an_unanchored_activation_while_suggested_is_selected_keeps_the_selection() {
        let state = state_with(&["a", "b"], &["x"]);
        select_suggested(&state).await;

        let result = activate_mods_inner(&state, activate_request("x"))
            .await
            .expect("must succeed");

        assert!(!result.did_switch_to_current);
        assert_eq!(
            result.selected,
            crate::dto::common::OrderSourceDto::Suggested
        );
    }

    #[tokio::test]
    async fn a_settled_move_reports_an_order_that_differs_from_the_file() {
        let state = state_with(&["a", "b", "c"], &[]);
        let before = get_pending_active_changes_inner(&state)
            .await
            .expect("must succeed");

        move_mods_inner(&state, move_request(&["c"], AnchorDto::Start))
            .await
            .expect("must succeed");
        let moved = get_pending_active_changes_inner(&state)
            .await
            .expect("must succeed");
        let reverted = revert_order_to_file_inner(&state)
            .await
            .expect("same mods as the file");

        assert!(!before.order_unapplied);
        assert!(moved.order_unapplied);
        assert!(
            moved.unapplied.added.is_empty() && moved.unapplied.removed.is_empty(),
            "the membership diff cannot see a reorder"
        );
        assert!(!reverted.order_unapplied);
    }

    #[tokio::test]
    async fn plan_activate_mods_refuses_a_stale_anchor() {
        let state = state_with(&["a"], &["x"]);
        let mut request = activate_request("x");
        request.anchor = Some(AnchorDto::After {
            mod_id: "gone".to_string(),
        });

        let error = plan_activate_mods_inner(&state, request)
            .await
            .expect_err("gone is not active");

        assert_eq!(
            error.code,
            crate::error::CommandErrorCode::MoveAnchorNotActive
        );
    }

    #[tokio::test]
    async fn discard_pending_changes_resets_working_without_a_rescan() {
        let state = state_with(&["a"], &["x"]);
        activate_mods_inner(&state, activate_request("x"))
            .await
            .expect("must succeed");

        let pending = discard_pending_changes_inner(&state)
            .await
            .expect("must succeed");

        assert_eq!(working_ids_of(&state), ["a"]);
        assert!(pending.unscanned.added.is_empty());
    }

    #[tokio::test]
    async fn revert_order_to_file_restores_the_file_sequence() {
        let state = state_with(&["a", "b", "c"], &[]);
        move_mods_inner(&state, move_request(&["c"], AnchorDto::Start))
            .await
            .expect("must succeed");

        revert_order_to_file_inner(&state)
            .await
            .expect("same mods as the file");

        assert_eq!(current_ids_of(&state), ["a", "b", "c"]);
        assert_eq!(working_ids_of(&state), ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn revert_order_to_file_refuses_with_unscanned_changes() {
        let state = state_with(&["a"], &["x"]);
        activate_mods_inner(&state, activate_request("x"))
            .await
            .expect("must succeed");

        let error = revert_order_to_file_inner(&state)
            .await
            .expect_err("the working set is stale");

        assert_eq!(
            error.code,
            crate::error::CommandErrorCode::RevertOrderPendingChanges
        );
    }

    #[tokio::test]
    async fn revert_order_to_file_refuses_when_current_and_the_file_hold_different_mods() {
        // Current holds a and b; the file last held a and c.
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(
            rim_session::test_support::session_fixture_with_file_mods(&["a", "b"], &["a", "c"]),
        );

        let error = revert_order_to_file_inner(&state)
            .await
            .expect_err("the file lists other mods than Current");

        assert_eq!(
            error.code,
            crate::error::CommandErrorCode::RevertOrderMembershipDiffers
        );
    }

    #[tokio::test]
    async fn applying_after_a_move_writes_the_moved_order() {
        let (temp_dir, session) =
            crate::test_support::session_fixture_with_temp_paths(&["a", "b", "c"]);
        struct FixedProbe;
        impl crate::state::GameProcessProbe for FixedProbe {
            fn is_running(&self) -> bool {
                false
            }
        }
        let state = AppState {
            game_process_probe: std::sync::Arc::new(FixedProbe),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);
        move_mods_inner(&state, move_request(&["c"], AnchorDto::Start))
            .await
            .expect("must succeed");

        crate::commands::apply::apply_inner(
            &state,
            crate::dto::settings::ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await
        .expect("a settled move is not stale");

        let written = std::fs::read_to_string(temp_dir.path().join("ModsConfig.xml"))
            .expect("read the written ModsConfig.xml");
        let position = |id: &str| written.find(&format!("<li>{id}</li>")).expect("listed");
        assert!(position("c") < position("a") && position("a") < position("b"));
    }

    #[tokio::test]
    async fn rescan_refuses_when_a_move_happened_mid_scan() {
        let (_scratch, state) = loaded_sample_state().await;
        // The sample game has one active mod: activate a second so a move can change the sequence.
        activate_mods_inner(&state, activate_request("zzz.mod"))
            .await
            .expect("zzz.mod is installed but inactive, so it can be activated");
        let live_session = std::sync::Arc::clone(&state.session);
        let mut has_moved = false;

        let result = rescan_project_inner(&state, move |_| {
            if has_moved {
                return;
            }
            has_moved = true;
            let mut guard = live_session.write().expect("lock");
            let session = guard.as_mut().expect("the live session is still loaded");
            MoveMods::execute(session, &[ModId::new("zzz.mod")], &Anchor::Start)
                .expect("moving zzz.mod to the start must succeed");
        })
        .await;

        let error = result.expect_err("a mid-scan move must refuse the swap");
        assert_eq!(error.code, crate::error::CommandErrorCode::InvalidInput);
        assert_eq!(working_ids_of(&state)[0], "zzz.mod", "the move survives");
    }

    #[tokio::test]
    async fn a_rescan_after_a_settled_move_keeps_the_moved_order() {
        let (_scratch, state) = loaded_sample_state().await;
        activate_mods_inner(&state, activate_request("zzz.mod"))
            .await
            .expect("zzz.mod is installed but inactive, so it can be activated");
        rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");
        move_mods_inner(&state, move_request(&["zzz.mod"], AnchorDto::Start))
            .await
            .expect("must succeed");
        assert_eq!(current_ids_of(&state)[0], "zzz.mod");

        rescan_project_inner(&state, |_| {})
            .await
            .expect("rescanning the scratch sample game must succeed");

        assert_eq!(current_ids_of(&state)[0], "zzz.mod");
        assert_eq!(working_ids_of(&state)[0], "zzz.mod");
    }

    #[tokio::test]
    async fn the_active_list_is_reorderable_only_in_load_order() {
        let state = state_with(&["a", "b"], &["x"]);
        let by_name = ModListQueryDto {
            sort: Some(ListSortDto {
                key: ListSortKeyDto::Name,
                direction: SortDirectionDto::Ascending,
            }),
            ..ModListQueryDto::default()
        };

        let in_order =
            crate::commands::mods::list_working_mods_inner(&state, ModListQueryDto::default())
                .await
                .expect("must succeed");
        let sorted = crate::commands::mods::list_working_mods_inner(&state, by_name)
            .await
            .expect("must succeed");
        let inactive = list_inactive_mods_inner(&state, ModListQueryDto::default())
            .await
            .expect("must succeed");

        assert!(in_order.reorderable);
        assert!(!sorted.reorderable);
        assert!(!inactive.reorderable);
    }
}
