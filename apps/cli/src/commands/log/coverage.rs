//! The coverage view of `log import`: what kind of file was read, what it
//! covers, where the game itself stopped logging, and which counts that
//! makes lower bounds, as JSON and as text lines.

use rim_session::ports::{
    ConsoleFill, EndState, GapEnd, LogCoverage, LogKind, LoggingGap, PatchPhaseEvidence,
    PlayerLogCoverage, SnapshotCoverage,
};
use rim_session::use_cases::GameLogSummary;
use serde::Serialize;

use super::entries::format_class;
use crate::common::TerminalSafe;

/// The most logging gaps the text output lists; `--json` lists every one.
const TEXT_GAPS_LISTED: usize = 10;

/// The kind's stable output name.
pub(super) fn format_kind(kind: LogKind) -> &'static str {
    match kind {
        LogKind::PlayerLog => "player_log",
        LogKind::ConsoleSnapshot => "console_snapshot",
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum EndStateJson {
    CleanExit,
    Crashed { report_path: Option<String> },
    Truncated,
}

impl From<&EndState> for EndStateJson {
    fn from(state: &EndState) -> Self {
        match state {
            EndState::CleanExit => Self::CleanExit,
            EndState::Crashed { report_path } => Self::Crashed {
                report_path: report_path.clone(),
            },
            EndState::Truncated => Self::Truncated,
        }
    }
}

fn format_patch_phase(evidence: PatchPhaseEvidence) -> &'static str {
    match evidence {
        PatchPhaseEvidence::NoneLogged => "none_logged",
        PatchPhaseEvidence::PatchFailureLogged => "patch_failure_logged",
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum CoverageJson {
    PlayerLog {
        passes: u64,
        banner_lines: Vec<u64>,
        patch_phase: &'static str,
        end_state: EndStateJson,
    },
    ConsoleSnapshot {
        entries: u64,
        /// `below_cap`, `at_cap` or `over_cap`.
        console_fill: &'static str,
        /// The copy starts mid-entry (hand-cut).
        head_truncated: bool,
    },
}

fn format_console_fill(fill: ConsoleFill) -> &'static str {
    match fill {
        ConsoleFill::BelowCap => "below_cap",
        ConsoleFill::AtCap => "at_cap",
        ConsoleFill::OverCap => "over_cap",
    }
}

impl From<&LogCoverage> for CoverageJson {
    fn from(coverage: &LogCoverage) -> Self {
        match coverage {
            LogCoverage::PlayerLog(player_log) => Self::PlayerLog {
                passes: player_log.passes,
                banner_lines: player_log.banner_lines.clone(),
                patch_phase: format_patch_phase(player_log.patch_phase),
                end_state: (&player_log.end_state).into(),
            },
            LogCoverage::ConsoleSnapshot(snapshot) => Self::ConsoleSnapshot {
                entries: snapshot.entries,
                console_fill: format_console_fill(snapshot.fill()),
                head_truncated: snapshot.head_truncated,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GapEndJson {
    Resumed { line: u64 },
    NeverResumed,
}

#[derive(Debug, Serialize)]
pub(super) struct LoggingGapJson {
    stop_line: u64,
    end: GapEndJson,
}

impl From<&LoggingGap> for LoggingGapJson {
    fn from(gap: &LoggingGap) -> Self {
        Self {
            stop_line: gap.stop_line,
            end: match gap.resume {
                GapEnd::Resumed { line } => GapEndJson::Resumed { line },
                GapEnd::NeverResumed => GapEndJson::NeverResumed,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct LowerBoundFamilyJson {
    class: &'static str,
    key: String,
}

pub(super) fn logging_gaps_json(summary: &GameLogSummary) -> Vec<LoggingGapJson> {
    summary.logging_gaps.iter().map(Into::into).collect()
}

pub(super) fn lower_bound_families_json(summary: &GameLogSummary) -> Vec<LowerBoundFamilyJson> {
    summary
        .lower_bound_families()
        .into_iter()
        .map(|(class, key)| LowerBoundFamilyJson {
            class: format_class(class),
            key: key.as_str().to_string(),
        })
        .collect()
}

/// `count` with the singular or plural of a noun.
fn counted(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn describe_end_state(state: &EndState) -> String {
    match state {
        EndState::CleanExit => "clean exit (Unity's memory-statistics footer)".to_string(),
        EndState::Crashed { report_path: None } => {
            "crashed (Unity's crash handler; no report location follows it)".to_string()
        }
        EndState::Crashed {
            report_path: Some(path),
        } => format!("crashed (Unity's crash handler; report location {path})"),
        EndState::Truncated => "truncated (no exit footer)".to_string(),
    }
}

fn describe_patch_phase(evidence: PatchPhaseEvidence) -> &'static str {
    match evidence {
        PatchPhaseEvidence::NoneLogged => "no patch-phase evidence",
        PatchPhaseEvidence::PatchFailureLogged => "patch failures logged",
    }
}

fn describe_player_log(coverage: &PlayerLogCoverage) -> String {
    format!(
        "Player.log, {}, {}, ended: {}",
        counted(
            usize::try_from(coverage.passes).unwrap_or(usize::MAX),
            "startup pass",
            "startup passes"
        ),
        describe_patch_phase(coverage.patch_phase),
        describe_end_state(&coverage.end_state)
    )
}

fn describe_snapshot(coverage: &SnapshotCoverage) -> String {
    let entries = usize::try_from(coverage.entries).unwrap_or(usize::MAX);
    let caveat = match coverage.fill() {
        ConsoleFill::BelowCap => {
            "shows only what the console held; says nothing about stages it does not show"
        }
        ConsoleFill::AtCap => "at the console cap: older entries were dropped by the game",
        ConsoleFill::OverCap => {
            "more entries than one console holds: pasted copies or a segmentation problem; \
             what the game dropped cannot be said"
        }
    };
    let head = if coverage.head_truncated {
        "; the copy starts mid-entry (the first entry's text is missing)"
    } else {
        ""
    };
    format!(
        "console snapshot, {} \u{2014} {caveat}{head}",
        counted(entries, "entry", "entries")
    )
}

/// The one-line description of what the file covers.
pub(super) fn describe_coverage(coverage: &LogCoverage) -> String {
    match coverage {
        LogCoverage::PlayerLog(player_log) => describe_player_log(player_log),
        LogCoverage::ConsoleSnapshot(snapshot) => describe_snapshot(snapshot),
    }
}

/// The warning that leads a file's text output when the game stopped
/// logging, and nothing for a log with no gap.
pub(super) fn print_gap_warning(summary: &GameLogSummary) {
    let total = summary.logging_gaps.len();
    if total == 0 {
        return;
    }
    let unresumed = summary
        .logging_gaps
        .iter()
        .filter(|gap| gap.resume == GapEnd::NeverResumed)
        .count();
    let tail = if unresumed == 0 {
        String::new()
    } else {
        format!("; {unresumed} never resumed, so the rest of the file after it is unreliable")
    };
    println!(
        "WARNING: the game stopped writing messages {}{tail}. What it never wrote cannot be \
         recovered; at least the counts of families spanning a gap are lower bounds, and \
         any count may be short",
        counted(total, "time", "times")
    );
    for gap in summary.logging_gaps.iter().take(TEXT_GAPS_LISTED) {
        match gap.resume {
            GapEnd::Resumed { line } => {
                println!("  gap: line {} to line {line}", gap.stop_line);
            }
            GapEnd::NeverResumed => println!("  gap: line {} to the end", gap.stop_line),
        }
    }
    if total > TEXT_GAPS_LISTED {
        println!(
            "  ... and {} more (all are in the --json output)",
            total - TEXT_GAPS_LISTED
        );
    }
}

/// The `coverage:` line.
pub(super) fn print_coverage(summary: &GameLogSummary, is_kind_forced: bool) {
    let forced = if is_kind_forced {
        " [kind forced by --kind]"
    } else {
        ""
    };
    println!(
        "  coverage: {}{forced}",
        TerminalSafe::line(describe_coverage(&summary.coverage))
    );
}

/// How many families have a count that is a lower bound, for the `classes:`
/// section.
pub(super) fn print_lower_bound_note(summary: &GameLogSummary) {
    let families = summary.lower_bound_families();
    if families.is_empty() {
        return;
    }
    println!(
        "  {}, {}% of entries, span a logging gap: at least these families' counts are lower \
         bounds; any count may be short",
        counted(families.len(), "family", "families"),
        percent_of(summary.lower_bound_entries(), summary.totals().entries)
    );
}

/// `part` as a whole percent of `total`, rounded to nearest; `0` of nothing.
fn percent_of(part: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    let percent = (u128::from(part) * 100 + u128::from(total) / 2) / u128::from(total);
    u64::try_from(percent).unwrap_or(u64::MAX)
}

/// The scope a snapshot's title states: only what one console held.
pub(super) fn snapshot_scope(summary: &GameLogSummary) -> String {
    let entries = summary.totals().entries;
    format!(
        "only the {} the console held",
        counted(
            usize::try_from(entries).unwrap_or(usize::MAX),
            "entry",
            "entries"
        )
    )
}
