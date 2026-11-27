//! The Waylith dialect: RimWorld 1.7's own engine writes a different
//! `Player.log` and a different console copy than Unity's. Everything that is
//! specific to that framing lives here; the Unity paths never call into it.
//!
//! **`Player.log` (confirmed by a real 1.7.5014 log and the decompiled
//! writer).** A header block (`=== <product> log ===` then `Started:`,
//! `Version:`, ... lines and a blank line), then one entry per
//! `yyyy-MM-dd HH:mm:ss.fff LEVEL[ [category]] message` line. The message may
//! span lines (column 0 or indented), and the stack the writer captured for
//! the entry follows it, one `    at <method> [0x<IL>]` line per frame
//! (optionally ` in <file>:<line>`). RimWorld's own `Log.*` entries carry the
//! category `Verse`; a mod's `Debug.Log*` carries `UnityDebug`; the engine's
//! own startup chatter carries its own categories. The explicit level
//! replaces Unity's per-arm severity inference. A clean exit is the writer's
//! `Log closed.` line; a crash is a `CRIT [Crash]` entry headed `=== CRASH ===`.
//!
//! **Console copy (confirmed by a real 1.7.5014 copy).** Each entry is its
//! message (one or more lines, continuation lines at column 0), then the
//! stack the console captured in the same frame shape as the `Player.log`
//! writer's (`    at <method> [0x<IL>]`, optionally ` in <file>:<line>`), then
//! one blank line. There is no timestamp and no level field: the first frame
//! is the logging call, `Verse.Log.Message|Warning|Error(String text)`, and
//! that method is the level. The version banner is an ordinary entry. That the
//! first frame is `Verse.Log.*` is confirmed for RimWorld's own entries in a
//! vanilla run only: a mod's `Debug.Log*` (the `UnityDebug` stub) may open with
//! another frame, in which case segmentation is unaffected (the trace start is
//! any four-space frame) and the severity falls back to the classifier's. A
//! stackless message is the single line `No stack trace.` in place of the
//! frames (the console copies `Verse.LogMessage.StackTrace`'s answer for a
//! null stack, e.g. under `-DisableLogStackTraces`): it starts the trace and
//! gives no severity.

use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::Severity;

use super::patterns::{
    WAYLITH_BACKLOG_OVERFLOW_PATTERN, WAYLITH_CRASH_HEAD, WAYLITH_ENTRY_HEADER_PATTERN,
    WAYLITH_FRAME_PATTERN, WAYLITH_LOG_CALL_ARGUMENTS_PREFIX, WAYLITH_LOG_CALL_FRAME_PREFIX,
    WAYLITH_LOG_CLOSED_MESSAGE, WAYLITH_LOG_HEADER_PATTERN, fixed_regex,
};

static ENTRY_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(WAYLITH_ENTRY_HEADER_PATTERN));
static LOG_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(WAYLITH_LOG_HEADER_PATTERN));
static FRAME_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(WAYLITH_FRAME_PATTERN));
static BACKLOG_OVERFLOW_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(WAYLITH_BACKLOG_OVERFLOW_PATTERN));

/// The category RimWorld's own `Log.*` calls carry.
const VERSE_CATEGORY: &str = "Verse";
/// The category a mod's `Debug.Log*` call carries (the Unity stub's own).
const UNITY_DEBUG_CATEGORY: &str = "UnityDebug";
/// The category of the crash entry.
const CRASH_CATEGORY: &str = "Crash";

/// The writer's level field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Level {
    Trace,
    Debug,
    Information,
    Warning,
    Error,
    Critical,
    /// The field was five spaces (a level the writer has no name for).
    Unnamed,
}

impl Level {
    fn from_field(field: &str) -> Self {
        match field {
            "TRACE" => Self::Trace,
            "DEBUG" => Self::Debug,
            " INFO" => Self::Information,
            " WARN" => Self::Warning,
            "ERROR" => Self::Error,
            " CRIT" => Self::Critical,
            _ => Self::Unnamed,
        }
    }

    /// The port's severity of this level; `None` for an unnamed level (the
    /// classifier's own severity then stands).
    pub(super) fn severity(self) -> Option<Severity> {
        match self {
            Self::Trace | Self::Debug | Self::Information => Some(Severity::Message),
            Self::Warning => Some(Severity::Warning),
            Self::Error | Self::Critical => Some(Severity::Error),
            Self::Unnamed => None,
        }
    }
}

/// One entry's first line, split into its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EntryHeader<'a> {
    pub(super) level: Level,
    pub(super) category: Option<&'a str>,
    /// The first line of the message (the rest are continuation lines).
    pub(super) message: &'a str,
}

/// The entry header `line` is, or `None` when it is not one. The cheap
/// positional checks first: the regex only runs on lines shaped like a
/// timestamp.
pub(super) fn parse_entry_header(line: &str) -> Option<EntryHeader<'_>> {
    let bytes = line.as_bytes();
    if bytes.len() < 25 || bytes[4] != b'-' || bytes[10] != b' ' || bytes[19] != b'.' {
        return None;
    }
    let captures = ENTRY_HEADER_RE.captures(line)?;
    Some(EntryHeader {
        level: Level::from_field(captures.name("level")?.as_str()),
        category: captures.name("category").map(|found| found.as_str()),
        message: captures.name("message")?.as_str(),
    })
}

/// Whether a trimmed line is the header block's first line.
pub(super) fn is_log_header_line(trimmed: &str) -> bool {
    trimmed.starts_with("=== ") && LOG_HEADER_RE.is_match(trimmed)
}

/// Whether `text` is a Waylith stack frame (see [`WAYLITH_FRAME_PATTERN`]).
pub(super) fn is_frame(text: &str) -> bool {
    text.starts_with("    at ") && FRAME_RE.is_match(text)
}

/// The severity a console copy's first frame gives its entry, when `text` is
/// the logging call `Verse.Log.Message|Warning|Error(String text)` (a Waylith
/// frame, see [`is_frame`]); `None` for any other line. A 1.7 `Player.log`
/// carries the same frame but its level is the entry header's, never this.
pub(super) fn console_log_severity(text: &str) -> Option<Severity> {
    let rest = text.strip_prefix(WAYLITH_LOG_CALL_FRAME_PREFIX)?;
    let (method, arguments) = rest.split_once('(')?;
    if !arguments.starts_with(WAYLITH_LOG_CALL_ARGUMENTS_PREFIX) || !is_frame(text) {
        return None;
    }
    match method {
        "Message" => Some(Severity::Message),
        "Warning" => Some(Severity::Warning),
        "Error" => Some(Severity::Error),
        _ => None,
    }
}

/// The text the per-line measurements (timers, def-cache lines) read: the
/// message of an entry's first line, else the line itself. An entry in one of
/// the engine's own categories reads as nothing: its "took 84 ms" is the
/// engine's (a GPU probe), not a mod's startup cost.
pub(super) fn measured_text(line: &str) -> &str {
    match parse_entry_header(line) {
        Some(header) if header.is_engine_category() => "",
        Some(header) => header.message,
        None => line,
    }
}

impl EntryHeader<'_> {
    /// Whether the category belongs to the engine's own chatter rather than
    /// to RimWorld or a mod.
    pub(super) fn is_engine_category(&self) -> bool {
        self.category
            .is_some_and(|category| category != VERSE_CATEGORY && category != UNITY_DEBUG_CATEGORY)
    }

    /// Whether this entry is one the writer itself produces and coverage
    /// reads: the clean-exit line, the crash head, or the backlog warning.
    pub(super) fn is_session_marker(&self) -> bool {
        match self.category {
            None => {
                self.message == WAYLITH_LOG_CLOSED_MESSAGE
                    || BACKLOG_OVERFLOW_RE.is_match(self.message)
            }
            Some(category) => category == CRASH_CATEGORY && self.message == WAYLITH_CRASH_HEAD,
        }
    }
}

/// Whether a trimmed message is the writer's pre-initialization backlog
/// overflow warning.
pub(super) fn is_backlog_overflow(trimmed: &str) -> bool {
    BACKLOG_OVERFLOW_RE.is_match(trimmed)
}
