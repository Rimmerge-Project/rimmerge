//! Real-install timing guard for the post-scan analysis step every project
//! load, Rescan and load-order import runs after the per-mod scan:
//! `infra::build_explained_report` (`analysis::build_ref` plus the lazy
//! dangling-reference explanation, whose candidate-folder walk overlaps
//! `build_ref`). The scan itself is not timed — only the analysis over it.
//! Two builds over the same scan must also produce identical reports (the
//! determinism contract, here over the step's parallel parts against real
//! data).
//!
//! `#[ignore]`d, gated by [`common`]'s three-check tier guard (unset
//! `RIMMERGE_GAME_DIR` skips; set to something that isn't an install panics;
//! a vanilla/reset `ModsConfig.xml` below the active-mod floor also panics —
//! see `common`'s own doc comment). Read-only: nothing here writes anywhere.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::time::{Duration, Instant};

use rim_analyzer::analysis::RunContext;
use rim_analyzer::domain::Report;
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// The budget for one warm `build_explained_report` over a real install of
/// about 1,000 active mods, in a release build. Measured at about 3 to 4 s
/// alone on the reference install since `build_ref`'s two sides and the
/// dangling-reference vote run in parallel (4 to 5.5 s before that), and
/// up to 13 s while the rest of this tier scans the install on the other
/// test threads (down from about 25 s alone before the step was profiled:
/// the explanation's folder walk by itself took 15 s). The margin absorbs
/// that contention; the walk or the vote regressing to its old shape still
/// fails.
const BUILD_BUDGET: Duration = Duration::from_secs(20);

/// `report` as JSON with the one per-run field, `metadata.generated_at`,
/// blanked.
fn normalized_json(report: &Report) -> serde_json::Value {
    let mut value = serde_json::to_value(report)
        .unwrap_or_else(|error| panic!("serializing the report: {error}"));
    value["metadata"]["generated_at"] = serde_json::Value::Null;
    value
}

#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn post_scan_analysis_of_the_real_install_is_fast_and_deterministic() {
    let Some((config, game_version)) = common::real_scan_config(RERUN_COMMAND) else {
        return;
    };
    let scan =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));
    let context = RunContext {
        game_dir: config.game_dir.clone(),
        workshop_dir: config.workshop_dir.clone(),
        mods_config_path: config.mods_config_path.clone(),
        game_version,
    };

    // The first build is untimed: it also warms the OS file cache for the
    // explanation's candidate files (unloaded folders, inactive mods),
    // which the scan never reads. Cold, right after a large build has
    // evicted them, the same step measured three times slower, which is
    // disk speed, not the code under guard.
    let warm_up = infra::build_explained_report(&scan, &context);
    let started = Instant::now();
    let report = infra::build_explained_report(&scan, &context);
    let elapsed = started.elapsed();
    eprintln!(
        "build_explained_report: {:.1} ms over {} scanned mods, {} conflicts",
        elapsed.as_secs_f64() * 1e3,
        scan.scanned_mods.len(),
        report.conflicts.len()
    );
    assert!(
        elapsed < BUILD_BUDGET,
        "the post-scan analysis took {elapsed:?}, over its {BUILD_BUDGET:?} budget"
    );

    assert!(
        normalized_json(&report) == normalized_json(&warm_up),
        "two builds over the same scan produced different reports"
    );
}
