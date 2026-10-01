//! Full-size ground-truth test for the `Player.log` parser: reads the real
//! log named by `RIMMERGE_REAL_GAME_LOG` and, when
//! `RIMMERGE_EXPECTED_GAME_LOG_COUNTS` is also pinned, asserts its exact
//! measured counts. Follows the pins pattern (`docs/testing.md`'s "the pins
//! file pattern"): `RIMMERGE_REAL_GAME_LOG` unset prints an honest
//! `skipping:` line; set to a path that doesn't exist, this test panics (a
//! typo must never silently read as a skip); set to a real file, it parses
//! for real. `RIMMERGE_EXPECTED_GAME_LOG_COUNTS` is the weaker, third pin
//! shape (a real log's own counts are a property of that one file, not a
//! structural fact every log must satisfy) — unset, only the shape
//! (non-empty, deterministic) is asserted.
//!
//! Run explicitly with:
//! `cargo nextest run -p rim-io --all-features --release --run-ignored ignored-only -E 'binary(real_game_log)'`
//!
//! `RIMMERGE_REAL_GAME_LOG_DIR` names a folder of real logs and console copies
//! (searched recursively; the same command runs its test). Unset prints a
//! `skipping:` line; set to something that is not a directory (or a directory
//! with no `.log`/`.txt` file) panics; set to a directory, every file under it
//! is parsed under both framings and must conserve its lines and leak nothing,
//! and every console copy must hold at most 1,000 entries, equal to its
//! trace-start count.
//!
//! No other documented tier command covers this file:
//! `crates/rim-io/tests/real_network_databases.rs` is also `#[ignore]`d and
//! hits the real network, so `-p rim-io` can't simply join the two
//! `RIMMERGE_GAME_DIR`-driven tier commands in the root `CLAUDE.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rim_io::game_log::{FileGameLogReader, Framing, parse_as};
use rim_session::ports::{
    ClassTally, ConsoleFill, EntryClass, GameLogReader, KindChoice, LogCoverage, LogKind,
    ParsedGameLog,
};

const RERUN_COMMAND: &str = "cargo nextest run -p rim-io --all-features --release \
                              --run-ignored ignored-only -E 'binary(real_game_log)'";

/// `RIMMERGE_REAL_GAME_LOG`, following the tier's three-state rule: unset
/// skips with an honest line; set to a path that doesn't exist panics
/// (never a silent skip on a typo); set and present, returns it.
fn real_game_log_path() -> Option<PathBuf> {
    let Ok(raw) = std::env::var("RIMMERGE_REAL_GAME_LOG") else {
        eprintln!("skipping: RIMMERGE_REAL_GAME_LOG is not set. Run: {RERUN_COMMAND}");
        return None;
    };
    let path = PathBuf::from(raw);
    assert!(
        path.exists(),
        "RIMMERGE_REAL_GAME_LOG is set to {} but that file does not exist -- a typo here must \
         fail loudly, not skip the test. Point it at a real Player.log.",
        path.display()
    );
    Some(path)
}

/// Every metric name `RIMMERGE_EXPECTED_GAME_LOG_COUNTS` recognizes — a key
/// outside this list is almost always a typo (it would otherwise silently
/// never get asserted), so [`expected_counts`] rejects it outright rather
/// than accepting and ignoring it.
const VALID_METRICS: &[&str] = &[
    "patch_failures",
    "cross_references",
    "dds_failures",
    "dependency_warnings",
    "load_events",
    "load_order",
    "timers",
    "failures_with_stack_trace",
];

/// `RIMMERGE_EXPECTED_GAME_LOG_COUNTS=patch_failures:<n>,cross_references:<n>,
/// dds_failures:<n>,dependency_warnings:<n>,load_events:<n>,load_order:<n>,timers:<n>,
/// failures_with_stack_trace:<n>` — the third pin shape from
/// `docs/testing.md`: a real log's own counts are a property of that
/// specific file, not a structural fact any log must satisfy, so this
/// never panics when unset, it just skips the exact comparison. A key
/// outside [`VALID_METRICS`] panics naming the valid set — an unrecognized
/// key would otherwise parse cleanly and simply never be asserted.
fn expected_counts() -> BTreeMap<String, usize> {
    let Ok(raw) = std::env::var("RIMMERGE_EXPECTED_GAME_LOG_COUNTS") else {
        return BTreeMap::new();
    };
    raw.split(',')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (name, count) = entry.split_once(':').unwrap_or_else(|| {
                panic!(
                    "RIMMERGE_EXPECTED_GAME_LOG_COUNTS entries must be '<name>:<count>', got {entry}"
                )
            });
            if !VALID_METRICS.contains(&name) {
                panic!(
                    "RIMMERGE_EXPECTED_GAME_LOG_COUNTS names an unknown metric {name:?}; valid \
                     metrics are {VALID_METRICS:?}"
                )
            }
            let count: usize = count.parse().unwrap_or_else(|_| {
                panic!(
                    "RIMMERGE_EXPECTED_GAME_LOG_COUNTS's {name} count must be a number, got {count}"
                )
            });
            (name.to_string(), count)
        })
        .collect()
}

/// When `pins` names `metric`, asserts `measured` matches it exactly —
/// including `0`, a real and useful value to pin (e.g. no lines of some
/// kind on this particular install). Only when `metric` is *unpinned*
/// does this fall back to the shape-only floor (`measured > 0`), since
/// then there is nothing else to check it against.
fn assert_count(pins: &BTreeMap<String, usize>, metric: &str, measured: usize) {
    match pins.get(metric) {
        Some(&expected) => assert_eq!(measured, expected, "{metric} count changed"),
        None => {
            assert!(
                measured > 0,
                "expected at least one {metric} entry on a real, full-size Player.log"
            );
            eprintln!(
                "{metric}={measured} (no RIMMERGE_EXPECTED_GAME_LOG_COUNTS entry -- not asserted)"
            );
        }
    }
}

#[test]
#[ignore = "reads a real Player.log named by RIMMERGE_REAL_GAME_LOG; run explicitly, see this \
            file's own doc comment"]
fn real_player_log_parses_and_matches_its_pinned_counts() {
    let Some(path) = real_game_log_path() else {
        return;
    };
    // The formats come the way the product gets them without a fetched
    // cache: from the embedded rules bundle.
    let knowledge = rim_io::vendored_knowledge().knowledge;
    let formats = knowledge.log_formats();

    let reader = FileGameLogReader::new();
    let started = std::time::Instant::now();
    let log = reader
        .read(&path, &formats, KindChoice::Detect)
        .unwrap_or_else(|error| panic!("real log must parse cleanly: {error}"));
    eprintln!(
        "parsed {} lines in {:.2?}",
        log.read_stats.lines_read,
        started.elapsed()
    );

    // Parsing is pure and must be deterministic on the same input — this is
    // the shape assertion the pins-file pattern falls back to when no exact
    // counts are pinned.
    let second_pass = reader
        .read(&path, &formats, KindChoice::Detect)
        .unwrap_or_else(|error| panic!("real log must parse cleanly on a second read: {error}"));
    assert_eq!(
        log, second_pass,
        "parsing the same real log twice must give an identical result"
    );

    // Every stack-trace block must pair with the terse failure line that
    // claims it — a general parser invariant, not something specific to
    // one log's content, so it holds regardless of whether counts are
    // pinned.
    assert!(
        log.extra_stack_traces.is_empty(),
        "every stack-trace block should pair with a terse patch-failure line; found {} left over",
        log.extra_stack_traces.len()
    );

    // Every terse failure's own `file:` line pairs 1:1 -- the `PatchFailure`
    // entry's `file:` continuation is exact on a
    // real log; `source_file: Option<_>` is defensive against a hostile or
    // truncated file, never evidence it's ever actually missing here. A
    // structural fact about the format, not this specific log's content,
    // so it's asserted unconditionally rather than pinned.
    let failures_with_source_file = log
        .patch_failures
        .iter()
        .filter(|f| f.source_file.is_some())
        .count();
    assert_eq!(
        failures_with_source_file,
        log.patch_failures.len(),
        "every patch failure's terse line must pair 1:1 with a following file: line on a real log"
    );

    // Unlike `source_file`, a stack-trace block is not guaranteed for
    // every terse failure (`RawPatchFailure`'s own doc comment: "an
    // excerpt may have more terse failures than blocks or vice versa"),
    // so how many of them actually carry one is a property of this
    // specific log, not a structural fact -- pinnable, not unconditional.
    let failures_with_stack_trace = log
        .patch_failures
        .iter()
        .filter(|f| f.stack_trace.is_some())
        .count();

    // Every stack-trace block's own leaf (`ops.first()`, innermost per
    // `StackTraceBlock::ops`'s own doc comment) must be the real leaf, not
    // an enclosing wrapper op standing in for it -- a wrapper's reason is
    // always one of the fixed "Error in ..." shapes
    // (`PatchOperationSequence: Error in the operation at position=N`,
    // or an `Error in <branch>` line), which a genuine leaf
    // reason (the concrete operation's own failure text) never is. Content-
    // independent, so this holds on any real log, not just one with a
    // specific pinned leaf reason.
    for failure in &log.patch_failures {
        let Some(stack) = &failure.stack_trace else {
            continue;
        };
        let leaf = stack
            .ops
            .first()
            .expect("a real stack-trace block always has at least one op");
        assert!(
            !leaf.reason.starts_with("Error in "),
            "the leaf op must be the innermost failure, never an enclosing wrapper's own \
             \"Error in ...\" reason: {leaf:?}"
        );
    }

    let pins = expected_counts();
    assert_count(&pins, "patch_failures", log.patch_failures.len());
    assert_count(&pins, "cross_references", log.cross_references.len());
    assert_count(&pins, "dds_failures", log.dds_failures.len());
    assert_count(&pins, "dependency_warnings", log.dependency_warnings.len());
    // `load_events` is asserted beside `load_order` because `map_or(0, ..)`
    // below cannot tell "no load event" from "an event with zero mods".
    assert_count(&pins, "load_events", log.load_events.len());
    assert_count(
        &pins,
        "load_order",
        log.load_events.last().map_or(0, |event| event.mods.len()),
    );
    assert_count(&pins, "timers", log.timers.len());
    assert_count(
        &pins,
        "failures_with_stack_trace",
        failures_with_stack_trace,
    );
}

// -- the directory tier -----------------------------------------------------

/// `RIMMERGE_REAL_GAME_LOG_DIR`, following the tier's three-state rule: unset
/// skips with an honest line; set to something that is not a directory (or a
/// directory holding no log file) panics; set to a directory with logs,
/// returns every log file under it, recursively, in path order.
fn real_game_log_files() -> Option<Vec<PathBuf>> {
    let Ok(raw) = std::env::var("RIMMERGE_REAL_GAME_LOG_DIR") else {
        eprintln!("skipping: RIMMERGE_REAL_GAME_LOG_DIR is not set. Run: {RERUN_COMMAND}");
        return None;
    };
    let dir = PathBuf::from(raw);
    assert!(
        dir.is_dir(),
        "RIMMERGE_REAL_GAME_LOG_DIR is set to {} but that is not a directory -- a typo here must \
         fail loudly, not skip the test. Point it at a folder of real logs.",
        dir.display()
    );
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| is_log_extension(path))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "RIMMERGE_REAL_GAME_LOG_DIR is set to {} but no .log/.txt file is under it",
        dir.display()
    );
    Some(files)
}

fn is_log_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("log") || extension.eq_ignore_ascii_case("txt")
        })
}

/// Unity's `ExtractStackTrace` frame that starts every entry's trace in a
/// console copy (the engine's copy format, one per entry).
const COPY_TRACE_START: &str = "UnityEngine.StackTraceUtility:ExtractStackTrace ()";

fn count_copy_trace_starts(content: &str) -> u64 {
    content
        .lines()
        .filter(|line| line.trim() == COPY_TRACE_START)
        .count() as u64
}

/// Unity prints this as the very first line of every `Player.log`.
const PLAYER_LOG_FIRST_LINE_PREFIX: &str = "Mono path[0] = ";

/// The kind a file must be detected as, from evidence that does not go
/// through the product's detection rule: a copy trace-start line anywhere
/// in the file (0 of 29 real `Player.log`s hold one, all 137 real snapshots
/// do) means a console snapshot, and a first line starting with Unity's
/// `Mono path[0] = ` means a `Player.log`. `None` for a file with neither
/// (a hand-written note): nothing independent says what it is.
fn oracle_kind(content: &str) -> Option<LogKind> {
    let is_snapshot = count_copy_trace_starts(content) > 0;
    let first_line = content.lines().next().unwrap_or_default();
    let is_player_log = first_line
        .trim_start_matches('\u{feff}')
        .starts_with(PLAYER_LOG_FIRST_LINE_PREFIX);
    match (is_snapshot, is_player_log) {
        (true, true) => panic!("a file with both a copy trace start and a Unity first line"),
        (true, false) => Some(LogKind::ConsoleSnapshot),
        (false, true) => Some(LogKind::PlayerLog),
        (false, false) => None,
    }
}

/// The console cap: the engine's `LogMessageQueue.maxMessages`.
const CONSOLE_CAP: u64 = 1_000;

/// The engine's own message when it stops logging (`Log.Notify_Message
/// ReceivedThreadedInternal`): one per logging gap.
const LOGGING_STOPPED_LINE: &str = "Reached max messages limit. Stopping logging to avoid spam.";

fn count_stop_lines(content: &str) -> u64 {
    content
        .lines()
        .filter(|line| line.trim() == LOGGING_STOPPED_LINE)
        .count() as u64
}

/// One line describing what the reader says the file covers.
fn coverage_line(log: &ParsedGameLog) -> String {
    let gaps = log.logging_gaps.len();
    match &log.coverage {
        LogCoverage::PlayerLog(coverage) => format!(
            "Player.log, {} passes, patch phase {:?}, ended {:?}, {gaps} logging gaps",
            coverage.passes, coverage.patch_phase, coverage.end_state
        ),
        LogCoverage::ConsoleSnapshot(coverage) => format!(
            "console snapshot, {} entries, {:?}, head truncated {}, {gaps} logging gaps",
            coverage.entries,
            coverage.fill(),
            coverage.head_truncated
        ),
    }
}

fn assert_conserved(log: &ParsedGameLog, label: &str) {
    let totals = log.totals();
    assert!(totals.is_conserved(), "{label}: P1 broken: {totals:?}");
}

/// P2: describes the lines that matched a known class's loose sentinel while
/// sitting in another class (empty when the log is clean).
fn leak_report(log: &ParsedGameLog, label: &str) -> Option<String> {
    (log.sentinels.total != 0).then(|| {
        format!(
            "{label}: {} sentinel leak(s): {:#?}",
            log.sentinels.total, log.sentinels.leaks
        )
    })
}

fn print_file_summary(label: &str, log: &ParsedGameLog) {
    let totals = log.totals();
    let families: usize = log.classes.values().map(|tally| tally.families.len()).sum();
    let unclassified = log.classes.get(&EntryClass::Unclassified);
    eprintln!(
        "{label}: {} lines, {} entries, {families} families, unclassified {} entries / {} \
         families",
        totals.lines_read,
        totals.entries,
        unclassified.map_or(0, ClassTally::entries),
        unclassified.map_or(0, |tally| tally.families.len()),
    );
    let Some(tally) = unclassified else {
        return;
    };
    let mut top: Vec<_> = tally.families.iter().collect();
    top.sort_by_key(|(_, family)| std::cmp::Reverse(family.count));
    for (key, family) in top.into_iter().take(10) {
        eprintln!("    {} x {}", family.count, key.as_str());
    }
}

#[test]
#[ignore = "reads every real log under RIMMERGE_REAL_GAME_LOG_DIR; run explicitly, see this \
            file's own doc comment"]
fn every_real_log_under_the_directory_conserves_its_lines_and_leaks_nothing() {
    let Some(files) = real_game_log_files() else {
        return;
    };
    // The formats come the way the product gets them without a fetched
    // cache: from the embedded rules bundle.
    let knowledge = rim_io::vendored_knowledge().knowledge;
    let formats = knowledge.log_formats();
    let reader = FileGameLogReader::new();
    let mut snapshots = 0usize;
    let mut player_logs = 0usize;
    let mut oracle_decided = 0usize;
    let mut undecided: Vec<String> = Vec::new();
    let mut at_cap = 0usize;
    let mut head_truncated = 0usize;
    let mut max_snapshot_entries = 0u64;
    let mut checked = 0usize;
    let mut leaks: Vec<String> = Vec::new();
    for path in files {
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        let content = String::from_utf8_lossy(&bytes);
        let label = path.display().to_string();
        // The product's own path: the file reader decides the kind by content.
        let log = reader
            .read(&path, &formats, KindChoice::Detect)
            .unwrap_or_else(|error| panic!("{label}: the reader must read it: {error}"));
        let detected = log.coverage.kind();
        match oracle_kind(&content) {
            Some(expected) => {
                assert_eq!(detected, expected, "{label}: detected kind");
                oracle_decided += 1;
            }
            // A hand-written note: still parsed (conservation and the
            // sentinels must hold on any text), but nothing independent
            // says what kind it is.
            None => undecided.push(label.clone()),
        }
        let native = Framing::from(detected);
        let foreign = match native {
            Framing::PlayerLog => Framing::ConsoleCopy,
            Framing::ConsoleCopy => Framing::PlayerLog,
        };
        assert_eq!(
            log,
            parse_as(&content, &formats, native),
            "{label}: the reader and the pure parse must agree"
        );
        assert_eq!(
            log.logging_gaps.len() as u64,
            count_stop_lines(&content),
            "{label}: one logging gap per stop message"
        );
        eprintln!("{label}: {}", coverage_line(&log));
        assert_conserved(&log, &label);
        leaks.extend(leak_report(&log, &label));
        assert_eq!(
            log,
            parse_as(&content, &formats, native),
            "{label}: parsing twice must give an identical result"
        );
        // The other framing reads the same lines as different entries (a whole
        // Player.log becomes one giant console message), so only the line
        // accounting is framing-independent.
        assert_conserved(&parse_as(&content, &formats, foreign), &label);
        print_file_summary(&label, &log);
        if let LogCoverage::ConsoleSnapshot(coverage) = &log.coverage {
            snapshots += 1;
            let entries = log.totals().entries;
            max_snapshot_entries = max_snapshot_entries.max(entries);
            assert_eq!(coverage.entries, entries, "{label}: coverage entries");
            assert_eq!(
                coverage.fill(),
                match entries.cmp(&CONSOLE_CAP) {
                    std::cmp::Ordering::Less => ConsoleFill::BelowCap,
                    std::cmp::Ordering::Equal => ConsoleFill::AtCap,
                    std::cmp::Ordering::Greater => ConsoleFill::OverCap,
                },
                "{label}: cap state"
            );
            at_cap += usize::from(coverage.fill() == ConsoleFill::AtCap);
            head_truncated += usize::from(coverage.head_truncated);
            let trace_starts = count_copy_trace_starts(&content);
            if trace_starts > 0 {
                assert!(
                    entries <= CONSOLE_CAP,
                    "{label}: a console copy holds at most {CONSOLE_CAP} entries, got {entries}"
                );
                assert_eq!(
                    entries, trace_starts,
                    "{label}: entries must equal the copy's trace-start count"
                );
            }
        } else {
            player_logs += 1;
        }
        checked += 1;
    }
    eprintln!(
        "checked {checked} files: {player_logs} Player.logs, {snapshots} console snapshots \
         (max {max_snapshot_entries} entries, {at_cap} at the cap, {head_truncated} head \
         truncated); the independent oracle decided {oracle_decided}, {} undecided: \
         {undecided:?}",
        undecided.len()
    );
    assert!(
        leaks.is_empty(),
        "{}",
        leaks.join(
            "
"
        )
    );
    assert!(checked > 0, "the directory held no log");
}
