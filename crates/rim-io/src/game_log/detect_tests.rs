//! Tests for kind detection. Every
//! fixture is synthetic.

use rim_session::ports::{KindChoice, LogFormats, LogKind};
use tempfile::tempdir;

use super::detect::{PEEK_BYTES, PEEK_LINES};
use super::segment::Framing;
use super::*;

const BANNER: &str = "RimWorld 1.6.4104 rev1234";
const TRACE_START: &str = "UnityEngine.StackTraceUtility:ExtractStackTrace ()";

/// One console-copy entry: text, the copy trace, a blank separator.
fn copy_entry(text: &str) -> String {
    format!("{text}\n{TRACE_START}\nVerse.Log:Message (string)\n\n")
}

/// `before` Unity boot lines the way a `Player.log` starts, then the banner
/// and a line after it.
fn player_log_with_banner_at(line: usize) -> String {
    let mut text = String::from("Mono path[0] = 'C:/Example/Data/Managed'\n");
    for index in 1..line - 1 {
        text.push_str(&format!("Unity boot line {index}\n"));
    }
    text.push_str(&format!("{BANNER}\nafter\n"));
    text
}

fn detect_file(text: &str) -> LogKind {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("log.txt");
    std::fs::write(&path, text).expect("write");
    FileGameLogReader::new()
        .read(&path, &LogFormats::empty(), KindChoice::Detect)
        .expect("reads")
        .coverage
        .kind()
}

#[test]
fn a_player_log_with_its_banner_near_line_twenty_two_is_a_player_log() {
    assert_eq!(
        detect_kind(&player_log_with_banner_at(22)),
        LogKind::PlayerLog
    );
}

#[test]
fn a_banner_on_the_first_line_is_a_player_log() {
    assert_eq!(
        detect_kind(&format!("{BANNER}\nmessage\n")),
        LogKind::PlayerLog
    );
}

#[test]
fn an_uncleared_console_copy_with_the_banner_entry_near_the_top_is_a_snapshot() {
    // The shape a fresh, never-cleared console copy has: the command line,
    // then the banner entry, each with its trace.
    let mut text = copy_entry("Command line arguments: -example");
    text.push_str(&copy_entry(BANNER));
    text.push_str(&copy_entry(
        "[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed",
    ));

    assert_eq!(detect_kind(&text), LogKind::ConsoleSnapshot);
    assert_eq!(detect_file(&text), LogKind::ConsoleSnapshot);
}

#[test]
fn a_no_stack_trace_line_before_the_banner_is_a_snapshot() {
    let text = format!(
        "Command line arguments: -example\nNo stack trace.\n\n{BANNER}\nNo stack trace.\n\n"
    );

    assert_eq!(detect_kind(&text), LogKind::ConsoleSnapshot);
}

#[test]
fn a_snapshot_signal_anywhere_in_the_peek_outranks_the_banner() {
    let trace_first = format!("one\n{TRACE_START}\n\n{BANNER}\n");
    let banner_first = format!("one\n{BANNER}\n{TRACE_START}\n");

    assert_eq!(detect_kind(&trace_first), LogKind::ConsoleSnapshot);
    assert_eq!(detect_kind(&banner_first), LogKind::ConsoleSnapshot);
}

#[test]
fn an_uncleared_copy_from_a_launch_without_arguments_is_a_snapshot() {
    // No launch options: the game logs no `Command line arguments:` entry,
    // so the banner is the console's very first entry.
    let mut text = copy_entry(BANNER);
    text.push_str(&copy_entry(
        "[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed",
    ));

    assert_eq!(detect_kind(&text), LogKind::ConsoleSnapshot);
    assert_eq!(detect_file(&text), LogKind::ConsoleSnapshot);
}

#[test]
fn neither_signal_reads_as_a_snapshot() {
    assert_eq!(detect_kind("one\ntwo\n"), LogKind::ConsoleSnapshot);
    assert_eq!(detect_kind(""), LogKind::ConsoleSnapshot);
}

#[test]
fn a_looser_banner_than_the_pass_counters_is_not_a_banner() {
    assert_eq!(
        detect_kind("RimWorld 1.6\nmessage\n"),
        LogKind::ConsoleSnapshot
    );
    assert_eq!(
        detect_kind("RimWorld 1.6.4104 rev1234 extra\nmessage\n"),
        LogKind::ConsoleSnapshot
    );
}

#[test]
fn a_banner_quoted_mid_line_and_a_near_miss_trace_line_signal_nothing() {
    let text = format!("The log said: {BANNER}\nNo stack trace. really\nnot {TRACE_START}\n");

    assert_eq!(detect_kind(&text), LogKind::ConsoleSnapshot);
}

#[test]
fn a_signal_on_the_last_peeked_line_counts_and_one_past_it_does_not() {
    let padding = |lines: usize| "filler\n".repeat(lines);

    let on_last = format!("{}{BANNER}\n", padding(PEEK_LINES - 1));
    let past_last = format!("{}{BANNER}\n", padding(PEEK_LINES));

    assert_eq!(detect_kind(&on_last), LogKind::PlayerLog);
    assert_eq!(detect_kind(&past_last), LogKind::ConsoleSnapshot);
}

#[test]
fn the_peek_stops_after_its_byte_budget_however_few_lines_that_is() {
    let huge_line = format!("{}\n", "x".repeat(70_000));
    let lines_to_spend_the_budget = PEEK_BYTES.div_ceil(MAX_LINE_BYTES);
    let within = format!(
        "{}{BANNER}\n",
        huge_line.repeat(lines_to_spend_the_budget - 2)
    );
    let beyond = format!(
        "{}{BANNER}\n",
        huge_line.repeat(lines_to_spend_the_budget + 1)
    );

    assert_eq!(detect_kind(&within), LogKind::PlayerLog);
    assert_eq!(detect_kind(&beyond), LogKind::ConsoleSnapshot);
}

// -- a Unity first line vetoes the Waylith console reading -------------------

/// A four-space frame in Waylith's shape, with Mono's own `:0 ` tail kept
/// (trailing space) or stripped.
fn four_space_mono_frame(has_trailing_space: bool) -> String {
    let tail = if has_trailing_space { " " } else { "" };
    format!("    at Example.Type.Run () [0x00000] in <filename unknown>:0{tail}\n")
}

fn unity_banner_file_with(frame: &str) -> String {
    format!("Mono path[0] = 'C:/Example/Data/Managed'\n{BANNER}\nSystem.Exception: boom\n{frame}")
}

#[test]
fn a_unity_player_log_with_a_four_space_mono_frame_and_its_trailing_space_is_a_player_log() {
    let text = unity_banner_file_with(&four_space_mono_frame(true));

    assert_eq!(detect_kind(&text), LogKind::PlayerLog);
}

#[test]
fn a_unity_player_log_with_a_four_space_frame_that_matches_the_waylith_shape_is_a_player_log() {
    let frame = four_space_mono_frame(false);
    assert!(
        super::waylith::is_frame(frame.trim_end_matches('\n')),
        "the frame must be the exact shape the Waylith rule accepts"
    );

    let text = unity_banner_file_with(&frame);

    assert_eq!(detect_kind(&text), LogKind::PlayerLog);
    assert_eq!(detect_framing(&text), Framing::PlayerLog);
}

#[test]
fn the_unity_first_line_veto_ignores_a_leading_byte_order_mark() {
    let text = format!(
        "\u{feff}{}",
        unity_banner_file_with(&four_space_mono_frame(false))
    );

    assert_eq!(detect_framing(&text), Framing::PlayerLog);
}

#[test]
fn without_the_mono_path_line_the_same_frame_reads_as_a_waylith_console_copy() {
    // Documents the remaining dependency: with no Unity first line, a
    // four-space frame in the Waylith shape and no timestamped header is the
    // 1.7 console signal even beside the banner, because only the indentation
    // and the stripped trailing space differ from a real Mono frame.
    let text = format!(
        "{BANNER}\nSystem.Exception: boom\n{}",
        four_space_mono_frame(false)
    );

    assert_eq!(detect_framing(&text), Framing::WaylithConsole);
}
