//! Reads and parses a RimWorld `Player.log`. [`parse`] is pure — a function
//! over `&str`, unit-testable directly against fixture text with no
//! filesystem involved — and produces attribution-free
//! [`rim_session::ports::ParsedGameLog`], whose mod references are still raw
//! strings (a `[Tag]` display name, a `file:` or block-trailer path): this
//! module never constructs a `ModId` or sees a `Report`, and cannot (`rim-io`
//! depends on `rim-analyzer`, but attributing a raw string to a specific
//! active mod needs the live scan's own mod list, which only
//! `rim_session::use_cases::ImportGameLog` has). [`FileGameLogReader`] is the
//! [`rim_session::ports::GameLogReader`] adapter: it decides the file's kind
//! by content ([`detect_kind`]), then streams it through the same single-pass
//! core as [`parse`] under that kind's [`Framing`]. A console snapshot is read,
//! not refused; its coverage says what it can and cannot tell.
//!
//! **One pass, bounded memory.** Lines are read one at a time into a reused
//! buffer ([`lines::BoundedLines`]) and fed to [`LogParser`]. Each line goes
//! to the entry pipeline ([`pipeline`]): a segmenter splits the lines into
//! entries (a head plus its continuation lines) or blank separators, a
//! classifier puts each entry in exactly one class (`Unclassified`
//! included), and an aggregator groups entries into families with a count, a
//! line span, the passes seen and one sample. The typed result lists (patch
//! failures and their stack-trace blocks, cross-references, DDS failures,
//! dependency warnings, load events) are read off the classified entries
//! ([`records`]); only timers and def-cache lines are taken from every line,
//! because a timer can be an indented continuation of another message. There
//! is no file-size or line-count refusal: each unit is bounded instead, and
//! every bound truncates **and counts** into
//! [`rim_session::ports::LogReadStats`]:
//!
//! - a line keeps its first [`lines::MAX_LINE_BYTES`] bytes (real logs hold
//!   lines of ~735 KB); the rest is dropped and `lines_truncated` counts it;
//! - a stack-trace block examines at most [`extract::MAX_BLOCK_LINES`] lines
//!   (`stack_block_lines_dropped`), and a multi-line join that outgrows one
//!   line's byte bound or [`extract::MAX_JOIN_LINES`] lines without forming an
//!   operation is abandoned (`stack_joins_abandoned`);
//! - a line with invalid UTF-8 is decoded lossily (`lines_with_invalid_utf8`).
//!
//! Working memory therefore does not grow with the file. The typed result
//! lists (one record per matching occurrence, for each class) do grow with the
//! number of occurrences.
//!
//! **Every line lands somewhere.** Nothing is dropped: `lines_read` equals the
//! lines of all entries plus the blank separators, and every bound that
//! truncates (families per class, samples, tracked passes, indexed
//! back-references) counts what it pushed out. A loose sentinel check flags
//! any line a known class's strict pattern missed.
//!
//! **Startup passes.** A real log holds two `RimWorld <version>` banners (a
//! pre-patching loader restarts the game in-process), and the game repeats
//! its startup warnings in each pass. A dependency warning is therefore kept
//! from the last pass that logged it; timers keep every pass, tagged with it; patch
//! failures pair with their stack-trace blocks within a pass ([`pairing`]).
//!
//! Pattern strings live in [`patterns`] (one constant per pattern, shared by
//! the classifier's set and the individual regexes). The three mod-produced
//! formats (a patch-reporting mod's stack-trace block, a texture loader's
//! fallback lines, a patching library's back-reference stubs) are **data**,
//! not code: they arrive as [`rim_session::ports::LogShapes`] inside the
//! [`LogFormats`] every read is handed, and [`shapes`] compiles them once per
//! parse. With no shapes a log still parses with every line accounted for;
//! those formats' lines then land in the classes the engine's own shapes give
//! them.
//!
//! **Line splitting is load-bearing** (the fixture's own header comment):
//! the real log is CRLF and contains a few lone `\r` characters outside
//! any CRLF pair. A line ends at `\n` and one `\r` directly before it is
//! stripped, which is exactly right; splitting on "any line break" is not —
//! it also splits a lone `\r`, shifting every later line. [`lines`] implements
//! that rule, matching `str::lines()`.

mod aggregate;
mod attribution;
mod classify;
mod coverage;
mod detect;
mod entry;
mod extract;
mod lines;
mod pairing;
mod patterns;
mod pipeline;
mod records;
mod segment;
mod sentinels;
mod shapes;

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek};
use std::path::Path;

use rim_session::ports::{
    DefCacheCarrier, GameLogError, GameLogReader, KindChoice, LogFormats, LogKind, LogReadStats,
    ParsedGameLog, RawTimer,
};

pub use self::detect::detect_kind;
use self::detect::detect_kind_in;
use self::extract::parse_timer_line;
pub use self::lines::MAX_LINE_BYTES;
use self::lines::{BoundedLines, ReadLine};
use self::pipeline::EntryPipeline;
pub use self::segment::Framing;
use self::shapes::CompiledShapes;
pub(crate) use self::shapes::{
    ShapeCompileError, check_back_reference, check_stack_block, check_texture_fallback,
};

// -- pure parsing ---------------------------------------------------------

/// Parses `content` into a [`ParsedGameLog`]. Pure: no filesystem, no
/// attribution, no [`rim_analyzer::domain::ModId`]. A thin wrapper over the
/// same single-pass core [`FileGameLogReader`] streams through, so it
/// splits, truncates and counts exactly as a file read does.
#[must_use]
pub fn parse(content: &str, formats: &LogFormats<'_>) -> ParsedGameLog {
    parse_as(content, formats, Framing::PlayerLog)
}

/// [`parse`] with an explicit [`Framing`]: `Framing::ConsoleCopy` parses an
/// in-game console copy (text, stack trace, blank line per entry) instead of
/// a `Player.log`, and the result's coverage is the matching kind's.
#[must_use]
pub fn parse_as(content: &str, formats: &LogFormats<'_>, framing: Framing) -> ParsedGameLog {
    #[allow(clippy::expect_used)]
    parse_stream(content.as_bytes(), formats, framing)
        .expect("invariant: reading an in-memory byte slice cannot fail")
}

impl From<LogKind> for Framing {
    fn from(kind: LogKind) -> Self {
        match kind {
            LogKind::PlayerLog => Self::PlayerLog,
            LogKind::ConsoleSnapshot => Self::ConsoleCopy,
        }
    }
}

/// Reads `reader` to the end, one bounded line at a time, through a
/// [`LogParser`] under `framing`.
fn parse_stream<Reader: BufRead>(
    reader: Reader,
    formats: &LogFormats<'_>,
    framing: Framing,
) -> io::Result<ParsedGameLog> {
    let mut lines = BoundedLines::new(reader);
    let shapes = CompiledShapes::new(formats.shapes);
    let mut parser = LogParser::new(formats.def_cache_carriers, &shapes, framing);
    while let Some(line) = lines.next_line()? {
        parser.feed(&line);
    }
    Ok(parser.finish())
}

/// The single-pass state behind [`parse`]: the entry pipeline (which also
/// yields the typed records) plus the two measurements taken on every line
/// whatever entry it belongs to, timers and def-cache lines.
struct LogParser<'formats> {
    carriers: &'formats [DefCacheCarrier],
    /// Segments every line into entries, classifies and aggregates them, and
    /// collects the typed records of the classes that have one.
    entries: EntryPipeline<'formats>,
    stats: LogReadStats,
    timers: Vec<RawTimer>,
    def_cache_lines: Vec<String>,
}

impl<'formats> LogParser<'formats> {
    fn new(
        carriers: &'formats [DefCacheCarrier],
        shapes: &'formats CompiledShapes,
        framing: Framing,
    ) -> Self {
        Self {
            carriers,
            entries: EntryPipeline::new(carriers, shapes, framing),
            stats: LogReadStats::default(),
            timers: Vec::new(),
            def_cache_lines: Vec::new(),
        }
    }

    /// Feeds one line.
    fn feed(&mut self, line: &ReadLine<'_>) {
        self.stats.lines_read += 1;
        self.stats.lines_truncated += u64::from(line.was_truncated);
        self.stats.lines_with_invalid_utf8 += u64::from(line.had_invalid_utf8);
        self.entries.feed(&line.text, &mut self.stats);
        self.tap_line(line.text.trim());
    }

    /// The two measurements that read every line, not every entry: a timer
    /// can be an indented continuation of another message (a mod's `Startup
    /// init took 61ms` under its own `initialized` line), and a def-cache
    /// line is data by its prefix. Runs after the entry pipeline has seen
    /// the line, so a timer carries the pass its own line belongs to.
    fn tap_line(&mut self, trimmed: &str) {
        if let Some(timer) = parse_timer_line(trimmed, self.entries.pass()) {
            self.timers.push(timer);
        }
        if self.is_def_cache_line(trimmed) {
            self.def_cache_lines.push(trimmed.to_string());
        }
    }

    /// Which prefix marks a def-cache-plugin line is data
    /// ([`DefCacheCarrier::log_line_prefix`]); an empty carrier slice
    /// collects nothing. Lines are kept verbatim (color tags left as-is for
    /// the consumer to strip/interpret).
    fn is_def_cache_line(&self, line: &str) -> bool {
        self.carriers
            .iter()
            .any(|carrier| line.starts_with(&carrier.log_line_prefix))
    }

    /// Closes whatever the end of the input leaves open and assembles the
    /// result.
    fn finish(mut self) -> ParsedGameLog {
        let entries = self.entries.finish(&mut self.stats);
        ParsedGameLog {
            patch_failures: entries.records.patch_failures,
            extra_stack_traces: entries.records.extra_stack_traces,
            cross_references: entries.records.cross_references,
            dds_failures: entries.records.dds_failures,
            dependency_warnings: entries.records.dependency_warnings,
            timers: self.timers,
            def_cache_lines: self.def_cache_lines,
            load_events: entries.records.load_events,
            read_stats: self.stats,
            classes: entries.classes,
            blank_separator_lines: entries.blank_separator_lines,
            sentinels: entries.sentinels,
            coverage: entries.coverage,
            logging_gaps: entries.logging_gaps,
        }
    }
}

// -- reading from disk -----------------------------------------------------

/// The read buffer size for a log file: large enough that a 75 MB log is a
/// few hundred reads, small enough to be irrelevant next to the result.
const READ_BUFFER_BYTES: usize = 256 * 1024;

/// [`GameLogReader`] backed by the real filesystem: decides the file's kind
/// by content (or takes the caller's override), then streams `path` through
/// the same single-pass core as [`parse`], with no size limit.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileGameLogReader;

impl FileGameLogReader {
    /// Builds the reader. Stateless — nothing to configure.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl GameLogReader for FileGameLogReader {
    fn read(
        &self,
        path: &Path,
        formats: &LogFormats<'_>,
        kind: KindChoice,
    ) -> Result<ParsedGameLog, GameLogError> {
        let fail = |error: io::Error| GameLogError(format!("{}: {error}", path.display()));
        let file = File::open(path).map_err(fail)?;
        let mut reader = BufReader::with_capacity(READ_BUFFER_BYTES, file);
        let kind = match kind {
            KindChoice::Force(kind) => kind,
            KindChoice::Detect => {
                let detected = detect_kind_in(&mut reader).map_err(fail)?;
                reader.rewind().map_err(|error| {
                    GameLogError(format!(
                        "{}: {error} (kind detection re-reads the file, which this path \
                         does not allow; force the kind, `--kind` on the command line, \
                         to skip detection)",
                        path.display()
                    ))
                })?;
                detected
            }
        };
        parse_stream(reader, formats, kind.into()).map_err(fail)
    }
}

// Sibling-file tests; `#[path]` keeps each module named as declared.
#[cfg(test)]
#[path = "game_log/game_log_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "game_log/entry_tests.rs"]
mod entry_tests;

#[cfg(test)]
#[path = "game_log/coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
#[path = "game_log/detect_tests.rs"]
mod detect_tests;
