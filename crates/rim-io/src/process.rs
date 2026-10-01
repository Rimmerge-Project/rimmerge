//! [`GameProcessProbe`]: whether RimWorld's own process is currently
//! running — shared by every interface (`apps/desktop`, `apps/cli`) that
//! refuses to write `ModsConfig.xml` out from under a live game, so the
//! process-list scan and its exact match rule live in exactly one place.

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// The process name RimWorld's 64-bit Windows build runs under.
const RIMWORLD_PROCESS_NAME: &str = "rimworldwin64.exe";

/// Probes whether RimWorld's own process is currently running. A trait
/// (not a bare function) so `apply`'s refusal path is testable with a
/// fake that returns a fixed answer, without touching the real process
/// list.
pub trait GameProcessProbe: Send + Sync {
    /// Whether a process named [`RIMWORLD_PROCESS_NAME`]
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
        system.processes().values().any(|process| {
            process
                .name()
                .to_string_lossy()
                .eq_ignore_ascii_case(RIMWORLD_PROCESS_NAME)
        })
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
    fn the_real_probe_does_not_panic_scanning_this_machines_process_list() {
        // Doesn't assert a value (whether RimWorld happens to be running
        // on the test machine is undefined) — just proves the scan itself
        // completes without panicking, on every CI/dev machine.
        let _ = SysinfoGameProcessProbe::new().is_running();
    }
}
