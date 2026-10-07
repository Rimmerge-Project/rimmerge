//! `get_game_launch_status`/`launch_game`: the Launch RimWorld button's
//! query and command.
//!
//! Both are session commands (`no_project_loaded` wins as everywhere else)
//! with one documented exception to "does its real work inside
//! `with_session`": they take a short snapshot under the session lock
//! (`game_dir` and `Session::order_on_disk`), release it, and only then
//! read the process list and ask the launcher. The lock is held only for
//! the snapshot, never across the process-list scan or a spawn, so a poll
//! or launch never makes another command wait on it (the snapshot itself
//! can still wait for a long command such as `verify`).
//! Neither writes anything or emits `session://changed`: nothing in the
//! session changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rim_session::ports::{GameLauncher, LaunchFailure, LaunchRoute};
use rim_session::use_cases::{GameLaunchFacts, LaunchGame};
use rim_session::{GameProcess, LaunchUnavailable, OrderOnDisk};
use tauri::async_runtime::spawn_blocking;

use crate::dto::game_launch::{GameLaunchStatusDto, GameLaunchedDto, LaunchGameRequestDto};
use crate::error::CommandError;
use crate::state::{AppState, GameProcessProbe, with_session};

/// [`LaunchGame`] is generic over its launcher, while the app holds the
/// launcher as a shared `dyn` port. This newtype lets the use case own a
/// clone of that `Arc` without touching `rim-session` (a blanket impl for
/// `Arc<T>` there would widen a reviewed crate for one caller).
struct SharedLauncher(Arc<dyn GameLauncher + Send + Sync>);

impl GameLauncher for SharedLauncher {
    fn route(&self, game_dir: &Path) -> Result<LaunchRoute, LaunchUnavailable> {
        self.0.route(game_dir)
    }

    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure> {
        self.0.launch(route)
    }
}

/// What the session says about the launch, read under its lock.
struct SessionSnapshot {
    game_dir: PathBuf,
    order: OrderOnDisk,
}

async fn snapshot(state: &AppState) -> Result<SessionSnapshot, CommandError> {
    with_session(state, |session| {
        Ok(SessionSnapshot {
            game_dir: session.paths().game_dir.clone(),
            order: session.order_on_disk(),
        })
    })
    .await
}

fn game_process(probe: &dyn GameProcessProbe) -> GameProcess {
    if probe.is_running() {
        GameProcess::Running
    } else {
        GameProcess::NotRunning
    }
}

/// Snapshots the session, then runs `work` on a blocking thread with the
/// lock released, the probe's answer folded into the facts, and the
/// app's launcher wrapped in the use case.
async fn with_launch_facts<T, F>(state: &AppState, work: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(LaunchGame<SharedLauncher>, GameLaunchFacts) -> T + Send + 'static,
{
    let SessionSnapshot { game_dir, order } = snapshot(state).await?;
    let probe = Arc::clone(&state.game_process_probe);
    let launcher = SharedLauncher(Arc::clone(&state.game_launcher));
    spawn_blocking(move || {
        let facts = GameLaunchFacts {
            game_dir,
            order,
            game: game_process(probe.as_ref()),
        };
        work(LaunchGame::new(launcher), facts)
    })
    .await
    .map_err(|join_error| {
        tracing::warn!(error = %join_error, "game launch task panicked");
        CommandError::internal("background task panicked")
    })
}

/// What the Launch RimWorld button shows, from the selected order, the
/// process list, and how this install is started. Read-only.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the blocking task panics.
pub(crate) async fn get_game_launch_status_inner(
    state: &AppState,
) -> Result<GameLaunchStatusDto, CommandError> {
    with_launch_facts(state, |use_case, facts| use_case.status(&facts).into()).await
}

/// See `get_game_launch_status_inner`.
///
/// # Errors
///
/// See `get_game_launch_status_inner`.
#[tauri::command]
pub async fn get_game_launch_status(
    state: tauri::State<'_, AppState>,
) -> Result<GameLaunchStatusDto, CommandError> {
    get_game_launch_status_inner(&state).await
}

/// Starts RimWorld, re-deriving the status first: a running game and an
/// install with nothing to start are always refused, and an order the file
/// doesn't hold is refused unless `request.if_not_applied` says to launch
/// anyway. The request names no path or URL; the launcher derives both
/// from the install itself.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::rimworld_running`]'s code when the game is running,
/// `order_not_applied`, `game_executable_missing`, `steam_launch_failed`
/// or `game_start_failed` per [`rim_session::use_cases::LaunchGameError`],
/// or [`CommandError::internal`] when the blocking task panics.
pub(crate) async fn launch_game_inner(
    state: &AppState,
    request: LaunchGameRequestDto,
) -> Result<GameLaunchedDto, CommandError> {
    let if_not_applied = request.if_not_applied.into();
    let kind = with_launch_facts(state, move |use_case, facts| {
        use_case.execute(&facts, if_not_applied)
    })
    .await??;
    tracing::info!(route = ?kind, "game launch requested");
    Ok(GameLaunchedDto { route: kind.into() })
}

/// See `launch_game_inner`. Emits no `session://changed`: launching
/// changes nothing in the session.
///
/// # Errors
///
/// See `launch_game_inner`.
#[tauri::command]
pub async fn launch_game(
    state: tauri::State<'_, AppState>,
    request: LaunchGameRequestDto,
) -> Result<GameLaunchedDto, CommandError> {
    launch_game_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_session::ports::ModsConfigFile;
    use rim_session::test_support::{report_fixture, session_fixture};
    use rim_session::{ProjectPaths, Session};

    use super::*;
    use crate::dto::active_set::DeactivateRequestDto;
    use crate::dto::game_launch::{
        GameLaunchNeedsApplyDto, GameLaunchReadyDto, GameLaunchRouteDto, GameLaunchUnavailableDto,
        IfNotAppliedDto, LaunchUnavailableDto, UnappliedReasonDto,
    };
    use crate::error::CommandErrorCode;
    use crate::test_support::RecordingGameLauncher;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    fn request(if_not_applied: IfNotAppliedDto) -> LaunchGameRequestDto {
        LaunchGameRequestDto { if_not_applied }
    }

    /// A state holding an applied two-mod session, with `launcher` and a
    /// probe answering `game_running`. The real launcher and probe never
    /// appear.
    fn state_with(launcher: &Arc<RecordingGameLauncher>, game_running: bool) -> AppState {
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(game_running)),
            game_launcher: launcher.clone(),
            ..AppState::default()
        };
        *state.session.write().expect("lock") = Some(session_fixture(&["a", "b"]));
        state
    }

    /// A session whose `ModsConfig.xml` last held `b, a` while the selected
    /// (suggested) order is `a, b`.
    fn session_behind_the_file() -> Session {
        let paths = ProjectPaths {
            game_dir: PathBuf::from("game"),
            workshop_dir: PathBuf::from("workshop"),
            mods_config: PathBuf::from("ModsConfig.xml"),
            profile_dir: PathBuf::from("profile"),
        };
        let mut session = Session::new(
            paths,
            report_fixture(&["a", "b"]),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6.0".to_string(),
                active_mods: vec![ModId::new("b"), ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        session.select(rim_resolve::domain::OrderSource::Suggested);
        session
    }

    fn put_behind_the_file(state: &AppState) {
        *state.session.write().expect("lock") = Some(session_behind_the_file());
    }

    #[tokio::test]
    async fn status_and_launch_need_a_loaded_project_before_anything_else() {
        let launcher = Arc::new(RecordingGameLauncher::steam());
        let state = AppState {
            game_process_probe: Arc::new(FixedProbe(true)),
            game_launcher: launcher.clone(),
            ..AppState::default()
        };

        let status = get_game_launch_status_inner(&state).await;
        let launch = launch_game_inner(&state, request(IfNotAppliedDto::LaunchAnyway)).await;

        assert_eq!(status.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
        assert_eq!(launch.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
        assert!(launcher.launched().is_empty());
    }

    #[tokio::test]
    async fn an_applied_order_is_ready_through_the_launchers_route() {
        let launcher = Arc::new(RecordingGameLauncher::steam());
        let state = state_with(&launcher, false);

        let status = get_game_launch_status_inner(&state).await.expect("status");

        assert_eq!(
            status,
            GameLaunchStatusDto::Ready(GameLaunchReadyDto {
                route: GameLaunchRouteDto::Steam
            })
        );
        assert_eq!(launcher.route_queries(), vec![PathBuf::from("game")]);
    }

    #[tokio::test]
    async fn an_order_the_file_lacks_needs_apply_with_its_reason() {
        let state = state_with(&Arc::new(RecordingGameLauncher::executable()), false);
        put_behind_the_file(&state);

        let status = get_game_launch_status_inner(&state).await.expect("status");

        assert_eq!(
            status,
            GameLaunchStatusDto::NeedsApply(GameLaunchNeedsApplyDto {
                route: GameLaunchRouteDto::Executable,
                reason: UnappliedReasonDto::OrderDiffers,
            })
        );
    }

    #[tokio::test]
    async fn unscanned_activation_changes_are_reported_as_their_own_reason() {
        let state = state_with(&Arc::new(RecordingGameLauncher::steam()), false);
        crate::commands::active_set::deactivate_mods_inner(
            &state,
            DeactivateRequestDto {
                ids: vec!["b".to_string()],
            },
        )
        .await
        .expect("stage a deactivation");

        let status = get_game_launch_status_inner(&state).await.expect("status");

        assert_eq!(
            status,
            GameLaunchStatusDto::NeedsApply(GameLaunchNeedsApplyDto {
                route: GameLaunchRouteDto::Steam,
                reason: UnappliedReasonDto::ActivationChangesNotScanned,
            })
        );
    }

    #[tokio::test]
    async fn a_running_game_is_reported_ahead_of_everything_else() {
        let state = state_with(&Arc::new(RecordingGameLauncher::unavailable()), true);

        let status = get_game_launch_status_inner(&state).await.expect("status");

        assert_eq!(status, GameLaunchStatusDto::GameRunning);
    }

    #[tokio::test]
    async fn an_install_with_nothing_to_start_is_unavailable() {
        let state = state_with(&Arc::new(RecordingGameLauncher::unavailable()), false);

        let status = get_game_launch_status_inner(&state).await.expect("status");

        assert_eq!(
            status,
            GameLaunchStatusDto::Unavailable(GameLaunchUnavailableDto {
                reason: LaunchUnavailableDto::ExecutableMissing
            })
        );
    }

    #[tokio::test]
    async fn launching_an_applied_order_starts_the_game_once_and_names_the_route() {
        let launcher = Arc::new(RecordingGameLauncher::steam());
        let state = state_with(&launcher, false);

        let launched = launch_game_inner(&state, request(IfNotAppliedDto::Refuse))
            .await
            .expect("launch");

        assert_eq!(launched.route, GameLaunchRouteDto::Steam);
        assert_eq!(launcher.launched(), vec![LaunchRoute::Steam]);
        assert_eq!(launcher.route_queries(), vec![PathBuf::from("game")]);
    }

    #[tokio::test]
    async fn a_launch_from_needs_apply_is_refused_unless_launch_anyway() {
        let launcher = Arc::new(RecordingGameLauncher::steam());
        let state = state_with(&launcher, false);
        put_behind_the_file(&state);

        let refused = launch_game_inner(&state, request(IfNotAppliedDto::Refuse)).await;
        assert_eq!(refused.unwrap_err().code, CommandErrorCode::OrderNotApplied);
        assert!(launcher.launched().is_empty());

        let anyway = launch_game_inner(&state, request(IfNotAppliedDto::LaunchAnyway))
            .await
            .expect("launch anyway");
        assert_eq!(anyway.route, GameLaunchRouteDto::Steam);
        assert_eq!(launcher.launched(), vec![LaunchRoute::Steam]);
    }

    #[tokio::test]
    async fn a_running_game_is_refused_as_rimworld_running_even_when_launching_anyway() {
        let launcher = Arc::new(RecordingGameLauncher::steam());
        let state = state_with(&launcher, true);

        let error = launch_game_inner(&state, request(IfNotAppliedDto::LaunchAnyway))
            .await
            .unwrap_err();

        assert_eq!(error.code, CommandErrorCode::RimworldRunning);
        assert!(launcher.launched().is_empty());
    }

    #[tokio::test]
    async fn an_install_with_nothing_to_start_is_refused_with_its_code() {
        let launcher = Arc::new(RecordingGameLauncher::unavailable());
        let state = state_with(&launcher, false);

        let error = launch_game_inner(&state, request(IfNotAppliedDto::Refuse))
            .await
            .unwrap_err();

        assert_eq!(error.code, CommandErrorCode::GameExecutableMissing);
    }

    #[tokio::test]
    async fn a_launcher_failure_maps_to_the_code_of_its_route() {
        let cases = [
            (
                RecordingGameLauncher::steam()
                    .failing_with(LaunchFailure::SteamRefused("no handler".to_string())),
                CommandErrorCode::SteamLaunchFailed,
            ),
            (
                RecordingGameLauncher::executable()
                    .failing_with(LaunchFailure::SpawnFailed("denied".to_string())),
                CommandErrorCode::GameStartFailed,
            ),
        ];

        for (launcher, expected) in cases {
            let state = state_with(&Arc::new(launcher), false);

            let error = launch_game_inner(&state, request(IfNotAppliedDto::Refuse))
                .await
                .unwrap_err();

            assert_eq!(error.code, expected);
        }
    }

    #[tokio::test]
    async fn launch_only_reads_the_session() {
        let state = state_with(&Arc::new(RecordingGameLauncher::steam()), false);
        let before = state
            .session
            .read()
            .expect("lock")
            .as_ref()
            .map(Session::order_on_disk);

        launch_game_inner(&state, request(IfNotAppliedDto::Refuse))
            .await
            .expect("launch");

        let after = state
            .session
            .read()
            .expect("lock")
            .as_ref()
            .map(Session::order_on_disk);
        assert_eq!(before, after);
    }
}
