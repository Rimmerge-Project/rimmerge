//! What a parsed game log covers: its kind, how a `Player.log` ends, how full
//! a console snapshot is, and where the engine itself stopped logging.
//!
//! A summary of a partial file must say it is partial, so every parse
//! carries a [`LogCoverage`]: one variant per [`LogKind`], each holding only
//! the facts that kind can state. **Only positive evidence is reported.** A
//! console snapshot never proves a stage was not reached, and no claim is
//! made about a config check: the engine logs no marker for one (it runs in
//! developer mode only, in parallel, without a start or end line).

use std::cmp::Ordering;
use std::fmt;

/// The most entries the game's in-memory console keeps: the engine's
/// `LogMessageQueue.maxMessages`. The queue is first-in-first-out, so once it
/// is full each new message evicts the **oldest** entry whole.
pub const CONSOLE_CAP: u64 = 1_000;

/// The most banner lines a [`PlayerLogCoverage`] lists (its `passes` count
/// is the authority past that).
pub const MAX_BANNER_LINES: usize = 16;

/// The most bytes of a crash-report path a [`EndState::Crashed`] keeps.
pub const MAX_REPORT_PATH_BYTES: usize = 1_024;

/// The two kinds of game log the import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogKind {
    /// The game's own `Player.log`: the whole session, from start-up to
    /// exit or crash.
    PlayerLog,
    /// A copy of the in-game debug console: at most [`CONSOLE_CAP`]
    /// entries, the oldest already evicted, with no start-up stage
    /// markers.
    ConsoleSnapshot,
}

impl fmt::Display for LogKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PlayerLog => "Player.log",
            Self::ConsoleSnapshot => "console snapshot",
        })
    }
}

/// How a read decides a file's [`LogKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KindChoice {
    /// By content, from the first lines: a console-copy trace start
    /// anywhere there means a console snapshot; otherwise the exact
    /// `RimWorld <version> rev<n>` banner means a `Player.log`; neither
    /// means a console snapshot, the reading that can only under-claim. A
    /// file name is never consulted.
    #[default]
    Detect,
    /// The caller's override; detection is skipped.
    Force(LogKind),
}

/// What evidence a `Player.log` holds that the patch phase was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatchPhaseEvidence {
    /// Nothing the log says proves the phase ran (which is not proof that
    /// it did not).
    #[default]
    NoneLogged,
    /// At least one `Patch operation ... failed` entry, which only the
    /// patch phase logs.
    PatchFailureLogged,
}

/// How a `Player.log` ends. Only Unity writes either end marker: RimWorld
/// logs nothing when the game quits normally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EndState {
    /// Unity's `Memory Statistics:` footer is present and no crash was
    /// logged.
    CleanExit,
    /// Unity's crash handler logged that it intercepted a crash (or wrote
    /// its `Crash!!!` report header). A crash outranks a footer.
    Crashed {
        /// The location the crash handler printed on the line after its
        /// message (at most [`MAX_REPORT_PATH_BYTES`] bytes), when the log
        /// holds that line.
        report_path: Option<String>,
    },
    /// Neither marker: the log stops where the game stopped writing (a
    /// force-close, a freeze, a copy taken while the game still ran).
    #[default]
    Truncated,
}

/// What a `Player.log` covers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerLogCoverage {
    /// Startup passes: `RimWorld <version>` banner entries. A pre-patching
    /// loader restarts the game in-process, so a real log holds two.
    pub passes: u64,
    /// The first [`MAX_BANNER_LINES`] banner lines, 1-based.
    pub banner_lines: Vec<u64>,
    /// Evidence the patch phase was reached.
    pub patch_phase: PatchPhaseEvidence,
    /// How the log ends.
    pub end_state: EndState,
}

/// How an entry count compares with the console's capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleFill {
    /// Fewer than [`CONSOLE_CAP`] entries: nothing says the game evicted
    /// any (a Clear before the copy also leaves a short snapshot).
    BelowCap,
    /// Exactly [`CONSOLE_CAP`] entries: the game evicted the older entries
    /// before the copy, so the snapshot's first entry is not the session's.
    AtCap,
    /// More entries than one console holds: the file is more than one copy
    /// (pasted together) or the entries were segmented wrongly. What was
    /// evicted cannot be said.
    OverCap,
}

/// What a console snapshot covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotCoverage {
    /// Entries in the snapshot. A merged run of identical messages is one
    /// entry, so this is a lower bound of what the game logged.
    pub entries: u64,
    /// The copy starts mid-entry: its first non-blank line is a Unity frame
    /// or `No stack trace.`, which only ever follows an entry's text (a
    /// hand-cut copy). The first entry's text is missing. Observed by the
    /// parser from the file's own first line.
    pub head_truncated: bool,
}

impl SnapshotCoverage {
    /// How [`Self::entries`] compares with the console's capacity. Derived,
    /// never stored.
    #[must_use]
    pub fn fill(&self) -> ConsoleFill {
        match self.entries.cmp(&CONSOLE_CAP) {
            Ordering::Less => ConsoleFill::BelowCap,
            Ordering::Equal => ConsoleFill::AtCap,
            Ordering::Greater => ConsoleFill::OverCap,
        }
    }
}

/// The coverage of one parse: one variant per [`LogKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogCoverage {
    /// A `Player.log`.
    PlayerLog(PlayerLogCoverage),
    /// A console snapshot.
    ConsoleSnapshot(SnapshotCoverage),
}

impl Default for LogCoverage {
    fn default() -> Self {
        Self::PlayerLog(PlayerLogCoverage::default())
    }
}

impl LogCoverage {
    /// The kind this coverage describes. Derived from the variant, so the
    /// kind and its facts cannot disagree.
    #[must_use]
    pub fn kind(&self) -> LogKind {
        match self {
            Self::PlayerLog(_) => LogKind::PlayerLog,
            Self::ConsoleSnapshot(_) => LogKind::ConsoleSnapshot,
        }
    }

    /// Whether the game itself dropped entries before the file was written:
    /// a console snapshot at the cap. (Logging gaps are a separate list.)
    #[must_use]
    pub fn is_missing_older_entries(&self) -> bool {
        match self {
            Self::PlayerLog(_) => false,
            Self::ConsoleSnapshot(snapshot) => snapshot.fill() == ConsoleFill::AtCap,
        }
    }
}

/// How a logging gap ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapEnd {
    /// The engine logged `Message logging is now once again on.` on this
    /// 1-based line.
    Resumed {
        /// The resume line.
        line: u64,
    },
    /// No resume line follows: everything after the stop line is
    /// unreliable.
    NeverResumed,
}

/// A stretch where the engine stopped writing messages: after 10,000 Unity
/// log lines it logs `Reached max messages limit. Stopping logging to avoid
/// spam.` and writes nothing more until it logs `Message logging is now once
/// again on.` What the game never wrote cannot be recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoggingGap {
    /// The 1-based line of the stop message.
    pub stop_line: u64,
    /// How the gap ended.
    pub resume: GapEnd,
}

impl LoggingGap {
    /// Whether a family whose entries span `first_line..=last_line` may have
    /// lost occurrences to this gap: its span overlaps the gap's. A gap
    /// that never resumed runs to the end of the file.
    #[must_use]
    pub fn overlaps(&self, first_line: u64, last_line: u64) -> bool {
        let starts_before_gap_ends = match self.resume {
            GapEnd::Resumed { line } => first_line <= line,
            GapEnd::NeverResumed => true,
        };
        starts_before_gap_ends && last_line >= self.stop_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn closed(stop_line: u64, resume_line: u64) -> LoggingGap {
        LoggingGap {
            stop_line,
            resume: GapEnd::Resumed { line: resume_line },
        }
    }

    const fn snapshot_of(entries: u64) -> SnapshotCoverage {
        SnapshotCoverage {
            entries,
            head_truncated: false,
        }
    }

    #[test]
    fn a_snapshot_is_below_at_or_over_the_console_cap() {
        assert_eq!(snapshot_of(999).fill(), ConsoleFill::BelowCap);
        assert_eq!(snapshot_of(1_000).fill(), ConsoleFill::AtCap);
        assert_eq!(snapshot_of(1_001).fill(), ConsoleFill::OverCap);
    }

    #[test]
    fn coverage_reports_the_kind_of_its_variant() {
        let snapshot = LogCoverage::ConsoleSnapshot(snapshot_of(3));

        assert_eq!(LogCoverage::default().kind(), LogKind::PlayerLog);
        assert_eq!(snapshot.kind(), LogKind::ConsoleSnapshot);
    }

    #[test]
    fn only_a_snapshot_at_the_cap_is_known_to_miss_older_entries() {
        let snapshot = |entries| LogCoverage::ConsoleSnapshot(snapshot_of(entries));

        assert!(snapshot(1_000).is_missing_older_entries());
        assert!(!snapshot(999).is_missing_older_entries());
        assert!(
            !snapshot(1_001).is_missing_older_entries(),
            "over the cap is reported as its own state, not as a known eviction"
        );
        assert!(!LogCoverage::default().is_missing_older_entries());
    }

    #[test]
    fn a_family_span_overlaps_a_closed_gap_when_the_ranges_meet() {
        let gap = closed(100, 200);

        assert!(gap.overlaps(150, 160), "inside");
        assert!(gap.overlaps(10, 100), "ends on the stop line");
        assert!(gap.overlaps(200, 300), "starts on the resume line");
        assert!(gap.overlaps(10, 300), "spans the whole gap");
        assert!(!gap.overlaps(10, 99), "ends before the stop");
        assert!(!gap.overlaps(201, 300), "starts after the resume");
    }

    #[test]
    fn a_gap_that_never_resumed_runs_to_the_end_of_the_file() {
        let gap = LoggingGap {
            stop_line: 100,
            resume: GapEnd::NeverResumed,
        };

        assert!(gap.overlaps(5_000, 6_000));
        assert!(!gap.overlaps(1, 99));
    }
}
