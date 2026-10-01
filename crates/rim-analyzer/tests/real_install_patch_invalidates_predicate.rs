//! The real acceptance case for `EdgeKind::PatchInvalidatesPredicate` — two
//! real mods, one replacing a predicate step's own key child, the other
//! adding to the same predicate step, produce an in-game failure unless this
//! edge orders them.
//!
//! **No third-party mod is named here**: the shape/band assertions below
//! always run; `RIMMERGE_EXPECTED_PREDICATE_PAIR=<after mod id>,<before mod
//! id>` additionally pins the exact acceptance pair for a maintainer's own
//! install, mirroring `RIMMERGE_EXPECTED_CONTRIBUTES_NOTHING`.
//!
//! `#[ignore]`d: reads the real game/workshop install and the real
//! `ModsConfig.xml` (read-only), requires `RIMMERGE_PERF_PROFILE_DIR`
//! (this crate's own scan has no profile concept, but every test in this
//! documented tier sets it — see `common`'s own doc comment).
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> RIMMERGE_EXPECTED_PREDICATE_PAIR=<after>,<before> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use rim_analyzer::analysis::{self, RunContext};
use rim_analyzer::domain::{EdgeKind, ModId};
use rim_analyzer::infra;

mod common;

const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// This install's own general population is under 20 predicate-
/// invalidation false positives; a real regression in the producer's own
/// `A`-side tolerance gate (see its doc comment) widens this well past
/// that on a large install.
const MAX_PLAUSIBLE_PREDICATE_EDGES: usize = 20;

#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn predicate_invalidation_edges_are_bounded_and_match_the_pinned_pair_when_set() {
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
    let report = analysis::build_ref(&scan, &context);

    let predicate_edges: Vec<_> = report
        .edges
        .iter()
        .map(|edge_report| &edge_report.edge)
        .filter(|edge| edge.kind == EdgeKind::PatchInvalidatesPredicate)
        .collect();
    eprintln!(
        "PatchInvalidatesPredicate: {} edges on this install",
        predicate_edges.len()
    );
    assert!(
        predicate_edges.len() <= MAX_PLAUSIBLE_PREDICATE_EDGES,
        "expected at most {MAX_PLAUSIBLE_PREDICATE_EDGES} PatchInvalidatesPredicate edges — a \
         count this high suggests the producer's own `A`-side tolerance gate regressed (see its \
         doc comment): {predicate_edges:?}"
    );

    let Some(pinned_pair) =
        common::require_pin_var("RIMMERGE_EXPECTED_PREDICATE_PAIR", RERUN_COMMAND)
    else {
        return;
    };
    let (after, before) = pinned_pair.split_once(',').unwrap_or_else(|| {
        panic!("RIMMERGE_EXPECTED_PREDICATE_PAIR must be '<after mod id>,<before mod id>', got {pinned_pair}")
    });
    let (after, before) = (ModId::new(after), ModId::new(before));

    let active: std::collections::BTreeSet<ModId> = scan
        .scanned_mods
        .iter()
        .map(|scanned_mod| scanned_mod.info.id.clone())
        .collect();
    if !active.contains(&after) || !active.contains(&before) {
        eprintln!(
            "skipping the exact-pin check: {after}/{before} are not both active on this \
             install. Run: {RERUN_COMMAND}"
        );
        return;
    }

    let acceptance = predicate_edges
        .iter()
        .find(|edge| edge.after == after && edge.before == before);
    assert!(
        acceptance.is_some(),
        "expected a PatchInvalidatesPredicate edge after={after} before={before} — not found \
         among {} edges of this kind: {predicate_edges:?}",
        predicate_edges.len()
    );
}
