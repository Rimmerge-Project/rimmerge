//! The segmenter: decides, line by line, whether a line opens a new entry,
//! joins the open one, or is a blank separator between entries. It knows
//! nothing about what an entry means; the classifier decides that from the
//! head.
//!
//! **Two framings.** A `Player.log` has no delimiter between messages, so an
//! entry is a head plus every following line that looks like part of it
//! (frames, indented detail, and the block contexts below). A console copy
//! (the engine's "copy all" button) has a fixed shape instead: the message
//! text (which may hold blank lines), then a stack trace or `No stack
//! trace.`, then one blank line before the next entry; that shape is used as
//! it is.
//!
//! **Block contexts.** Some heads announce lines that would otherwise look
//! like new heads (a load block's ` - <id>` items, a stack-trace block's
//! body). The head sets a context, scoped to its own head shape, and the
//! context ends at the first line that is not part of it.
//!
//! **Stated limit: unterminated blocks.** A crash report or stack-trace block
//! whose own end marker never comes takes every following non-blank line, up
//! to the next blank line. A `Crash!!!` head with no
//! `========== END OF STACKTRACE` line is the known case. Nothing is lost --
//! the lines still conserve -- but entries that follow inside the block are
//! not seen as entries. The loose sentinels keep this visible: a known head
//! swallowed this way is reported as a leak, not dropped. A console copy
//! never enters a stack-trace or crash-report block (its own structure
//! delimits entries).

use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::{EngineInfoKind, EntryClass};

use super::extract::{LOAD_ORDER_ITEM_RE, is_load_header};
use super::patterns::{CRASH_REPORT_HEAD, MEMORY_FOOTER_HEAD, UNITY_FRAME_PATTERN, fixed_regex};
use super::shapes::{CompiledShapes, IdSpan, RefLocations, StackRefKind, StackRefMatch};

/// How a file's entries are delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// `Player.log`: continuation lines are recognised by their shape.
    PlayerLog,
    /// An in-game console copy: text, then a stack trace, then a blank line.
    ConsoleCopy,
}

/// One line, trimmed once for every check that needs it.
pub(super) struct LineView<'a> {
    pub(super) text: &'a str,
    pub(super) trimmed: &'a str,
}

impl<'a> LineView<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self {
            text,
            trimmed: text.trim(),
        }
    }
}

/// What a line is, to the entry stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Segment {
    /// The line opens a new entry.
    Head,
    /// The line joins the open entry.
    Continuation(Rule),
    /// A blank line that closes the open entry and belongs to none.
    Separator,
}

/// Why a line joined the open entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rule {
    /// Unity's memory-statistics footer (indented lines, `[ALLOC_` lines,
    /// blank lines).
    MemoryFooter,
    /// A crash report, up to its end marker.
    CrashReport,
    /// The body of a patch-reporting mod's stack-trace block.
    StackBlockBody,
    /// The trailer source-file line after that block.
    StackBlockTrailer,
    /// The line after a texture loader's fallback head.
    TextureFallbackTrailer,
    /// A ` - <id>` item of a mod-list block.
    LoadBlockItem,
    /// A line of the XML node an `XML error:` head dumps.
    XmlNodeDump,
    /// The `file: <path>` line after a patch failure.
    PatchFailureFileLine,
    /// A blank line inside a console-copy message text.
    ConsoleBlankInText,
    /// Console copy: a line of the message text.
    ConsoleText,
    /// Console copy: the first line of the stack trace.
    ConsoleTraceStart,
    /// Console copy: a line of the stack trace.
    ConsoleTrace,
    /// `  at Ns.Type.Method (args)`.
    MonoFrame,
    /// `(wrapper managed-to-native) ...`.
    WrapperFrame,
    /// `--- End of inner exception stack trace ---`.
    InnerExceptionEnd,
    /// `Rethrow as ...`.
    Rethrow,
    /// Unity development builds' `(Filename: ...)` line.
    UnityFilename,
    /// `No stack trace.`
    NoStackTrace,
    /// A patching library's back-reference stub, with where its id sits.
    StackRefStub(IdSpan),
    /// A patching library's back-reference original line, with
    /// where its id sits.
    StackRefOriginal(IdSpan),
    /// Any other indented line.
    Indented,
    /// `Namespace.Type:Method (args)`.
    UnityFrame,
}

impl Rule {
    /// The line is a stack frame: a method and its parameter names, never a
    /// message. Loose sentinels skip these lines (a parameter called `loadID`
    /// is not a load-ID reference message).
    pub(super) fn is_stack_frame(self) -> bool {
        matches!(
            self,
            Self::MonoFrame
                | Self::WrapperFrame
                | Self::UnityFrame
                | Self::ConsoleTraceStart
                | Self::ConsoleTrace
        )
    }
}

/// The block an entry's head opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockContext {
    None,
    MemoryFooter,
    CrashReport,
    StackBlock,
    StackBlockTrailer,
    TextureTrailer,
    LoadBlock,
    XmlNodeDump,
    PatchFileLine,
}

/// The block context a head of `class` opens.
pub(super) fn context_for_head(class: EntryClass, trimmed: &str) -> BlockContext {
    match class {
        EntryClass::LoadEvent => BlockContext::LoadBlock,
        EntryClass::PatchStackTrace => BlockContext::StackBlock,
        EntryClass::TextureFallback => BlockContext::TextureTrailer,
        EntryClass::PatchFailure => BlockContext::PatchFileLine,
        EntryClass::EngineInfo(EngineInfoKind::SessionMarker) => session_marker_context(trimmed),
        _ if trimmed.starts_with("XML error:") => BlockContext::XmlNodeDump,
        _ => BlockContext::None,
    }
}

fn session_marker_context(trimmed: &str) -> BlockContext {
    match trimmed {
        MEMORY_FOOTER_HEAD => BlockContext::MemoryFooter,
        CRASH_REPORT_HEAD => BlockContext::CrashReport,
        _ => BlockContext::None,
    }
}

pub(super) const NO_STACK_TRACE_LINE: &str = "No stack trace.";
const CRASH_REPORT_END_PREFIX: &str = "========== END OF STACKTRACE";
const MEMORY_ALLOCATOR_PREFIX: &str = "[ALLOC_";

static UNITY_FRAME_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(UNITY_FRAME_PATTERN));

/// `Namespace.Type:Method (args)` on a trimmed line. The cheap
/// checks first: the regex only runs on lines that end in `)` and hold a
/// `:`.
pub(super) fn is_unity_frame(trimmed: &str) -> bool {
    trimmed.ends_with(')') && trimmed.contains(':') && UNITY_FRAME_RE.is_match(trimmed)
}

/// A line that starts a console copy's stack trace: a Unity frame or
/// `No stack trace.` (on a trimmed line).
pub(super) fn is_trace_start(trimmed: &str) -> bool {
    trimmed == NO_STACK_TRACE_LINE || is_unity_frame(trimmed)
}

/// `  at Ns.Type.Method (args)`: whitespace, then `at `.
pub(super) fn is_mono_frame(text: &str) -> bool {
    let rest = text.trim_start();
    rest.len() != text.len() && rest.starts_with("at ")
}

fn is_indented(text: &str) -> bool {
    text.starts_with(char::is_whitespace)
}

/// Splits lines into entries. Holds only the state one entry needs.
pub(super) struct Segmenter<'shapes> {
    framing: Framing,
    shapes: &'shapes CompiledShapes,
    ref_locations: RefLocations,
    /// The last back-reference line matched, and what it matched as.
    last_ref: Option<(String, StackRefMatch)>,
    is_entry_open: bool,
    context: BlockContext,
    is_trace_started: bool,
}

impl<'shapes> Segmenter<'shapes> {
    pub(super) fn new(framing: Framing, shapes: &'shapes CompiledShapes) -> Self {
        Self {
            framing,
            shapes,
            ref_locations: shapes.new_ref_locations(),
            last_ref: None,
            is_entry_open: false,
            context: BlockContext::None,
            is_trace_started: false,
        }
    }

    /// Sets the block context the just-returned [`Segment::Head`] opened.
    ///
    /// A console copy has its own structure (text, trace, blank line), and a
    /// stack block or crash report would swallow the trace start and the
    /// blank separator, merging entries; those two contexts are not entered
    /// under that framing.
    pub(super) fn enter_context(&mut self, context: BlockContext) {
        let overrides_console_structure = matches!(
            context,
            BlockContext::StackBlock | BlockContext::CrashReport
        );
        self.context = if self.framing == Framing::ConsoleCopy && overrides_console_structure {
            BlockContext::None
        } else {
            context
        };
    }

    /// Decides what `line` is.
    pub(super) fn push(&mut self, line: &LineView<'_>) -> Segment {
        if line.trimmed.is_empty() {
            return self.blank_line();
        }
        if !self.is_entry_open {
            return self.open_head(line);
        }
        match self.continuation_rule(line) {
            Some(rule) => Segment::Continuation(rule),
            None => self.open_head(line),
        }
    }

    fn open_head(&mut self, line: &LineView<'_>) -> Segment {
        self.is_entry_open = true;
        self.context = BlockContext::None;
        self.is_trace_started =
            self.framing == Framing::ConsoleCopy && is_trace_start(line.trimmed);
        Segment::Head
    }

    fn blank_line(&mut self) -> Segment {
        if self.is_entry_open {
            if self.framing == Framing::ConsoleCopy && !self.is_trace_started {
                return Segment::Continuation(Rule::ConsoleBlankInText);
            }
            if self.context == BlockContext::MemoryFooter {
                return Segment::Continuation(Rule::MemoryFooter);
            }
        }
        self.is_entry_open = false;
        self.context = BlockContext::None;
        Segment::Separator
    }

    fn continuation_rule(&mut self, line: &LineView<'_>) -> Option<Rule> {
        if let Some(rule) = self.context_rule(line) {
            return Some(rule);
        }
        match self.framing {
            Framing::ConsoleCopy => Some(self.console_rule(line.trimmed)),
            Framing::PlayerLog => self.line_shape_rule(line),
        }
    }

    /// The rule a block context gives `line`, ending the context when the
    /// line is not part of it (the line then falls through to the shape
    /// rules).
    fn context_rule(&mut self, line: &LineView<'_>) -> Option<Rule> {
        let LineView { text, trimmed } = *line;
        match self.context {
            BlockContext::None => None,
            BlockContext::MemoryFooter => {
                if is_indented(text) || trimmed.starts_with(MEMORY_ALLOCATOR_PREFIX) {
                    return Some(Rule::MemoryFooter);
                }
                self.context = BlockContext::None;
                None
            }
            BlockContext::CrashReport => {
                if trimmed.starts_with(CRASH_REPORT_END_PREFIX) {
                    self.context = BlockContext::None;
                }
                Some(Rule::CrashReport)
            }
            BlockContext::StackBlock => {
                if self.shapes.is_stack_end(trimmed) {
                    self.context = BlockContext::StackBlockTrailer;
                }
                Some(Rule::StackBlockBody)
            }
            BlockContext::StackBlockTrailer => {
                self.context = BlockContext::None;
                self.shapes
                    .is_stack_trailer(trimmed)
                    .then_some(Rule::StackBlockTrailer)
            }
            BlockContext::TextureTrailer => {
                self.context = BlockContext::None;
                self.shapes
                    .is_texture_fallback_trailer(trimmed)
                    .then_some(Rule::TextureFallbackTrailer)
            }
            BlockContext::LoadBlock => self.load_block_rule(trimmed),
            BlockContext::XmlNodeDump => {
                if trimmed.starts_with('<') {
                    return Some(Rule::XmlNodeDump);
                }
                self.context = BlockContext::None;
                None
            }
            BlockContext::PatchFileLine => {
                self.context = BlockContext::None;
                text.starts_with("file: ")
                    .then_some(Rule::PatchFailureFileLine)
            }
        }
    }

    fn load_block_rule(&mut self, trimmed: &str) -> Option<Rule> {
        if LOAD_ORDER_ITEM_RE.is_match(trimmed) {
            return Some(Rule::LoadBlockItem);
        }
        self.context = BlockContext::None;
        None
    }

    /// Console framing: text lines up to the trace, then trace lines.
    fn console_rule(&mut self, trimmed: &str) -> Rule {
        if self.is_trace_started {
            return Rule::ConsoleTrace;
        }
        if is_trace_start(trimmed) {
            self.is_trace_started = true;
            return Rule::ConsoleTraceStart;
        }
        Rule::ConsoleText
    }

    /// `Player.log` framing: the shape of the line decides, first match
    /// wins.
    fn line_shape_rule(&mut self, line: &LineView<'_>) -> Option<Rule> {
        let LineView { text, trimmed } = *line;
        let rest = text.trim_start();
        if is_mono_frame(text) {
            return Some(Rule::MonoFrame);
        }
        if text.starts_with("(wrapper ") {
            return Some(Rule::WrapperFrame);
        }
        if rest.starts_with("--- End of ") {
            return Some(Rule::InnerExceptionEnd);
        }
        if rest.starts_with("Rethrow as ") {
            return Some(Rule::Rethrow);
        }
        if text.starts_with("(Filename: ") {
            return Some(Rule::UnityFilename);
        }
        self.trailing_shape_rule(text, trimmed)
    }

    /// The back-reference rule of `trimmed`, if it is one. A storm repeats
    /// one line thousands of times in a row, so the last match is remembered
    /// and a repeat costs a string comparison instead of a regex capture.
    fn stack_ref_rule(&mut self, trimmed: &str) -> Option<Rule> {
        let reference = match &self.last_ref {
            Some((line, reference)) if line == trimmed => *reference,
            _ => {
                let found = self
                    .shapes
                    .match_stack_ref(trimmed, &mut self.ref_locations)?;
                self.last_ref = Some((trimmed.to_string(), found));
                found
            }
        };
        Some(match reference.kind {
            StackRefKind::Stub => Rule::StackRefStub(reference.id),
            StackRefKind::Original => Rule::StackRefOriginal(reference.id),
        })
    }

    fn trailing_shape_rule(&mut self, text: &str, trimmed: &str) -> Option<Rule> {
        if trimmed == NO_STACK_TRACE_LINE {
            return Some(Rule::NoStackTrace);
        }
        if let Some(rule) = self.stack_ref_rule(trimmed) {
            return Some(rule);
        }
        // A mod-list header is a head wherever it sits: the load-event
        // records are read off its entry, and a header that a stray indent
        // glued onto the previous entry would be lost.
        if is_indented(text) && !is_load_header(trimmed) {
            return Some(Rule::Indented);
        }
        is_unity_frame(trimmed).then_some(Rule::UnityFrame)
    }
}
