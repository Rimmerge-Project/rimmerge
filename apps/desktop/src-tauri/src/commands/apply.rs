//! `apply`: writes the order named by `request.source` to `ModsConfig.xml`
//! (with a backup) and saves decisions/rules — refusing when RimWorld itself looks like
//! it's running, unless the caller forces it.

use std::sync::Arc;

use rim_session::BaselineChange;
use rim_session::ports::ModsConfigStore;

use crate::commands::emit_session_changed;
use crate::commands::file_baseline::refresh_mods_config_baseline_inner;
use crate::dto::common::OrderSourceDto;
use crate::dto::preflight::ApplyPreflightDto;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::settings::{ApplyReportDto, ApplyRequestDto, apply_report};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Persists decisions and rules, and — when `request.write_mods_config`
/// is true — writes the selected order to `ModsConfig.xml` with a
/// timestamped backup. Refuses to write `ModsConfig.xml` while
/// RimWorld looks like it's running (per
/// [`crate::state::AppState::game_process_probe`]), unless
/// `request.force` is set (RimWorld overwrites the file on exit, so a
/// concurrent write is likely to be lost or to corrupt the running game's
/// own save of it).
///
/// The probe check runs *inside* [`with_session`]'s closure — on the same
/// blocking thread, after the "is a project even loaded" check
/// `with_session` itself does — so [`CommandError::no_project_loaded`]
/// always wins over [`CommandError::rimworld_running`] when both are
/// true, and the (syscall-heavy) process-list scan never blocks the
/// async command thread.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::rimworld_running`] when the game process is detected
/// and `request.force` is `false`, or [`CommandError`] when saving
/// decisions/rules or writing `ModsConfig.xml` fails.
pub(crate) async fn apply_inner(
    state: &AppState,
    request: ApplyRequestDto,
) -> Result<ApplyReportDto, CommandError> {
    let adapters = state.adapters.clone();
    let probe = Arc::clone(&state.game_process_probe);
    with_session(state, move |session| {
        if request.write_mods_config && !request.force && probe.is_running() {
            return Err(CommandError::rimworld_running());
        }

        let mods_config_path = session.paths().mods_config.clone();
        let profile_dir = session.paths().profile_dir.clone();

        let use_case = rim_session::use_cases::Apply::new(
            adapters.config_store,
            adapters.decision_store,
            adapters.rule_store,
            adapters.merge_mod_writer,
            adapters.def_reader,
            adapters.asset_locator,
        );
        let outcome = use_case.execute(
            session,
            rim_session::use_cases::ApplyOptions {
                source: request.source.into(),
                write_mods_config: request.write_mods_config,
                write_merge_mod: request.write_merge_mod,
            },
        )?;

        Ok(apply_report(&outcome, &mods_config_path, &profile_dir))
    })
    .await
}

/// See [`apply_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`apply_inner`].
#[tauri::command]
pub async fn apply(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ApplyRequestDto,
) -> Result<ApplyReportDto, CommandError> {
    let result = apply_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Applied);
    }
    result
}

/// Lists the hard problems in `source`'s order and the outside change of
/// `ModsConfig.xml` that writing it would overwrite, so the dialog can
/// confirm them first. The file is read **fresh** here (the 5 s poll may be
/// that old): the baseline is refreshed through
/// [`refresh_mods_config_baseline_inner`] (probe under the lock, read in a
/// bare `spawn_blocking`, commit under the lock; the lock is never held
/// across the read) and the preflight is computed from the committed
/// result. It only reads. Committing the new list is the baseline's one-shot
/// change notice (the next poll reads the same list and reports nothing), so
/// the [`BaselineChange`] is returned for the command to announce; the
/// refetch that announcement causes reads the same file, finds it
/// unchanged and emits nothing, so it cannot loop. A failed read keeps the
/// last baseline, as the poll does.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the blocking read panics.
pub(crate) async fn get_apply_preflight_inner<S>(
    state: &AppState,
    source: OrderSourceDto,
    store: S,
) -> Result<(ApplyPreflightDto, BaselineChange), CommandError>
where
    S: ModsConfigStore + Send + 'static,
{
    let (_, change) = refresh_mods_config_baseline_inner(state, store).await?;
    let preflight = with_session(state, move |session| {
        Ok(rim_session::use_cases::PreflightApply::new()
            .execute(session, source.into())
            .into())
    })
    .await?;
    Ok((preflight, change))
}

/// See `get_apply_preflight_inner`; also emits `session://changed` with
/// [`SessionChangeReasonDto::FileBaselineChanged`] when the fresh read found
/// the list or the status different, as the poll's refresh does, so the
/// Mods marks, the hint and the launch status follow.
///
/// # Errors
///
/// See `get_apply_preflight_inner`.
#[tauri::command]
pub async fn get_apply_preflight(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    source: OrderSourceDto,
) -> Result<ApplyPreflightDto, CommandError> {
    let store = state.adapters.config_store;
    let (preflight, change) = get_apply_preflight_inner(&state, source, store).await?;
    if change == BaselineChange::Changed {
        emit_session_changed(&app, SessionChangeReasonDto::FileBaselineChanged);
    }
    Ok(preflight)
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_session::ports::ModsConfigFile;
    use rim_session::test_support::{
        InMemoryModsConfigStore, session_fixture, session_fixture_with_inactive,
    };

    use super::*;
    use crate::state::GameProcessProbe;
    use crate::test_support::session_fixture_with_temp_paths;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    fn mods_config_file(active: &[&str]) -> ModsConfigFile {
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: active.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        }
    }

    fn state_with_probe(running: bool) -> AppState {
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(running)),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session_fixture(&["a"]));
        state
    }

    #[tokio::test]
    async fn apply_is_refused_when_the_probe_reports_the_game_running() {
        let state = state_with_probe(true);

        let result = apply_inner(
            &state,
            ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::RimworldRunning
        );
    }

    #[tokio::test]
    async fn apply_writes_mods_config_when_forced_despite_the_game_running() {
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(true)),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);

        let report = apply_inner(
            &state,
            ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: true,
            },
        )
        .await
        .expect("forced apply must succeed even with the game running");

        assert!(report.backup_path.is_some());
        assert!(
            temp_dir
                .path()
                .join("profile")
                .join("decisions.json")
                .exists(),
            "decisions must have been saved alongside the ModsConfig.xml write"
        );
    }

    #[tokio::test]
    async fn apply_no_project_loaded_wins_over_rimworld_running() {
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(true)),
            ..AppState::default()
        };
        // No session loaded.

        let result = apply_inner(
            &state,
            ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    #[tokio::test]
    async fn apply_writes_the_source_named_in_the_request_not_the_selected_one() {
        use rim_analyzer::domain::ModId;
        use rim_resolve::domain::{Action, Decision, FindingKey};

        let (temp_dir, mut session) = session_fixture_with_temp_paths(&["a", "b"]);
        session
            .decide(Decision {
                key: FindingKey::UndeclaredHardDependency {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                action: Action::Reorder {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("reorder is always a valid action");
        assert_eq!(
            session.selected(),
            rim_resolve::domain::OrderSource::Current
        );
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(false)),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);

        apply_inner(
            &state,
            ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Suggested,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await
        .expect("apply must succeed");

        let written = std::fs::read_to_string(temp_dir.path().join("ModsConfig.xml"))
            .expect("read the written ModsConfig.xml");
        let position = |id: &str| written.find(&format!("<li>{id}</li>")).expect("id written");
        assert!(
            position("b") < position("a"),
            "the suggested order puts b before a: {written}"
        );
    }

    /// The Dashboard's "Mods that move" is derived from the live orders: after
    /// an apply makes Current equal Suggested it must read 0, not the count
    /// the sorter measured against the pre-apply Current.
    #[tokio::test]
    async fn dashboard_moved_mods_is_zero_after_applying_the_suggested_order() {
        use rim_analyzer::domain::ModId;
        use rim_resolve::domain::{Action, Decision, FindingKey};

        let (_temp_dir, mut session) = session_fixture_with_temp_paths(&["a", "b"]);
        session
            .decide(Decision {
                key: FindingKey::UndeclaredHardDependency {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                action: Action::Reorder {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("reorder is always a valid action");
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(false)),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);
        let before = crate::commands::dashboard::get_dashboard_inner(&state)
            .await
            .expect("dashboard");
        assert!(before.moved_mods > 0, "sanity: the sorter moves something");

        apply_inner(
            &state,
            ApplyRequestDto {
                source: crate::dto::common::OrderSourceDto::Suggested,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await
        .expect("apply must succeed");

        let after = crate::commands::dashboard::get_dashboard_inner(&state)
            .await
            .expect("dashboard");
        assert_eq!(after.moved_mods, 0);
    }

    #[tokio::test]
    async fn preflight_without_a_project_is_no_project_loaded() {
        let state = AppState::default();

        let result = get_apply_preflight_inner(
            &state,
            crate::dto::common::OrderSourceDto::Suggested,
            InMemoryModsConfigStore::new(mods_config_file(&["a"])),
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    #[tokio::test]
    async fn preflight_lists_a_missing_mod_the_suggested_order_removes() {
        use crate::dto::preflight::{HardProblemDto, MissingModOutcomeDto};

        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("gone")
            .build();
        let session = rim_session::test_support::session_with_sources_and_mods(
            rim_analyzer::analysis::SourceIndex::default(),
            report,
            &["a"],
        );
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let preflight = get_apply_preflight_inner(
            &state,
            crate::dto::common::OrderSourceDto::Suggested,
            InMemoryModsConfigStore::new(mods_config_file(&["a"])),
        )
        .await
        .expect("preflight")
        .0;

        assert!(preflight.requires_confirmation);
        let [item] = preflight.items.as_slice() else {
            panic!("expected exactly one problem: {:?}", preflight.items);
        };
        assert!(!item.acknowledged);
        assert!(matches!(
            &item.problem,
            HardProblemDto::MissingMod(missing)
                if missing.mod_id == "gone"
                    && missing.outcome == MissingModOutcomeDto::RemovedFromActiveList
        ));
    }

    #[tokio::test]
    async fn preflight_reads_the_file_fresh_and_reports_an_outside_change() {
        use crate::dto::file_baseline::ExternalFileChangeDto;

        let state = AppState::default();
        *state.session.write().expect("lock") =
            Some(session_fixture_with_inactive(&["a", "b"], &["x"]));
        // The session's baseline still says a, b: only a read of the file
        // can see the outside edit (no poll ran).
        let store = InMemoryModsConfigStore::new(mods_config_file(&["a", "b", "x"]));

        let preflight = get_apply_preflight_inner(&state, OrderSourceDto::Current, store)
            .await
            .expect("preflight")
            .0;

        assert_eq!(
            preflight.outside_change,
            ExternalFileChangeDto::Membership {
                added: 1,
                removed: 0
            }
        );
        assert!(preflight.items.is_empty());
        assert!(preflight.requires_confirmation);
    }

    /// The commit is the baseline's one-shot notice: the command must announce it
    /// from the first preflight, because no later read will report a change.
    #[tokio::test]
    async fn preflight_reports_the_baseline_change_once() {
        let state = AppState::default();
        *state.session.write().expect("lock") =
            Some(session_fixture_with_inactive(&["a", "b"], &["x"]));
        let outside = || InMemoryModsConfigStore::new(mods_config_file(&["a", "b", "x"]));

        let (_, first) = get_apply_preflight_inner(&state, OrderSourceDto::Current, outside())
            .await
            .expect("first preflight");
        let (second_preflight, second) =
            get_apply_preflight_inner(&state, OrderSourceDto::Current, outside())
                .await
                .expect("second preflight");

        assert_eq!(first, BaselineChange::Changed);
        assert_eq!(second, BaselineChange::Unchanged);
        assert!(
            second_preflight.requires_confirmation,
            "the outside change is still reported after the notice was spent"
        );
    }

    #[tokio::test]
    async fn preflight_of_an_untouched_file_asks_nothing() {
        use crate::dto::file_baseline::ExternalFileChangeDto;

        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture(&["a", "b"]));
        let store = InMemoryModsConfigStore::new(mods_config_file(&["a", "b"]));

        let preflight = get_apply_preflight_inner(&state, OrderSourceDto::Current, store)
            .await
            .expect("preflight")
            .0;

        assert_eq!(preflight.outside_change, ExternalFileChangeDto::None);
        assert!(!preflight.requires_confirmation);
    }

    /// An outside edit on a real file: the preflight sees it, "Apply
    /// anyway" still writes with a backup that holds the outside content,
    /// and the next preflight is quiet.
    #[tokio::test]
    async fn applying_over_an_outside_change_keeps_a_backup_of_the_outside_file() {
        use crate::dto::file_baseline::ExternalFileChangeDto;

        let (temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(false)),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session);
        let mods_config = temp_dir.path().join("ModsConfig.xml");
        std::fs::write(
            &mods_config,
            "<ModsConfigData><version>1.6.0</version><activeMods><li>a</li><li>b</li><li>outsider</li></activeMods></ModsConfigData>",
        )
        .expect("simulate an outside edit");

        let preflight =
            get_apply_preflight_inner(&state, OrderSourceDto::Current, state.adapters.config_store)
                .await
                .expect("preflight")
                .0;
        assert!(preflight.requires_confirmation);
        assert_eq!(
            preflight.outside_change,
            ExternalFileChangeDto::Membership {
                added: 1,
                removed: 0
            }
        );

        let report = apply_inner(
            &state,
            ApplyRequestDto {
                source: OrderSourceDto::Current,
                write_mods_config: true,
                write_merge_mod: false,
                force: false,
            },
        )
        .await
        .expect("apply anyway");

        let backup = report.backup_path.expect("a backup is always made");
        let backed_up = std::fs::read_to_string(&backup).expect("read backup");
        assert!(backed_up.contains("<li>outsider</li>"), "{backed_up}");
        let written = std::fs::read_to_string(&mods_config).expect("read written file");
        assert!(!written.contains("outsider"), "{written}");
        let after =
            get_apply_preflight_inner(&state, OrderSourceDto::Current, state.adapters.config_store)
                .await
                .expect("preflight after apply")
                .0;
        assert!(!after.requires_confirmation);
    }
}
