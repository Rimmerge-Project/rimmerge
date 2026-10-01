//! `rimmerge config show|set|clear` end to end, always against a
//! `tempfile` base directory (`--base`) — never the real
//! `%LOCALAPPDATA%\rimmerge`, and never the real install
//!.
//!
//! Every test clears `RIMMERGE_GAME_DIR`/`RIMMERGE_WORKSHOP_DIR`/
//! `RIMMERGE_MODS_CONFIG` on the spawned process
//! ([`Command::env_remove`]): the *point* of these cases is which rung of
//! the ladder answers, and an outer shell that happens to have the
//! real-install tier's own `RIMMERGE_GAME_DIR` exported would otherwise
//! silently outrank `config.json` and make them pass for the wrong
//! reason.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use tempfile::tempdir;

fn config_cmd() -> Command {
    let mut cmd = Command::cargo_bin("rimmerge").unwrap_or_else(|error| {
        panic!("locating the rimmerge binary: {error}");
    });
    cmd.arg("config")
        .env_remove("RIMMERGE_GAME_DIR")
        .env_remove("RIMMERGE_WORKSHOP_DIR")
        .env_remove("RIMMERGE_MODS_CONFIG");
    cmd
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A scratch directory shaped like a RimWorld install, so
/// `is_game_dir`-dependent output can be asserted without the real game.
///
/// Not a `#[test]` fn itself, so `clippy::expect_used`'s test allowance
/// doesn't reach it — the same reason `common::copy_dir_recursive`
/// propagates instead of `expect`ing.
fn scratch_install(root: &Path) -> std::path::PathBuf {
    let game_dir = root.join("game");
    fs::create_dir_all(game_dir.join("Data").join("Core"))
        .unwrap_or_else(|error| panic!("seeding Data/Core: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4518 rev435\n")
        .unwrap_or_else(|error| panic!("seeding Version.txt: {error}"));
    game_dir
}

#[test]
fn show_on_a_fresh_base_reports_nothing_pinned() {
    let dir = tempdir().expect("tempdir");

    let output = config_cmd()
        .arg("show")
        .arg("--base")
        .arg(dir.path())
        .output()
        .expect("running config show");

    assert!(output.status.success(), "{}", stdout_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("config.json"), "{stdout}");
    assert!(stdout.contains("(nothing pinned)"), "{stdout}");
}

#[test]
fn set_then_show_round_trips_every_path() {
    let dir = tempdir().expect("tempdir");
    let game_dir = scratch_install(dir.path());
    let workshop_dir = dir.path().join("workshop");
    let mods_config = dir.path().join("ModsConfig.xml");

    let set = config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(&workshop_dir)
        .arg("--mods-config")
        .arg(&mods_config)
        .output()
        .expect("running config set");
    assert!(set.status.success(), "{}", stdout_of(&set));

    let show = config_cmd()
        .arg("show")
        .arg("--base")
        .arg(dir.path())
        .output()
        .expect("running config show");
    let stdout = stdout_of(&show);

    assert!(show.status.success(), "{stdout}");
    assert!(
        stdout.contains(&game_dir.display().to_string()),
        "the pinned install must be listed: {stdout}"
    );
    assert!(
        stdout.contains(&workshop_dir.display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains(&mods_config.display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains("resolved:"),
        "a fully pinned config must resolve: {stdout}"
    );
}

#[test]
fn set_writes_the_documented_json_shape() {
    let dir = tempdir().expect("tempdir");
    let game_dir = scratch_install(dir.path());

    config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&game_dir)
        .output()
        .expect("running config set");

    let text = fs::read_to_string(dir.path().join("config.json")).expect("config.json must exist");
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["game_dir"], game_dir.display().to_string());
    assert!(value["workshop_dir"].is_null());
}

#[test]
fn set_only_changes_the_flags_it_is_given() {
    let dir = tempdir().expect("tempdir");
    let game_dir = scratch_install(dir.path());
    let mods_config = dir.path().join("ModsConfig.xml");

    config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&game_dir)
        .output()
        .expect("first set");
    config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--mods-config")
        .arg(&mods_config)
        .output()
        .expect("second set");

    let text = fs::read_to_string(dir.path().join("config.json")).expect("config.json");
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(
        value["game_dir"],
        game_dir.display().to_string(),
        "the first set's game-dir must survive the second set"
    );
    assert_eq!(value["mods_config"], mods_config.display().to_string());
}

#[test]
fn set_with_no_flags_is_an_error_rather_than_a_silent_no_op() {
    let dir = tempdir().expect("tempdir");

    let output = config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .output()
        .expect("running config set");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("nothing to set"), "{stderr}");
    assert!(
        !dir.path().join("config.json").exists(),
        "a refused set must not write the file"
    );
}

#[test]
fn setting_a_directory_that_is_not_an_install_warns_but_still_writes() {
    let dir = tempdir().expect("tempdir");
    let not_a_game = dir.path().join("empty");
    fs::create_dir_all(&not_a_game).expect("mkdir");

    let output = config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&not_a_game)
        .output()
        .expect("running config set");
    let stdout = stdout_of(&output);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("does not look like a RimWorld install"),
        "{stdout}"
    );
    assert!(dir.path().join("config.json").exists());
}

#[test]
fn clear_unpins_everything() {
    let dir = tempdir().expect("tempdir");
    let game_dir = scratch_install(dir.path());
    config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&game_dir)
        .output()
        .expect("seed set");

    let output = config_cmd()
        .arg("clear")
        .arg("--base")
        .arg(dir.path())
        .output()
        .expect("running config clear");
    assert!(output.status.success(), "{}", stdout_of(&output));

    let show = config_cmd()
        .arg("show")
        .arg("--base")
        .arg(dir.path())
        .output()
        .expect("running config show");
    assert!(
        stdout_of(&show).contains("(nothing pinned)"),
        "{}",
        stdout_of(&show)
    );
}

#[test]
fn the_environment_variable_outranks_the_pinned_config() {
    let dir = tempdir().expect("tempdir");
    let pinned = scratch_install(dir.path());
    let from_env = dir.path().join("from-env");
    config_cmd()
        .arg("set")
        .arg("--base")
        .arg(dir.path())
        .arg("--game-dir")
        .arg(&pinned)
        .arg("--mods-config")
        .arg(dir.path().join("ModsConfig.xml"))
        .output()
        .expect("seed set");

    let show = config_cmd()
        .arg("show")
        .arg("--base")
        .arg(dir.path())
        .env("RIMMERGE_GAME_DIR", &from_env)
        .output()
        .expect("running config show");
    let stdout = stdout_of(&show);

    assert!(
        stdout.contains(&format!("game-dir:     {}", from_env.display())),
        "RIMMERGE_GAME_DIR must outrank config.json in the resolved block: {stdout}"
    );
    assert!(
        stdout.contains(&format!("game-dir: {}", pinned.display())),
        "...while the pinned block still reports what is actually in the file: {stdout}"
    );
}
