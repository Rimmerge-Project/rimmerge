//! End-to-end tests for `rimmerge log import`, against a
//! scratch copy of `rim-io`'s `merge_game` fixture (never the real game
//! install) and the shared `rim-io` `Player.log` excerpt fixture.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use serde_json::Value;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

struct ScratchGame {
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

fn scratch_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    copy_dir_recursive(&merge_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy merge_game fixture: {error}"));
    ScratchGame {
        mods_config: game_dir.join("ModsConfig.xml"),
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        game_dir,
    }
}

fn path_args(cmd: &mut Command, game: &ScratchGame) {
    cmd.arg("--game-dir")
        .arg(&game.game_dir)
        .arg("--workshop-dir")
        .arg(&game.workshop_dir)
        .arg("--mods-config")
        .arg(&game.mods_config)
        .arg("--profile-dir")
        .arg(&game.profile_dir);
}

/// The `Player.log` excerpt the log parser's own tests use
/// (`crates/rim-io/tests/fixtures/player_log_excerpt.log`) — shared rather
/// than duplicated.
fn player_log_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-io")
        .join("tests")
        .join("fixtures")
        .join("player_log_excerpt.log")
}

#[test]
fn log_import_prints_counts_and_unattributed_failures_as_text() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(player_log_fixture());
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("7 patch failures"))
        .stdout(predicates::str::contains("2 DDS failures"))
        .stdout(predicates::str::contains("2 dependency warnings"))
        .stdout(predicates::str::contains("(unattributed:"));
}

#[test]
fn log_import_json_round_trips_every_section() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log")
        .arg("import")
        .arg(player_log_fixture())
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let json: Value = serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    assert_eq!(json["patch_failures"].as_array().expect("array").len(), 7);
    assert_eq!(json["dds_failures"].as_array().expect("array").len(), 2);
    let events = json["load_events"].as_array().expect("array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "new_game");
    assert!(events[0]["save_name"].is_null());
    assert_eq!(events[0]["mods"].as_array().expect("array").len(), 11);
    assert_eq!(json["load_events_disagree"], false);
    // None of `merge_game`'s two mods (ModA/ModB) match anything in the
    // fixture log, by path or by display name — every reference must
    // still come through as `Unattributed`, never dropped.
    assert!(
        json["patch_failures"][0]["attribution"]["unattributed_raw"].is_string(),
        "full JSON: {json}"
    );
    assert!(json["patch_failures"][0]["attribution"]["mod_id"].is_null());
}

/// A synthetic console snapshot: `count` entries of a message, `No stack
/// trace.` and a blank line (the game's copy format).
fn write_snapshot(dir: &Path, name: &str, count: usize) -> PathBuf {
    let path = dir.join(name);
    let text: String = (0..count)
        .map(|index| format!("snapshot message {index}\nNo stack trace.\n\n"))
        .collect();
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

fn import_json(game: &ScratchGame, paths: &[&Path], extra: &[&str]) -> Value {
    let mut cmd = common::rimmerge();
    cmd.arg("log")
        .arg("import")
        .args(paths)
        .arg("--json")
        .args(extra);
    path_args(&mut cmd, game);
    let output = cmd
        .output()
        .unwrap_or_else(|error| panic!("must run: {error}"));
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("output must be valid JSON: {error}"))
}

#[test]
fn log_import_reads_a_console_snapshot_by_content_and_labels_it() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    // A name with no kind in it: the content decides.
    let snapshot = write_snapshot(dir.path(), "midgame1.txt", 3);

    let json = import_json(&game, &[&snapshot], &[]);

    assert_eq!(json["kind"], "console_snapshot");
    assert_eq!(json["coverage"]["kind"], "console_snapshot");
    assert_eq!(json["coverage"]["entries"], 3);
    assert_eq!(json["coverage"]["console_fill"], "below_cap");
    assert_eq!(json["coverage"]["head_truncated"], false);
    assert_eq!(json["totals"]["conserved"], true);
    assert!(
        json.get("path").is_none(),
        "one path keeps the single shape"
    );
}

#[test]
fn log_import_flags_a_snapshot_at_the_console_cap_in_json_and_text() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "full.txt", 1_000);

    let json = import_json(&game, &[&snapshot], &[]);
    assert_eq!(json["coverage"]["console_fill"], "at_cap");

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&snapshot);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "Console snapshot import (only the 1000 entries the console held):",
        ))
        .stdout(predicates::str::contains(
            "coverage: console snapshot, 1000 entries \u{2014} at the console cap: older entries \
             were dropped by the game",
        ));
}

#[test]
fn log_import_text_labels_a_snapshot_below_the_cap_as_partial() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "small.txt", 2);

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&snapshot);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "coverage: console snapshot, 2 entries",
        ))
        .stdout(predicates::str::contains(
            "says nothing about stages it does not show",
        ))
        .stdout(predicates::str::contains("at the console cap").not());
}

/// An uncleared console copy: the game logs its own banner through the
/// console queue, so the copy opens with the command line and the banner
/// entry, each followed by its copy stack trace, and only then the failures.
fn write_uncleared_console_copy(dir: &Path, name: &str) -> PathBuf {
    let trace = "UnityEngine.StackTraceUtility:ExtractStackTrace ()\nVerse.Log:Message (string)";
    let entry = |text: &str| format!("{text}\n{trace}\n\n");
    let text = [
        entry("Command line arguments: -example"),
        entry("RimWorld 1.6.4104 rev1234"),
        entry("[Example Mod] Patch operation Verse.PatchOperationAdd(Defs/ThingDef) failed\nfile: C:\\example\\patch.xml"),
    ]
    .concat();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

#[test]
fn log_import_reads_an_uncleared_console_copy_with_the_banner_near_the_top_as_a_snapshot() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let copy = write_uncleared_console_copy(dir.path(), "midgame1.txt");

    let json = import_json(&game, &[&copy], &[]);

    assert_eq!(json["kind"], "console_snapshot");
    assert_eq!(json["coverage"]["kind"], "console_snapshot");
    assert_eq!(json["coverage"]["entries"], 3);
    assert_eq!(json["coverage"]["console_fill"], "below_cap");
    assert_eq!(json["coverage"]["head_truncated"], false);
    assert!(
        json["totals"].get("passes").is_none(),
        "a snapshot states no startup passes: {}",
        json["totals"]
    );
}

#[test]
fn log_import_text_for_a_snapshot_states_its_scope_and_no_startup_passes() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let copy = write_uncleared_console_copy(dir.path(), "midgame1.txt");

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&copy);
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");

    assert!(output.status.success());
    assert!(
        stdout.contains("Console snapshot import (only the 3 entries the console held):"),
        "{stdout}"
    );
    assert!(stdout.contains("coverage: console snapshot, 3 entries"));
    assert!(
        !stdout.contains("startup pass"),
        "a snapshot's totals must not read as a whole session: {stdout}"
    );
}

#[test]
fn log_import_flags_a_snapshot_over_the_console_cap_and_one_starting_mid_entry() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let over_cap = write_snapshot(dir.path(), "pasted.txt", 1_001);
    let hand_cut = dir.path().join("cut.txt");
    std::fs::write(&hand_cut, "No stack trace.\n\nmessage\nNo stack trace.\n\n")
        .unwrap_or_else(|error| panic!("write fixture: {error}"));

    let json = import_json(&game, &[&over_cap, &hand_cut], &[]);

    assert_eq!(json[0]["coverage"]["console_fill"], "over_cap");
    assert_eq!(json[1]["coverage"]["head_truncated"], true);
    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&over_cap).arg(&hand_cut);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "more entries than one console holds",
        ))
        .stdout(predicates::str::contains(
            "the copy starts mid-entry (the first entry's text is missing)",
        ));
}

#[test]
fn log_import_reports_a_player_logs_coverage() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let json = import_json(&game, &[&player_log_fixture()], &[]);

    assert_eq!(json["kind"], "player_log");
    assert_eq!(json["coverage"]["kind"], "player_log");
    assert_eq!(json["coverage"]["passes"], 1);
    assert_eq!(
        json["totals"]["passes"], 1,
        "a Player.log keeps its startup passes"
    );
    assert_eq!(json["coverage"]["patch_phase"], "patch_failure_logged");
    assert!(json["coverage"]["end_state"]["state"].is_string());
    assert_eq!(json["logging_gaps"], Value::Array(Vec::new()));
}

#[test]
fn log_import_kind_override_wins_over_detection() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "copy.txt", 4);

    let as_player_log = import_json(&game, &[&snapshot], &["--kind", "player-log"]);
    let as_snapshot = import_json(
        &game,
        &[&player_log_fixture()],
        &["--kind", "console-snapshot"],
    );

    assert_eq!(as_player_log["kind"], "player_log");
    assert_eq!(as_player_log["totals"]["conserved"], true);
    assert_eq!(as_snapshot["kind"], "console_snapshot");
    assert_eq!(as_snapshot["totals"]["conserved"], true);
}

#[test]
fn log_import_text_says_when_the_kind_was_forced() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "copy.txt", 1);

    let mut cmd = common::rimmerge();
    cmd.arg("log")
        .arg("import")
        .arg(&snapshot)
        .args(["--kind", "console-snapshot"]);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("[kind forced by --kind]"));
}

#[test]
fn log_import_several_paths_print_a_json_array_with_each_path() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "midgame2.txt", 2);
    let player_log = player_log_fixture();

    let json = import_json(&game, &[&snapshot, &player_log], &[]);

    let files = json.as_array().expect("several paths give an array");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], snapshot.display().to_string());
    assert_eq!(files[0]["kind"], "console_snapshot");
    assert_eq!(files[1]["path"], player_log.display().to_string());
    assert_eq!(files[1]["kind"], "player_log");
}

#[test]
fn log_import_several_paths_print_one_labelled_block_each_in_text() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "midgame2.txt", 2);

    let mut cmd = common::rimmerge();
    cmd.arg("log")
        .arg("import")
        .arg(&snapshot)
        .arg(player_log_fixture());
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("==> "))
        .stdout(predicates::str::contains(
            "Console snapshot import (only the 2 entries the console held):",
        ))
        .stdout(predicates::str::contains("Player.log import:"));
}

#[test]
fn log_import_with_a_missing_path_among_several_fails_naming_it() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let snapshot = write_snapshot(dir.path(), "midgame2.txt", 2);
    let missing = dir.path().join("nope.txt");

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&snapshot).arg(&missing);
    path_args(&mut cmd, &game);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("nope.txt"));
}

#[test]
fn log_import_without_a_path_is_a_usage_error() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import");
    path_args(&mut cmd, &game);
    cmd.assert().failure();
}

/// A Player.log with two logging gaps, the second never resumed, and a
/// family that spans the first gap.
fn write_log_with_gaps(dir: &Path) -> PathBuf {
    let path = dir.join("gaps.log");
    let text = "RimWorld 1.6.4104 rev1234\n\
                [Example] noisy line\n\
                Reached max messages limit. Stopping logging to avoid spam.\n\
                Message logging is now once again on.\n\
                [Example] noisy line\n\
                Reached max messages limit. Stopping logging to avoid spam.\n";
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

#[test]
fn log_import_json_lists_the_logging_gaps_and_the_lower_bound_families() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_log_with_gaps(dir.path());

    let json = import_json(&game, &[&log], &[]);

    let gaps = json["logging_gaps"].as_array().expect("array");
    assert_eq!(gaps.len(), 2);
    assert_eq!(gaps[0]["stop_line"], 3);
    assert_eq!(gaps[0]["end"]["kind"], "resumed");
    assert_eq!(gaps[0]["end"]["line"], 4);
    assert_eq!(gaps[1]["end"]["kind"], "never_resumed");
    let lower_bound = json["lower_bound_families"].as_array().expect("array");
    assert_eq!(lower_bound.len(), 1, "{lower_bound:?}");
    assert_eq!(lower_bound[0]["class"], "mod_message");
}

#[test]
fn log_import_text_leads_with_the_gap_count() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_log_with_gaps(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log);
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");

    assert!(output.status.success());
    let first_line = stdout.lines().next().expect("a first line");
    assert!(
        first_line
            .starts_with("WARNING: the game stopped writing messages 2 times; 1 never resumed"),
        "{first_line}"
    );
    assert!(stdout.contains(
        "at least the counts of families spanning a gap are lower bounds, and any count may be short"
    ));
    assert!(
        stdout.contains("1 family, ") && stdout.contains("% of entries, span a logging gap"),
        "the note names the entry share the lower-bound families hold: {stdout}"
    );
}

/// A synthetic session log with a banner (so it is not taken for a console
/// copy) and two save-load blocks whose mod lists differ.
fn write_two_save_loads(dir: &Path) -> PathBuf {
    let path = dir.join("Player.log");
    std::fs::write(
        &path,
        "RimWorld 1.6.0 rev1\n\
         Loading game from file SaveA with mods:\n  - example.first\n  - example.second\n\
         some line\n\
         Loading game from file SaveB with mods:\n  - example.second\n",
    )
    .unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

#[test]
fn log_import_json_lists_every_save_load_event_and_flags_disagreement() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_two_save_loads(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log).arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let json: Value = serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    let events = json["load_events"].as_array().expect("array");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "save_load");
    assert_eq!(events[0]["save_name"], "SaveA");
    assert_eq!(events[0]["line"], 2);
    assert_eq!(events[1]["save_name"], "SaveB");
    assert_eq!(events[1]["line"], 6);
    assert_eq!(events[1]["mods"].as_array().expect("array").len(), 1);
    assert_eq!(json["load_events_disagree"], true);
}

#[test]
fn log_import_text_prints_one_line_per_load_event_and_the_disagreement() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_two_save_loads(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("2 load events"))
        .stdout(predicates::str::contains(
            "load event: save load \"SaveA\", line 2, 2 mods",
        ))
        .stdout(predicates::str::contains(
            // Anchored at the line end: "1 mods" would otherwise also match.
            "load event: save load \"SaveB\", line 6, 1 mod\n",
        ))
        .stdout(predicates::str::contains("mod lists differ"));
}

/// A banner line, a 200 KiB line (over the per-line bound), one line with
/// an invalid UTF-8 byte, and a load block after them.
fn write_log_needing_bounds(dir: &Path) -> PathBuf {
    let path = dir.join("Player.log");
    let mut bytes = b"RimWorld 1.6.0 rev1\n".to_vec();
    bytes.extend(std::iter::repeat_n(b'x', 200 * 1024));
    bytes.extend_from_slice(b"\nbad \xFF byte\n");
    bytes.extend_from_slice(b"Initializing new game with mods:\n  - example.first\n");
    std::fs::write(&path, bytes).unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

#[test]
fn log_import_json_reports_the_read_statistics() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_log_needing_bounds(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log).arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let json: Value = serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    let stats = &json["read_stats"];
    assert_eq!(stats["lines_read"], 5);
    assert_eq!(stats["lines_truncated"], 1);
    assert_eq!(stats["lines_with_invalid_utf8"], 1);
    assert_eq!(stats["stack_block_lines_dropped"], 0);
    assert_eq!(stats["stack_joins_abandoned"], 0);
    assert_eq!(json["load_events"].as_array().expect("array").len(), 1);
}

#[test]
fn log_import_text_names_the_bounds_it_applied() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_log_needing_bounds(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("read 5 lines; bounded:"))
        .stdout(predicates::str::is_match(
            r"(?m)^  read 5 lines; bounded: 1 line truncated to the first 64 KiB, 1 line with invalid UTF-8 \(decoded lossily\)$",
        )
        .expect("valid regex"));
}

#[test]
fn log_import_text_is_silent_about_bounds_for_an_ordinary_log() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(player_log_fixture());
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("bounded:").not());
}

#[test]
fn log_import_json_carries_the_totals_the_classes_and_the_sentinels() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log")
        .arg("import")
        .arg(player_log_fixture())
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let json: Value = serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    let totals = &json["totals"];
    assert_eq!(totals["conserved"], true, "totals: {totals}");
    assert_eq!(
        totals["lines_read"].as_u64().expect("number"),
        totals["entry_lines"].as_u64().expect("number")
            + totals["blank_separator_lines"].as_u64().expect("number")
    );
    let classes = json["classes"].as_array().expect("array");
    let patch_failures = classes
        .iter()
        .find(|class| class["class"] == "patch_failure")
        .expect("the fixture holds patch failures");
    assert_eq!(patch_failures["entries"], 7);
    assert!(patch_failures["families"][0]["key"].is_string());
    assert!(patch_failures["overflow"].is_null());
    assert_eq!(json["read_stats"]["passes_folded"], 0);
    assert_eq!(json["read_stats"]["stack_refs_dropped"], 0);
    // The fixture's `#` annotation lines quote log shapes in prose, which the
    // loose sentinels report as leaks; nothing else leaks.
    let leaks = json["sentinels"]["leaks"].as_array().expect("array");
    assert_eq!(json["sentinels"]["total"], leaks.len());
    assert!(
        leaks.iter().all(|leak| leak["text"]
            .as_str()
            .is_some_and(|text| text.starts_with('#'))),
        "leaks: {leaks:?}"
    );
}

#[test]
fn log_import_text_prints_the_totals_line_and_one_line_per_class() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(player_log_fixture());
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("totals: "))
        .stdout(predicates::str::contains(" startup passes"))
        .stdout(predicates::str::contains("classes:"))
        .stdout(predicates::str::contains("patch_failure: 7 entries in"))
        .stdout(predicates::str::contains("WARNING:"))
        .stdout(predicates::str::contains("loose pattern"));
}

/// A banner, then twelve distinct unclassified messages: `word_a` once,
/// `word_b` twice, and so on up to `word_l` twelve times.
fn write_log_with_twelve_unclassified_families(dir: &Path) -> PathBuf {
    let path = dir.join("Player.log");
    let mut text = String::from("RimWorld 1.6.0 rev1\n");
    for (index, letter) in ('a'..='l').enumerate() {
        for _ in 0..=index {
            text.push_str(&format!("plain words {letter}\n\n"));
        }
    }
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

#[test]
fn log_import_text_lists_only_the_ten_most_frequent_unclassified_families() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let log = write_log_with_twelve_unclassified_families(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log);
    path_args(&mut cmd, &game);
    let output = cmd.assert().success().get_output().stdout.clone();

    let text = String::from_utf8(output).expect("utf-8 output");
    let listed: Vec<&str> = text
        .lines()
        .skip_while(|line| *line != "top unclassified families:")
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect();
    assert_eq!(listed.len(), 10, "listed: {listed:?}");
    assert!(listed[0].contains("12 x plain words l"), "{listed:?}");
    assert!(listed[9].contains("3 x plain words c"), "{listed:?}");
}

#[test]
fn log_import_text_says_hits_and_that_only_the_first_twenty_are_listed() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let path = dir.path().join("Player.log");
    let mut text = String::from("RimWorld 1.6.0 rev1\n");
    for index in 0..25 {
        text.push_str(&format!("Adding duplicate Verse.ThingDef Foo{index}\n"));
    }
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("write fixture: {error}"));

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&path);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("25 sentinel hit(s)"))
        .stdout(predicates::str::contains("only the first 20 are listed"));
}

/// The synthetic assembly blobs `fixture gen` also uses (`Exports.dll` and
/// `ExportsSolo.dll`, named after their own assembly).
fn synthetic_assembly(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-resolve")
        .join("tests")
        .join("fixtures")
        .join("synthetic")
        .join("assemblies")
        .join("bin")
        .join(file_name)
}

/// Ships `file_name` in `<mod>/1.6/Assemblies/` of the scratch game.
fn ship_assembly(game: &ScratchGame, mod_folder: &str, file_name: &str) {
    let assemblies = game
        .game_dir
        .join("Mods")
        .join(mod_folder)
        .join("1.6")
        .join("Assemblies");
    std::fs::create_dir_all(&assemblies)
        .unwrap_or_else(|error| panic!("create Assemblies: {error}"));
    std::fs::copy(synthetic_assembly(file_name), assemblies.join(file_name))
        .unwrap_or_else(|error| panic!("copy {file_name}: {error}"));
}

/// A log whose families resolve every way: a `[Tag]` message naming a mod, a
/// cross-reference (no evidence), an exception thrown in a DLL both mods ship
/// (ambiguous), one thrown in a DLL only mod A ships, one in a DLL no mod
/// ships, and a type-load error naming the shared DLL.
fn write_log_for_family_attribution(dir: &Path) -> PathBuf {
    let path = dir.join("Player.log");
    std::fs::write(
        &path,
        "RimWorld 1.6.0 rev1\n\
         [Fixture Mod A] hello world\n\
         Could not resolve cross-reference to Verse.ThingDef named Foo (wanter=bar)\n\
         System.Exception: shared\n  at Exports.Thing.Run () in <b>:0\n\
         System.Exception: solo\n  at ExportsSolo.Thing.Run () in <b>:0\n\
         System.Exception: nowhere\n  at Nowhere.Thing.Run () in <b>:0\n\
         Exception loading Exports.dll: System.Exception: bad image\n",
    )
    .unwrap_or_else(|error| panic!("write fixture: {error}"));
    path
}

fn scratch_game_shipping_assemblies(dir: &Path) -> ScratchGame {
    let game = scratch_game(dir);
    ship_assembly(&game, "ModA", "Exports.dll");
    ship_assembly(&game, "ModB", "Exports.dll");
    ship_assembly(&game, "ModA", "ExportsSolo.dll");
    game
}

/// The `attribution` of the one family of `class` whose key contains `needle`.
fn family_attribution<'a>(json: &'a Value, class: &str, needle: &str) -> &'a Value {
    let class_json = json["classes"]
        .as_array()
        .unwrap_or_else(|| panic!("classes is an array in {json}"))
        .iter()
        .find(|entry| entry["class"] == class)
        .unwrap_or_else(|| panic!("no {class} class in {json}"));
    let matching: Vec<&Value> = class_json["families"]
        .as_array()
        .unwrap_or_else(|| panic!("families is an array in {class_json}"))
        .iter()
        .filter(|family| {
            family["key"]
                .as_str()
                .is_some_and(|key| key.contains(needle))
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "families of {class} matching {needle}: {matching:?}"
    );
    &matching[0]["attribution"]
}

#[test]
fn log_import_json_attributes_families_as_a_mod_nothing_or_several() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game_shipping_assemblies(dir.path());
    let log = write_log_for_family_attribution(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log).arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("must run");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let json: Value = serde_json::from_slice(&output.stdout).expect("output must be valid JSON");

    let tag = family_attribution(&json, "mod_message", "hello world");
    assert_eq!(tag["kind"], "mod", "{tag}");
    assert_eq!(tag["mod_name"], "Fixture Mod A");
    assert!(tag["mod_id"].is_string());
    let cross_reference = family_attribution(&json, "cross_reference", "Foo");
    assert!(cross_reference.is_null(), "no evidence: {cross_reference}");

    let solo = family_attribution(&json, "runtime_exception", "ExportsSolo.Thing");
    assert_eq!(solo["kind"], "mod", "{solo}");
    assert_eq!(solo["mod_name"], "Fixture Mod A");
    let nowhere = family_attribution(&json, "runtime_exception", "Nowhere.Thing");
    assert_eq!(nowhere["kind"], "unattributed", "{nowhere}");
    assert_eq!(nowhere["raw"], "Nowhere.Thing");

    for (class, needle, raw) in [
        ("runtime_exception", "|Exports.Thing", "Exports.Thing"),
        ("type_load_error", "Exports", "Exports"),
    ] {
        let shared = family_attribution(&json, class, needle);
        assert_eq!(shared["kind"], "ambiguous", "{class}: {shared}");
        assert_eq!(shared["raw"], raw);
        let names: Vec<&str> = shared["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .map(|candidate| candidate["mod_name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, ["Fixture Mod A", "Fixture Mod B"], "{class}");
    }
}

#[test]
fn log_import_text_prints_each_family_line_with_its_attribution() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game_shipping_assemblies(dir.path());
    let log = write_log_for_family_attribution(dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&log);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "1 x [Fixture Mod A] hello world -> Fixture Mod A\n",
        ))
        .stdout(predicates::str::contains(
            "-> (ambiguous: Exports.Thing; 2 mods: Fixture Mod A, Fixture Mod B)\n",
        ))
        .stdout(predicates::str::contains(
            "-> (unattributed: Nowhere.Thing)\n",
        ));
}

#[test]
fn log_import_text_lists_at_most_ten_gaps_and_counts_the_rest() {
    let dir = tempdir().expect("tempdir");
    let game = scratch_game(dir.path());
    let path = dir.path().join("many-gaps.log");
    let cycle = "Reached max messages limit. Stopping logging to avoid spam.\n\
                 Message logging is now once again on.\n";
    std::fs::write(
        &path,
        format!("RimWorld 1.6.4104 rev1234\n{}", cycle.repeat(12)),
    )
    .unwrap_or_else(|error| panic!("write fixture: {error}"));

    let mut cmd = common::rimmerge();
    cmd.arg("log").arg("import").arg(&path);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "stopped writing messages 12 times",
        ))
        .stdout(predicates::str::contains("... and 2 more"));
}
