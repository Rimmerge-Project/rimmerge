//! Hermetic real-pipeline end-to-end test: generates a complete synthetic
//! install from the
//! committed spec, then runs `load`, `sort`, `ledger`, `defs changes`,
//! `merge coverage`, `verify --source suggested --json`, and
//! `apply --dry-run` against it — the hermetic tier that stands in for
//! the real install in every ordinary `cargo nextest run --workspace`
//! (**not** `#[ignore]`d, unlike every `real_install_*.rs` file in this
//! crate). No network, no real game install, no real `ModsConfig.xml`.
//!
//! **Disclosed gap**: `assign propose/create/set-row/coverage/export` is
//! not exercised here. The assignment framework needs a candidate def
//! type shaped with a `TargetKey` field (`rim_resolve::domain::
//! AssignmentSchema`'s own classification rules) — `rim-io`'s own
//! `assign_game` fixture is
//! purpose-built for exactly that shape. The synthetic install generator
//! ships `example.PartAssignmentDef` instances (for the
//! `PatchInvalidatesPredicate` construct only) with no such field, so
//! `assign propose` finds no real candidate against it. Building that
//! shape into the generator is left out deliberately — a disclosed gap,
//! not a silent omission, and `assign_game`'s own coverage of
//! the assignment framework is unaffected by this file either way.

use std::path::{Path, PathBuf};
use std::time::Instant;

use tempfile::tempdir;

mod common;

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-resolve")
        .join("tests")
        .join("fixtures")
        .join("synthetic-install.json")
}

fn rimmerge() -> common::CliCommand {
    common::rimmerge()
}

struct Install {
    game_dir: PathBuf,
    workshop_dir: PathBuf,
    mods_config: PathBuf,
}

fn generate(out: &Path) -> Install {
    rimmerge()
        .args(["fixture", "gen", "--spec"])
        .arg(spec_path())
        .args(["--out"])
        .arg(out)
        .assert()
        .success();
    Install {
        game_dir: out.join("game"),
        workshop_dir: out.join("workshop").join("content").join("294100"),
        mods_config: out.join("game").join("ModsConfig.xml"),
    }
}

fn install_args(install: &Install, profile_dir: &Path) -> Vec<String> {
    vec![
        "--game-dir".to_string(),
        install.game_dir.display().to_string(),
        "--workshop-dir".to_string(),
        install.workshop_dir.display().to_string(),
        "--mods-config".to_string(),
        install.mods_config.display().to_string(),
        "--profile-dir".to_string(),
        profile_dir.display().to_string(),
    ]
}

/// The full real-pipeline sequence, run once against
/// `profile_dir` and returning the `verify --json` output (used both for
/// the anti-vacuous-green floors and the determinism check).
fn run_pipeline(install: &Install, profile_dir: &Path) -> Vec<u8> {
    let args = install_args(install, profile_dir);

    rimmerge().arg("load").args(&args).assert().success();

    let report_path = profile_dir.join("report.json");
    rimmerge()
        .args(["sort", "--report"])
        .arg(&report_path)
        .assert()
        .success();
    rimmerge()
        .args(["ledger", "--report"])
        .arg(&report_path)
        .assert()
        .success();
    rimmerge()
        .args(["defs", "changes", "synth.dependent.001"])
        .args(&args)
        .assert()
        .success();
    rimmerge()
        .args(["merge", "coverage"])
        .args(&args)
        .assert()
        .success();
    rimmerge()
        .args(["apply", "--dry-run"])
        .args(&args)
        .assert()
        .success();

    let output = rimmerge()
        .args(["verify", "--source", "suggested", "--json"])
        .args(&args)
        .assert()
        .success();
    output.get_output().stdout.clone()
}

/// The full pipeline, hermetic (no network, no real install), producing
/// a JSON report the anti-vacuous-green floors below check -- and, run
/// twice into two separate profile directories from the same generated
/// install, proving the determinism contract (root `CLAUDE.md`: "the
/// analyzer's JSON report and the sorter's output must be byte-identical
/// across runs").
#[test]
fn the_synthetic_pipeline_is_real_nontrivial_and_deterministic() {
    let root = tempdir().unwrap_or_else(|e| panic!("creating tempdir: {e}"));
    let install = generate(&root.path().join("install"));

    let start = Instant::now();
    let verify_json_1 = run_pipeline(&install, &root.path().join("profile1"));
    let elapsed = start.elapsed();
    let verify_json_2 = run_pipeline(&install, &root.path().join("profile2"));

    assert_eq!(
        verify_json_1, verify_json_2,
        "verify --json must be byte-identical across two independent runs against the same \
         generated install"
    );

    // The scanned `report.json` itself, not just `verify`'s own output --
    // the determinism contract (root `CLAUDE.md`) is
    // about the analyzer's report, and `metadata.generated_at` is the
    // one field that's *expected* to differ (an RFC 3339 timestamp of
    // when each `load` ran), so it's dropped before comparing.
    let mut report_1: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("profile1").join("report.json"))
            .unwrap_or_else(|e| panic!("reading profile1's report.json: {e}")),
    )
    .unwrap_or_else(|e| panic!("parsing profile1's report.json: {e}"));
    let mut report_2: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("profile2").join("report.json"))
            .unwrap_or_else(|e| panic!("reading profile2's report.json: {e}")),
    )
    .unwrap_or_else(|e| panic!("parsing profile2's report.json: {e}"));
    for report in [&mut report_1, &mut report_2] {
        report["metadata"]["generated_at"] = serde_json::Value::Null;
    }
    assert_eq!(
        report_1, report_2,
        "report.json must be byte-identical (aside from generated_at) across two independent \
         runs against the same generated install"
    );

    let verify: serde_json::Value = serde_json::from_slice(&verify_json_1)
        .unwrap_or_else(|e| panic!("parsing verify --json output: {e}"));

    // --- anti-vacuous-green floors ------------------------------------
    let report: rim_analyzer::domain::Report = serde_json::from_slice(
        &std::fs::read(root.path().join("profile1").join("report.json"))
            .unwrap_or_else(|e| panic!("reading report.json: {e}")),
    )
    .unwrap_or_else(|e| panic!("parsing report.json: {e}"));

    assert!(
        report.mods.len() >= 200,
        "expected >= 200 active mods, got {}",
        report.mods.len()
    );
    assert!(
        !report.constraints.is_empty(),
        "expected at least one any-of constraint, got none"
    );

    let sort_output = rimmerge()
        .args(["sort", "--report"])
        .arg(root.path().join("profile1").join("report.json"))
        .assert()
        .success();
    let sort_stdout = String::from_utf8_lossy(&sort_output.get_output().stdout).into_owned();
    assert!(
        sort_stdout.contains("dropped edges: ") && !sort_stdout.contains("dropped edges: 0,"),
        "expected at least one dropped edge, got: {sort_stdout}"
    );

    let findings = verify
        .get("operations")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("verify --json has no \"operations\" array: {verify}"));
    assert!(
        !findings.is_empty(),
        "expected at least one predicted patch failure (verify --json \"operations\"), got none"
    );

    // A floor wall time -- the same "did this tier actually run" tell
    // root `CLAUDE.md` documents for the real-install tier, scaled down
    // for a hermetic, ~400-mod install: a run finishing in effectively
    // zero time did not really scan/sort/replay anything.
    assert!(
        elapsed.as_millis() >= 50,
        "the pipeline finished suspiciously fast ({elapsed:?}) for a ~{}-mod install -- did it \
         actually run?",
        report.mods.len()
    );

    eprintln!(
        "synthetic pipeline: {} mods, {} constraints, {} predicted patch failures in {elapsed:?}",
        report.mods.len(),
        report.constraints.len(),
        findings.len()
    );
}
