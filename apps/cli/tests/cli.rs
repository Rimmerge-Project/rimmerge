//! End-to-end CLI tests over the JSON-in commands (`sort`/`ledger`).

use std::path::PathBuf;

mod common;

fn fixture_report() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tiny_report.json")
}

#[test]
fn sort_prints_the_suggested_order_and_disturbance_stats() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg(fixture_report())
        .assert()
        .success()
        .stdout(predicates::str::contains("Suggested order (2 mods)"))
        .stdout(predicates::str::contains("framework.mod"))
        .stdout(predicates::str::contains("addon.mod"))
        .stdout(predicates::str::contains("Disturbance vs current order:"));
}

#[test]
fn sort_reorders_the_declared_dependency_ahead_of_its_dependent() {
    let mut cmd = common::rimmerge();
    let output = cmd
        .arg("sort")
        .arg("--report")
        .arg(fixture_report())
        .output()
        .expect("command must run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let framework_line = stdout
        .lines()
        .find(|line| line.contains("framework.mod"))
        .expect("framework.mod must be listed");
    let addon_line = stdout
        .lines()
        .find(|line| line.contains("addon.mod"))
        .expect("addon.mod must be listed");
    assert!(
        framework_line.trim_start().starts_with('1'),
        "framework.mod must move to the front: {framework_line:?}"
    );
    assert!(
        addon_line.trim_start().starts_with('2'),
        "addon.mod must follow it: {addon_line:?}"
    );
}

#[test]
fn ledger_prints_a_breakdown_and_the_undeclared_hard_dependency_finding() {
    let mut cmd = common::rimmerge();
    cmd.arg("ledger")
        .arg("--report")
        .arg(fixture_report())
        .assert()
        .success()
        .stdout(predicates::str::contains("Ledger for Current"))
        .stdout(predicates::str::contains("auto"))
        .stdout(predicates::str::contains("needs_input"));
}

#[test]
fn sort_accepts_preserve_current_tie_break_and_still_satisfies_the_declared_edge() {
    let mut cmd = common::rimmerge();
    let output = cmd
        .arg("sort")
        .arg("--report")
        .arg(fixture_report())
        .arg("--tie-break")
        .arg("preserve-current")
        .output()
        .expect("command must run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let framework_line = stdout
        .lines()
        .find(|line| line.contains("framework.mod"))
        .expect("framework.mod must be listed");
    let addon_line = stdout
        .lines()
        .find(|line| line.contains("addon.mod"))
        .expect("addon.mod must be listed");
    assert!(
        framework_line.trim_start().starts_with('1'),
        "the Hard/Declared edge must hold under either tie-break mode: {framework_line:?}"
    );
    assert!(addon_line.trim_start().starts_with('2'));
}

#[test]
fn sort_clap_accepts_the_inert_import_toggle_flags() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg(fixture_report())
        .arg("--use-imported-pairs")
        .arg("--no-imported-placements")
        .assert()
        .success()
        .stdout(predicates::str::contains("Suggested order (2 mods)"));
}

#[test]
fn ledger_clap_accepts_the_tie_break_flag_and_inert_import_toggle_flags() {
    let mut cmd = common::rimmerge();
    cmd.arg("ledger")
        .arg("--report")
        .arg(fixture_report())
        .arg("--tie-break")
        .arg("preserve-current")
        .arg("--use-imported-pairs")
        .arg("--no-imported-placements")
        .assert()
        .success()
        .stdout(predicates::str::contains("Ledger for Current"));
}

#[test]
fn sort_fails_gracefully_on_a_missing_report_file() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg("does-not-exist.json")
        .assert()
        .failure();
}

#[test]
fn fixture_trim_caps_conflicts_per_kind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("trimmed.json");
    let mut cmd = common::rimmerge();
    cmd.arg("fixture")
        .arg("trim")
        .arg("--in")
        .arg(fixture_report())
        .arg("--out")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicates::str::contains("trimmed 0 conflicts"));

    assert!(output.is_file());
}

/// The shared command builder must keep every CLI test off the machine's
/// own app-global state: here an ambient `LOCALAPPDATA` holds a saved
/// `app-settings.json` with the master switch off, and `network status`
/// through [`common::rimmerge`] must still report the shipped default (on).
#[test]
fn the_shared_command_builder_ignores_the_machines_own_app_settings() {
    let ambient = tempfile::tempdir().expect("tempdir");
    let ambient_base = ambient.path().join("rimmerge");
    std::fs::create_dir_all(&ambient_base).expect("create ambient base");
    std::fs::write(
        ambient_base.join("app-settings.json"),
        r#"{"schema": 1, "network": {"allow_network": false, "check_for_updates": false,
            "auto_refresh_rule_databases": false, "fetch_community_rules": false,
            "fetch_steam_workshop": false, "fetch_rimmerge_rules": false},
            "reminders": {"rule_databases_stale_after_days": 30}}"#,
    )
    .expect("seed the ambient settings");

    common::rimmerge()
        .env("LOCALAPPDATA", ambient.path())
        .args(["network", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("allow_network: on"));
}

fn contradiction_report() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("contradiction_report.json")
}

#[test]
fn sort_dropped_lists_the_losing_edge_its_cycle_and_the_edge_that_overruled_it() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg(contradiction_report())
        .arg("--dropped")
        .assert()
        .success()
        .stdout(predicates::str::contains("Dropped edges (1 of 1):"))
        .stdout(predicates::str::contains(
            "addon.mod <- framework.mod  [declared] load_after:",
        ))
        .stdout(predicates::str::contains(
            "cycle: addon.mod -> framework.mod",
        ))
        .stdout(predicates::str::contains(
            "overruled by: framework.mod <- addon.mod  [hard] assembly_ref:",
        ));
}

#[test]
fn sort_dropped_with_a_mod_filter_for_an_unrelated_mod_lists_nothing() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg(contradiction_report())
        .args(["--dropped", "--mod", "unrelated.mod"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Dropped edges (0 of 1):"));
}

#[test]
fn sort_mod_filter_without_dropped_is_a_usage_error() {
    let mut cmd = common::rimmerge();
    cmd.arg("sort")
        .arg("--report")
        .arg(fixture_report())
        .args(["--mod", "addon.mod"])
        .assert()
        .failure();
}
