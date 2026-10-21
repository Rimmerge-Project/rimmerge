//! [`SystemGameLauncher`]: starts RimWorld, through Steam or by running the
//! install's own executable.
//!
//! Neither route blocks the caller until game exit or reads its output. Unix
//! children are reaped in the background, including Linux Steam clients.
//! The Steam rule itself lives in `rim_analyzer::infra::paths`; this adapter
//! only turns a route into an OS request.

use std::path::Path;
use std::process::{Command, Stdio};

use rim_analyzer::infra::paths::is_game_dir;
#[cfg(not(target_os = "linux"))]
use rim_analyzer::infra::paths::is_steam_managed_install;
use rim_session::LaunchUnavailable;
use rim_session::ports::{
    GameExecutable, GameExecutableName, GameLauncher, LaunchFailure, LaunchRoute, SteamClient,
};

/// The link the OS hands to the Steam client already on this machine. A
/// constant, never built from input; a test pins it to RimWorld's app id.
#[cfg(not(target_os = "linux"))]
const STEAM_RUN_URL: &str = "steam://run/294100";

/// `DETACHED_PROCESS`: the game gets no console of ours.
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// `CREATE_NEW_PROCESS_GROUP`: a Ctrl+C aimed at Rimmerge never reaches the
/// game.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// The real launcher. Holds the Steam-client lookup as a function so a test
/// can answer it without reading this machine's Steam.
#[derive(Debug, Clone, Copy)]
pub struct SystemGameLauncher {
    steam_route: fn(&Path) -> Result<Option<SteamClient>, LaunchUnavailable>,
}

impl SystemGameLauncher {
    /// Builds the launcher over this machine's real Steam roots.
    #[must_use]
    pub fn new() -> Self {
        Self {
            steam_route: system_steam_client,
        }
    }

    /// A launcher whose Steam lookup is injected, so route tests
    /// never depend on the machine they run on.
    #[cfg(test)]
    fn with_steam_check(
        steam_route: fn(&Path) -> Result<Option<SteamClient>, LaunchUnavailable>,
    ) -> Self {
        Self { steam_route }
    }
}

impl Default for SystemGameLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl GameLauncher for SystemGameLauncher {
    fn route(&self, game_dir: &Path) -> Result<LaunchRoute, LaunchUnavailable> {
        if let Some(client) = (self.steam_route)(game_dir)? {
            return Ok(LaunchRoute::Steam(client));
        }
        if !is_game_dir(game_dir) {
            return Err(LaunchUnavailable::ExecutableMissing);
        }
        find_executable(game_dir).map_or(Err(LaunchUnavailable::ExecutableMissing), |executable| {
            Ok(LaunchRoute::Executable(executable))
        })
    }

    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure> {
        match route {
            LaunchRoute::Steam(client) => launch_steam(*client),
            LaunchRoute::Executable(executable) => executable_command(executable)
                .and_then(|mut command| spawn_executable(&mut command))
                .map_err(|error| LaunchFailure::SpawnFailed(error.to_string())),
        }
    }
}

fn system_steam_client(game_dir: &Path) -> Result<Option<SteamClient>, LaunchUnavailable> {
    #[cfg(target_os = "linux")]
    {
        use rim_analyzer::infra::paths::{LinuxSteamClient, steam_install_for_game};
        steam_install_for_game(game_dir)
            .map(|install| {
                install.map(|install| match install.client {
                    LinuxSteamClient::Native => SteamClient::Native,
                    LinuxSteamClient::Flatpak => SteamClient::Flatpak,
                })
            })
            .map_err(|_| LaunchUnavailable::ExecutableMissing)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(is_steam_managed_install(game_dir).then_some(SteamClient::SystemDefault))
    }
}

fn launch_steam(client: SteamClient) -> Result<(), LaunchFailure> {
    #[cfg(target_os = "linux")]
    {
        if client == SteamClient::Flatpak {
            check_flatpak_steam(&mut Command::new("flatpak"))?;
        }
        let mut command = steam_command(client)
            .map_err(|error| LaunchFailure::SteamRefused(error.to_string()))?;
        spawn_executable(&mut command)
            .map_err(|error| LaunchFailure::SteamRefused(error.to_string()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        if client != SteamClient::SystemDefault {
            return Err(LaunchFailure::SteamRefused(
                "unsupported Steam client on this platform".into(),
            ));
        }
        open::that_detached(STEAM_RUN_URL)
            .map_err(|error| LaunchFailure::SteamRefused(error.to_string()))
    }
}

#[cfg(target_os = "linux")]
fn check_flatpak_steam(command: &mut Command) -> Result<(), LaunchFailure> {
    let status = command
        .args(["info", "--show-ref", "com.valvesoftware.Steam"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| LaunchFailure::SteamRefused(error.to_string()))?;
    if !status.success() {
        return Err(LaunchFailure::SteamRefused(format!(
            "com.valvesoftware.Steam is unavailable ({status}); install the Steam Flatpak client"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn steam_command(client: SteamClient) -> std::io::Result<Command> {
    use rim_analyzer::infra::paths::RIMWORLD_STEAM_APP_ID;
    use std::os::unix::process::CommandExt;
    let mut command = match client {
        SteamClient::Native => Command::new("steam"),
        SteamClient::Flatpak => {
            let mut command = Command::new("flatpak");
            command.args(["run", "com.valvesoftware.Steam"]);
            command
        }
        SteamClient::SystemDefault => {
            return Err(std::io::Error::other(
                "Linux Steam launches require an owning client",
            ));
        }
    };
    command.args(["-applaunch", RIMWORLD_STEAM_APP_ID]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.process_group(0);
    Ok(command)
}

#[cfg(unix)]
fn spawn_executable(command: &mut Command) -> std::io::Result<()> {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<std::process::Child>(1);
    // Prepare the waiter before spawning so a thread failure cannot orphan a child.
    std::thread::Builder::new()
        .name("rimmerge-game-wait".into())
        .spawn(move || {
            if let Ok(mut child) = receiver.recv() {
                match child.wait() {
                    Ok(status) if !status.success() => {
                        tracing::warn!(pid = child.id(), %status, "game or Steam client exited unsuccessfully");
                    }
                    Err(error) => {
                        tracing::warn!(pid = child.id(), %error, "failed to wait for game or Steam client");
                    }
                    Ok(_) => {}
                }
            }
        })?;
    let child = command.spawn()?;
    // Defensive cleanup if the waiter disappears before accepting the child.
    sender.send(child).map_err(|error| {
        let mut child = error.0;
        let _ = child.kill();
        let _ = child.wait();
        std::io::Error::other("game waiter exited before accepting the child")
    })
}

#[cfg(not(unix))]
fn spawn_executable(command: &mut Command) -> std::io::Result<()> {
    command.spawn().map(drop)
}

/// The command that runs `executable`: no arguments, no shell, started from
/// the install folder, in a separate process group, stdio closed.
fn executable_command(executable: &GameExecutable) -> std::io::Result<Command> {
    // Keep library aliases, but resolve relative paths before changing the cwd.
    let executable = std::path::absolute(executable.path())?;
    let mut command = Command::new(&executable);
    if let Some(install_dir) = executable.parent() {
        command.current_dir(install_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    Ok(command)
}

#[cfg(windows)]
const EXECUTABLE_CANDIDATES: &[GameExecutableName] = &[GameExecutableName::Windows];

#[cfg(not(windows))]
const EXECUTABLE_CANDIDATES: &[GameExecutableName] =
    &[GameExecutableName::LinuxX86_64, GameExecutableName::Linux];

fn find_executable(game_dir: &Path) -> Option<GameExecutable> {
    EXECUTABLE_CANDIDATES.iter().copied().find_map(|name| {
        let executable = GameExecutable::of_install_named(game_dir, name);
        let metadata = std::fs::symlink_metadata(executable.path()).ok()?;
        if !metadata.file_type().is_file() {
            return None;
        }
        #[cfg(unix)]
        if std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o111 == 0 {
            return None;
        }
        Some(executable)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_os = "linux"))]
    use rim_analyzer::infra::paths::RIMWORLD_STEAM_APP_ID;
    #[cfg(unix)]
    use rim_session::ports::GameExecutableName;
    use rim_session::ports::RIMWORLD_EXECUTABLE;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "helper started only by the synthetic reaping regression"]
    fn synthetic_launched_child() {
        use std::io::Write;
        println!("synthetic-pid={}", std::process::id());
        std::io::stdout().flush().expect("pid");
        let signal = std::path::PathBuf::from(
            std::env::var_os("RIMMERGE_SYNTHETIC_EXIT").expect("exit signal"),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !signal.is_file() {
            assert!(
                std::time::Instant::now() < deadline,
                "parent did not release the helper"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn direct_spawn_separates_process_group_and_reaps_child_without_blocking() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().expect("scratch");
        let executable = dir.path().join("RimWorldLinux.x86_64");
        fs::copy(std::env::current_exe().expect("test binary"), &executable).expect("helper");
        let output = dir.path().join("stdout");
        let signal = dir.path().join("exit");
        let mut command = executable_command(&GameExecutable::of_install_named(
            dir.path(),
            GameExecutableName::LinuxX86_64,
        ))
        .expect("absolute helper command");
        command
            .env("RIMMERGE_SYNTHETIC_EXIT", &signal)
            .args([
                "--exact",
                "game_launch::tests::synthetic_launched_child",
                "--ignored",
                "--nocapture",
            ])
            .stdout(fs::File::create(&output).expect("output"));
        spawn_executable(&mut command).expect("synthetic spawn");
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            let text = fs::read_to_string(&output).expect("helper output");
            if let Some(pid) = text.split_inclusive('\n').find_map(|line| {
                line.strip_prefix("synthetic-pid=")
                    .and_then(|pid| pid.strip_suffix('\n'))
                    .and_then(|pid| pid.parse::<u32>().ok())
            }) {
                break sysinfo::Pid::from_u32(pid);
            }
            assert!(Instant::now() < deadline, "helper did not publish its PID");
            std::thread::sleep(Duration::from_millis(2));
        };
        let mut system = sysinfo::System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        assert_ne!(
            system
                .process(pid)
                .expect("spawn returned while helper is alive")
                .status(),
            sysinfo::ProcessStatus::Zombie
        );
        let process_group = |pid| {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).expect("process stat");
            stat.rsplit_once(')')
                .expect("process name terminator")
                .1
                .split_whitespace()
                .nth(2)
                .expect("process group")
                .parse::<u32>()
                .expect("numeric process group")
        };
        let child_group = process_group(pid.as_u32());
        let parent_group = process_group(std::process::id());
        fs::write(&signal, "exit").expect("release helper");
        loop {
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[pid]),
                true,
                sysinfo::ProcessRefreshKind::nothing(),
            );
            if system.process(pid).is_none() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "finished child remains present (possibly zombie)"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            child_group,
            pid.as_u32(),
            "child leads its own process group"
        );
        assert_ne!(child_group, parent_group, "child leaves the parent's group");
    }

    #[test]
    fn direct_spawn_propagates_an_unavailable_executable() {
        let dir = tempfile::tempdir().expect("scratch");
        let mut command = executable_command(&GameExecutable::of_install_named(
            dir.path(),
            GameExecutableName::Linux,
        ))
        .expect("absolute helper command");
        assert_eq!(
            spawn_executable(&mut command)
                .expect_err("no helper executable")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    fn never_steam(_: &Path) -> Result<Option<SteamClient>, LaunchUnavailable> {
        Ok(None)
    }

    fn always_steam(_: &Path) -> Result<Option<SteamClient>, LaunchUnavailable> {
        Ok(Some(SteamClient::SystemDefault))
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

    #[cfg(unix)]
    fn install_with_linux_executable(name: GameExecutableName, executable_mode: u32) -> TempDir {
        use std::os::unix::fs::PermissionsExt;

        let dir = install_without_executable();
        let path = dir.path().join(name.filename());
        fs::write(&path, b"ELF").expect("linux executable");
        fs::set_permissions(&path, fs::Permissions::from_mode(executable_mode))
            .expect("permissions");
        dir
    }

    #[test]
    fn route_is_steam_for_a_steam_managed_install() {
        // Not an install on disk: the Steam answer must come first.
        let install = tempfile::tempdir().expect("temp dir");
        let launcher = SystemGameLauncher::with_steam_check(always_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Ok(LaunchRoute::Steam(SteamClient::SystemDefault)));
    }

    #[cfg(windows)]
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

    #[cfg(unix)]
    #[test]
    fn windows_executable_requires_a_steam_route_on_linux() {
        let install = install_with_executable();
        assert_eq!(
            SystemGameLauncher::with_steam_check(never_steam).route(install.path()),
            Err(LaunchUnavailable::ExecutableMissing)
        );
        assert_eq!(
            SystemGameLauncher::with_steam_check(always_steam).route(install.path()),
            Ok(LaunchRoute::Steam(SteamClient::SystemDefault))
        );
    }

    #[cfg(unix)]
    #[test]
    fn route_prefers_the_linux_x86_64_executable() {
        use std::os::unix::fs::PermissionsExt;

        let install = install_with_linux_executable(GameExecutableName::Linux, 0o755);
        let x86_64 = install
            .path()
            .join(GameExecutableName::LinuxX86_64.filename());
        fs::write(&x86_64, b"ELF").expect("linux executable");
        fs::set_permissions(&x86_64, fs::Permissions::from_mode(0o755)).expect("permissions");
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(
            route,
            Ok(LaunchRoute::Executable(GameExecutable::of_install_named(
                install.path(),
                GameExecutableName::LinuxX86_64
            )))
        );
    }

    #[cfg(unix)]
    #[test]
    fn route_uses_the_other_native_executable_when_x86_64_is_not_executable() {
        let install = install_with_linux_executable(GameExecutableName::Linux, 0o755);
        use std::os::unix::fs::PermissionsExt;
        let candidate = install
            .path()
            .join(GameExecutableName::LinuxX86_64.filename());
        fs::write(&candidate, b"ELF").expect("non executable candidate");
        fs::set_permissions(candidate, fs::Permissions::from_mode(0o644)).expect("permissions");
        assert_eq!(
            SystemGameLauncher::with_steam_check(never_steam).route(install.path()),
            Ok(LaunchRoute::Executable(GameExecutable::of_install_named(
                install.path(),
                GameExecutableName::Linux
            )))
        );
    }

    #[cfg(unix)]
    #[test]
    fn route_rejects_a_linux_executable_without_execute_permission() {
        let install = install_with_linux_executable(GameExecutableName::LinuxX86_64, 0o644);
        let launcher = SystemGameLauncher::with_steam_check(never_steam);

        let route = launcher.route(install.path());

        assert_eq!(route, Err(LaunchUnavailable::ExecutableMissing));
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

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_steam_url_is_run_with_rimworlds_app_id() {
        assert_eq!(
            STEAM_RUN_URL,
            format!("steam://run/{RIMWORLD_STEAM_APP_ID}")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn direct_launch_accepts_a_relative_install_directory() {
        use std::os::unix::fs::PermissionsExt;
        let cwd = std::env::current_dir().expect("current directory");
        let dir = tempfile::tempdir_in(&cwd).expect("scratch install");
        fs::create_dir_all(dir.path().join("Data/Core")).expect("core");
        fs::write(dir.path().join("Version.txt"), "1.6").expect("version");
        let executable = dir.path().join("RimWorldLinux");
        fs::write(&executable, "#!/bin/sh\nprintf started > started\n")
            .expect("synthetic executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("mode");
        let relative = dir.path().strip_prefix(&cwd).expect("relative install");
        assert!(!relative.is_absolute());
        let launcher = SystemGameLauncher::with_steam_check(never_steam);
        let route = launcher.route(relative).expect("relative install accepted");
        launcher.launch(&route).expect("relative install launches");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !dir.path().join("started").is_file() {
            assert!(
                std::time::Instant::now() < deadline,
                "helper did not run in the install directory"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flatpak_preflight_refuses_an_uninstalled_client() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("mock directory");
        let mock = dir.path().join("flatpak");
        let args = dir.path().join("args");
        fs::write(
            &mock,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE_ARGS\"\nexit \"$MOCK_EXIT\"\n",
        )
        .expect("mock");
        fs::set_permissions(&mock, fs::Permissions::from_mode(0o755)).expect("mode");
        for exit in ["0", "1"] {
            let result = check_flatpak_steam(
                Command::new(&mock)
                    .env("CAPTURE_ARGS", &args)
                    .env("MOCK_EXIT", exit),
            );
            if exit == "0" {
                result.expect("installed client");
            } else {
                assert!(matches!(result, Err(LaunchFailure::SteamRefused(message))
                    if message.contains("com.valvesoftware.Steam") && message.contains("exit status: 1")));
            }
            assert_eq!(
                fs::read_to_string(&args).expect("args"),
                "info\n--show-ref\ncom.valvesoftware.Steam\n"
            );
        }
        assert!(matches!(
            check_flatpak_steam(&mut Command::new(dir.path().join("missing"))),
            Err(LaunchFailure::SteamRefused(_))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_commands_target_the_owner_and_reap_mock_clients() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        for (client, program, expected) in [
            (SteamClient::Native, "steam", "-applaunch\n294100\n"),
            (
                SteamClient::Flatpak,
                "flatpak",
                "run\ncom.valvesoftware.Steam\n-applaunch\n294100\n",
            ),
        ] {
            let dir = tempfile::tempdir().expect("mock client directory");
            let args = dir.path().join("args");
            let pid_file = dir.path().join("pid");
            let mock = dir.path().join(program);
            fs::write(&mock, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE_ARGS\"\nprintf '%s\\n' \"$$\" > \"$CAPTURE_PID\"\nexit 23\n").expect("mock client");
            fs::set_permissions(&mock, fs::Permissions::from_mode(0o755))
                .expect("mock permissions");
            let mut command = steam_command(client).expect("owning client command");
            assert_eq!(command.get_program(), program);
            command
                .env("PATH", dir.path())
                .env("CAPTURE_ARGS", &args)
                .env("CAPTURE_PID", &pid_file);
            spawn_executable(&mut command).expect("spawn mock client");
            let deadline = Instant::now() + Duration::from_secs(5);
            let pid = loop {
                if let Some(pid) = fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok())
                {
                    break sysinfo::Pid::from_u32(pid);
                }
                assert!(
                    Instant::now() < deadline,
                    "mock client did not record its PID"
                );
                std::thread::sleep(Duration::from_millis(2));
            };
            let mut system = sysinfo::System::new();
            loop {
                system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::Some(&[pid]),
                    true,
                    sysinfo::ProcessRefreshKind::nothing(),
                );
                if system.process(pid).is_none() {
                    break;
                }
                assert!(Instant::now() < deadline, "mock client was not reaped");
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(
                fs::read_to_string(&args).expect("recorded arguments"),
                expected
            );
            fs::remove_file(&mock).expect("remove owning client");
            let mut missing = steam_command(client).expect("command remains pinned to owner");
            missing.env("PATH", dir.path());
            assert_eq!(
                spawn_executable(&mut missing)
                    .expect_err("no fallback to another client")
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
        assert!(matches!(
            launch_steam(SteamClient::SystemDefault),
            Err(LaunchFailure::SteamRefused(_))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn conflicting_steam_ownership_refuses_instead_of_launching_a_native_executable() {
        let install = install_with_linux_executable(GameExecutableName::Linux, 0o755);
        let launcher =
            SystemGameLauncher::with_steam_check(|_| Err(LaunchUnavailable::ExecutableMissing));
        assert_eq!(
            launcher.route(install.path()),
            Err(LaunchUnavailable::ExecutableMissing)
        );
    }

    #[test]
    fn the_executable_command_has_no_arguments_and_runs_from_the_install_folder() {
        // std exposes no getters for stdio or creation flags, so those are
        // pinned by review (DETACHED_PROCESS = 8, CREATE_NEW_PROCESS_GROUP = 512).
        let install_dir = Path::new("some").join("install");
        let executable = GameExecutable::of_install(&install_dir);

        let command = executable_command(&executable).expect("absolute command");
        let absolute_install = std::path::absolute(&install_dir).expect("absolute install");

        assert_eq!(
            command.get_program(),
            absolute_install.join(RIMWORLD_EXECUTABLE)
        );
        assert_eq!(command.get_args().count(), 0);
        assert_eq!(command.get_current_dir(), Some(absolute_install.as_path()));
    }

    #[test]
    fn the_executable_name_matches_the_probed_process_name() {
        assert!(
            crate::process::RIMWORLD_PROCESS_NAMES
                .iter()
                .any(|name| RIMWORLD_EXECUTABLE.eq_ignore_ascii_case(name))
        );
    }
}
