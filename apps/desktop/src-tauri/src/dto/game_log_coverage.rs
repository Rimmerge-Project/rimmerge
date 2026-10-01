//! DTOs for what an imported game log covers: its kind (carried by the
//! coverage variant, so the two cannot disagree), how a `Player.log` ends,
//! how full a console snapshot is, where the engine itself stopped logging,
//! and which counts that makes lower bounds. Mirrors
//! [`rim_session::ports::LogCoverage`] and its parts one variant at a time;
//! the UI renders every sentence from these codes.

use rim_session::ports::{
    ConsoleFill, EndState, GapEnd, LogCoverage, LoggingGap, PatchPhaseEvidence,
};
use rim_session::use_cases::GameLogSummary;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Mirrors [`PatchPhaseEvidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PatchPhaseEvidenceDto {
    /// Nothing the log says proves the patch phase ran (not proof it did not).
    NoneLogged,
    /// At least one engine patch-failure entry exists.
    PatchFailureLogged,
}

impl From<PatchPhaseEvidence> for PatchPhaseEvidenceDto {
    fn from(value: PatchPhaseEvidence) -> Self {
        match value {
            PatchPhaseEvidence::NoneLogged => Self::NoneLogged,
            PatchPhaseEvidence::PatchFailureLogged => Self::PatchFailureLogged,
        }
    }
}

/// Mirrors [`EndState`]: how a `Player.log` ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EndStateDto {
    /// Unity's memory-statistics footer is present and no crash was logged.
    CleanExit,
    /// Unity's crash handler logged a crash.
    #[serde(rename_all = "camelCase")]
    Crashed {
        /// The crash-report location the log printed, when it holds it.
        report_path: Option<String>,
    },
    /// Neither marker: the log stops where the game stopped writing.
    Truncated,
}

impl From<&EndState> for EndStateDto {
    fn from(value: &EndState) -> Self {
        match value {
            EndState::CleanExit => Self::CleanExit,
            EndState::Crashed { report_path } => Self::Crashed {
                report_path: report_path.clone(),
            },
            EndState::Truncated => Self::Truncated,
        }
    }
}

/// Mirrors [`ConsoleFill`]: a snapshot's entry count against the console's
/// capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ConsoleFillDto {
    /// Fewer entries than the console holds: nothing says any were dropped.
    BelowCap,
    /// Exactly the console's capacity: older entries were dropped by the game.
    AtCap,
    /// More entries than one console holds (pasted copies or a segmentation
    /// problem).
    OverCap,
}

impl From<ConsoleFill> for ConsoleFillDto {
    fn from(value: ConsoleFill) -> Self {
        match value {
            ConsoleFill::BelowCap => Self::BelowCap,
            ConsoleFill::AtCap => Self::AtCap,
            ConsoleFill::OverCap => Self::OverCap,
        }
    }
}

/// Mirrors [`LogCoverage`]: one variant per log kind, each holding only the
/// facts that kind can state. The variant is the kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum LogCoverageDto {
    /// The game's own `Player.log`.
    #[serde(rename_all = "camelCase")]
    PlayerLog {
        /// Startup passes (`RimWorld <version>` banner entries).
        #[ts(type = "number")]
        passes: u64,
        /// Evidence the patch phase was reached.
        patch_phase: PatchPhaseEvidenceDto,
        /// How the log ends.
        end_state: EndStateDto,
    },
    /// A copy of the in-game debug console.
    #[serde(rename_all = "camelCase")]
    ConsoleSnapshot {
        /// Entries in the snapshot (a lower bound of what the game logged).
        #[ts(type = "number")]
        entries: u64,
        /// The entry count against the console's capacity.
        fill: ConsoleFillDto,
        /// The copy starts mid-entry (hand-cut).
        head_truncated: bool,
    },
}

impl From<&LogCoverage> for LogCoverageDto {
    fn from(value: &LogCoverage) -> Self {
        match value {
            LogCoverage::PlayerLog(player_log) => Self::PlayerLog {
                passes: player_log.passes,
                patch_phase: player_log.patch_phase.into(),
                end_state: (&player_log.end_state).into(),
            },
            LogCoverage::ConsoleSnapshot(snapshot) => Self::ConsoleSnapshot {
                entries: snapshot.entries,
                fill: snapshot.fill().into(),
                head_truncated: snapshot.head_truncated,
            },
        }
    }
}

/// Mirrors [`GapEnd`]: how a logging gap ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GapEndDto {
    /// The engine logged that it resumed, on this 1-based line.
    #[serde(rename_all = "camelCase")]
    Resumed {
        /// The resume line.
        #[ts(type = "number")]
        line: u64,
    },
    /// No resume line follows: everything after the stop line is unreliable.
    NeverResumed,
}

impl From<GapEnd> for GapEndDto {
    fn from(value: GapEnd) -> Self {
        match value {
            GapEnd::Resumed { line } => Self::Resumed { line },
            GapEnd::NeverResumed => Self::NeverResumed,
        }
    }
}

/// Mirrors [`LoggingGap`]: a stretch where the engine wrote no messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LoggingGapDto {
    /// The 1-based line of the stop message.
    #[ts(type = "number")]
    pub stop_line: u64,
    /// How the gap ended.
    pub end: GapEndDto,
}

impl From<&LoggingGap> for LoggingGapDto {
    fn from(value: &LoggingGap) -> Self {
        Self {
            stop_line: value.stop_line,
            end: value.resume.into(),
        }
    }
}

/// How much of the log a logging gap puts in doubt: the families whose line
/// span overlaps a gap, whose counts may be short. Derived in Rust from
/// [`GameLogSummary::lower_bound_families`]; the UI never recomputes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LowerBoundDto {
    /// Families whose span overlaps a gap (at least one).
    #[ts(type = "number")]
    pub families: u64,
    /// Entries those families hold.
    #[ts(type = "number")]
    pub entries: u64,
    /// `entries` as a whole percent of every entry in the log, rounded to
    /// nearest.
    #[ts(type = "number")]
    pub entries_percent: u64,
}

impl LowerBoundDto {
    /// The lower-bound summary of `summary`, or `None` when no family spans
    /// a gap.
    pub fn of(summary: &GameLogSummary) -> Option<Self> {
        let families = u64::try_from(summary.lower_bound_families().len()).unwrap_or(u64::MAX);
        if families == 0 {
            return None;
        }
        let entries = summary.lower_bound_entries();
        Some(Self {
            families,
            entries,
            entries_percent: percent_of(entries, summary.totals().entries),
        })
    }
}

/// `part` as a whole percent of `total`, rounded to nearest; `0` of nothing.
/// A part strictly between none and all never reads as `0` or `100`: it
/// clamps to `1`..=`99`, so the figure cannot claim "nothing" or
/// "everything" while some of the log is outside it.
fn percent_of(part: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    let percent = (u128::from(part) * 100 + u128::from(total) / 2) / u128::from(total);
    let percent = u64::try_from(percent).unwrap_or(u64::MAX);
    if part > 0 && part < total {
        percent.clamp(1, 99)
    } else {
        percent
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use rim_session::ports::{
        ClassTally, EntryClass, Family, FamilyKey, KindChoice, ParsedGameLog, PlayerLogCoverage,
        SnapshotCoverage,
    };
    use rim_session::test_support::FakeGameLogReader;
    use rim_session::use_cases::ImportGameLog;

    use super::*;
    use crate::test_support::session_fixture_with_temp_paths;

    fn family_spanning(count: u64, first_line: u64, last_line: u64) -> Family {
        Family {
            count,
            lines: count,
            first_line,
            last_line,
            count_by_pass: BTreeMap::new(),
            severity: None,
            sample: None,
            attribution_input: None,
        }
    }

    fn summary_with_gap_over(families: [(&str, Family); 2], gap: LoggingGap) -> GameLogSummary {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a.mod"]);
        let mut parsed = ParsedGameLog {
            logging_gaps: vec![gap],
            ..ParsedGameLog::default()
        };
        parsed.classes.insert(
            EntryClass::ModMessage,
            ClassTally {
                families: families
                    .into_iter()
                    .map(|(key, family)| (FamilyKey::new(key.to_string()), family))
                    .collect(),
                ..ClassTally::default()
            },
        );
        ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("the fake log imports")
    }

    fn snapshot_dto(entries: u64, head_truncated: bool) -> LogCoverageDto {
        (&LogCoverage::ConsoleSnapshot(SnapshotCoverage {
            entries,
            head_truncated,
        }))
            .into()
    }

    fn player_log_dto(end_state: EndState, patch_phase: PatchPhaseEvidence) -> LogCoverageDto {
        (&LogCoverage::PlayerLog(PlayerLogCoverage {
            passes: 2,
            banner_lines: vec![1, 900],
            patch_phase,
            end_state,
        }))
            .into()
    }

    #[test]
    fn a_player_log_maps_passes_patch_phase_and_each_end_state() {
        let crashed = EndState::Crashed {
            report_path: Some("C:\\scratch\\crash.dmp".to_string()),
        };
        let cases = [
            (EndState::CleanExit, EndStateDto::CleanExit),
            (
                crashed,
                EndStateDto::Crashed {
                    report_path: Some("C:\\scratch\\crash.dmp".to_string()),
                },
            ),
            (
                EndState::Crashed { report_path: None },
                EndStateDto::Crashed { report_path: None },
            ),
            (EndState::Truncated, EndStateDto::Truncated),
        ];

        for (end_state, expected) in cases {
            let dto = player_log_dto(end_state, PatchPhaseEvidence::PatchFailureLogged);

            assert_eq!(
                dto,
                LogCoverageDto::PlayerLog {
                    passes: 2,
                    patch_phase: PatchPhaseEvidenceDto::PatchFailureLogged,
                    end_state: expected,
                }
            );
        }
    }

    #[test]
    fn no_patch_phase_evidence_maps_to_none_logged() {
        let dto = player_log_dto(EndState::Truncated, PatchPhaseEvidence::NoneLogged);

        assert!(matches!(
            dto,
            LogCoverageDto::PlayerLog {
                patch_phase: PatchPhaseEvidenceDto::NoneLogged,
                ..
            }
        ));
    }

    #[test]
    fn a_snapshot_maps_its_fill_at_each_side_of_the_console_cap() {
        let fill_of = |entries| match snapshot_dto(entries, false) {
            LogCoverageDto::ConsoleSnapshot { fill, .. } => fill,
            other @ LogCoverageDto::PlayerLog { .. } => panic!("expected a snapshot: {other:?}"),
        };

        assert_eq!(fill_of(999), ConsoleFillDto::BelowCap);
        assert_eq!(fill_of(1_000), ConsoleFillDto::AtCap);
        assert_eq!(fill_of(1_001), ConsoleFillDto::OverCap);
    }

    #[test]
    fn a_snapshot_keeps_its_entry_count_and_head_truncation() {
        assert_eq!(
            snapshot_dto(392, true),
            LogCoverageDto::ConsoleSnapshot {
                entries: 392,
                fill: ConsoleFillDto::BelowCap,
                head_truncated: true,
            }
        );
    }

    #[test]
    fn coverage_serializes_as_a_camel_case_kind_tagged_union() {
        let player_log = serde_json::to_value(player_log_dto(
            EndState::Crashed {
                report_path: Some("crash.dmp".to_string()),
            },
            PatchPhaseEvidence::NoneLogged,
        ))
        .expect("serializes");
        let snapshot = serde_json::to_value(snapshot_dto(1_000, false)).expect("serializes");

        assert_eq!(
            player_log,
            serde_json::json!({
                "kind": "playerLog",
                "passes": 2,
                "patchPhase": "noneLogged",
                "endState": {"kind": "crashed", "reportPath": "crash.dmp"},
            })
        );
        assert_eq!(
            snapshot,
            serde_json::json!({
                "kind": "consoleSnapshot",
                "entries": 1000,
                "fill": "atCap",
                "headTruncated": false,
            })
        );
    }

    #[test]
    fn a_logging_gap_maps_a_resume_line_and_a_gap_that_never_resumed() {
        let resumed = LoggingGap {
            stop_line: 100,
            resume: GapEnd::Resumed { line: 140 },
        };
        let open = LoggingGap {
            stop_line: 900,
            resume: GapEnd::NeverResumed,
        };

        assert_eq!(
            LoggingGapDto::from(&resumed),
            LoggingGapDto {
                stop_line: 100,
                end: GapEndDto::Resumed { line: 140 },
            }
        );
        assert_eq!(
            LoggingGapDto::from(&open),
            LoggingGapDto {
                stop_line: 900,
                end: GapEndDto::NeverResumed,
            }
        );
        assert_eq!(
            serde_json::to_value(LoggingGapDto::from(&open)).expect("serializes"),
            serde_json::json!({"stopLine": 900, "end": {"kind": "neverResumed"}})
        );
    }

    #[test]
    fn the_percent_rounds_to_nearest_and_an_empty_log_is_zero() {
        assert_eq!(percent_of(1, 3), 33);
        assert_eq!(percent_of(2, 3), 67);
        assert_eq!(percent_of(5, 5), 100);
        assert_eq!(percent_of(0, 0), 0);
    }

    #[test]
    fn the_lower_bound_counts_the_families_spanning_a_gap_and_their_share() {
        let gap = LoggingGap {
            stop_line: 100,
            resume: GapEnd::Resumed { line: 200 },
        };
        let summary = summary_with_gap_over(
            [
                ("inside", family_spanning(2, 150, 160)),
                ("before", family_spanning(6, 10, 20)),
            ],
            gap,
        );

        assert_eq!(
            LowerBoundDto::of(&summary),
            Some(LowerBoundDto {
                families: 1,
                entries: 2,
                entries_percent: 25,
            })
        );
    }

    #[test]
    fn no_family_spanning_a_gap_means_no_lower_bound() {
        let gap = LoggingGap {
            stop_line: 500,
            resume: GapEnd::NeverResumed,
        };
        let summary = summary_with_gap_over(
            [
                ("one", family_spanning(2, 150, 160)),
                ("two", family_spanning(6, 10, 20)),
            ],
            gap,
        );

        assert_eq!(LowerBoundDto::of(&summary), None);
    }

    #[test]
    fn a_partial_share_never_rounds_to_zero_or_all() {
        assert_eq!(percent_of(1, 1_000), 1);
        assert_eq!(percent_of(999, 1_000), 99);
        assert_eq!(percent_of(0, 1_000), 0);
        assert_eq!(percent_of(1_000, 1_000), 100);
    }
}
