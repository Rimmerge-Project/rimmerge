//! The typed-record grammar of a `Player.log`: each function here turns one
//! line (or one stack-trace block's lines) into the port's raw record type,
//! with no IO and no state beyond the block being read.
//! [`super::records::RecordCollector`] and [`super::LogParser`]'s timer tap
//! decide *which* lines reach these functions; this module only decides
//! what a line means once it does.

use std::sync::LazyLock;

use regex::Regex;
use rim_resolve::domain::normalize_log_text;
use rim_session::ports::{
    LoadEventKind, LogReadStats, RawCrossReference, RawDdsFailure, RawDependencyWarning, RawTimer,
    StackTraceBlock, StackTraceOp,
};

use super::MAX_LINE_BYTES;
use super::patterns::{LOAD_SAVE_HEADER_PATTERN, PATCH_FAILED_PATTERN, fixed_regex};
use super::shapes::CompiledShapes;

// -- regexes -------------------------------------------------------------
//
// The fixed-literal note for these patterns is in `patterns.rs`.

static PATCH_FAILED_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(PATCH_FAILED_PATTERN));

static FILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^file:\s*(?P<path>.+)$")
            .expect("FILE_RE is a fixed, compile-time-checked pattern")
    }
});

/// One ` - <id>` item of a mod-list block (the segmenter's block context
/// and the load-event extractor share it).
pub(super) static LOAD_ORDER_ITEM_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^-\s*(?P<id>\S+)(?:\s*\(incompatible version\))?\s*$")
            .expect("LOAD_ORDER_ITEM_RE is a fixed, compile-time-checked pattern")
    }
});

/// One stack-trace line: `<Class>(<detail>): <reason>` or, when there is
/// no parenthesized detail at all (`Verse.PatchOperationSequence: Error
/// in the operation at position=1`), `<Class>: <reason>`. `.*\)` is
/// greedy, so it finds the *last* `)` on the line before requiring the
/// following `:` — safe here because a real op line never contains a
/// second, unrelated `)` after its own detail closes.
static STACK_OP_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^(?P<class>[\w.]+)(?:\((?P<detail>.*)\))?:\s*(?P<reason>.+)$")
            .expect("STACK_OP_LINE_RE is a fixed, compile-time-checked pattern")
    }
});

static CROSS_REF_WANTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^Could not resolve cross-reference to (?P<type>\S+) named (?P<name>\S+) \(wanter=(?P<wanter>[^)]*)\)\s*$")
        .expect("CROSS_REF_WANTER_RE is a fixed, compile-time-checked pattern")
    }
});

static CROSS_REF_WANTING_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^Could not resolve cross-reference: No (?P<type>\S+) named (?P<name>\S+) found to give to (?P<wanting>.+?)(?:\s*\((?P<note>[^)]*)\))?\s*$")
        .expect("CROSS_REF_WANTING_RE is a fixed, compile-time-checked pattern")
    }
});

static DEPENDENCY_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"^Mod (?P<mod>.+) dependency \((?P<dep>[^)]*)\) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified\.\s*$")
        .expect("DEPENDENCY_RE is a fixed, compile-time-checked pattern")
    }
});

static LOAD_SAVE_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(LOAD_SAVE_HEADER_PATTERN));

static TIMER_VANILLA_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"\(vanilla load took\s+(?P<s>[0-9]+(?:\.[0-9]+)?)s\)")
            .expect("TIMER_VANILLA_RE is a fixed, compile-time-checked pattern")
    }
});

static TIMER_TOOK_S_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"\(took\s+(?P<s>[0-9]+(?:\.[0-9]+)?)s(?:;[^)]*)?\)")
            .expect("TIMER_TOOK_S_RE is a fixed, compile-time-checked pattern")
    }
});

/// `in Nms`/`in N ms.` **and** a bare `took Nms`/`took N ms` with no
/// leading `in` at all: real, adjacent lines like `EarlyLoader: Game
/// processing took 2252.5076ms` and `Startup init took 61ms (...)` are
/// genuine, attributable startup cost, and a narrower `\bin\b`-only
/// pattern would silently drop them. `game_log`'s own tests cover these
/// shapes directly.
static TIMER_MS_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"(?:\bin|\btook)\s+(?P<ms>[0-9]+(?:\.[0-9]+)?)\s*ms\b")
            .expect("TIMER_MS_RE is a fixed, compile-time-checked pattern")
    }
});

static TIMER_SECONDS_WORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"\btook\s+(?P<s>[0-9]+(?:\.[0-9]+)?)\s+seconds\b")
            .expect("TIMER_SECONDS_WORD_RE is a fixed, compile-time-checked pattern")
    }
});

static COLOR_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(r"</?color(?:=[^>]*)?>")
            .expect("COLOR_TAG_RE is a fixed, compile-time-checked pattern")
    }
});

/// The most lines of one stack-trace block that are examined. A real block
/// is a handful of lines; a block whose end line never comes
/// must not grow without bound, so lines past this are counted
/// (`stack_block_lines_dropped`) and skipped while the block keeps waiting
/// for its end.
pub(super) const MAX_BLOCK_LINES: usize = 5_000;

/// The most lines one multi-line join may accumulate before it is abandoned
/// (`stack_joins_abandoned`), alongside the [`MAX_LINE_BYTES`] byte bound.
/// A wrapped operation line spans two or three physical lines; the join is
/// retried on every new line, so an unbounded one is quadratic work.
pub(super) const MAX_JOIN_LINES: usize = 100;

/// One terse `[<Mod Name>] Patch operation <op> failed` line, still
/// unpaired with a stack-trace block — [`super::pairing::pair_failures`]'s
/// own input, never exposed outside `game_log`.
pub(super) struct TerseFailure {
    pub(super) mod_tag: String,
    pub(super) operation: String,
    pub(super) source_file: Option<String>,
}

/// A stack-trace block still reading its lines.
pub(super) struct OpenBlock {
    mod_tag: String,
    ops: Vec<StackTraceOp>,
    /// Lines not yet forming an operation on their own: a multi-line
    /// quoted-xpath rendering occurs inside real blocks (a
    /// `Verse.PatchOperationRemove` detail split across two physical
    /// lines). Neither physical line matches `STACK_OP_LINE_RE` alone, so
    /// lines accumulate and the *joined*, whitespace-collapsed text
    /// (`normalize_log_text`) is retried on every new line, clearing only
    /// once a join succeeds.
    pending: Vec<String>,
    pending_bytes: usize,
    lines_examined: usize,
}

impl OpenBlock {
    /// The mod tag on the block's start line.
    pub(super) fn mod_tag(&self) -> &str {
        &self.mod_tag
    }

    pub(super) fn new(mod_tag: String) -> Self {
        Self {
            mod_tag,
            ops: Vec::new(),
            pending: Vec::new(),
            pending_bytes: 0,
            lines_examined: 0,
        }
    }

    /// Takes one line of the block, updating `stats` for what it bounds.
    /// `shapes` says how an operation line renders an xpath and a branch.
    pub(super) fn absorb(&mut self, line: &str, shapes: &CompiledShapes, stats: &mut LogReadStats) {
        if self.lines_examined >= MAX_BLOCK_LINES {
            stats.stack_block_lines_dropped += 1;
            return;
        }
        self.lines_examined += 1;
        self.push_pending(line);
        if self.resolve_pending(shapes) {
            return;
        }
        if self.pending_bytes > MAX_LINE_BYTES || self.pending.len() >= MAX_JOIN_LINES {
            // A join this long is not a wrapped operation line, and
            // re-joining it on every further line would cost quadratic time.
            stats.stack_joins_abandoned += 1;
            self.clear_pending();
            // The line that tipped it over may itself open a wrapped
            // operation line, so it starts the next join (unless it alone
            // exceeds the byte bound).
            if line.len() < MAX_LINE_BYTES {
                self.push_pending(line);
                self.resolve_pending(shapes);
            }
        }
    }

    fn push_pending(&mut self, line: &str) {
        self.pending.push(line.to_string());
        self.pending_bytes += line.len() + 1;
    }

    /// Retries the joined pending text as one operation line; on success
    /// records the op and clears the join.
    fn resolve_pending(&mut self, shapes: &CompiledShapes) -> bool {
        let joined = normalize_log_text(&self.pending.join(" "));
        let Some(op) = parse_stack_op_line(&joined, shapes) else {
            return false;
        };
        self.ops.push(op);
        self.clear_pending();
        true
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_bytes = 0;
    }

    /// The block as the parser reports it once its entry closes, with the
    /// trailer path that followed the block's end line when there
    /// was one. A `pending` that never resolved is
    /// dropped (not seen in real logs beyond the case `absorb` handles).
    pub(super) fn into_block(self, source_file: Option<String>) -> StackTraceBlock {
        StackTraceBlock {
            mod_tag: self.mod_tag,
            ops: self.ops,
            source_file,
        }
    }
}

/// A terse `[<Mod>] Patch operation <op> failed` head as a
/// [`TerseFailure`] with no `file:` line yet.
pub(super) fn parse_terse_failure(line: &str) -> Option<TerseFailure> {
    let caps = PATCH_FAILED_RE.captures(line)?;
    Some(TerseFailure {
        mod_tag: caps["mod"].to_string(),
        operation: caps["op"].to_string(),
        source_file: None,
    })
}

/// The path of a `file: <path>` line.
pub(super) fn parse_file_line(line: &str) -> Option<String> {
    let caps = FILE_RE.captures(line)?;
    Some(caps["path"].trim().to_string())
}

/// The package id of a ` - <id>` mod-list item.
pub(super) fn parse_load_item(line: &str) -> Option<String> {
    let caps = LOAD_ORDER_ITEM_RE.captures(line)?;
    Some(caps["id"].to_string())
}

/// One line as a cross-reference error, in either shape.
pub(super) fn parse_cross_reference(line: &str) -> Option<RawCrossReference> {
    if let Some(caps) = CROSS_REF_WANTER_RE.captures(line) {
        return Some(RawCrossReference::Wanter {
            missing_type: caps["type"].to_string(),
            missing_name: caps["name"].to_string(),
            wanter_field: caps["wanter"].to_string(),
        });
    }
    let caps = CROSS_REF_WANTING_RE.captures(line)?;
    Some(RawCrossReference::WantingDef {
        missing_type: caps["type"].to_string(),
        missing_name: caps["name"].to_string(),
        wanting_def: caps["wanting"].trim().to_string(),
        note: caps.name("note").map(|m| m.as_str().to_string()),
    })
}

/// A texture fallback's size report, in the form the log shapes give it.
pub(super) fn parse_dds_failure(line: &str, shapes: &CompiledShapes) -> Option<RawDdsFailure> {
    let dimensions = shapes.texture_dimensions(line)?;
    Some(RawDdsFailure {
        path: dimensions.path,
        width: dimensions.width,
        height: dimensions.height,
        format: dimensions.format,
    })
}

pub(super) fn parse_dependency_warning(line: &str) -> Option<RawDependencyWarning> {
    let caps = DEPENDENCY_RE.captures(line)?;
    Some(RawDependencyWarning {
        mod_name: caps["mod"].to_string(),
        dependency_id: caps["dep"].to_string(),
    })
}

/// Parses one stack-trace line into a [`StackTraceOp`], computing
/// `xpath`/`branch` once here rather than leaving every later reader to
/// recompute them.
fn parse_stack_op_line(line: &str, shapes: &CompiledShapes) -> Option<StackTraceOp> {
    let caps = STACK_OP_LINE_RE.captures(line)?;
    let detail = caps.name("detail").map(|m| m.as_str().to_string());
    let reason = caps["reason"].trim().to_string();
    let xpath = detail
        .as_deref()
        .and_then(|detail| shapes.xpath_from_detail(detail));
    let branch = shapes.branch_of(&reason).map(str::to_string);
    Some(StackTraceOp {
        class: caps["class"].to_string(),
        detail,
        xpath,
        reason,
        branch,
    })
}

/// Strips Unity rich-text `<color=...>`/`</color>` tags — every one of
/// the timer renderings this parser recognizes needs this first
/// (a def-cache plugin's own "8 seconds" timer is wrapped in one).
pub(super) fn strip_color_tags(line: &str) -> String {
    COLOR_TAG_RE.replace_all(line, "").into_owned()
}

/// Strips color tags once, then tries each recognized timer rendering
/// (see [`TIMER_MS_RE`]'s own doc comment) in turn and, on the first
/// match, splits the line into `label` (everything before the timer
/// text, trimmed) and a whole-millisecond duration.
pub(super) fn parse_timer_line(line: &str, pass: u32) -> Option<RawTimer> {
    let stripped = strip_color_tags(line);
    let (start, seconds) = find_timer_seconds(&stripped)?;
    Some(RawTimer {
        label: stripped[..start].trim().to_string(),
        milliseconds: (seconds * 1000.0).round() as u64,
        pass,
    })
}

/// Whether `line` holds a timer rendering, without building the record.
pub(super) fn is_timer_line(line: &str) -> bool {
    find_timer_seconds(&strip_color_tags(line)).is_some()
}

/// Returns the byte offset the matched timer text starts at (so the
/// caller can slice everything before it as the label) and the duration
/// in seconds, trying `(vanilla load took Ns)`, `(took Ns[; ...])`, `in
/// Nms`/`in N ms.`, then `took N seconds` in turn.
fn find_timer_seconds(text: &str) -> Option<(usize, f64)> {
    if let Some(m) = TIMER_VANILLA_RE.captures(text) {
        return Some((m.get(0)?.start(), m["s"].parse().ok()?));
    }
    if let Some(m) = TIMER_TOOK_S_RE.captures(text) {
        return Some((m.get(0)?.start(), m["s"].parse().ok()?));
    }
    if let Some(m) = TIMER_MS_RE.captures(text) {
        let ms: f64 = m["ms"].parse().ok()?;
        return Some((m.get(0)?.start(), ms / 1000.0));
    }
    let m = TIMER_SECONDS_WORD_RE.captures(text)?;
    Some((m.get(0)?.start(), m["s"].parse().ok()?))
}

/// Whether a trimmed line opens a mod-list block. The cheap prefix check
/// first: the segmenter asks this of every indented line.
pub(super) fn is_load_header(line: &str) -> bool {
    (line.starts_with("Initializing ") || line.starts_with("Loading game from file "))
        && parse_load_header(line).is_some()
}

/// The [`LoadEventKind`] a trimmed line opens a mod-list block as, or
/// `None` when it is not a block header.
pub(super) fn parse_load_header(line: &str) -> Option<LoadEventKind> {
    if line == "Initializing new game with mods:" {
        return Some(LoadEventKind::NewGame);
    }
    let caps = LOAD_SAVE_HEADER_RE.captures(line)?;
    Some(LoadEventKind::SaveLoad {
        save_name: caps["save"].to_string(),
    })
}
