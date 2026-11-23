//! `import_game_log`: reads and parses a game log (a `Player.log` or an
//! in-game console snapshot, detected from content), attributes every failure/timer/etc. against the current session's own
//! active mods, and returns the summary — never cached or persisted on
//! the session (the log is chosen by the user each time; no
//! watching, no default path). A button on the `/startup` page and the
//! dashboard; every result states its kind and coverage, so a snapshot is
//! labelled, never mistaken for a whole session; the frontend holds the last result in `stores/session.ts`
//! for as long as it's useful (predicted-vs-observed, the def-cache note).

use rim_session::ports::{GameLogReader, KindChoice};
use rim_session::use_cases::{ImportGameLog, attribute_game_log};
use rim_session::user_path::UserPath;
use tauri::async_runtime::spawn_blocking;

use crate::dto::game_log::{GameLogSummaryDto, ImportGameLogRequestDto};
use crate::error::{CommandError, parse_user_path};
use crate::state::{AppState, with_session};

/// See [`import_game_log`]. Generic over the reader port so a test can
/// inject `rim_session::test_support::FakeGameLogReader` instead of the
/// real, file-backed one — production callers always pass
/// `state.adapters.game_log_reader`.
///
/// A log can be hundreds of megabytes, so the read never holds the session
/// lock: a short `with_session` copies the project's log formats, the read and
/// parse run on a bare `spawn_blocking`, and a second short `with_session`
/// attributes the parsed log against the session as it is then (a pure join
/// over the active mods, no file access). Nothing is committed to the session,
/// so a project swapped in between just attributes the log against the new
/// project, which is what the user sees next anyway.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError::invalid_input`] when the log can't be read.
pub(crate) async fn import_game_log_with_reader<Reader>(
    state: &AppState,
    path: UserPath,
    reader: Reader,
) -> Result<GameLogSummaryDto, CommandError>
where
    Reader: GameLogReader + Send + 'static,
{
    let knowledge = with_session(state, |session| Ok(session.mod_knowledge().clone())).await?;
    let parsed = spawn_blocking(move || {
        ImportGameLog::new(reader)
            .read(path.as_path(), &knowledge.log_formats(), KindChoice::Detect)
            .map_err(|error| CommandError::invalid_input(error.to_string()))
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "game log read task panicked");
        Err(CommandError::internal("background task panicked"))
    })?;
    with_session(state, move |session| {
        let summary = attribute_game_log(session, parsed);
        Ok((&summary).into())
    })
    .await
}

/// Parses `request.path` (before anything else: no session, no file) and
/// imports it with the project's own reader.
///
/// # Errors
///
/// See [`import_game_log_with_reader`]; also `network_path_refused` for a
/// network share and [`CommandError::invalid_input`] for an empty, relative
/// or device path.
pub(crate) async fn import_game_log_inner(
    state: &AppState,
    request: ImportGameLogRequestDto,
) -> Result<GameLogSummaryDto, CommandError> {
    let path = parse_user_path(&request.path, "the log file")?;
    let reader = state.adapters.game_log_reader;
    import_game_log_with_reader(state, path, reader).await
}

/// Reads, parses, and attributes `request.path` against the currently
/// loaded session's own active mods.
///
/// # Errors
///
/// See `import_game_log_inner`.
#[tauri::command]
pub async fn import_game_log(
    state: tauri::State<'_, AppState>,
    request: ImportGameLogRequestDto,
) -> Result<GameLogSummaryDto, CommandError> {
    import_game_log_inner(&state, request).await
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
    use crate::test_support::{Gate, session_fixture_with_temp_paths};

    /// An absolute drive-letter path for a log that a fake reader never opens.
    fn log_path(name: &str) -> UserPath {
        UserPath::parse(&std::env::temp_dir().join(name).to_string_lossy())
            .expect("the temp folder is a drive path")
    }

    /// A reader whose read waits at a gate, then returns an empty log.
    struct GatedReader(std::sync::Arc<Gate>);

    impl GameLogReader for GatedReader {
        fn read(
            &self,
            _path: &std::path::Path,
            _formats: &rim_session::ports::LogFormats<'_>,
            _kind: KindChoice,
        ) -> Result<ParsedGameLog, GameLogError> {
            self.0.block();
            Ok(ParsedGameLog::default())
        }
    }

    /// While the log is read another session command must run; were the lock
    /// held across the read, the read would give up on its own timeout.
    #[tokio::test]
    async fn the_session_lock_is_free_while_the_log_is_read() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a.mod"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let gate = Gate::new();

        let import = import_game_log_with_reader(
            &state,
            log_path("Player.log"),
            GatedReader(std::sync::Arc::clone(&gate)),
        );
        let other_command = async {
            gate.wait_until_blocked().await;
            let meanwhile = with_session(&state, |session| Ok(session.working().ids().len())).await;
            gate.release();
            meanwhile
        };
        let (imported, meanwhile) = tokio::join!(import, other_command);

        assert_eq!(meanwhile.expect("a second command ran"), 1);
        assert!(gate.was_released(), "the lock blocked the read");
        assert!(imported.is_ok());
    }

    /// The command refuses a share from its text alone: with no project
    /// loaded the answer is still the path refusal (not `no_project_loaded`),
    /// so neither the session nor the file system was consulted.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_network_share_is_refused_before_the_session_or_the_file_is_consulted() {
        let state = AppState::default();

        for raw in [
            r"\\server\share\Player.log",
            r"\\?\UNC\server\share\Player.log",
        ] {
            let result = import_game_log_inner(
                &state,
                ImportGameLogRequestDto {
                    path: raw.to_string(),
                },
            )
            .await;

            assert_eq!(
                result.expect_err("refused").code,
                crate::error::CommandErrorCode::NetworkPathRefused,
                "{raw}"
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_device_or_relative_log_path_is_invalid_input() {
        let state = AppState::default();

        for raw in [r"Player.log", r"\\.\pipe\x", r"C:\x\CON"] {
            let result = import_game_log_inner(
                &state,
                ImportGameLogRequestDto {
                    path: raw.to_string(),
                },
            )
            .await;

            assert_eq!(
                result.expect_err("refused").code,
                crate::error::CommandErrorCode::InvalidInput,
                "{raw}"
            );
        }
    }

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
            log_path("Player.log"),
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
            log_path("Player.log"),
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
            log_path("midgame1.txt"),
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

        let dto = import_game_log_with_reader(
            &state,
            UserPath::parse(&path.to_string_lossy()).expect("drive path"),
            FileGameLogReader::new(),
        )
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

        let dto = import_game_log_with_reader(
            &state,
            UserPath::parse(&path.to_string_lossy()).expect("drive path"),
            FileGameLogReader::new(),
        )
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

        let dto = import_game_log_with_reader(
            &state,
            UserPath::parse(&path.to_string_lossy()).expect("drive path"),
            FileGameLogReader::new(),
        )
        .await
        .expect("the log is accepted");

        assert!(dto.logging_gaps.is_empty());
        assert_eq!(dto.lower_bound, None);
    }
}
