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

/// Executable names Rimmerge may start from a RimWorld install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameExecutableName {
    /// Native Linux player executable.
    Linux,
    /// Native Linux 64-bit player executable.
    LinuxX86_64,
    /// Windows player executable, including Proton/Wine installs.
    Windows,
}

impl GameExecutableName {
    /// The filename shipped by RimWorld for this executable kind.
    #[must_use]
    pub const fn filename(self) -> &'static str {
        match self {
            Self::Linux => "RimWorldLinux",
            Self::LinuxX86_64 => "RimWorldLinux.x86_64",
            Self::Windows => RIMWORLD_EXECUTABLE,
        }
    }
}

/// The Steam client that owns the selected installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamClient {
    /// The operating system's registered Steam handler (Windows).
    SystemDefault,
    /// Steam installed directly on Linux.
    Native,
    /// The com.valvesoftware.Steam Flatpak client.
    Flatpak,
}

/// The one executable a launch may start, derived from the install directory.
///
/// The field is private; constructors only join the install directory with a
/// filename from the closed [`GameExecutableName`] enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExecutable {
    path: PathBuf,
}

impl GameExecutable {
    /// `<game_dir>/RimWorldWin64.exe`. Pure; whether it exists is the
    /// adapter's check.
    #[must_use]
    pub fn of_install(game_dir: &Path) -> Self {
        Self::of_install_named(game_dir, GameExecutableName::Windows)
    }

    /// Builds an executable for one of the known RimWorld player names.
    #[must_use]
    pub fn of_install_named(game_dir: &Path, name: GameExecutableName) -> Self {
        Self {
            path: game_dir.join(name.filename()),
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
    Steam(SteamClient),
    /// By running the install's own executable.
    Executable(GameExecutable),
}

impl LaunchRoute {
    /// The route without its payload, for status and reporting.
    #[must_use]
    pub fn kind(&self) -> LaunchRouteKind {
        match self {
            Self::Steam(_) => LaunchRouteKind::Steam,
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
    /// The OS refused to start the owning Steam client or its registered handler.
    #[error("Steam did not accept the launch request: {0}")]
    SteamRefused(String),
    /// Spawning the selected RimWorld executable failed.
    #[error("could not start the RimWorld executable: {0}")]
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
    fn known_executable_names_are_derived_from_the_install() {
        let game_dir = Path::new("some").join("install");

        assert_eq!(
            GameExecutable::of_install_named(&game_dir, GameExecutableName::Linux).path(),
            game_dir.join("RimWorldLinux")
        );
        assert_eq!(
            GameExecutable::of_install_named(&game_dir, GameExecutableName::LinuxX86_64).path(),
            game_dir.join("RimWorldLinux.x86_64")
        );
    }

    #[test]
    fn a_route_reports_its_kind() {
        let executable = LaunchRoute::Executable(GameExecutable::of_install(Path::new("x")));

        assert_eq!(
            LaunchRoute::Steam(crate::ports::SteamClient::SystemDefault).kind(),
            LaunchRouteKind::Steam
        );
        assert_eq!(executable.kind(), LaunchRouteKind::Executable);
    }
}
