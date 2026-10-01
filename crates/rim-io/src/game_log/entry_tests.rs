//! Tests of the entry pipeline: the classifier arms, the segmenter's two
//! framings and block contexts, the aggregation bounds, the sentinel check,
//! and the conservation properties (P1/P2). Every fixture is synthetic.

use std::io::BufReader;

use proptest::prelude::*;
use rim_session::ports::{
    AttributionInput, ClassTally, EngineInfoKind, EntryClass, ParsedGameLog, SaveLoadPhase,
    Severity,
};

use super::classify::ClassifierSet;
use super::sentinels::SentinelSet;
use super::shapes::{CompiledShapes, ShapeSentinel};
use super::tests::{GeneratedLog, stream, test_formats};
use super::*;

fn fixture_text() -> &'static str {
    include_str!("../../tests/fixtures/player_log_excerpt.log")
}

fn formats() -> LogFormats<'static> {
    test_formats()
}

fn compiled_shapes() -> CompiledShapes {
    CompiledShapes::new(formats().shapes)
}

fn player(text: &str) -> ParsedGameLog {
    parse(text, &formats())
}

fn console(text: &str) -> ParsedGameLog {
    parse_as(text, &formats(), Framing::ConsoleCopy)
}

fn entries_of(log: &ParsedGameLog, class: EntryClass) -> u64 {
    log.classes.get(&class).map_or(0, ClassTally::entries)
}

fn lines_of_class(log: &ParsedGameLog, class: EntryClass) -> u64 {
    log.classes.get(&class).map_or(0, ClassTally::lines)
}

fn families_of(log: &ParsedGameLog, class: EntryClass) -> usize {
    log.classes
        .get(&class)
        .map_or(0, |tally| tally.families.len())
}

/// P1 and P2: every line accounted for, and no sentinel leak.
fn assert_p1_and_p2(log: &ParsedGameLog) {
    let totals = log.totals();
    assert!(totals.is_conserved(), "P1 broken: {totals:?}");
    assert_eq!(
        log.sentinels.total, 0,
        "sentinel leaks: {:?}",
        log.sentinels.leaks
    );
}

fn classification_of(head: &str) -> (EntryClass, Option<Severity>) {
    let found =
        ClassifierSet::builtin().classify(head, formats().def_cache_carriers, &compiled_shapes());
    (found.class, found.severity)
}

fn class_of(head: &str) -> EntryClass {
    classification_of(head).0
}

// -- the classifier: each class, and a near miss that must not classify ------

#[test]
fn every_class_recognises_its_own_head() {
    use EngineInfoKind as Info;
    use EntryClass as C;
    let cases: &[(&str, EntryClass)] = &[
        ("Initializing new game with mods:", C::LoadEvent),
        ("Loading game from file My Save with mods:", C::LoadEvent),
        ("RimWorld 1.6.4871 rev591", C::EngineInfo(Info::Banner)),
        (
            "Reached max messages limit. Stopping logging to avoid spam.",
            C::EngineInfo(Info::LoggingStopped),
        ),
        (
            "Message logging is now once again on.",
            C::EngineInfo(Info::LoggingResumed),
        ),
        (
            "[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed",
            C::PatchFailure,
        ),
        ("Config error in Example_Def: bad field", C::ConfigError),
        (
            "Exception in ConfigErrors() of Example_Def: System.Exception: x",
            C::ConfigError,
        ),
        ("Error in patch.Apply(): boom", C::PatchError),
        (
            "Config error in Example Mod patch Verse.PatchOperationAdd: bad",
            C::PatchError,
        ),
        ("[Example Mod - Start of stack trace]", C::PatchStackTrace),
        (
            "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)",
            C::CrossReference,
        ),
        (
            "Could not resolve cross-reference: No Verse.ThingDef named Foo found to give to Verse.Tool fist",
            C::CrossReference,
        ),
        (
            r#"XML error: Could not find parent node named "Base" for node "Child". Full node: <Child/>"#,
            C::MissingParent,
        ),
        (
            "XML error: Cyclic inheritance hierarchy detected",
            C::XmlError,
        ),
        ("Exception reading a.xml as XML: bad", C::XmlError),
        ("Mod/Defs/a.xml: unknown parse failure", C::XmlError),
        ("Adding duplicate Verse.ThingDef name: Foo", C::DuplicateDef),
        (
            "DDS loading failed for 'C:/a/b.dds': anything at all",
            C::TextureFallback,
        ),
        (
            "Could not load UnityEngine.Texture2D at 'Things/X' in any active mod or in base resources.",
            C::TextureLoadFailure,
        ),
        (
            "Failed to find any textures at Things/X while constructing Graphic_Multi",
            C::TextureLoadFailure,
        ),
        (
            "Error in static constructor of Example.Type: System.Exception: x",
            C::TypeLoadError,
        ),
        (
            "Object with load ID Thing_Human12 is referenced (xml node name: pawn) but is not deep-saved.",
            C::SaveLoadReference(SaveLoadPhase::Save),
        ),
        (
            "Could not resolve reference to object with loadID Thing_X of type Y.",
            C::SaveLoadReference(SaveLoadPhase::Load),
        ),
        (
            "Mod Foo dependency (bar.baz) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.",
            C::ModMetadataWarning,
        ),
        (
            r#"Could not find any RuleDef for symbol "foo" with any resolver that could resolve x"#,
            C::BaseGenRuleMissing,
        ),
        (
            "Exception ticking Foo123 (at (1, 2, 3)): System.NullReferenceException: x",
            C::RuntimeException,
        ),
        ("System.InvalidCastException: bad", C::RuntimeException),
        ("GUI Error: Invalid GUILayout state", C::UnityRuntimeError),
        ("EXAMPLECACHE: purged 3 files", C::DefCacheLine),
        ("[Some Mod] init took 61ms", C::Timer),
        (
            "Command line arguments: -x",
            C::EngineInfo(Info::SessionMarker),
        ),
        ("Memory Statistics:", C::EngineInfo(Info::SessionMarker)),
        ("Crash!!!", C::EngineInfo(Info::SessionMarker)),
        ("Mono path[0] = 'C:/x'", C::EngineInfo(Info::UnityRuntime)),
        ("[Some Mod] hello", C::ModMessage),
        ("just some words", C::Unclassified),
    ];
    for (head, expected) in cases {
        assert_eq!(class_of(head), *expected, "{head}");
    }
}

#[test]
fn a_near_miss_does_not_classify_as_the_class_it_resembles() {
    use EntryClass as C;
    let cases: &[(&str, EntryClass)] = &[
        ("Initializing new game with mods", C::Unclassified),
        ("RimWorld 1.6 rev", C::Unclassified),
        ("Reached max messages limit", C::Unclassified),
        ("[Example Mod] Patch operation succeeded", C::ModMessage),
        ("Config error in", C::Unclassified),
        ("Error in patch.Apply()", C::Unclassified),
        ("[Example Mod - End of stack trace]", C::ModMessage),
        ("Could not resolve cross reference to X", C::Unclassified),
        ("XML error", C::Unclassified),
        ("unknown parse failure", C::Unclassified),
        ("Adding duplicate", C::Unclassified),
        ("DDS loading failed", C::Unclassified),
        ("Could not load Texture2D at 'x'", C::Unclassified),
        ("Error in static constructor", C::Unclassified),
        ("Object with load ID 5", C::Unclassified),
        ("Mod Foo dependency (x) is fine", C::Unclassified),
        ("Could not find any RuleDef for symbol foo", C::Unclassified),
        ("No exception here", C::Unclassified),
        ("GUI Error", C::Unclassified),
        ("[Some Mod] init done", C::ModMessage),
        ("Command line arguments", C::Unclassified),
        ("Mono path is set", C::Unclassified),
        ("[unclosed tag", C::Unclassified),
    ];
    for (head, expected) in cases {
        assert_eq!(class_of(head), *expected, "{head}");
    }
}

#[test]
fn the_sound_variant_of_a_cross_reference_is_a_warning_and_the_rest_are_errors() {
    let sound = "Could not resolve cross-reference: No Verse.SoundDef named X found to give to \
                 Verse.Tool y (using undefined sound instead)";
    let plain = "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)";

    assert_eq!(classification_of(sound).1, Some(Severity::Warning));
    assert_eq!(classification_of(plain).1, Some(Severity::Error));
}

#[test]
fn a_crash_marker_is_an_error_and_the_memory_footer_head_is_a_message() {
    assert_eq!(classification_of("Crash!!!").1, Some(Severity::Error));
    assert_eq!(
        classification_of("Memory Statistics:").1,
        Some(Severity::Message)
    );
}

#[test]
fn the_def_form_of_a_config_error_wins_over_the_patch_form() {
    assert_eq!(
        class_of("Config error in Some_Def: x"),
        EntryClass::ConfigError
    );
    assert_eq!(
        class_of("Config error in Some Mod patch Verse.PatchOperationAdd: x"),
        EntryClass::PatchError
    );
}

#[test]
fn a_color_tagged_head_classifies_by_its_text() {
    assert_eq!(
        class_of("<color=orange>Adding duplicate Verse.ThingDef name: Foo</color>"),
        EntryClass::DuplicateDef
    );
}

#[test]
fn a_def_cache_line_wins_over_the_timer_it_also_is() {
    assert_eq!(
        class_of("EXAMPLECACHE: purge took 12ms"),
        EntryClass::DefCacheLine
    );
    assert_eq!(class_of("[Some Mod] init took 12ms"), EntryClass::Timer);
}

#[test]
fn every_class_but_the_generic_ones_has_a_sentinel() {
    use EngineInfoKind as Info;
    use EntryClass as C;
    let engine = SentinelSet::builtin();
    let from_shapes = SentinelSet::from_shapes(compiled_shapes().sentinels());
    let has_sentinel = |class| engine.covers(class) || from_shapes.covers(class);
    let covered = [
        C::LoadEvent,
        C::EngineInfo(Info::LoggingStopped),
        C::EngineInfo(Info::LoggingResumed),
        C::PatchFailure,
        C::ConfigError,
        C::PatchError,
        C::PatchStackTrace,
        C::CrossReference,
        C::MissingParent,
        C::XmlError,
        C::DuplicateDef,
        C::TextureFallback,
        C::TextureLoadFailure,
        C::TypeLoadError,
        C::SaveLoadReference(SaveLoadPhase::Save),
        C::SaveLoadReference(SaveLoadPhase::Load),
        C::ModMetadataWarning,
        C::BaseGenRuleMissing,
        C::RuntimeException,
        C::UnityRuntimeError,
    ];
    for class in covered {
        assert!(has_sentinel(class), "{class:?} has no sentinel");
    }
    // Exempt on purpose: the banner and Unity chatter (too generic to be a
    // fragment), session markers, def-cache lines (their prefix is data),
    // timers (a measurement), and the two catch-all classes.
    let exempt = [
        C::EngineInfo(Info::Banner),
        C::EngineInfo(Info::SessionMarker),
        C::EngineInfo(Info::UnityRuntime),
        C::DefCacheLine,
        C::Timer,
        C::ModMessage,
        C::Unclassified,
    ];
    for class in exempt {
        assert!(!has_sentinel(class), "{class:?} unexpectedly covered");
    }
}

#[test]
fn shape_sentinels_that_exceed_the_regex_limits_are_dropped_without_panicking() {
    // Beyond what a template can hold (512 bytes), so it only arises from a
    // constructed value; it proves the set is built under the size limit and
    // a failure drops the shape sentinels instead of aborting the parse.
    let oversized = ShapeSentinel {
        name: "texture-fallback",
        literal: "x".repeat(2 * 1024 * 1024),
        class: EntryClass::TextureFallback,
    };

    let from_shapes = SentinelSet::from_shapes(vec![oversized]);

    assert!(!from_shapes.covers(EntryClass::TextureFallback));
}

#[test]
fn shape_sentinels_within_the_limits_are_kept() {
    let from_shapes = SentinelSet::from_shapes(compiled_shapes().sentinels());

    assert!(from_shapes.covers(EntryClass::TextureFallback));
    assert!(from_shapes.covers(EntryClass::PatchStackTrace));
}

// -- the segmenter: Player.log framing ------------------------------------------

#[test]
fn a_load_block_is_one_entry_with_its_items_and_ends_at_the_first_other_line() {
    let log = player(
        "Loading game from file A Save with mods:\n  - core\n  - example.mod (incompatible version)\n[Tag] next\n",
    );

    assert_eq!(entries_of(&log, EntryClass::LoadEvent), 1);
    assert_eq!(lines_of_class(&log, EntryClass::LoadEvent), 3);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn an_item_shaped_line_outside_a_load_block_is_not_glued_to_anything() {
    let log = player("[Tag] hello\n- core\n");

    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_stack_block_swallows_its_body_and_its_source_file_line() {
    let log = player(
        "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"/Defs\"): Failed\n[Patch operation x failed]\n[End of stack trace]\nSource file: C:\\m\\a.xml\n[Tag] after\n",
    );

    assert_eq!(entries_of(&log, EntryClass::PatchStackTrace), 1);
    assert_eq!(lines_of_class(&log, EntryClass::PatchStackTrace), 5);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn an_unclosed_stack_block_takes_the_rest_of_its_lines_and_stays_conserved() {
    let log = player(
        "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"/Defs\"): Failed\nmore\nmore\n",
    );

    assert_eq!(entries_of(&log, EntryClass::PatchStackTrace), 1);
    assert_eq!(lines_of_class(&log, EntryClass::PatchStackTrace), 4);
    assert_p1_and_p2(&log);
}

#[test]
fn a_stack_block_ends_at_its_end_line_even_when_no_source_file_line_follows() {
    let log =
        player("[Example Mod - Start of stack trace]\nbody\n[End of stack trace]\n[Tag] next\n");

    assert_eq!(lines_of_class(&log, EntryClass::PatchStackTrace), 3);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
}

#[test]
fn the_memory_footer_keeps_its_indented_alloc_and_blank_lines() {
    let log = player(
        "Memory Statistics:\n  [ALLOC_TEMP_MAIN]\n    Peak memory usage: 4MB\n\n[ALLOC_DEFAULT] x\n\n[Tag] after\n",
    );

    let marker = EntryClass::EngineInfo(EngineInfoKind::SessionMarker);
    assert_eq!(entries_of(&log, marker), 1);
    // head + 2 indented + blank + [ALLOC_ line + blank inside the footer
    assert_eq!(lines_of_class(&log, marker), 6);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_blank_line_outside_a_footer_is_a_separator_and_belongs_to_no_entry() {
    let log = player("[Tag] one\n\n[Tag] two\n\n\n");

    assert_eq!(log.blank_separator_lines, 3);
    assert_eq!(log.totals().entry_lines, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn an_xml_error_dump_takes_the_lines_that_start_with_a_tag() {
    let log = player(
        "XML error: Could not find parent node named \"A\" for node \"B\". Full node: <B>\n<li>one</li>\n</B>\nnext line\n",
    );

    assert_eq!(entries_of(&log, EntryClass::MissingParent), 1);
    assert_eq!(lines_of_class(&log, EntryClass::MissingParent), 3);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_tag_shaped_line_after_an_ordinary_head_is_not_an_xml_dump() {
    let log = player("[Tag] hello\n<li>one</li>\n");

    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
}

#[test]
fn the_file_line_after_a_patch_failure_joins_it_but_a_file_line_elsewhere_does_not() {
    let log = player(
        "[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\nfile: C:\\m\\a.xml\n[Tag] hello\nfile: C:\\m\\b.xml\n",
    );

    assert_eq!(lines_of_class(&log, EntryClass::PatchFailure), 2);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn the_line_after_a_texture_fallback_head_joins_it_only_when_it_is_the_trailer() {
    let joined = player("DDS loading failed for 'a.dds': x\nLoading from png instead.\n");
    let not_joined = player("DDS loading failed for 'a.dds': x\nsomething else\n");

    assert_eq!(lines_of_class(&joined, EntryClass::TextureFallback), 2);
    assert_eq!(lines_of_class(&not_joined, EntryClass::TextureFallback), 1);
    assert_eq!(entries_of(&not_joined, EntryClass::Unclassified), 1);
}

#[test]
fn a_texture_fallback_and_its_trailer_are_one_entry_in_a_console_copy_too() {
    // The loader writes both lines as one warning, so a console copy shows
    // them as one message's text, followed by that message's trace.
    let text = format!(
        "DDS loading failed for 'a.dds': Cannot load compressed texture with non multiple of 4 dimensions of 10x10 and format DXT1\r\nLoading from png instead.\r\n{COPY_TRACE}\r\n[Tag] next\r\nNo stack trace.\r\n\r\n"
    );

    let log = console(&text);

    assert_eq!(entries_of(&log, EntryClass::TextureFallback), 1);
    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.dds_failures.len(), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn frames_and_indented_detail_join_the_entry_they_follow() {
    let log = player(
        "System.Exception: boom\n  at Example.Type.Method () [0x00000] in <a>:0\n(wrapper managed-to-native) X:Y ()\n--- End of inner exception stack trace ---\n    - PREFIX detail\nUnityEngine.Debug:Log (object)\n",
    );

    assert_eq!(entries_of(&log, EntryClass::RuntimeException), 1);
    assert_eq!(lines_of_class(&log, EntryClass::RuntimeException), 6);
    assert_p1_and_p2(&log);
}

#[test]
fn a_line_that_only_resembles_a_frame_starts_its_own_entry() {
    let log = player("System.Exception: boom\nnot a frame: (really)\nplain words\n");

    assert_eq!(lines_of_class(&log, EntryClass::RuntimeException), 1);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 2);
}

#[test]
fn a_crash_report_runs_to_its_end_marker() {
    let log = player(
        "Crash!!!\nSymptom: x\nframe 1\n========== END OF STACKTRACE ===========\n[Tag] after\n",
    );

    let marker = EntryClass::EngineInfo(EngineInfoKind::SessionMarker);
    assert_eq!(lines_of_class(&log, marker), 4);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn an_xml_dump_ends_at_the_first_line_that_is_not_a_tag_even_when_it_joins_another_way() {
    let log = player(
        "XML error: Could not find parent node named \"A\" for node \"B\". Full node: <B>\n<li>one</li>\n  indented\n<b>\n",
    );

    assert_eq!(lines_of_class(&log, EntryClass::MissingParent), 3);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_p1_and_p2(&log);
}

// -- the segmenter: console-copy framing ----------------------------------------

const COPY_TRACE: &str =
    "UnityEngine.StackTraceUtility:ExtractStackTrace ()\r\nVerse.Log:Message (string)\r\n";

#[test]
fn a_console_copy_entry_is_its_text_then_its_trace_then_a_blank_line() {
    let text = format!("[Tag] first\r\n{COPY_TRACE}\r\n[Tag] second\r\nNo stack trace.\r\n\r\n");

    let log = console(&text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.blank_separator_lines, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn an_unclosed_stack_block_head_in_a_console_copy_does_not_merge_entries() {
    let text = "[Example Mod - Start of stack trace]\r\nUnityEngine.StackTraceUtility:ExtractStackTrace ()\r\n\r\n[Tag] next\r\nNo stack trace.\r\n\r\n";

    let log = console(text);

    assert_eq!(log.totals().entries, 2);
    assert_eq!(log.blank_separator_lines, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn a_crash_head_in_a_console_copy_does_not_merge_entries() {
    let text = "Crash!!!\r\nNo stack trace.\r\n\r\n[Tag] next\r\nNo stack trace.\r\n\r\n";

    let log = console(text);

    assert_eq!(log.totals().entries, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn a_blank_line_inside_a_console_message_text_does_not_end_the_entry() {
    let text =
        format!("line one\r\n\r\nline two\r\n{COPY_TRACE}\r\nsecond message\r\n{COPY_TRACE}");

    let log = console(&text);

    assert_eq!(log.totals().entries, 2, "one entry per trace start");
    assert_eq!(log.blank_separator_lines, 1);
    assert_p1_and_p2(&log);
}

#[test]
fn the_same_text_read_as_a_player_log_splits_at_the_blank_line() {
    let text = format!("line one\r\n\r\nline two\r\n{COPY_TRACE}");

    let log = player(&text);

    assert_eq!(log.blank_separator_lines, 1);
    assert!(log.totals().entries > 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_console_entry_whose_text_holds_mono_frames_keeps_them_as_text() {
    let text = format!(
        "System.Exception: boom\r\n  at Example.Type.Method () in <a>:0\r\n{COPY_TRACE}\r\nnext\r\n{COPY_TRACE}"
    );

    let log = console(&text);

    assert_eq!(log.totals().entries, 2);
    assert_p1_and_p2(&log);
}

#[test]
fn a_console_copy_of_one_thousand_entries_has_one_thousand_entries() {
    let entry = format!("[Tag] message\r\n{COPY_TRACE}\r\n");
    let text = entry.repeat(1_000);

    let log = console(&text);

    assert_eq!(log.totals().entries, 1_000);
    assert_p1_and_p2(&log);
}

// -- aggregation ---------------------------------------------------------------

#[test]
fn entries_with_the_same_key_merge_into_one_family_with_a_span_and_a_sample() {
    let log = player(
        "Adding duplicate Verse.ThingDef name: Foo\n[Tag] x\nAdding duplicate Verse.ThingDef name: Foo\n",
    );

    let tally = &log.classes[&EntryClass::DuplicateDef];
    let (key, family) = tally.families.iter().next().expect("one family");
    assert_eq!(tally.families.len(), 1);
    assert_eq!(key.as_str(), "Verse.ThingDef Foo");
    assert_eq!(family.count, 2);
    assert_eq!((family.first_line, family.last_line), (1, 3));
    assert_eq!(family.severity, Some(Severity::Error));
    let sample = family.sample.as_ref().expect("a sample is kept");
    assert_eq!(sample.text, "Adding duplicate Verse.ThingDef name: Foo");
    assert!(!sample.is_truncated);
}

#[test]
fn two_defs_with_different_names_are_different_families() {
    let log = player(
        "Adding duplicate Verse.ThingDef name: Foo\nAdding duplicate Verse.ThingDef name: Bar\n",
    );

    assert_eq!(families_of(&log, EntryClass::DuplicateDef), 2);
}

#[test]
fn numbers_and_long_hex_words_are_normalized_in_a_generic_key() {
    let log = player(
        "Tried to calculate chance for 12 at DEADBEEF01 end\nTried to calculate chance for 99 at ABCDEF0123 end\n",
    );

    assert_eq!(families_of(&log, EntryClass::Unclassified), 1);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 2);
}

#[test]
fn a_family_counts_its_entries_per_startup_pass() {
    let log = player(
        "Mono path[0] = 'a'\nRimWorld 1.6.4871 rev591\nAdding duplicate T name: Foo\nRimWorld 1.6.4871 rev591\nAdding duplicate T name: Foo\nAdding duplicate T name: Foo\n",
    );

    let family = log.classes[&EntryClass::DuplicateDef]
        .families
        .values()
        .next()
        .expect("one family");
    assert_eq!(family.count_by_pass.get(&1), Some(&1));
    assert_eq!(family.count_by_pass.get(&2), Some(&2));
    assert_eq!(log.totals().passes, 2);
}

#[test]
fn entries_before_the_first_banner_are_pass_zero() {
    let log = player("Adding duplicate T name: Foo\nRimWorld 1.6.4871 rev591\n");

    let family = log.classes[&EntryClass::DuplicateDef]
        .families
        .values()
        .next()
        .expect("one family");
    assert_eq!(family.count_by_pass.get(&0), Some(&1));
}

#[test]
fn startup_passes_past_the_tracked_limit_fold_into_the_last_and_are_counted() {
    let text = "RimWorld 1.6.4871 rev591\n".repeat(20);

    let log = player(&text);

    let family = log.classes[&EntryClass::EngineInfo(EngineInfoKind::Banner)]
        .families
        .values()
        .next()
        .expect("one family");
    assert_eq!(family.count_by_pass.len(), 16);
    assert_eq!(family.count_by_pass.get(&16), Some(&5));
    assert_eq!(log.read_stats.passes_folded, 4);
    assert_eq!(log.totals().passes, 20, "passes are counted honestly");
}

fn alpha(mut number: usize) -> String {
    let mut word = String::new();
    loop {
        word.push(char::from(
            b'a' + u8::try_from(number % 26).expect("below 26"),
        ));
        number /= 26;
        if number == 0 {
            return word;
        }
    }
}

#[test]
fn a_class_past_its_family_bound_counts_the_rest_in_its_overflow() {
    let mut text = String::new();
    for index in 0..5_010 {
        text.push_str(&format!("distinct message {}\n", alpha(index)));
    }

    let log = player(&text);

    let tally = &log.classes[&EntryClass::Unclassified];
    assert_eq!(tally.families.len(), 5_000);
    assert_eq!(tally.overflow.entries, 10);
    assert_eq!(tally.overflow.lines, 10);
    assert_eq!(tally.entries(), 5_010);
    assert_p1_and_p2(&log);
}

#[test]
fn families_across_classes_stop_at_the_overall_bound_and_the_rest_overflow() {
    let makers: [fn(&str) -> String; 12] = [
        |w| format!("distinct message {w}\n"),
        |w| format!("[Tag] {w}\n"),
        |w| format!("Could not resolve cross-reference to T named {w} (wanter=x)\n"),
        |w| format!("XML error: Could not find parent node named \"{w}\" for node \"n\".\n"),
        |w| format!("Could not load T at '{w}' in any active mod or in base resources.\n"),
        |w| {
            format!(
                "Mod {w} dependency (x) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.\n"
            )
        },
        |w| format!("[{w}] Patch operation Verse.PatchOperationAdd(x) failed\n"),
        |w| format!("DDS loading failed for '{w}': x\n"),
        |w| format!("Adding duplicate T name: {w}\n"),
        |w| format!("Could not find any RuleDef for symbol \"{w}\" x\n"),
        |w| format!("Config error in Def_{w}: x\n"),
        |w| format!("XML format error: {w}\n"),
    ];
    let mut text = String::new();
    for make in makers {
        for index in 0..4_200 {
            text.push_str(&make(&alpha(index)));
        }
    }

    let log = player(&text);

    let families: usize = log.classes.values().map(|tally| tally.families.len()).sum();
    let overflow: u64 = log
        .classes
        .values()
        .map(|tally| tally.overflow.entries)
        .sum();
    assert_eq!(families, 50_000);
    assert_eq!(overflow, 12 * 4_200 - 50_000);
    assert_p1_and_p2(&log);
}

#[test]
fn a_sample_is_cut_to_its_line_and_byte_bounds_and_says_so() {
    let mut text = String::from("System.Exception: boom\n");
    for index in 0..40 {
        text.push_str(&format!("  at Example.Type{index}.Method () in <a>:0\n"));
    }

    let log = player(&text);

    let family = log.classes[&EntryClass::RuntimeException]
        .families
        .values()
        .next()
        .expect("one family");
    let sample = family.sample.as_ref().expect("sample");
    assert_eq!(sample.text.lines().count(), 24);
    assert!(sample.is_truncated);
    assert_eq!(family.lines, 41, "every line is still counted");
}

#[test]
fn a_long_line_is_classified_from_its_head_and_keyed_by_the_symbol_alone() {
    let filler = "x".repeat(70_000);
    let text = format!("Could not find any RuleDef for symbol \"castle\" with {filler}\n");

    let log = player(&text);

    let tally = &log.classes[&EntryClass::BaseGenRuleMissing];
    assert_eq!(
        tally.families.keys().next().map(|key| key.as_str()),
        Some("castle")
    );
    assert_eq!(log.read_stats.lines_truncated, 1);
    assert_p1_and_p2(&log);
}

// -- runtime exceptions and back-references --------------------------------------

#[test]
fn a_runtime_exception_is_keyed_by_wrapper_exception_type_and_first_mod_frame() {
    let log = player(
        "Exception ticking Foo1 (at (1, 2, 3)): System.NullReferenceException: x\n  at Verse.Thing.Tick () in <a>:0\n  at Example.Mod.Thing.Run () in <b>:0\n\nException ticking Foo2 (at (4, 5, 6)): System.NullReferenceException: y\n  at Verse.Thing.Tick () in <a>:0\n  at Example.Mod.Thing.Run () in <b>:0\n",
    );

    let tally = &log.classes[&EntryClass::RuntimeException];
    assert_eq!(tally.families.len(), 1);
    let key = tally.families.keys().next().expect("a key");
    assert_eq!(
        key.as_str(),
        "Exception ticking|System.NullReferenceException|Example.Mod.Thing"
    );
}

fn runtime_keys(text: &str) -> Vec<String> {
    let log = player(text);
    log.classes[&EntryClass::RuntimeException]
        .families
        .keys()
        .map(|key| key.as_str().to_string())
        .collect()
}

#[test]
fn two_exception_types_at_the_same_frame_behind_a_wrapper_are_two_families() {
    let keys = runtime_keys(
        "Exception ticking Foo1 (at (1, 2, 3)): System.NullReferenceException: x\n  at Example.Mod.Thing.Run () in <b>:0\n\nException ticking Foo2 (at (1, 2, 3)): System.InvalidCastException: y\n  at Example.Mod.Thing.Run () in <b>:0\n",
    );

    assert_eq!(
        keys,
        [
            "Exception ticking|System.InvalidCastException|Example.Mod.Thing",
            "Exception ticking|System.NullReferenceException|Example.Mod.Thing",
        ]
    );
}

#[test]
fn a_head_that_opens_with_the_word_exception_but_has_no_wrapper_still_keys_on_the_inner_type() {
    let keys = runtime_keys("Exception in UIRootUpdate: System.IndexOutOfRangeException: x\n");

    assert_eq!(keys, ["|System.IndexOutOfRangeException|"]);
}

#[test]
fn two_types_in_one_namespace_are_two_families_in_the_mono_frame_format() {
    let keys = runtime_keys(
        "System.Exception: a\n  at Example.Mod.Thing.Run () in <b>:0\n\nSystem.Exception: a\n  at Example.Mod.Other.Run () in <b>:0\n",
    );

    assert_eq!(
        keys,
        [
            "|System.Exception|Example.Mod.Other",
            "|System.Exception|Example.Mod.Thing",
        ]
    );
}

#[test]
fn two_types_in_one_namespace_are_two_families_in_the_unity_frame_format() {
    let keys = runtime_keys(
        "System.Exception: a\nExample.Mod.Thing:Run ()\n\nSystem.Exception: a\nExample.Mod.Other:Run ()\n",
    );

    assert_eq!(
        keys,
        [
            "|System.Exception|Example.Mod.Other",
            "|System.Exception|Example.Mod.Thing",
        ]
    );
}

#[test]
fn a_unity_frame_in_an_engine_namespace_is_skipped_for_the_key() {
    let keys =
        runtime_keys("System.Exception: a\nVerse.Log:Error (string)\nExample.Mod.Thing:Run ()\n");

    assert_eq!(keys, ["|System.Exception|Example.Mod.Thing"]);
}

#[test]
fn a_back_reference_stub_resolves_through_the_index_to_its_originals_frame() {
    let log = player(
        "System.InvalidCastException: bad\n[Ref 1A2B3C4D]\n  at Example.Mod.Thing.Run () in <b>:0\n\nSystem.InvalidCastException: bad\n[Ref 1A2B3C4D] Duplicate stacktrace, see ref for original\n\nSystem.InvalidCastException: bad\n[Ref 1A2B3C4D] Duplicate stacktrace, see ref for original\n",
    );

    let tally = &log.classes[&EntryClass::RuntimeException];
    assert_eq!(tally.families.len(), 1);
    let (key, family) = tally.families.iter().next().expect("family");
    assert_eq!(
        key.as_str(),
        "|System.InvalidCastException|Example.Mod.Thing"
    );
    assert_eq!(family.count, 3);
    assert_p1_and_p2(&log);
}

#[test]
fn a_stub_whose_original_is_unknown_gets_its_own_family() {
    let log = player(
        "System.InvalidCastException: bad\n[Ref FFFFFFFF] Duplicate stacktrace, see ref for original\n",
    );

    let tally = &log.classes[&EntryClass::RuntimeException];
    let key = tally.families.keys().next().expect("a key");
    assert_eq!(key.as_str(), "|System.InvalidCastException|");
}

#[test]
fn originals_past_the_index_bound_are_counted_not_dropped_silently() {
    let mut text = String::new();
    for index in 0..10_005_u32 {
        text.push_str(&format!(
            "System.Exception: a\n[Ref {index:08X}]\n  at Example.Mod.Type.Run () in <b>:0\n\n"
        ));
    }

    let log = player(&text);

    assert_eq!(log.read_stats.stack_refs_dropped, 5);
    assert_p1_and_p2(&log);
}

#[test]
fn one_entry_registers_at_most_its_share_of_originals() {
    let mut text = String::from("System.Exception: a\n");
    for index in 0..70_u32 {
        text.push_str(&format!("[Ref {index:08X}]\n"));
    }
    text.push_str("  at Example.Mod.Type.Run () in <b>:0\n");

    let log = player(&text);

    assert_eq!(log.read_stats.stack_refs_dropped, 6);
    assert_p1_and_p2(&log);
}

#[test]
fn a_back_reference_id_longer_than_sixteen_hex_digits_is_not_a_back_reference() {
    let frame = "  at Example.Mod.Type.Run () in <b>:0\n";
    let sixteen = player(&format!(
        "System.Exception: a\n[Ref 0123456789ABCDEF]\n{frame}"
    ));
    let seventeen = player(&format!(
        "System.Exception: a\n[Ref 0123456789ABCDEF0]\n{frame}"
    ));

    assert_eq!(lines_of_class(&sixteen, EntryClass::RuntimeException), 3);
    assert_eq!(lines_of_class(&seventeen, EntryClass::RuntimeException), 1);
    assert_eq!(entries_of(&seventeen, EntryClass::ModMessage), 1);
}

// -- the sentinels -----------------------------------------------------------------

#[test]
fn a_known_fragment_inside_an_entry_of_another_class_is_a_leak() {
    let log = player("Some unrelated message\n  detail: Could not resolve cross-reference to X\n");

    assert_eq!(log.sentinels.total, 1);
    let leak = &log.sentinels.leaks[0];
    assert_eq!(leak.sentinel, "cross-reference");
    assert_eq!(leak.found, EntryClass::Unclassified);
    assert_eq!(leak.line, 2);
}

#[test]
fn a_reworded_engine_message_is_reported_as_a_leak() {
    let log = player("Adding duplicate Verse.ThingDef Foo\n");

    assert_eq!(log.sentinels.total, 1);
    assert_eq!(log.sentinels.leaks[0].sentinel, "duplicate-def");
}

#[test]
fn a_known_message_in_its_own_class_is_not_a_leak() {
    let log = player(
        "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)\n[Ex - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"/Defs\"): Patch operation x failed\n[End of stack trace]\n",
    );

    assert_p1_and_p2(&log);
}

#[test]
fn an_unclosed_stack_block_that_swallows_a_terse_failure_reports_a_leak() {
    let log = player(
        "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"/Defs\"): Failed\n[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed\nfile: C:\\m\\a.xml\n[Tag] after\nplain words\n",
    );

    assert!(log.totals().is_conserved());
    assert_eq!(entries_of(&log, EntryClass::PatchFailure), 0);
    assert_eq!(log.sentinels.total, 1, "{:?}", log.sentinels.leaks);
    let leak = &log.sentinels.leaks[0];
    assert_eq!(leak.sentinel, "patch-operation-failed");
    assert_eq!(leak.found, EntryClass::PatchStackTrace);
    assert_eq!(leak.line, 3);
}

#[test]
fn a_quoted_failure_in_the_body_of_a_closed_stack_block_is_not_a_leak() {
    let log = player(
        "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"/Defs\"): Patch operation x failed\n[End of stack trace]\nSource file: C:\\m\\a.xml\n",
    );

    assert_p1_and_p2(&log);
}

#[test]
fn an_indented_terse_failure_in_a_stack_block_body_is_a_leak() {
    // The allow row for quoted failures must see through the indent: an
    // indented copy of a complete terse-failure head is still a swallowed
    // failure.
    let log = player(
        "[Example Mod - Start of stack trace]\n  [Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed\n[End of stack trace]\nSource file: C:\\m\\a.xml\n",
    );

    assert_eq!(log.sentinels.total, 1, "{:?}", log.sentinels.leaks);
    let leak = &log.sentinels.leaks[0];
    assert_eq!(leak.sentinel, "patch-operation-failed");
    assert_eq!(leak.found, EntryClass::PatchStackTrace);
    assert_eq!(leak.line, 2);
}

#[test]
fn a_second_cross_reference_glued_under_the_first_is_a_leak() {
    let log = player(
        "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)\n  Could not resolve cross-reference to Verse.ThingDef named Baz (wanter=bar)\n",
    );

    assert_eq!(log.sentinels.total, 1, "{:?}", log.sentinels.leaks);
    let leak = &log.sentinels.leaks[0];
    assert_eq!(leak.sentinel, "cross-reference");
    assert_eq!(leak.found, EntryClass::CrossReference);
    assert_eq!(leak.line, 2);
}

#[test]
fn an_indented_detail_line_inside_its_own_class_is_not_a_leak() {
    // Mentions the fragment but is not a head of the class.
    let log = player(
        "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)\n  detail: see the cross-reference table\n",
    );

    assert_p1_and_p2(&log);
}

#[test]
fn a_patch_failure_text_on_the_trailer_of_a_stack_block_is_a_leak() {
    // The trailer is not the block's body, so no allow-list row covers it.
    let log = player(
        "[Example Mod - Start of stack trace]\nbody\n[End of stack trace]\nSource file: C:\\m\\Patch operation x failed.xml\n",
    );

    assert_eq!(log.sentinels.total, 1, "{:?}", log.sentinels.leaks);
    assert_eq!(log.sentinels.leaks[0].line, 4);
}

#[test]
fn the_unconfirmed_load_id_registration_shape_stays_unclassified_and_is_not_a_leak() {
    let log = player(
        "Exception registering Example.Thing in loaded object directory with unique load ID Thing_12: boom\n",
    );

    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_p1_and_p2(&log);
}

#[test]
fn a_saved_xml_node_with_a_load_id_element_is_data_not_a_leak_in_any_class() {
    let node = "<li Class=\"Thing\"><loadID>563</loadID><def>X</def></li>";
    let after_a_head = player(&format!("Subnode:\n{node}\n"));
    let inside_a_dump = player(&format!(
        "XML error: Could not find parent node named \"A\" for node \"B\". Full node: <B>\n{node}\n"
    ));

    assert_p1_and_p2(&after_a_head);
    assert_p1_and_p2(&inside_a_dump);
    assert_eq!(lines_of_class(&inside_a_dump, EntryClass::MissingParent), 2);
}

#[test]
fn an_unclassified_message_quoting_a_saved_node_and_an_orphaned_frame_are_not_leaks() {
    let log = player(
        "Could not find class Example.Thing while resolving node li. Full node: <li><loadID>7</loadID></li>\n\n  at Verse.LoadedObjectDirectory.ObjectWithLoadID[T] (System.String loadID) [0x0011b] in <a>:0\n",
    );

    assert_eq!(entries_of(&log, EntryClass::Unclassified), 2);
    assert_p1_and_p2(&log);
}

#[test]
fn a_reworded_load_id_message_that_also_quotes_a_saved_node_is_still_a_leak() {
    let log = player(
        "Could not find referenced thing with loadID Thing_1 in node <li><loadID>5</loadID></li>\n",
    );

    assert_eq!(log.sentinels.total, 1);
    assert_eq!(log.sentinels.leaks[0].sentinel, "save-load-reference");
    assert_eq!(log.sentinels.leaks[0].found, EntryClass::Unclassified);
}

#[test]
fn a_reworded_load_id_message_outside_its_class_is_a_leak() {
    let log = player("Could not find the object with load ID Thing_12 anywhere\n");

    assert_eq!(log.sentinels.total, 1);
    assert_eq!(log.sentinels.leaks[0].sentinel, "save-load-reference");
    assert_eq!(log.sentinels.leaks[0].found, EntryClass::Unclassified);
}

#[test]
fn a_plain_line_that_borrows_the_shape_of_a_unity_frame_is_a_head_and_is_checked() {
    let log = player("[Tag] hello\nhttps://example.org/x (Config error in Foo: bar)\n");

    assert_eq!(entries_of(&log, EntryClass::Unclassified), 1);
    assert_eq!(log.sentinels.total, 1);
    assert_eq!(log.sentinels.leaks[0].sentinel, "config-error");
}

#[test]
fn real_unity_frame_shapes_are_recognised_and_look_alikes_are_not() {
    use super::segment::is_unity_frame;
    for frame in [
        "Verse.Log:Error (string)",
        "UnityEngine.StackTraceUtility:ExtractStackTrace ()",
        "Example.Type:<Run>b__0 (int)",
        "Example.Type:.ctor (int)",
        "Verse.Scribe_Collections:Look<Verse.Pawn, int> (System.Collections.Generic.List`1<Verse.Pawn>&,string)",
        "Example.Type/Nested:Method|0_0 (object)",
    ] {
        assert!(is_unity_frame(frame), "{frame}");
    }
    for text in [
        "https://example.org/x (Config error in Foo: bar)",
        "Config error: something (else)",
        "note: two words (here)",
        "Example.Type:Method with spaces (x)",
    ] {
        assert!(!is_unity_frame(text), "{text}");
    }
}

#[test]
fn a_stack_frame_naming_a_parameter_is_not_checked() {
    let log = player(
        "System.Exception: boom\n  at Example.Thing.Run (System.String Config error) [0x0011b] in <a>:0\n",
    );

    assert_p1_and_p2(&log);
}

#[test]
fn the_fixture_conserves_every_line_and_only_its_own_annotations_look_like_leaks() {
    let log = player(fixture_text());

    assert!(log.totals().is_conserved());
    // The fixture's `#` annotation lines quote log shapes in prose.
    assert!(
        log.sentinels
            .leaks
            .iter()
            .all(|leak| leak.text.starts_with('#')),
        "{:?}",
        log.sentinels.leaks
    );
    assert_eq!(
        log.totals().lines_read,
        fixture_text().lines().count() as u64
    );
}

// -- size: a storm and a long tail of distinct heads ------------------------------

#[test]
fn a_two_hundred_thousand_entry_storm_is_one_family_and_the_tail_overflows() {
    let storm = b"System.InvalidCastException: Specified cast is not valid.\n[Ref ABCDEF01] Duplicate stacktrace, see ref for original\n\n".to_vec();
    let mut parts = vec![(storm, 200_000)];
    for index in 0..5_010 {
        parts.push((
            format!("distinct tail message {}\n", alpha(index)).into_bytes(),
            1,
        ));
    }

    let log = stream(GeneratedLog::new(parts)).expect("must parse");

    let storm_tally = &log.classes[&EntryClass::RuntimeException];
    assert_eq!(storm_tally.families.len(), 1);
    assert_eq!(storm_tally.entries(), 200_000);
    let tail = &log.classes[&EntryClass::Unclassified];
    assert_eq!(tail.families.len(), 5_000);
    assert_eq!(tail.overflow.entries, 10);
    assert_eq!(log.blank_separator_lines, 200_000);
    assert_p1_and_p2(&log);
}

// -- properties -------------------------------------------------------------------

const KNOWN_HEADS: &[&str] = &[
    "Initializing new game with mods:",
    "RimWorld 1.6.4871 rev591",
    "[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed",
    "Config error in Example_Def: bad",
    "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)",
    "Adding duplicate T name: Foo",
    "Mod Foo dependency (bar) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.",
    "System.NullReferenceException: x",
    "Command line arguments: -x",
    "[Some Mod] hello",
    "plain unclassified words",
];

fn line_strategy() -> impl Strategy<Value = Vec<u8>> {
    let known = proptest::sample::select(KNOWN_HEADS).prop_map(|head| head.as_bytes().to_vec());
    let shaped = proptest::sample::select(vec![
        "  at Example.Type.Method () in <a>:0",
        "(wrapper managed-to-native) X:Y ()",
        "--- End of inner exception stack trace ---",
        "    - PREFIX detail",
        "Unity.Type:Method (object)",
        "[Example Mod - Start of stack trace]",
        "[End of stack trace]",
        "Source file: C:\\m\\a.xml",
        "file: C:\\m\\a.xml",
        "DDS loading failed for 'a.dds': x",
        "Loading from png instead.",
        "Loading game from file S with mods:",
        "  - example.mod",
        "Memory Statistics:",
        "  [ALLOC_TEMP_MAIN]",
        "Crash!!!",
        "XML error: Could not find parent node named \"A\" for node \"B\"",
        "<li>x</li>",
        "[Ref 0000ABCD]",
        "[Ref 0000ABCD] Duplicate stacktrace, see ref for original",
        "No stack trace.",
        "UnityEngine.StackTraceUtility:ExtractStackTrace ()",
        "",
        "   ",
    ])
    .prop_map(|line| line.as_bytes().to_vec());
    let noisy = prop_oneof![
        Just(b"has a lone \r carriage return".to_vec()),
        Just(vec![b'b', 0xFF, b'a', b'd']),
        Just(vec![b'x'; 70_000]),
        "[ -~]{0,40}".prop_map(String::into_bytes),
    ];
    prop_oneof![4 => known, 6 => shaped, 2 => noisy]
}

fn render(lines: &[Vec<u8>], line_ending: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line);
        bytes.extend_from_slice(line_ending);
    }
    bytes
}

fn parse_bytes(bytes: &[u8], framing: Framing, buffer_bytes: usize) -> ParsedGameLog {
    super::parse_stream(
        BufReader::with_capacity(buffer_bytes, bytes),
        &formats(),
        framing,
    )
    .expect("an in-memory read cannot fail")
}

proptest! {
    #[test]
    fn every_line_is_accounted_for_under_both_framings(
        lines in proptest::collection::vec(line_strategy(), 0..60),
        use_crlf in any::<bool>(),
    ) {
        let bytes = render(&lines, if use_crlf { b"\r\n" } else { b"\n" });
        for framing in [Framing::PlayerLog, Framing::ConsoleCopy] {
            let log = parse_bytes(&bytes, framing, 8 * 1024);
            prop_assert!(log.totals().is_conserved(), "{:?}", log.totals());
            prop_assert_eq!(log.totals().lines_read, lines.len() as u64);
        }
    }

    #[test]
    fn parsing_twice_and_in_any_read_chunking_gives_the_same_result(
        lines in proptest::collection::vec(line_strategy(), 0..40),
        chunk in 1_usize..64,
    ) {
        let bytes = render(&lines, b"\n");
        for framing in [Framing::PlayerLog, Framing::ConsoleCopy] {
            let whole = parse_bytes(&bytes, framing, 256 * 1024);
            prop_assert_eq!(&whole, &parse_bytes(&bytes, framing, 256 * 1024));
            prop_assert_eq!(&whole, &parse_bytes(&bytes, framing, chunk));
        }
    }

    #[test]
    fn a_known_head_between_blank_lines_lands_in_its_class(
        before in proptest::collection::vec(line_strategy(), 0..12),
        which in 0..KNOWN_HEADS.len(),
    ) {
        let head = KNOWN_HEADS[which];
        let mut prefix = before;
        prefix.push(Vec::new());
        let mut full = prefix.clone();
        full.push(head.as_bytes().to_vec());
        full.push(Vec::new());

        let without = parse_bytes(&render(&prefix, b"\n"), Framing::PlayerLog, 8 * 1024);
        let with = parse_bytes(&render(&full, b"\n"), Framing::PlayerLog, 8 * 1024);

        let expected = class_of(head);
        prop_assert_eq!(
            entries_of(&with, expected),
            entries_of(&without, expected) + 1,
            "{} not added to {:?}",
            head,
            expected
        );
    }

    #[test]
    fn every_family_counts_agree_with_its_independent_counters(
        lines in proptest::collection::vec(line_strategy(), 0..60),
    ) {
        let bytes = render(&lines, b"\n");
        for framing in [Framing::PlayerLog, Framing::ConsoleCopy] {
            let log = parse_bytes(&bytes, framing, 8 * 1024);
            let mut entries_by_pass_counters = 0_u64;
            for tally in log.classes.values() {
                let mut class_entries = tally.overflow.entries;
                for family in tally.families.values() {
                    let by_pass: u64 = family.count_by_pass.values().sum();
                    prop_assert_eq!(by_pass, family.count);
                    prop_assert!(family.lines >= family.count);
                    prop_assert!(family.first_line <= family.last_line);
                    // Heads sit on distinct lines, so a family seen twice or
                    // more must have moved its last line past its first.
                    if family.count > 1 {
                        prop_assert!(family.last_line > family.first_line);
                    }
                    class_entries += by_pass;
                }
                prop_assert_eq!(class_entries, tally.entries());
                entries_by_pass_counters += class_entries;
            }
            prop_assert_eq!(entries_by_pass_counters, log.totals().entries);
        }
    }
}

// -- the mod-produced formats are data -------------------------------------

/// One line of each mod-produced format (a stack block with its trailer, a
/// texture fallback pair, a back-reference stub), after an engine banner.
const MOD_FORMAT_LINES: &str = "RimWorld 1.6.0 rev1\n\
[Example Mod - Start of stack trace]\n\
Verse.PatchOperationAdd(xpath=\"/Defs\"): Failed to find\n\
[End of stack trace]\n\
Source file: C:\\mods\\a.xml\n\
DDS loading failed for 'Things/a': Cannot load compressed texture with non multiple of 4 dimensions of 30x20 and format DXT5\n\
Loading from png instead.\n\
[Ref 1A2B] Duplicate stacktrace, see ref for original\n";

#[test]
fn with_the_example_shapes_the_three_formats_are_recognised() {
    let log = player(MOD_FORMAT_LINES);

    assert_p1_and_p2(&log);
    assert_eq!(entries_of(&log, EntryClass::PatchStackTrace), 1);
    assert_eq!(entries_of(&log, EntryClass::TextureFallback), 1);
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(log.dds_failures.len(), 1);
}

/// Property P1 and P2 with no shape at all: the format lines still land
/// somewhere, every line is accounted for, and nothing crashes. The block's
/// start, end and the stub are `[Tag]` messages, the rest is unclassified,
/// and no typed record is built from them. The sentinels for these formats
/// are derived from the loaded shapes, so none exists here and none can
/// leak: with no known format there is nothing a known-class sentinel
/// could say was missed.
#[test]
fn an_empty_set_of_shapes_still_conserves_every_line_and_reports_no_leak() {
    let log = parse(MOD_FORMAT_LINES, &LogFormats::empty());

    assert_p1_and_p2(&log);
    assert_eq!(log.read_stats.lines_read, 8);
    assert_eq!(entries_of(&log, EntryClass::PatchStackTrace), 0);
    assert_eq!(entries_of(&log, EntryClass::TextureFallback), 0);
    assert_eq!(entries_of(&log, EntryClass::ModMessage), 3);
    assert_eq!(entries_of(&log, EntryClass::Unclassified), 4);
    assert!(log.extra_stack_traces.is_empty());
    assert!(log.dds_failures.is_empty());
    assert!(log.sentinels.leaks.is_empty());
}

#[test]
fn an_empty_set_of_shapes_conserves_a_console_copy_too() {
    let text = "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(): Failed\n\
                [End of stack trace]\nSource file: C:\\mods\\a.xml\nNo stack trace.\n\n";

    let log = parse_as(text, &LogFormats::empty(), Framing::ConsoleCopy);

    assert_p1_and_p2(&log);
}

fn dds_line(path: &str) -> String {
    format!(
        "RimWorld 1.6.0 rev1\nDDS loading failed for '{path}': Cannot load compressed \
         texture with non multiple of 4 dimensions of 30x20 and format DXT5\n\
         Loading from png instead.\n"
    )
}

/// The dimensions template's path is `.+`, where the old pattern's was
/// `[^']*`: a path holding an apostrophe is now captured instead of the
/// record being dropped.
#[test]
fn a_texture_path_with_an_apostrophe_is_captured() {
    let log = player(&dds_line("Things/Bob's sword"));

    assert_p1_and_p2(&log);
    assert_eq!(log.dds_failures.len(), 1);
    assert_eq!(log.dds_failures[0].path, "Things/Bob's sword");
    assert_eq!(entries_of(&log, EntryClass::TextureFallback), 1);
}

/// An empty path matches neither the head nor the dimensions template (both
/// need a path), exactly as it matched no head before: the line is
/// unclassified and the texture sentinel reports it as a leak, so it is
/// visible rather than lost.
#[test]
fn a_texture_line_with_an_empty_path_is_a_visible_leak_not_a_record() {
    let log = player(&dds_line(""));

    assert!(log.totals().is_conserved());
    assert!(log.dds_failures.is_empty());
    assert_eq!(entries_of(&log, EntryClass::TextureFallback), 0);
    assert_eq!(log.sentinels.total, 1, "{:?}", log.sentinels.leaks);
    assert_eq!(log.sentinels.leaks[0].sentinel, "texture-fallback");
}

// -- attribution inputs: what a family's first entry says about its mod --------------

/// The inputs of every family of `class`, in key order.
fn inputs_of(log: &ParsedGameLog, class: EntryClass) -> Vec<Option<AttributionInput>> {
    log.classes.get(&class).map_or_else(Vec::new, |tally| {
        tally
            .families
            .values()
            .map(|family| family.attribution_input.clone())
            .collect()
    })
}

/// The input of the one family of `class`.
fn only_input(log: &ParsedGameLog, class: EntryClass) -> Option<AttributionInput> {
    let mut inputs = inputs_of(log, class);
    assert_eq!(inputs.len(), 1, "expected one {class:?} family");
    inputs.remove(0)
}

fn blamed_type(name: &str) -> Option<AttributionInput> {
    AttributionInput::type_name(name)
}

/// The family keys of `class`, in key order.
fn keys_of(log: &ParsedGameLog, class: EntryClass) -> Vec<String> {
    log.classes.get(&class).map_or_else(Vec::new, |tally| {
        tally
            .families
            .keys()
            .map(|key| key.as_str().to_string())
            .collect()
    })
}

/// The `|`-joined key part naming the frame, of the one runtime-exception
/// family of `text` (`wrapper|exception|frame`).
fn key_frame_of(text: &str) -> String {
    let log = player(text);
    let keys = keys_of(&log, EntryClass::RuntimeException);
    assert_eq!(keys.len(), 1, "{keys:?}");
    keys[0].rsplit('|').next().unwrap_or_default().to_string()
}

#[test]
fn a_patch_failure_family_carries_the_tag_and_file_of_its_first_entry() {
    let log = player(
        "[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\nfile: C:\\m\\a.xml\n[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\nfile: C:\\m\\b.xml\n",
    );

    assert_eq!(entries_of(&log, EntryClass::PatchFailure), 2);
    assert_eq!(
        only_input(&log, EntryClass::PatchFailure),
        Some(AttributionInput::display_name_and_path(
            "Example Mod",
            "C:\\m\\a.xml"
        )),
        "one family, attributed once, from its first entry"
    );
}

#[test]
fn a_patch_failure_with_no_file_line_carries_its_tag_alone() {
    let log = player("[Example Mod] Patch operation Verse.PatchOperationAdd(x) failed\n");

    assert_eq!(
        only_input(&log, EntryClass::PatchFailure),
        Some(AttributionInput::display_name("Example Mod"))
    );
}

#[test]
fn a_stack_block_family_carries_its_start_tag_and_trailer_path() {
    let log = player(
        "[Example Mod - Start of stack trace]\nVerse.PatchOperationAdd(xpath=\"a\"): Failed to find a node with the given xpath\n[End of stack trace]\nSource file: C:\\m\\a.xml\n",
    );

    assert_eq!(
        only_input(&log, EntryClass::PatchStackTrace),
        Some(AttributionInput::display_name_and_path(
            "Example Mod",
            "C:\\m\\a.xml"
        ))
    );
}

#[test]
fn a_texture_fallback_family_carries_its_path_whatever_the_reason() {
    let log = player("DDS loading failed for 'C:/m/a.dds': some other reason\n");

    assert_eq!(
        only_input(&log, EntryClass::TextureFallback),
        Some(AttributionInput::path("C:/m/a.dds"))
    );
}

#[test]
fn a_tag_line_family_carries_its_balanced_tag_and_an_untagged_line_carries_none() {
    let log = player("[Example Mod [Adopted]] hello\n[Tag] one\n");
    let unclassified = player("no tag here\n");

    assert_eq!(
        inputs_of(&log, EntryClass::ModMessage),
        [
            Some(AttributionInput::display_name("Example Mod [Adopted]")),
            Some(AttributionInput::display_name("Tag")),
        ]
    );
    assert_eq!(only_input(&unclassified, EntryClass::Unclassified), None);
}

#[test]
fn a_metadata_warning_family_carries_the_mod_it_names() {
    let log = player(
        "Mod Example Mod dependency (a.b) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.\n",
    );

    assert_eq!(
        only_input(&log, EntryClass::ModMetadataWarning),
        Some(AttributionInput::display_name("Example Mod"))
    );
}

#[test]
fn a_cross_reference_and_the_engines_own_messages_carry_no_input() {
    let log = player(
        "Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)\nAdding duplicate Verse.ThingDef name: Foo\nCould not load UnityEngine.Texture2D at 'Things/X' in any active mod or in base resources.\nRimWorld 1.6.4871 rev591\n",
    );

    for class in [
        EntryClass::CrossReference,
        EntryClass::DuplicateDef,
        EntryClass::TextureLoadFailure,
        EntryClass::EngineInfo(EngineInfoKind::Banner),
    ] {
        assert_eq!(only_input(&log, class), None, "{class:?}");
    }
}

#[test]
fn an_exception_family_carries_only_its_innermost_non_engine_frame_type() {
    let log = player(
        "System.Exception: a\n  at Verse.Thing.Tick () in <b>:0\n  at Example.Mod.Thing.Run () in <b>:0\n  at Example.Mod.Thing.Other () in <b>:0\n  at Example.Lib.Helper.Do () in <b>:0\n  at RimWorld.Foo.Bar () in <b>:0\n",
    );

    assert_eq!(
        only_input(&log, EntryClass::RuntimeException),
        blamed_type("Example.Mod.Thing"),
        "engine frames are skipped and only the innermost mod type is kept"
    );
}

#[test]
fn an_outer_frame_never_stands_in_for_the_innermost_type() {
    let log = player(
        "System.Exception: a\n  at Unknown.Ns.Thing.Run () in <b>:0\n  at Example.Lib.Helper.Do () in <b>:0\n",
    );

    assert_eq!(
        only_input(&log, EntryClass::RuntimeException),
        blamed_type("Unknown.Ns.Thing"),
        "the caller's type is not evidence about the thrower"
    );
}

#[test]
fn an_exception_family_with_engine_frames_only_carries_no_input() {
    let log = player(
        "System.Exception: a\n  at Verse.Thing.Tick () in <b>:0\n  at System.Foo.Bar () in <b>:0\n",
    );

    assert_eq!(only_input(&log, EntryClass::RuntimeException), None);
}

#[test]
fn a_frame_in_the_unity_format_is_a_type_name_too() {
    let log = player("System.Exception: a\nExample.Mod.Thing:Run ()\n");

    assert_eq!(
        only_input(&log, EntryClass::RuntimeException),
        blamed_type("Example.Mod.Thing")
    );
}

#[test]
fn a_mono_frame_names_its_real_type_not_its_namespace() {
    let cases = [
        (
            "  at Example.Ext+<Run>d__1.MoveNext () in <b>:0",
            "Example.Ext",
        ),
        (
            "  at Example.Job+<>c.<Drop>b__0_0 (Verse.Thing t) in <b>:0",
            "Example.Job",
        ),
        ("  at Example.Main..cctor () in <b>:0", "Example.Main"),
        (
            "  at Example.Main..ctor (System.Int32 x) in <b>:0",
            "Example.Main",
        ),
        (
            "  at Example.Algo.BFS`1[T].FloodFill (T start) in <b>:0",
            "Example.Algo.BFS`1",
        ),
        (
            "  at Example.Algo.Search.Run[T] (T start) in <b>:0",
            "Example.Algo.Search",
        ),
        (
            "  at Example.Deep.Type.Method () in <b>:0",
            "Example.Deep.Type",
        ),
    ];
    for (frame, expected) in cases {
        let text = format!("System.Exception: a\n{frame}\n");
        assert_eq!(key_frame_of(&text), expected, "key of {frame}");
        assert_eq!(
            only_input(&player(&text), EntryClass::RuntimeException),
            blamed_type(expected),
            "attribution of {frame}"
        );
    }
}

#[test]
fn a_unity_frame_names_its_outer_type_like_a_mono_one() {
    let cases = [
        ("Example.Job+<>c:<Drop>b__0_0 (Verse.Thing)", "Example.Job"),
        ("Example.Ext/Nested:Run ()", "Example.Ext"),
        ("Example.Main:.ctor ()", "Example.Main"),
    ];
    for (frame, expected) in cases {
        let text = format!("System.Exception: a\n{frame}\n");
        assert_eq!(key_frame_of(&text), expected, "key of {frame}");
    }
}

#[test]
fn a_namespace_that_merely_starts_with_an_engine_root_is_not_the_engine() {
    let cases = [
        (
            "  at RimWorldColumns.Panel.Draw () in <b>:0",
            "RimWorldColumns.Panel",
        ),
        ("  at UnityExplorer.Ui.Show () in <b>:0", "UnityExplorer.Ui"),
        ("  at Systematic.Core.Run () in <b>:0", "Systematic.Core"),
    ];
    for (frame, expected) in cases {
        let text = format!("System.Exception: a\n{frame}\n");
        assert_eq!(key_frame_of(&text), expected, "{frame}");
    }
    for engine in [
        "System.Foo.Bar",
        "Mono.Cecil.Reader.Read",
        "UnityEngine.Object.Destroy",
        "Unity.Collections.NativeArray.Dispose",
        "Verse.Thing.Tick",
        "RimWorld.Planet.World.Tick",
        "LudeonTK.Debug.Show",
        "HarmonyLib.Patch.Apply",
        "MonoMod.Utils.Dyn.Run",
    ] {
        let text = format!("System.Exception: a\n  at {engine} () in <b>:0\n");
        assert_eq!(key_frame_of(&text), "", "{engine} is the engine's");
    }
}

#[test]
fn every_analyzer_engine_root_is_an_engine_root_here_too() {
    for root in rim_analyzer::analysis::edges::ENGINE_NAMESPACE_ROOTS {
        assert!(
            super::patterns::ENGINE_NAMESPACE_ROOTS
                .iter()
                .any(|own| own.eq_ignore_ascii_case(root)),
            "{root} is an ownership-excluded root in the analyzer but not a skipped frame here"
        );
    }
}

#[test]
fn a_type_load_error_family_carries_the_type_or_assembly_its_head_names() {
    let log = player(
        "Error in static constructor of Example.Main: System.TypeInitializationException: x\n  at Verse.Foo.Bar () in <b>:0\nException loading Example.Lib.dll: System.Exception: y\nError initializing mod: z\n",
    );

    let inputs = inputs_of(&log, EntryClass::TypeLoadError);

    assert_eq!(inputs.len(), 3);
    assert!(inputs.contains(&blamed_type("Example.Main")));
    assert!(inputs.contains(&AttributionInput::assembly("Example.Lib")));
    assert!(
        inputs.contains(&None),
        "a head naming neither has no evidence"
    );
}

#[test]
fn a_stub_family_carries_the_frame_its_original_showed() {
    let log = player(
        "System.InvalidCastException: bad\n[Ref 1A2B3C4D]\n  at Example.Mod.Thing.Run () in <b>:0\n\nSystem.InvalidCastException: bad\n[Ref 1A2B3C4D] Duplicate stacktrace, see ref for original\n\nSystem.InvalidCastException: bad\n[Ref FFFFFFFF] Duplicate stacktrace, see ref for original\n",
    );

    let inputs = inputs_of(&log, EntryClass::RuntimeException);

    assert_eq!(
        inputs,
        [None, blamed_type("Example.Mod.Thing")],
        "the unknown stub has no frame to attribute, the known one shares its original's family"
    );
}
