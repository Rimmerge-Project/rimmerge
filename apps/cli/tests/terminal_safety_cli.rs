//! Text-mode output never carries a control character from a mod, a report
//! or a log: a scratch copy of `rim-io`'s `merge_game` fixture is rewritten
//! so mod names, def labels and an xpath carry U+009B (the
//! C1 "control sequence introducer", which XML 1.0 allows in text and a
//! terminal may act on), and each text-mode command that prints such text is
//! run against it. U+001B (ESC) cannot appear in XML at all, so it enters
//! through command-line arguments (`patch new`) and through report JSON
//! (`sort`/`startup`); `log import` has its own ESC test in `log_cli.rs`.
//!
//! Every assertion checks two things: no control character other than a
//! newline or tab leaves the process, and the hostile text really did reach
//! the output in its stripped form (so a fixture that stopped carrying the
//! text cannot turn the test vacuous).

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

const CSI: char = '\u{9b}';
const ESC: char = '\u{1b}';

struct ScratchGame {
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

fn rewrite(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path:?}: {error}"));
    assert!(
        text.contains(from),
        "{path:?} lost the fixture text {from:?}"
    );
    fs::write(path, text.replace(from, to))
        .unwrap_or_else(|error| panic!("write {path:?}: {error}"));
}

/// `merge_game` with hostile names, a hostile def label, and a third active
/// mod whose patch targets a def nobody defines (a `DeadTarget` failure that
/// `verify` prints).
fn hostile_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    copy_dir_recursive(&merge_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy merge_game fixture: {error}"));
    let mods = game_dir.join("Mods");

    rewrite(
        &mods.join("ModA").join("About").join("About.xml"),
        "<name>Fixture Mod A</name>",
        &format!("<name>Mod{CSI}[31mA</name>"),
    );
    rewrite(
        &mods.join("ModA").join("About").join("About.xml"),
        "<author>Fixture Author</author>",
        &format!("<author>Auth{CSI}or</author>"),
    );
    rewrite(
        &mods
            .join("ModB")
            .join("1.6")
            .join("Defs")
            .join("ThingDefs.xml"),
        "<label>reinforced fixture wall</label>",
        &format!("<label>evil{CSI}[2Jwall</label>"),
    );

    let about = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modc</packageId>
  <name>Patcher{CSI}[1mC</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
    );
    let patch = format!(
        r#"<Patch>
  <Operation Class="PatchOperationAdd">
    <xpath>Defs/ThingDef[defName="Fixture_NoSuch{CSI}Def"]/comps</xpath>
    <value>
      <li>
        <compClass>CompFixture</compClass>
      </li>
    </value>
  </Operation>
</Patch>
"#
    );
    let load_folders =
        "<loadFolders>\n  <v1.6>\n    <li>1.6</li>\n    <li>/</li>\n  </v1.6>\n</loadFolders>\n";
    let root = mods.join("ModC");
    let write = |path: PathBuf, text: &str| {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {parent:?}: {error}"));
        }
        fs::write(&path, text).unwrap_or_else(|error| panic!("write {path:?}: {error}"));
    };
    write(root.join("About").join("About.xml"), &about);
    write(root.join("LoadFolders.xml"), load_folders);
    write(root.join("Patches").join("x.xml"), &patch);

    let mods_config_path = game_dir.join("ModsConfig.xml");
    write(
        mods_config_path.clone(),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModsConfigData>\n  <version>1.6.4871 rev590</version>\n  <activeMods>\n    <li>fixture.moda</li>\n    <li>fixture.modb</li>\n    <li>fixture.modc</li>\n  </activeMods>\n</ModsConfigData>\n",
    );

    ScratchGame {
        mods_config: mods_config_path,
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

/// Runs `rimmerge <args> <path args>` and returns its stdout and stderr,
/// both asserted free of control characters.
fn run_clean(game: &ScratchGame, args: &[&str]) -> String {
    let mut cmd = common::rimmerge();
    cmd.args(args);
    path_args(&mut cmd, game);
    let output = cmd
        .output()
        .unwrap_or_else(|error| panic!("spawn rimmerge: {error}"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_no_control_characters(args, &stdout);
    assert_no_control_characters(args, &stderr);
    assert!(
        output.status.success(),
        "{args:?} failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    stdout
}

/// The CLI's own progress line rewrites itself with `\r` on stderr, so a
/// carriage return is the one control character allowed there; everything
/// else is the output under test.
fn assert_no_control_characters(args: &[&str], text: &str) {
    let offender = text
        .chars()
        .find(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'));
    assert!(
        offender.is_none(),
        "{args:?} printed control character {offender:?}:\n{text:?}"
    );
}

#[test]
fn mods_list_and_show_strip_control_characters_from_names() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let list = run_clean(&game, &["mods", "list"]);
    let show = run_clean(&game, &["mods", "show", "fixture.moda"]);

    assert!(list.contains("Mod[31mA"), "{list}");
    assert!(show.contains("Mod[31mA (fixture.moda)"), "{show}");
    assert!(show.contains("authors: Author"), "{show}");
}

#[test]
fn mods_deactivate_dry_run_prints_a_clean_plan() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let plan = run_clean(
        &game,
        &["mods", "deactivate", "fixture.modb", "--dry-run", "--yes"],
    );

    assert!(plan.contains("fixture.modb"), "{plan}");
}

#[test]
fn verify_text_strips_control_characters_from_the_operation_and_the_mod_name() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let text = run_clean(&game, &["verify", "--source", "current"]);

    assert!(text.contains("[Patcher[1mC]"), "{text}");
    assert!(text.contains("Fixture_NoSuchDef"), "{text}");
}

#[test]
fn defs_search_changes_and_inspect_strip_control_characters() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let changes = run_clean(&game, &["defs", "changes", "fixture.modb"]);
    let search = run_clean(&game, &["defs", "search", "wall"]);
    let inspect = run_clean(&game, &["defs", "inspect", "ThingDef/Fixture_Wall"]);

    assert!(changes.contains("Fixture_Wall"), "{changes}");
    assert!(search.contains("Fixture_Wall"), "{search}");
    assert!(inspect.contains("evil[2Jwall"), "{inspect}");
}

#[test]
fn merge_plan_strips_control_characters_from_field_values() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let plan = run_clean(
        &game,
        &[
            "merge",
            "plan",
            "--key",
            "def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]",
        ],
    );

    assert!(plan.contains("evil[2Jwall"), "{plan}");
}

#[test]
fn apply_dry_run_prints_clean_ids() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());

    let apply = run_clean(&game, &["apply", "--dry-run", "--source", "current"]);

    assert!(apply.contains("Dry run"), "{apply}");
}

#[test]
fn report_driven_commands_strip_control_characters_from_report_text() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());
    run_clean(&game, &["load"]);
    let report_path = game.profile_dir.join("report.json");
    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).expect("read report")).expect("parse");
    // JSON escapes carry what XML cannot: a real ESC, as well as U+009B.
    report["mods"][0]["name"] = serde_json::Value::from(format!("Esc{ESC}[2J{CSI}Name"));
    report["mods"][0]["id"] = serde_json::Value::from(format!("fixture.moda{ESC}"));
    let hostile_report = temp.path().join("hostile-report.json");
    fs::write(
        &hostile_report,
        serde_json::to_vec(&report).expect("serialize"),
    )
    .expect("write");

    for subcommand in ["sort", "ledger", "startup"] {
        let mut cmd = common::rimmerge();
        cmd.arg(subcommand).arg("--report").arg(&hostile_report);
        let output = cmd.output().expect("spawn rimmerge");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_no_control_characters(&[subcommand], &stdout);
        assert_no_control_characters(&[subcommand], &stderr);
        assert!(output.status.success(), "{subcommand} failed: {stderr}");
    }
}

#[test]
fn patch_commands_strip_control_characters_from_project_text() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());
    let hostile_name = format!("Evil{ESC}[2J{CSI}patch");

    let created = run_clean(
        &game,
        &[
            "patch",
            "new",
            "--name",
            &hostile_name,
            "--package-id",
            "sample.hostile",
            "--scope",
            "fixture.moda,fixture.modb",
        ],
    );
    let id = created
        .split_whitespace()
        .last()
        .expect("created patch <id>")
        .to_string();

    let list = run_clean(&game, &["patch", "list"]);
    let show = run_clean(&game, &["patch", "show", "--patch", &id]);

    assert!(list.contains("Evil[2Jpatch"), "{list}");
    assert!(show.contains("Evil[2Jpatch"), "{show}");
}

#[test]
fn an_error_naming_hostile_text_is_stripped_too() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());
    let hostile_id = format!("no{ESC}[2J{CSI}such.mod");

    let mut cmd = common::rimmerge();
    cmd.args(["mods", "activate", &hostile_id]);
    path_args(&mut cmd, &game);
    let output = cmd.output().expect("spawn rimmerge");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(!output.status.success());
    assert_no_control_characters(&["mods", "activate"], &stderr);
    // `ModId` lowercases its text, hence `[2j`.
    assert!(stderr.contains("no[2jsuch.mod"), "{stderr}");
}

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
fn log_import_text_strips_control_characters_from_log_text() {
    let temp = tempdir().expect("tempdir");
    let game = hostile_game(temp.path());
    let log = fs::read_to_string(player_log_fixture())
        .expect("read the shared log fixture")
        .replace("Example Biomes", &format!("Example{ESC}[2J{CSI}Biomes"))
        .replace("ExampleDog.dds", &format!("Example{ESC}Dog.dds"))
        .replace("ExampleXenoPatch", &format!("Xeno{ESC}{CSI}Patch"));
    let hostile_log = temp.path().join("hostile.log");
    fs::write(&hostile_log, log).expect("write the hostile log");

    let hostile_path = hostile_log.to_str().expect("utf-8 path");
    let text = run_clean(&game, &["log", "import", hostile_path]);

    assert!(text.contains("Example[2JBiomes"), "{text}");
    assert!(text.contains("ExampleDog.dds"), "{text}");
    assert!(text.contains("XenoPatch"), "{text}");
}
