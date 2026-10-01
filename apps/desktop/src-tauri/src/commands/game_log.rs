//! `import_game_log`: reads and parses a game log (a `Player.log` or an
//! in-game console snapshot, detected from content), attributes every failure/timer/etc. against the current session's own
//! active mods, and returns the summary — never cached or persisted on
//! the session (the log is chosen by the user each time; no
//! watching, no default path). A button on the `/startup` page and the
//! dashboard; every result states its kind and coverage, so a snapshot is
//! labelled, never mistaken for a whole session; the frontend holds the last result in `stores/session.ts`
//! for as long as it's useful (predicted-vs-observed, the def-cache note).

use std::path::PathBuf;

use rim_session::ports::{GameLogReader, KindChoice};
use rim_session::use_cases::ImportGameLog;

use crate::dto::game_log::{GameLogSummaryDto, ImportGameLogRequestDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// See [`import_game_log`]. Generic over the reader port so a test can
/// inject `rim_session::test_support::FakeGameLogReader` instead of the
/// real, file-backed one — production callers always pass
/// `state.adapters.game_log_reader`.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError::invalid_input`] when the log can't be read.
pub(crate) async fn import_game_log_with_reader<Reader>(
    state: &AppState,
    path: PathBuf,
    reader: Reader,
) -> Result<GameLogSummaryDto, CommandError>
where
    Reader: GameLogReader + Send + 'static,
{
    with_session(state, move |session| {
        let use_case = ImportGameLog::new(reader);
        let summary = use_case
            .execute(session, &path, KindChoice::Detect)
            .map_err(|error| CommandError::invalid_input(error.to_string()))?;
        Ok((&summary).into())
    })
    .await
}

/// Reads, parses, and attributes `request.path` against the currently
/// loaded session's own active mods.
///
/// # Errors
///
/// See [`import_game_log_with_reader`].
#[tauri::command]
pub async fn import_game_log(
    state: tauri::State<'_, AppState>,
    request: ImportGameLogRequestDto,
) -> Result<GameLogSummaryDto, CommandError> {
    let reader = state.adapters.game_log_reader;
    import_game_log_with_reader(&state, PathBuf::from(request.path), reader).await
}

#[cfg(test)]
mod tests {
    use rim_io::FileGameLogReader;
    use rim_session::ports::{
        GameLogError, LogCoverage, ParsedGameLog, RawPatchFailure, SnapshotCoverage,
    };
    use rim_session::test_support::FakeGameLogReader;

    use super::*;
    use crate::dto::game_log_coverage::{
        ConsoleFillDto, EndStateDto, GapEndDto, LogCoverageDto, LoggingGapDto,
    };
    use crate::test_support::session_fixture_with_temp_paths;

    #[tokio::test]
    async fn a_successful_parse_maps_to_the_dto() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
        let parsed = ParsedGameLog {
            patch_failures: vec![RawPatchFailure {
                mod_tag: "A Mod".to_string(),
                operation: "Verse.PatchOperationAdd(Defs/ThingDef)".to_string(),
                source_file: None,
                stack_trace: None,
            }],
            ..Default::default()
        };

        let dto = import_game_log_with_reader(
            &state,
            PathBuf::from("Player.log"),
            FakeGameLogReader::new(Ok(parsed)),
        )
        .await
        .expect("import_game_log");

        assert_eq!(dto.patch_failures.len(), 1);
        assert_eq!(
            dto.patch_failures[0].operation,
            "Verse.PatchOperationAdd(Defs/ThingDef)"
        );
    }

    #[tokio::test]
    async fn a_read_failure_maps_to_invalid_input() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);

        let result = import_game_log_with_reader(
            &state,
            PathBuf::from("Player.log"),
            FakeGameLogReader::new(Err(GameLogError("not a Player.log".to_string()))),
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::InvalidInput
        );
    }

    fn state_with_session() -> (tempfile::TempDir, AppState) {
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
        (temp_dir, state)
    }

    /// A console snapshot is imported and labelled as one (its kind and
    /// coverage ride on the summary), never refused.
    #[tokio::test]
    async fn a_console_snapshot_is_accepted_and_labelled_as_one() {
        let (_temp_dir, state) = state_with_session();
        let snapshot = ParsedGameLog {
            coverage: LogCoverage::ConsoleSnapshot(SnapshotCoverage {
                entries: 3,
                head_truncated: false,
            }),
            ..Default::default()
        };

        let dto = import_game_log_with_reader(
            &state,
            PathBuf::from("midgame1.txt"),
            FakeGameLogReader::new(Ok(snapshot)),
        )
        .await
        .expect("a console snapshot is accepted");

        assert_eq!(
            dto.coverage,
            LogCoverageDto::ConsoleSnapshot {
                entries: 3,
                fill: ConsoleFillDto::BelowCap,
                head_truncated: false,
            }
        );
    }

    /// An uncleared console copy holds the `RimWorld <version>` banner entry
    /// near its top (the game logs it into the console queue); the real
    /// reader must still call it a snapshot, so the UI labels it as one
    /// instead of presenting a partial summary as a `Player.log`.
    #[tokio::test]
    async fn an_uncleared_console_copy_with_the_banner_near_the_top_is_labelled_a_snapshot() {
        let (temp_dir, state) = state_with_session();
        let trace =
            "UnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)";
        let entry = |text: &str| format!("{text}\n{trace}\n\n");
        let copy = [
            entry("Command line arguments: -example"),
            entry("RimWorld 1.6.4104 rev1234"),
            entry("[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed"),
        ]
        .concat();
        let path = temp_dir.path().join("midgame1.txt");
        std::fs::write(&path, copy).expect("write the copy");

        let dto = import_game_log_with_reader(&state, path, FileGameLogReader::new())
            .await
            .expect("the copy is accepted");

        assert!(
            matches!(
                dto.coverage,
                LogCoverageDto::ConsoleSnapshot { entries: 3, .. }
            ),
            "{:?}",
            dto.coverage
        );
    }

    #[tokio::test]
    async fn a_player_log_with_logging_gaps_reports_them_and_the_lower_bound() {
        let (temp_dir, state) = state_with_session();
        let log = "RimWorld 1.6.4104 rev1234\n\
                   [Example] noisy line\n\
                   Reached max messages limit. Stopping logging to avoid spam.\n\
                   Message logging is now once again on.\n\
                   [Example] noisy line\n\
                   Reached max messages limit. Stopping logging to avoid spam.\n";
        let path = temp_dir.path().join("Player.log");
        std::fs::write(&path, log).expect("write the log");

        let dto = import_game_log_with_reader(&state, path, FileGameLogReader::new())
            .await
            .expect("the log is accepted");

        assert!(matches!(
            dto.coverage,
            LogCoverageDto::PlayerLog {
                end_state: EndStateDto::Truncated,
                ..
            }
        ));
        assert_eq!(
            dto.logging_gaps,
            vec![
                LoggingGapDto {
                    stop_line: 3,
                    end: GapEndDto::Resumed { line: 4 },
                },
                LoggingGapDto {
                    stop_line: 6,
                    end: GapEndDto::NeverResumed,
                },
            ]
        );
        let lower_bound = dto.lower_bound.expect("the noisy family spans a gap");
        assert_eq!((lower_bound.families, lower_bound.entries), (1, 2));
        // The banner entry, the 2 noisy entries and the 3 gap-message entries
        // make 6; the 2 noisy ones are the lower bound.
        assert_eq!(lower_bound.entries_percent, 33);
    }

    #[tokio::test]
    async fn a_player_log_with_no_gap_has_no_lower_bound() {
        let (temp_dir, state) = state_with_session();
        let path = temp_dir.path().join("Player.log");
        std::fs::write(
            &path,
            "RimWorld 1.6.4104 rev1234
[Example] a line
",
        )
        .expect("write the log");

        let dto = import_game_log_with_reader(&state, path, FileGameLogReader::new())
            .await
            .expect("the log is accepted");

        assert!(dto.logging_gaps.is_empty());
        assert_eq!(dto.lower_bound, None);
    }
}
