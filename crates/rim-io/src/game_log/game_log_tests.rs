//! `game_log`'s own tests, over the facade and its `extract` grammar.

use std::io::Read;
use std::sync::LazyLock;

use rim_session::ports::{
    LoadEventKind, LogShapes, RawCrossReference, RawLoadEvent, RawPatchFailure,
};
use tempfile::tempdir;

use super::extract::{MAX_BLOCK_LINES, MAX_JOIN_LINES};
use super::*;

fn fixture_text() -> &'static str {
    include_str!("../../tests/fixtures/player_log_excerpt.log")
}

/// The carrier list these tests parse with. An invented carrier, not
/// the embedded bundle's real one — `player_log_excerpt.log`'s own
/// `EXAMPLECACHE:` lines are synthetic for exactly this reason, so
/// this crate's own tests never depend on what the `rules` repo
/// happens to ship. The real carrier's own shape is pinned
/// separately, by `mod_knowledge.rs`'s own contract test.
fn test_carriers() -> Vec<DefCacheCarrier> {
    rim_session::test_support::example_carriers()
}

static TEST_CARRIERS: LazyLock<Vec<DefCacheCarrier>> = LazyLock::new(test_carriers);
static TEST_SHAPES: LazyLock<LogShapes> =
    LazyLock::new(rim_session::test_support::example_log_shapes);

/// The formats these tests parse with: the invented carrier and the
/// example mod-produced formats.
pub(super) fn test_formats() -> LogFormats<'static> {
    LogFormats {
        def_cache_carriers: &TEST_CARRIERS,
        shapes: &TEST_SHAPES,
    }
}

/// The example formats with no def-cache carrier.
pub(super) fn shapes_only() -> LogFormats<'static> {
    LogFormats {
        def_cache_carriers: &[],
        shapes: &TEST_SHAPES,
    }
}

fn find_failure<'a>(log: &'a ParsedGameLog, mod_tag: &str) -> &'a RawPatchFailure {
    log.patch_failures
        .iter()
        .find(|f| f.mod_tag == mod_tag)
        .unwrap_or_else(|| panic!("no patch failure tagged [{mod_tag}]"))
}

#[test]
fn a_terse_failure_pairs_with_its_own_file_line() {
    let log = parse(fixture_text(), &test_formats());
    let failure = find_failure(&log, "Example Progression: Warfare");
    assert_eq!(
        failure.operation,
        r#"Verse.PatchOperationReplace(Defs/ResearchProjectDef[defName="EX_Longbow"]/description)"#
    );
    assert_eq!(
        failure.source_file.as_deref(),
        Some(
            r"C:\Game\Workshop\294100\3000000003\Extra Mods\Example Weapons\Patches\example weapons research patches.xml"
        )
    );
}

#[test]
fn a_stack_trace_block_attaches_to_its_own_terse_failure_with_leaf_and_chain() {
    let log = parse(fixture_text(), &test_formats());
    // Positionally the 3rd terse failure/3rd block in the fixture:
    // leaf `PatchOperationAdd` (ExampleFanS), enclosing
    // `PatchOperationConditional` (the top-level, `<nomatch>`).
    let failure = log
        .patch_failures
        .iter()
        .find(|f| f.operation.contains("ExampleFanS") && f.operation.contains("Conditional"))
        .expect("the Conditional(ExampleFanS) terse failure must be present");
    let stack = failure
        .stack_trace
        .as_ref()
        .expect("this failure must have a paired stack-trace block");
    assert_eq!(stack.ops.len(), 2, "leaf + one enclosing op");

    let leaf = &stack.ops[0];
    assert_eq!(leaf.class, "Verse.PatchOperationAdd");
    assert_eq!(
        leaf.xpath.as_deref(),
        Some(r#"/Defs/ThingDef[defName="ExampleFanS"]"#)
    );
    assert_eq!(leaf.reason, "Failed to find a node with the given xpath");
    assert_eq!(leaf.branch, None);

    let top_level = &stack.ops[1];
    assert_eq!(top_level.class, "Verse.PatchOperationConditional");
    assert_eq!(
        top_level.xpath.as_deref(),
        Some(r#"/Defs/ThingDef[defName="ExampleFanS"]/researchPrerequisites"#)
    );
    assert_eq!(top_level.branch.as_deref(), Some("nomatch"));
    assert_eq!(
        stack.source_file.as_deref(),
        Some(
            r"C:\Game\Workshop\294100\3000000002\Extra Mods\Example Hygiene\Patches\example hygiene patch.xml"
        )
    );
}

#[test]
fn a_single_line_block_has_the_leaf_as_its_own_top_level() {
    let log = parse(fixture_text(), &test_formats());
    let failure = find_failure(&log, "Example Biomes");
    let stack = failure.stack_trace.as_ref().expect("must have a block");
    assert_eq!(stack.ops.len(), 1, "no enclosing chain at all");
    assert_eq!(stack.ops[0].class, "Verse.PatchOperationAdd");
}

#[test]
fn a_multi_line_xpath_inside_a_stack_trace_block_still_parses_the_leaf_correctly() {
    // A real log's shape, verbatim: the `xpath="..."` value is split
    // across two physical lines, tabs and all. Neither line matches
    // `STACK_OP_LINE_RE` alone; without the join, both would be
    // silently dropped and `ops[0]` would become the *enclosing*
    // `PatchOperationSequence` instead of the real leaf.
    let text = "[Example Compat Collection - Start of stack trace]\r\n\
Verse.PatchOperationRemove(xpath=\"\r\n\
\t\t\t\t\t\t\tDefs/XenotypeDef[defName=\"EX_Blank\"]/genes/li[text()=\"EX_DisabledAllWork_Blank\"]\"): Failed to find a node with the given xpath\r\n\
Verse.PatchOperationSequence: Error in the operation at position=4\r\n\
Verse.PatchOperationFindMod(Example Races Expanded - Copperbark): Error in <match>\r\n\
Verse.PatchOperationFindMod(Example - Xenotypes and Genes): Error in <match>\r\n\
[End of stack trace]\r\n\
Source file: Q:\\Steam\\steamapps\\workshop\\content\\294100\\3000000019\\1.6\\Patches\\ExampleXenotypesAndGenes.xml\r\n";

    let log = parse(text, &test_formats());
    assert_eq!(log.extra_stack_traces.len(), 1);
    let block = &log.extra_stack_traces[0];
    assert_eq!(block.ops.len(), 4, "leaf + 3 enclosing ops, not 3");

    let leaf = &block.ops[0];
    assert_eq!(leaf.class, "Verse.PatchOperationRemove");
    assert_eq!(
        leaf.xpath.as_deref(),
        Some(r#"Defs/XenotypeDef[defName="EX_Blank"]/genes/li[text()="EX_DisabledAllWork_Blank"]"#),
        "no leading whitespace from the line join"
    );
    assert_eq!(leaf.reason, "Failed to find a node with the given xpath");
    assert_eq!(leaf.branch, None);

    // The two `Error in the operation at position=4`/`<match>` ops
    // must survive as their own, distinct entries — not merged into
    // the leaf or dropped.
    assert_eq!(block.ops[1].class, "Verse.PatchOperationSequence");
    assert_eq!(block.ops[1].reason, "Error in the operation at position=4");
    assert_eq!(block.ops[3].class, "Verse.PatchOperationFindMod");
    assert_eq!(
        block.ops[3].detail.as_deref(),
        Some("Example - Xenotypes and Genes")
    );
}

#[test]
fn the_enclosing_chain_carries_a_non_branch_reason_untouched() {
    // The ExampleSplitCooler block's middle op
    // (`Verse.PatchOperationSequence: Error in the operation at
    // position=1`) has neither a detail nor a `<match>`/`<nomatch>`
    // branch — the parser must not invent either.
    let log = parse(fixture_text(), &test_formats());
    let failure = log
        .patch_failures
        .iter()
        .find(|f| f.operation.contains("Example Temperature Expanded"))
        .expect("the FindMod terse failure must be present");
    let stack = failure.stack_trace.as_ref().expect("must have a block");
    assert_eq!(stack.ops.len(), 3);
    let sequence = &stack.ops[1];
    assert_eq!(sequence.class, "Verse.PatchOperationSequence");
    assert_eq!(sequence.detail, None);
    assert_eq!(sequence.xpath, None);
    assert_eq!(sequence.branch, None);
    assert_eq!(sequence.reason, "Error in the operation at position=1");

    let top_level = &stack.ops[2];
    assert_eq!(top_level.class, "Verse.PatchOperationFindMod");
    assert_eq!(
        top_level.detail.as_deref(),
        Some("Example Temperature Expanded")
    );
    assert_eq!(
        top_level.xpath, None,
        "a FindMod's detail is a mod name, not an xpath"
    );
    assert_eq!(top_level.branch.as_deref(), Some("match"));
}

#[test]
fn a_bracketed_display_name_containing_its_own_brackets_is_not_dropped() {
    let log = parse(fixture_text(), &test_formats());
    let matches: Vec<_> = log
        .patch_failures
        .iter()
        .filter(|f| f.mod_tag == "Example Badge Fork [Adopted]")
        .collect();
    assert_eq!(matches.len(), 2, "both of this mod's failures must parse");
    assert!(matches[0].operation.starts_with("Verse.PatchOperationAdd"));
}

#[test]
fn both_cross_reference_shapes_parse() {
    let log = parse(fixture_text(), &test_formats());
    assert_eq!(log.cross_references.len(), 4);

    let wanting_defs: Vec<_> = log
        .cross_references
        .iter()
        .filter_map(|c| match c {
            RawCrossReference::WantingDef {
                missing_type,
                missing_name,
                wanting_def,
                note,
            } => Some((missing_type, missing_name, wanting_def, note)),
            RawCrossReference::Wanter { .. } => None,
        })
        .collect();
    assert_eq!(wanting_defs.len(), 2);
    let (missing_type, missing_name, wanting_def, note) = wanting_defs[0];
    assert_eq!(missing_type, "Verse.SoundDef");
    assert_eq!(missing_name, "MeleeHit_ExamplePunch");
    assert_eq!(wanting_def, "Verse.Tool fist");
    assert_eq!(note.as_deref(), Some("using undefined sound instead"));

    let wanters: Vec<_> = log
        .cross_references
        .iter()
        .filter_map(|c| match c {
            RawCrossReference::Wanter {
                missing_type,
                missing_name,
                wanter_field,
            } => Some((missing_type, missing_name, wanter_field)),
            RawCrossReference::WantingDef { .. } => None,
        })
        .collect();
    assert_eq!(wanters.len(), 2);
    assert_eq!(wanters[0].0, "Verse.ThingDef");
    assert_eq!(wanters[0].1, "EX_ResearchSpot");
    assert_eq!(wanters[0].2, "requiredBuildings");
}

#[test]
fn a_dds_failure_line_parses_path_dimensions_and_format() {
    let log = parse(fixture_text(), &test_formats());
    assert_eq!(log.dds_failures.len(), 2);
    let first = &log.dds_failures[0];
    assert_eq!(
        first.path,
        r"C:\Game\Workshop\294100\3000000005\Textures\ExampleDog.dds"
    );
    assert_eq!(first.width, 1470);
    assert_eq!(first.height, 1961);
    assert_eq!(first.format, "DXT1");
}

#[test]
fn dependency_without_url_warnings_parse() {
    let log = parse(fixture_text(), &test_formats());
    assert_eq!(log.dependency_warnings.len(), 2);
    assert_eq!(log.dependency_warnings[0].mod_name, "Fixture Addon");
    assert_eq!(
        log.dependency_warnings[0].dependency_id,
        "example.framework"
    );
}

#[test]
fn a_tagged_timer_strips_its_bracket_into_the_label() {
    let log = parse(fixture_text(), &test_formats());
    let timer = log
        .timers
        .iter()
        .find(|t| t.label.starts_with("[ExampleXenoPatch]"))
        .expect("the ExampleXenoPatch timer must parse");
    assert_eq!(timer.label, "[ExampleXenoPatch] Prep work / pre-caching");
    assert_eq!(timer.milliseconds, 51_734);
}

#[test]
fn an_untagged_timer_keeps_its_free_text_label() {
    let log = parse(fixture_text(), &test_formats());
    let timer = log
        .timers
        .iter()
        .find(|t| t.label == "EarlyLoader: Starting...")
        .expect("the vanilla-load timer must parse");
    assert_eq!(timer.milliseconds, 9_327);
}

#[test]
fn every_known_timer_rendering_is_recognized() {
    let log = parse(fixture_text(), &test_formats());
    // The fixture holds 8 matching *lines*: the `(took Ns[; ...])`
    // shape appears twice (`[ExampleXenoPatch]`'s line and the
    // untagged "Merged duplicate example recipes..." one), plus the
    // bare `took Nms` shape (`EarlyLoader: Game processing took
    // 2252.5076ms`) (see `TIMER_MS_RE`'s own doc comment and the
    // dedicated `a_bare_took_nms_rendering_is_recognized...` test
    // below).
    //
    // The fixture's own `#`-prefixed annotations describe the timer
    // renderings rather than quoting them verbatim: a quoted timer
    // would be a byte-for-byte real match for these regexes and
    // contradict the fixture header's claim that `#` lines match no
    // log shape. The parser needs no `#`-skip rule, since a real
    // `Player.log` contains no `#`-prefixed line at all.
    assert_eq!(log.timers.len(), 8);
    let ms_rendering = log
        .timers
        .iter()
        .find(|t| t.label.contains("Example Extensions"))
        .expect("the `in Nms, N failed` rendering must parse");
    assert_eq!(ms_rendering.milliseconds, 472_940);
    let color_wrapped = log
        .timers
        .iter()
        .find(|t| t.milliseconds == 8_000)
        .expect("the color-tag-wrapped `took N seconds` rendering must parse");
    assert_eq!(color_wrapped.milliseconds, 8_000);
    assert!(
        !color_wrapped.label.contains("<color"),
        "color tags must be stripped from the label"
    );
}

#[test]
fn a_bare_took_nms_rendering_is_recognized_should_fix_5() {
    // The bare `took Nms`/`took N ms` rendering with no leading `in`
    // at all, using real log lines.
    let text = "EarlyLoader: Game processing took 2252.5076ms\n\
EarlyLoader: Serializing took 2501.3184ms\n\
  Startup init took 61ms (Runtime patching 26ms, WeaponRegistry 19ms)\n\
  Startup init took 105ms (Runtime patching 19ms, TraitStatMutability 0ms)\n\
<color=#66ffb5>[ExampleMeleeAnim]</color> Found 46 items of clothing that cover hands, took 3.70 ms:\n";

    let log = parse(text, &test_formats());
    assert_eq!(log.timers.len(), 5, "all 5 real lines must be recognized");
    assert_eq!(log.timers[0].label, "EarlyLoader: Game processing");
    assert_eq!(log.timers[0].milliseconds, 2_253);
    assert_eq!(log.timers[1].label, "EarlyLoader: Serializing");
    assert_eq!(log.timers[1].milliseconds, 2_501);
    assert_eq!(log.timers[2].label, "Startup init");
    assert_eq!(log.timers[2].milliseconds, 61);
    // The inner `Runtime patching 26ms` fragments (no leading
    // `in`/`took` of their own) must not each spawn their own timer.
    assert_eq!(log.timers[3].label, "Startup init");
    assert_eq!(log.timers[3].milliseconds, 105);
    assert_eq!(log.timers[4].milliseconds, 4);
    assert!(
        !log.timers[4].label.contains("<color"),
        "color tags must be stripped even for this rendering"
    );
}

#[test]
fn def_cache_lines_are_surfaced_verbatim() {
    let log = parse(fixture_text(), &test_formats());
    assert!(
        log.def_cache_lines
            .iter()
            .any(|l| l.contains("Cache not found or got purged"))
    );
    assert!(
        log.def_cache_lines
            .iter()
            .any(|l| l.contains("Cache created"))
    );
}

#[test]
fn the_new_game_block_parses_in_order_with_suffix_stripped() {
    let log = parse(fixture_text(), &test_formats());
    assert_eq!(log.load_events.len(), 1);
    let event = &log.load_events[0];
    assert_eq!(event.kind, LoadEventKind::NewGame);
    assert_eq!(
        event.mods,
        vec![
            "Example.Mod",
            "example.earlyloader",
            "example.patchlib",
            "example.logexceptions",
            "Ludeon.RimWorld",
            "Ludeon.RimWorld.Royalty",
            "Ludeon.RimWorld.Ideology",
            "Ludeon.RimWorld.Biotech",
            "Ludeon.RimWorld.Anomaly",
            "Ludeon.RimWorld.Odyssey",
            "example.xmlframework",
        ]
    );
}

fn parse_plain(content: &str) -> ParsedGameLog {
    parse(content, &shapes_only())
}

#[test]
fn a_save_load_block_parses_with_its_save_name_line_and_suffix_stripped() {
    let log = parse_plain(
        "noise
Loading game from file My Save 2 with mods:
  - example.first
               - example.second (incompatible version)
  - Ludeon.RimWorld
next line
",
    );

    assert_eq!(
        log.load_events,
        vec![RawLoadEvent {
            line: 2,
            kind: LoadEventKind::SaveLoad {
                save_name: "My Save 2".to_string()
            },
            mods: vec![
                "example.first".to_string(),
                "example.second".to_string(),
                "Ludeon.RimWorld".to_string(),
            ],
        }]
    );
}

#[test]
fn every_block_of_both_kinds_comes_back_in_file_order() {
    let log = parse_plain(
        "Initializing new game with mods:
  - example.first

             Loading game from file SaveA with mods:
  - example.first
  - example.second
             filler
             Loading game from file SaveB with mods:
  - example.second
",
    );

    let summary: Vec<(usize, &LoadEventKind, usize)> = log
        .load_events
        .iter()
        .map(|event| (event.line, &event.kind, event.mods.len()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (1, &LoadEventKind::NewGame, 1),
            (
                4,
                &LoadEventKind::SaveLoad {
                    save_name: "SaveA".to_string()
                },
                2
            ),
            (
                8,
                &LoadEventKind::SaveLoad {
                    save_name: "SaveB".to_string()
                },
                1
            ),
        ]
    );
}

#[test]
fn a_block_ends_at_the_first_non_item_line() {
    let log = parse_plain(
        "Loading game from file S with mods:
  - example.first
not an item
  - example.stray
",
    );

    assert_eq!(log.load_events.len(), 1);
    assert_eq!(log.load_events[0].mods, vec!["example.first".to_string()]);
}

#[test]
fn adjacent_headers_each_open_their_own_event() {
    let log = parse_plain(
        "Loading game from file A with mods:
  - x
Loading game from file B with mods:
Initializing new game with mods:
",
    );

    let counts: Vec<usize> = log.load_events.iter().map(|e| e.mods.len()).collect();
    assert_eq!(counts, vec![1, 0, 0]);
}

#[test]
fn a_log_with_no_block_has_no_load_events() {
    let log = parse_plain(
        "RimWorld 1.6.0 rev1
some line
  - example.first
",
    );

    assert!(log.load_events.is_empty());
}

#[test]
fn a_header_without_items_is_an_event_with_no_mods() {
    let log = parse_plain(
        "Initializing new game with mods:
",
    );

    assert_eq!(log.load_events.len(), 1);
    assert!(log.load_events[0].mods.is_empty());
}

#[test]
fn a_terse_failure_with_no_stack_trace_block_is_tolerated() {
    let log = parse(fixture_text(), &test_formats());
    // The fixture has 7 terse failures but only 4 blocks — the tail
    // 3 (Warfare/Replace plus the two Example Badge failures) must have
    // no stack trace, not panic and not silently vanish.
    let unpaired = log
        .patch_failures
        .iter()
        .filter(|f| f.stack_trace.is_none())
        .count();
    assert_eq!(unpaired, 3);
    assert_eq!(log.patch_failures.len(), 7);
}

#[test]
fn a_stack_trace_block_with_no_terse_line_is_kept_as_an_extra_not_dropped() {
    // The reverse tolerance case: more blocks than terse failures.
    // The shared fixture never exercises this direction (real logs
    // pair 1:1), so this uses a small, self-contained
    // synthetic excerpt instead.
    let text = "\
[Mod A] Patch operation Verse.PatchOperationAdd(Defs/ThingDef[defName=\"X\"]) failed
file: C:\\mods\\a\\Patches\\a.xml
[Mod A - Start of stack trace]
Verse.PatchOperationAdd(xpath=\"Defs/ThingDef[defName=\"X\"]\"): Failed to find a node with the given xpath
[End of stack trace]
Source file: C:\\mods\\a\\Patches\\a.xml
[Mod B - Start of stack trace]
Verse.PatchOperationAdd(xpath=\"Defs/ThingDef[defName=\"Y\"]\"): Failed to find a node with the given xpath
[End of stack trace]
Source file: C:\\mods\\b\\Patches\\b.xml
";
    let log = parse(text, &test_formats());
    assert_eq!(log.patch_failures.len(), 1);
    assert!(log.patch_failures[0].stack_trace.is_some());
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(log.extra_stack_traces[0].mod_tag, "Mod B");
}

#[test]
fn a_lone_carriage_return_does_not_shift_later_lines() {
    // Mirrors the fixture header's own documented trap: a lone `\r`
    // (no matching `\n`) embedded mid-file must not be treated as a
    // line break.
    let text = "line one\rstill line one\nline two\n";
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, vec!["line one\rstill line one", "line two"]);
}

/// A `Read` that produces `parts` in order — each `(bytes, repeat)` is
/// emitted `repeat` times — without ever holding the whole stream, so a
/// test can push gigabytes or millions of lines through the reader in
/// seconds with no large fixture.
pub(super) struct GeneratedLog {
    parts: Vec<(Vec<u8>, u64)>,
    part: usize,
    repeats_done: u64,
    offset: usize,
}

impl GeneratedLog {
    pub(super) fn new(parts: Vec<(Vec<u8>, u64)>) -> Self {
        Self {
            parts,
            part: 0,
            repeats_done: 0,
            offset: 0,
        }
    }
}

impl Read for GeneratedLog {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < out.len() {
            let Some((bytes, repeat)) = self.parts.get(self.part) else {
                break;
            };
            if self.repeats_done >= *repeat {
                self.part += 1;
                self.repeats_done = 0;
                continue;
            }
            let chunk = &bytes[self.offset..];
            let take = chunk.len().min(out.len() - written);
            out[written..written + take].copy_from_slice(&chunk[..take]);
            written += take;
            self.offset += take;
            if self.offset == bytes.len() {
                self.offset = 0;
                self.repeats_done += 1;
            }
        }
        Ok(written)
    }
}

const TERSE_FAILURE_LINES: &str = "[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed\nfile: C:\\mods\\example\\patch.xml\n";
const DEPENDENCY_LINE: &str = "Mod Example dependency (example.framework) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.";

pub(super) fn stream(source: impl Read) -> io::Result<ParsedGameLog> {
    parse_stream(
        BufReader::with_capacity(READ_BUFFER_BYTES, source),
        &test_formats(),
        Framing::PlayerLog,
    )
}

#[test]
fn a_log_past_the_old_line_limit_is_parsed_completely() {
    // The old 2,000,000-line refusal is gone; a file of any line count is read
    // whole, with memory independent of it.
    const OLD_LINE_LIMIT: u64 = 2_000_000;
    let source = GeneratedLog::new(vec![
        (b"RimWorld 1.6.0 rev1\n".to_vec(), 1),
        (b"x\n".to_vec(), OLD_LINE_LIMIT + 1),
        (TERSE_FAILURE_LINES.as_bytes().to_vec(), 1),
    ]);

    let log = stream(source).expect("must parse");

    assert_eq!(log.read_stats.lines_read, 1 + OLD_LINE_LIMIT + 1 + 2);
    assert_eq!(log.patch_failures.len(), 1, "the failure after the limit");
    assert!(!log.read_stats.has_losses());
}

#[test]
fn a_log_past_the_old_byte_limit_is_parsed_completely() {
    // The old 256 MiB refusal is gone. 2,200 lines of 128 KiB pass it (the
    // heads are kept, the tails counted).
    const OLD_BYTE_LIMIT: u64 = 256 * 1024 * 1024;
    const LINE_BYTES: usize = 128 * 1024;
    const LINES: u64 = 2_200;
    assert!(LINE_BYTES as u64 * LINES > OLD_BYTE_LIMIT);
    let mut long_line = vec![b'x'; LINE_BYTES - 1];
    long_line.push(b'\n');
    let source = GeneratedLog::new(vec![
        (b"RimWorld 1.6.0 rev1\n".to_vec(), 1),
        (long_line, LINES),
        (TERSE_FAILURE_LINES.as_bytes().to_vec(), 1),
    ]);

    let log = stream(source).expect("must parse");

    assert_eq!(log.read_stats.lines_read, 1 + LINES + 2);
    assert_eq!(log.read_stats.lines_truncated, LINES);
    assert_eq!(log.patch_failures.len(), 1);
}

#[test]
fn a_long_line_is_truncated_and_counted_and_the_next_failure_still_parses() {
    let mut text = String::from("RimWorld 1.6.0 rev1\n");
    text.push_str(&"y".repeat(MAX_LINE_BYTES * 3));
    text.push('\n');
    text.push_str(TERSE_FAILURE_LINES);

    let log = parse(&text, &test_formats());

    assert_eq!(log.read_stats.lines_truncated, 1);
    assert_eq!(log.read_stats.lines_read, 4);
    assert_eq!(log.patch_failures.len(), 1);
    assert_eq!(
        log.patch_failures[0].source_file.as_deref(),
        Some(r"C:\mods\example\patch.xml")
    );
}

#[test]
fn a_long_line_is_still_classified_from_its_head() {
    let text = format!("{DEPENDENCY_LINE}{}\n", " ".repeat(MAX_LINE_BYTES * 2));

    let log = parse(&text, &shapes_only());

    assert_eq!(log.read_stats.lines_truncated, 1);
    assert_eq!(log.dependency_warnings.len(), 1);
}

#[test]
fn invalid_utf8_is_counted_and_neighbouring_lines_still_parse() {
    let mut bytes = b"RimWorld 1.6.0 rev1\n".to_vec();
    bytes.extend_from_slice(b"garbage \xFF\xFE bytes\n");
    bytes.extend_from_slice(TERSE_FAILURE_LINES.as_bytes());
    bytes.extend_from_slice(DEPENDENCY_LINE.as_bytes());
    bytes.push(b'\n');

    let log = stream(bytes.as_slice()).expect("must parse");

    assert_eq!(log.read_stats.lines_with_invalid_utf8, 1);
    assert_eq!(log.patch_failures.len(), 1);
    assert_eq!(log.dependency_warnings.len(), 1);
}

#[test]
fn a_stack_block_that_never_closes_hits_the_cap_and_is_counted() {
    let extra = 25_u64;
    let source = GeneratedLog::new(vec![
        (b"[Example Mod - Start of stack trace]\n".to_vec(), 1),
        (
            b"Verse.PatchOperationAdd(xpath=\"Defs\"): Failed to find a node\n".to_vec(),
            MAX_BLOCK_LINES as u64 + extra,
        ),
    ]);

    let log = stream(source).expect("must parse");

    assert_eq!(log.read_stats.stack_block_lines_dropped, extra);
    assert_eq!(log.extra_stack_traces.len(), 1, "the open block is kept");
    assert_eq!(log.extra_stack_traces[0].ops.len(), MAX_BLOCK_LINES);
}

#[test]
fn parsing_continues_after_a_capped_block_closes() {
    let source = GeneratedLog::new(vec![
        (b"[Example Mod - Start of stack trace]\n".to_vec(), 1),
        (b"not an op line\n".to_vec(), MAX_BLOCK_LINES as u64 + 3),
        // The block names the failure's own file: pairing needs the two
        // paths to agree when both are present.
        (
            b"[End of stack trace]\nSource file: C:\\mods\\example\\patch.xml\n".to_vec(),
            1,
        ),
        (TERSE_FAILURE_LINES.as_bytes().to_vec(), 1),
    ]);

    let log = stream(source).expect("must parse");

    assert_eq!(log.read_stats.stack_block_lines_dropped, 3);
    // The first abandonment comes after MAX_JOIN_LINES lines; each later
    // join is re-seeded with the line that tipped the previous one, so
    // it abandons after MAX_JOIN_LINES - 1 further lines. (Clearing
    // without re-seeding happens to give the same total at these sizes;
    // `the_line_that_tips_a_join_over_its_bound_starts_the_next_join`
    // is the test that pins the re-seed itself.)
    assert_eq!(
        log.read_stats.stack_joins_abandoned,
        (1 + (MAX_BLOCK_LINES - MAX_JOIN_LINES) / (MAX_JOIN_LINES - 1)) as u64,
        "one abandonment at MAX_JOIN_LINES, then one per MAX_JOIN_LINES - 1 re-seeded lines"
    );
    assert_eq!(log.patch_failures.len(), 1);
    assert!(
        log.extra_stack_traces.is_empty(),
        "the capped block pairs with the failure"
    );
    assert_eq!(
        log.patch_failures[0]
            .stack_trace
            .as_ref()
            .and_then(|block| block.source_file.as_deref()),
        Some(r"C:\mods\example\patch.xml")
    );
}

#[test]
fn an_oversized_unresolved_join_is_abandoned_and_counted() {
    let filler = "z".repeat(MAX_LINE_BYTES);
    let text = format!(
        "[Example Mod - Start of stack trace]\n{filler}\n\
             Verse.PatchOperationAdd(xpath=\"Defs\"): Failed to find a node\n[End of stack trace]\n"
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.read_stats.stack_joins_abandoned, 1);
    assert_eq!(
        log.extra_stack_traces[0].ops.len(),
        1,
        "the real op after it"
    );
}

#[test]
fn crlf_and_lone_carriage_returns_behave_as_before() {
    let text = "RimWorld 1.6.0 rev1\r\n[Example Mod] Patch operation Verse.PatchOperationAdd(a\rb) failed\r\nfile: C:\\p.xml\r\n";

    let log = parse(text, &shapes_only());

    assert_eq!(log.read_stats.lines_read, 3);
    assert_eq!(log.patch_failures.len(), 1);
    assert_eq!(
        log.patch_failures[0].operation,
        "Verse.PatchOperationAdd(a\rb)"
    );
    assert_eq!(
        log.patch_failures[0].source_file.as_deref(),
        Some(r"C:\p.xml")
    );
}

#[test]
fn streaming_and_parse_agree_on_the_fixture() {
    let from_str = parse(fixture_text(), &test_formats());

    let streamed = stream(fixture_text().as_bytes()).expect("must parse");
    let one_byte_reads = parse_stream(
        BufReader::with_capacity(1, fixture_text().as_bytes()),
        &test_formats(),
        Framing::PlayerLog,
    )
    .expect("must parse");

    assert_eq!(streamed, from_str);
    assert_eq!(one_byte_reads, from_str);
    assert_eq!(
        usize::try_from(from_str.read_stats.lines_read).expect("fits"),
        fixture_text().lines().count()
    );
    assert!(!from_str.read_stats.has_losses());
}

#[test]
fn reader_reads_a_file_with_an_invalid_byte_instead_of_refusing_it() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("Player.log");
    let mut bytes = b"RimWorld 1.6.0 rev1\nbad \xFF byte\n".to_vec();
    bytes.extend_from_slice(TERSE_FAILURE_LINES.as_bytes());
    std::fs::write(&path, bytes).expect("write fixture");

    let log = FileGameLogReader::new()
        .read(&path, &shapes_only(), KindChoice::Detect)
        .expect("a bad byte must never refuse the file");

    assert_eq!(log.read_stats.lines_with_invalid_utf8, 1);
    assert_eq!(log.patch_failures.len(), 1);
}

#[test]
fn the_line_that_tips_a_join_over_its_bound_starts_the_next_join() {
    // 99 lines that never form an operation, then the opening half of a
    // wrapped `xpath="` line as the 100th (which trips the line bound),
    // then its continuation: the leaf must survive the abandonment.
    let mut text = String::from("[Example Mod - Start of stack trace]\n");
    text.push_str(&"not an op line\n".repeat(MAX_JOIN_LINES - 1));
    text.push_str("Verse.PatchOperationRemove(xpath=\"\n");
    text.push_str("\t\tDefs/ThingDef[defName=\"X\"]\"): Failed to find a node\n");
    text.push_str("[End of stack trace]\n");

    let log = parse(&text, &shapes_only());

    assert_eq!(log.read_stats.stack_joins_abandoned, 1);
    let ops = &log.extra_stack_traces[0].ops;
    assert_eq!(ops.len(), 1, "the wrapped leaf op must not be lost");
    assert_eq!(ops[0].class, "Verse.PatchOperationRemove");
    assert_eq!(
        ops[0].xpath.as_deref(),
        Some("Defs/ThingDef[defName=\"X\"]")
    );
}

#[test]
fn reader_accepts_a_real_looking_player_log() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("Player.log");
    std::fs::write(&path, fixture_text()).expect("write fixture");
    let log = FileGameLogReader::new()
        .read(&path, &test_formats(), KindChoice::Detect)
        .expect("the fixture excerpt must be accepted");
    assert!(!log.patch_failures.is_empty());
}

// -- startup passes and keyed pairing (log-import step 4b) ------------------

const BANNER_LINE: &str = "RimWorld 1.6.4104 rev1234\n";

/// One patch-reporting mod's stack-trace block whose leaf names `xpath`.
fn stack_block(mod_tag: &str, xpath: &str, source_file: &str) -> String {
    format!(
        "[{mod_tag} - Start of stack trace]\n\
         Verse.PatchOperationAdd(xpath=\"{xpath}\"): Failed to find a node with the given xpath\n\
         [End of stack trace]\n\
         Source file: {source_file}\n"
    )
}

/// One terse failure and its `file:` line.
fn terse_failure(mod_tag: &str, xpath: &str, file: &str) -> String {
    format!("[{mod_tag}] Patch operation Verse.PatchOperationAdd({xpath}) failed\nfile: {file}\n")
}

fn leaf_xpath(failure: &RawPatchFailure) -> Option<&str> {
    failure
        .stack_trace
        .as_ref()
        .and_then(|block| block.ops.first())
        .and_then(|op| op.xpath.as_deref())
}

#[test]
fn a_dependency_warning_logged_in_both_startup_passes_is_reported_once() {
    let text = format!("{BANNER_LINE}{DEPENDENCY_LINE}\n{BANNER_LINE}{DEPENDENCY_LINE}\n");

    let log = parse(&text, &shapes_only());

    assert_eq!(log.dependency_warnings.len(), 1);
    assert_eq!(
        log.dependency_warnings[0].dependency_id,
        "example.framework"
    );
}

const FIRST_ONLY_DEPENDENCY_LINE: &str = "Mod Only First dependency (first.only) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.\n";
const SECOND_DEPENDENCY_LINE: &str = "Mod Second dependency (second.dep) needs to have <downloadUrl> and/or <steamWorkshopUrl> specified.\n";

fn dependency_ids(log: &ParsedGameLog) -> Vec<&str> {
    log.dependency_warnings
        .iter()
        .map(|warning| warning.dependency_id.as_str())
        .collect()
}

#[test]
fn a_dependency_warning_only_the_first_pass_logged_survives() {
    let text = format!(
        "{BANNER_LINE}{FIRST_ONLY_DEPENDENCY_LINE}{DEPENDENCY_LINE}\n{BANNER_LINE}{DEPENDENCY_LINE}\nplain line\n"
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(
        dependency_ids(&log),
        vec!["first.only", "example.framework"]
    );

    let only_first = format!("{BANNER_LINE}{FIRST_ONLY_DEPENDENCY_LINE}{BANNER_LINE}plain line\n");
    assert_eq!(
        parse(&only_first, &shapes_only()).dependency_warnings.len(),
        1
    );
}

#[test]
fn a_second_pass_cut_off_partway_keeps_the_union_without_duplicates() {
    let text = format!(
        "{BANNER_LINE}{FIRST_ONLY_DEPENDENCY_LINE}{SECOND_DEPENDENCY_LINE}{DEPENDENCY_LINE}\n\
         {BANNER_LINE}{FIRST_ONLY_DEPENDENCY_LINE}{SECOND_DEPENDENCY_LINE}"
    );

    let log = parse(&text, &shapes_only());

    // The two the last pass reached come from it; the one it never reached
    // stays from pass 1, listed first (pass order, then position).
    assert_eq!(
        dependency_ids(&log),
        vec!["example.framework", "first.only", "second.dep"]
    );
}

#[test]
fn identical_passes_report_each_dependency_warning_once_in_order() {
    let pass = format!("{FIRST_ONLY_DEPENDENCY_LINE}{SECOND_DEPENDENCY_LINE}{DEPENDENCY_LINE}\n");
    let text = format!("{BANNER_LINE}{pass}{BANNER_LINE}{pass}");

    let log = parse(&text, &shapes_only());

    assert_eq!(
        dependency_ids(&log),
        vec!["first.only", "second.dep", "example.framework"]
    );
}

#[test]
fn a_log_with_no_banner_keeps_its_dependency_warnings() {
    let text = format!("{DEPENDENCY_LINE}\n{DEPENDENCY_LINE}\n");

    let log = parse(&text, &shapes_only());

    assert_eq!(
        log.dependency_warnings.len(),
        2,
        "one pass, nothing to fold"
    );
}

#[test]
fn every_timer_is_kept_and_tagged_with_its_startup_pass() {
    let text = format!(
        "[Example] Early init took 10ms\n\
         {BANNER_LINE}\
         [Example] Load took 20ms\n\
         [Example] Header line\n\
         \x20 Startup init took 30ms (indented continuation)\n\
         {BANNER_LINE}\
         [Example] Load took 40ms\n"
    );

    let log = parse(&text, &shapes_only());

    let seen: Vec<(u64, u32)> = log
        .timers
        .iter()
        .map(|timer| (timer.milliseconds, timer.pass))
        .collect();
    assert_eq!(seen, vec![(10, 0), (20, 1), (30, 1), (40, 2)]);
}

#[test]
fn a_missing_block_mid_sequence_leaves_its_own_failure_bare_and_no_other_shifts() {
    let text = format!(
        "{}{}{}{}{}",
        stack_block("Mod A", "Defs/One", r"C:\mods\a\one.xml"),
        stack_block("Mod A", "Defs/Three", r"C:\mods\a\three.xml"),
        terse_failure("Mod A", "Defs/One", r"C:\mods\a\one.xml"),
        terse_failure("Mod A", "Defs/Two", r"C:\mods\a\two.xml"),
        terse_failure("Mod A", "Defs/Three", r"C:\mods\a\three.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.patch_failures.len(), 3);
    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/One"));
    assert_eq!(
        leaf_xpath(&log.patch_failures[1]),
        None,
        "the failure whose block was never printed stays bare"
    );
    assert_eq!(leaf_xpath(&log.patch_failures[2]), Some("Defs/Three"));
    assert!(log.extra_stack_traces.is_empty());
}

#[test]
fn a_terse_failure_and_a_block_of_different_mod_tags_stay_unpaired() {
    let text = format!(
        "{}{}",
        stack_block("Mod B", "Defs/X", r"C:\mods\shared\x.xml"),
        terse_failure("Mod A", "Defs/X", r"C:\mods\shared\x.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.patch_failures.len(), 1);
    assert!(log.patch_failures[0].stack_trace.is_none());
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(log.extra_stack_traces[0].mod_tag, "Mod B");
}

#[test]
fn a_block_of_one_startup_pass_is_never_paired_with_a_failure_of_another() {
    let text = format!(
        "{BANNER_LINE}{}{BANNER_LINE}{}",
        stack_block("Mod A", "Defs/X", r"C:\mods\a\x.xml"),
        terse_failure("Mod A", "Defs/X", r"C:\mods\a\x.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert!(log.patch_failures[0].stack_trace.is_none());
    assert_eq!(log.extra_stack_traces.len(), 1);
}

#[test]
fn paths_are_compared_ignoring_case_and_slash_direction() {
    // Blocks come in the opposite order to their failures, and spell the
    // paths differently: only the normalized key pairs them correctly (a
    // fallback by tag would cross them).
    let text = format!(
        "{}{}{}{}",
        stack_block("Mod A", "Defs/Two", "c:/mods/a/TWO.xml"),
        stack_block("Mod A", "Defs/One", "c:/mods/a/one.xml"),
        terse_failure("Mod A", "Defs/One", r"C:\Mods\A\One.xml"),
        terse_failure("Mod A", "Defs/Two", r"C:\Mods\A\Two.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/One"));
    assert_eq!(leaf_xpath(&log.patch_failures[1]), Some("Defs/Two"));
}

#[test]
fn a_failure_and_a_block_of_the_same_mod_naming_different_files_stay_unpaired() {
    let text = format!(
        "{}{}",
        stack_block("Mod A", "Defs/X", r"C:\mods\a\block-side.xml"),
        terse_failure("Mod A", "Defs/X", r"C:\mods\a\failure-side.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.patch_failures.len(), 1);
    assert!(log.patch_failures[0].stack_trace.is_none());
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(
        log.extra_stack_traces[0].source_file.as_deref(),
        Some(r"C:\mods\a\block-side.xml")
    );
}

#[test]
fn a_logging_gap_never_gives_a_failure_the_leaf_of_another_files_block() {
    // Failure One's block was lost, and failure Two's terse line was lost:
    // the tag alone would pair One with Two's block and hand it the wrong
    // leaf xpath.
    let text = format!(
        "{}{}",
        stack_block("Mod A", "Defs/Two", r"C:\mods\a\two.xml"),
        terse_failure("Mod A", "Defs/One", r"C:\mods\a\one.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.patch_failures.len(), 1);
    assert_eq!(leaf_xpath(&log.patch_failures[0]), None);
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(
        log.extra_stack_traces[0]
            .ops
            .first()
            .and_then(|op| op.xpath.as_deref()),
        Some("Defs/Two")
    );
}

#[test]
fn a_block_without_a_source_file_line_pairs_by_tag() {
    let text = format!(
        "[Mod A - Start of stack trace]\n\
         Verse.PatchOperationAdd(xpath=\"Defs/X\"): Failed to find a node with the given xpath\n\
         [End of stack trace]\n\
         {}",
        terse_failure("Mod A", "Defs/X", r"C:\mods\a\x.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/X"));
    assert!(log.extra_stack_traces.is_empty());
}

#[test]
fn a_failure_without_a_file_skips_a_block_an_exact_pair_already_claimed() {
    // The exact phase pairs the first failure with the P block, which stays at
    // the front of the tag lane. The path-less second failure must pass over
    // it to the Q block rather than be handed the claimed one again.
    let text = format!(
        "{}{}{}[Mod A] Patch operation Verse.PatchOperationAdd(Defs/Q) failed\n",
        stack_block("Mod A", "Defs/P", r"C:\mods\a\p.xml"),
        stack_block("Mod A", "Defs/Q", r"C:\mods\a\q.xml"),
        terse_failure("Mod A", "Defs/P", r"C:\mods\a\p.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(log.patch_failures.len(), 2);
    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/P"));
    assert_eq!(leaf_xpath(&log.patch_failures[1]), Some("Defs/Q"));
    assert!(log.extra_stack_traces.is_empty());
}

#[test]
fn a_failure_naming_a_file_passes_over_another_files_block_to_a_pathless_one() {
    // The Q block names another file, so it can never pair with a failure
    // naming P; the path-less block after it can.
    let text = format!(
        "{}[Mod A - Start of stack trace]\n\
         Verse.PatchOperationAdd(xpath=\"Defs/P\"): Failed to find a node with the given xpath\n\
         [End of stack trace]\n\
         {}",
        stack_block("Mod A", "Defs/Q", r"C:\mods\a\q.xml"),
        terse_failure("Mod A", "Defs/P", r"C:\mods\a\p.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/P"));
    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(
        log.extra_stack_traces[0].source_file.as_deref(),
        Some(r"C:\mods\a\q.xml")
    );
}

#[test]
fn an_exact_path_match_is_not_taken_by_an_earlier_failure_falling_back_to_its_tag() {
    // The first failure has no block of its own path; the second matches the
    // only block exactly. A fallback that ran per failure in file order
    // would hand the block to the first.
    let text = format!(
        "{}{}{}",
        stack_block("Mod A", "Defs/Two", r"C:\mods\a\two.xml"),
        terse_failure("Mod A", "Defs/One", r"C:\mods\a\one.xml"),
        terse_failure("Mod A", "Defs/Two", r"C:\mods\a\two.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert!(log.patch_failures[0].stack_trace.is_none());
    assert_eq!(leaf_xpath(&log.patch_failures[1]), Some("Defs/Two"));
}

#[test]
fn a_failure_without_a_file_line_pairs_by_tag() {
    let text = format!(
        "{}[Mod A] Patch operation Verse.PatchOperationAdd(Defs/X) failed\n",
        stack_block("Mod A", "Defs/X", r"C:\mods\a\x.xml"),
    );

    let log = parse(&text, &shapes_only());

    assert_eq!(leaf_xpath(&log.patch_failures[0]), Some("Defs/X"));
}

#[test]
fn a_stack_block_is_read_from_its_entry_even_when_its_source_file_line_is_missing() {
    let text = "[Mod A - Start of stack trace]\n\
                Verse.PatchOperationAdd(xpath=\"Defs/X\"): Failed to find a node with the given xpath\n\
                [End of stack trace]\n\
                plain line\n";

    let log = parse(text, &shapes_only());

    assert_eq!(log.extra_stack_traces.len(), 1);
    assert_eq!(log.extra_stack_traces[0].source_file, None);
}
