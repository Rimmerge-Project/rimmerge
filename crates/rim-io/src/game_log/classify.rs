//! The classifier: an entry's head decides its class, first match wins in the
//! order of [`ARMS`] (the documented contract), and the class decides its
//! family key.
//!
//! One [`RegexSet`] holds every anchored head pattern, built once; a head costs
//! one set match, and the lowest matching pattern index names the winning
//! arm. Four arms are not patterns of that set and are checked at their own
//! position in the order: a def-cache plugin's line prefix (data, from the
//! carriers), a timer line (the same recogniser the per-line timer scan
//! uses), and the two mod-produced heads (a stack-trace block's start and a
//! texture fallback's head, data from the log shapes).

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Regex, RegexSet};
use rim_session::ports::{
    DefCacheCarrier, EngineInfoKind, EntryClass, FamilyKey, SaveLoadPhase, Severity,
};

use super::extract::{is_timer_line, strip_color_tags};
use super::patterns::{
    BANNER_PATTERN, BASEGEN_RULE_MISSING_PATTERN, CONFIG_ERROR_PATTERN, CRASH_MARKER_PATTERN,
    CROSS_REFERENCE_PATTERN, CROSS_REFERENCE_SOUND_PATTERN, DUPLICATE_DEF_PATTERN,
    EXCEPTION_TYPE_PATTERN, KEY_VOLATILE_PATTERN, LOAD_NEW_GAME_PATTERN, LOAD_REFERENCE_PATTERN,
    LOAD_SAVE_HEADER_PATTERN, LOGGING_RESUMED_PATTERN, LOGGING_STOPPED_PATTERN,
    MISSING_PARENT_PATTERN, MOD_MESSAGE_PATTERN, MOD_METADATA_ERROR_PATTERN,
    MOD_METADATA_WARNING_PATTERN, PATCH_ERROR_PATTERN, PATCH_FAILED_PATTERN,
    RUNTIME_EXCEPTION_PATTERN, RUNTIME_WRAPPER_PATTERN, SAVE_REFERENCE_PATTERN,
    SESSION_MARKER_PATTERN, TEXTURE_LOAD_FAILURE_PATTERN, TYPE_LOAD_ERROR_PATTERN,
    UNITY_RUNTIME_ERROR_PATTERN, UNITY_RUNTIME_PATTERN, XML_ERROR_PATTERN, XML_WARNING_PATTERN,
    fixed_regex,
};
use super::shapes::CompiledShapes;

/// What the classifier decided about a head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Classification {
    pub(super) class: EntryClass,
    pub(super) severity: Option<Severity>,
}

/// How an arm recognises a head.
enum ArmKind {
    /// Anchored head patterns, each with the severity of the format string
    /// it matches. The lowest matching pattern of the arm wins.
    Patterns(&'static [(&'static str, Option<Severity>)]),
    /// The head starts with a def-cache carrier's log-line prefix.
    DefCacheLine,
    /// The head holds a startup timer.
    Timer,
    /// The head starts a mod's stack-trace block (a log shape).
    StackBlockStart,
    /// The head is a mod's texture-fallback head (a log shape).
    TextureFallbackHead,
}

struct Arm {
    class: EntryClass,
    kind: ArmKind,
}

const MESSAGE: Option<Severity> = Some(Severity::Message);
const WARNING: Option<Severity> = Some(Severity::Warning);
const ERROR: Option<Severity> = Some(Severity::Error);

const fn patterns(class: EntryClass, list: &'static [(&'static str, Option<Severity>)]) -> Arm {
    Arm {
        class,
        kind: ArmKind::Patterns(list),
    }
}

const fn engine_info(
    kind: EngineInfoKind,
    list: &'static [(&'static str, Option<Severity>)],
) -> Arm {
    patterns(EntryClass::EngineInfo(kind), list)
}

/// Every arm in first-match-wins order. The order is a contract:
///
/// - the def form of `ConfigError` comes before `PatchError`'s
///   `Config error in <mod name> patch` (a defName has no space);
/// - `PatchFailure` and `PatchStackTrace` come before `ModMessage` (both
///   start with `[`), and a stack block's start is data, so with no log
///   shape loaded such a head is a `ModMessage`;
/// - `TextureFallback` comes before `TextureLoadFailure`;
/// - `DefCacheLine` comes before `Timer` (the carrier's own timer line is a
///   def-cache line, and the per-line timer scan still sees it);
/// - `RuntimeException`'s generic `...Exception: ` arm comes after every
///   engine class whose text holds an exception.
const ARMS: &[Arm] = &[
    patterns(
        EntryClass::LoadEvent,
        &[
            (LOAD_NEW_GAME_PATTERN, MESSAGE),
            (LOAD_SAVE_HEADER_PATTERN, MESSAGE),
        ],
    ),
    engine_info(EngineInfoKind::Banner, &[(BANNER_PATTERN, MESSAGE)]),
    engine_info(
        EngineInfoKind::LoggingStopped,
        &[(LOGGING_STOPPED_PATTERN, WARNING)],
    ),
    engine_info(
        EngineInfoKind::LoggingResumed,
        &[(LOGGING_RESUMED_PATTERN, MESSAGE)],
    ),
    patterns(EntryClass::PatchFailure, &[(PATCH_FAILED_PATTERN, ERROR)]),
    patterns(EntryClass::ConfigError, &[(CONFIG_ERROR_PATTERN, ERROR)]),
    patterns(EntryClass::PatchError, &[(PATCH_ERROR_PATTERN, ERROR)]),
    Arm {
        class: EntryClass::PatchStackTrace,
        kind: ArmKind::StackBlockStart,
    },
    patterns(
        EntryClass::CrossReference,
        &[
            (CROSS_REFERENCE_SOUND_PATTERN, WARNING),
            (CROSS_REFERENCE_PATTERN, ERROR),
        ],
    ),
    patterns(
        EntryClass::MissingParent,
        &[(MISSING_PARENT_PATTERN, ERROR)],
    ),
    patterns(
        EntryClass::XmlError,
        &[(XML_WARNING_PATTERN, WARNING), (XML_ERROR_PATTERN, ERROR)],
    ),
    patterns(EntryClass::DuplicateDef, &[(DUPLICATE_DEF_PATTERN, ERROR)]),
    Arm {
        class: EntryClass::TextureFallback,
        kind: ArmKind::TextureFallbackHead,
    },
    patterns(
        EntryClass::TextureLoadFailure,
        &[(TEXTURE_LOAD_FAILURE_PATTERN, ERROR)],
    ),
    patterns(
        EntryClass::TypeLoadError,
        &[(TYPE_LOAD_ERROR_PATTERN, ERROR)],
    ),
    patterns(
        EntryClass::SaveLoadReference(SaveLoadPhase::Save),
        &[(SAVE_REFERENCE_PATTERN, WARNING)],
    ),
    patterns(
        EntryClass::SaveLoadReference(SaveLoadPhase::Load),
        &[(LOAD_REFERENCE_PATTERN, WARNING)],
    ),
    patterns(
        EntryClass::ModMetadataWarning,
        &[
            (MOD_METADATA_WARNING_PATTERN, WARNING),
            (MOD_METADATA_ERROR_PATTERN, ERROR),
        ],
    ),
    patterns(
        EntryClass::BaseGenRuleMissing,
        &[(BASEGEN_RULE_MISSING_PATTERN, WARNING)],
    ),
    patterns(
        EntryClass::RuntimeException,
        &[(RUNTIME_EXCEPTION_PATTERN, ERROR)],
    ),
    patterns(
        EntryClass::UnityRuntimeError,
        &[(UNITY_RUNTIME_ERROR_PATTERN, None)],
    ),
    Arm {
        class: EntryClass::DefCacheLine,
        kind: ArmKind::DefCacheLine,
    },
    Arm {
        class: EntryClass::Timer,
        kind: ArmKind::Timer,
    },
    engine_info(
        EngineInfoKind::SessionMarker,
        &[
            (SESSION_MARKER_PATTERN, MESSAGE),
            (CRASH_MARKER_PATTERN, ERROR),
        ],
    ),
    engine_info(
        EngineInfoKind::UnityRuntime,
        &[(UNITY_RUNTIME_PATTERN, MESSAGE)],
    ),
    patterns(EntryClass::ModMessage, &[(MOD_MESSAGE_PATTERN, None)]),
];

/// One pattern of the set: which arm owns it and its severity.
struct PatternOwner {
    arm_index: usize,
    severity: Option<Severity>,
}

/// An arm that is not a pattern, and where it sits in the order.
struct SpecialArm {
    arm_index: usize,
    kind: SpecialKind,
    severity: Option<Severity>,
}

#[derive(Clone, Copy)]
enum SpecialKind {
    DefCacheLine,
    Timer,
    StackBlockStart,
    TextureFallbackHead,
}

/// What the special arms match a head against.
struct HeadContext<'a> {
    /// The head, trimmed.
    trimmed: &'a str,
    /// The head, trimmed and without color tags: what the patterns see.
    stripped: &'a str,
    carriers: &'a [DefCacheCarrier],
    shapes: &'a CompiledShapes,
}

/// The head classifier, built once.
pub(super) struct ClassifierSet {
    set: RegexSet,
    owners: Vec<PatternOwner>,
    special_arms: Vec<SpecialArm>,
}

static BUILTIN_CLASSIFIER: LazyLock<ClassifierSet> = LazyLock::new(ClassifierSet::new);

impl ClassifierSet {
    /// The classifier over the built-in arms.
    pub(super) fn builtin() -> &'static Self {
        &BUILTIN_CLASSIFIER
    }

    fn new() -> Self {
        let mut texts: Vec<&'static str> = Vec::new();
        let mut owners = Vec::new();
        let mut special_arms = Vec::new();
        for (arm_index, arm) in ARMS.iter().enumerate() {
            match &arm.kind {
                ArmKind::Patterns(list) => {
                    for &(text, severity) in *list {
                        texts.push(text);
                        owners.push(PatternOwner {
                            arm_index,
                            severity,
                        });
                    }
                }
                ArmKind::DefCacheLine => special_arms.push(SpecialArm {
                    arm_index,
                    kind: SpecialKind::DefCacheLine,
                    severity: None,
                }),
                ArmKind::Timer => special_arms.push(SpecialArm {
                    arm_index,
                    kind: SpecialKind::Timer,
                    severity: None,
                }),
                ArmKind::StackBlockStart => special_arms.push(SpecialArm {
                    arm_index,
                    kind: SpecialKind::StackBlockStart,
                    severity: ERROR,
                }),
                ArmKind::TextureFallbackHead => special_arms.push(SpecialArm {
                    arm_index,
                    kind: SpecialKind::TextureFallbackHead,
                    severity: None,
                }),
            }
        }
        #[allow(clippy::expect_used)]
        let set = RegexSet::new(&texts).expect("the classifier arms are fixed, valid patterns");
        Self {
            set,
            owners,
            special_arms,
        }
    }

    /// The class of an entry headed by `head`.
    pub(super) fn classify(
        &self,
        head: &str,
        carriers: &[DefCacheCarrier],
        shapes: &CompiledShapes,
    ) -> Classification {
        let trimmed = head.trim();
        let stripped = strip_color(trimmed);
        let winner = self.set.matches(&stripped).iter().next();
        let cutoff = winner.map_or(usize::MAX, |pattern| self.owners[pattern].arm_index);
        let context = HeadContext {
            trimmed,
            stripped: &stripped,
            carriers,
            shapes,
        };
        for special in self
            .special_arms
            .iter()
            .take_while(|s| s.arm_index < cutoff)
        {
            if Self::special_matches(special.kind, &context) {
                return Classification {
                    class: ARMS[special.arm_index].class,
                    severity: special.severity,
                };
            }
        }
        match winner {
            Some(pattern) => Classification {
                class: ARMS[self.owners[pattern].arm_index].class,
                severity: self.owners[pattern].severity,
            },
            None => Classification {
                class: EntryClass::Unclassified,
                severity: None,
            },
        }
    }

    fn special_matches(kind: SpecialKind, context: &HeadContext<'_>) -> bool {
        let HeadContext {
            trimmed,
            stripped,
            carriers,
            shapes,
        } = *context;
        match kind {
            SpecialKind::DefCacheLine => carriers
                .iter()
                .any(|carrier| trimmed.starts_with(&carrier.log_line_prefix)),
            SpecialKind::Timer => might_hold_timer(trimmed) && is_timer_line(trimmed),
            SpecialKind::StackBlockStart => shapes.is_stack_start(stripped),
            SpecialKind::TextureFallbackHead => shapes.is_texture_fallback_head(stripped),
        }
    }
}

/// Every timer rendering holds `took` or an `ms` unit, so a head with
/// neither cannot be a timer and skips the timer regexes.
fn might_hold_timer(text: &str) -> bool {
    text.contains("took") || text.contains("ms")
}

/// `text` without Unity `<color>` tags.
pub(super) fn strip_color(text: &str) -> Cow<'_, str> {
    if text.contains("color") {
        Cow::Owned(strip_color_tags(text))
    } else {
        Cow::Borrowed(text)
    }
}

// -- family keys ------------------------------------------------------------

static DUPLICATE_DEF_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(DUPLICATE_DEF_PATTERN));
static BASEGEN_RULE_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(BASEGEN_RULE_MISSING_PATTERN));
static RUNTIME_WRAPPER_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(RUNTIME_WRAPPER_PATTERN));
static EXCEPTION_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(EXCEPTION_TYPE_PATTERN));
static KEY_VOLATILE_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(KEY_VOLATILE_PATTERN));

/// How a class turns a head into a family key.
enum KeyStyle {
    /// The head verbatim: it already names one fact.
    Verbatim,
    /// The def type and name.
    DuplicateDef,
    /// The RuleDef symbol.
    BaseGenSymbol,
    /// The wrapper, the exception type, and the first non-engine frame.
    RuntimeException,
    /// The head with numbers and long hex words normalized.
    Generic,
}

fn key_style(class: EntryClass) -> KeyStyle {
    match class {
        EntryClass::PatchFailure
        | EntryClass::PatchStackTrace
        | EntryClass::CrossReference
        | EntryClass::MissingParent
        | EntryClass::TextureFallback
        | EntryClass::TextureLoadFailure
        | EntryClass::ModMetadataWarning
        | EntryClass::LoadEvent
        | EntryClass::EngineInfo(EngineInfoKind::Banner) => KeyStyle::Verbatim,
        EntryClass::DuplicateDef => KeyStyle::DuplicateDef,
        EntryClass::BaseGenRuleMissing => KeyStyle::BaseGenSymbol,
        EntryClass::RuntimeException => KeyStyle::RuntimeException,
        EntryClass::EngineInfo(
            EngineInfoKind::LoggingStopped
            | EngineInfoKind::LoggingResumed
            | EngineInfoKind::SessionMarker
            | EngineInfoKind::UnityRuntime,
        )
        | EntryClass::ConfigError
        | EntryClass::PatchError
        | EntryClass::XmlError
        | EntryClass::TypeLoadError
        | EntryClass::SaveLoadReference(_)
        | EntryClass::UnityRuntimeError
        | EntryClass::DefCacheLine
        | EntryClass::Timer
        | EntryClass::ModMessage
        | EntryClass::Unclassified => KeyStyle::Generic,
    }
}

/// The family key of an entry of `class` headed by `head`. `frame` is the
/// first non-engine frame type of the entry's stack (resolved through
/// back-references), which only a runtime exception's key uses.
pub(super) fn family_key(class: EntryClass, head: &str, frame: Option<&str>) -> FamilyKey {
    let stripped = strip_color(head.trim());
    let key = match key_style(class) {
        KeyStyle::Verbatim => stripped.into_owned(),
        KeyStyle::DuplicateDef => duplicate_def_key(&stripped),
        KeyStyle::BaseGenSymbol => basegen_symbol_key(&stripped),
        KeyStyle::RuntimeException => runtime_exception_key(&stripped, frame),
        KeyStyle::Generic => generic_key(&stripped),
    };
    FamilyKey::new(key)
}

fn duplicate_def_key(head: &str) -> String {
    match DUPLICATE_DEF_RE.captures(head) {
        Some(captures) => format!("{} {}", &captures["def_type"], &captures["def_name"]),
        None => generic_key(head),
    }
}

fn basegen_symbol_key(head: &str) -> String {
    match BASEGEN_RULE_RE.captures(head) {
        Some(captures) => captures["symbol"].to_string(),
        None => generic_key(head),
    }
}

fn runtime_exception_key(head: &str, frame: Option<&str>) -> String {
    let wrapper_end = RUNTIME_WRAPPER_RE
        .find(head)
        .map_or(0, |wrapper| wrapper.end());
    let wrapper = &head[..wrapper_end];
    let exception = EXCEPTION_TYPE_RE
        .captures_at(head, exception_search_start(head, wrapper_end))
        .and_then(|captures| captures.get(1))
        .map_or("", |found| found.as_str());
    format!("{wrapper}|{exception}|{}", frame.unwrap_or(""))
}

/// Where the exception type is looked for: after the wrapper's own words, so
/// the wrapper's `Exception` is never read as the type. A head with no
/// wrapper pattern match that still starts with the word `Exception` (`Exception
/// in <name>: ...`) skips that word too.
fn exception_search_start(head: &str, wrapper_end: usize) -> usize {
    if wrapper_end > 0 {
        return wrapper_end;
    }
    head.strip_prefix("Exception ")
        .map_or(0, |_| "Exception ".len())
}

fn generic_key(head: &str) -> String {
    KEY_VOLATILE_RE.replace_all(head, "#").into_owned()
}
