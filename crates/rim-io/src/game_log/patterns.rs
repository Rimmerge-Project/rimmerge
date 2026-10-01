//! Every pattern the entry pipeline (segmenter, classifier, family keys)
//! and the head patterns the typed-record extractors share with it, each
//! written down once as a `&str` constant.
//!
//! **One source per pattern.** The classifier builds one `RegexSet` from these
//! strings and, where an arm also needs captures, compiles the same constant
//! as an individual `Regex`, so no pattern literal is duplicated between the
//! set and a capturing regex.
//!
//! Every pattern here is a fixed literal, not user input — a typo is a
//! compile-time-discoverable bug, caught by `game_log`'s own tests
//! (`rim_analyzer::extract::xpath_target`'s own `LazyLock<Regex>` +
//! `#[allow(clippy::expect_used)]` convention, kept in one place:
//! [`fixed_regex`]).

use regex::Regex;

/// Compiles one of this module's fixed patterns.
///
/// # Panics
///
/// Panics when `pattern` is not valid regex syntax; every caller passes one
/// of the constants below, so this is a bug caught by the first test that
/// touches it.
pub(super) fn fixed_regex(pattern: &'static str) -> Regex {
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("a game_log pattern constant is a fixed, valid regex")
}

// -- engine heads shared with the record extractors -----------------------------

/// A terse patch failure. The mod group is greedy (`.+`), not `[^\]]+`: a
/// mod's own display name can itself contain a bracketed substring (e.g.
/// "Example Badge Fork [Adopted]"), and `[^\]]+` stops at that *inner* `]`
/// instead of the real, outer one, silently dropping the whole line.
/// "Patch operation" is a fixed literal that never appears inside a mod's
/// own name, so the greedy match backtracks to the correct, last `]` before
/// it.
pub(super) const PATCH_FAILED_PATTERN: &str =
    r"^\[(?P<mod>.+)\]\s+Patch operation\s+(?P<op>.+?)\s+failed\s*$";

/// `Loading game from file <save name> with mods:`. The save name is
/// everything between the fixed prefix and the `with mods:` suffix (a save
/// name can hold spaces).
pub(super) const LOAD_SAVE_HEADER_PATTERN: &str =
    r"^Loading game from file (?P<save>.+) with mods:$";

/// `Initializing new game with mods:`.
pub(super) const LOAD_NEW_GAME_PATTERN: &str = r"^Initializing new game with mods:$";

// -- engine heads (classifier arms) ------------------------------------------

pub(super) const BANNER_PATTERN: &str = r"^RimWorld \d+\.\d+\.\d+ rev\d+$";

/// The first line of every entry's stack trace in a console copy (the
/// engine's own `ExtractStackTrace` frame). Present in every real console
/// snapshot and in no `Player.log`, which is what makes it the positive
/// snapshot signal of kind detection.
pub(super) const COPY_TRACE_START_LINE: &str = "UnityEngine.StackTraceUtility:ExtractStackTrace ()";
pub(super) const LOGGING_STOPPED_PATTERN: &str =
    r"^Reached max messages limit\. Stopping logging to avoid spam\.$";
pub(super) const LOGGING_RESUMED_PATTERN: &str = r"^Message logging is now once again on\.$";

/// A def's config error: a defName has no spaces, which keeps this apart
/// from [`PATCH_ERROR_PATTERN`]'s `Config error in <mod name> patch`.
pub(super) const CONFIG_ERROR_PATTERN: &str =
    r"^(?:Config error in [^\s:]+: |Exception in ConfigErrors\(\) of [^\s:]+: )";

pub(super) const PATCH_ERROR_PATTERN: &str = r"^(?:Error in patch\.(?:Apply|Complete)\(\): |Config error in .+ patch |Exception in ConfigErrors\(\) of .+ patch |Unexpected (?:document )?element in patch XML|Attempted to use PatchOperation directly)";

/// The one cross-reference shape logged as a Warning.
pub(super) const CROSS_REFERENCE_SOUND_PATTERN: &str =
    r"^Could not resolve cross-reference: No .+ \(using undefined sound instead\)$";
pub(super) const CROSS_REFERENCE_PATTERN: &str = r"^(?:Could not resolve cross-reference(?: to |: No )|Trying to resolve null field for def named |Faulty MayRequire|Missing 'key' and/or 'value'|Failed to load key/value pair: |Cannot use value types for object cross reference)";

pub(super) const MISSING_PARENT_PATTERN: &str = r#"^XML error: Could not find parent node named ""#;

/// An XML read failure the engine logs as a Warning.
pub(super) const XML_WARNING_PATTERN: &str = r"^Exception reading .+ as XML: ";
pub(super) const XML_ERROR_PATTERN: &str = r"^(?:XML error: |XML format error: |.+: unknown parse failure$|.+: root element named .+; should be named Defs$|Type .+ is not a Def type or could not be found, in file |Exception loading def from file |Could not find type named |Exception parsing .+ to type |XML .+ defines the same field twice|Exception loading list element |Malformed dictionary XML|CDATA can only be used for strings|Exception while executing PostLoad on |Could not load defs for mod |Mod .+ did not load any content)";

/// `Adding duplicate <Full.Type.Name> name: <defName>`; the captures build
/// the family key.
pub(super) const DUPLICATE_DEF_PATTERN: &str =
    r"^Adding duplicate (?P<def_type>\S+) name: (?P<def_name>\S+)";

pub(super) const TEXTURE_LOAD_FAILURE_PATTERN: &str = r"^(?:Could not load \S+ at '.*' (?:for def '.*' )?in any active mod or in base resources\.|Failed to find any textures at .+ while constructing |Exception loading \S+ from file\.)";

/// The type a type-load error's head names.
pub(super) const TYPE_LOAD_TYPE_PATTERN: &str = r"^(?:Error in static constructor of|Error while instantiating a mod of type) (?P<type>[^\s:]+)";
/// The assembly a type-load error's head names: `<dll>` is the file name minus
/// `.dll`, `<assembly>` the name in a `getting types in assembly` message.
pub(super) const TYPE_LOAD_ASSEMBLY_PATTERN: &str = r"^(?:Exception loading (?P<dll>\S+?)\.dll:|(?:ReflectionTypeLoadException|Exception) getting types in assembly (?P<assembly>[^\s,:]+))";

pub(super) const TYPE_LOAD_ERROR_PATTERN: &str = r"^(?:Exception loading \S+\.dll: |ReflectionTypeLoadException getting types in assembly |Exception getting types in assembly |Error in static constructor of |Error initializing mod: |Error while instantiating a mod of type )";

/// Save-time load-ID checks (developer mode only).
pub(super) const SAVE_REFERENCE_PATTERN: &str = r"^(?:Object with load ID .+ is referenced \(xml node name: |DebugLoadIDsSavingErrorsChecker error: |\S+ was already deepsaved at )";
/// Load-time load-ID and version messages.
pub(super) const LOAD_REFERENCE_PATTERN: &str = r"^(?:Could not resolve reference to (?:object|Thing) with loadID |Loaded file \(.+\) is from version )";

/// The About.xml metadata warnings the engine logs as Warnings.
pub(super) const MOD_METADATA_WARNING_PATTERN: &str = r"^(?:Mod .+ dependency \(.*\) needs to have <downloadUrl>|Mod .+ has a dependency |Mod .+ <packageId> \(.*\) is not in valid format\.|Mod .+ is missing (?:packageId|supported versions list) in About\.xml|Mod .+: targetVersion field is obsolete)";
/// The About.xml metadata problems the engine logs as Errors.
pub(super) const MOD_METADATA_ERROR_PATTERN: &str = r"^(?:Unable to parse version string on mod |Mod .+: <supportedVersions> in mod About\.xml must)";

/// The mod a [`MOD_METADATA_WARNING_PATTERN`] / [`MOD_METADATA_ERROR_PATTERN`]
/// message names: `Mod <name> <what is wrong>`, the name up to the last
/// occurrence of that wording (a name can hold spaces and colons). The one
/// shape with no `Mod` prefix (`Unable to parse version string on mod`) names
/// no mod here.
pub(super) const MOD_METADATA_NAME_PATTERN: &str = r"^Mod (?:(?P<name>.+) (?:dependency \(|has a dependency |<packageId> \(|is missing (?:packageId|supported versions list) in About\.xml)|(?P<colon_name>.+): (?:targetVersion field is obsolete|<supportedVersions> in mod About\.xml must))";

/// `Could not find any RuleDef for symbol "<symbol>" ...`: the head names
/// the symbol; the rest of the (huge) line is ignored.
pub(super) const BASEGEN_RULE_MISSING_PATTERN: &str =
    r#"^Could not find any RuleDef for symbol "(?P<symbol>[^"]*)"#;

pub(super) const RUNTIME_EXCEPTION_PATTERN: &str = r"^(?:Exception ticking |Exception filling window for |Exception in \S+ TryIssueJobPackage: |Root level exception in OnGUI\(\): |Critical error in root Start\(\): |Exception in \S+|(?:[\w`]+\.)*\w*Exception(?:: |$))";

// -- Unity heads ---------------------------------------------------------------

pub(super) const UNITY_RUNTIME_ERROR_PATTERN: &str = r"^(?:null texture passed to GUI\.DrawTexture$|GUI Error: |Event\.Use\(\) should not be called for events of type |Mouse position stack is not empty\.|.+ must be instantiated using the ScriptableObject\.CreateInstance method)";

/// The head of Unity's memory-statistics footer, which Unity writes as it
/// exits normally (RimWorld logs nothing on quit).
pub(super) const MEMORY_FOOTER_HEAD: &str = "Memory Statistics:";
/// The head of Unity's crash report, which precedes its stack trace.
pub(super) const CRASH_REPORT_HEAD: &str = "Crash!!!";
/// The start of Unity's crash-handler message; the next line names where the
/// crash report was written.
pub(super) const CRASH_HANDLER_PREFIX: &str = "A crash has been intercepted by the crash handler";
/// What the crash-handler's location line starts with, after its indent.
pub(super) const CRASH_HANDLER_LOCATION_PREFIX: &str = "* ";

/// Session markers that are plain information.
pub(super) const SESSION_MARKER_PATTERN: &str =
    r"^(?:Command line arguments: |Memory Statistics:$)";
/// Session markers that report a crash.
pub(super) const CRASH_MARKER_PATTERN: &str =
    r"^(?:Crash!!!$|A crash has been intercepted by the crash handler)";

pub(super) const UNITY_RUNTIME_PATTERN: &str = r"^(?:Mono path\[\d+\] = |Mono config path = |Initialize engine version: |GfxDevice: |Direct3D:$|Begin MonoManager ReloadAssembly$|- Loaded All Assemblies, in |- Finished resetting the current domain, in |UnloadTime: |Unloading \d+ unused Assets to reduce memory usage\.|Unloading \d+ Unused Serialized files|Total: [\d.]+ ms \(FindLiveObjects: |Fallback handler could not load library |<RI> |\[PhysX\] |\[Subsystems\] )";

// -- the generic `[Tag]` convention ----------------------------------------------

pub(super) const MOD_MESSAGE_PATTERN: &str = r"^\[[^\]]+\]";

// -- segmentation ----------------------------------------------------------------

/// A Unity stack frame: `Namespace.Type:Method (args)`. The method part is
/// one token of identifier characters (`.ctor`, `<M>b__0`, `M|0_0`) and generic
/// argument lists (`Look<Verse.Pawn, int>`, which may hold spaces and `/`),
/// with no bare `/`, `:` or space, so an ordinary line such as
/// `https://example.org/x (Config error in Foo: bar)` is not a frame. A
/// generic list never holds `=`, which keeps a rich-text tag such as
/// `<color=red>…</color>` from passing for one.
pub(super) const UNITY_FRAME_PATTERN: &str =
    r"^[A-Za-z_<][\w.`+<>/$\[\],|-]*:(?:[\w.`$|\[\]+-]|<[^()=]*>)+\s?\(.*\)\s*$";

// -- family keys -----------------------------------------------------------------

/// Runs of digits, and hex words of 8 or more characters, in a generic
/// family key.
pub(super) const KEY_VOLATILE_PATTERN: &str = r"\b[0-9A-Fa-f]{8,}\b|\d+";

/// A runtime exception's wrapper prefix.
pub(super) const RUNTIME_WRAPPER_PATTERN: &str = r"^(Exception ticking|Exception filling window for \S+|Exception in \S+ TryIssueJobPackage|Root level exception in OnGUI\(\)|Critical error in root Start\(\))";

/// The exception type named anywhere in a runtime exception's head.
pub(super) const EXCEPTION_TYPE_PATTERN: &str = r"((?:[\w`]+\.)*\w*Exception)\b";

/// The dotted name a stack frame line starts with, generic argument lists
/// (`BFS`1[T].Run`) and nested-type markers (`Outer+Inner`) included, so the
/// constructor form `Type..ctor` reads whole.
pub(super) const FRAME_NAME_PATTERN: &str = r"^\s*(?:at\s+)?([\w`<>+.\[\],]+)";

/// The first `.`-separated segment of a type in one of these is the engine's
/// or a runtime library's, not a mod's: compared exactly, so a mod whose
/// namespace merely starts with one (`RimWorldColumns`, `UnityExplorer`) is
/// kept. A superset of the analyzer's own `ENGINE_NAMESPACE_ROOTS` (which only
/// says whose assembly a namespace can never name); a test keeps them in step.
pub(super) const ENGINE_NAMESPACE_ROOTS: &[&str] = &[
    "System",
    "Mono",
    "UnityEngine",
    "Unity",
    "Verse",
    "RimWorld",
    "LudeonTK",
    "HarmonyLib",
    "MonoMod",
];
