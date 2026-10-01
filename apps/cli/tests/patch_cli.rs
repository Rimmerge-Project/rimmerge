//! End-to-end CLI tests for `patch new/list/plan/decide/export/delete`,
//! against a scratch copy of `rim-io`'s `merge_game` fixture — never the
//! real game install or `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

/// A scratch copy of `merge_game` under `temp_dir`, plus the four path
/// args every command in this file needs.
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

const PACKAGE_ID: &str = "sample.abcompat";
const FOLDER_NAME: &str = "sample_abcompat";

/// Not a test function itself (so `clippy::expect_used`'s test allowance
/// doesn't apply here) — every fallible step uses `unwrap_or_else(|e|
/// panic!(...))` instead, exactly like `scratch_game`/`snapshot_files`.
fn patch_new(game: &ScratchGame) -> String {
    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("new")
        .arg("--name")
        .arg("AB compat")
        .arg("--package-id")
        .arg(PACKAGE_ID)
        .arg("--display-name")
        .arg("A + B Compatibility")
        .arg("--scope")
        .arg("fixture.moda,fixture.modb");
    path_args(&mut cmd, game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout =
        String::from_utf8(output).unwrap_or_else(|error| panic!("stdout must be UTF-8: {error}"));
    stdout
        .trim()
        .strip_prefix("created patch ")
        .unwrap_or_else(|| panic!("unexpected `patch new` output: {stdout:?}"))
        .to_string()
}

#[test]
fn new_then_list_shows_the_created_patch() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("patch").arg("list");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(patch_id))
        .stdout(predicates::str::contains(PACKAGE_ID));
}

#[test]
fn plan_prints_the_scoped_two_owner_diff() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("plan")
        .arg("--patch")
        .arg(&patch_id)
        .arg("--key")
        .arg("def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "def_override:ThingDef/Fixture_Wall",
        ))
        .stdout(predicates::str::contains("winner=fixture.modb"))
        .stdout(predicates::str::contains("statBases/MaxHitPoints"));
}

/// Not a test function itself — see [`patch_new`]'s own doc comment.
fn patch_decide(game: &ScratchGame, patch_id: &str) {
    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("decide")
        .arg("--patch")
        .arg(patch_id)
        .arg("--key")
        .arg("def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]")
        .arg("--choice")
        .arg("statBases/MaxHitPoints=from:fixture.moda");
    path_args(&mut cmd, game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Complete"));
}

#[test]
fn decide_then_export_writes_the_expected_files() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);
    patch_decide(&game, &patch_id);
    let out_dir = temp_dir.path().join("export");

    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("export")
        .arg("--patch")
        .arg(&patch_id)
        .arg("--out")
        .arg(&out_dir);
    path_args(&mut cmd, &game);

    cmd.assert().success();

    let mod_dir = out_dir.join(FOLDER_NAME);
    let about = fs::read_to_string(mod_dir.join("About").join("About.xml"))
        .expect("About.xml must have been written");
    assert!(
        about.contains(PACKAGE_ID),
        "About.xml must contain the package id: {about}"
    );
    let patches_xml = fs::read_to_string(mod_dir.join("Patches").join("rimmerge_ThingDef.xml"))
        .expect("the ThingDef patch file must have been written");
    assert!(
        patches_xml.contains("MaxHitPoints"),
        "the resolved field must land in the patch file: {patches_xml}"
    );
}

/// `--out` must be resolved (absolutized,
/// and canonicalized when it already exists) by the interface before it
/// reaches `ExportPatch`, which refuses a relative `out_dir` outright
/// (`ExportPatchError::OutDirNotAbsolute`) — see
/// `crates/rim-session/src/use_cases/export_patch.rs`'s own doc comment.
/// Run from a temp cwd distinct from the scratch game tree so a relative
/// `./export` resolves somewhere real and disposable either way.
#[test]
fn export_with_a_relative_out_dir_succeeds_from_a_temp_cwd() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);
    patch_decide(&game, &patch_id);
    let cwd = tempdir().expect("separate tempdir for the relative --out to resolve against");

    let mut cmd = common::rimmerge();
    cmd.current_dir(cwd.path());
    cmd.arg("patch")
        .arg("export")
        .arg("--patch")
        .arg(&patch_id)
        .arg("--out")
        .arg("./export");
    path_args(&mut cmd, &game);

    cmd.assert().success();

    let about = fs::read_to_string(
        cwd.path()
            .join("export")
            .join(FOLDER_NAME)
            .join("About")
            .join("About.xml"),
    )
    .expect("About.xml must have been written under the relative --out, resolved against cwd");
    assert!(about.contains(PACKAGE_ID));
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

/// Not a test function itself — see [`patch_new`]'s own doc comment.
fn patch_export(game: &ScratchGame, patch_id: &str, out_dir: &Path) {
    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("export")
        .arg("--patch")
        .arg(patch_id)
        .arg("--out")
        .arg(out_dir);
    path_args(&mut cmd, game);
    cmd.assert().success();
}

#[test]
fn exporting_the_same_decisions_twice_is_byte_identical() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);
    patch_decide(&game, &patch_id);
    let out_dir = temp_dir.path().join("export");
    let mod_dir = out_dir.join(FOLDER_NAME);

    patch_export(&game, &patch_id, &out_dir);
    let first_snapshot = snapshot_files(&mod_dir);

    patch_export(&game, &patch_id, &out_dir);
    let second_snapshot = snapshot_files(&mod_dir);

    assert_eq!(
        first_snapshot, second_snapshot,
        "exporting the same decisions twice must be byte-identical"
    );
}

#[test]
fn export_into_the_games_mods_folder_is_refused() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);
    patch_decide(&game, &patch_id);
    let mods_subfolder = game.game_dir.join("Mods").join("wherever");

    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("export")
        .arg("--patch")
        .arg(&patch_id)
        .arg("--out")
        .arg(&mods_subfolder);
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("OutDirIsModsFolder"));
}

#[test]
fn show_prints_the_package_id_and_the_needs_input_count() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("patch").arg("show").arg("--patch").arg(&patch_id);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(PACKAGE_ID))
        .stdout(predicates::str::contains("needs_input="));
}

#[test]
fn revert_after_decide_reports_the_reverted_key() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);
    patch_decide(&game, &patch_id);
    let key = "def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]";

    let mut cmd = common::rimmerge();
    cmd.arg("patch")
        .arg("revert")
        .arg("--patch")
        .arg(&patch_id)
        .arg("--key")
        .arg(key);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(format!(
            "reverted the decision on {key}"
        )));
}

#[test]
fn import_with_no_profile_decisions_imports_nothing() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("patch").arg("import").arg("--patch").arg(&patch_id);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("imported 0 decision(s):"));
}

#[test]
fn prune_with_no_orphaned_decisions_prunes_nothing() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("patch").arg("prune").arg("--patch").arg(&patch_id);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("pruned 0 decision(s):"));
}

#[test]
fn delete_removes_the_patch_from_the_list() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let patch_id = patch_new(&game);

    let mut delete_cmd = common::rimmerge();
    delete_cmd
        .arg("patch")
        .arg("delete")
        .arg("--patch")
        .arg(&patch_id);
    path_args(&mut delete_cmd, &game);
    delete_cmd.assert().success();

    let mut list_cmd = common::rimmerge();
    list_cmd.arg("patch").arg("list");
    path_args(&mut list_cmd, &game);
    list_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("no patches on this profile"));
}
