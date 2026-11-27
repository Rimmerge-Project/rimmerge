//! Tests of the 1.7 (Waylith) dialect: the entry header and frame shapes,
//! segmentation into a message and its frames, the explicit level, frame
//! attribution, the end states, and kind detection. Every fixture is
//! synthetic; the shapes are the real ones (a real 1.7.5014 `Player.log`
//! and the decompiled writer; the console copy is a real 1.7.5014 copy's shape).

use rim_session::ports::{
    AttributionInput, ClassTally, EndState, EngineInfoKind, EntryClass, GapEnd, KindChoice,
    LogCoverage, LogKind, ParsedGameLog, PatchPhaseEvidence, Severity,
};
use tempfile::tempdir;

use super::segment::{BlockContext, LineView, Rule, Segment, Segmenter};
use super::shapes::CompiledShapes;
use super::tests::test_formats;
use super::waylith::{
    Level, console_log_severity, is_frame, is_log_header_line, parse_entry_header,
};
use super::*;

fn fixture_text() -> &'static str {
    include_str!("../../tests/fixtures/player_log_waylith_excerpt.log")
}

fn waylith_log(text: &str) -> ParsedGameLog {
    parse_as(text, &test_formats(), Framing::WaylithLog)
}

fn waylith_console(text: &str) -> ParsedGameLog {
    parse_as(text, &test_formats(), Framing::WaylithConsole)
}

fn entries_of(log: &ParsedGameLog, class: EntryClass) -> u64 {
    log.classes.get(&class).map_or(0, ClassTally::entries)
}

fn assert_p1_and_p2(log: &ParsedGameLog) {
    let totals = log.totals();
    assert!(totals.is_conserved(), "P1 broken: {totals:?}");
    assert_eq!(
        log.sentinels.total, 0,
        "sentinel leaks: {:?}",
        log.sentinels.leaks
    );
}

fn player_coverage(log: &ParsedGameLog) -> &rim_session::ports::PlayerLogCoverage {
    match &log.coverage {
        LogCoverage::PlayerLog(coverage) => coverage,
        LogCoverage::ConsoleSnapshot(_) => panic!("expected Player.log coverage"),
    }
}

/// One entry: the header line and any message lines (ending in a newline), then frames.
fn entry(header: &str, rest: &str) -> String {
    format!("2026-01-02 10:00:00.000 {header}{rest}")
}

const FRAME: &str = "    at Verse.Log.Message(String text) [0x19]\n";

// -- the header and frame shapes ----------------------------------------------

#[test]
fn an_entry_header_names_its_level_category_and_message() {
    let cases = [
        ("TRACE [Cat] m", Level::Trace, Some("Cat"), "m"),
        ("DEBUG [Cat] m", Level::Debug, Some("Cat"), "m"),
        (
            " INFO [Verse] RimWorld 1.7.5014 rev1195",
            Level::Information,
            Some("Verse"),
            "RimWorld 1.7.5014 rev1195",
        ),
        (
            " WARN [Vulkan] [General] [Loader Message] x",
            Level::Warning,
            Some("Vulkan"),
            "[General] [Loader Message] x",
        ),
        (
            "ERROR [Verse] [Example Mod] boom",
            Level::Error,
            Some("Verse"),
            "[Example Mod] boom",
        ),
        (
            " CRIT [Crash] === CRASH ===",
            Level::Critical,
            Some("Crash"),
            "=== CRASH ===",
        ),
        (" INFO Log closed.", Level::Information, None, "Log closed."),
        (
            "      [Cat] unnamed level",
            Level::Unnamed,
            Some("Cat"),
            "unnamed level",
        ),
        (" INFO [Verse] ", Level::Information, Some("Verse"), ""),
    ];
    for (rest, level, category, message) in cases {
        let line = format!("2026-01-02 10:00:00.123 {rest}");

        let header = parse_entry_header(&line).unwrap_or_else(|| panic!("{line:?} is a header"));

        assert_eq!(header.level, level, "{line:?}");
        assert_eq!(header.category, category, "{line:?}");
        assert_eq!(header.message, message, "{line:?}");
    }
}

#[test]
fn a_line_that_only_resembles_an_entry_header_is_not_one() {
    let not_headers = [
        "Objects: 1",
        "    [0] VkInstance 0x2918e53d160",
        "2026-01-02 10:00:00.123",
        "2026-01-02 10:00:00.123 INFO [Verse] one-space level",
        "2026-01-02 10:00:00.123  FINE [Verse] unknown level name",
        "2026-01-02 10:00:00 INFO [Verse] no milliseconds",
        "2026/01/02 10:00:00.123  INFO [Verse] slashes",
        "=== Waylith log ===",
        "",
    ];
    for line in not_headers {
        assert!(parse_entry_header(line).is_none(), "{line:?}");
    }
}

#[test]
fn the_level_decides_the_severity_and_an_unnamed_level_decides_nothing() {
    assert_eq!(Level::Trace.severity(), Some(Severity::Message));
    assert_eq!(Level::Debug.severity(), Some(Severity::Message));
    assert_eq!(Level::Information.severity(), Some(Severity::Message));
    assert_eq!(Level::Warning.severity(), Some(Severity::Warning));
    assert_eq!(Level::Error.severity(), Some(Severity::Error));
    assert_eq!(Level::Critical.severity(), Some(Severity::Error));
    assert_eq!(Level::Unnamed.severity(), None);
}

#[test]
fn the_header_block_line_names_any_product() {
    assert!(is_log_header_line("=== Waylith log ==="));
    assert!(is_log_header_line("=== Example Game log ==="));
    assert!(!is_log_header_line("=== CRASH ==="));
    assert!(!is_log_header_line("=== log ==="));
    assert!(!is_log_header_line("Started:   2026-01-02"));
}

#[test]
fn a_frame_needs_four_spaces_at_and_the_il_offset_not_indentation_alone() {
    let frames = [
        "    at Waylith.Graphics.VulkanStartup.SelectWindowsLoader(String bundledPath) [0x334]",
        "    at .InvokeMessageMethod_Verse.Root_Entry.Start(Closure , Object ) [0xFFFFFFFF]",
        "    at Waylith.Windowing.Core.PlatformBase.Iterate() [0x58] in C:\\src\\Loop.cs:194",
        "    at <unknown-method> [0x0]",
    ];
    let not_frames = [
        "    [0] VkInstance 0x2918e53d160",
        "    at Verse.Thing.Tick()",
        "   at Verse.Thing.Tick() in C:\\src\\Thing.cs:line 12",
        "  at Verse.Thing.Tick (System.Int32 n) [0x00000] in <abc>:0",
        "    at ",
        "at Verse.Thing.Tick() [0x1]",
        "        at Verse.Thing.Tick() [0x1]",
    ];
    for line in frames {
        assert!(is_frame(line), "{line:?}");
    }
    for line in not_frames {
        assert!(!is_frame(line), "{line:?}");
    }
}

#[test]
fn only_the_one_string_log_calls_give_a_console_entry_its_level() {
    let with_level = [
        (
            "    at Verse.Log.Message(String text) [0x19]",
            Severity::Message,
        ),
        (
            "    at Verse.Log.Warning(String text) [0x19]",
            Severity::Warning,
        ),
        (
            "    at Verse.Log.Error(String text) [0x1C]",
            Severity::Error,
        ),
        (
            "    at Verse.Log.Error(String text) [0x1C] in C:\\src\\Log.cs:143",
            Severity::Error,
        ),
    ];
    let without_level = [
        "    at Verse.Log.ErrorOnce(String text, Int32 key) [0x1C]",
        "    at Verse.Log.Clear() [0x0]",
        "    at Verse.Logger.Error(String text) [0x1C]",
        "    at Example.Thing.Error(String text) [0x1C]",
        "    at Verse.Log.Error(String text)",
        "   at Verse.Log.Error(String text) in C:\\src\\Log.cs:line 9",
        "  at Verse.Log.Error (System.String text) [0x00000] in <abc>:0",
        "Verse.Log:Error (string)",
    ];
    for (line, expected) in with_level {
        assert_eq!(console_log_severity(line), Some(expected), "{line:?}");
    }
    for line in without_level {
        assert_eq!(console_log_severity(line), None, "{line:?}");
    }
}

// -- segmentation ---------------------------------------------------------------

#[test]
fn an_entry_is_its_header_line_through_the_line_before_the_next_header() {
    let text = [
        entry(" INFO [Verse] first\n", FRAME),
        entry(" WARN [Verse] second\nsecond body line\n", FRAME),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.totals().lines_read, 5);
    assert_p1_and_p2(&log);
}

#[test]
fn a_blank_line_inside_a_message_stays_in_its_entry() {
    let text = [
        entry(" INFO [Verse] first\n\nstill the first\n", FRAME),
        entry(" INFO [Verse] second\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.blank_separator_lines, 0);
    assert_p1_and_p2(&log);
}

#[test]
fn the_header_block_is_one_entry_and_its_blank_line_a_separator() {
    let text = format!(
        "=== Waylith log ===\nStarted:   2026-01-02 10:00:00.000 +00:00\nVersion:   1.7.0.1\n\n{}",
        entry(" INFO [Verse] first\n", "")
    );

    let log = waylith_log(&text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.blank_separator_lines, 1);
    assert_eq!(
        entries_of(&log, EntryClass::EngineInfo(EngineInfoKind::SessionMarker)),
        1
    );
    assert_p1_and_p2(&log);
}

#[test]
fn a_line_before_any_header_opens_its_own_entry() {
    let log = waylith_log("stray line\nstray tail\n");

    assert_eq!(log.totals().entries, 1);
    assert_p1_and_p2(&log);
}

/// Segments `lines` as one Waylith `Player.log` under the test shapes. The
/// first line is a head; `context` is the block it opens.
fn waylith_segments(context: BlockContext, lines: &[&str]) -> Vec<Segment> {
    let formats = test_formats();
    let shapes = CompiledShapes::new(formats.shapes);
    let mut segmenter = Segmenter::new(Framing::WaylithLog, &shapes);
    lines
        .iter()
        .map(|text| {
            let segment = segmenter.push(&LineView::new(text));
            if segment == Segment::Head {
                segmenter.enter_context(context);
            }
            segment
        })
        .collect()
}

const HEAD_LINE: &str = "2026-01-02 10:00:00.000  WARN [Vulkan] layer note";
const ENGINE_FRAME: &str = "    at Waylith.Graphics.GraphicsDevice.Create(Int32 n) [0x0]";

#[test]
fn only_the_frame_shape_is_a_frame_whatever_the_indentation_of_a_body_line() {
    let segments = waylith_segments(
        BlockContext::None,
        &[
            HEAD_LINE,
            "Objects: 1",
            "    [0] ExampleInstance 0x10",
            "        [1] DeeperIndent 0x20",
            ENGINE_FRAME,
        ],
    );

    assert_eq!(
        segments,
        [
            Segment::Head,
            Segment::Continuation(Rule::WaylithBody),
            Segment::Continuation(Rule::Indented),
            Segment::Continuation(Rule::Indented),
            Segment::Continuation(Rule::WaylithFrame),
        ]
    );
}

#[test]
fn an_exception_texts_three_space_frames_are_mono_shaped_never_waylith_frames() {
    let segments = waylith_segments(
        BlockContext::None,
        &[
            HEAD_LINE,
            "   at Example.Body.Thing.Run() in C:\\src\\Thing.cs:line 9",
            "  at Example.Body.Thing.Run () [0x00000] in <abc>:0",
        ],
    );

    assert_eq!(
        segments[1..],
        [
            Segment::Continuation(Rule::MonoFrame),
            Segment::Continuation(Rule::MonoFrame)
        ]
    );
}

#[test]
fn an_indented_body_line_stays_body_in_the_entry_and_its_frame_counts() {
    let text = entry(
        " WARN [Vulkan] layer note\nObjects: 1\n    [0] ExampleInstance 0x10\n",
        "    at Waylith.Graphics.GraphicsDevice.Create(Int32 n) [0x0]\n",
    );

    let log = waylith_log(&text);

    let tally = &log.classes[&EntryClass::EngineInfo(EngineInfoKind::UnityRuntime)];
    let family = tally.families.values().next().expect("one family");
    assert_eq!(family.lines, 4, "head, two body lines and one frame");
    let sample = family.sample.as_ref().expect("a sample").text.clone();
    assert!(sample.contains("[0] ExampleInstance"), "{sample}");
    assert_p1_and_p2(&log);
}

#[test]
fn an_unterminated_block_cannot_swallow_the_frames_or_the_next_entry() {
    // A stack-trace block that never prints its end marker: its body lines
    // are body, but the frame shape wins over the open block, and the next
    // header still starts a new entry.
    let segments = waylith_segments(
        BlockContext::StackBlock,
        &[
            "2026-01-02 10:00:00.000  WARN [Verse] [Example Climate Tuning - Start of stack trace]",
            "Verse.PatchOperationAdd(xpath=\"x\"): Failed",
            "    at Verse.Log.Warning(String text) [0x19]",
            "    at Verse.TickList.Tick() [0x88]",
            "2026-01-02 10:00:01.000  INFO [Verse] after",
        ],
    );

    assert_eq!(
        segments,
        [
            Segment::Head,
            Segment::Continuation(Rule::StackBlockBody),
            Segment::Continuation(Rule::WaylithFrame),
            Segment::Continuation(Rule::WaylithFrame),
            Segment::Head,
        ]
    );
}

#[test]
fn an_unterminated_block_still_yields_two_entries_and_conserves_lines() {
    let text = [
        entry(
            " WARN [Verse] [Example Climate Tuning - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"x\"): Failed\n",
            "    at Verse.Log.Warning(String text) [0x19]\n",
        ),
        entry(" INFO [Verse] after\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.totals().lines_read, 4);
    assert_p1_and_p2(&log);
}

// -- body-level recognizers see the message -------------------------------------

#[test]
fn a_verse_message_is_classified_by_its_text_with_the_level_as_severity() {
    let text = fixture_text();

    let log = waylith_log(text);

    let metadata = &log.classes[&EntryClass::ModMetadataWarning];
    assert_eq!(metadata.entries(), 2);
    assert!(
        metadata
            .families
            .values()
            .all(|family| family.severity == Some(Severity::Warning)),
        "WARN is a warning"
    );
    let failures = &log.classes[&EntryClass::PatchFailure];
    assert!(
        failures
            .families
            .values()
            .all(|family| family.severity == Some(Severity::Error)),
        "ERROR is an error"
    );
}

#[test]
fn the_explicit_level_replaces_the_arms_inferred_severity() {
    // The arm says a patch failure is an error; the log says it was a warning.
    let text = entry(
        " WARN [Verse] [Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\nfile: C:\\m\\a.xml\n",
        "",
    );

    let log = waylith_log(&text);

    let family = log.classes[&EntryClass::PatchFailure]
        .families
        .values()
        .next()
        .expect("one family");
    assert_eq!(family.severity, Some(Severity::Warning));
}

#[test]
fn the_excerpt_yields_every_typed_record_of_a_unity_log_from_its_message_bodies() {
    let log = waylith_log(fixture_text());

    assert_eq!(log.patch_failures.len(), 2);
    assert!(
        log.patch_failures
            .iter()
            .all(|failure| failure.source_file.is_some()),
        "the file: line pairs with its failure"
    );
    let paired = log
        .patch_failures
        .iter()
        .find(|failure| failure.mod_tag == "Example Climate Tuning")
        .expect("the climate failure");
    assert!(
        paired.stack_trace.is_some(),
        "the stack-trace block pairs by mod and file"
    );
    assert!(log.extra_stack_traces.is_empty());
    assert_eq!(log.cross_references.len(), 1);
    assert_eq!(log.dependency_warnings.len(), 2);
    assert_eq!(log.load_events.len(), 2);
    assert_eq!(
        log.load_events[0].mods,
        ["ludeon.rimworld", "example.framework", "example.biomes"]
    );
    assert_p1_and_p2(&log);
}

#[test]
fn a_timer_label_never_carries_the_timestamp_prefix() {
    let log = waylith_log(fixture_text());

    assert_eq!(log.timers.len(), 1);
    assert_eq!(log.timers[0].label, "EarlyLoader: Game processing");
    assert_eq!(log.timers[0].milliseconds, 2253);
}

#[test]
fn an_engine_categorys_duration_is_not_a_startup_timer() {
    let text = [
        entry(" INFO [Vulkan] GpuProbe: completed in 84 ms.\n", ""),
        entry(" INFO [Verse] EarlyLoader: Game processing took 12ms\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.timers.len(), 1);
    assert_eq!(log.timers[0].label, "EarlyLoader: Game processing");
}

#[test]
fn a_def_cache_line_is_found_behind_the_header() {
    let text = entry(
        " INFO [Verse] EXAMPLECACHE: <color=green>Cache not found or got purged!</color>\n",
        "",
    );

    let log = waylith_log(&text);

    assert_eq!(log.def_cache_lines.len(), 1);
    assert!(log.def_cache_lines[0].starts_with("EXAMPLECACHE:"));
}

#[test]
fn the_banner_counts_a_startup_pass_under_its_category() {
    let log = waylith_log(fixture_text());

    assert_eq!(player_coverage(&log).passes, 1);
    assert_eq!(player_coverage(&log).banner_lines.len(), 1);
}

#[test]
fn engine_chatter_is_not_an_unclassified_rimworld_message() {
    let text = [
        entry(
            " INFO [GraphicsExample] Selected device 'Example GPU'\n",
            "",
        ),
        entry(" INFO [Verse] some message nobody classifies\n", ""),
        entry(" INFO [UnityDebug] a mod's own debug line\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(
        entries_of(&log, EntryClass::EngineInfo(EngineInfoKind::UnityRuntime)),
        1
    );
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 2);
}

#[test]
fn an_engine_category_message_that_a_rimworld_arm_knows_keeps_its_class() {
    let text = entry(
        "ERROR [ModLoading] Exception loading Example.Lib.dll: System.Exception: boom\n",
        "",
    );

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::TypeLoadError), 1);
}

#[test]
fn an_engine_categorys_mod_convention_shapes_are_engine_chatter_not_mod_evidence() {
    // The two real 1.7.5014 shapes: a bracketed loader tag in the message and
    // a duration. The category, not the message shape, says who wrote them.
    let text = [
        entry(
            " WARN [Vulkan] [General] [Loader Message] Removing layer VK_LAYER_EXAMPLE because it is a duplicate\nObjects: 1\n    [0] VkInstance 0x2918e53d160\n",
            "    at Waylith.Graphics.GraphicsDevice.DebugUtilsCallback(Int32 n) [0x102]\n",
        ),
        entry(
            " INFO [Vulkan] GpuProbe: completed in 84 ms. GPU probe: 18080 GFLOP/s, 281.5 GB/s -> overall 8.28x (Medium)\n",
            "    at Waylith.Hardware.GpuProbe.Run(Int32 n) [0x335]\n",
        ),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::ModMessage), 0);
    assert_eq!(entries_of(&log, EntryClass::Timer), 0);
    assert_eq!(entries_of(&log, EntryClass::DefCacheLine), 0);
    assert_eq!(
        entries_of(&log, EntryClass::EngineInfo(EngineInfoKind::UnityRuntime)),
        2
    );
    assert!(log.timers.is_empty());
    assert_p1_and_p2(&log);
}

#[test]
fn a_verse_timer_and_mod_message_keep_their_classes() {
    let text = [
        entry(" INFO [Verse] [Example Mod] hello\n", ""),
        entry(" INFO [Verse] EarlyLoader: Game processing took 12ms\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_eq!(entries_of(&log, EntryClass::Timer), 1);
    assert_eq!(log.timers.len(), 1);
}

#[test]
fn a_mods_unity_debug_message_keeps_the_mod_convention_classes() {
    let text = entry(" INFO [UnityDebug] [Example Mod] hello\n", "");

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
}

// -- frame attribution ------------------------------------------------------------

fn runtime_exception_input(log: &ParsedGameLog) -> Option<AttributionInput> {
    let tally = &log.classes[&EntryClass::RuntimeException];
    assert_eq!(tally.families.len(), 1);
    tally
        .families
        .values()
        .next()
        .expect("one family")
        .attribution_input
        .clone()
}

#[test]
fn an_exceptions_own_dotnet_frames_name_the_mod_type_that_owns_it() {
    let log = waylith_log(fixture_text());

    assert_eq!(
        runtime_exception_input(&log),
        AttributionInput::type_name("ExampleBiomes.CompGrow")
    );
}

#[test]
fn a_mod_type_among_the_captured_frames_is_the_owner_when_the_body_has_none() {
    let text = entry(
        "ERROR [Verse] Exception ticking Example_Pawn1: System.InvalidOperationException: nope\n",
        "    at Verse.Log.Error(String text) [0x1C]\n    at Verse.TickList.Tick() [0x88]\n    at ExampleClimate.Patches.Postfix(Thing __instance) [0x12] in C:\\src\\Patches.cs:30\n    at RimWorldWaylith.Program.Main(String[] args) [0x265]\n",
    );

    let log = waylith_log(&text);

    assert_eq!(
        runtime_exception_input(&log),
        AttributionInput::type_name("ExampleClimate.Patches")
    );
}

#[test]
fn engine_namespaces_and_synthesized_frames_are_never_the_owner() {
    let text = entry(
        "ERROR [Verse] Exception ticking Example_Pawn1: System.InvalidOperationException: nope\n",
        concat!(
            "    at Verse.Log.Error(String text) [0x1C]\n",
            "    at Waylith.UnityCompat.UnityWorld.Update(UnityFrame& frame) [0x17]\n",
            "    at Brutal.VulkanApi.Instance..ctor(CreateInfo createInfo) [0x2DA]\n",
            "    at RimWorldWaylith.Bootstrap+<>c__DisplayClass21_0.<Initialize>b__3() [0x7A]\n",
            "    at .InvokeMessageMethod_Verse.Root_Entry.Start(Closure , Object ) [0xFFFFFFFF]\n",
            "    at .ExamplePatches.Postfixes.Tick_Patch1(Thing __instance) [0x0]\n",
            "    at System.Threading.Thread+StartHelper.Callback(Object state) [0x0]\n",
            "    at UnityEngine.Debug.Log(Object message) [0x0]\n",
        ),
    );

    let log = waylith_log(&text);

    assert_eq!(runtime_exception_input(&log), None);
}

#[test]
fn a_mod_namespace_that_merely_starts_like_an_engine_one_is_kept() {
    let text = entry(
        "ERROR [Verse] Exception ticking Example_Pawn1: System.InvalidOperationException: nope\n",
        "    at WaylithExtras.Thing.Run() [0x1]\n",
    );

    let log = waylith_log(&text);

    assert_eq!(
        runtime_exception_input(&log),
        AttributionInput::type_name("WaylithExtras.Thing")
    );
}

// -- coverage -----------------------------------------------------------------------

#[test]
fn log_closed_is_a_clean_exit() {
    let log = waylith_log(fixture_text());

    assert_eq!(player_coverage(&log).end_state, EndState::CleanExit);
    assert_eq!(
        player_coverage(&log).patch_phase,
        PatchPhaseEvidence::PatchFailureLogged
    );
}

#[test]
fn a_log_with_neither_closing_line_nor_crash_is_cut_off() {
    let text = entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", FRAME);

    let log = waylith_log(&text);

    assert_eq!(player_coverage(&log).end_state, EndState::Truncated);
}

#[test]
fn a_crash_entry_is_a_crash_with_its_report_path_even_after_log_closed() {
    let text = [
        entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", FRAME),
        entry(
            " CRIT [Crash] === CRASH ===\nCrash report written to: C:\\Crashes\\example.txt\nSystem.InvalidOperationException: boom\n   at Example.Thing.Run() in C:\\src\\Thing.cs:line 9\n",
            "",
        ),
        entry(" INFO Log closed.\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(
        player_coverage(&log).end_state,
        EndState::Crashed {
            report_path: Some("C:\\Crashes\\example.txt".to_string())
        }
    );
    assert_p1_and_p2(&log);
}

#[test]
fn a_crash_without_a_report_has_no_path() {
    let text = entry(" CRIT [Crash] === CRASH ===\nSystem.Exception: boom\n", "");

    let log = waylith_log(&text);

    assert_eq!(
        player_coverage(&log).end_state,
        EndState::Crashed { report_path: None }
    );
}

#[test]
fn log_closed_in_a_verse_message_is_not_the_writers_marker() {
    let text = entry(" INFO [Verse] Log closed.\n", "");

    let log = waylith_log(&text);

    assert_eq!(player_coverage(&log).end_state, EndState::Truncated);
}

#[test]
fn the_message_limit_gap_works_behind_the_header() {
    let text = [
        entry(
            " WARN [Verse] Reached max messages limit. Stopping logging to avoid spam.\n",
            "",
        ),
        entry(" INFO [Verse] Message logging is now once again on.\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.logging_gaps.len(), 1);
    assert_eq!(log.logging_gaps[0].stop_line, 1);
    assert_eq!(log.logging_gaps[0].resume, GapEnd::Resumed { line: 2 });
}

#[test]
fn a_backlog_overflow_warning_is_a_gap_at_its_own_line() {
    let text = [
        entry(" INFO [Verse] first\n", ""),
        entry(
            " WARN 12 earlier log line(s) were dropped (pre-init backlog overflow).\n",
            "",
        ),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.logging_gaps.len(), 1);
    assert_eq!(log.logging_gaps[0].stop_line, 2);
    assert_eq!(log.logging_gaps[0].resume, GapEnd::Resumed { line: 2 });
}

// -- the console copy (real 1.7 shape) ----------------------------------------------

fn console_fixture_text() -> &'static str {
    include_str!("../../tests/fixtures/console_waylith_excerpt.txt")
}

const CONSOLE_TRACE: &str = "    at Verse.Root.Start() [0xF]\n    at .InvokeMessageMethod_Verse.Root_Entry.Start(Closure , Object ) [0xFFFFFFFF]\n    at Waylith.Windowing.Core.PlatformBase.DeliverUpdate() [0x6] in C:\\src\\Loop.cs:143\n";

/// One console entry: its message, the logging call that is its level, more
/// frames, and the blank line that ends it.
fn console_entry(text: &str, level: &str) -> String {
    format!("{text}\n    at Verse.Log.{level}(String text) [0x19]\n{CONSOLE_TRACE}\n")
}

fn console_snapshot(log: &ParsedGameLog) -> &rim_session::ports::SnapshotCoverage {
    match &log.coverage {
        LogCoverage::ConsoleSnapshot(coverage) => coverage,
        LogCoverage::PlayerLog(_) => panic!("expected snapshot coverage"),
    }
}

#[test]
fn a_console_copy_is_text_then_waylith_frames_then_a_blank_line() {
    let text = [
        console_entry("RimWorld 1.7.5014 rev1195", "Message"),
        console_entry("[Example Mod] two\nlines of text", "Message"),
        console_entry("Mod Fixture Addon dependency (example.framework) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.", "Warning"),
    ]
    .concat();

    let log = waylith_console(&text);

    let coverage = console_snapshot(&log);
    assert_eq!(coverage.entries, 3);
    assert!(!coverage.head_truncated);
    assert_eq!(log.dependency_warnings.len(), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_console_copy_that_starts_in_a_trace_is_head_truncated() {
    let text = format!(
        "    at Verse.Log.Warning(String text) [0x19]\n{CONSOLE_TRACE}\n{}",
        console_entry("a complete entry", "Message")
    );

    let log = waylith_console(&text);

    assert!(console_snapshot(&log).head_truncated);
    assert_eq!(console_snapshot(&log).entries, 2);
    // The head-truncated entry's own head is the call frame: only it can be
    // the warning (the complete entry is a message).
    let warnings = log
        .classes
        .values()
        .flat_map(|tally| tally.families.values())
        .filter(|family| family.severity == Some(Severity::Warning))
        .count();
    assert_eq!(warnings, 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_blank_line_inside_a_console_message_stays_in_its_entry() {
    let text = console_entry("first paragraph\n\nsecond paragraph", "Message");

    let log = waylith_console(&text);

    assert_eq!(log.totals().entries, 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_console_entrys_severity_is_its_first_frames_log_call() {
    // The arm says a patch failure is an error; the copy's call says Warning.
    let text = console_entry(
        "[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\nfile: C:\\m\\a.xml",
        "Warning",
    );

    let log = waylith_console(&text);

    let family = log.classes[&EntryClass::PatchFailure]
        .families
        .values()
        .next()
        .expect("one family");
    assert_eq!(family.severity, Some(Severity::Warning));
}

#[test]
fn a_later_log_frame_never_overrides_the_first_ones_level() {
    let text = format!(
        "boom\n    at Verse.Log.Error(String text) [0x1C]\n    at Verse.Log.Warning(String text) [0x19]\n{CONSOLE_TRACE}\n"
    );

    let log = waylith_console(&text);

    assert!(
        log.classes
            .values()
            .flat_map(|tally| tally.families.values())
            .all(|family| family.severity == Some(Severity::Error))
    );
}

#[test]
fn a_frame_shaped_line_is_a_trace_start_but_an_indented_body_line_is_text() {
    let text = format!(
        "headline\n    [0] VkInstance 0x10\n    at Verse.Log.Message(String text) [0x19]\n{CONSOLE_TRACE}\nsecond\n    at Verse.Log.Message(String text) [0x19]\n{CONSOLE_TRACE}\n"
    );

    let log = waylith_console(&text);

    assert_eq!(console_snapshot(&log).entries, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn an_exceptions_own_three_space_frames_stay_message_text_and_name_the_owner() {
    let text = console_entry(
        "Exception ticking Example_Pawn1: System.NullReferenceException: nope\n   at ExampleBiomes.CompGrow.CompTick() in C:\\src\\CompGrow.cs:line 18\n   at Verse.TickList.Tick()",
        "Error",
    );

    let log = waylith_console(&text);

    assert_eq!(console_snapshot(&log).entries, 1);
    assert_eq!(
        runtime_exception_input(&log),
        AttributionInput::type_name("ExampleBiomes.CompGrow")
    );
    assert_p1_and_p2(&log);
}

#[test]
fn a_mod_type_among_a_console_entrys_frames_is_the_owner_and_engine_frames_never_are() {
    let owned = format!(
        "Exception ticking Example_Pawn1: System.InvalidOperationException: nope\n    at Verse.Log.Error(String text) [0x1C]\n    at ExampleClimate.Patches.Postfix(Thing __instance) [0x12] in C:\\src\\Patches.cs:30\n{CONSOLE_TRACE}\n"
    );
    let engine_only = console_entry(
        "Exception ticking Example_Pawn1: System.InvalidOperationException: nope",
        "Error",
    );

    assert_eq!(
        runtime_exception_input(&waylith_console(&owned)),
        AttributionInput::type_name("ExampleClimate.Patches")
    );
    assert_eq!(
        runtime_exception_input(&waylith_console(&engine_only)),
        None
    );
}

#[test]
fn the_console_excerpt_is_six_entries_with_the_calls_as_levels() {
    let log = waylith_console(console_fixture_text());

    let coverage = console_snapshot(&log);
    assert_eq!(coverage.entries, 6);
    assert!(!coverage.head_truncated);
    assert_eq!(log.dependency_warnings.len(), 1);
    assert_eq!(
        runtime_exception_input(&log),
        AttributionInput::type_name("ExampleBiomes.CompGrow")
    );
    let severities: Vec<Severity> = log
        .classes
        .values()
        .flat_map(|tally| tally.families.values())
        .filter_map(|family| family.severity)
        .collect();
    assert!(severities.contains(&Severity::Warning));
    assert!(severities.contains(&Severity::Error));
    // The classifier's own rule gives a `[Tag]` line no severity: only the
    // `Verse.Log.Message` frame can make it a message.
    let mod_messages: Vec<Option<Severity>> = log.classes[&EntryClass::ModMessage]
        .families
        .values()
        .map(|family| family.severity)
        .collect();
    assert!(!mod_messages.is_empty());
    assert!(
        mod_messages
            .iter()
            .all(|severity| *severity == Some(Severity::Message)),
        "{mod_messages:?}"
    );
    assert_p1_and_p2(&log);
}

#[test]
fn a_waylith_console_copy_has_snapshot_coverage() {
    let log = waylith_console(console_fixture_text());

    assert!(matches!(log.coverage, LogCoverage::ConsoleSnapshot(_)));
    assert_eq!(log.coverage.kind(), LogKind::ConsoleSnapshot);
}

// -- detection ----------------------------------------------------------------------

#[test]
fn a_header_block_first_line_is_a_waylith_player_log() {
    assert_eq!(detect_framing(fixture_text()), Framing::WaylithLog);
    assert_eq!(detect_kind(fixture_text()), LogKind::PlayerLog);
}

#[test]
fn an_excerpt_cut_to_timestamped_entries_is_a_waylith_player_log() {
    let text = entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", FRAME);

    assert_eq!(detect_framing(&text), Framing::WaylithLog);
    assert_eq!(
        detect_framing(&format!("\n\n{text}")),
        Framing::WaylithLog,
        "leading blank lines do not hide it"
    );
    assert_eq!(
        detect_framing(&format!("\u{feff}{text}")),
        Framing::WaylithLog,
        "a byte-order mark does not hide it"
    );
}

#[test]
fn a_timestamped_line_after_the_first_line_never_makes_a_unity_log_waylith() {
    let text = "Mono path[0] = 'C:/Game/Data/Managed'\nRimWorld 1.6.4871 rev591\n2026-01-02 10:00:00.123  INFO [Verse] quoted by a mod\n";

    assert_eq!(detect_framing(text), Framing::PlayerLog);
}

#[test]
fn the_consoles_log_call_frame_makes_a_waylith_console_copy_even_with_the_banner_first() {
    let banner_first = console_entry("RimWorld 1.7.5014 rev1195", "Message");

    assert_eq!(detect_framing(&banner_first), Framing::WaylithConsole);
    assert_eq!(detect_kind(&banner_first), LogKind::ConsoleSnapshot);
    assert_eq!(
        detect_framing(console_fixture_text()),
        Framing::WaylithConsole
    );
}

#[test]
fn a_head_truncated_console_copy_is_still_detected() {
    let text = format!(
        "    at Verse.Log.Warning(String text) [0x19]\n{CONSOLE_TRACE}\n{}",
        console_entry("next", "Message")
    );

    assert_eq!(detect_framing(&text), Framing::WaylithConsole);
}

#[test]
fn a_waylith_player_log_is_never_a_console_copy_though_its_entries_hold_the_same_frame() {
    // Header block first, timestamped first line, a frame first then a
    // timestamped entry, and the banner rule's reach: every 1.7 Player.log
    // shape holds `Verse.Log.*(String text)` frames.
    let entry_with_call = entry(
        " WARN [Verse] boom\n",
        "    at Verse.Log.Warning(String text) [0x19]\n",
    );
    let shapes = [
        fixture_text().to_string(),
        entry_with_call.clone(),
        format!("    at Verse.Log.Warning(String text) [0x19]\n{entry_with_call}"),
        format!(
            "{CONSOLE_TRACE}    at Verse.Log.Message(String text) [0x19]\nbody\n{entry_with_call}"
        ),
    ];

    for text in shapes {
        assert_eq!(detect_framing(&text), Framing::WaylithLog, "{text}");
    }
}

/// A 1.7 console entry with no stack: its message, `No stack trace.`, a blank line.
fn stackless_console_entry(text: &str) -> String {
    format!("{text}\nNo stack trace.\n\n")
}

#[test]
fn a_waylith_copy_with_a_stackless_entry_among_framed_ones_is_still_a_console_copy() {
    let text = [
        console_entry("RimWorld 1.7.5014 rev1195", "Message"),
        stackless_console_entry("[Example Mod] quiet line"),
        console_entry("later", "Warning"),
    ]
    .concat();
    let stackless_first = format!(
        "{}{}",
        stackless_console_entry("first, no stack"),
        console_entry("second", "Error")
    );

    assert_eq!(detect_framing(&text), Framing::WaylithConsole);
    assert_eq!(detect_framing(&stackless_first), Framing::WaylithConsole);
    let log = waylith_console(&text);
    assert_eq!(console_snapshot(&log).entries, 3);
    assert!(!console_snapshot(&log).head_truncated);
    assert_p1_and_p2(&log);
    let stackless = waylith_console(&stackless_first);
    assert_eq!(console_snapshot(&stackless).entries, 2);
    assert_p1_and_p2(&stackless);
}

#[test]
fn a_stackless_entry_gives_no_severity_of_its_own() {
    let log = waylith_console(&stackless_console_entry("[Example Mod] quiet line"));

    let severities: Vec<Option<Severity>> = log.classes[&EntryClass::ModMessage]
        .families
        .values()
        .map(|family| family.severity)
        .collect();
    assert_eq!(severities, vec![None]);
}

#[test]
fn a_bare_no_stack_trace_line_stays_a_unity_console_copy() {
    let banner_first = "RimWorld 1.6.4871 rev591\nNo stack trace.\n\n";

    assert_eq!(detect_framing(banner_first), Framing::ConsoleCopy);
    assert_eq!(
        detect_framing(&stackless_console_entry("just text")),
        Framing::ConsoleCopy
    );
}

#[test]
fn a_frame_and_a_timestamped_header_in_one_window_is_a_waylith_player_log_even_with_the_banner() {
    let text = format!(
        "RimWorld 1.7.5014 rev1195\n{FRAME}{}",
        entry(" INFO [Verse] hello\n", FRAME)
    );

    assert_eq!(detect_framing(&text), Framing::WaylithLog);
}

#[test]
fn a_log_call_frame_with_trailing_junk_gives_no_level() {
    assert_eq!(
        console_log_severity("    at Verse.Log.Error(String text) [0x1C]"),
        Some(Severity::Error)
    );
    assert_eq!(
        console_log_severity("    at Verse.Log.Error(String text) [0x1C] junk"),
        None
    );
}

#[test]
fn a_unity_log_with_the_banner_is_unaffected_by_the_console_frame_rule() {
    let unity = "Mono path[0] = 'C:/Game/Data/Managed'\nRimWorld 1.6.4871 rev591\nVerse.Log:Message (string)\n";
    let unity_console = "RimWorld 1.6.4871 rev591\nUnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)\n\n";

    assert_eq!(detect_framing(unity), Framing::PlayerLog);
    assert_eq!(detect_framing(unity_console), Framing::ConsoleCopy);
}

#[test]
fn an_unrelated_dotnet_frame_does_not_decide_the_engine() {
    let banner_first = "RimWorld 1.6.4871 rev591\nSystem.Exception: boom\n   at Example.Thing.Run() in C:\\src\\Thing.cs:line 9\n";
    let neither = "System.Exception: boom\n   at Example.Thing.Run() in C:\\src\\Thing.cs:line 9\n";
    let three_space_log_call =
        "System.Exception: boom\n   at Verse.Log.Message(String text) [0x19]\n";

    assert_eq!(detect_framing(banner_first), Framing::PlayerLog);
    assert_eq!(detect_framing(neither), Framing::ConsoleCopy);
    assert_eq!(detect_framing(three_space_log_call), Framing::ConsoleCopy);
}

#[test]
fn the_unity_snapshot_signal_still_outranks_a_waylith_console_signal() {
    let text = format!(
        "UnityEngine.StackTraceUtility:ExtractStackTrace ()\n    at Verse.Log.Message(String text) [0x19]\n{CONSOLE_TRACE}RimWorld 1.6.4871 rev591\n"
    );

    assert_eq!(detect_framing(&text), Framing::ConsoleCopy);
}

#[test]
fn the_reader_reads_a_waylith_file_under_its_own_framing_forced_or_detected() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("Player.log");
    std::fs::write(&path, fixture_text()).expect("write");
    let read = |choice| {
        FileGameLogReader::new()
            .read(&path, &test_formats(), choice)
            .expect("reads")
    };

    let detected = read(KindChoice::Detect);
    let forced = read(KindChoice::Force(LogKind::PlayerLog));

    assert_eq!(detected, waylith_log(fixture_text()));
    assert_eq!(forced, detected);
}

#[test]
fn forcing_the_console_kind_on_a_waylith_player_log_reads_it_as_a_waylith_copy() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("Player.log");
    std::fs::write(&path, fixture_text()).expect("write");

    let log = FileGameLogReader::new()
        .read(
            &path,
            &test_formats(),
            KindChoice::Force(LogKind::ConsoleSnapshot),
        )
        .expect("reads");

    assert_eq!(log, waylith_console(fixture_text()));
    assert!(log.totals().is_conserved());
}

#[test]
fn framing_names_its_kind_and_engine_and_round_trips() {
    for framing in [
        Framing::PlayerLog,
        Framing::ConsoleCopy,
        Framing::WaylithLog,
        Framing::WaylithConsole,
    ] {
        assert_eq!(Framing::of(framing.kind(), framing.dialect()), framing);
    }
    assert_eq!(Framing::from(LogKind::PlayerLog), Framing::PlayerLog);
    assert_eq!(
        Framing::from(LogKind::ConsoleSnapshot),
        Framing::ConsoleCopy
    );
}

// -- the whole excerpt ---------------------------------------------------------------

#[test]
fn the_excerpt_parses_deterministically_conserving_every_line() {
    let first = waylith_log(fixture_text());
    let second = waylith_log(fixture_text());

    assert_eq!(first, second);
    assert_p1_and_p2(&first);
    assert_eq!(
        first.read_stats.lines_read,
        fixture_text().lines().count() as u64
    );
}

#[test]
fn every_unity_framing_still_conserves_the_waylith_excerpt() {
    for framing in [
        Framing::PlayerLog,
        Framing::ConsoleCopy,
        Framing::WaylithConsole,
    ] {
        let log = parse_as(fixture_text(), &test_formats(), framing);

        assert!(log.totals().is_conserved(), "{framing:?}");
    }
}

#[test]
fn a_waylith_log_with_frames_disabled_is_just_messages() {
    // `-DisableLogStackTraces`: no frames follow any entry.
    let text = [
        entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", ""),
        entry(" WARN [Verse] Mod Fixture Addon dependency (example.framework) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.\n", ""),
        entry(" INFO Log closed.\n", ""),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(log.totals().entries, 3);
    assert_eq!(log.dependency_warnings.len(), 1);
    assert_eq!(player_coverage(&log).end_state, EndState::CleanExit);
    assert_p1_and_p2(&log);
}

#[test]
fn a_unity_snapshot_signal_outranks_a_timestamped_first_line() {
    let text = format!(
        "{}UnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)\n\n",
        entry(" INFO [Verse] looks like a 1.7 entry\n", "")
    );

    assert_eq!(detect_framing(&text), Framing::ConsoleCopy);
}

#[test]
fn an_excerpt_that_starts_mid_entry_is_found_by_its_first_timestamped_header() {
    let frame_first = format!(
        "    at Waylith.Graphics.GraphicsDevice.Create(Int32 n) [0x0]\n{}",
        entry(" INFO [Verse] first whole entry\n", FRAME)
    );
    let body_first = format!(
        "    [0] ExampleInstance 0x10\n{}",
        entry(" WARN [Vulkan] note\n", "")
    );

    assert_eq!(detect_framing(&frame_first), Framing::WaylithLog);
    assert_eq!(detect_framing(&body_first), Framing::WaylithLog);
    assert_eq!(detect_kind(&body_first), LogKind::PlayerLog);
}

#[test]
fn the_mid_entry_fallback_yields_to_the_unity_banner_and_to_plain_text() {
    let unity = format!(
        "Mono path[0] = 'C:/Game/Data/Managed'\nRimWorld 1.6.4871 rev591\n{}",
        entry(" INFO [Verse] quoted by a mod\n", "")
    );

    assert_eq!(detect_framing(&unity), Framing::PlayerLog);
    assert_eq!(detect_framing("one\ntwo\n"), Framing::ConsoleCopy);
}

#[test]
fn a_leading_byte_order_mark_parses_like_none() {
    for text in [
        fixture_text(),
        &entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", FRAME),
    ] {
        let bare = waylith_log(text);

        let marked = waylith_log(&format!("\u{feff}{text}"));

        assert_eq!(marked, bare);
        assert_p1_and_p2(&marked);
    }
}

#[test]
fn a_byte_order_mark_is_stripped_once_and_only_from_the_first_line() {
    let text = entry(" INFO [Verse] RimWorld 1.7.5014 rev1195\n", "");
    let second = entry(" INFO [Verse] [Example Mod] second\n", "");

    let log = waylith_log(&format!("\u{feff}{text}\u{feff}{second}"));

    assert_eq!(
        player_coverage(&log).passes,
        1,
        "the marked first line is a banner"
    );
    assert_eq!(
        entries_of(&log, EntryClass::ModMessage),
        0,
        "a mark mid-file stays part of its line"
    );
}

/// A reader that has no `Seek`: what a pipe or a device path gives.
struct OneWayReader(std::io::Cursor<Vec<u8>>);

impl std::io::Read for OneWayReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buffer)
    }
}

#[test]
fn a_forced_kind_reads_a_source_that_cannot_seek() {
    for (text, framing) in [
        (fixture_text(), Framing::WaylithLog),
        (
            "Mono path[0] = 'C:/Game/Data/Managed'\nRimWorld 1.6.4871 rev591\nmessage\n",
            Framing::PlayerLog,
        ),
    ] {
        let reader = BufReader::with_capacity(
            256,
            OneWayReader(std::io::Cursor::new(text.as_bytes().to_vec())),
        );

        let log = parse_forced(reader, &test_formats(), LogKind::PlayerLog).expect("reads");

        assert_eq!(log, parse_as(text, &test_formats(), framing));
    }
}

/// A reader that returns at most `step` bytes per read, whatever the caller
/// asks for and whatever its own buffer could hold.
struct TrickleReader {
    inner: std::io::Cursor<Vec<u8>>,
    step: usize,
}

impl std::io::Read for TrickleReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let limit = buffer.len().min(self.step);
        self.inner.read(&mut buffer[..limit])
    }
}

#[test]
fn a_forced_kind_frames_a_waylith_file_whose_first_reads_are_shorter_than_its_first_line() {
    for step in 1..=3 {
        let reader = BufReader::with_capacity(
            16,
            TrickleReader {
                inner: std::io::Cursor::new(fixture_text().as_bytes().to_vec()),
                step,
            },
        );

        let log = parse_forced(reader, &test_formats(), LogKind::PlayerLog).expect("reads");

        assert_eq!(log, waylith_log(fixture_text()), "step {step}");
    }
}

#[test]
fn a_forced_console_kind_frames_a_waylith_copy_whose_frame_is_on_a_later_line() {
    for step in 1..=3 {
        let reader = BufReader::with_capacity(
            16,
            TrickleReader {
                inner: std::io::Cursor::new(console_fixture_text().as_bytes().to_vec()),
                step,
            },
        );

        let log = parse_forced(reader, &test_formats(), LogKind::ConsoleSnapshot).expect("reads");

        assert_eq!(log, waylith_console(console_fixture_text()), "step {step}");
    }
}

/// A buffered reader whose `Seek` always fails, like a pipe.
struct RefusesToSeek(BufReader<std::io::Cursor<Vec<u8>>>);

impl std::io::Read for RefusesToSeek {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buffer)
    }
}

impl BufRead for RefusesToSeek {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.0.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.0.consume(amount);
    }
}

impl Seek for RefusesToSeek {
    fn seek(&mut self, _position: std::io::SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other("seek refused"))
    }
}

#[test]
fn a_detected_kind_on_a_source_that_cannot_rewind_says_to_force_the_kind() {
    let reader = RefusesToSeek(BufReader::new(std::io::Cursor::new(
        fixture_text().as_bytes().to_vec(),
    )));

    let error = parse_detected(reader, &test_formats(), Path::new("Player.log"))
        .expect_err("a rewind failure is an error");

    assert!(error.0.contains("seek refused"), "{}", error.0);
    assert!(
        error.0.contains("`--kind` on the command line"),
        "{}",
        error.0
    );
    assert!(!error.0.contains("  "), "{}", error.0);
}

// -- engine exception wrappers and decoder failures ------------------------------

#[test]
fn a_graphic_database_failure_and_a_post_long_event_failure_are_runtime_exceptions_in_both_framings()
 {
    let heads = [
        "Exception getting Example.Graphic_Widget at Things/Example/WidgetA: System.MissingMethodException: nope",
        "Could not execute post-long-event action. Exception: System.NullReferenceException: nope",
        "Rat threw exception in WorkGiver ExampleClean: System.NullReferenceException: nope",
    ];
    for head in heads {
        let text = entry(&format!("ERROR [Verse] {head}\n"), FRAME);
        let console = format!("{head}\n{FRAME}\n");

        let player_log = waylith_log(&text);
        let console_copy = waylith_console(&console);

        assert_eq!(
            entries_of(&player_log, EntryClass::RuntimeException),
            1,
            "{head}"
        );
        assert_eq!(
            entries_of(&player_log, EntryClass::Unclassified),
            0,
            "{head}"
        );
        assert_eq!(
            entries_of(&console_copy, EntryClass::RuntimeException),
            1,
            "{head}"
        );
    }
}

#[test]
fn a_vulkan_decoder_failure_keeps_its_class_and_names_its_file() {
    let text = [
        entry(
            "ERROR [Vulkan] Unable to load PSD C:/Mods/Example Mod/Textures/Widget.psd due to an exception in PsdSharp: PSD file is corrupt.)\n",
            "    at Waylith.Rendering.GpuTextureService.DecodeTextureFile(String fullPath) [0x4F]\n",
        ),
        entry(
            "ERROR [Vulkan] C:/Mods/Example Mod/Textures/Menu.png: the file's contents are not the format its extension names; rename it so the decoder meant for it reads it\n",
            "    at Waylith.Rendering.GpuTextureService.LoadTextureFromFile(String filePath) [0x19]\n",
        ),
    ]
    .concat();

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::TextureLoadFailure), 2);
    assert_eq!(
        entries_of(&log, EntryClass::EngineInfo(EngineInfoKind::UnityRuntime)),
        0
    );
    let inputs: Vec<_> = log.classes[&EntryClass::TextureLoadFailure]
        .families
        .values()
        .map(|family| family.attribution_input.clone())
        .collect();
    assert!(inputs.contains(&Some(AttributionInput::path(
        "C:/Mods/Example Mod/Textures/Menu.png"
    ))));
    assert_p1_and_p2(&log);
}

#[test]
fn an_unrelated_vulkan_error_stays_engine_chatter() {
    // A WARN in an engine category that no arm recognizes (it only looks like
    // a startup timer): engine chatter, not a texture failure.
    let text = entry(" WARN [Vulkan] GpuProbe: completed in 84 ms\n", "");

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::TextureLoadFailure), 0);
    assert_eq!(
        entries_of(&log, EntryClass::EngineInfo(EngineInfoKind::UnityRuntime)),
        1
    );
    assert_eq!(entries_of(&log, EntryClass::Timer), 0);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 0);
}

#[test]
fn a_determining_job_failure_keeps_its_exception_and_key_frame_in_the_entry_on_waylith() {
    // On a Unity log the exception line is its own entry; on Waylith it
    // continues the head's entry, so the key frame is present (the type is
    // not in the key either way).
    let text = entry(
        "ERROR [Verse] Rat threw exception while determining job (main) curDriver=JobDriver_Example\n",
        "System.NullReferenceException: nope\n   at ExampleMod.Work.Cleaner.Scan () in <b>:0\n    at Verse.Log.Error(String text) [0x19]\n",
    );

    let log = waylith_log(&text);

    assert_eq!(entries_of(&log, EntryClass::RuntimeException), 1);
    let keys: Vec<_> = log.classes[&EntryClass::RuntimeException]
        .families
        .keys()
        .map(|key| key.as_str().to_string())
        .collect();
    assert_eq!(
        keys,
        ["threw exception while determining job (main)||ExampleMod.Work.Cleaner"]
    );
    assert_p1_and_p2(&log);
}

// -- a modded console copy: the log-call frame can be absent ----------------------------

fn modded_console_fixture_text() -> &'static str {
    include_str!("../../tests/fixtures/console_waylith_modded_excerpt.txt")
}

/// A console entry whose trace opens at the caller (a mod's static
/// constructor), never at a `Verse.Log.*` frame.
fn caller_first_console_entry(text: &str) -> String {
    format!(
        "{text}\n    at ExampleFixture.HarmonyPatches..cctor() [0x19]\n    at System.Runtime.CompilerServices.RuntimeHelpers.RunClassConstructor(QCallTypeHandle type) [0x0]\n{CONSOLE_TRACE}\n"
    )
}

#[test]
fn a_console_copy_whose_traces_open_at_the_caller_is_a_waylith_console_copy() {
    let text = [
        caller_first_console_entry("first"),
        caller_first_console_entry("second\nwith a second line"),
        caller_first_console_entry("third"),
    ]
    .concat();

    assert_eq!(detect_framing(&text), Framing::WaylithConsole);
    assert_eq!(detect_kind(&text), LogKind::ConsoleSnapshot);
    let log = parse_as(&text, &test_formats(), detect_framing(&text));
    assert_eq!(console_snapshot(&log).entries, 3);
    assert!(!console_snapshot(&log).head_truncated);
    assert_p1_and_p2(&log);
}

#[test]
fn a_mixed_copy_with_and_without_the_log_call_frame_segments_every_entry() {
    let text = [
        console_entry("RimWorld 1.7.5014 rev1195", "Message"),
        caller_first_console_entry("[Example Mod] no log call"),
        console_entry("[Example Mod] with a log call", "Warning"),
        caller_first_console_entry("[Example Mod] no log call again"),
    ]
    .concat();

    assert_eq!(detect_framing(&text), Framing::WaylithConsole);
    let log = waylith_console(&text);
    assert_eq!(console_snapshot(&log).entries, 4);
    assert_p1_and_p2(&log);
}

#[test]
fn the_modded_excerpt_fixture_segments_into_its_entries_and_keeps_message_continuations() {
    let text = modded_console_fixture_text();

    // Like the real modded copy (0 of 1,000 entries): no log-call frame
    // anywhere, so the old "Verse.Log.* frame" rule cannot detect it.
    assert!(!text.contains("Verse.Log."));
    assert_eq!(detect_framing(text), Framing::WaylithConsole);
    let log = waylith_console(text);

    assert_eq!(console_snapshot(&log).entries, 6);
    assert!(!console_snapshot(&log).head_truncated);
    assert_eq!(entries_of(&log, EntryClass::TypeLoadError), 1);
    assert_eq!(log.dependency_warnings.len(), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn an_entry_without_a_log_call_frame_keeps_the_classifiers_severity() {
    let log = waylith_console(&caller_first_console_entry("[Example Mod] plain text"));

    let severities: Vec<Option<Severity>> = log.classes[&EntryClass::ModMessage]
        .families
        .values()
        .map(|family| family.severity)
        .collect();
    assert_eq!(severities.len(), 1);
    assert_ne!(severities, vec![Some(Severity::Error)]);
    assert_p1_and_p2(&log);
}

#[test]
fn a_waylith_player_log_excerpt_cut_mid_entry_with_a_caller_frame_first_is_still_a_player_log() {
    let text = format!(
        "{CONSOLE_TRACE}    at ExampleFixture.HarmonyPatches..cctor() [0x19]\nbody\n{}",
        entry(" WARN [Verse] boom\n", FRAME)
    );

    assert_eq!(detect_framing(&text), Framing::WaylithLog);
    assert_eq!(detect_kind(&text), LogKind::PlayerLog);
}

#[test]
fn a_unity_console_copy_is_unchanged_by_the_frame_shape_rule() {
    let unity_console = "RimWorld 1.6.4871 rev591\nUnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)\n\n[Example Mod] text\nNo stack trace.\n\n";
    let three_space = "RimWorld 1.6.4871 rev591\nSystem.Exception: boom\n   at ExampleFixture.Thing.Run() [0x19]\n";

    assert_eq!(detect_framing(unity_console), Framing::ConsoleCopy);
    assert_eq!(detect_framing(three_space), Framing::PlayerLog);
}
