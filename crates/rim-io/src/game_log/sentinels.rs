//! Loose sentinels (property P2, "no leaks from known classes").
//!
//! Each known class has a deliberately loose pattern (a message fragment,
//! not the strict head pattern). After segmentation, every line that hits a
//! sentinel must sit in an entry of one of that sentinel's classes; a hit in
//! any other entry is a **leak**: the strict classifier missed a line the
//! loose one recognised, which is exactly what a reworded engine message
//! looks like. The table ships and runs on every import, so the check holds
//! on the user's logs, not only on the maintainer's corpus.
//!
//! **Glued heads.** A hit that sits in an entry of its own sentinel's class
//! passes, unless the line is a continuation that is itself a head of that
//! class (a second cross-reference indented under a first): a message the
//! segmenter glued into the previous entry would otherwise vanish into that
//! entry's count silently, so it is a leak too.
//!
//! **Allow-list.** Every exception is a row of [`ALLOW_LIST`] with a stated
//! reason, scoped by the sentinel, the entry's class, and where useful the
//! line's segmentation rule and shape (a class alone is too coarse: a
//! swallowed line sits in the swallowing entry's class). Besides the rows, a
//! stack-frame line (a Mono or Unity frame) is not checked at all: it names a
//! method and its parameters, never a message, and a parameter called `loadID`
//! would otherwise read as a load-ID reference message.
//!
//! The sentinels for the mod-produced formats are derived from the loaded
//! log shapes (the longest fixed segment of each head template), not
//! authored separately, so they exist exactly when a format is known: with
//! no log shape loaded there is no such sentinel, and the format's lines sit
//! in whatever class the engine's own shapes give them without being a
//! leak.

use std::cell::LazyCell;
use std::sync::LazyLock;

use regex::{Regex, RegexSet, RegexSetBuilder};
use rim_session::ports::{EngineInfoKind, EntryClass, SaveLoadPhase, SentinelLeak, SentinelReport};

use super::patterns::{PATCH_FAILED_PATTERN, fixed_regex};
use super::segment::Rule;
use super::shapes::{DFA_SIZE_LIMIT_BYTES, NEST_LIMIT, SIZE_LIMIT_BYTES, ShapeSentinel};

/// The most leaks a report lists in full (the total counts them all).
const MAX_LISTED_LEAKS: usize = 20;
/// The most bytes of a leaking line a report keeps.
const MAX_LEAK_TEXT_BYTES: usize = 256;

struct SentinelDef {
    name: &'static str,
    /// A loose pattern, unanchored.
    pattern: &'static str,
    /// The classes a hit may sit in.
    allowed: &'static [EntryClass],
}

const fn sentinel(
    name: &'static str,
    pattern: &'static str,
    allowed: &'static [EntryClass],
) -> SentinelDef {
    SentinelDef {
        name,
        pattern,
        allowed,
    }
}

/// The engine's sentinels. Exceptions to them are in [`ALLOW_LIST`].
const ENGINE_SENTINELS: &[SentinelDef] = &[
    sentinel(
        "patch-operation-failed",
        r"Patch operation.*failed",
        &[EntryClass::PatchFailure],
    ),
    sentinel(
        "patch-error",
        r"Error in patch\.",
        &[EntryClass::PatchError],
    ),
    sentinel(
        "cross-reference",
        r"cross-reference",
        &[EntryClass::CrossReference],
    ),
    sentinel(
        "config-error",
        r"Config error",
        &[EntryClass::ConfigError, EntryClass::PatchError],
    ),
    sentinel(
        "missing-parent",
        r"Could not find parent node",
        &[EntryClass::MissingParent],
    ),
    sentinel(
        "xml-error",
        r"XML error|XML format error|unknown parse failure",
        &[EntryClass::MissingParent, EntryClass::XmlError],
    ),
    sentinel(
        "duplicate-def",
        r"Adding duplicate",
        &[EntryClass::DuplicateDef],
    ),
    sentinel(
        "texture-load-failure",
        r"Failed to find any textures| in any active mod or in base resources",
        &[EntryClass::TextureLoadFailure],
    ),
    sentinel(
        "type-load-error",
        r"static constructor|getting types in assembly",
        &[EntryClass::TypeLoadError],
    ),
    sentinel(
        "save-load-reference",
        r"load ID|loadID|Could not resolve reference|DebugLoadIDsSavingErrorsChecker",
        &[
            EntryClass::SaveLoadReference(SaveLoadPhase::Save),
            EntryClass::SaveLoadReference(SaveLoadPhase::Load),
        ],
    ),
    sentinel(
        "runtime-exception",
        r"Exception ticking|Exception filling window|TryIssueJobPackage",
        &[EntryClass::RuntimeException],
    ),
    sentinel(
        "mod-metadata-warning",
        r"needs to have <downloadUrl>",
        &[EntryClass::ModMetadataWarning],
    ),
    sentinel(
        "basegen-rule-missing",
        r"RuleDef for symbol",
        &[EntryClass::BaseGenRuleMissing],
    ),
    sentinel("load-event", r"with mods:", &[EntryClass::LoadEvent]),
    sentinel(
        "logging-stopped",
        r"max messages limit",
        &[EntryClass::EngineInfo(EngineInfoKind::LoggingStopped)],
    ),
    sentinel(
        "logging-resumed",
        r"once again on",
        &[EntryClass::EngineInfo(EngineInfoKind::LoggingResumed)],
    ),
    sentinel(
        "unity-runtime-error",
        r"GUI Error|ScriptableObject\.CreateInstance",
        &[EntryClass::UnityRuntimeError],
    ),
];

/// One documented exception to a sentinel.
struct AllowDef {
    /// The sentinel's name.
    sentinel: &'static str,
    /// The class of the entry the line sits in (`None`: any class).
    found: Option<EntryClass>,
    /// Only lines the segmenter gave this rule (`None`: any line).
    rule: Option<Rule>,
    /// Only lines matching this pattern (`None`: any line).
    only_if: Option<&'static str>,
    /// Never lines matching this pattern, tested on the trimmed line (`None`:
    /// no such lines).
    unless: Option<&'static str>,
}

/// Every exception to a sentinel, each with its reason in the comment above it.
const ALLOW_LIST: &[AllowDef] = &[
    // A patch-reporting mod's stack-trace block may quote a failure in its
    // body. A body line that is itself a complete terse-failure head is a real
    // failure the block swallowed (an unclosed block), so it stays a leak.
    AllowDef {
        sentinel: "patch-operation-failed",
        found: Some(EntryClass::PatchStackTrace),
        rule: Some(Rule::StackBlockBody),
        only_if: None,
        unless: Some(PATCH_FAILED_PATTERN),
    },
    // An engine message whose wording is not confirmed against the decompiled
    // game: it stays Unclassified and visible, and is not a classifier leak.
    AllowDef {
        sentinel: "save-load-reference",
        found: Some(EntryClass::Unclassified),
        rule: None,
        only_if: Some(r"^Exception registering .+ in loaded object directory with unique load ID "),
        unless: None,
    },
    // Saved-game XML the engine dumps (`Subnode:`, `... Full node: <li ...>`):
    // the element name `loadID` is data, not a load-ID message, and the dump
    // can sit in an entry of any class. Only the element is excused: a line
    // that also carries load-ID text of its own outside the element is still
    // checked, so a reworded message quoting a node can't hide behind it.
    AllowDef {
        sentinel: "save-load-reference",
        found: None,
        rule: None,
        only_if: Some(r"</?loadID>"),
        unless: Some(
            r"load ID|Could not resolve reference|DebugLoadIDsSavingErrorsChecker|loadID(?:[^>]|$)",
        ),
    },
    // An orphaned stack frame (the excerpt lost the lines above it, so the
    // segmenter sees a head): a frame names a method and its parameters, and a
    // parameter may be called `loadID`. Frames that continue an entry are
    // skipped outright, before the sentinels run.
    AllowDef {
        sentinel: "save-load-reference",
        found: None,
        rule: None,
        only_if: Some(r"^\s+at \S+ \(.*\)"),
        unless: None,
    },
];

/// An [`AllowDef`], compiled.
struct AllowRow {
    found: Option<EntryClass>,
    rule: Option<Rule>,
    only_if: Option<Regex>,
    unless: Option<Regex>,
}

impl AllowRow {
    fn new(def: &AllowDef) -> Self {
        Self {
            found: def.found,
            rule: def.rule,
            only_if: def.only_if.map(fixed_regex),
            unless: def.unless.map(fixed_regex),
        }
    }

    fn permits(&self, line: &PlacedLine<'_>) -> bool {
        self.found.is_none_or(|expected| expected == line.found)
            && self.rule.is_none_or(|expected| line.rule == Some(expected))
            && self
                .only_if
                .as_ref()
                .is_none_or(|re| re.is_match(line.text))
            // Trimmed: a head pattern is anchored at the line start, and an
            // indented copy of a head is still that head.
            && self
                .unless
                .as_ref()
                .is_none_or(|re| !re.is_match(line.text.trim()))
    }
}

/// One sentinel as the set holds it.
struct SentinelRow {
    name: String,
    allowed: Vec<EntryClass>,
    exceptions: Vec<AllowRow>,
}

/// Every sentinel, compiled into one set.
pub(super) struct SentinelSet {
    set: RegexSet,
    rows: Vec<SentinelRow>,
}

static BUILTIN_SENTINELS: LazyLock<SentinelSet> = LazyLock::new(SentinelSet::new);

impl SentinelSet {
    /// The built-in sentinels.
    pub(super) fn builtin() -> &'static Self {
        &BUILTIN_SENTINELS
    }

    fn new() -> Self {
        let mut patterns: Vec<String> = Vec::new();
        let mut rows = Vec::new();
        for def in ENGINE_SENTINELS {
            patterns.push(def.pattern.to_string());
            rows.push(SentinelRow {
                name: def.name.to_string(),
                allowed: def.allowed.to_vec(),
                exceptions: ALLOW_LIST
                    .iter()
                    .filter(|allow| allow.sentinel == def.name)
                    .map(AllowRow::new)
                    .collect(),
            });
        }
        // Every pattern is a fixed literal of this file, so it is valid
        // regex syntax and within the limits by construction.
        #[allow(clippy::expect_used)]
        Self::compile(&patterns, rows).expect("the engine sentinel patterns compile")
    }

    /// The sentinels derived from the loaded log shapes. Each pattern is an
    /// escaped literal built from data, so it is compiled under the same
    /// limits as the templates; a set that does not compile drops the shape
    /// sentinels (as [`super::shapes::CompiledShapes::new`] drops a row that
    /// does not compile) rather than failing the parse.
    pub(super) fn from_shapes(sentinels: Vec<ShapeSentinel>) -> Self {
        let mut patterns = Vec::new();
        let mut rows = Vec::new();
        for sentinel in sentinels {
            patterns.push(regex::escape(&sentinel.literal));
            rows.push(SentinelRow {
                name: sentinel.name.to_string(),
                allowed: vec![sentinel.class],
                exceptions: Vec::new(),
            });
        }
        Self::compile(&patterns, rows).unwrap_or_else(|_| Self {
            set: RegexSet::empty(),
            rows: Vec::new(),
        })
    }

    fn compile(patterns: &[String], rows: Vec<SentinelRow>) -> Result<Self, regex::Error> {
        let set = RegexSetBuilder::new(patterns)
            .size_limit(SIZE_LIMIT_BYTES)
            .dfa_size_limit(DFA_SIZE_LIMIT_BYTES)
            .nest_limit(NEST_LIMIT)
            .build()?;
        Ok(Self { set, rows })
    }

    /// Whether `class` has a sentinel that expects it.
    #[cfg(test)]
    pub(super) fn covers(&self, class: EntryClass) -> bool {
        self.rows.iter().any(|row| row.allowed.contains(&class))
    }
}

/// One line and where it sits.
pub(super) struct PlacedLine<'a> {
    /// 1-based line number.
    pub(super) number: u64,
    pub(super) text: &'a str,
    /// The class of the entry the line sits in.
    pub(super) found: EntryClass,
    /// Why the line joined the entry, or `None` for the entry's head.
    pub(super) rule: Option<Rule>,
}

/// Checks lines against the sentinels and collects the leaks.
pub(super) struct SentinelChecker {
    /// The engine's sentinels, shared by every parse.
    engine: &'static SentinelSet,
    /// The sentinels of this parse's log shapes.
    shapes: SentinelSet,
    report: SentinelReport,
}

impl SentinelChecker {
    pub(super) fn new(engine: &'static SentinelSet, shapes: Vec<ShapeSentinel>) -> Self {
        Self {
            engine,
            shapes: SentinelSet::from_shapes(shapes),
            report: SentinelReport::default(),
        }
    }

    /// Checks one line. `is_head_of_found` says whether the line, taken as
    /// a head, is of the class of the entry it sits in; it runs at most once,
    /// and only for a continuation line that hits a sentinel of that class.
    pub(super) fn check(&mut self, line: &PlacedLine<'_>, is_head_of_found: impl FnOnce() -> bool) {
        let is_own_head = LazyCell::new(is_head_of_found);
        for set in [self.engine, &self.shapes] {
            check_against(set, &mut self.report, line, &is_own_head);
        }
    }

    /// The finished report.
    pub(super) fn finish(self) -> SentinelReport {
        self.report
    }
}

/// Checks one line against one set of sentinels, recording each leak.
fn check_against<IsOwnHead: FnOnce() -> bool>(
    sentinels: &SentinelSet,
    report: &mut SentinelReport,
    line: &PlacedLine<'_>,
    is_own_head: &LazyCell<bool, IsOwnHead>,
) {
    if !sentinels.set.is_match(line.text) {
        return;
    }
    for index in sentinels.set.matches(line.text).iter() {
        let row = &sentinels.rows[index];
        let is_of_class = row.allowed.contains(&line.found);
        let is_glued_head = is_of_class && line.rule.is_some() && **is_own_head;
        let is_expected = !is_glued_head
            && (is_of_class || row.exceptions.iter().any(|allow| allow.permits(line)));
        if !is_expected {
            record_leak(report, row, line);
        }
    }
}

fn record_leak(report: &mut SentinelReport, row: &SentinelRow, line: &PlacedLine<'_>) {
    let PlacedLine {
        number,
        text,
        found,
        ..
    } = *line;
    report.total += 1;
    if report.leaks.len() >= MAX_LISTED_LEAKS {
        return;
    }
    let cut = text
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|&index| index <= MAX_LEAK_TEXT_BYTES)
        .last()
        .filter(|_| text.len() > MAX_LEAK_TEXT_BYTES)
        .unwrap_or(text.len());
    report.leaks.push(SentinelLeak {
        sentinel: row.name.clone(),
        expected: row.allowed.clone(),
        found,
        line: number,
        text: text[..cut].to_string(),
    });
}
