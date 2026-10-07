//! Mapping the "Launch RimWorld" refusals and failures.

use rim_session::ports::LaunchFailure;
use rim_session::use_cases::LaunchGameError;

use super::{CommandError, CommandErrorCode};

impl From<LaunchGameError> for CommandError {
    /// `GameRunning` reuses [`CommandErrorCode::RimworldRunning`] (the same
    /// fact `apply` reports) but keeps its own message: `apply`'s says
    /// "or retry with force", which does not apply to a launch. The
    /// message stays English technical detail; the frontend words each
    /// code.
    fn from(error: LaunchGameError) -> Self {
        let code = match &error {
            LaunchGameError::GameRunning => CommandErrorCode::RimworldRunning,
            LaunchGameError::NotApplied(_) => CommandErrorCode::OrderNotApplied,
            LaunchGameError::Unavailable(_) => CommandErrorCode::GameExecutableMissing,
            LaunchGameError::Failed(LaunchFailure::SteamRefused(_)) => {
                CommandErrorCode::SteamLaunchFailed
            }
            LaunchGameError::Failed(LaunchFailure::SpawnFailed(_)) => {
                CommandErrorCode::GameStartFailed
            }
        };
        Self::new(code, error.to_string())
    }
}
