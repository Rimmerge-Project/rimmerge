//! Real-install timing guard for the per-mod scan every project load,
//! Rescan and load-order import starts with: `infra::scan` (discovery, every
//! active mod's loaded folders, its root survey, and the game's own
//! managed-assembly and resource reads). The post-scan analysis has its own
//! guard in `real_install_post_scan_analysis.rs`. Two scans of the same
//! install must also agree on what they found (the determinism contract,
//! here over the scan's parallel per-mod work against real data).
//!
//! `#[ignore]`d, gated by [`common`]'s three-check tier guard (unset
//! `RIMMERGE_GAME_DIR` skips; set to something that isn't an install panics;
//! a vanilla/reset `ModsConfig.xml` below the active-mod floor also panics —
//! see `common`'s own doc comment). Read-only: nothing here writes anywhere.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::time::{Duration, Instant};

use rim_analyzer::domain::ScanOutput;
use rim_analyzer::extract::mods_config::parse_active_mods;
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// The budget for one warm `infra::scan` of a real install of about 1,000
/// active mods, in a release build. Measured at about 6 s alone on the
/// reference install (10 s before issue #27 cut its file-system calls, and
/// over 100 s on a cold file cache, which is why the timed scan runs
/// second). Generous on purpose: the rest of this tier scans the same
/// install on the other test threads, and this only has to catch the scan
/// regressing to a shape several times slower — a per-entry handle open
/// again, or a quadratic walk.
const SCAN_BUDGET: Duration = Duration::from_secs(60);

/// What one scan found, per mod and in total — enough to tell two scans
/// apart without holding both whole scans in memory at once.
#[derive(Debug, PartialEq, Eq)]
struct ScanFingerprint {
    per_mod: Vec<(String, [usize; 7])>,
    warnings: Vec<String>,
    unloaded_defs_dirs: usize,
    core_resource_textures: usize,
    vanilla_types: usize,
}

impl ScanFingerprint {
    fn of(scan: &ScanOutput) -> Self {
        let per_mod = scan
            .scanned_mods
            .iter()
            .map(|scanned| {
                let id = scanned.info.id.as_str();
                let ref_sites = scan
                    .ref_sites_by_mod
                    .get(&scanned.info.id)
                    .map_or(0, Vec::len);
                (
                    id.to_string(),
                    [
                        scanned.defs.len(),
                        scanned.templates.len(),
                        scanned.patch_ops.len(),
                        scanned.textures.len(),
                        scanned.assemblies.len(),
                        scanned.undecodable_textures.len(),
                        ref_sites,
                    ],
                )
            })
            .collect();
        Self {
            per_mod,
            warnings: scan.warnings.iter().map(|w| w.message.clone()).collect(),
            unloaded_defs_dirs: scan.unloaded_defs_dirs_by_mod.values().map(Vec::len).sum(),
            core_resource_textures: scan.core_resource_textures.len(),
            vanilla_types: scan.vanilla_type_hierarchy.len(),
        }
    }
}

#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn scan_of_the_real_install_is_fast_and_deterministic() {
    let Some((mut config, _)) = common::real_scan_config(RERUN_COMMAND) else {
        return;
    };
    // Both scans take the active list read once here: the game rewrites
    // `ModsConfig.xml` when it closes, and a list changed between the two
    // scans would read as a determinism failure.
    let bytes = std::fs::read(&config.mods_config_path)
        .unwrap_or_else(|error| panic!("reading the real ModsConfig.xml: {error}"));
    let active = parse_active_mods(&bytes)
        .unwrap_or_else(|error| panic!("parsing the real ModsConfig.xml: {error}"));
    config.active_mods = Some(active);

    // The first scan is untimed: it also warms the OS file cache, and a
    // cold scan measures the disk, not the code under guard.
    let first = ScanFingerprint::of(
        &infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}")),
    );
    let started = Instant::now();
    let scan =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));
    let elapsed = started.elapsed();
    eprintln!(
        "infra::scan: {:.1} ms over {} scanned mods",
        elapsed.as_secs_f64() * 1e3,
        scan.scanned_mods.len()
    );
    assert!(
        elapsed < SCAN_BUDGET,
        "the scan took {elapsed:?}, over its {SCAN_BUDGET:?} budget"
    );

    assert!(
        ScanFingerprint::of(&scan) == first,
        "two scans of the same install found different content"
    );
}
