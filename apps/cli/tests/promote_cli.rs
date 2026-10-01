//! `rimmerge promote` end to end, against a scratch copy of `rim-io`'s
//! `merge_game` fixture — never the real game install or `ModsConfig.xml`
//!

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

/// Seeds `<profile_dir>/rules.json` with one imported (`RimSortCommunity`)
/// pair rule between the fixture's two mods — the "already imported"
/// state `promote` is meant to act on.
fn seed_imported_pair_rule(game: &ScratchGame) {
    fs::create_dir_all(&game.profile_dir)
        .unwrap_or_else(|error| panic!("create profile dir: {error}"));
    fs::write(game.profile_dir.join("rules.json"),
        r#"{
            "version": 2,
            "pairs": [
                {"after": "fixture.modb", "before": "fixture.moda", "origin": "rim_sort_community", "comment": "imported"}
            ],
            "placements": [],
            "incompatibles": [],
            "tag_rules": [],
            "manual_tags": [],
            "settings": {
                "threshold": 80,
                "enforce_soft": false,
                "enforce_awareness": false,
                "suggest_merge_when_clean": true,
                "tie_break": "rebuild",
                "use_imported_pairs": false,
                "use_imported_placements": true
            }
        }"#)
    .unwrap_or_else(|error| panic!("seed rules.json: {error}"));
}

/// Seeds `<profile_dir>/rules.json` with one imported (`RimSortCommunity`)
/// `Bottom` placement rule pinning `fixture.moda` — the "already imported"
/// state `promote --placement` is meant to act on.
fn seed_imported_placement_rule(game: &ScratchGame) {
    fs::create_dir_all(&game.profile_dir)
        .unwrap_or_else(|error| panic!("create profile dir: {error}"));
    fs::write(game.profile_dir.join("rules.json"),
        r#"{
            "version": 2,
            "pairs": [],
            "placements": [
                {"mod_id": "fixture.moda", "placement": "bottom", "origin": "rim_sort_community", "comment": "imported"}
            ],
            "incompatibles": [],
            "tag_rules": [],
            "manual_tags": [],
            "settings": {
                "threshold": 80,
                "enforce_soft": false,
                "enforce_awareness": false,
                "suggest_merge_when_clean": true,
                "tie_break": "rebuild",
                "use_imported_pairs": false,
                "use_imported_placements": true
            }
        }"#)
    .unwrap_or_else(|error| panic!("seed rules.json: {error}"));
}

/// Writes a minimal RimSort database directory under `dir` whose
/// `communityRules.json` re-asserts the same `Bottom` placement on
/// `fixture.moda` that [`seed_imported_placement_rule`] seeded directly —
/// the "re-import" half of the promote round trip.
fn write_rimsort_db_reasserting_the_placement(dir: &Path) {
    fs::create_dir_all(dir.join("Community-Rules-Database"))
        .unwrap_or_else(|error| panic!("create Community-Rules-Database dir: {error}"));
    fs::create_dir_all(dir.join("Steam-Workshop-Database"))
        .unwrap_or_else(|error| panic!("create Steam-Workshop-Database dir: {error}"));
    fs::write(dir.join("userRules.json"), r#"{"rules":{}}"#)
        .unwrap_or_else(|error| panic!("write userRules.json: {error}"));
    fs::write(
        dir.join("Community-Rules-Database")
            .join("communityRules.json"),
        r#"{"rules":{"fixture.moda":{"loadBottom":{"value":true}}}}"#,
    )
    .unwrap_or_else(|error| panic!("write communityRules.json: {error}"));
    fs::write(
        dir.join("Steam-Workshop-Database").join("steamDB.json"),
        r#"{"database":{}}"#,
    )
    .unwrap_or_else(|error| panic!("write steamDB.json: {error}"));
}

fn rules_json(game: &ScratchGame) -> serde_json::Value {
    let raw = fs::read_to_string(game.profile_dir.join("rules.json"))
        .unwrap_or_else(|error| panic!("read rules.json: {error}"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("rules.json must be valid JSON: {error}"))
}

#[test]
fn promote_adds_a_user_owned_copy_alongside_the_imported_original() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_imported_pair_rule(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("promote")
        .arg("--pair")
        .arg("fixture.modb,fixture.moda");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("promoted to a user-owned rule"));

    let pairs = rules_json(&game)["pairs"]
        .as_array()
        .expect("pairs array")
        .clone();
    assert_eq!(
        pairs.len(),
        2,
        "the import stays alongside the promoted copy: {pairs:?}"
    );
    let origins: Vec<&str> = pairs
        .iter()
        .map(|p| p["origin"].as_str().expect("origin string"))
        .collect();
    assert!(origins.contains(&"rim_sort_community"), "{origins:?}");
    assert!(origins.contains(&"user_decision"), "{origins:?}");
}

#[test]
fn promoting_the_same_pair_twice_does_not_duplicate_the_user_owned_copy() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_imported_pair_rule(&game);

    let run_promote = || {
        let mut cmd = common::rimmerge();
        cmd.arg("promote")
            .arg("--pair")
            .arg("fixture.modb,fixture.moda");
        path_args(&mut cmd, &game);
        cmd.assert().success();
    };
    run_promote();
    run_promote();

    let pairs = rules_json(&game)["pairs"]
        .as_array()
        .expect("pairs array")
        .clone();
    assert_eq!(
        pairs.len(),
        2,
        "a repeat promote must not add a second copy: {pairs:?}"
    );
}

/// The `--placement` half of the promote/re-import round trip: promoting
/// an imported community placement adds a `UserDecision` copy keyed like
/// the import, and a subsequent re-import (which replaces every
/// imported-origin rule, per `Session::apply_import`) leaves that
/// `UserDecision` copy untouched because it isn't imported-origin.
#[test]
fn promote_placement_survives_a_reimport_that_reasserts_the_same_placement() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_imported_placement_rule(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("promote").arg("--placement").arg("fixture.moda");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("promoted to a user-owned rule"));

    let placements_after_promote = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(
        placements_after_promote.len(),
        2,
        "the import stays alongside the promoted copy: {placements_after_promote:?}"
    );
    let origins_after_promote: Vec<&str> = placements_after_promote
        .iter()
        .map(|p| p["origin"].as_str().expect("origin string"))
        .collect();
    assert!(
        origins_after_promote.contains(&"rim_sort_community"),
        "{origins_after_promote:?}"
    );
    assert!(
        origins_after_promote.contains(&"user_decision"),
        "{origins_after_promote:?}"
    );

    let rimsort_dir = temp_dir.path().join("rimsort_db");
    write_rimsort_db_reasserting_the_placement(&rimsort_dir);
    let mut cmd = common::rimmerge();
    cmd.arg("import").arg("--rimsort-dir").arg(&rimsort_dir);
    path_args(&mut cmd, &game);
    cmd.assert().success();

    let placements_after_reimport = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(
        placements_after_reimport.len(),
        2,
        "the promoted copy must survive a re-import: {placements_after_reimport:?}"
    );
    let origins_after_reimport: Vec<&str> = placements_after_reimport
        .iter()
        .map(|p| p["origin"].as_str().expect("origin string"))
        .collect();
    assert!(
        origins_after_reimport.contains(&"rim_sort_community"),
        "{origins_after_reimport:?}"
    );
    assert!(
        origins_after_reimport.contains(&"user_decision"),
        "{origins_after_reimport:?}"
    );
}

#[test]
fn promoting_a_pair_with_no_imported_rule_reports_nothing_to_promote() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("promote")
        .arg("--pair")
        .arg("fixture.modb,fixture.moda");
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("nothing to promote"));
}

#[test]
fn promote_rejects_pair_and_placement_together() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("promote")
        .arg("--pair")
        .arg("fixture.modb,fixture.moda")
        .arg("--placement")
        .arg("fixture.moda");
    path_args(&mut cmd, &game);
    cmd.assert().failure();
}
