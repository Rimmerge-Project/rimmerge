//! `apply`: writes the order named by `request.source` to `ModsConfig.xml`
//! (with a backup) and saves decisions/rules — refusing when RimWorld itself looks like
//! it's running, unless the caller forces it.

use std::sync::Arc;

use crate::commands::emit_session_changed;
use crate::dto::common::OrderSourceDto;
use crate::dto::preflight::ApplyPreflightDto;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::settings::{ApplyReportDto, ApplyRequestDto, apply_report};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Persists decisions and rules, and — when `request.write_mods_config`
/// is true — writes the selected order to `ModsConfig.xml` with a
/// timestamped backup. Refuses to write `ModsConfig.xml` while
/// `RimWorldWin64.exe` looks like it's running (per
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

/// Lists the hard problems in `source`'s order — what
/// [`apply_inner`] would write — so the dialog can confirm them first.
/// Read-only.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_apply_preflight_inner(
    state: &AppState,
    source: OrderSourceDto,
) -> Result<ApplyPreflightDto, CommandError> {
    with_session(state, move |session| {
        Ok(rim_session::use_cases::PreflightApply::new()
            .execute(session, source.into())
            .into())
    })
    .await
}

/// See `get_apply_preflight_inner`.
///
/// # Errors
///
/// See `get_apply_preflight_inner`.
#[tauri::command]
pub async fn get_apply_preflight(
    state: tauri::State<'_, AppState>,
    source: OrderSourceDto,
) -> Result<ApplyPreflightDto, CommandError> {
    get_apply_preflight_inner(&state, source).await
}

#[cfg(test)]
mod tests {
    use rim_session::test_support::session_fixture;

    use super::*;
    use crate::state::GameProcessProbe;
    use crate::test_support::session_fixture_with_temp_paths;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
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

    #[tokio::test]
    async fn preflight_without_a_project_is_no_project_loaded() {
        let state = AppState::default();

        let result =
            get_apply_preflight_inner(&state, crate::dto::common::OrderSourceDto::Suggested).await;

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

        let preflight =
            get_apply_preflight_inner(&state, crate::dto::common::OrderSourceDto::Suggested)
                .await
                .expect("preflight");

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
}
