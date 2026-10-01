//! `rimmerge network status`/`on`/`off`/`set`/`reset` end to end, against a
//! scratch `<base>/app-settings.json` — never the real profile, never
//! the network (every assertion here is a pure read/write of that one
//! JSON file; nothing in this file's own path ever reaches
//! `GithubRuleDatabaseFetcher`/`GithubReleaseFeed`).

use std::path::Path;

use assert_cmd::Command;
use tempfile::tempdir;

fn cmd(base: &Path) -> Command {
    let mut cmd =
        Command::cargo_bin("rimmerge").unwrap_or_else(|error| panic!("binary must build: {error}"));
    cmd.env("RIMMERGE_PROFILE_DIR", base);
    cmd
}

#[test]
fn on_with_a_per_source_flag_turns_off_only_that_source() {
    let temp_dir = tempdir().expect("tempdir");
    let base = temp_dir.path().join("base");

    cmd(&base)
        .args(["network", "on", "--community-rules", "off"])
        .assert()
        .success()
        .stdout(predicates::str::contains("fetch_community_rules: off"))
        .stdout(predicates::str::contains("fetch_steam_workshop: on"))
        .stdout(predicates::str::contains("fetch_rimmerge_rules: on"));

    // The write persisted, not just this call's own printed line.
    cmd(&base)
        .args(["network", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("fetch_community_rules: off"));
}

#[test]
fn steam_workshop_and_rimmerge_rules_flags_round_trip_independently() {
    let temp_dir = tempdir().expect("tempdir");
    let base = temp_dir.path().join("base");

    cmd(&base)
        .args([
            "network",
            "on",
            "--steam-workshop",
            "on",
            "--rimmerge-rules",
            "off",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("fetch_steam_workshop: on"))
        .stdout(predicates::str::contains("fetch_rimmerge_rules: off"))
        // Untouched by either flag, stays at its own default.
        .stdout(predicates::str::contains("fetch_community_rules: on"));
}

#[test]
fn set_turns_one_source_off_without_turning_internet_access_on() {
    let temp_dir = tempdir().expect("tempdir");
    let base = temp_dir.path().join("base");
    cmd(&base).args(["network", "off"]).assert().success();

    cmd(&base)
        .args(["network", "set", "--steam-workshop", "off"])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: off"))
        .stdout(predicates::str::contains("fetch_steam_workshop: off"))
        .stdout(predicates::str::contains("fetch_community_rules: on"));

    // The write persisted, master switch included.
    cmd(&base)
        .args(["network", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: off"))
        .stdout(predicates::str::contains("fetch_steam_workshop: off"));
}

#[test]
fn set_with_no_switch_is_refused_and_writes_nothing() {
    let temp_dir = tempdir().expect("tempdir");
    let base = temp_dir.path().join("base");

    cmd(&base)
        .args(["network", "set"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("name at least one switch"));

    assert!(
        !base.join("app-settings.json").exists(),
        "a refused set must not create the settings file"
    );
}

#[test]
fn reset_restores_every_switch_after_off_without_hand_editing_json() {
    let temp_dir = tempdir().expect("tempdir");
    let base = temp_dir.path().join("base");

    cmd(&base)
        .args([
            "network",
            "off",
            "--updates",
            "off",
            "--auto-refresh",
            "off",
            "--community-rules",
            "off",
            "--steam-workshop",
            "on",
            "--rimmerge-rules",
            "off",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: off"));

    cmd(&base)
        .args(["network", "reset"])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: on"))
        .stdout(predicates::str::contains("check_for_updates: on"))
        .stdout(predicates::str::contains("auto_refresh_rule_databases: on"))
        .stdout(predicates::str::contains("fetch_community_rules: on"))
        .stdout(predicates::str::contains("fetch_steam_workshop: on"))
        .stdout(predicates::str::contains("fetch_rimmerge_rules: on"));

    // The reset persisted, not just this call's own printed line.
    cmd(&base)
        .args(["network", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: on"));
}
