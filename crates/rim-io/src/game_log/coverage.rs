//! What a parse covers: the facts only a few entries carry (the engine's
//! logging-gap messages, Unity's exit footer and crash handler or the 1.7
//! engine's `Log closed.` line and crash entry, the banner lines), collected while the pipeline runs and combined, at the end, with
//! the class tallies into the port's [`LogCoverage`].
//!
//! **Positive evidence only.** A fact is recorded when a line says it. A log
//! that holds no footer and no crash line is `Truncated`; that is a statement
//! about what the file contains, not about how the game ended. No claim is
//! made about a config check (the engine logs no marker for one).

use std::collections::BTreeMap;

use rim_session::ports::{
    ClassTally, EndState, EngineInfoKind, EntryClass, GapEnd, LogCoverage, LoggingGap,
    MAX_BANNER_LINES, MAX_LOGGING_GAPS, MAX_REPORT_PATH_BYTES, PatchPhaseEvidence,
    PlayerLogCoverage, SnapshotCoverage, totals_of,
};

use super::patterns::{
    CRASH_HANDLER_LOCATION_PREFIX, CRASH_HANDLER_PREFIX, CRASH_REPORT_HEAD, MEMORY_FOOTER_HEAD,
    WAYLITH_CRASH_HEAD, WAYLITH_CRASH_LOCATION_PREFIX, WAYLITH_LOG_CLOSED_MESSAGE,
};
use super::segment::Framing;
use super::waylith::is_backlog_overflow;

/// Which marker the open entry is, so its continuation lines are read in
/// that light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenMarker {
    /// Nothing the collector reads continuation lines of.
    Other,
    /// A crash message (Unity's crash handler, or the 1.7 engine's
    /// `=== CRASH ===` entry), whose next line names a location.
    CrashHandler,
}

/// Whether a crash was logged, and where the handler said its report went.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrashFact {
    report_path: Option<String>,
}

/// What a finished collector hands back.
pub(super) struct CollectedCoverage {
    pub(super) coverage: LogCoverage,
    pub(super) logging_gaps: Vec<LoggingGap>,
    /// Gaps past [`MAX_LOGGING_GAPS`]: counted, not listed.
    pub(super) logging_gaps_dropped: u64,
    /// Crash-report paths cut to [`MAX_REPORT_PATH_BYTES`]: counted.
    pub(super) crash_report_paths_truncated: u64,
}

/// Gathers the coverage facts from the entries as they open and grow.
///
/// **Memory bound:** at most [`MAX_LOGGING_GAPS`] gaps, [`MAX_BANNER_LINES`]
/// banner lines and one crash path of [`MAX_REPORT_PATH_BYTES`] bytes.
pub(super) struct CoverageCollector {
    banner_lines: Vec<u64>,
    gaps: Vec<LoggingGap>,
    gaps_dropped: u64,
    /// The stop line of the gap still waiting for its resume message.
    open_stop_line: Option<u64>,
    /// Unity's memory footer or the 1.7 engine's `Log closed.` line.
    has_clean_exit_marker: bool,
    crash: Option<CrashFact>,
    open_marker: OpenMarker,
    crash_report_paths_truncated: u64,
    /// Whether the file's first head (its first non-blank line) is a trace
    /// start; `None` until a head is seen. Read only for a console copy.
    first_head_is_trace_start: Option<bool>,
}

impl CoverageCollector {
    pub(super) fn new() -> Self {
        Self {
            banner_lines: Vec::new(),
            gaps: Vec::new(),
            gaps_dropped: 0,
            open_stop_line: None,
            has_clean_exit_marker: false,
            crash: None,
            open_marker: OpenMarker::Other,
            crash_report_paths_truncated: 0,
            first_head_is_trace_start: None,
        }
    }

    /// An entry of `class` opens at `line` (1-based) with head `trimmed`;
    /// `is_trace_start` says whether that head would start a console copy's
    /// trace (decided by the segmenter, which knows the engine's shape).
    pub(super) fn begin(
        &mut self,
        class: EntryClass,
        line: u64,
        trimmed: &str,
        is_trace_start: bool,
    ) {
        self.open_marker = OpenMarker::Other;
        self.first_head_is_trace_start.get_or_insert(is_trace_start);
        let EntryClass::EngineInfo(kind) = class else {
            return;
        };
        match kind {
            EngineInfoKind::Banner => self.note_banner(line),
            EngineInfoKind::LoggingStopped => self.note_stop(line),
            EngineInfoKind::LoggingResumed => self.note_resume(line),
            EngineInfoKind::SessionMarker => self.note_session_marker(line, trimmed),
            EngineInfoKind::UnityRuntime => {}
        }
    }

    /// A continuation line of the open entry.
    pub(super) fn absorb(&mut self, trimmed: &str) {
        if self.open_marker != OpenMarker::CrashHandler {
            return;
        }
        let Some(crash) = self.crash.as_mut() else {
            return;
        };
        if crash.report_path.is_some() {
            return;
        }
        let location = trimmed
            .strip_prefix(CRASH_HANDLER_LOCATION_PREFIX)
            .or_else(|| trimmed.strip_prefix(WAYLITH_CRASH_LOCATION_PREFIX));
        if let Some(path) = location {
            let path = path.trim();
            self.crash_report_paths_truncated += u64::from(path.len() > MAX_REPORT_PATH_BYTES);
            crash.report_path = Some(cut_at_char_boundary(path, MAX_REPORT_PATH_BYTES));
        }
    }

    fn note_banner(&mut self, line: u64) {
        if self.banner_lines.len() < MAX_BANNER_LINES {
            self.banner_lines.push(line);
        }
    }

    /// A stop message opens a gap; one that arrives while a gap is still
    /// open leaves that gap without a resume.
    fn note_stop(&mut self, line: u64) {
        if let Some(previous) = self.open_stop_line.replace(line) {
            self.push_gap(LoggingGap {
                stop_line: previous,
                resume: GapEnd::NeverResumed,
            });
        }
    }

    /// A resume message closes the open gap; with none open (its stop
    /// scrolled out of a snapshot) it is not a gap this file can place.
    fn note_resume(&mut self, line: u64) {
        if let Some(stop_line) = self.open_stop_line.take() {
            self.push_gap(LoggingGap {
                stop_line,
                resume: GapEnd::Resumed { line },
            });
        }
    }

    fn push_gap(&mut self, gap: LoggingGap) {
        if self.gaps.len() < MAX_LOGGING_GAPS {
            self.gaps.push(gap);
        } else {
            self.gaps_dropped += 1;
        }
    }

    fn note_session_marker(&mut self, line: u64, trimmed: &str) {
        if trimmed == MEMORY_FOOTER_HEAD || trimmed == WAYLITH_LOG_CLOSED_MESSAGE {
            self.has_clean_exit_marker = true;
        } else if trimmed == CRASH_REPORT_HEAD {
            self.crash.get_or_insert(CrashFact { report_path: None });
        } else if trimmed.starts_with(CRASH_HANDLER_PREFIX) || trimmed == WAYLITH_CRASH_HEAD {
            self.crash.get_or_insert(CrashFact { report_path: None });
            self.open_marker = OpenMarker::CrashHandler;
        } else if is_backlog_overflow(trimmed) {
            // The writer dropped entries that came after its pre-start
            // backlog filled, and wrote them just before this line: a gap
            // that opens and closes here. Inferred from the decompiled
            // writer; no real log has shown it yet.
            self.push_gap(LoggingGap {
                stop_line: line,
                resume: GapEnd::Resumed { line },
            });
        }
    }

    /// Closes the open gap (the end of the input is its end) and builds the
    /// coverage of a parse read under `framing`.
    pub(super) fn finish(
        mut self,
        framing: Framing,
        classes: &BTreeMap<EntryClass, ClassTally>,
    ) -> CollectedCoverage {
        if let Some(stop_line) = self.open_stop_line.take() {
            self.push_gap(LoggingGap {
                stop_line,
                resume: GapEnd::NeverResumed,
            });
        }
        let logging_gaps = std::mem::take(&mut self.gaps);
        let logging_gaps_dropped = self.gaps_dropped;
        let crash_report_paths_truncated = self.crash_report_paths_truncated;
        let coverage = match framing {
            Framing::PlayerLog | Framing::WaylithLog => {
                LogCoverage::PlayerLog(self.player_log_coverage(classes))
            }
            Framing::ConsoleCopy | Framing::WaylithConsole => {
                LogCoverage::ConsoleSnapshot(SnapshotCoverage {
                    entries: totals_of(classes, 0, 0).entries,
                    head_truncated: self.first_head_is_trace_start == Some(true),
                })
            }
        };
        CollectedCoverage {
            coverage,
            logging_gaps,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        }
    }

    fn player_log_coverage(self, classes: &BTreeMap<EntryClass, ClassTally>) -> PlayerLogCoverage {
        let entries_of = |class| classes.get(&class).map_or(0, ClassTally::entries);
        let end_state = match (self.crash, self.has_clean_exit_marker) {
            (Some(crash), _) => EndState::Crashed {
                report_path: crash.report_path,
            },
            (None, true) => EndState::CleanExit,
            (None, false) => EndState::Truncated,
        };
        PlayerLogCoverage {
            passes: entries_of(EntryClass::EngineInfo(EngineInfoKind::Banner)),
            banner_lines: self.banner_lines,
            patch_phase: if entries_of(EntryClass::PatchFailure) == 0 {
                PatchPhaseEvidence::NoneLogged
            } else {
                PatchPhaseEvidence::PatchFailureLogged
            },
            end_state,
        }
    }
}

/// `text` cut to at most `max_bytes` bytes, on a character boundary.
fn cut_at_char_boundary(text: &str, max_bytes: usize) -> String {
    let end = (0..=max_bytes.min(text.len()))
        .rev()
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(0);
    text.get(..end).unwrap_or_default().to_string()
}
