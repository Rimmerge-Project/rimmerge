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

/// A texture the engine could not load: the 1.6 wordings (`Could not load`,
/// `Failed to find any textures`, `Exception loading ... from file.`) and the
/// two Waylith decoder failures (`GpuTextureService`, `[Vulkan]` at ERROR):
/// `Unable to load PSD <path> due to an exception in PsdSharp: ...` and
/// `<path>: the file's contents are not the format its extension names; ...`.
pub(super) const TEXTURE_LOAD_FAILURE_PATTERN: &str = r"^(?:Unable to load PSD .+ due to an exception in PsdSharp: |.+: the file's contents are not the format its extension names; rename it so the decoder meant for it reads it$|Could not load \S+ at '.*' (?:for def '.*' )?in any active mod or in base resources\.|Failed to find any textures at .+ while constructing |Exception loading \S+ from file\.)";

/// The file a Waylith texture-decoder failure names (the capture `psd_path`
/// for the PsdSharp wording, `content_path` for the format-mismatch one); the
/// 1.6 wordings name a resource path, not a file, and give none here. The
/// decoder's other PSD wording (`Unable to load PSD file <name> ...`) holds a
/// bare file name, so its capture reads `file <name>` and resolves to no mod.
pub(super) const TEXTURE_LOAD_PATH_PATTERN: &str = r"^(?:Unable to load PSD (?P<psd_path>.+) due to an exception in PsdSharp: |(?P<content_path>.+): the file's contents are not the format its extension names; rename it so the decoder meant for it reads it$)";

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

/// The wording of the engine's pawn-job exception wrappers, after the pawn's
/// name (which varies and never enters a key): `<pawn> threw exception in
/// WorkGiver <defName>: <ex>` (`JobGiver_Work.TryIssueJobPackage`), `... in
/// GiverTryGiveJobTargeted on WorkGiver <defName>: <ex>`, `... while
/// determining job (main|constant)` (`Pawn_JobTracker`, the exception on a
/// later line), `Pawn <pawn> threw exception while executing a global finish
/// action (<n>), jobDriver=..., job=...: <ex>` (`JobDriver`) and `Pawn <pawn>
/// threw exception while executing toil <toil>'s finish action (<n>), ...:
/// <ex>` (`Toil`), all confirmed in the decompiled 1.7 `Assembly-CSharp`.
macro_rules! pawn_exception_wrapper {
    () => {
        r"threw exception (?:in WorkGiver [^\s:]+|in GiverTryGiveJobTargeted on WorkGiver [^\s:]+|while determining job \((?:main|constant)\)|while executing a global finish action|while executing toil)"
    };
}

pub(super) const RUNTIME_EXCEPTION_PATTERN: &str = concat!(
    r"^(?:Exception ticking |Exception filling window for |Exception in \S+ TryIssueJobPackage: |Root level exception in OnGUI\(\): |Critical error in root Start\(\): |Exception in \S+|Exception getting \S* at |Could not execute post-long-event action\. Exception: |(?:[^\[\s:][^:]*? )?",
    pawn_exception_wrapper!(),
    r"|(?:[\w`]+\.)*\w*Exception(?:: |$))"
);

/// A pawn-job wrapper: the optional pawn name, then the capture `wrapper`,
/// the stable words that name the wrapper and its WorkGiver (the key's wrapper).
pub(super) const RUNTIME_PAWN_WRAPPER_PATTERN: &str = concat!(
    r"^(?:[^\[\s:][^:]*? )?(?P<wrapper>",
    pawn_exception_wrapper!(),
    r")"
);

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
pub(super) const RUNTIME_WRAPPER_PATTERN: &str = r"^(Exception ticking|Exception filling window for \S+|Exception in \S+ TryIssueJobPackage|Root level exception in OnGUI\(\)|Critical error in root Start\(\)|Exception getting \S*|Could not execute post-long-event action\. Exception:)";

/// The exception type named anywhere in a runtime exception's head.
pub(super) const EXCEPTION_TYPE_PATTERN: &str = r"((?:[\w`]+\.)*\w*Exception)\b";

/// The exception type of a pawn-job wrapper: the type that follows a `: ` and
/// is itself followed by `: ` or the end of the head. The job text between the
/// wrapper's words and that `: ` (`jobDriver=<Ns.Type>, job=<defName> ...
/// [workGiverDef: <defName>], toilIndex=<n>`) names defs and drivers that can
/// end in `Exception`; none of them is followed by `: `.
pub(super) const PAWN_EXCEPTION_TYPE_PATTERN: &str = r"(?:: )((?:[\w`]+\.)*\w*Exception)(?:: |$)";

/// The dotted name a stack frame line starts with, generic argument lists
/// (`BFS`1[T].Run`) and nested-type markers (`Outer+Inner`) included, so the
/// constructor form `Type..ctor` reads whole.
pub(super) const FRAME_NAME_PATTERN: &str = r"^\s*(?:at\s+)?([\w`<>+.\[\],]+)";

// -- the Waylith dialect (RimWorld 1.7) -----------------------------------------

/// The header block's first line: `=== <product> log ===`. The product name
/// is whatever the engine's app state holds (the decompiled writer falls back
/// to `Waylith`, and a real 1.7.5014 log says `Waylith`), so no name is fixed.
pub(super) const WAYLITH_LOG_HEADER_PATTERN: &str = r"^=== \S.* log ===$";

/// A Waylith entry's first line: `yyyy-MM-dd HH:mm:ss.fff LEVEL[ [category]]
/// message`. LEVEL is the writer's 5-wide field (`TRACE`, `DEBUG`, ` INFO`,
/// ` WARN`, `ERROR`, ` CRIT`, or five spaces); the category is absent for
/// the writer's own plain lines (`Log closed.`). The message may be empty.
pub(super) const WAYLITH_ENTRY_HEADER_PATTERN: &str = r"^[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3} (?P<level>TRACE|DEBUG| INFO| WARN|ERROR| CRIT|     )(?: \[(?P<category>[^\]]*)\])? (?P<message>.*)$";

/// A Waylith stack frame, the writer's `    at <method> [0x<IL offset>]` with
/// an optional ` in <file>:<line>`. Four spaces, `at`, and the bracketed IL
/// offset together: indentation alone never makes a line a frame (a message
/// body holds indented lines such as `    [0] VkInstance 0x...`, and an
/// exception's own text holds .NET frames of three spaces and no offset).
pub(super) const WAYLITH_FRAME_PATTERN: &str =
    r"^    at \S.*? \[0x[0-9A-Fa-f]+\](?: in .+:[0-9]+)?$";

/// The writer's own message on a clean exit (no category).
pub(super) const WAYLITH_LOG_CLOSED_MESSAGE: &str = "Log closed.";
/// The head of the crash entry (`CRIT [Crash]`), followed by a
/// `Crash report written to: <path>` line when a report exists.
pub(super) const WAYLITH_CRASH_HEAD: &str = "=== CRASH ===";
/// The crash report's location line, after the crash head.
pub(super) const WAYLITH_CRASH_LOCATION_PREFIX: &str = "Crash report written to: ";
/// The writer's warning that the pre-initialization backlog overflowed and
/// earlier lines were lost (no category).
pub(super) const WAYLITH_BACKLOG_OVERFLOW_PATTERN: &str =
    r"^[0-9]+ earlier log line\(s\) were dropped \(pre-init backlog overflow\)\.$";

/// The start of the frame every entry of a 1.7 console copy opens its trace
/// with: the logging call itself, in the `Player.log` writer's frame shape.
pub(super) const WAYLITH_LOG_CALL_FRAME_PREFIX: &str = "    at Verse.Log.";
/// What follows the method name of that frame: the one-string overload and the
/// bracketed IL offset (`Message`, `Warning` and `Error` all take `text`).
pub(super) const WAYLITH_LOG_CALL_ARGUMENTS_PREFIX: &str = "String text) [0x";

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
    "Waylith",
    "Brutal",
    "RimWorldWaylith",
    "Microsoft",
    "UnityStubs",
];
