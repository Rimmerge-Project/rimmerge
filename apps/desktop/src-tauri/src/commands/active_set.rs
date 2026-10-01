//! `list_inactive_mods`/`plan_activate_mods`/`activate_mods`/
//! `plan_deactivate_mods`/`deactivate_mods`/`get_pending_active_changes`/
//! `rescan_project`: the Mods page's Inactive tab, the working active-mod
//! set, and re-scanning it.
//!
//! Every rule lives in `rim-session` (`ActiveSet`, `ActivateMods`,
//! `DeactivateMods`, `Rescan`) — these commands only map DTOs in and out
//! and, for the two mutators plus `rescan_project`, emit
//! `session://changed`.

use std::time::Instant;

use rim_session::use_cases::{ActivateMods, DeactivateMods, LoadProject};
use tauri::async_runtime::spawn_blocking;

use crate::commands::project::finish_loaded_session;
use crate::commands::{EVENT_PROJECT_PROGRESS, emit_session_changed};
use crate::dto::active_set::{
    ActivatePlanDto, ActivateRequestDto, DeactivatePlanDto, DeactivateRequestDto,
    PendingActiveChangesDto,
};
use crate::dto::mods::{ModFilterDto, ModPageDto};
use crate::dto::project::{ProgressEventDto, ProjectSummaryDto, SessionChangeReasonDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Searches and pages the inactive mod list — the Mods page's Inactive
/// tab. Filtering/sorting/paging lives in
/// [`rim_session::Session::inactive_mods`]; this only maps the DTO in and
/// out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::invalid_input`] when `filter.tag` isn't a valid tag
/// slug (accepted for symmetry with `list_mods`'s own filter shape, even
/// though an inactive mod never actually carries one).
pub(crate) async fn list_inactive_mods_inner(
    state: &AppState,
    filter: ModFilterDto,
) -> Result<ModPageDto, CommandError> {
    with_session(state, move |session| {
        let filter: rim_session::ModFilter = filter.try_into()?;
        Ok(session.inactive_mods(&filter).into())
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
    filter: ModFilterDto,
) -> Result<ModPageDto, CommandError> {
    list_inactive_mods_inner(&state, filter).await
}

/// Plans what `activate_mods` would do — the confirm dialog's own read,
/// before anything is mutated.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn plan_activate_mods_inner(
    state: &AppState,
    request: ActivateRequestDto,
) -> Result<ActivatePlanDto, CommandError> {
    with_session(state, move |session| {
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
/// or [`CommandError::invalid_input`] when an id in `request.ids` isn't
/// known to the current report.
pub(crate) async fn activate_mods_inner(
    state: &AppState,
    request: ActivateRequestDto,
) -> Result<PendingActiveChangesDto, CommandError> {
    with_session(state, move |session| {
        let plan = ActivateMods::plan(session, &request.mod_ids(), request.with_dependencies);
        ActivateMods::execute(session, &plan)?;
        Ok((&session.pending_changes()).into())
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
) -> Result<PendingActiveChangesDto, CommandError> {
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

/// Re-scans the session's own working set: [`LoadProject::execute_with_active_set`]
/// over the working set's own ids, which re-reads decisions/rules/
/// patches/assignments from disk exactly like an ordinary
/// [`LoadProject::execute`] would (safe, because the working set is
/// this crate's only in-memory-only state).
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
/// snapshots `(paths, working ids)` and releases the lock immediately;
/// (2) the real scan runs in a bare [`spawn_blocking`], under
/// `load_lock`, touching no live `Session` at all — a panic there can
/// only fail this call, never poison anything already loaded; (3) a
/// second short `with_session` call swaps the new session in, but only
/// if `session.working().ids()` still matches the phase-1 snapshot —
/// otherwise something else (`activate_mods`/`deactivate_mods`) changed
/// the working set while the scan was running, and swapping in a
/// session scanned against the *old* set would silently discard that
/// change, so this refuses instead and asks for a retry.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when the working set changed while the
/// scan was running, or [`CommandError`] when the re-scan itself fails
/// (see [`rim_session::use_cases::LoadProjectError`]).
pub(crate) async fn rescan_project_inner(
    state: &AppState,
    mut on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<ProjectSummaryDto, CommandError> {
    let _permit = state.load_lock.lock().await;

    let (paths, working_ids) = with_session(state, |session| {
        Ok((session.paths().clone(), session.working().ids().to_vec()))
    })
    .await?;
    let working_ids_snapshot = working_ids.clone();

    let adapters = state.adapters.clone();
    let (new_session, mut summary) = spawn_blocking(move || {
        let use_case = LoadProject::new(
            adapters.scanner,
            adapters.config_store,
            adapters.decision_store,
            adapters.rule_store,
            adapters.patch_store,
            adapters.assignment_store,
            adapters.mod_knowledge_store.clone(),
            crate::commands::rules_databases::network_policy().fetch_rimmerge_rules,
        );

        let start = Instant::now();
        let def_reader = adapters.def_reader.clone();
        let mut session =
            use_case.execute_with_active_set(paths, working_ids, &mut |progress| {
                on_progress(progress.into());
            })?;
        let summary = finish_loaded_session(&mut session, def_reader, start);

        Ok::<_, CommandError>((session, summary))
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "rescan_project scan task panicked");
        Err(CommandError::internal("background task panicked"))
    })?;

    let selected = with_session(state, move |session| {
        if session.working().ids() != working_ids_snapshot.as_slice() {
            return Err(CommandError::invalid_input(
                "the working active-mod set changed during the rescan; rescan again",
            ));
        }
        // A rescan swaps in a fresh `Session`, which starts on the current
        // order; the user's selection is not a scan result, so it is
        // carried over from the live session (read here, not snapshotted
        // before the scan, so a `select_order` made mid-scan survives).
        let selected = session.selected();
        let mut new_session = new_session;
        new_session.select(selected);
        *session = new_session;
        Ok(selected)
    })
    .await?;
    summary.selected = selected.into();

    Ok(summary)
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

        let page = list_inactive_mods_inner(
            &state,
            ModFilterDto {
                limit: 10,
                ..ModFilterDto::default()
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].mod_id, "aaa.mod");
        assert_eq!(page.items[1].mod_id, "zzz.mod");
    }

    #[tokio::test]
    async fn plan_activate_mods_plans_without_mutating_the_working_set() {
        let state = state_with(&["a"], &["b"]);

        let plan = plan_activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
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

        let pending = activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["b".to_string()],
                with_dependencies: false,
            },
        )
        .await
        .expect("must succeed");

        assert_eq!(pending.unscanned.added, vec!["b".to_string()]);
    }

    #[tokio::test]
    async fn activate_mods_rejects_an_unknown_id() {
        let state = state_with(&["a"], &[]);

        let result = activate_mods_inner(
            &state,
            ActivateRequestDto {
                ids: vec!["ghost".to_string()],
                with_dependencies: false,
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

    #[tokio::test]
    async fn no_project_loaded_is_reported_for_every_command() {
        let state = AppState::default();

        assert_eq!(
            list_inactive_mods_inner(&state, ModFilterDto::default())
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
}
