//! End-to-end CLI tests for `rimmerge verify`, against a scratch copy of
//! `rim-io`'s `merge_game` fixture — never the real game install or
//! `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
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

/// Adds a third active mod, `fixture.modc`, whose own patch targets a
/// `ThingDef` no mod in this fixture defines — the zero-owner `DeadTarget`
/// shape, the simplest, most deterministic real failure `VerifyOrder` can
/// predict (no replay ambiguity at all: the target genuinely doesn't
/// exist, so the operation matches nothing regardless of load order).
fn add_a_patcher_targeting_a_nonexistent_def(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;
    let about = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modc</packageId>
  <name>Fixture Mod C</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#;
    let patch = r#"<Patch>
  <Operation Class="PatchOperationAdd">
    <xpath>Defs/ThingDef[defName="Fixture_NoSuchDef"]/comps</xpath>
    <value>
      <li>
        <compClass>CompFixture</compClass>
      </li>
    </value>
  </Operation>
</Patch>
"#;

    let root = game.game_dir.join("Mods").join("ModC");
    fs::create_dir_all(root.join("About"))
        .unwrap_or_else(|error| panic!("create ModC/About: {error}"));
    fs::write(root.join("About").join("About.xml"), about)
        .unwrap_or_else(|error| panic!("write ModC/About.xml: {error}"));
    fs::write(root.join("LoadFolders.xml"), load_folders)
        .unwrap_or_else(|error| panic!("write ModC/LoadFolders.xml: {error}"));
    fs::create_dir_all(root.join("Patches"))
        .unwrap_or_else(|error| panic!("create ModC/Patches: {error}"));
    fs::write(root.join("Patches").join("x.xml"), patch)
        .unwrap_or_else(|error| panic!("write ModC/Patches/x.xml: {error}"));

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

#[test]
fn verify_against_the_plain_fixture_predicts_nothing() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("0 operations predicted to fail across 0 def targets"),
        "expected a clean fixture to predict nothing: {stdout}"
    );
}

#[test]
fn verify_predicts_a_dead_target_and_the_text_output_leads_with_the_operation_identity() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_patcher_targeting_a_nonexistent_def(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("1 operations predicted to fail across 1 def targets"),
        "expected exactly one predicted operation over one def target: {stdout}"
    );
    // Matches RimWorld's own real `Player.log` line shape exactly
    // (`[<mod name>] Patch operation <identity> failed`) so a user can
    // grep this output against a real log — the whole point of leading
    // with `Finding::PatchWillFail::operation`. The `(1 def target)`
    // suffix is this group's own affected-def count (one header per real
    // operation, not one per def).
    assert!(stdout.contains(r#"[Fixture Mod C] Patch operation Verse.PatchOperationAdd(Defs/ThingDef[defName="Fixture_NoSuchDef"]/comps) failed (1 def target)"#
        ),
        "text output must lead with the top-level operation's own log identity: {stdout}"
    );
    assert!(stdout.contains("cause: DeadTarget"), "{stdout}");
}

/// A single top-level operation whose head matches several defNames via
/// an `OR`-list (`VerifyOrder`'s own per-def architecture checks each
/// match independently) must not print the identical headline once per
/// matched def. Two nonexistent defNames sharing one operation
/// must collapse into one grouped header naming both def targets
/// beneath it, not two separate headers.
#[test]
fn verify_groups_one_or_list_operation_matching_two_defs_into_a_single_header() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());

    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;
    let about = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modc</packageId>
  <name>Fixture Mod C</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#;
    let patch = r#"<Patch>
  <Operation Class="PatchOperationAdd">
    <xpath>Defs/ThingDef[defName="Fixture_NoSuchDef" or defName="Fixture_NoSuchDef2"]/comps</xpath>
    <value>
      <li>
        <compClass>CompFixture</compClass>
      </li>
    </value>
  </Operation>
</Patch>
"#;
    let root = game.game_dir.join("Mods").join("ModC");
    fs::create_dir_all(root.join("About")).unwrap_or_else(|error| panic!("create About: {error}"));
    fs::write(root.join("About").join("About.xml"), about)
        .unwrap_or_else(|error| panic!("write About.xml: {error}"));
    fs::write(root.join("LoadFolders.xml"), load_folders)
        .unwrap_or_else(|error| panic!("write LoadFolders.xml: {error}"));
    fs::create_dir_all(root.join("Patches"))
        .unwrap_or_else(|error| panic!("create Patches: {error}"));
    fs::write(root.join("Patches").join("x.xml"), patch)
        .unwrap_or_else(|error| panic!("write Patches/x.xml: {error}"));
    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("1 operations predicted to fail across 2 def targets"),
        "one real operation, two def targets: {stdout}"
    );
    assert_eq!(
        stdout
            .matches("Patch operation Verse.PatchOperationAdd")
            .count(),
        1,
        "the headline must appear exactly once, not once per matched def: {stdout}"
    );
    assert!(stdout.contains("failed (2 def targets)"), "{stdout}");
    assert!(stdout.contains("ThingDef/Fixture_NoSuchDef"), "{stdout}");
    assert!(stdout.contains("ThingDef/Fixture_NoSuchDef2"), "{stdout}");
}

#[test]
fn verify_json_carries_the_operation_and_leaf_xpath() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_patcher_targeting_a_nonexistent_def(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify")
        .arg("--source")
        .arg("current")
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");
    let json: Value = serde_json::from_str(&stdout).expect("valid JSON");

    assert_eq!(json["source"], "current");
    assert_eq!(json["operations_total"], 1, "{json}");
    assert_eq!(json["def_targets_total"], 1, "{json}");
    let operations = json["operations"].as_array().expect("operations array");
    assert_eq!(operations.len(), 1, "{json}");
    let operation = &operations[0];
    assert_eq!(operation["mod_id"], "fixture.modc");
    assert_eq!(operation["mod_name"], "Fixture Mod C");
    assert_eq!(
        operation["operation"],
        r#"Verse.PatchOperationAdd(Defs/ThingDef[defName="Fixture_NoSuchDef"]/comps)"#
    );
    let defs = operation["defs"].as_array().expect("defs array");
    assert_eq!(defs.len(), 1, "{json}");
    let def = &defs[0];
    assert_eq!(def["def_type"], "ThingDef");
    assert_eq!(def["def_name"], "Fixture_NoSuchDef");
    assert_eq!(def["cause"], "DeadTarget");
    assert_eq!(
        def["leaf_xpath"],
        r#"Defs/ThingDef[defName="Fixture_NoSuchDef"]/comps"#
    );
}

/// `--source suggested` (the default) is the order `apply` would
/// actually write — no `--enforce-*`/`--no-inferred` override flags
/// exist on this command (see `verify.rs`'s own doc comment for why);
/// this only pins that the default runs cleanly end to end.
#[test]
fn verify_defaults_to_the_suggested_source() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());

    let mut cmd = common::rimmerge();
    cmd.arg("verify");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("VerifyOrder (suggested):"),
        "expected the default source to be suggested: {stdout}"
    );
}

/// Adds two more active patchers of `Fixture_Wall`: `fixture.modc`
/// removes its `<label>`, and `fixture.modd` — loading after it — tries
/// to replace the node that is now gone. That is the `RemovedBy` shape,
/// the only cause (besides `NotYetInjected`) whose suggestion carries a
/// `Reorder` alternative, and therefore the only one the `fix:` line is
/// printed for.
///
/// **`fixture.modd` inserts a sibling next to `<label>`, rather than
/// replacing `<label>` itself** — a `Replace` of the identical node
/// modc removes would make this pair *cosmetic* (whichever
/// mod runs first, `Fixture_Wall` ends up with no `label` either way —
/// modc's own `Remove` either strips modd's fresh content or modd's own
/// `Replace` never finds the node modc already removed), and a cosmetic
/// row gets no `fix:`/`reorder` at all. An `Insert` needs `label` to
/// still exist as its own anchor (so it still depends on modc's own
/// removal, the `RemovedBy` shape every test below means to exercise),
/// but never touches or renames `label` itself, so modc's own `Remove`
/// keeps succeeding under *either* order — no regression — while the
/// final tree genuinely differs (modd's own sibling node present only
/// when it runs first), a real content-losing case. The dedicated
/// cosmetic case has its own fixture,
/// [`add_a_remover_and_a_cosmetic_replacer_of_the_same_node`].
fn add_a_remover_and_a_later_patcher_of_the_same_node(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;
    for (folder, package_id, name, operation) in [
        (
            "ModC",
            "fixture.modc",
            "Fixture Mod C",
            r#"  <Operation Class="PatchOperationRemove">
    <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
  </Operation>"#,
        ),
        (
            "ModD",
            "fixture.modd",
            "Fixture Mod D",
            r#"  <Operation Class="PatchOperationInsert">
    <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
    <order>Append</order>
    <value>
      <description>relabelled fixture wall</description>
    </value>
  </Operation>"#,
        ),
    ] {
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        let patch = format!("<Patch>\n{operation}\n</Patch>\n");
        let root = game.game_dir.join("Mods").join(folder);
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {folder}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {folder}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {folder}/LoadFolders.xml: {error}"));
        fs::create_dir_all(root.join("Patches"))
            .unwrap_or_else(|error| panic!("create {folder}/Patches: {error}"));
        fs::write(root.join("Patches").join("x.xml"), patch)
            .unwrap_or_else(|error| panic!("write {folder}/Patches/x.xml: {error}"));
    }

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
    <li>fixture.modd</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

/// The cosmetic counterpart of
/// [`add_a_remover_and_a_later_patcher_of_the_same_node`]: `fixture.modc`
/// removes `Fixture_Wall`'s `<label>`, and `fixture.modd` — loading after
/// it — `Replace`s that same node, keeping the tag `label`. Whichever
/// mod runs first, `Fixture_Wall` ends up with no `label` at all (a
/// `Replace`'s own `<value>` node is gone the moment a later
/// `Remove` of the same site runs; a `Remove` that already ran leaves
/// nothing for the `Replace` to find) — the counterfactual's own
/// `final_def_unchanged` must read `true`, and verify must offer no
/// `set-pair` fix for it. The two mods also collide on the identical
/// target, so the analyzer's own `PatchCollision` finding exists for
/// verify to point at instead.
fn add_a_remover_and_a_cosmetic_replacer_of_the_same_node(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;
    for (folder, package_id, name, operation) in [
        (
            "ModC",
            "fixture.modc",
            "Fixture Mod C",
            r#"  <Operation Class="PatchOperationRemove">
    <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
  </Operation>"#,
        ),
        (
            "ModD",
            "fixture.modd",
            "Fixture Mod D",
            r#"  <Operation Class="PatchOperationReplace">
    <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
    <value>
      <label>relabelled fixture wall</label>
    </value>
  </Operation>"#,
        ),
    ] {
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        let patch = format!("<Patch>\n{operation}\n</Patch>\n");
        let root = game.game_dir.join("Mods").join(folder);
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {folder}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {folder}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {folder}/LoadFolders.xml: {error}"));
        fs::create_dir_all(root.join("Patches"))
            .unwrap_or_else(|error| panic!("create {folder}/Patches: {error}"));
        fs::write(root.join("Patches").join("x.xml"), patch)
            .unwrap_or_else(|error| panic!("write {folder}/Patches/x.xml: {error}"));
    }

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
    <li>fixture.modd</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

/// A cosmetic row: no `fix:`/`set-pair` line, a `cosmetic:`
/// notice instead, and — since the two mods also collide on the exact
/// same target — a pointer at the existing `PatchCollision` finding as
/// the way to keep the losing mod's intent.
#[test]
fn verify_prints_a_cosmetic_notice_with_no_set_pair_for_a_cosmetic_row() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_cosmetic_replacer_of_the_same_node(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("cause: RemovedBy(fixture.modc)"),
        "still classified RemovedBy — only the reorder offer changes: {stdout}"
    );
    assert!(
        !stdout.contains("fix: rimmerge rule set-pair"),
        "a cosmetic row must never offer set-pair: {stdout}"
    );
    assert!(
        stdout.contains("cosmetic: the same final def either order — no set-pair rule offered"),
        "expected the cosmetic notice: {stdout}"
    );
    assert!(
        stdout.contains("merge: rimmerge merge plan --key 'patch_collision:"),
        "the two mods collide on the identical target, so an existing merge exists to point \
         at: {stdout}"
    );
}

/// The `--json` mirror: `reorderKind` carries `"cosmetic"`, `reorder` is
/// `null`, and `cosmeticMergeKey` names the existing `PatchCollision`.
#[test]
fn verify_json_carries_cosmetic_reorder_kind_and_no_reorder() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_cosmetic_replacer_of_the_same_node(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify")
        .arg("--source")
        .arg("current")
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");
    let json: Value = serde_json::from_str(&stdout).expect("valid JSON");

    let operations = json["operations"].as_array().expect("operations array");
    let def = operations
        .iter()
        .flat_map(|operation| operation["defs"].as_array().expect("defs array"))
        .find(|def| def["cause"] == "RemovedBy(fixture.modc)")
        .unwrap_or_else(|| panic!("a RemovedBy row: {json}"));

    assert_eq!(def["reorder_kind"], "cosmetic", "{json}");
    assert_eq!(def["reorder"], Value::Null, "{json}");
    assert!(
        def["cosmetic_merge_key"]
            .as_str()
            .is_some_and(|key| key.starts_with("patch_collision:")),
        "{json}"
    );
}

/// An order-fixable row prints the exact, copy-pasteable
/// `rimmerge rule set-pair` invocation, with the direction read off
/// `rim_resolve::ledger::suggest`'s own `Reorder` alternative — the
/// remover loads *after* the mod whose operation failed, so its removal
/// no longer runs first.
#[test]
fn verify_prints_a_copy_pasteable_set_pair_command_for_an_order_fixable_row() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_later_patcher_of_the_same_node(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("cause: RemovedBy(fixture.modc)"),
        "expected the later patcher to be classified RemovedBy: {stdout}"
    );
    // Single-quoted, per the shell contract in `set_pair_command`'s own
    // doc comment: a `packageId` is untrusted `About.xml` text.
    assert!(
        stdout.contains(
            "    fix: rimmerge rule set-pair --after 'fixture.modc' --before 'fixture.modd' \
             --comment '"
        ),
        "expected the copy-pasteable set-pair line, remover after the failing mod: {stdout}"
    );
    assert!(
        !stdout.contains("note: a user pair rule already exists"),
        "no user rule exists at this key yet: {stdout}"
    );
    assert_eq!(
        stdout.matches("fix: rimmerge rule set-pair").count(),
        1,
        "one relation, one printed command: {stdout}"
    );
}

/// The `--json` mirror of the line above: `reorder` carries the same
/// direction plus the command itself, so a measurement run can grep one
/// field instead of reassembling the invocation.
#[test]
fn verify_json_carries_the_reorder_and_its_set_pair_command() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_later_patcher_of_the_same_node(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify")
        .arg("--source")
        .arg("current")
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");
    let json: Value = serde_json::from_str(&stdout).expect("valid JSON");

    let operations = json["operations"].as_array().expect("operations array");
    let def = operations
        .iter()
        .flat_map(|operation| operation["defs"].as_array().expect("defs array"))
        .find(|def| def["cause"] == "RemovedBy(fixture.modc)")
        .unwrap_or_else(|| panic!("a RemovedBy row: {json}"));
    assert_eq!(def["reorder"]["after"], "fixture.modc", "{json}");
    assert_eq!(def["reorder"]["before"], "fixture.modd", "{json}");
    assert_eq!(
        def["reorder"]["set_pair_command"]
            .as_str()
            .expect("set_pair_command string")
            .split(" --comment ")
            .next()
            .expect("command prefix"),
        "rimmerge rule set-pair --after 'fixture.modc' --before 'fixture.modd'",
        "{json}"
    );
    assert_eq!(def["reorder"]["existing_user_rule"], false, "{json}");
    assert_eq!(
        def["reorder"]["existing_overrides_declared"], false,
        "{json}"
    );
    assert_eq!(
        def["reorder"]["conflicts"],
        serde_json::json!([]),
        "no conflicting edge exists in this fixture: {json}"
    );
}

/// Same fixture as [`add_a_remover_and_a_later_patcher_of_the_same_node`],
/// plus `fixture.modd` declaring its own `<loadAfter>fixture.modc</loadAfter>`
/// — the real `example.architect` shape: a `Reorder` fix that reverses
/// the mod's *own* declared order. The declared `LoadAfter` edge is
/// `after: fixture.modd, before:
/// fixture.modc`, the exact opposite of the `RemovedBy` fix's own
/// `after: fixture.modc, before: fixture.modd`.
fn add_a_remover_and_a_later_patcher_with_a_conflicting_declared_load_after(game: &ScratchGame) {
    add_a_remover_and_a_later_patcher_of_the_same_node(game);
    let about_path = game
        .game_dir
        .join("Mods")
        .join("ModD")
        .join("About")
        .join("About.xml");
    let about = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modd</packageId>
  <name>Fixture Mod D</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
  <loadAfter>
    <li>fixture.modc</li>
  </loadAfter>
</ModMetaData>
"#;
    fs::write(&about_path, about).unwrap_or_else(|error| panic!("rewrite ModD/About.xml: {error}"));
}

/// A `Reorder` that reverses a
/// declared edge must say so, in text, under its own `fix:` line.
#[test]
fn verify_prints_a_conflict_line_for_a_reorder_that_reverses_a_declared_edge() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_later_patcher_with_a_conflicting_declared_load_after(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains(
            "    fix: rimmerge rule set-pair --after 'fixture.modc' --before 'fixture.modd' \
             --comment '"
        ),
        "the fix line still prints regardless of the conflict: {stdout}"
    );
    let fix_pos = stdout
        .find("    fix: rimmerge rule set-pair")
        .expect("fix line present");
    let conflict_pos = stdout
        .find("    conflicts with LoadAfter (reverses it):")
        .unwrap_or_else(|| panic!("expected a conflicts-with line naming LoadAfter: {stdout}"));
    assert!(
        conflict_pos > fix_pos,
        "the conflicts line must print under (after) the fix: line: {stdout}"
    );
}

/// The `--json` mirror: `reorder.conflicts` carries the contradicting
/// edge's kind/detail/status.
#[test]
fn verify_json_carries_the_reorder_conflict() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_later_patcher_with_a_conflicting_declared_load_after(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify")
        .arg("--source")
        .arg("current")
        .arg("--json");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");
    let json: Value = serde_json::from_str(&stdout).expect("valid JSON");

    let operations = json["operations"].as_array().expect("operations array");
    let def = operations
        .iter()
        .flat_map(|operation| operation["defs"].as_array().expect("defs array"))
        .find(|def| def["cause"] == "RemovedBy(fixture.modc)")
        .unwrap_or_else(|| panic!("a RemovedBy row: {json}"));
    let conflicts = def["reorder"]["conflicts"]
        .as_array()
        .expect("conflicts array");
    let kinds: Vec<&str> = conflicts
        .iter()
        .map(|c| c["kind"].as_str().expect("kind string"))
        .collect();
    assert!(
        kinds.contains(&"LoadAfter"),
        "expected the reversed declared edge among the conflicts: {json}"
    );
}

/// The `fix:` line is deduplicated **per group**. Every other fixture has
/// a single def per operation, so this is the one that fails if the dedup
/// is deleted.
///
/// `fixture.mode` owns a second def; `fixture.modc`'s single `OR`-list
/// `Remove` strips `label` from both defs, and `fixture.modd`'s single
/// `OR`-list `Replace` then fails on both. One operation identity, two def
/// targets, one relation — so exactly one `fix:` line.
fn add_a_second_def_and_two_or_list_patchers(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;
    let or_head = r#"Defs/ThingDef[defName="Fixture_Wall" or defName="Fixture_Wall2"]/label"#;
    let mods: [(&str, &str, &str, String); 3] = [
        (
            "ModE",
            "fixture.mode",
            "Fixture Mod E",
            String::new(), // defs only, written below
        ),
        (
            "ModC",
            "fixture.modc",
            "Fixture Mod C",
            format!(
                "<Patch>\n  <Operation Class=\"PatchOperationRemove\">\n    <xpath>{or_head}</xpath>\n  </Operation>\n</Patch>\n"
            ),
        ),
        (
            "ModD",
            "fixture.modd",
            "Fixture Mod D",
            format!(
                "<Patch>\n  <Operation Class=\"PatchOperationReplace\">\n    <xpath>{or_head}</xpath>\n    <value>\n      <label>relabelled</label>\n    </value>\n  </Operation>\n</Patch>\n"
            ),
        ),
    ];
    for (folder, package_id, name, patch) in &mods {
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        let root = game.game_dir.join("Mods").join(folder);
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {folder}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {folder}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {folder}/LoadFolders.xml: {error}"));
        if patch.is_empty() {
            continue;
        }
        fs::create_dir_all(root.join("Patches"))
            .unwrap_or_else(|error| panic!("create {folder}/Patches: {error}"));
        fs::write(root.join("Patches").join("x.xml"), patch)
            .unwrap_or_else(|error| panic!("write {folder}/Patches/x.xml: {error}"));
    }

    let defs = r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <ThingDef>
    <defName>Fixture_Wall2</defName>
    <label>second fixture wall</label>
  </ThingDef>
</Defs>
"#;
    let defs_dir = game
        .game_dir
        .join("Mods")
        .join("ModE")
        .join("1.6")
        .join("Defs");
    fs::create_dir_all(&defs_dir).unwrap_or_else(|error| panic!("create ModE/Defs: {error}"));
    fs::write(defs_dir.join("ThingDefs.xml"), defs)
        .unwrap_or_else(|error| panic!("write ModE defs: {error}"));

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.mode</li>
    <li>fixture.modc</li>
    <li>fixture.modd</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

#[test]
fn verify_prints_one_fix_line_for_an_or_list_operation_failing_on_two_defs() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_second_def_and_two_or_list_patchers(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains("1 operations predicted to fail across 2 def targets"),
        "one real operation, two def targets: {stdout}"
    );
    assert_eq!(
        stdout.matches("cause: RemovedBy(fixture.modc)").count(),
        2,
        "both def targets carry the same cause: {stdout}"
    );
    assert_eq!(
        stdout.matches("fix: rimmerge rule set-pair").count(),
        1,
        "two def targets, one relation, one printed command: {stdout}"
    );
}

/// `rule set-pair` *clears*
/// `--override-declared` when the flag is omitted, so the printed command
/// must carry an existing rule's flag forward — and must say so when it is
/// about to overwrite a hand-written comment. `--source current` is used
/// deliberately: it replays the on-disk order, so creating the rule cannot
/// re-sort the failure away mid-test.
#[test]
fn verify_carries_an_existing_user_rules_override_declared_flag_into_the_printed_command() {
    let temp = tempdir().expect("tempdir");
    let game = scratch_game(temp.path());
    add_a_remover_and_a_later_patcher_of_the_same_node(&game);

    let mut set = common::rimmerge();
    set.arg("rule")
        .arg("set-pair")
        .arg("--after")
        .arg("fixture.modc")
        .arg("--before")
        .arg("fixture.modd")
        .arg("--override-declared")
        .arg("--comment")
        .arg("hand-written, keep me");
    path_args(&mut set, &game);
    set.assert().success();

    let mut cmd = common::rimmerge();
    cmd.arg("verify").arg("--source").arg("current");
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf8 stdout");

    assert!(
        stdout.contains(
            "fix: rimmerge rule set-pair --after 'fixture.modc' --before 'fixture.modd' \
             --override-declared --comment '"
        ),
        "the existing rule's override flag must survive a re-run: {stdout}"
    );
    assert!(
        stdout.contains("note: a user pair rule already exists for this pair"),
        "the overwrite must be disclosed: {stdout}"
    );
}
