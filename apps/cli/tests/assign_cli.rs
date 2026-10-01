//! End-to-end CLI tests for `assign propose/create/list/show/update/
//! set-row/clear-row/items/copy-from/coverage/export/delete`, against a
//! scratch copy of `rim-io`'s `assign_game` fixture — never the real
//! game install or `ModsConfig.xml`.
//!
//! `assign_game` (`crates/rim-io/tests/fixtures/assign_game`): three mods
//! under one `1.6` game — `fixture.framework` ships five
//! `example.PartAssignmentDef` instances (a `example.PartAssignmentDef`-shaped def: a
//! `speciesNames` target key, a `parts` item slot, an `enabled` scalar),
//! `fixture.parts` ships the five `example.PartDef` items those groups
//! reference, `fixture.target` ships five `ThingDef`s with a `<race>`
//! marker (`Race0`..`Race4`) for the target shape to learn from. Five of
//! each is deliberate, not padding: `MIN_RESOLVED_DISTINCT` (5) is the
//! reference-field gate every `TargetKey`/`ItemSlot` classification
//! must clear, so fewer instances
//! would silently reclassify `speciesNames`/`parts` as `Opaque` instead of
//! exercising the classification this test suite is actually for.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::tempdir;

mod common;
use common::copy_dir_recursive;

/// `crates/rim-io/tests/fixtures/assign_game`.
fn assign_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-io")
        .join("tests")
        .join("fixtures")
        .join("assign_game")
}

/// A scratch copy of `assign_game` under `temp_dir`, plus the four path
/// args every command in this file needs.
struct ScratchGame {
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

fn scratch_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    copy_dir_recursive(&assign_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy assign_game fixture: {error}"));
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

const PACKAGE_ID: &str = "mypatch.partassign";
const FOLDER_NAME: &str = "mypatch_partassign";
const DEF_TYPE: &str = "example.PartAssignmentDef";
const REFS: &str = "fixture.framework,fixture.parts";
const TARGETS: &str = "fixture.target";

/// Not a test function itself (so `clippy::expect_used`'s test allowance
/// doesn't apply here) — every fallible step uses `unwrap_or_else(|e|
/// panic!(...))` instead, exactly like `patch_cli.rs`'s own `patch_new`.
fn assign_create(game: &ScratchGame) -> String {
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("create")
        .arg("--name")
        .arg("Part Patch")
        .arg("--package-id")
        .arg(PACKAGE_ID)
        .arg("--display-name")
        .arg("Sample Part Patch")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg(TARGETS)
        .arg("--def-type")
        .arg(DEF_TYPE);
    path_args(&mut cmd, game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout =
        String::from_utf8(output).unwrap_or_else(|error| panic!("stdout must be UTF-8: {error}"));
    stdout
        .trim()
        .strip_prefix("created assignment ")
        .unwrap_or_else(|| panic!("unexpected `assign create` output: {stdout:?}"))
        .to_string()
}

#[test]
fn propose_phase_1_lists_the_candidate_type_with_its_count_and_owners() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // No `--def-type`: the cheap phase-1 listing (type, instance count,
    // owners) — no instance reads, so no `TargetKey(ThingDef)`-shaped role
    // text appears at all yet.
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("propose")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg(TARGETS);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(DEF_TYPE))
        .stdout(predicates::str::contains("fixture.framework"))
        .stdout(predicates::str::contains("TargetKey").not());
}

#[test]
fn propose_phase_2_with_def_type_prints_the_inferred_schema() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("propose")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg(TARGETS)
        .arg("--def-type")
        .arg(DEF_TYPE);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(DEF_TYPE))
        .stdout(predicates::str::contains("TargetKey(ThingDef)"))
        .stdout(predicates::str::contains("ItemSlot(example.PartDef)"));
}

#[test]
fn propose_json_with_def_type_carries_the_target_shapes_required_children() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("propose")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg(TARGETS)
        .arg("--def-type")
        .arg(DEF_TYPE)
        .arg("--json");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("propose --json must be valid JSON");
    let fields = json[0]["fields"]
        .as_array()
        .expect("candidate must carry a fields array");
    let race_names = fields
        .iter()
        .find(|field| field["path"] == "speciesNames")
        .expect("speciesNames must be a field");
    assert_eq!(race_names["role"], "TargetKey(ThingDef)");
    assert_eq!(race_names["required_children"][0], "race");
}

#[test]
fn create_then_list_shows_the_created_assignment() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign").arg("list");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(&id))
        .stdout(predicates::str::contains(PACKAGE_ID));
}

#[test]
fn create_with_an_unknown_def_type_fails() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("create")
        .arg("--name")
        .arg("n")
        .arg("--package-id")
        .arg(PACKAGE_ID)
        .arg("--display-name")
        .arg("d")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg(TARGETS)
        .arg("--def-type")
        .arg("NotARealDefType");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no candidate def type"));
}

/// Not a test function itself — see [`assign_create`]'s own doc comment.
fn assign_set_row(game: &ScratchGame, id: &str) {
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(id)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("mypatch_partassign_Race0")
        .arg("--value")
        .arg("parts=names:Part1")
        .arg("--value")
        .arg("enabled=text:false");
    path_args(&mut cmd, game);
    cmd.assert().success();
}

#[test]
fn set_row_then_show_lists_the_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);

    let mut cmd = common::rimmerge();
    cmd.arg("assign").arg("show").arg("--assignment").arg(&id);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("ThingDef/Race0"))
        .stdout(predicates::str::contains("mypatch_partassign_Race0"));
}

#[test]
fn show_json_carries_the_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg(&id)
        .arg("--json");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("show --json must be valid JSON");
    assert_eq!(json["sections"][0]["rows"][0]["def_name"], "Race0");
    assert_eq!(
        json["sections"][0]["rows"][0]["row_def_name"],
        "mypatch_partassign_Race0"
    );
}

#[test]
fn clear_row_removes_a_previously_set_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("clear-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("cleared row"));

    let mut show_cmd = common::rimmerge();
    show_cmd
        .arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut show_cmd, &game);
    show_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("rows (0):"));
}

#[test]
fn items_pages_across_the_five_parts() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("items")
        .arg("example.PartDef")
        .arg("--offset")
        .arg("1")
        .arg("--limit")
        .arg("1");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("5 total"))
        .stdout(predicates::str::contains("Part1"))
        .stdout(predicates::str::contains("EffectBeta"))
        .stdout(predicates::str::contains("Part0").not());
}

#[test]
fn items_search_filters_before_paging() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("items")
        .arg("example.PartDef")
        .arg("--search")
        .arg("part3")
        .arg("--json");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("items --json must be valid JSON");
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["def_name"], "Part3");
}

#[test]
fn copy_from_builds_a_row_from_an_existing_instance() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    // The framework's own `Group_R1` instance is an existing active
    // `example.PartAssignmentDef` instance to copy from — the fixture's own
    // reference data doubles as this.
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("copy-from")
        .arg("--assignment")
        .arg(&id)
        .arg("--target")
        .arg("ThingDef/Race2")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--source")
        .arg("Group_R1")
        .arg("--json");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("copy-from --json must be valid JSON");
    assert_eq!(json["def_name"], "Race2");
    assert_eq!(json["row_def_name"], "mypatch_partassign_Race2");
}

#[test]
fn update_shrinking_targets_drops_the_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("update")
        .arg("--assignment")
        .arg(&id)
        .arg("--targets")
        .arg("");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("dropped 1 row(s)"))
        .stdout(predicates::str::contains("ThingDef/Race0"));

    let mut show_cmd = common::rimmerge();
    show_cmd
        .arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut show_cmd, &game);
    show_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("rows (0):"));
}

#[test]
fn coverage_lists_every_candidate_target_as_an_override() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("coverage")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut cmd, &game);

    // Every race is already named by the framework's own five reference
    // instances (`Group_R0`..`Group_R4`), so every candidate starts out
    // an override, not fresh coverage.
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("ThingDef/Race0"))
        .stdout(predicates::str::contains("Override"));
}

/// Every file under `dir`, as `(relative path, content bytes)`, sorted for
/// a deterministic byte-for-byte comparison across two independent
/// exports of the same target.
fn snapshot_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}")) {
            let entry = entry.unwrap_or_else(|e| panic!("dir entry under {dir:?}: {e}"));
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or_else(|e| panic!("{path:?} must be under {root:?}: {e}"))
                    .to_path_buf();
                let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
                out.push((relative, bytes));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Not a test function itself — see [`assign_create`]'s own doc comment.
fn assign_export(game: &ScratchGame, id: &str, out_dir: &Path) {
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("export")
        .arg("--assignment")
        .arg(id)
        .arg("--out")
        .arg(out_dir);
    path_args(&mut cmd, game);
    cmd.assert().success();
}

#[test]
fn export_writes_the_expected_files_and_gates_by_the_items_owner() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);
    let out_dir = temp_dir.path().join("export");

    assign_export(&game, &id, &out_dir);

    let mod_dir = out_dir.join(FOLDER_NAME);
    let about = fs::read_to_string(mod_dir.join("About").join("About.xml"))
        .expect("About.xml must have been written");
    assert!(about.contains(PACKAGE_ID));
    // `parts=Part1` is owned by `fixture.parts`; the target itself is
    // owned by `fixture.target` — both must be declared, `fixture.framework`
    // (a pure reference, referenced by no chosen row value) must not be.
    assert!(about.contains("fixture.parts"));
    assert!(about.contains("fixture.target"));
    assert!(!about.contains("fixture.framework"));

    let defs_xml = fs::read_to_string(
        mod_dir
            .join("Defs")
            .join(format!("rimmerge_{DEF_TYPE}.xml")),
    )
    .expect("the rendered Defs file must have been written");
    assert!(defs_xml.contains("mypatch_partassign_Race0"));
    assert!(defs_xml.contains("Part1"));
}

#[test]
fn exporting_the_same_rows_twice_is_byte_identical() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);
    let out_dir = temp_dir.path().join("export");
    let mod_dir = out_dir.join(FOLDER_NAME);

    assign_export(&game, &id, &out_dir);
    let first_snapshot = snapshot_files(&mod_dir);

    assign_export(&game, &id, &out_dir);
    let second_snapshot = snapshot_files(&mod_dir);

    assert_eq!(
        first_snapshot, second_snapshot,
        "exporting the same rows twice must be byte-identical"
    );
}

#[test]
fn export_into_the_games_mods_folder_is_refused() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);
    assign_set_row(&game, &id);
    let mods_subfolder = game.game_dir.join("Mods").join("wherever");

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("export")
        .arg("--assignment")
        .arg(&id)
        .arg("--out")
        .arg(&mods_subfolder);
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("OutDirIsModsFolder"));
}

#[test]
fn delete_removes_the_assignment_from_the_list() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut delete_cmd = common::rimmerge();
    delete_cmd
        .arg("assign")
        .arg("delete")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut delete_cmd, &game);
    delete_cmd.assert().success();

    let mut list_cmd = common::rimmerge();
    list_cmd.arg("assign").arg("list");
    path_args(&mut list_cmd, &game);
    list_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("no assignments on this profile"));
}

#[test]
fn an_unknown_assignment_id_exits_1_with_not_found() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg("abcdef012345");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn set_row_with_a_non_finite_chance_is_rejected_by_the_domain() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    // `chanceParts` is this fixture's own `Chances` field (paired with the
    // `parts` slot by name). The CLI's own `parse_row_value` lets `NaN`
    // through unchanged
    // (it's a plain, valid `f64` as far as parsing goes); it's
    // `AssignmentProject::set_row` (the domain) that refuses it, per
    // `AssignmentRowError::NonFiniteChance` — this test pins that the
    // rejection happens there, not silently earlier or not at all.
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("mypatch_partassign_Race0")
        .arg("--value")
        .arg("parts=names:Part0")
        .arg("--value")
        .arg("chanceParts=numbers:NaN");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("non-finite"))
        .stderr(predicates::str::contains("chanceParts"));
}

#[test]
fn set_row_with_negative_zero_chance_is_accepted() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("mypatch_partassign_Race0")
        .arg("--value")
        .arg("parts=names:Part0")
        .arg("--value")
        .arg("chanceParts=numbers:-0");
    path_args(&mut cmd, &game);

    // `-0.0` is finite — accepted like any other legitimate chance value.
    cmd.assert().success();
}

// ---------------------------------------------------------------------
// Standalone ("new def") projects — a "good to have" extension: a
// candidate with no `TargetKey` field is only offered when `--targets` is
// empty, and its rows are free-standing instances addressed by their own
// `defName` rather than a `<def_type>/<def_name>` target. `example.PartDef`
// (owned by `fixture.parts`) has only a scalar `effect` field — nothing
// resolves to a def outside R, so it never gets a `TargetKey` field at
// all, exactly the shape this extension is for.
// ---------------------------------------------------------------------

const STANDALONE_PACKAGE_ID: &str = "sample.newpart";
const STANDALONE_FOLDER_NAME: &str = "sample_newpart";
const STANDALONE_DEF_TYPE: &str = "example.PartDef";
const STANDALONE_REFS: &str = "fixture.parts";

/// Not a test function itself — see [`assign_create`]'s own doc comment.
fn standalone_create(game: &ScratchGame) -> String {
    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("create")
        .arg("--name")
        .arg("New Part")
        .arg("--package-id")
        .arg(STANDALONE_PACKAGE_ID)
        .arg("--display-name")
        .arg("Sample's New Part")
        .arg("--refs")
        .arg(STANDALONE_REFS)
        .arg("--targets")
        .arg("")
        .arg("--def-type")
        .arg(STANDALONE_DEF_TYPE);
    path_args(&mut cmd, game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout =
        String::from_utf8(output).unwrap_or_else(|error| panic!("stdout must be UTF-8: {error}"));
    stdout
        .trim()
        .strip_prefix("created assignment ")
        .unwrap_or_else(|| panic!("unexpected `assign create` output: {stdout:?}"))
        .to_string()
}

#[test]
fn propose_phase_1_lists_a_no_target_key_candidate_when_targets_is_empty() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("propose")
        .arg("--refs")
        .arg(STANDALONE_REFS)
        .arg("--targets")
        .arg("");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(STANDALONE_DEF_TYPE))
        .stdout(predicates::str::contains("fixture.parts"));
}

#[test]
fn create_a_standalone_project_with_no_targets_needs_no_target_set() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let id = standalone_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign").arg("list");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(&id))
        .stdout(predicates::str::contains(STANDALONE_PACKAGE_ID));
}

#[test]
fn set_free_standing_row_then_show_lists_it_and_coverage_is_not_applicable() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = standalone_create(&game);

    let mut set_cmd = common::rimmerge();
    set_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-name")
        .arg("sample_newpart_Base")
        .arg("--value")
        .arg("effect=text:EffectZeta");
    path_args(&mut set_cmd, &game);
    set_cmd.assert().success();

    let mut show_cmd = common::rimmerge();
    show_cmd
        .arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut show_cmd, &game);
    show_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("standalone rows (1):"))
        .stdout(predicates::str::contains("sample_newpart_Base"));

    let mut coverage_cmd = common::rimmerge();
    coverage_cmd
        .arg("assign")
        .arg("coverage")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut coverage_cmd, &game);
    coverage_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("not applicable"));
}

#[test]
fn exports_a_standalone_project_with_no_may_require() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = standalone_create(&game);

    let mut set_cmd = common::rimmerge();
    set_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-name")
        .arg("sample_newpart_Base")
        .arg("--value")
        .arg("effect=text:EffectZeta");
    path_args(&mut set_cmd, &game);
    set_cmd.assert().success();

    let out_dir = temp_dir.path().join("export");
    assign_export(&game, &id, &out_dir);

    let mod_dir = out_dir.join(STANDALONE_FOLDER_NAME);
    let defs_xml = fs::read_to_string(
        mod_dir
            .join("Defs")
            .join(format!("rimmerge_{STANDALONE_DEF_TYPE}.xml")),
    )
    .expect("the rendered Defs file must have been written");
    assert!(defs_xml.contains("sample_newpart_Base"));
    assert!(defs_xml.contains("EffectZeta"));
    assert!(
        !defs_xml.contains("MayRequire"),
        "a standalone row has no target to gate against: {defs_xml}"
    );
}

#[test]
fn clear_free_standing_row_removes_a_previously_set_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = standalone_create(&game);

    let mut set_cmd = common::rimmerge();
    set_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-name")
        .arg("sample_newpart_Base")
        .arg("--value")
        .arg("effect=text:EffectZeta");
    path_args(&mut set_cmd, &game);
    set_cmd.assert().success();

    let mut clear_cmd = common::rimmerge();
    clear_cmd
        .arg("assign")
        .arg("clear-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-name")
        .arg("sample_newpart_Base");
    path_args(&mut clear_cmd, &game);
    clear_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("cleared row"));

    let mut show_cmd = common::rimmerge();
    show_cmd
        .arg("assign")
        .arg("show")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut show_cmd, &game);
    show_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("standalone rows (0):"));
}

// ---------------------------------------------------------------------
// Multi-section projects — a project owning both a free-standing
// `example.PartDef` section and a target-keyed `example.PartAssignmentDef`
// section, the latter
// referencing the former's own row. Mirrors
// `crates/rim-io/tests/assignment_multi_section_end_to_end.rs`'s own flow
// (see that file's doc comment for the exact fixture def names and the
// ordering constraint — a no-`TargetKey` candidate is only offered while
// `--targets` is empty, which is why T is widened only after the
// free-standing section already exists), entirely through the CLI
// binary.
// ---------------------------------------------------------------------

const MULTI_PACKAGE_ID: &str = "sample.multisection";
const MULTI_FOLDER_NAME: &str = "sample_multisection";
const MULTI_PART_DEF_TYPE: &str = "example.PartDef";
const MULTI_RACE_DEF_TYPE: &str = "example.PartAssignmentDef";
const MULTI_OWN_PART_NAME: &str = "sample_multisection_OwnPart";
const MULTI_RACE_ROW_DEF_NAME: &str = "sample_multisection_Group_Race0";

/// Builds a two-section project: `example.PartDef` free-standing (one own
/// row, `MULTI_OWN_PART_NAME`), created first while `--targets` is empty
/// (a no-`TargetKey` candidate is only offered then); then T is widened
/// to `TARGETS` and `example.PartAssignmentDef` is added as a target-keyed
/// section via `add-section`, with one row on `ThingDef/Race0` naming the
/// free-standing part in its own `parts` slot. Not a test function itself
/// — see [`assign_create`]'s own doc comment.
fn multi_section_project(game: &ScratchGame) -> String {
    let mut create_cmd = common::rimmerge();
    create_cmd
        .arg("assign")
        .arg("create")
        .arg("--name")
        .arg("Multi-section race patch")
        .arg("--package-id")
        .arg(MULTI_PACKAGE_ID)
        .arg("--display-name")
        .arg("Sample's Multi-Section Patch")
        .arg("--refs")
        .arg(REFS)
        .arg("--targets")
        .arg("")
        .arg("--def-type")
        .arg(MULTI_PART_DEF_TYPE);
    path_args(&mut create_cmd, game);
    let output = create_cmd.assert().success().get_output().stdout.clone();
    let stdout =
        String::from_utf8(output).unwrap_or_else(|error| panic!("stdout must be UTF-8: {error}"));
    let id = stdout
        .trim()
        .strip_prefix("created assignment ")
        .unwrap_or_else(|| panic!("unexpected `assign create` output: {stdout:?}"))
        .to_string();

    let mut set_own_cmd = common::rimmerge();
    set_own_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-name")
        .arg(MULTI_OWN_PART_NAME);
    path_args(&mut set_own_cmd, game);
    set_own_cmd.assert().success();

    let mut update_cmd = common::rimmerge();
    update_cmd
        .arg("assign")
        .arg("update")
        .arg("--assignment")
        .arg(&id)
        .arg("--targets")
        .arg(TARGETS);
    path_args(&mut update_cmd, game);
    update_cmd.assert().success();

    let mut add_section_cmd = common::rimmerge();
    add_section_cmd
        .arg("assign")
        .arg("add-section")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-type")
        .arg(MULTI_RACE_DEF_TYPE);
    path_args(&mut add_section_cmd, game);
    add_section_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "added section example.PartAssignmentDef (target-keyed)",
        ));

    let mut set_target_cmd = common::rimmerge();
    set_target_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_RACE_DEF_TYPE)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg(MULTI_RACE_ROW_DEF_NAME)
        .arg("--value")
        .arg(format!("parts=names:{MULTI_OWN_PART_NAME}"));
    path_args(&mut set_target_cmd, game);
    set_target_cmd.assert().success();

    id
}

#[test]
fn multi_section_export_renders_both_sections_with_no_self_dependency() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    // Strengthen the row built by `multi_section_project` to name a real
    // *external* item (`Part0`, owned by `fixture.parts`) alongside the
    // free-standing own one, so `modDependencies` has a genuine candidate
    // the own-package exclusion must skip *past* — not merely an entry
    // that happens to be empty (see
    // `an_external_item_of_the_same_type_as_a_free_standing_section_renders_and_is_not_skipped`
    // below for the single-reference case proven end to end).
    let mut widen_row_cmd = common::rimmerge();
    widen_row_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_RACE_DEF_TYPE)
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg(MULTI_RACE_ROW_DEF_NAME)
        .arg("--value")
        .arg(format!("parts=names:{MULTI_OWN_PART_NAME},Part0"));
    path_args(&mut widen_row_cmd, &game);
    widen_row_cmd.assert().success();

    let out_dir = temp_dir.path().join("export");
    assign_export(&game, &id, &out_dir);

    let mod_dir = out_dir.join(MULTI_FOLDER_NAME);
    assert!(
        mod_dir
            .join("Defs")
            .join(format!("rimmerge_{MULTI_PART_DEF_TYPE}.xml"))
            .exists(),
        "the free-standing section must render its own file"
    );
    let race_group_xml = fs::read_to_string(
        mod_dir
            .join("Defs")
            .join(format!("rimmerge_{MULTI_RACE_DEF_TYPE}.xml")),
    )
    .expect("the rendered PartAssignmentDef file must have been written");
    assert!(
        race_group_xml.contains(MULTI_OWN_PART_NAME),
        "the row must name the free-standing part by its own defName: {race_group_xml}"
    );
    assert!(
        race_group_xml.contains("Part0"),
        "the row must also render the external item alongside the own one: {race_group_xml}"
    );

    let about = fs::read_to_string(mod_dir.join("About").join("About.xml"))
        .expect("About.xml must have been written");
    let deps_section =
        &about[about.find("<modDependencies>").unwrap()..about.find("</modDependencies>").unwrap()];
    let load_after_section =
        &about[about.find("<loadAfter>").unwrap()..about.find("</loadAfter>").unwrap()];
    // The real, non-vacuous assertion: `fixture.parts` (Part0's own
    // owner) is a genuine `modDependencies` entry the own-package
    // exclusion had to skip *past* — proving that exclusion is selective,
    // not a check that happens to pass because the element is empty.
    assert!(
        deps_section.contains("fixture.parts"),
        "modDependencies must name Part0's own owner: {deps_section}"
    );
    assert!(
        !deps_section.contains(MULTI_PACKAGE_ID),
        "modDependencies must never name this project's own package id: {deps_section}"
    );
    assert!(
        !load_after_section.contains(MULTI_PACKAGE_ID),
        "loadAfter must never name this project's own package id: {load_after_section}"
    );
}

/// Once a project has a free-standing section for item type `X` (here
/// `example.PartDef`), an `ItemSlot` reference to a genuinely active,
/// *external* `X` instance — from any row, not only one also naming an
/// own `X` row — renders exactly like any other item slot value: a
/// real `fixture.parts`-owned `Part0`, named by a row with **no** own-`X`
/// reference at all, is rendered, not skipped, and export reports no skip
/// for it.
#[test]
fn an_external_item_of_the_same_type_as_a_free_standing_section_renders_and_is_not_skipped() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut set_external_cmd = common::rimmerge();
    set_external_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_RACE_DEF_TYPE)
        .arg("--target")
        .arg("ThingDef/Race1")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("sample_multisection_Group_Race1")
        .arg("--value")
        // `Part0` alone — a real, active `fixture.parts` instance, never
        // this project's own row — is the whole case; no mixing with
        // `MULTI_OWN_PART_NAME` is needed.
        .arg("parts=names:Part0");
    path_args(&mut set_external_cmd, &game);
    set_external_cmd.assert().success();

    let out_dir = temp_dir.path().join("export");
    let mut export_cmd = common::rimmerge();
    export_cmd
        .arg("assign")
        .arg("export")
        .arg("--assignment")
        .arg(&id)
        .arg("--out")
        .arg(&out_dir);
    path_args(&mut export_cmd, &game);
    // No skip at all for this field — `export` prints no
    // "skipped N field(s):" heading whatsoever once `outcome.skipped` is
    // empty (`commands/assign.rs`'s own `run_export`), so the absence of
    // both the heading and any skip reason text is the correct assertion,
    // not a "skipped 0" line that command never prints.
    export_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("skipped").not())
        .stdout(predicates::str::contains("no longer exist as a known own instance").not());

    let race_group_xml = fs::read_to_string(
        out_dir
            .join(MULTI_FOLDER_NAME)
            .join("Defs")
            .join(format!("rimmerge_{MULTI_RACE_DEF_TYPE}.xml")),
    )
    .expect("the rendered PartAssignmentDef file must have been written");
    assert!(
        race_group_xml.contains("Part0"),
        "the external reference must now be rendered, not skipped: {race_group_xml}"
    );
}

#[test]
fn add_section_refuses_an_unowned_type_and_a_duplicate() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut unowned_cmd = common::rimmerge();
    unowned_cmd
        .arg("assign")
        .arg("add-section")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-type")
        .arg("nobody.owns.This");
    path_args(&mut unowned_cmd, &game);
    unowned_cmd
        .assert()
        .failure()
        .stderr(predicates::str::contains("not owned"));

    let mut duplicate_cmd = common::rimmerge();
    duplicate_cmd
        .arg("assign")
        .arg("add-section")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-type")
        .arg(DEF_TYPE);
    path_args(&mut duplicate_cmd, &game);
    duplicate_cmd
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[test]
fn remove_section_refuses_when_referenced_and_force_removes_it() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut refuse_cmd = common::rimmerge();
    refuse_cmd
        .arg("assign")
        .arg("remove-section")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-type")
        .arg(MULTI_PART_DEF_TYPE);
    path_args(&mut refuse_cmd, &game);
    refuse_cmd
        .assert()
        .failure()
        // The whole referencing-row line, not just the def type — pins
        // `format_row_key` against regressing to `{key:?}` (which would
        // still pass a bare `contains(MULTI_RACE_DEF_TYPE)` check).
        .stderr(predicates::str::contains(
            "example.PartAssignmentDef ThingDef/Race0 parts",
        ))
        .stderr(predicates::str::contains("--force"));

    let mut force_cmd = common::rimmerge();
    force_cmd
        .arg("assign")
        .arg("remove-section")
        .arg("--assignment")
        .arg(&id)
        .arg("--def-type")
        .arg(MULTI_PART_DEF_TYPE)
        .arg("--force");
    path_args(&mut force_cmd, &game);
    force_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("removed section example.PartDef"));
}

#[test]
fn set_row_without_section_on_a_two_section_project_names_both_sections() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut ambiguous_cmd = common::rimmerge();
    ambiguous_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--target")
        .arg("ThingDef/Race1")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("sample_multisection_Group_Race1")
        .arg("--value")
        .arg(format!("parts=names:{MULTI_OWN_PART_NAME}"));
    path_args(&mut ambiguous_cmd, &game);
    ambiguous_cmd
        .assert()
        .failure()
        .stderr(predicates::str::contains(MULTI_PART_DEF_TYPE))
        .stderr(predicates::str::contains(MULTI_RACE_DEF_TYPE))
        .stderr(predicates::str::contains("--section"));

    let mut with_section_cmd = common::rimmerge();
    with_section_cmd
        .arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_RACE_DEF_TYPE)
        .arg("--target")
        .arg("ThingDef/Race1")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("sample_multisection_Group_Race1")
        .arg("--value")
        .arg(format!("parts=names:{MULTI_OWN_PART_NAME}"));
    path_args(&mut with_section_cmd, &game);
    with_section_cmd.assert().success();
}

#[test]
fn items_with_assignment_lists_the_projects_own_row_first_as_this_project() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut text_cmd = common::rimmerge();
    text_cmd
        .arg("assign")
        .arg("items")
        .arg(MULTI_PART_DEF_TYPE)
        .arg("--assignment")
        .arg(&id);
    path_args(&mut text_cmd, &game);
    let output = text_cmd.assert().success().get_output().stdout.clone();
    let stdout =
        String::from_utf8(output).unwrap_or_else(|error| panic!("stdout must be UTF-8: {error}"));
    let own_pos = stdout
        .find(MULTI_OWN_PART_NAME)
        .expect("the project's own row must be listed");
    let active_pos = stdout
        .find("Part0")
        .expect("the active list's own items must still be listed");
    assert!(
        own_pos < active_pos,
        "the project's own row must be listed first: {stdout}"
    );
    assert!(stdout.contains("this project"));

    let mut json_cmd = common::rimmerge();
    json_cmd
        .arg("assign")
        .arg("items")
        .arg(MULTI_PART_DEF_TYPE)
        .arg("--assignment")
        .arg(&id)
        .arg("--json");
    path_args(&mut json_cmd, &game);
    let output = json_cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("items --json must be valid JSON");
    assert_eq!(json["items"][0]["def_name"], MULTI_OWN_PART_NAME);
    assert_eq!(json["items"][0]["own"], true);
    assert_eq!(json["items"][1]["own"], false);
}

#[test]
fn coverage_with_section_targets_the_named_section() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut race_cmd = common::rimmerge();
    race_cmd
        .arg("assign")
        .arg("coverage")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_RACE_DEF_TYPE);
    path_args(&mut race_cmd, &game);
    race_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("ThingDef/Race0"));

    let mut part_cmd = common::rimmerge();
    part_cmd
        .arg("assign")
        .arg("coverage")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_PART_DEF_TYPE);
    path_args(&mut part_cmd, &game);
    part_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("not applicable"));
}

#[test]
fn show_lists_both_sections() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign").arg("show").arg("--assignment").arg(&id);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "sections (2): example.PartAssignmentDef,example.PartDef",
        ))
        .stdout(predicates::str::contains(
            "section: example.PartDef (free-standing)",
        ))
        .stdout(predicates::str::contains(
            "section: example.PartAssignmentDef (target-keyed)",
        ));
}

#[test]
fn resolve_section_names_the_actual_sections_on_an_unknown_def_type() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("set-row")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg("nobody.owns.This")
        .arg("--target")
        .arg("ThingDef/Race0")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--def-name")
        .arg("mypatch_partassign_Race0");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("nobody.owns.This"))
        .stderr(predicates::str::contains(DEF_TYPE));
}

#[test]
fn clear_row_with_neither_addressing_mode_fails() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = assign_create(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("clear-row")
        .arg("--assignment")
        .arg(&id);
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("specify either"));
}

#[test]
fn copy_from_refuses_a_free_standing_section() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let id = multi_section_project(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("assign")
        .arg("copy-from")
        .arg("--assignment")
        .arg(&id)
        .arg("--section")
        .arg(MULTI_PART_DEF_TYPE)
        .arg("--target")
        .arg("ThingDef/Race1")
        .arg("--key-field")
        .arg("speciesNames")
        .arg("--source")
        .arg("Part0");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains(MULTI_PART_DEF_TYPE))
        .stderr(predicates::str::contains("free-standing"));
}
