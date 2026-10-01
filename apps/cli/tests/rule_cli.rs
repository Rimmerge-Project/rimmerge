//! `rimmerge rule set-placement` end to end, against a scratch copy of
//! `rim-io`'s `merge_game` fixture — never the real game install or
//! `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
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

fn rules_json(game: &ScratchGame) -> serde_json::Value {
    let raw = fs::read_to_string(game.profile_dir.join("rules.json"))
        .unwrap_or_else(|error| panic!("read rules.json: {error}"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("rules.json must be valid JSON: {error}"))
}

#[test]
fn set_placement_persists_a_user_decision_bottom_pin() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-placement")
        .arg("--mod-id")
        .arg("fixture.moda")
        .arg("--placement")
        .arg("bottom")
        .arg("--comment")
        .arg("pinned for testing");
    path_args(&mut cmd, &game);
    cmd.assert().success().stdout(predicates::str::contains(
        "pinned fixture.moda to the bottom",
    ));

    let placements = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(placements.len(), 1, "{placements:?}");
    assert_eq!(placements[0]["mod_id"], "fixture.moda");
    assert_eq!(placements[0]["placement"], "bottom");
    assert_eq!(placements[0]["origin"], "user_decision");
    assert_eq!(placements[0]["comment"], "pinned for testing");
}

#[test]
fn set_placement_top_persists_a_user_decision_top_pin() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-placement")
        .arg("--mod-id")
        .arg("fixture.modb")
        .arg("--placement")
        .arg("top");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("pinned fixture.modb to the top"));

    let placements = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(placements.len(), 1, "{placements:?}");
    assert_eq!(placements[0]["mod_id"], "fixture.modb");
    assert_eq!(placements[0]["placement"], "top");
    assert_eq!(placements[0]["origin"], "user_decision");
}

/// A typo'd
/// `--mod-id` must fail fast rather than silently persist a placement
/// rule that matches nothing and can never be diagnosed from the rules
/// page.
#[test]
fn set_placement_rejects_a_mod_id_with_no_active_mod() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-placement")
        .arg("--mod-id")
        .arg("fixture.typo-does-not-exist")
        .arg("--placement")
        .arg("bottom");
    path_args(&mut cmd, &game);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no active mod named"));

    assert!(
        !game.profile_dir.join("rules.json").exists(),
        "a rejected set-placement must not create rules.json at all"
    );
}

/// [`rim_session::use_cases::UpsertRule`]'s own "adds or replaces": a
/// second `set-placement` for the same mod replaces the first rather
/// than duplicating it.
#[test]
fn set_placement_twice_for_the_same_mod_replaces_rather_than_duplicates() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let run_set = |placement: &str| {
        let mut cmd = common::rimmerge();
        cmd.arg("rule")
            .arg("set-placement")
            .arg("--mod-id")
            .arg("fixture.moda")
            .arg("--placement")
            .arg(placement);
        path_args(&mut cmd, &game);
        cmd.assert().success();
    };
    run_set("bottom");
    run_set("top");

    let placements = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(
        placements.len(),
        1,
        "a repeat set-placement for the same mod must not add a second row: {placements:?}"
    );
    assert_eq!(placements[0]["placement"], "top");
}

#[test]
fn set_pair_persists_a_user_decision_pair_rule() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.modb")
        .arg("--comment")
        .arg("moda needs modb's changes");
    path_args(&mut cmd, &game);
    cmd.assert().success().stdout(predicates::str::contains(
        "fixture.moda now loads after fixture.modb",
    ));

    let pairs = rules_json(&game)["pairs"]
        .as_array()
        .expect("pairs array")
        .clone();
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    assert_eq!(pairs[0]["after"], "fixture.moda");
    assert_eq!(pairs[0]["before"], "fixture.modb");
    assert_eq!(pairs[0]["origin"], "user_decision");
    assert_eq!(pairs[0]["comment"], "moda needs modb's changes");
}

/// Mirrors `set_placement_rejects_a_mod_id_with_no_active_mod` for both
/// sides of a pair.
#[test]
fn set_pair_rejects_an_inactive_mod_on_either_side() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.typo-does-not-exist")
        .arg("--before")
        .arg("fixture.modb");
    path_args(&mut cmd, &game);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no active mod named"));

    assert!(
        !game.profile_dir.join("rules.json").exists(),
        "a rejected set-pair must not create rules.json at all"
    );

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.typo-does-not-exist");
    path_args(&mut cmd, &game);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no active mod named"));
}

#[test]
fn set_pair_rejects_the_same_mod_on_both_sides() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.moda");
    path_args(&mut cmd, &game);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("name the same mod"));
}

/// [`rim_session::use_cases::UpsertRule`]'s own "adds or replaces":
/// setting a pair twice replaces the first row rather than duplicating
/// it.
#[test]
fn set_pair_twice_for_the_same_pair_replaces_rather_than_duplicates() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let run_set = |comment: &str| {
        let mut cmd = common::rimmerge();
        cmd.arg("rule")
            .arg("set-pair")
            .arg("--after")
            .arg("fixture.moda")
            .arg("--before")
            .arg("fixture.modb")
            .arg("--comment")
            .arg(comment);
        path_args(&mut cmd, &game);
        cmd.assert().success();
    };
    run_set("first");
    run_set("second");

    let pairs = rules_json(&game)["pairs"]
        .as_array()
        .expect("pairs array")
        .clone();
    assert_eq!(
        pairs.len(),
        1,
        "a repeat set-pair for the same pair must not add a second row: {pairs:?}"
    );
    assert_eq!(pairs[0]["comment"], "second");
}

#[test]
fn remove_pair_deletes_a_previously_set_rule() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut set_cmd = common::rimmerge();
    set_cmd
        .arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.modb");
    path_args(&mut set_cmd, &game);
    set_cmd.assert().success();
    assert_eq!(
        rules_json(&game)["pairs"]
            .as_array()
            .expect("pairs array")
            .len(),
        1
    );

    let mut remove_cmd = common::rimmerge();
    remove_cmd
        .arg("rule")
        .arg("remove-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.modb");
    path_args(&mut remove_cmd, &game);
    remove_cmd
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "removed every rule stating fixture.moda after fixture.modb",
        ));

    assert!(
        rules_json(&game)["pairs"]
            .as_array()
            .expect("pairs array")
            .is_empty(),
        "the rule must be gone after remove-pair"
    );
}

/// `remove-pair` on a key with nothing stored is a no-op, not an error —
/// matching `Session::delete_rule`'s own contract.
#[test]
fn remove_pair_on_an_unknown_pair_is_a_no_op() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("rule")
        .arg("remove-pair")
        .arg("--after")
        .arg("fixture.moda")
        .arg("--before")
        .arg("fixture.modb");
    path_args(&mut cmd, &game);
    cmd.assert().success();
}
