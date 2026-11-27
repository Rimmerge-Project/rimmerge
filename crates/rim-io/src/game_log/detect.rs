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
//! **The 1.7 (Waylith) engine is recognised only on evidence a Unity log
//! never has, and a Unity snapshot signal outranks all of it but one.** A
//! Waylith `Player.log` begins with its `=== <product> log ===` header block,
//! which decides at once (no Unity log has it). Otherwise the whole window is
//! read, and in this order:
//!
//! 1. a Unity trace start (above) anywhere: a console snapshot, even when
//!    the first line is timestamped (a 1.7 log never holds that signal);
//! 2. a timestamped entry line as the first non-blank line (an excerpt cut
//!    from a 1.7 log): a Waylith `Player.log`;
//! 3. any four-space Waylith frame (`    at Ns.Type.Method(Args) [0xNN]`,
//!    optionally ` in <file>:<line>`: [`waylith::is_frame`]), **no**
//!    timestamped entry header anywhere in the window, and a first line that
//!    is **not** Unity's `Mono path[0] = ` boot line (the certain Unity
//!    signal: no 1.7 file starts with it, so it vetoes this rule and the file
//!    falls through to the banner rule, whatever frame-shaped lines it
//!    holds): a Waylith console copy. Without that veto a Unity file would be
//!    safe from the frame rule only by Unity's two-space frame indentation
//!    and the trailing space it leaves after `:0 `. The frame's *shape* is the signal, not a `Verse.Log.*` call: in a
//!    vanilla run every console entry opens its trace with
//!    `Verse.Log.Message|Warning|Error(String text)`, but in a modded game
//!    the log-call frame can be absent (observed: 0 of 1,000 entries in a
//!    1,021-mod copy, whose traces open at the caller, e.g. a mod's
//!    `..cctor()` or `RuntimeHelpers.RunClassConstructor`). A 1.7
//!    `Player.log` holds those frames too, so a timestamped header (which a
//!    console copy never has) always wins over the frame, wherever it sits
//!    in the window. Frame plus header is 1.7 for certain, so that pair
//!    answers a Waylith `Player.log` even where the exact banner is also in
//!    the window (never a Unity `Player.log`). A `No stack trace.` line
//!    next to a frame is covered by the same rule;
//! 4. a bare `No stack trace.` (no Waylith frame, no Unity trace start): a
//!    Unity console snapshot;
//! 5. the exact banner: a `Player.log`;
//! 6. a timestamped entry header anywhere in the window (an excerpt that
//!    starts mid-entry, on a frame or a body line): a Waylith `Player.log`.
//!    It runs last, after every Unity check, so a Unity file that reaches it
//!    holds neither a snapshot signal nor the banner in its window, and the
//!    real-log directory tier (every real 1.6 log and console copy, none
//!    detected as Waylith) is the evidence it never fires on one;
//! 7. otherwise a console snapshot.
//!
//! A leading byte-order mark on the first line is ignored.
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
//! coverage claims. The same window has a cost on the 1.7 side: a user-cut
//! 1.7 `Player.log` excerpt that starts inside a long message body (the longest
//! real run without a timestamp is 355 lines, in a full-mods log) and whose
//! 200-line window holds Waylith frames but no timestamped header reads as a
//! console copy. The caller's
//! [`rim_session::ports::KindChoice::Force`] overrides either way.

use std::io::{self, BufRead};
use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::LogKind;

use super::lines::BoundedLines;
use super::patterns::{BANNER_PATTERN, COPY_TRACE_START_LINE, fixed_regex};
use super::segment::{Framing, NO_STACK_TRACE_LINE};
use super::waylith;

/// How a Unity `Player.log` opens (its first non-blank line). No 1.7 file can
/// start with it (a Waylith log opens with its header block or a timestamp),
/// so it vetoes the Waylith console reading.
const UNITY_FIRST_LINE_PREFIX: &str = "Mono path[0] = ";

/// The most lines detection reads.
pub(super) const PEEK_LINES: usize = 200;

/// The most kept bytes detection reads (a line past its own bound counts
/// only its kept head), so a few enormous lines cannot make the peek
/// expensive.
pub(super) const PEEK_BYTES: usize = 1024 * 1024;

static BANNER_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(BANNER_PATTERN));

/// Whether one trimmed line is a Unity console copy's trace start: the
/// signal only a Unity snapshot carries.
fn is_unity_snapshot_signal(trimmed: &str) -> bool {
    trimmed == COPY_TRACE_START_LINE
}

/// The kind of log `content` is, by content alone; see the module header
/// for the rule and the peek window's trade-off.
#[must_use]
pub fn detect_kind(content: &str) -> LogKind {
    detect_framing(content).kind()
}

/// The framing (kind and engine) of `content`, by content alone.
#[must_use]
pub fn detect_framing(content: &str) -> Framing {
    #[allow(clippy::expect_used)]
    detect_framing_in(content.as_bytes())
        .expect("invariant: reading an in-memory byte slice cannot fail")
}

/// How a Waylith `Player.log` announces itself on its first non-blank line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaylithStart {
    /// The `=== <product> log ===` header block: no Unity log has it.
    HeaderBlock,
    /// A timestamped entry line (an excerpt cut from the log).
    EntryLine,
}

/// How the first non-blank line opens a Waylith `Player.log`, if it does.
fn waylith_start(line: &str) -> Option<WaylithStart> {
    let line = line.trim_start_matches('\u{feff}');
    if waylith::is_log_header_line(line.trim()) {
        return Some(WaylithStart::HeaderBlock);
    }
    waylith::parse_entry_header(line).map(|_| WaylithStart::EntryLine)
}

/// What the peek window held, line by line.
#[derive(Default)]
struct PeekFindings {
    first_line_start: Option<WaylithStart>,
    /// The first non-blank line is Unity's `Mono path[0] = ` boot line.
    is_unity_first_line: bool,
    saw_banner: bool,
    saw_no_stack_trace: bool,
    saw_frame: bool,
    saw_entry_header: bool,
}

impl PeekFindings {
    /// Whether the window proves a Waylith console copy (or, with a
    /// timestamped header beside it, a Waylith `Player.log`): any four-space
    /// Waylith frame, whatever method it names. A `No stack trace.` line
    /// alone names no engine (both console copies can hold it, see the
    /// module header), so it never counts here. A Unity first line
    /// (`Mono path[0] = `) vetoes it: no 1.7 file starts that way.
    fn has_waylith_console_evidence(&self) -> bool {
        self.saw_frame && !self.is_unity_first_line
    }

    /// The framing the findings decide, once no Unity trace start and no
    /// header block was seen: the first line, then the Waylith console
    /// evidence (a console copy, or a `Player.log` when a timestamped header
    /// is in the window too: only 1.7 has either), then a bare
    /// `No stack trace.` (a Unity console copy), then the banner, then a
    /// timestamped entry header anywhere in the window (a 1.7 excerpt that
    /// starts mid-entry), else a console snapshot.
    fn framing(&self) -> Framing {
        if self.first_line_start == Some(WaylithStart::EntryLine) {
            Framing::WaylithLog
        } else if self.has_waylith_console_evidence() {
            if self.saw_entry_header {
                Framing::WaylithLog
            } else {
                Framing::WaylithConsole
            }
        } else if self.saw_no_stack_trace {
            Framing::ConsoleCopy
        } else if self.saw_banner {
            Framing::PlayerLog
        } else if self.saw_entry_header {
            Framing::WaylithLog
        } else {
            Framing::ConsoleCopy
        }
    }
}

/// [`detect_framing`] over a reader, consuming at most the peek window.
pub(super) fn detect_framing_in<Reader: BufRead>(reader: Reader) -> io::Result<Framing> {
    let mut lines = BoundedLines::new(reader);
    let mut bytes_read = 0usize;
    let mut findings = PeekFindings::default();
    let mut is_first_line_pending = true;
    for _ in 0..PEEK_LINES {
        let Some(line) = lines.next_line()? else {
            break;
        };
        let trimmed = line.text.trim();
        if is_first_line_pending && !trimmed.is_empty() {
            is_first_line_pending = false;
            findings.first_line_start = waylith_start(&line.text);
            findings.is_unity_first_line = line
                .text
                .trim_start_matches('\u{feff}')
                .starts_with(UNITY_FIRST_LINE_PREFIX);
            if findings.first_line_start == Some(WaylithStart::HeaderBlock) {
                return Ok(Framing::WaylithLog);
            }
        }
        if is_unity_snapshot_signal(trimmed) {
            return Ok(Framing::ConsoleCopy);
        }
        findings.saw_no_stack_trace = findings.saw_no_stack_trace || trimmed == NO_STACK_TRACE_LINE;
        findings.saw_frame = findings.saw_frame || waylith::is_frame(&line.text);
        findings.saw_banner = findings.saw_banner || BANNER_RE.is_match(trimmed);
        findings.saw_entry_header =
            findings.saw_entry_header || waylith::parse_entry_header(&line.text).is_some();
        bytes_read += line.text.len();
        if bytes_read >= PEEK_BYTES {
            break;
        }
    }
    Ok(findings.framing())
}
