//! Kind detection: is a file a `Player.log` or a console snapshot, decided
//! by its content alone (a file's name never decides: the maintainer's real
//! names, `Player.log` and `midgame1.txt`, carry no kind, and a name that
//! disagrees with the content would only mislabel the summary).
//!
//! **The snapshot signal outranks the banner.** The game logs its
//! `RimWorld <version> rev<n>` banner through `Log.Message`, so it is also
//! an entry of a console that was never cleared, and with no launch options
//! it is the very first one (`Command line arguments:` is logged only when
//! there are arguments): the banner alone, even first, cannot tell the two
//! kinds apart. What can is the copy format. Every entry of a console copy
//! carries a stack trace that starts with Unity's `ExtractStackTrace` frame,
//! or the exact line `No stack trace.`; a `Player.log` holds neither in its
//! first lines (0 of 29 real ones within [`PEEK_LINES`]). So, reading the
//! whole peek window:
//!
//! - any copy trace-start or `No stack trace.` line: a console snapshot,
//!   wherever the banner is;
//! - otherwise the exact banner anywhere in the window: a `Player.log`;
//! - neither: a console snapshot, the reading that can only under-claim (a
//!   snapshot's coverage makes no claim about stages, where a
//!   `Player.log`'s would).
//!
//! The banner is the same exact pattern the pass counter uses
//! ([`BANNER_PATTERN`]), never a looser one.
//!
//! **Peek window and its trade-off.** At most [`PEEK_LINES`] lines and
//! [`PEEK_BYTES`] kept bytes are read (a line keeps at most
//! [`super::MAX_LINE_BYTES`], real lines run to ~735 KB). The 137 real
//! snapshots hold their first trace start by line 71; a real `Player.log`
//! holds its banner by line 22. A `Player.log` whose banner lies beyond the
//! window reads as a console snapshot; a snapshot whose first trace start
//! lies beyond the window reads as a console snapshot too, unless its
//! banner lies inside it, when it reads as a `Player.log`. A wider window
//! would catch these at the price of reading more of the file before
//! parsing it, and a wrong snapshot reading only loses the `Player.log`
//! coverage claims. The caller's
//! [`rim_session::ports::KindChoice::Force`] overrides either way.

use std::io::{self, BufRead};
use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::LogKind;

use super::lines::BoundedLines;
use super::patterns::{BANNER_PATTERN, COPY_TRACE_START_LINE, fixed_regex};
use super::segment::NO_STACK_TRACE_LINE;

/// The most lines detection reads.
pub(super) const PEEK_LINES: usize = 200;

/// The most kept bytes detection reads (a line past its own bound counts
/// only its kept head), so a few enormous lines cannot make the peek
/// expensive.
pub(super) const PEEK_BYTES: usize = 1024 * 1024;

static BANNER_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(BANNER_PATTERN));

/// Whether one trimmed line is a console copy's trace start or its
/// `No stack trace.` line: the signal only a snapshot carries.
fn is_snapshot_signal(trimmed: &str) -> bool {
    trimmed == COPY_TRACE_START_LINE || trimmed == NO_STACK_TRACE_LINE
}

/// The kind of log `content` is, by content alone; see the module header
/// for the rule and the peek window's trade-off.
#[must_use]
pub fn detect_kind(content: &str) -> LogKind {
    #[allow(clippy::expect_used)]
    detect_kind_in(content.as_bytes())
        .expect("invariant: reading an in-memory byte slice cannot fail")
}

/// [`detect_kind`] over a reader, consuming at most the peek window.
pub(super) fn detect_kind_in<Reader: BufRead>(reader: Reader) -> io::Result<LogKind> {
    let mut lines = BoundedLines::new(reader);
    let mut bytes_read = 0usize;
    let mut saw_banner = false;
    for _ in 0..PEEK_LINES {
        let Some(line) = lines.next_line()? else {
            break;
        };
        let trimmed = line.text.trim();
        if is_snapshot_signal(trimmed) {
            return Ok(LogKind::ConsoleSnapshot);
        }
        saw_banner = saw_banner || BANNER_RE.is_match(trimmed);
        bytes_read += line.text.len();
        if bytes_read >= PEEK_BYTES {
            break;
        }
    }
    Ok(if saw_banner {
        LogKind::PlayerLog
    } else {
        LogKind::ConsoleSnapshot
    })
}
