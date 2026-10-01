//! The entry being read: its head, its line count, a bounded sample, and the
//! few facts that must be gathered while its lines stream past (the first
//! non-engine stack frame, which is both a runtime exception's family-key part
//! and its attribution evidence, and the back-references a patching library
//! prints instead of repeating a trace).
//!
//! An entry keeps at most a head (already bounded by the line reader) and a
//! sample of its first lines; every other line is counted and examined but
//! not stored, so memory never depends on an entry's length.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::{EntryClass, FamilyKey, Severity};

use super::classify::Classification;
use super::patterns::{ENGINE_NAMESPACE_ROOTS, FRAME_NAME_PATTERN, fixed_regex};
use super::segment::{LineView, Rule, is_mono_frame, is_unity_frame};

/// The most lines a sample keeps.
pub(super) const MAX_SAMPLE_LINES: usize = 24;
/// The most bytes a sample keeps.
pub(super) const MAX_SAMPLE_BYTES: usize = 2 * 1024;
/// The most back-reference stubs remembered for one entry's family key.
const MAX_PENDING_STUB_IDS: usize = 8;
/// The most back-reference originals one entry may register.
const MAX_ENTRY_REFS: usize = 64;

static FRAME_NAME_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(FRAME_NAME_PATTERN));

/// The stack facts of one entry, gathered as its lines arrive.
#[derive(Default)]
struct FrameFacts {
    /// The entry's family key (and attribution) needs its first non-engine
    /// frame.
    wants_key_frame: bool,
    /// Back-reference stubs seen before any direct frame, in order.
    pending_stub_ids: Vec<String>,
    /// The first non-engine frame type after the head.
    direct_frame: Option<String>,
    /// Back-reference originals still waiting for the frame that follows.
    awaiting_original_ids: Vec<String>,
    /// Originals that found their frame: `(id, frame type)`.
    resolved_originals: Vec<(String, String)>,
    /// Originals not registered because the per-entry cap was reached.
    originals_dropped: u64,
}

impl FrameFacts {
    /// Forgets the previous entry's facts, keeping the vectors' capacity.
    fn reset(&mut self, class: EntryClass) {
        self.wants_key_frame = class == EntryClass::RuntimeException;
        self.pending_stub_ids.clear();
        self.direct_frame = None;
        self.awaiting_original_ids.clear();
        self.resolved_originals.clear();
        self.originals_dropped = 0;
    }

    fn wants_frame(&self) -> bool {
        !self.awaiting_original_ids.is_empty()
            || (self.wants_key_frame && self.direct_frame.is_none())
    }

    fn note_stub(&mut self, id: &str) {
        let is_first_frame_still_unknown = self.direct_frame.is_none();
        if self.wants_key_frame
            && is_first_frame_still_unknown
            && self.pending_stub_ids.len() < MAX_PENDING_STUB_IDS
        {
            self.pending_stub_ids.push(id.to_string());
        }
    }

    fn note_original(&mut self, id: &str) {
        if self.awaiting_original_ids.len() + self.resolved_originals.len() >= MAX_ENTRY_REFS {
            self.originals_dropped += 1;
            return;
        }
        self.awaiting_original_ids.push(id.to_string());
    }

    fn note_frame(&mut self, frame: String) {
        for id in self.awaiting_original_ids.drain(..) {
            self.resolved_originals.push((id, frame.clone()));
        }
        if self.wants_key_frame && self.direct_frame.is_none() {
            self.direct_frame = Some(frame);
        }
    }
}

/// The type of a stack frame line that is not the engine's or a runtime
/// library's, or `None` when the line is not such a frame: the *outer* type
/// (a nested type reads as its outer one), cut to [`FamilyKey::MAX_BYTES`]
/// (a longer type only ever feeds a key that is cut there anyway, and the cut
/// keeps a stored type small whatever the line).
fn non_engine_frame_type(line: &LineView<'_>) -> Option<String> {
    let type_name: Cow<'_, str> = if is_mono_frame(line.text) {
        mono_frame_type(line.text)?
    } else if is_unity_frame(line.trimmed) {
        Cow::Borrowed(unity_frame_type(line.trimmed)?)
    } else {
        return None;
    };
    if type_name.is_empty() || is_engine_type(&type_name) {
        return None;
    }
    let cut = floor_char_boundary(&type_name, FamilyKey::MAX_BYTES);
    Some(type_name[..cut].to_string())
}

/// `at Ns.Type.Method (args)`: the dotted name minus its last segment (the
/// method). Real shapes that a plain split at the last dot gets wrong:
/// `Ns.Type..ctor` / `Ns.Type..cctor` (the constructor is the last segment),
/// `Ns.Type`1[T].Method` (generic arguments hold no type), and
/// `Ns.Outer+<Inner>d__1.MoveNext` (a nested type is its outer type).
fn mono_frame_type(text: &str) -> Option<Cow<'_, str>> {
    let name = FRAME_NAME_RE.captures(text)?.get(1)?.as_str();
    let name = without_generic_arguments(name);
    let outer = outer_type(&name);
    if outer.len() < name.len() {
        return Some(Cow::Owned(outer.to_string()));
    }
    let type_name = name
        .strip_suffix("..ctor")
        .or_else(|| name.strip_suffix("..cctor"))
        .or_else(|| name.rsplit_once('.').map(|(type_name, _method)| type_name))?;
    Some(Cow::Owned(type_name.to_string()))
}

/// `Ns.Type:Method (args)`: everything before the `:` is the type (a
/// namespace and type share dots, so a split at the last dot would name the
/// namespace), cut to its outer type.
fn unity_frame_type(trimmed: &str) -> Option<&str> {
    trimmed
        .split_once(':')
        .map(|(type_name, _method)| outer_type(type_name))
}

/// `name` without its `[...]` generic argument lists (`BFS`1[T]` becomes
/// `BFS`1`); brackets nest.
fn without_generic_arguments(name: &str) -> Cow<'_, str> {
    if !name.contains('[') {
        return Cow::Borrowed(name);
    }
    let mut depth = 0_usize;
    let kept: String = name
        .chars()
        .filter(|&character| match character {
            '[' => {
                depth += 1;
                false
            }
            ']' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect();
    Cow::Owned(kept)
}

/// `name` cut at its first nested-type marker (`+` in Mono, `/` in Unity's
/// frames): a nested or compiler-generated type (`Outer+<>c`,
/// `Outer+<M>d__1`) belongs to its outer type, which is what names the mod's
/// code. The one cut both frame shapes and a type-load head use, so a family
/// key and its attribution agree.
pub(super) fn outer_type(name: &str) -> &str {
    name.find(['+', '/']).map_or(name, |cut| &name[..cut])
}

/// The type is in an engine or runtime-library namespace: its first
/// `.`-separated segment is exactly one of [`ENGINE_NAMESPACE_ROOTS`].
fn is_engine_type(type_name: &str) -> bool {
    type_name
        .split('.')
        .next()
        .is_some_and(|root| ENGINE_NAMESPACE_ROOTS.contains(&root))
}

/// The entry being read. One instance is reused for every entry of a parse.
pub(super) struct OpenEntry {
    head: String,
    class: EntryClass,
    severity: Option<Severity>,
    head_line: u64,
    pass: u32,
    lines: u64,
    sample: String,
    sample_lines: usize,
    is_sample_truncated: bool,
    frames: FrameFacts,
}

impl OpenEntry {
    pub(super) fn new() -> Self {
        Self {
            head: String::new(),
            class: EntryClass::Unclassified,
            severity: None,
            head_line: 0,
            pass: 0,
            lines: 0,
            sample: String::new(),
            sample_lines: 0,
            is_sample_truncated: false,
            frames: FrameFacts::default(),
        }
    }

    /// Starts a new entry headed by `head` at 1-based `head_line`.
    pub(super) fn begin(
        &mut self,
        head_line: u64,
        head: &str,
        classification: Classification,
        pass: u32,
    ) {
        self.head.clear();
        self.head.push_str(head);
        self.class = classification.class;
        self.severity = classification.severity;
        self.head_line = head_line;
        self.pass = pass;
        self.lines = 1;
        self.sample.clear();
        self.sample_lines = 0;
        self.is_sample_truncated = false;
        self.frames.reset(classification.class);
        self.push_sample(head);
    }

    /// Adds one continuation line.
    pub(super) fn absorb(&mut self, line: &LineView<'_>, rule: Rule) {
        self.lines += 1;
        self.push_sample(line.text);
        match rule {
            Rule::StackRefStub(id) => self.frames.note_stub(id.of(line.trimmed)),
            Rule::StackRefOriginal(id) => self.frames.note_original(id.of(line.trimmed)),
            _ => self.note_frame_line(line),
        }
    }

    fn note_frame_line(&mut self, line: &LineView<'_>) {
        if !self.frames.wants_frame() {
            return;
        }
        if let Some(frame) = non_engine_frame_type(line) {
            self.frames.note_frame(frame);
        }
    }

    fn push_sample(&mut self, line: &str) {
        if self.sample_lines >= MAX_SAMPLE_LINES || self.sample.len() >= MAX_SAMPLE_BYTES {
            self.is_sample_truncated = true;
            return;
        }
        if self.sample_lines > 0 {
            self.sample.push('\n');
        }
        let room = MAX_SAMPLE_BYTES - self.sample.len();
        let cut = floor_char_boundary(line, room);
        self.is_sample_truncated |= cut < line.len();
        self.sample.push_str(&line[..cut]);
        self.sample_lines += 1;
    }

    pub(super) fn class(&self) -> EntryClass {
        self.class
    }

    pub(super) fn severity(&self) -> Option<Severity> {
        self.severity
    }

    pub(super) fn head(&self) -> &str {
        &self.head
    }

    pub(super) fn head_line(&self) -> u64 {
        self.head_line
    }

    pub(super) fn pass(&self) -> u32 {
        self.pass
    }

    pub(super) fn lines(&self) -> u64 {
        self.lines
    }

    pub(super) fn sample(&self) -> &str {
        &self.sample
    }

    pub(super) fn is_sample_truncated(&self) -> bool {
        self.is_sample_truncated
    }

    /// The originals this entry found frames for, taken out for
    /// registration.
    pub(super) fn take_resolved_originals(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.frames.resolved_originals)
    }

    /// How many originals this entry could not register.
    pub(super) fn originals_dropped(&self) -> u64 {
        self.frames.originals_dropped
    }

    /// Back-reference stubs seen before the first direct frame, in order.
    pub(super) fn pending_stub_ids(&self) -> &[String] {
        &self.frames.pending_stub_ids
    }

    /// The first non-engine frame type printed directly in this entry.
    pub(super) fn direct_frame(&self) -> Option<&str> {
        self.frames.direct_frame.as_deref()
    }
}

/// The largest index `<= limit` that is a char boundary of `text`, or
/// `text.len()` when it fits.
fn floor_char_boundary(text: &str, limit: usize) -> usize {
    if limit >= text.len() {
        return text.len();
    }
    (0..=limit)
        .rev()
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(0)
}
