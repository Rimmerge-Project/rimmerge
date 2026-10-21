//! [`GameProcessProbe`]: whether RimWorld's own process is currently
//! running — shared by every interface (`apps/desktop`, `apps/cli`) that
//! refuses to write `ModsConfig.xml` out from under a live game, so the
//! process-list scan and its exact match rule live in exactly one place.

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
#[cfg(target_os = "linux")]
use sysinfo::{ProcessStatus, UpdateKind};

/// Process names used by native Linux, Linux x86_64 and Windows/Proton builds.
pub(crate) const RIMWORLD_PROCESS_NAMES: &[&str] =
    &["rimworldlinux", "rimworldlinux.x86_64", "rimworldwin64.exe"];

fn is_rimworld_process_name(name: &str) -> bool {
    RIMWORLD_PROCESS_NAMES
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn is_rimworld_process(process: &sysinfo::Process) -> bool {
    #[cfg(target_os = "linux")]
    if process.status() == ProcessStatus::Zombie {
        return false;
    }
    is_rimworld_process_identity(
        &process.name().to_string_lossy(),
        process.exe().and_then(std::path::Path::file_name),
    )
}

fn is_rimworld_process_identity(name: &str, executable_name: Option<&std::ffi::OsStr>) -> bool {
    if let Some(executable_name) = executable_name {
        let executable_name = executable_name.to_string_lossy();
        if is_rimworld_process_name(&executable_name) {
            return true;
        }
        #[cfg(target_os = "linux")]
        if !["wine", "wine64", "wine-preloader", "wine64-preloader"]
            .iter()
            .any(|loader| executable_name.eq_ignore_ascii_case(loader))
        {
            return false;
        }
        #[cfg(not(target_os = "linux"))]
        return false;
    }
    if is_rimworld_process_name(name) {
        return true;
    }
    // Linux exposes at most 15 bytes of comm. No generic prefix matching:
    // these aliases apply only to missing executable paths or known Wine loaders.
    #[cfg(target_os = "linux")]
    {
        ["RimWorldLinux.x", "RimWorldWin64.e"]
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn process_refresh_kind() -> ProcessRefreshKind {
    let kind = ProcessRefreshKind::nothing();
    #[cfg(target_os = "linux")]
    let kind = kind.with_exe(UpdateKind::Always);
    kind
}

/// Probes whether RimWorld's own process is currently running. A trait
/// (not a bare function) so `apply`'s refusal path is testable with a
/// fake that returns a fixed answer, without touching the real process
/// list.
pub trait GameProcessProbe: Send + Sync {
    /// Whether a supported RimWorld process name (case-insensitive) exists.
    fn is_running(&self) -> bool;
}

/// The real probe: checks the live process list via `sysinfo`, refreshing
/// process names and, on Linux, executable paths, without memory/cpu/disk work.
#[derive(Debug, Default, Clone, Copy)]
pub struct SysinfoGameProcessProbe;

impl SysinfoGameProcessProbe {
    /// Builds the probe. Stateless — every call re-scans the live process
    /// list.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl GameProcessProbe for SysinfoGameProcessProbe {
    fn is_running(&self) -> bool {
        let mut system = System::new();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
        system.processes().values().any(is_rimworld_process)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn fixed_probe_reports_the_configured_value() {
        assert!(FixedProbe(true).is_running());
        assert!(!FixedProbe(false).is_running());
    }

    #[test]
    fn supported_process_names_are_case_insensitive() {
        assert!(is_rimworld_process_name("RimWorldLinux"));
        assert!(is_rimworld_process_name("RIMWORLDLINUX.X86_64"));
        assert!(is_rimworld_process_name("RimWorldWin64.EXE"));
        assert!(!is_rimworld_process_name("RimWorld"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn truncated_names_are_only_a_fallback_for_missing_executable_paths() {
        for name in ["RimWorldLinux.x", "RimWorldWin64.e"] {
            assert_eq!(name.len(), 15);
            assert!(is_rimworld_process_identity(name, None));
            assert!(!is_rimworld_process_identity(
                name,
                Some(std::ffi::OsStr::new("unrelated"))
            ));
        }
        for name in [
            "RimWorldLinux.xy",
            "RimWorldLinux.x-backup",
            "RimWorldWin64.ex",
            "RimWorldWin64",
            "RimWorld",
        ] {
            assert!(!is_rimworld_process_identity(name, None));
        }
        assert!(is_rimworld_process_identity(
            "unrelated",
            Some(std::ffi::OsStr::new("RimWorldLinux.x86_64"))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wine_loaders_fall_back_only_to_known_game_identities() {
        for loader in ["wine", "wine64", "wine-preloader", "WINE64-PRELOADER"] {
            for name in ["RimWorldWin64.exe", "RimWorldWin64.e"] {
                assert!(is_rimworld_process_identity(
                    name,
                    Some(std::ffi::OsStr::new(loader))
                ));
            }
            for name in ["RimWorldWin64.ex", "RimWorldWin64.e-backup", "unrelated"] {
                assert!(!is_rimworld_process_identity(
                    name,
                    Some(std::ffi::OsStr::new(loader))
                ));
            }
        }
        assert!(!is_rimworld_process_identity(
            "RimWorldWin64.e",
            Some(std::ffi::OsStr::new("wine64-backup"))
        ));
    }

    #[test]
    fn the_real_probe_does_not_panic_scanning_this_machines_process_list() {
        // Doesn't assert a value (whether RimWorld happens to be running
        // on the test machine is undefined) — just proves the scan itself
        // completes without panicking, on every CI/dev machine.
        let _ = SysinfoGameProcessProbe::new().is_running();
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "helper started only by the synthetic process regression"]
    fn synthetic_process_child() {
        use std::io::Write;
        println!("synthetic-ready");
        std::io::stdout().flush().expect("flush ready");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("wait for parent");
    }

    #[cfg(target_os = "linux")]
    struct ChildGuard(std::process::Child);
    #[cfg(target_os = "linux")]
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_exited_unreaped_synthetic_game_is_not_running() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().expect("scratch");
        let executable = dir.path().join("RimWorldLinux.x86_64");
        std::fs::copy(std::env::current_exe().expect("test binary"), &executable).expect("helper");
        let mut child = ChildGuard(
            Command::new(executable)
                .args([
                    "--exact",
                    "process::tests::synthetic_process_child",
                    "--ignored",
                    "--nocapture",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("synthetic game"),
        );
        assert!(
            BufReader::new(child.0.stdout.take().expect("stdout"))
                .lines()
                .any(|line| line.expect("ready") == "synthetic-ready")
        );
        let pid = sysinfo::Pid::from_u32(child.0.id());
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            process_refresh_kind(),
        );
        assert!(is_rimworld_process(
            system.process(pid).expect("live helper")
        ));
        writeln!(child.0.stdin.take().expect("stdin")).expect("allow helper to exit");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                process_refresh_kind(),
            );
            let process = system.process(pid).expect("unreaped child remains present");
            if process.status() == ProcessStatus::Zombie {
                assert!(is_rimworld_process_identity(
                    &process.name().to_string_lossy(),
                    None
                ));
                assert!(!is_rimworld_process(process));
                break;
            }
            assert!(Instant::now() < deadline, "helper did not become a zombie");
            std::thread::sleep(Duration::from_millis(2));
        }
        // ChildGuard waits only after the assertion, including on panic.
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_synthetic_executables_despite_linux_name_truncation() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let dir = tempfile::tempdir().expect("scratch");
        for (name, expected) in [
            ("RimWorldLinux.x86_64", true),
            ("RimWorldWin64.exe", true),
            ("RimWorldLinux.x86_64-backup", false),
            ("unrelated", false),
        ] {
            let executable = dir.path().join(name);
            std::fs::copy(std::env::current_exe().expect("test binary"), &executable)
                .expect("copy synthetic helper");
            let mut child = ChildGuard(
                Command::new(&executable)
                    .args([
                        "--exact",
                        "process::tests::synthetic_process_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("synthetic child"),
            );
            let output = BufReader::new(child.0.stdout.take().expect("stdout"));
            assert!(
                output
                    .lines()
                    .any(|line| line.expect("ready line") == "synthetic-ready")
            );
            let mut system = System::new();
            let pid = sysinfo::Pid::from_u32(child.0.id());
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                process_refresh_kind(),
            );
            let process = system.process(pid).expect("synthetic process present");
            assert_eq!(is_rimworld_process(process), expected, "{name}");
            if expected {
                assert!(process.name().as_encoded_bytes().len() <= 15);
            }
        }
    }
}
