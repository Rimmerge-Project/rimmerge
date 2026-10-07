//! [`LaunchGame`]: starts RimWorld, refusing when the game is running, the
//! install can't be started, or `ModsConfig.xml` is behind and the caller
//! didn't say to go ahead.
//!
//! Takes no [`crate::Session`]: the interface snapshots the facts under the
//! session lock and calls this outside it, so the lock is never held across
//! a process spawn. Nothing is persisted and nothing in the session changes,
//! so the snapshot-persist-rollback shape does not apply.

use std::path::PathBuf;

use crate::game_launch::{
    GameLaunchStatus, GameProcess, LaunchRouteKind, LaunchUnavailable, OrderOnDisk,
    UnappliedReason, game_launch_status,
};
use crate::ports::{GameLauncher, LaunchFailure, LaunchRoute};

/// What the interface knows when it asks: the install folder, whether the
/// file holds the selected order, and whether the game is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLaunchFacts {
    /// The configured game install folder.
    pub game_dir: PathBuf,
    /// [`crate::Session::order_on_disk`], read under the session lock.
    pub order: OrderOnDisk,
    /// The process probe's answer, read just before the call.
    pub game: GameProcess,
}

/// What [`LaunchGame::execute`] does when `ModsConfig.xml` is behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfNotApplied {
    /// Refuse with [`LaunchGameError::NotApplied`].
    Refuse,
    /// Launch anyway; the user chose to.
    LaunchAnyway,
}

/// Why [`LaunchGame::execute`] did not start the game.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchGameError {
    /// RimWorld is already running.
    #[error("RimWorld is already running")]
    GameRunning,
    /// `ModsConfig.xml` doesn't hold the selected order and the caller
    /// asked to refuse.
    #[error("ModsConfig.xml does not hold the selected order")]
    NotApplied(UnappliedReason),
    /// This install can't be started from here.
    #[error("this install has no RimWorldWin64.exe")]
    Unavailable(LaunchUnavailable),
    /// The OS refused the launch request.
    #[error(transparent)]
    Failed(#[from] LaunchFailure),
}

/// Reports the Launch RimWorld button's status and performs the launch.
pub struct LaunchGame<Launcher: GameLauncher> {
    launcher: Launcher,
}

impl<Launcher: GameLauncher> LaunchGame<Launcher> {
    /// A use case starting the game through `launcher`.
    #[must_use]
    pub fn new(launcher: Launcher) -> Self {
        Self { launcher }
    }

    /// Query: what the button shows.
    #[must_use]
    pub fn status(&self, facts: &GameLaunchFacts) -> GameLaunchStatus {
        let route = self.launcher.route(&facts.game_dir);
        status_of(facts, &route)
    }

    /// Command: re-reads the route (the status the button showed may be
    /// seconds old), re-derives the status, and launches only from `Ready`,
    /// or from `NeedsApply` under [`IfNotApplied::LaunchAnyway`].
    ///
    /// # Errors
    ///
    /// [`LaunchGameError::GameRunning`] and [`LaunchGameError::Unavailable`]
    /// always refuse; [`LaunchGameError::NotApplied`] refuses under
    /// [`IfNotApplied::Refuse`]; [`LaunchGameError::Failed`] when the OS
    /// refused the request.
    pub fn execute(
        &self,
        facts: &GameLaunchFacts,
        if_not_applied: IfNotApplied,
    ) -> Result<LaunchRouteKind, LaunchGameError> {
        let route = self.launcher.route(&facts.game_dir);
        let route = match (status_of(facts, &route), route) {
            (GameLaunchStatus::GameRunning, _) => return Err(LaunchGameError::GameRunning),
            (GameLaunchStatus::Unavailable(reason), _) => {
                return Err(LaunchGameError::Unavailable(reason));
            }
            (GameLaunchStatus::NeedsApply { reason, .. }, route) => match if_not_applied {
                IfNotApplied::Refuse => return Err(LaunchGameError::NotApplied(reason)),
                IfNotApplied::LaunchAnyway => route,
            },
            (GameLaunchStatus::Ready { .. }, route) => route,
        };
        // `status_of` derives from this same route, so reaching here with an
        // `Err` is impossible; reporting it as `Unavailable` keeps the match
        // total without a second copy of the precedence rules.
        let route = route.map_err(LaunchGameError::Unavailable)?;
        self.launcher.launch(&route)?;
        Ok(route.kind())
    }
}

fn status_of(
    facts: &GameLaunchFacts,
    route: &Result<LaunchRoute, LaunchUnavailable>,
) -> GameLaunchStatus {
    let kind = route
        .as_ref()
        .map(LaunchRoute::kind)
        .map_err(|reason| *reason);
    game_launch_status(facts.game, kind, facts.order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::GameExecutable;
    use crate::test_support::{FAKE_LAUNCHER_INSTALL, FakeGameLauncher};

    fn facts(game: GameProcess, order: OrderOnDisk) -> GameLaunchFacts {
        GameLaunchFacts {
            game_dir: PathBuf::from("game"),
            order,
            game,
        }
    }

    fn not_applied() -> OrderOnDisk {
        OrderOnDisk::NotApplied(UnappliedReason::OrderDiffers)
    }

    #[test]
    fn ready_launches_once_through_the_route_the_launcher_reported() {
        let use_case = LaunchGame::new(FakeGameLauncher::steam());

        let kind = use_case
            .execute(
                &facts(GameProcess::NotRunning, OrderOnDisk::Applied),
                IfNotApplied::Refuse,
            )
            .expect("an applied order launches");

        assert_eq!(kind, LaunchRouteKind::Steam);
        assert_eq!(use_case.launcher.launched(), vec![LaunchRoute::Steam]);
    }

    #[test]
    fn a_running_game_is_refused_without_calling_launch() {
        let use_case = LaunchGame::new(FakeGameLauncher::steam());

        let error = use_case
            .execute(
                &facts(GameProcess::Running, OrderOnDisk::Applied),
                IfNotApplied::Refuse,
            )
            .expect_err("a running game is refused");

        assert_eq!(error, LaunchGameError::GameRunning);
        assert!(use_case.launcher.launched().is_empty());
    }

    #[test]
    fn not_applied_is_refused_under_refuse_and_launched_under_launch_anyway() {
        let use_case = LaunchGame::new(FakeGameLauncher::executable());
        let behind = facts(GameProcess::NotRunning, not_applied());

        let refused = use_case.execute(&behind, IfNotApplied::Refuse);
        assert_eq!(
            refused,
            Err(LaunchGameError::NotApplied(UnappliedReason::OrderDiffers))
        );
        assert!(use_case.launcher.launched().is_empty());

        let launched = use_case.execute(&behind, IfNotApplied::LaunchAnyway);
        assert_eq!(launched, Ok(LaunchRouteKind::Executable));
        let launchers_own_route = LaunchRoute::Executable(GameExecutable::of_install(
            std::path::Path::new(FAKE_LAUNCHER_INSTALL),
        ));
        assert_eq!(use_case.launcher.launched(), vec![launchers_own_route]);
    }

    #[test]
    fn a_running_game_is_refused_even_under_launch_anyway() {
        let use_case = LaunchGame::new(FakeGameLauncher::steam());

        let result = use_case.execute(
            &facts(GameProcess::Running, not_applied()),
            IfNotApplied::LaunchAnyway,
        );

        assert_eq!(result, Err(LaunchGameError::GameRunning));
        assert!(use_case.launcher.launched().is_empty());
    }

    #[test]
    fn an_unavailable_install_is_refused_without_calling_launch() {
        let use_case = LaunchGame::new(FakeGameLauncher::unavailable());

        let result = use_case.execute(
            &facts(GameProcess::NotRunning, OrderOnDisk::Applied),
            IfNotApplied::LaunchAnyway,
        );

        assert_eq!(
            result,
            Err(LaunchGameError::Unavailable(
                LaunchUnavailable::ExecutableMissing
            ))
        );
        assert!(use_case.launcher.launched().is_empty());
    }

    #[test]
    fn execute_re_reads_the_route_instead_of_trusting_an_earlier_status() {
        let use_case = LaunchGame::new(FakeGameLauncher::steam());
        let applied = facts(GameProcess::NotRunning, OrderOnDisk::Applied);
        assert_eq!(
            use_case.status(&applied),
            GameLaunchStatus::Ready {
                route: LaunchRouteKind::Steam
            }
        );

        use_case.launcher.become_unavailable();
        let result = use_case.execute(&applied, IfNotApplied::Refuse);

        assert_eq!(
            result,
            Err(LaunchGameError::Unavailable(
                LaunchUnavailable::ExecutableMissing
            ))
        );
        assert_eq!(use_case.launcher.route_queries(), 2);
    }

    #[test]
    fn a_launcher_failure_surfaces_as_failed_with_its_cause() {
        let failure = LaunchFailure::SteamRefused("no handler".to_string());
        let use_case = LaunchGame::new(FakeGameLauncher::steam().failing_with(failure.clone()));

        let result = use_case.execute(
            &facts(GameProcess::NotRunning, OrderOnDisk::Applied),
            IfNotApplied::Refuse,
        );

        assert_eq!(result, Err(LaunchGameError::Failed(failure)));
    }

    #[test]
    fn status_reports_needs_apply_with_the_route_and_reason() {
        let use_case = LaunchGame::new(FakeGameLauncher::executable());

        let status = use_case.status(&facts(GameProcess::NotRunning, not_applied()));

        assert_eq!(
            status,
            GameLaunchStatus::NeedsApply {
                route: LaunchRouteKind::Executable,
                reason: UnappliedReason::OrderDiffers
            }
        );
    }
}
