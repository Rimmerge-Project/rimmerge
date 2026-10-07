//! [`SystemGameLauncher`]: starts RimWorld, through Steam or by running the
//! install's own executable.
//!
//! Neither route waits for the game, reads its output, or writes anything.
//! The Steam rule itself lives in `rim_analyzer::infra::paths`; this adapter
//! only turns a route into an OS request.

use std::path::Path;
use std::process::{Command, Stdio};

use rim_analyzer::infra::paths::{is_game_dir, is_steam_managed_install};
use rim_session::LaunchUnavailable;
use rim_session::ports::{GameExecutable, GameLauncher, LaunchFailure, LaunchRoute};

/// The link Windows hands to the Steam client already on this machine. A
/// constant, never built from input; a test pins it to RimWorld's app id.
const STEAM_RUN_URL: &str = "steam://run/294100";

/// `DETACHED_PROCESS`: the game gets no console of ours.
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// `CREATE_NEW_PROCESS_GROUP`: a Ctrl+C aimed at Rimmerge never reaches the
/// game.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// The real launcher. Holds the Steam-install check as a function so a test
/// can answer it without reading this machine's Steam.
#[derive(Debug, Clone, Copy)]
pub struct SystemGameLauncher {
    is_steam_managed: fn(&Path) -> bool,
}

impl SystemGameLauncher {
    /// Builds the launcher over this machine's real Steam roots.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_steam_managed: is_steam_managed_install,
        }
    }

    /// A launcher whose Steam check is `is_steam_managed`, so route tests
    /// never depend on the machine they run on.
    #[cfg(test)]
    fn with_steam_check(is_steam_managed: fn(&Path) -> bool) -> Self {
        Self { is_steam_managed }
    }
}

impl Default for SystemGameLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl GameLauncher for SystemGameLauncher {
    fn route(&self, game_dir: &Path) -> Result<LaunchRoute, LaunchUnavailable> {
        if (self.is_steam_managed)(game_dir) {
            return Ok(LaunchRoute::Steam);
        }
        if !is_game_dir(game_dir) {
            return Err(LaunchUnavailable::ExecutableMissing);
        }
        let executable = GameExecutable::of_install(game_dir);
        // `symlink_metadata`, not `metadata`: a link could point anywhere,
        // and the only program a launch may start is the install's own file.
        let is_regular_file = std::fs::symlink_metadata(executable.path())
            .is_ok_and(|metadata| metadata.file_type().is_file());
        if is_regular_file {
            Ok(LaunchRoute::Executable(executable))
        } else {
            Err(LaunchUnavailable::ExecutableMissing)
        }
    }

    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure> {
        match route {
            LaunchRoute::Steam => open::that_detached(STEAM_RUN_URL)
                .map_err(|error| LaunchFailure::SteamRefused(error.to_string())),
            // Dropping the `Child` neither kills nor waits for the game.
            LaunchRoute::Executable(executable) => executable_command(executable)
                .spawn()
                .map(drop)
                .map_err(|error| LaunchFailure::SpawnFailed(error.to_string())),
        }
    }
}

/// The command that runs `executable`: no arguments, no shell, started from
/// the install folder, detached from our console, stdio closed.
fn executable_command(executable: &GameExecutable) -> Command {
    let mut command = Command::new(executable.path());
    if let Some(install_dir) = executable.path().parent() {
        command.current_dir(install_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use rim_analyzer::infra::paths::RIMWORLD_STEAM_APP_ID;
    use rim_session::ports::RIMWORLD_EXECUTABLE;
    use std::fs;
    use tempfile::TempDir;

    fn never_steam(_: &Path) -> bool {
        false
    }

    fn always_steam(_: &Path) -> bool {
        true
    }

    /// A folder that passes `is_game_dir`, without an executable.
    fn install_without_executable() -> TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("Version.txt"), "1.6.0 rev0").expect("version");
        fs::create_dir_all(dir.path().join("Data").join("Core")).expect("core");
        dir
    }

    fn install_with_executable() -> TempDir {
        let dir = install_without_executable();
        fs::write(dir.path().join(RIMWORLD_EXECUTABLE), b"MZ").expect("exe");
        dir
    }

    #[test]
    fn route_is_steam_for_a_steam_managed_install() {
        // Not an install on disk: the Steam answer must come first.
        let install = tempfile::tempdir().expect("temp dir");
        let launcher = SystemGameLauncher::with_steam_check(always_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Ok(LaunchRoute::Steam));
    }

    #[test]
    fn route_is_executable_for_an_install_with_the_exe() {
        let install = install_with_executable();
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(
            route,
            Ok(LaunchRoute::Executable(GameExecutable::of_install(
                install.path()
            )))
        );
    }

    #[test]
    fn route_is_unavailable_without_the_exe() {
        let install = install_without_executable();
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
    }

    #[test]
    fn route_refuses_a_folder_named_like_the_exe() {
        let install = install_without_executable();
        fs::create_dir(install.path().join(RIMWORLD_EXECUTABLE)).expect("dir");
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
    }

    // Creating a symlink on Windows needs a privilege dev machines may lack,
    // so the link itself is only exercised where it is free; the
    // non-regular-file refusal above runs everywhere.
    #[cfg(unix)]
    #[test]
    fn route_refuses_a_symlinked_exe() {
        let install = install_without_executable();
        let target = install.path().join("elsewhere.bin");
        fs::write(&target, b"MZ").expect("target");
        std::os::unix::fs::symlink(&target, install.path().join(RIMWORLD_EXECUTABLE))
            .expect("symlink");
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
    }

    #[cfg(windows)]
    #[test]
    fn route_refuses_a_symlinked_exe_on_windows() {
        const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
        let install = install_without_executable();
        let target = install.path().join("elsewhere.bin");
        fs::write(&target, b"MZ").expect("target");
        let link = install.path().join(RIMWORLD_EXECUTABLE);
        if let Err(error) = std::os::windows::fs::symlink_file(&target, &link) {
            assert_eq!(
                error.raw_os_error(),
                Some(ERROR_PRIVILEGE_NOT_HELD),
                "unexpected symlink error: {error}"
            );
            eprintln!("skipping: creating a symlink needs Developer Mode or admin");
            return;
        }
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
    }

    #[test]
    fn route_is_unavailable_for_a_folder_that_is_not_an_install() {
        let folder = tempfile::tempdir().expect("temp dir");
        fs::write(folder.path().join(RIMWORLD_EXECUTABLE), b"MZ").expect("exe");
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(folder.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
    }

    #[test]
    fn the_steam_url_is_run_with_rimworlds_app_id() {
        assert_eq!(
            STEAM_RUN_URL,
            format!("steam://run/{RIMWORLD_STEAM_APP_ID}")
        );
    }

    #[test]
    fn the_executable_command_has_no_arguments_and_runs_from_the_install_folder() {
        // std exposes no getters for stdio or creation flags, so those are
        // pinned by review (DETACHED_PROCESS = 8, CREATE_NEW_PROCESS_GROUP = 512).
        let install_dir = Path::new("some").join("install");
        let executable = GameExecutable::of_install(&install_dir);

        let command = executable_command(&executable);

        assert_eq!(command.get_program(), executable.path().as_os_str());
        assert_eq!(command.get_args().count(), 0);
        assert_eq!(command.get_current_dir(), Some(install_dir.as_path()));
    }

    #[test]
    fn the_executable_name_matches_the_probed_process_name() {
        assert!(RIMWORLD_EXECUTABLE.eq_ignore_ascii_case(crate::process::RIMWORLD_PROCESS_NAME));
    }
}
