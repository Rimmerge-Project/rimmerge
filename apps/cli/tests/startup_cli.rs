//! End-to-end tests for `rimmerge startup` — JSON-in,
//! same fixture `sort`/`ledger`'s own `cli.rs` uses.

use std::path::PathBuf;

mod common;

fn fixture_report() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tiny_report.json")
}

/// A schema-5 report (`mods` non-empty, no `mod_costs` key at all) — the
/// exact shape of a cached `report.json` scanned before startup costs existed.
fn report_predating_mod_costs() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("report_predating_mod_costs.json")
}

/// `Report.mod_costs` is `#[serde(default)]`, so a schema-5 report with
/// real mods but no `mod_costs` key deserializes as `mod_costs: []`;
/// rendering that as "0 mods, 0 B" would be a false "this install costs
/// nothing" at exit 0. `startup` must instead fail fast, naming the schema
/// gap and the regeneration command.
#[test]
fn startup_refuses_a_report_that_predates_mod_costs_instead_of_rendering_zero() {
    let mut cmd = common::rimmerge();
    cmd.arg("startup")
        .arg("--report")
        .arg(report_predating_mod_costs())
        .assert()
        .failure()
        .stderr(predicates::str::contains("predates startup costs"))
        .stderr(predicates::str::contains("analyze --json"));
}

#[test]
fn startup_prints_a_row_per_mod_and_a_totals_row() {
    let mut cmd = common::rimmerge();
    cmd.arg("startup")
        .arg("--report")
        .arg(fixture_report())
        .assert()
        .success()
        .stdout(predicates::str::contains("Startup cost (2 mods)"))
        .stdout(predicates::str::contains("Addon Mod"))
        .stdout(predicates::str::contains("Framework Mod"))
        .stdout(predicates::str::contains("TOTAL"));
}

#[test]
fn startup_defaults_to_sorting_by_descending_texture_bytes() {
    let mut cmd = common::rimmerge();
    let output = cmd
        .arg("startup")
        .arg("--report")
        .arg(fixture_report())
        .output()
        .expect("command must run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let framework_line = stdout
        .lines()
        .find(|line| line.contains("Framework Mod"))
        .expect("Framework Mod must be listed");
    let addon_line = stdout
        .lines()
        .find(|line| line.contains("Addon Mod"))
        .expect("Addon Mod must be listed");
    let framework_index = stdout.lines().position(|l| l == framework_line).unwrap();
    let addon_index = stdout.lines().position(|l| l == addon_line).unwrap();
    assert!(
        framework_index < addon_index,
        "Framework Mod (5000 texture bytes) must sort before Addon Mod (500) by default"
    );
}

#[test]
fn startup_sort_by_flag_reorders_by_the_chosen_column() {
    let mut cmd = common::rimmerge();
    let output = cmd
        .arg("startup")
        .arg("--report")
        .arg(fixture_report())
        .arg("--sort-by")
        .arg("assembly-bytes")
        .output()
        .expect("command must run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let framework_index = stdout
        .lines()
        .position(|l| l.contains("Framework Mod"))
        .expect("Framework Mod must be listed");
    let addon_index = stdout
        .lines()
        .position(|l| l.contains("Addon Mod"))
        .expect("Addon Mod must be listed");
    assert!(
        framework_index < addon_index,
        "framework.mod ships the only assembly bytes, so it must sort first"
    );
}

#[test]
fn startup_json_carries_raw_byte_counts_and_flags() {
    let mut cmd = common::rimmerge();
    let output = cmd
        .arg("startup")
        .arg("--report")
        .arg(fixture_report())
        .arg("--json")
        .output()
        .expect("command must run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");

    let mods = json["mods"].as_array().expect("mods array");
    assert_eq!(mods.len(), 2);
    let addon = mods
        .iter()
        .find(|m| m["mod_id"] == "addon.mod")
        .expect("addon.mod row");
    assert_eq!(addon["texture_bytes"], 500);
    assert_eq!(addon["content_only"], true);

    let framework = mods
        .iter()
        .find(|m| m["mod_id"] == "framework.mod")
        .expect("framework.mod row");
    assert_eq!(framework["texture_bytes"], 5000);
    assert_eq!(framework["overridden_texture_bytes"], 100);
    assert_eq!(framework["content_only"], false);

    assert_eq!(json["totals"]["texture_bytes"], 5500);
    assert_eq!(json["totals"]["patch_ops"], 3);
}
