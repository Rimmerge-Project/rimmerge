//! [`GameProcessProbe`]: whether RimWorld's own process is currently
//! running — shared by every interface (`apps/desktop`, `apps/cli`) that
//! refuses to write `ModsConfig.xml` out from under a live game, so the
//! process-list scan and its exact match rule live in exactly one place.

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// The process names RimWorld runs under, lowercase: the Unity build's
/// `RimWorldWin64.exe` and the Waylith (1.7) build's `RimWorld.exe`. The
/// guard matches by name, not path, and does not know which layout the
/// configured install has: a copy of the game started from anywhere else
/// (a test harness, a second install) is still "RimWorld is running", and
/// refusing is the safe side.
pub(crate) const RIMWORLD_PROCESS_NAMES: [&str; 2] = ["rimworldwin64.exe", "rimworld.exe"];

/// Whether `process_name` is one of [`RIMWORLD_PROCESS_NAMES`],
/// case-insensitively.
fn is_rimworld_process_name(process_name: &str) -> bool {
    RIMWORLD_PROCESS_NAMES
        .iter()
        .any(|name| process_name.eq_ignore_ascii_case(name))
}

/// Probes whether RimWorld's own process is currently running. A trait
/// (not a bare function) so `apply`'s refusal path is testable with a
/// fake that returns a fixed answer, without touching the real process
/// list.
pub trait GameProcessProbe: Send + Sync {
    /// Whether a process named in [`RIMWORLD_PROCESS_NAMES`]
    /// (case-insensitive) currently exists.
    fn is_running(&self) -> bool;
}

/// The real probe: checks the live process list via `sysinfo`, refreshing
/// only what's needed to read each process's name
/// (`ProcessRefreshKind::nothing()` — no memory/cpu/disk/exe-path work
/// sysinfo would otherwise do per process).
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
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        system
            .processes()
            .values()
            .any(|process| is_rimworld_process_name(&process.name().to_string_lossy()))
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
    fn both_engine_builds_executable_names_count_as_the_game_in_any_case() {
        for name in [
            "RimWorldWin64.exe",
            "rimworldwin64.exe",
            "RimWorld.exe",
            "RIMWORLD.EXE",
        ] {
            assert!(is_rimworld_process_name(name), "{name}");
        }
    }

    #[test]
    fn other_process_names_do_not_count_as_the_game() {
        for name in [
            "RimWorldLauncher.exe",
            "rimworld",
            "explorer.exe",
            "rimworld.exe.bak",
            "",
        ] {
            assert!(!is_rimworld_process_name(name), "{name}");
        }
    }

    #[test]
    fn the_real_probe_does_not_panic_scanning_this_machines_process_list() {
        // Doesn't assert a value (whether RimWorld happens to be running
        // on the test machine is undefined) — just proves the scan itself
        // completes without panicking, on every CI/dev machine.
        let _ = SysinfoGameProcessProbe::new().is_running();
    }
}
