//! Real-install invariants for
//! `ScanOutput.inactive_mods`/`discovered_mod_count`: every inactive id is
//! disjoint from the active/missing sets, sorted, and stable across two
//! independent scans of the same install. Read-only — nothing here writes
//! anywhere, and no `#[ignore]`d test in this file ever touches
//! `ModsConfig.xml`.
//!
//! `#[ignore]`d, gated by [`common`]'s three-check tier guard (unset
//! `RIMMERGE_GAME_DIR` skips; set to something that isn't an install panics;
//! a vanilla/reset `ModsConfig.xml` below the active-mod floor also panics —
//! see `common`'s own doc comment).
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::HashSet;

use rim_analyzer::domain::ModId;
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn inactive_mods_are_disjoint_sorted_and_deterministic_against_the_real_install() {
    let Some((config, _game_version)) = common::real_scan_config(RERUN_COMMAND) else {
        return;
    };

    let output_one =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));

    // `discovered_mod_count` covers every directory found — active
    // (`scanned_mods`) and inactive alike. `missing_mods` (an active id
    // with no directory on disk) contributes to neither side of this
    // equation, so the two must sum to the total exactly.
    assert_eq!(
        output_one.inactive_mods.len(),
        output_one.discovered_mod_count - output_one.scanned_mods.len(),
        "inactive_mods.len() + scanned_mods.len() must equal discovered_mod_count"
    );

    let active_ids: HashSet<ModId> = output_one
        .scanned_mods
        .iter()
        .map(|sm| sm.info.id.clone())
        .collect();
    let missing_ids: HashSet<ModId> = output_one.missing_mods.iter().cloned().collect();
    for inactive in &output_one.inactive_mods {
        assert!(
            !active_ids.contains(&inactive.id),
            "{} is both scanned (active) and inactive",
            inactive.id
        );
        assert!(
            !missing_ids.contains(&inactive.id),
            "{} is both missing (active-but-absent) and inactive (present-but-inactive)",
            inactive.id
        );
    }

    let ids: Vec<&ModId> = output_one.inactive_mods.iter().map(|m| &m.id).collect();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    assert_eq!(ids, sorted_ids, "inactive_mods must be sorted by id");

    // Determinism: a second, independent scan of the same install must
    // report the identical inactive list — this crate's own byte-
    // identical-JSON contract (root `CLAUDE.md`) extended to this field.
    let output_two = infra::scan(&config)
        .unwrap_or_else(|error| panic!("re-scanning the real install: {error}"));
    let ids_two: Vec<&ModId> = output_two.inactive_mods.iter().map(|m| &m.id).collect();
    assert_eq!(
        ids, ids_two,
        "two scans of the same install must report the identical inactive_mods list"
    );
    assert_eq!(
        output_one.discovered_mod_count,
        output_two.discovered_mod_count
    );
}
