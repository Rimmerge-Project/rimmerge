//! Game-log ports: the parsed `Player.log` and its reader.

use std::collections::BTreeMap;
use std::path::Path;

use super::{DefCacheCarrier, KindChoice, LogCoverage, LogShapes, LoggingGap};

/// One line inside a stack-trace block: an operation's own class, its
/// detail (the parenthesized text — an xpath for most op kinds, a mod
/// display name for `Verse.PatchOperationFindMod`, absent for
/// `Verse.PatchOperationSequence`'s bare `Error in the operation at
/// position=N`), and why it failed. `xpath`/`branch` are derived from
/// `detail`/`reason` once, at parse time, rather than recomputed by
/// every reader of this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTraceOp {
    /// The operation's own class, e.g. `Verse.PatchOperationAdd`.
    pub class: String,
    /// The raw parenthesized text, verbatim, when present.
    pub detail: Option<String>,
    /// The xpath text, extracted from `detail` when it starts with the
    /// stack-block shape's xpath prefix (quoted or not) — `None` when
    /// `detail` names something else (a mod display name) or is absent.
    pub xpath: Option<String>,
    /// Why this operation failed, e.g. `Failed to find a node with the
    /// given xpath` or a bare `Error in ...` branch line.
    pub reason: String,
    /// The branch marker (per the stack-block shape's branch markers)
    /// parsed out of `reason` (`"match"`/`"nomatch"`), when present.
    pub branch: Option<String>,
}

/// A patch-reporting mod's stack-trace block (its start line names the
/// mod, its end line closes it; see [`PatchStackBlockShape`](super::PatchStackBlockShape)): a rich
/// diagnostic for one patch failure, richer than the engine's terse
/// `Patch operation ... failed` line. `ops` is leaf
/// (innermost) first,
/// top-level (outermost) last — exactly the file's own top-to-bottom
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTraceBlock {
    /// The block header's own bracketed mod tag.
    pub mod_tag: String,
    /// Leaf first, top-level (outermost) last.
    pub ops: Vec<StackTraceOp>,
    /// The source-file path on the block's trailer line, after its end
    /// line.
    pub source_file: Option<String>,
}

/// One `[<Mod Name>] Patch operation <op> failed` line, paired with its
/// own `file:` line and, when the same startup pass holds a block for it
/// (same mod tag, preferring the same file path; the blocks precede their
/// terse lines in a real log), its own [`StackTraceBlock`] — an
/// excerpt may have more terse failures than blocks or vice versa
/// (`ParsedGameLog::extra_stack_traces` carries the overflow the other
/// way), so `stack_trace` is `None` rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPatchFailure {
    /// The terse line's own bracketed mod tag.
    pub mod_tag: String,
    /// The top-level operation's own RimWorld-log identity text, exactly
    /// as logged — the same text
    /// [`rim_resolve::domain::FindingKey::PatchWillFail::operation`]
    /// carries, once [`rim_resolve::domain::normalize_log_text`]
    /// collapses both to one line.
    pub operation: String,
    /// The `file:` line immediately following the terse failure.
    pub source_file: Option<String>,
    /// This failure's own richer stack-trace record, when the parser
    /// found a block that belongs to it.
    pub stack_trace: Option<StackTraceBlock>,
}

/// `Could not resolve cross-reference ...` — two genuinely different
/// renderings, both parsed: one carries the wanting *field*, the
/// other the wanting *def*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawCrossReference {
    /// `Could not resolve cross-reference to <Type> named <Name>
    /// (wanter=<field>)`.
    Wanter {
        /// The type that couldn't be resolved.
        missing_type: String,
        /// The name that couldn't be resolved.
        missing_name: String,
        /// The field on the wanting def/comp that named it.
        wanter_field: String,
    },
    /// `Could not resolve cross-reference: No <Type> named <Name> found
    /// to give to <Type> <name> (...)`.
    WantingDef {
        /// The type that couldn't be resolved.
        missing_type: String,
        /// The name that couldn't be resolved.
        missing_name: String,
        /// The def that wanted it, e.g. `Verse.Tool fist`.
        wanting_def: String,
        /// The trailing parenthetical, e.g. `using undefined sound
        /// instead`, when present.
        note: Option<String>,
    },
}

/// A texture loader's size report for a texture it could not load (see
/// [`TextureFallbackShape`](super::TextureFallbackShape)): the path, the width and height, and the
/// format. The sole source for the startup page's bad-DDS column (no
/// static header reader exists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDdsFailure {
    /// The texture file's own on-disk path, as logged.
    pub path: String,
    /// The texture's width in pixels.
    pub width: u32,
    /// The texture's height in pixels.
    pub height: u32,
    /// The compressed format, e.g. `DXT1`/`BC7`.
    pub format: String,
}

/// `Mod <display name> dependency (<packageId>) needs to have
/// <downloadUrl> and/or <steamWorkshopUrl> specified.`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDependencyWarning {
    /// The dependent mod's own display name, as logged.
    pub mod_name: String,
    /// The missing dependency's own package id.
    pub dependency_id: String,
}

/// One timer line, in any of its six renderings, normalized to a
/// whole-millisecond duration (never a bare `f64` seconds value — this
/// codebase's own determinism contract, root `CLAUDE.md`, treats a float
/// reaching output as a risk not worth taking for a display duration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTimer {
    /// Whatever context text preceded the timer rendering on its own
    /// line, Unity `<color>` tags already stripped — often, but not
    /// always, a `[Tag]`-bracketed mod name.
    pub label: String,
    /// The timed duration, in whole milliseconds (rounded).
    pub milliseconds: u64,
    /// The startup pass the line was logged in: the number of `RimWorld
    /// <version>` banners logged before it (`0` before the first banner).
    /// A real log has two passes, because a pre-patching loader restarts
    /// the game in-process; every pass's timers are kept, since no timer
    /// repeats across passes. This is the real pass, never folded: unlike
    /// [`Family::count_by_pass`], which folds passes above its tracked
    /// limit (16) into the last one.
    pub pass: u32,
}

/// What started a game session's mod-list block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadEventKind {
    /// `Initializing new game with mods:`.
    NewGame,
    /// `Loading game from file <save name> with mods:`.
    SaveLoad {
        /// The save's name exactly as the log printed it.
        save_name: String,
    },
}

/// One mod-list block the game logged when it started or loaded a game,
/// package ids still raw strings (conversion to `ModId` happens in
/// [`crate::use_cases::ImportGameLog`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLoadEvent {
    /// The 1-based line number of the block's header line.
    pub line: usize,
    /// Which header opened the block.
    pub kind: LoadEventKind,
    /// The active mods in load order, package ids only (the
    /// ` (incompatible version)` suffix stripped).
    pub mods: Vec<String>,
}

/// What the streaming reader saw of a log's raw bytes and what it had to
/// bound. Every counter is a loss the parse could not avoid, reported
/// instead of hidden: a nonzero counter means "the log held more than
/// the result shows", never a refusal.
///
/// `lines_read` is not a loss; it is the total the other counters are
/// read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogReadStats {
    /// Every line the reader split off, whatever it held.
    pub lines_read: u64,
    /// Lines longer than the per-line bound: only the head was
    /// classified, the rest was dropped.
    pub lines_truncated: u64,
    /// Lines (within their kept head) holding at least one byte that is
    /// not valid UTF-8, decoded with replacement characters.
    pub lines_with_invalid_utf8: u64,
    /// Lines inside one stack-trace block past the per-block bound: not
    /// examined, only counted.
    pub stack_block_lines_dropped: u64,
    /// Multi-line joins inside a stack-trace block that grew past the
    /// per-join byte or line bound without ever forming an operation
    /// line, and so were abandoned.
    pub stack_joins_abandoned: u64,
    /// Entries recorded under the last tracked pass because the log held
    /// more startup passes than a family tracks (hostile input only: a real
    /// log has two).
    pub passes_folded: u64,
    /// Stack back-reference originals that were not indexed because the
    /// index (or one entry's share of it) was full.
    pub stack_refs_dropped: u64,
    /// Logging gaps past the list's bound: counted, not listed.
    pub logging_gaps_dropped: u64,
    /// Crash-report locations cut to `MAX_REPORT_PATH_BYTES` bytes.
    pub crash_report_paths_truncated: u64,
}

/// One kind of loss a bounded read can report — the closed set
/// [`LogReadStats::losses`] yields. A new bound adds a variant here, a
/// counter on [`LogReadStats`], and one arm in `losses`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadLoss {
    /// A line was cut to its first `MAX_LINE_BYTES` bytes.
    LineTruncated,
    /// A line held invalid UTF-8 and was decoded with replacement
    /// characters.
    LineWithInvalidUtf8,
    /// A line inside an over-long stack-trace block was not examined.
    StackBlockLineDropped,
    /// A multi-line join outgrew its bound and was abandoned.
    StackJoinAbandoned,
    /// An entry from a startup pass beyond the tracked ones was folded into
    /// the last tracked pass.
    PassFolded,
    /// A stack back-reference original was not indexed.
    StackRefDropped,
    /// A logging gap past the list's bound was counted, not listed.
    LoggingGapDropped,
    /// A crash-report location was cut to its bound.
    CrashReportPathTruncated,
}

impl LogReadStats {
    /// Every loss that occurred, with its count, in a fixed order —
    /// the one authoritative list: [`Self::has_losses`] and every
    /// renderer derive from it. `lines_read` is a total, not a loss, and
    /// zero counters are omitted.
    #[must_use]
    pub fn losses(&self) -> Vec<(ReadLoss, u64)> {
        // Exhaustive destructuring: a new field fails to compile here until
        // it is either listed below as a loss or bound to `_` as a total.
        let Self {
            lines_read: _,
            lines_truncated,
            lines_with_invalid_utf8,
            stack_block_lines_dropped,
            stack_joins_abandoned,
            passes_folded,
            stack_refs_dropped,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        } = self;
        [
            (ReadLoss::LineTruncated, *lines_truncated),
            (ReadLoss::LineWithInvalidUtf8, *lines_with_invalid_utf8),
            (ReadLoss::StackBlockLineDropped, *stack_block_lines_dropped),
            (ReadLoss::StackJoinAbandoned, *stack_joins_abandoned),
            (ReadLoss::PassFolded, *passes_folded),
            (ReadLoss::StackRefDropped, *stack_refs_dropped),
            (ReadLoss::LoggingGapDropped, *logging_gaps_dropped),
            (
                ReadLoss::CrashReportPathTruncated,
                *crash_report_paths_truncated,
            ),
        ]
        .into_iter()
        .filter(|&(_, count)| count != 0)
        .collect()
    }

    /// Whether any bound truncated, replaced or dropped something.
    #[must_use]
    pub fn has_losses(&self) -> bool {
        !self.losses().is_empty()
    }
}

/// What a line of the engine's own output says about the session rather
/// than about a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EngineInfoKind {
    /// `RimWorld <version> rev<n>`, logged once per startup pass.
    Banner,
    /// The engine stopped logging after its message limit.
    LoggingStopped,
    /// The engine resumed logging.
    LoggingResumed,
    /// A session marker: command line, Unity's memory footer, a crash
    /// report.
    SessionMarker,
    /// Unity's own start-up and asset-loading chatter.
    UnityRuntime,
}

/// Whether a save-load reference message was logged while saving or while
/// loading (the save-time checks exist only in developer mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SaveLoadPhase {
    /// Logged while saving.
    Save,
    /// Logged while loading.
    Load,
}

/// The class every log entry lands in, exactly one. Declaration order is
/// the order classes are listed in; the classifier's first-match-wins order
/// is a separate contract, documented where the arms are (`rim-io`'s
/// `game_log/classify.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryClass {
    /// A mod-list block header (`Initializing new game ...` / `Loading game from file ...`).
    LoadEvent,
    /// Engine session information; see [`EngineInfoKind`].
    EngineInfo(EngineInfoKind),
    /// `[<mod>] Patch operation <op> failed`.
    PatchFailure,
    /// `Config error in <def>: ...`.
    ConfigError,
    /// Any other patch-loading error the engine reports.
    PatchError,
    /// A patch-reporting mod's stack-trace block (a mod format).
    PatchStackTrace,
    /// `Could not resolve cross-reference ...` and its siblings.
    CrossReference,
    /// `XML error: Could not find parent node named ...`.
    MissingParent,
    /// Any other XML parse or def-loading error.
    XmlError,
    /// `Adding duplicate <Type> name: <defName>`.
    DuplicateDef,
    /// A texture loader mod's fallback line (a mod format).
    TextureFallback,
    /// The engine failed to load a texture, audio clip or shader.
    TextureLoadFailure,
    /// An assembly, type or static-constructor failure.
    TypeLoadError,
    /// A load-ID reference message; see [`SaveLoadPhase`].
    SaveLoadReference(SaveLoadPhase),
    /// A mod's About.xml metadata warning.
    ModMetadataWarning,
    /// `Could not find any RuleDef for symbol ...`.
    BaseGenRuleMissing,
    /// An exception the engine or Unity logged while running.
    RuntimeException,
    /// Unity's own runtime error messages.
    UnityRuntimeError,
    /// A def-cache plugin's own log line.
    DefCacheLine,
    /// A startup timer line.
    Timer,
    /// A `[Tag] ...` line from a mod with no error shape.
    ModMessage,
    /// Everything else: kept, counted, and visible.
    Unclassified,
}

/// How serious the matched format string is, per the engine's own
/// `Log.Message` / `Log.Warning` / `Log.Error` call. A `Player.log` line
/// never shows its severity, so this is the static severity of the arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// `Log.Message`.
    Message,
    /// `Log.Warning`.
    Warning,
    /// `Log.Error`.
    Error,
}

/// The grouping key of a family: a normalized form of what makes two
/// entries "the same message". At most [`Self::MAX_BYTES`] bytes: the only
/// constructor cuts longer text at a character boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FamilyKey(String);

impl FamilyKey {
    /// The longest a key gets, in bytes.
    pub const MAX_BYTES: usize = 256;

    /// A key of `text`, cut to [`Self::MAX_BYTES`] at a character boundary.
    #[must_use]
    pub fn new(mut text: String) -> Self {
        if text.len() > Self::MAX_BYTES {
            let cut = (0..=Self::MAX_BYTES)
                .rev()
                .find(|&index| text.is_char_boundary(index))
                .unwrap_or(0);
            text.truncate(cut);
        }
        Self(text)
    }

    /// The key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An entry's line and entry counts, for an aggregate that has no
/// family of its own (an overflow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    /// Entries counted.
    pub entries: u64,
    /// Lines those entries span.
    pub lines: u64,
}

/// The first occurrence of a family, kept for the reader to see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySample {
    /// The entry's first lines, joined with `\n`.
    pub text: String,
    /// The entry had more text than the sample holds.
    pub is_truncated: bool,
}

/// What a family's first entry says about the mod behind it: the raw text
/// [`crate::use_cases::ImportGameLog`] later resolves against the session's
/// active mods. One variant per kind of evidence, each carrying only its
/// own fields (an entry with no evidence has no input at all, never an empty
/// one).
///
/// **Bounds** (the text comes from an untrusted log): a name is at most
/// [`Self::MAX_NAME_BYTES`] bytes, a path [`Self::MAX_PATH_BYTES`], a type or
/// assembly name [`Self::MAX_TYPE_NAME_BYTES`], so one input holds a single
/// bounded string whatever the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributionInput {
    /// A mod's display name: a `[Tag]`, or the name in a `Mod <name> ...`
    /// warning.
    DisplayName(String),
    /// A display name and the file the entry names (a patch failure, a
    /// stack-trace block): the path decides first, the name is the fallback.
    DisplayNameAndPath {
        /// The mod's display name as logged.
        name: String,
        /// The file path as logged.
        path: String,
    },
    /// A file path alone (a texture fallback).
    Path(String),
    /// The one type the entry blames: its stack's innermost non-engine frame,
    /// or the type a type-load error's head names (an outer type, not a
    /// nested one). Never empty; build it with [`Self::type_name`].
    TypeName(String),
    /// The assembly a type-load error's head names, by assembly name (no
    /// `.dll`). Never empty; build it with [`Self::assembly`].
    Assembly(String),
}

impl AttributionInput {
    /// The longest display name kept, in bytes.
    pub const MAX_NAME_BYTES: usize = 256;
    /// The longest path kept, in bytes.
    pub const MAX_PATH_BYTES: usize = 1024;
    /// The longest type or assembly name kept, in bytes.
    pub const MAX_TYPE_NAME_BYTES: usize = 128;

    /// A display name input, cut to [`Self::MAX_NAME_BYTES`].
    #[must_use]
    pub fn display_name(name: &str) -> Self {
        Self::DisplayName(cut_at_char_boundary(name, Self::MAX_NAME_BYTES))
    }

    /// A name-and-path input; each part is cut to its own bound.
    #[must_use]
    pub fn display_name_and_path(name: &str, path: &str) -> Self {
        Self::DisplayNameAndPath {
            name: cut_at_char_boundary(name, Self::MAX_NAME_BYTES),
            path: cut_at_char_boundary(path, Self::MAX_PATH_BYTES),
        }
    }

    /// A path input, cut to [`Self::MAX_PATH_BYTES`].
    #[must_use]
    pub fn path(path: &str) -> Self {
        Self::Path(cut_at_char_boundary(path, Self::MAX_PATH_BYTES))
    }

    /// A blamed-type input, cut to [`Self::MAX_TYPE_NAME_BYTES`]; `None` for an
    /// empty name.
    #[must_use]
    pub fn type_name(name: &str) -> Option<Self> {
        (!name.is_empty())
            .then(|| Self::TypeName(cut_at_char_boundary(name, Self::MAX_TYPE_NAME_BYTES)))
    }

    /// An assembly-name input, cut to [`Self::MAX_TYPE_NAME_BYTES`]; `None`
    /// for an empty name.
    #[must_use]
    pub fn assembly(name: &str) -> Option<Self> {
        (!name.is_empty())
            .then(|| Self::Assembly(cut_at_char_boundary(name, Self::MAX_TYPE_NAME_BYTES)))
    }
}

/// `text` cut to at most `limit` bytes at a character boundary.
fn cut_at_char_boundary(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let cut = (0..=limit)
        .rev()
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(0);
    text[..cut].to_string()
}

/// The text of a leading `[Tag]`, brackets balanced (`[A [B]] rest` is the
/// tag `A [B]`); `None` for no tag, an empty one, or one that never closes
/// within [`AttributionInput::MAX_NAME_BYTES`]. The one reading of a tag: the
/// parser's family evidence and the use case's timer attribution both use it.
#[must_use]
pub fn leading_bracket_tag(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('[')?;
    let mut depth = 1_usize;
    for (offset, byte) in rest
        .bytes()
        .enumerate()
        .take(AttributionInput::MAX_NAME_BYTES + 1)
    {
        match byte {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return rest.get(..offset).filter(|tag| !tag.is_empty());
        }
    }
    None
}

/// Every entry that shares one [`FamilyKey`] within a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// Entries in the family.
    pub count: u64,
    /// Lines those entries span.
    pub lines: u64,
    /// 1-based line of the first entry's head.
    pub first_line: u64,
    /// 1-based line of the last entry's head.
    pub last_line: u64,
    /// Entries per startup pass (pass 0 is the text before the first
    /// banner; later passes past the tracked limit fold into the last).
    /// So a pass key here can stand for several real passes, while
    /// [`RawTimer::pass`] is always the real one.
    pub count_by_pass: BTreeMap<u32, u64>,
    /// The static severity of the matched arm, when it has one.
    pub severity: Option<Severity>,
    /// The first occurrence, unless the sample budget ran out.
    pub sample: Option<EntrySample>,
    /// What the family's first entry says about the mod behind it, for the
    /// classes whose entries carry such evidence (the other classes, and
    /// entries with none, have `None`). Resolved once per family, not per
    /// entry.
    pub attribution_input: Option<AttributionInput>,
}

/// Every family of one class, plus what the family bounds pushed out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClassTally {
    /// The families, by key.
    pub families: BTreeMap<FamilyKey, Family>,
    /// Entries of families that did not fit under the family bounds.
    pub overflow: Tally,
}

impl ClassTally {
    /// Entries in the class: every family plus the overflow.
    #[must_use]
    pub fn entries(&self) -> u64 {
        self.families
            .values()
            .map(|family| family.count)
            .sum::<u64>()
            + self.overflow.entries
    }

    /// Lines the class's entries span.
    #[must_use]
    pub fn lines(&self) -> u64 {
        self.families
            .values()
            .map(|family| family.lines)
            .sum::<u64>()
            + self.overflow.lines
    }
}

/// One line that matched a loose sentinel of a class but sits in an entry
/// of another class: a suspected classifier leak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentinelLeak {
    /// The sentinel's name.
    pub sentinel: String,
    /// The classes the line was allowed to be in.
    pub expected: Vec<EntryClass>,
    /// The class of the entry the line is actually in.
    pub found: EntryClass,
    /// 1-based line number.
    pub line: u64,
    /// The line, cut to 256 bytes.
    pub text: String,
}

/// The loose-sentinel check of one parse (property P2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SentinelReport {
    /// Every leak found.
    pub total: u64,
    /// The first few leaks.
    pub leaks: Vec<SentinelLeak>,
}

/// The reconciliation of a parse (property P1), derived from the class
/// tallies and the reader's own counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogTotals {
    /// Lines the reader split off.
    pub lines_read: u64,
    /// Blank lines that separate entries and belong to none.
    pub blank_separator_lines: u64,
    /// Entries, all classes.
    pub entries: u64,
    /// Lines the entries span.
    pub entry_lines: u64,
    /// Startup passes (`RimWorld <version>` banners).
    pub passes: u64,
}

impl LogTotals {
    /// Whether every line is accounted for exactly once:
    /// `lines_read == entry_lines + blank_separator_lines`. False is a
    /// parser bug, never a data problem.
    #[must_use]
    pub fn is_conserved(&self) -> bool {
        self.lines_read == self.entry_lines + self.blank_separator_lines
    }
}

/// Everything `rim_io::game_log::parse` (via this port) recovers from
/// a `Player.log`, with every mod reference still a raw string — no
/// attribution: that needs a [`crate::Session`]'s own active-mod list
/// and happens in [`crate::use_cases::ImportGameLog`], not here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedGameLog {
    /// Every terse patch failure, paired with its `file:` line and, when
    /// found, its own stack-trace detail.
    pub patch_failures: Vec<RawPatchFailure>,
    /// Stack-trace blocks left over once every terse failure claimed its
    /// own (by mod tag and file path, within a startup pass) — never
    /// dropped.
    pub extra_stack_traces: Vec<StackTraceBlock>,
    /// Every `Could not resolve cross-reference ...` line.
    pub cross_references: Vec<RawCrossReference>,
    /// Every texture-fallback size report.
    pub dds_failures: Vec<RawDdsFailure>,
    /// Every dependency-without-URL warning, each identity (mod name and
    /// dependency id) taken from the last startup pass that logged it, in
    /// file order (the game logs the same warnings again in every pass,
    /// so summing the passes would double them, and a last pass cut off
    /// partway must not drop what only an earlier pass logged).
    pub dependency_warnings: Vec<RawDependencyWarning>,
    /// Every recognized timer line, from every startup pass, each tagged
    /// with its [`RawTimer::pass`].
    pub timers: Vec<RawTimer>,
    /// Every raw def-cache-plugin log line, verbatim — one per line
    /// whose prefix matches a loaded [`DefCacheCarrier::log_line_prefix`].
    /// The apply-dialog note is a separate concern; this import
    /// only ever surfaces the raw lines so that note can read them.
    pub def_cache_lines: Vec<String>,
    /// Every `Initializing new game with mods:` / `Loading game from
    /// file <save> with mods:` block, in file order. Empty when the log
    /// has no such block at all (a real shape: a log cut off before the
    /// game loaded) — "can't compare orders", distinct from an event
    /// with an empty mod list ("ran with zero mods").
    pub load_events: Vec<RawLoadEvent>,
    /// What the reader had to truncate, replace or drop to read the log
    /// at any size.
    pub read_stats: LogReadStats,
    /// Every entry of the log, grouped by class and family (property P1:
    /// nothing is dropped, what the classifier does not know is
    /// [`EntryClass::Unclassified`]).
    pub classes: BTreeMap<EntryClass, ClassTally>,
    /// Blank lines that separate entries and belong to no entry.
    pub blank_separator_lines: u64,
    /// The loose-sentinel check (property P2).
    pub sentinels: SentinelReport,
    /// What the file covers: its kind and the facts that kind can state.
    pub coverage: LogCoverage,
    /// Every stretch where the engine stopped logging, in file order, at
    /// most [`MAX_LOGGING_GAPS`]; the rest are counted in
    /// [`LogReadStats::logging_gaps_dropped`].
    pub logging_gaps: Vec<LoggingGap>,
}

/// The most logging gaps a parse lists.
pub const MAX_LOGGING_GAPS: usize = 1_000;

impl ParsedGameLog {
    /// The reconciliation of this parse (property P1).
    #[must_use]
    pub fn totals(&self) -> LogTotals {
        totals_of(
            &self.classes,
            self.read_stats.lines_read,
            self.blank_separator_lines,
        )
    }
}

/// Derives [`LogTotals`] from class tallies and the reader's counters; the
/// one place the sums are written. Takes the tallies as pairs so a caller
/// whose tallies sit inside a larger per-class value (the attributed
/// classes) needs no copy.
#[must_use]
pub fn totals_of<'a>(
    classes: impl IntoIterator<Item = (&'a EntryClass, &'a ClassTally)>,
    lines_read: u64,
    blank_separator_lines: u64,
) -> LogTotals {
    let mut totals = LogTotals {
        lines_read,
        blank_separator_lines,
        entries: 0,
        entry_lines: 0,
        passes: 0,
    };
    for (class, tally) in classes {
        totals.entries += tally.entries();
        totals.entry_lines += tally.lines();
        if *class == EntryClass::EngineInfo(EngineInfoKind::Banner) {
            totals.passes += tally.entries();
        }
    }
    totals
}

/// The mod-specific formats a `Player.log` parse recognises, borrowed from
/// the project's `ModKnowledge` for the duration of one read.
#[derive(Debug, Clone, Copy)]
pub struct LogFormats<'a> {
    /// The def-cache plugins whose log-line prefixes are collected into
    /// [`ParsedGameLog::def_cache_lines`].
    pub def_cache_carriers: &'a [DefCacheCarrier],
    /// The mod-produced line formats (stack-trace blocks, texture
    /// fallbacks, back-reference stubs).
    pub shapes: &'a LogShapes,
}

static NO_SHAPES: LogShapes = LogShapes::empty();

impl LogFormats<'static> {
    /// Formats that recognise nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            def_cache_carriers: &[],
            shapes: &NO_SHAPES,
        }
    }
}

/// A `Player.log` read/parse failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct GameLogError(pub String);

/// Reads and parses a game log: a `Player.log` or a console snapshot.
/// Read-only — the log is never modified, moved, or copied (root
/// `CLAUDE.md`); the path is chosen by the user each time, never watched or
/// auto-discovered. Parsing itself is pure (`rim_io::game_log::parse`,
/// unit-tested directly on fixture text, no filesystem); this port's own job
/// is finding the file, deciding its [`crate::ports::LogKind`] by content, and streaming it
/// line by line with bounded memory (no size refusal — per-unit bounds
/// truncate and are counted in [`ParsedGameLog::read_stats`]). A console
/// snapshot is **not** refused: the result's [`ParsedGameLog::coverage`]
/// says what kind of file it was and what it can and cannot tell.
///
/// The result holds one entry per occurrence for each of the classes
/// above, so its size grows with the number of matching lines; the
/// reader's own working memory does not grow with the file.
pub trait GameLogReader {
    /// Reads and parses `path`, as `kind` says: detected by content, or
    /// forced by the caller.
    ///
    /// # Errors
    ///
    /// Returns [`GameLogError`] when `path` can't be read. A file's size,
    /// its kind and bytes that are not valid UTF-8 are never a reason to
    /// refuse.
    ///
    /// `formats` supplies the mod-produced formats the parse recognises — a
    /// parameter, not adapter state, for the same reason
    /// [`DefCacheCarrierProbe`](super::DefCacheCarrierProbe)'s carrier
    /// list is: the data is loaded per project, the adapter is built once
    /// at the composition root. Empty formats collect no def-cache line and
    /// recognise no stack block, texture fallback or back-reference, which
    /// is the honest answer when none is known.
    fn read(
        &self,
        path: &Path,
        formats: &LogFormats<'_>,
        kind: KindChoice,
    ) -> Result<ParsedGameLog, GameLogError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(count: u64, lines: u64) -> Family {
        Family {
            count,
            lines,
            first_line: 1,
            last_line: 1,
            count_by_pass: BTreeMap::new(),
            severity: None,
            sample: None,
            attribution_input: None,
        }
    }

    #[test]
    fn attribution_inputs_are_cut_to_their_bounds_at_character_boundaries() {
        let long_name = "é".repeat(200);
        let long_path = format!("a{}", "é".repeat(700));

        let AttributionInput::DisplayNameAndPath { name, path } =
            AttributionInput::display_name_and_path(&long_name, &long_path)
        else {
            panic!("display_name_and_path builds its own variant");
        };

        assert_eq!(name.len(), AttributionInput::MAX_NAME_BYTES);
        assert_eq!(path.len(), 1023, "1024 falls inside a two-byte character");
    }

    #[test]
    fn a_type_or_assembly_input_is_cut_to_its_bound_and_never_empty() {
        let long = "x".repeat(300);

        let cut = AttributionInput::type_name(&long);
        let assembly = AttributionInput::assembly(&long);

        assert_eq!(
            cut,
            Some(AttributionInput::TypeName(
                "x".repeat(AttributionInput::MAX_TYPE_NAME_BYTES)
            ))
        );
        assert_eq!(
            assembly,
            Some(AttributionInput::Assembly(
                "x".repeat(AttributionInput::MAX_TYPE_NAME_BYTES)
            ))
        );
        assert_eq!(AttributionInput::type_name(""), None);
        assert_eq!(AttributionInput::assembly(""), None);
    }

    #[test]
    fn a_leading_tag_is_read_to_its_balancing_bracket() {
        assert_eq!(
            leading_bracket_tag("[Example Fork [Adopted]] rest"),
            Some("Example Fork [Adopted]")
        );
        assert_eq!(leading_bracket_tag("[Tag] blah [x]"), Some("Tag"));
        assert_eq!(leading_bracket_tag("[Tag] blah [x]]"), Some("Tag"));
    }

    #[test]
    fn text_with_no_usable_leading_tag_has_none() {
        assert_eq!(leading_bracket_tag("no tag"), None);
        assert_eq!(leading_bracket_tag("[] empty"), None);
        assert_eq!(leading_bracket_tag("[never closed"), None);
        assert_eq!(
            leading_bracket_tag(&format!("[{}]", "a".repeat(400))),
            None,
            "over the name bound"
        );
    }

    fn class_with(families: &[(&str, Family)], overflow: Tally) -> ClassTally {
        ClassTally {
            families: families
                .iter()
                .map(|(key, family)| (FamilyKey::new((*key).to_string()), family.clone()))
                .collect(),
            overflow,
        }
    }

    #[test]
    fn a_family_key_is_cut_to_its_bound_at_a_character_boundary() {
        let long = "é".repeat(200);

        let key = FamilyKey::new(long);

        assert!(key.as_str().len() <= FamilyKey::MAX_BYTES);
        assert_eq!(key.as_str().len(), 256);
        let odd = FamilyKey::new(format!("a{}", "é".repeat(200)));
        assert_eq!(odd.as_str().len(), 255, "cut back to the char boundary");
        assert_eq!(FamilyKey::new("short".to_string()).as_str(), "short");
    }

    #[test]
    fn totals_sum_families_and_overflow_and_count_banners_as_passes() {
        let classes = BTreeMap::from([
            (
                EntryClass::EngineInfo(EngineInfoKind::Banner),
                class_with(&[("banner", family(2, 2))], Tally::default()),
            ),
            (
                EntryClass::Unclassified,
                class_with(
                    &[("a", family(3, 5)), ("b", family(1, 1))],
                    Tally {
                        entries: 4,
                        lines: 6,
                    },
                ),
            ),
        ]);

        let totals = totals_of(&classes, 20, 4);

        assert_eq!(totals.entries, 2 + 3 + 1 + 4);
        assert_eq!(totals.entry_lines, 2 + 5 + 1 + 6);
        assert_eq!(totals.passes, 2);
        assert!(!totals.is_conserved(), "14 entry lines + 4 blanks != 20");
    }

    #[test]
    fn totals_are_conserved_when_lines_equal_entry_lines_plus_blanks() {
        let classes = BTreeMap::from([(
            EntryClass::ModMessage,
            class_with(&[("a", family(3, 5))], Tally::default()),
        )]);

        assert!(totals_of(&classes, 7, 2).is_conserved());
        assert!(!totals_of(&classes, 8, 2).is_conserved());
    }

    #[test]
    fn a_read_with_no_loss_reports_nothing() {
        let stats = LogReadStats {
            lines_read: 100,
            ..LogReadStats::default()
        };
        assert!(stats.losses().is_empty());
        assert!(!stats.has_losses());
    }

    #[test]
    fn each_loss_counter_alone_is_reported_with_its_count() {
        let cases = [
            (
                LogReadStats {
                    lines_truncated: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::LineTruncated,
            ),
            (
                LogReadStats {
                    lines_with_invalid_utf8: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::LineWithInvalidUtf8,
            ),
            (
                LogReadStats {
                    stack_block_lines_dropped: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::StackBlockLineDropped,
            ),
            (
                LogReadStats {
                    stack_joins_abandoned: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::StackJoinAbandoned,
            ),
            (
                LogReadStats {
                    passes_folded: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::PassFolded,
            ),
            (
                LogReadStats {
                    stack_refs_dropped: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::StackRefDropped,
            ),
            (
                LogReadStats {
                    logging_gaps_dropped: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::LoggingGapDropped,
            ),
            (
                LogReadStats {
                    crash_report_paths_truncated: 3,
                    ..LogReadStats::default()
                },
                ReadLoss::CrashReportPathTruncated,
            ),
        ];
        for (stats, kind) in cases {
            assert_eq!(stats.losses(), vec![(kind, 3)], "{kind:?}");
            assert!(stats.has_losses(), "{kind:?}");
        }
    }
}
