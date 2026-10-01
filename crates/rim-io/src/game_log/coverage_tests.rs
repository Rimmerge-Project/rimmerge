//! Tests for kind detection, coverage and logging gaps (log-import plan
//! step 5: B2, B8, B9). Every fixture is synthetic.

use rim_session::ports::{
    CONSOLE_CAP, ConsoleFill, EndState, EntryClass, GapEnd, KindChoice, LogCoverage, LogKind,
    LoggingGap, MAX_BANNER_LINES, MAX_LOGGING_GAPS, MAX_REPORT_PATH_BYTES, PatchPhaseEvidence,
    PlayerLogCoverage, ReadLoss, SnapshotCoverage,
};
use tempfile::tempdir;

use super::*;

const BANNER: &str = "RimWorld 1.6.4104 rev1234";
const STOP: &str = "Reached max messages limit. Stopping logging to avoid spam.";
const RESUME: &str = "Message logging is now once again on.";
const FAILURE: &str = "[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed";
const FOOTER: &str = "Memory Statistics:\n  [ALLOC_DEFAULT] 12 allocs\n[ALLOC_TEMP_MAIN]\n";
const CRASH_HANDLER: &str = "A crash has been intercepted by the crash handler. For call stack and other details, see the latest crash report generated in:";

fn player_log(text: &str) -> ParsedGameLog {
    parse(text, &LogFormats::empty())
}

fn snapshot(text: &str) -> ParsedGameLog {
    parse_as(text, &LogFormats::empty(), Framing::ConsoleCopy)
}

fn player_coverage(log: &ParsedGameLog) -> &PlayerLogCoverage {
    match &log.coverage {
        LogCoverage::PlayerLog(coverage) => coverage,
        LogCoverage::ConsoleSnapshot(_) => panic!("expected Player.log coverage"),
    }
}

/// `count` console-copy entries, each a message, `No stack trace.` and a
/// blank line.
fn console_entries(count: usize) -> String {
    (0..count)
        .map(|index| format!("message {index}\nNo stack trace.\n\n"))
        .collect()
}

// -- kind detection (the rule itself is tested in `detect_tests.rs`) ---------

#[test]
fn the_reader_decides_by_content_whatever_the_file_is_named() {
    let dir = tempdir().expect("tempdir");
    let named_like_a_copy = dir.path().join("Player_console_export.txt");
    std::fs::write(&named_like_a_copy, format!("{BANNER}\nmessage\n")).expect("write");
    let named_like_a_log = dir.path().join("Player.log");
    std::fs::write(&named_like_a_log, console_entries(3)).expect("write");

    let read = |path| {
        FileGameLogReader::new()
            .read(path, &LogFormats::empty(), KindChoice::Detect)
            .expect("reads")
    };

    assert_eq!(read(&named_like_a_copy).coverage.kind(), LogKind::PlayerLog);
    assert_eq!(
        read(&named_like_a_log).coverage.kind(),
        LogKind::ConsoleSnapshot
    );
}

#[test]
fn detection_does_not_lose_the_lines_it_peeked_at() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("Player.log");
    let text = format!("{BANNER}\n{FAILURE}\nfile: C:\\example\\patch.xml\n");
    std::fs::write(&path, &text).expect("write");

    let detected = FileGameLogReader::new()
        .read(&path, &LogFormats::empty(), KindChoice::Detect)
        .expect("reads");

    assert_eq!(detected, parse(&text, &LogFormats::empty()));
    assert_eq!(detected.read_stats.lines_read, 3);
}

#[test]
fn a_forced_kind_wins_over_what_the_content_says_and_still_conserves_lines() {
    let dir = tempdir().expect("tempdir");
    let player_log = dir.path().join("a.log");
    std::fs::write(
        &player_log,
        format!("{BANNER}\n{FAILURE}\nfile: x\n\n{STOP}\n"),
    )
    .expect("write");
    let copy = dir.path().join("b.txt");
    std::fs::write(&copy, console_entries(4)).expect("write");

    for path in [&player_log, &copy] {
        for kind in [LogKind::PlayerLog, LogKind::ConsoleSnapshot] {
            let log = FileGameLogReader::new()
                .read(path, &LogFormats::empty(), KindChoice::Force(kind))
                .expect("reads");

            assert_eq!(log.coverage.kind(), kind, "{path:?} forced as {kind}");
            assert!(log.totals().is_conserved(), "{path:?} forced as {kind}");
        }
    }
}

// -- Player.log coverage ------------------------------------------------------

#[test]
fn a_log_with_the_memory_footer_ended_cleanly() {
    let log = player_log(&format!("{BANNER}\nmessage\n{FOOTER}"));

    assert_eq!(player_coverage(&log).end_state, EndState::CleanExit);
}

#[test]
fn a_log_with_neither_end_marker_is_truncated() {
    let log = player_log(&format!("{BANNER}\nmessage\nanother message\n"));

    assert_eq!(player_coverage(&log).end_state, EndState::Truncated);
}

#[test]
fn a_crash_handler_message_makes_the_log_crashed_with_its_report_path() {
    let text = format!(
        "{BANNER}\nmessage\nCrash!!!\n=== END OF STACKTRACE ===\n\n{CRASH_HANDLER}\n * C:/Example/Crashes\n"
    );

    let log = player_log(&text);

    assert_eq!(
        player_coverage(&log).end_state,
        EndState::Crashed {
            report_path: Some("C:/Example/Crashes".to_string())
        }
    );
}

#[test]
fn a_crash_handler_message_cut_off_before_its_path_line_has_no_path() {
    let log = player_log(&format!("{BANNER}\n{CRASH_HANDLER}\n"));

    assert_eq!(
        player_coverage(&log).end_state,
        EndState::Crashed { report_path: None }
    );
}

#[test]
fn a_crash_report_header_alone_is_a_crash() {
    let log = player_log(&format!("{BANNER}\nCrash!!!\n"));

    assert_eq!(
        player_coverage(&log).end_state,
        EndState::Crashed { report_path: None }
    );
}

#[test]
fn a_crash_outranks_a_memory_footer() {
    let log = player_log(&format!(
        "{BANNER}\n{FOOTER}\n{CRASH_HANDLER}\n * C:/Example/Crashes\n"
    ));

    assert!(matches!(
        player_coverage(&log).end_state,
        EndState::Crashed { .. }
    ));
}

#[test]
fn an_overlong_crash_path_is_cut_on_a_character_boundary() {
    let path = "é".repeat(MAX_REPORT_PATH_BYTES);
    let log = player_log(&format!("{BANNER}\n{CRASH_HANDLER}\n * {path}\n"));

    let EndState::Crashed {
        report_path: Some(kept),
    } = &player_coverage(&log).end_state
    else {
        panic!("expected a crash with a path");
    };

    assert_eq!(kept.len(), MAX_REPORT_PATH_BYTES);
    assert!(kept.chars().all(|character| character == 'é'));
    assert_eq!(log.read_stats.crash_report_paths_truncated, 1);
    assert!(
        log.read_stats
            .losses()
            .contains(&(ReadLoss::CrashReportPathTruncated, 1))
    );
}

#[test]
fn a_crash_path_within_its_bound_is_not_counted_as_truncated() {
    let log = player_log(&format!(
        "{BANNER}\n{CRASH_HANDLER}\n * C:/Example/Crashes\n"
    ));

    assert_eq!(log.read_stats.crash_report_paths_truncated, 0);
}

#[test]
fn the_banner_lines_and_the_pass_count_are_reported() {
    let log = player_log(&format!("{BANNER}\na\nb\n{BANNER}\nc\n"));

    let coverage = player_coverage(&log);

    assert_eq!(coverage.passes, 2);
    assert_eq!(coverage.banner_lines, vec![1, 4]);
}

#[test]
fn banner_lines_are_bounded_but_the_pass_count_is_not() {
    let passes = MAX_BANNER_LINES + 4;
    let log = player_log(&format!("{BANNER}\n").repeat(passes));

    let coverage = player_coverage(&log);

    assert_eq!(coverage.passes, passes as u64);
    assert_eq!(coverage.banner_lines.len(), MAX_BANNER_LINES);
}

#[test]
fn a_patch_failure_is_positive_evidence_the_patch_phase_ran() {
    let with_failure = player_log(&format!(
        "{BANNER}\n{FAILURE}\nfile: C:\\example\\patch.xml\n"
    ));
    let without = player_log(&format!("{BANNER}\nmessage\n"));

    assert_eq!(
        player_coverage(&with_failure).patch_phase,
        PatchPhaseEvidence::PatchFailureLogged
    );
    assert_eq!(
        player_coverage(&without).patch_phase,
        PatchPhaseEvidence::NoneLogged
    );
}

// -- console snapshot coverage -------------------------------------------------

fn snapshot_coverage(log: &ParsedGameLog) -> &SnapshotCoverage {
    match &log.coverage {
        LogCoverage::ConsoleSnapshot(coverage) => coverage,
        LogCoverage::PlayerLog(_) => panic!("expected snapshot coverage"),
    }
}

#[test]
fn a_snapshot_reports_its_entry_count_and_how_full_the_console_was() {
    for (entries, fill) in [
        (999_usize, ConsoleFill::BelowCap),
        (1_000, ConsoleFill::AtCap),
        (1_001, ConsoleFill::OverCap),
    ] {
        let log = snapshot(&console_entries(entries));

        let coverage = snapshot_coverage(&log);

        assert_eq!(coverage.entries, entries as u64);
        assert_eq!(coverage.fill(), fill, "{entries} entries");
    }
    assert_eq!(CONSOLE_CAP, 1_000);
}

#[test]
fn a_normal_copy_is_not_head_truncated() {
    let log = snapshot(&console_entries(3));

    assert!(!snapshot_coverage(&log).head_truncated);
}

#[test]
fn a_copy_that_starts_on_a_trace_is_head_truncated() {
    let trace_first = snapshot(
        "UnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)\n\nmessage\nNo stack trace.\n\n",
    );
    let no_trace_first = snapshot("No stack trace.\n\nmessage\nNo stack trace.\n\n");
    let after_blank_lines = snapshot("\n\nNo stack trace.\n\nmessage\nNo stack trace.\n\n");

    assert!(snapshot_coverage(&trace_first).head_truncated);
    assert!(snapshot_coverage(&no_trace_first).head_truncated);
    assert!(
        snapshot_coverage(&after_blank_lines).head_truncated,
        "the first non-blank line decides"
    );
}

#[test]
fn a_trace_line_later_in_the_copy_does_not_make_it_head_truncated() {
    let log = snapshot("message\nNo stack trace.\n\nNo stack trace.\n\n");

    assert!(!snapshot_coverage(&log).head_truncated);
}

#[test]
fn an_empty_snapshot_is_not_head_truncated() {
    assert!(!snapshot_coverage(&snapshot("")).head_truncated);
}

#[test]
fn a_blank_line_inside_a_snapshot_entry_does_not_add_an_entry() {
    let text =
        "first line\n\nstill the same message\nNo stack trace.\n\nsecond\nNo stack trace.\n\n";

    let log = snapshot(text);

    let LogCoverage::ConsoleSnapshot(coverage) = &log.coverage else {
        panic!("expected snapshot coverage");
    };
    assert_eq!(coverage.entries, 2);
}

// -- logging gaps ---------------------------------------------------------------

fn gaps_of(text: &str) -> Vec<LoggingGap> {
    player_log(text).logging_gaps
}

#[test]
fn a_log_with_no_stop_message_has_no_gaps() {
    assert!(gaps_of(&format!("{BANNER}\nmessage\n")).is_empty());
}

#[test]
fn a_stop_followed_by_a_resume_is_a_closed_gap() {
    let gaps = gaps_of(&format!("{BANNER}\n{STOP}\nnative line\n{RESUME}\n"));

    assert_eq!(
        gaps,
        vec![LoggingGap {
            stop_line: 2,
            resume: GapEnd::Resumed { line: 4 }
        }]
    );
}

#[test]
fn a_stop_with_no_resume_is_a_gap_that_never_resumed() {
    let gaps = gaps_of(&format!("{BANNER}\n{STOP}\nnative line\n"));

    assert_eq!(
        gaps,
        vec![LoggingGap {
            stop_line: 2,
            resume: GapEnd::NeverResumed
        }]
    );
}

#[test]
fn several_gaps_are_listed_in_file_order() {
    let text = format!("{STOP}\n{RESUME}\nmessage\n{STOP}\n{RESUME}\n{STOP}\n");

    let gaps = gaps_of(&text);

    assert_eq!(
        gaps,
        vec![
            LoggingGap {
                stop_line: 1,
                resume: GapEnd::Resumed { line: 2 }
            },
            LoggingGap {
                stop_line: 4,
                resume: GapEnd::Resumed { line: 5 }
            },
            LoggingGap {
                stop_line: 6,
                resume: GapEnd::NeverResumed
            },
        ]
    );
}

#[test]
fn a_second_stop_while_a_gap_is_open_leaves_the_first_unresumed_and_keeps_the_count() {
    let gaps = gaps_of(&format!("{STOP}\nmessage\n{STOP}\n{RESUME}\n"));

    assert_eq!(gaps.len(), 2, "one gap per stop message");
    assert_eq!(gaps[0].resume, GapEnd::NeverResumed);
    assert_eq!(gaps[1].resume, GapEnd::Resumed { line: 4 });
}

#[test]
fn a_resume_with_no_stop_before_it_is_not_a_gap() {
    assert!(gaps_of(&format!("{BANNER}\n{RESUME}\n")).is_empty());
}

#[test]
fn a_snapshot_reports_its_gaps_too() {
    let text = format!("{STOP}\nNo stack trace.\n\n{RESUME}\nNo stack trace.\n\n");

    let log = snapshot(&text);

    assert_eq!(
        log.logging_gaps,
        vec![LoggingGap {
            stop_line: 1,
            resume: GapEnd::Resumed { line: 4 }
        }]
    );
}

#[test]
fn gaps_past_the_bound_are_counted_as_a_loss_not_listed() {
    let extra = 7;
    let text = format!("{STOP}\n{RESUME}\n").repeat(MAX_LOGGING_GAPS + extra);

    let log = player_log(&text);

    assert_eq!(log.logging_gaps.len(), MAX_LOGGING_GAPS);
    assert_eq!(log.read_stats.logging_gaps_dropped, extra as u64);
    assert!(
        log.read_stats
            .losses()
            .contains(&(ReadLoss::LoggingGapDropped, extra as u64))
    );
}

#[test]
fn the_gap_messages_are_engine_info_never_errors() {
    let log = player_log(&format!("{STOP}\n{RESUME}\n"));

    assert!(!log.classes.contains_key(&EntryClass::Unclassified));
    assert_eq!(log.sentinels.total, 0);
}
