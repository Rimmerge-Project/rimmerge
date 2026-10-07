//! The game-launch port: how an install is started, and the command that
//! starts it. Implemented by `rim-io`'s `SystemGameLauncher`, faked by
//! [`crate::test_support::FakeGameLauncher`].
//!
//! Whether the game is already running is *not* here: that probe stays in
//! `rim-io`, and its answer reaches [`crate::use_cases::LaunchGame`] as a
//! [`crate::GameProcess`] value.

use std::path::{Path, PathBuf};

use crate::game_launch::{LaunchRouteKind, LaunchUnavailable};

/// RimWorld's 64-bit Windows executable, in the install's root folder.
pub const RIMWORLD_EXECUTABLE: &str = "RimWorldWin64.exe";

/// The one executable a launch may start: `<game_dir>/RimWorldWin64.exe`.
///
/// The field is private and [`Self::of_install`] is the only constructor,
/// so a launch route can never name any other program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExecutable {
    path: PathBuf,
}

impl GameExecutable {
    /// `<game_dir>/RimWorldWin64.exe`. Pure; whether it exists is the
    /// adapter's check.
    #[must_use]
    pub fn of_install(game_dir: &Path) -> Self {
        Self {
            path: game_dir.join(RIMWORLD_EXECUTABLE),
        }
    }

    /// The executable's full path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// How to start this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRoute {
    /// Through Steam, which starts the copy its own manifest names.
    Steam,
    /// By running the install's own executable.
    Executable(GameExecutable),
}

impl LaunchRoute {
    /// The route without its payload, for status and reporting.
    #[must_use]
    pub fn kind(&self) -> LaunchRouteKind {
        match self {
            Self::Steam => LaunchRouteKind::Steam,
            Self::Executable(_) => LaunchRouteKind::Executable,
        }
    }
}

/// Starts RimWorld. One query (the button shows the state before any click)
/// and one command.
pub trait GameLauncher {
    /// Which route this install uses, or why it has none. Reads the
    /// filesystem; never writes.
    ///
    /// # Errors
    ///
    /// [`LaunchUnavailable`] when the install can't be started from here.
    fn route(&self, game_dir: &Path) -> Result<LaunchRoute, LaunchUnavailable>;

    /// Starts the game. Returns once the OS accepted the request; never
    /// waits for the game.
    ///
    /// # Errors
    ///
    /// [`LaunchFailure`] when the OS refused the request.
    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure>;
}

/// The OS refused to start the game.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchFailure {
    /// The OS refused the Steam run request (Steam missing, protocol not
    /// registered).
    #[error("Steam did not accept the launch request: {0}")]
    SteamRefused(String),
    /// Spawning `RimWorldWin64.exe` failed (access denied, bad image, ...).
    #[error("could not start RimWorldWin64.exe: {0}")]
    SpawnFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_executable_is_always_the_installs_rimworldwin64_exe() {
        let game_dir = Path::new("some").join("install");

        let executable = GameExecutable::of_install(&game_dir);

        assert_eq!(executable.path(), game_dir.join("RimWorldWin64.exe"));
    }

    #[test]
    fn a_route_reports_its_kind() {
        let executable = LaunchRoute::Executable(GameExecutable::of_install(Path::new("x")));

        assert_eq!(LaunchRoute::Steam.kind(), LaunchRouteKind::Steam);
        assert_eq!(executable.kind(), LaunchRouteKind::Executable);
    }
}
