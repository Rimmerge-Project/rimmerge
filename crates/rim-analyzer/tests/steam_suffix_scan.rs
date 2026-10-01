//! End-to-end coverage of RimWorld's `_steam` suffix: when a local and a
//! Steam Workshop copy of the same packageId both exist on disk, RimWorld
//! appends `_steam` to the Workshop copy's id in `ModsConfig.xml`. Both
//! copies must scan as distinct mods with distinct paths, and a bare
//! declared id (as in `About.xml`) must still resolve to the correct
//! active copy for edge-building — see `tests/fixtures/steam_suffix_game/`.

use std::collections::HashMap;
use std::path::PathBuf;

use rim_analyzer::domain::{EdgeKind, EdgeStatus, FolderPolicy, GameVersion, ModId, ScanStage};
use rim_analyzer::{analysis, infra};

fn game_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/steam_suffix_game")
}

fn workshop_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/steam_suffix_workshop")
}

fn scan_config(mods_config_file: &str) -> infra::ScanConfig {
    infra::ScanConfig {
        game_dir: game_dir(),
        workshop_dir: workshop_dir(),
        mods_config_path: game_dir().join(mods_config_file),
        game_version: GameVersion::new(1, 6),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    }
}

fn run_context(mods_config_file: &str) -> analysis::RunContext {
    analysis::RunContext {
        game_dir: game_dir(),
        workshop_dir: workshop_dir(),
        mods_config_path: game_dir().join(mods_config_file),
        game_version: GameVersion::new(1, 6),
    }
}

#[test]
fn dup_local_and_workshop_copies_are_scanned_as_distinct_mods() {
    let output = infra::scan(&scan_config("ModsConfig_dup_active.xml")).expect("scan must succeed");

    assert!(output.missing_mods.is_empty());
    assert_eq!(output.scanned_mods.len(), 2);

    let paths_by_id: HashMap<ModId, PathBuf> = output
        .scanned_mods
        .iter()
        .map(|sm| (sm.info.id.clone(), sm.info.path.clone()))
        .collect();

    let local_path = paths_by_id
        .get(&ModId::new("dup.mod"))
        .expect("local copy present under its exact ModsConfig id");
    let workshop_path = paths_by_id
        .get(&ModId::new("dup.mod_steam"))
        .expect("workshop copy present under its exact, _steam-suffixed id");
    assert_ne!(local_path, workshop_path);
    assert!(
        local_path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("Mods/DupMod")
    );
    assert!(
        workshop_path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("111111")
    );

    // Neither copy's Mod::id was rewritten to the bare "dup.mod" for both,
    // so there is no false def/patch/texture/assembly collision between
    // what would otherwise look like the same mod scanned twice.
    let report = analysis::build(output, &run_context("ModsConfig_dup_active.xml"));
    assert!(report.conflicts.is_empty());
    assert_eq!(report.mods.len(), 2);
}

#[test]
fn scan_with_progress_ticks_once_per_mod_and_preserves_scan_order() {
    let mut scanning_ticks = Vec::new();
    let output = infra::scan_with_progress(&scan_config("ModsConfig_dup_active.xml"), &mut |p| {
        if p.stage == ScanStage::Scanning {
            scanning_ticks.push((p.done, p.total));
        }
    })
    .expect("scan must succeed");

    // Two active mods: exactly two Scanning ticks, strictly increasing,
    // ending at done == total == 2.
    assert_eq!(scanning_ticks, vec![(1, 2), (2, 2)]);

    // `ModsConfig.xml` lists "dup.mod" before "dup.mod_steam" — the
    // progress-hook plumbing must not have disturbed rayon's
    // input-order-preserving collect (see `scan_all_with_progress`'s doc
    // comment), which is what keeps a scan's JSON output deterministic.
    let ids: Vec<ModId> = output
        .scanned_mods
        .iter()
        .map(|sm| sm.info.id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![ModId::new("dup.mod"), ModId::new("dup.mod_steam")]
    );
}

#[test]
fn steam_suffixed_id_resolves_to_the_workshop_copy_with_evaluated_edges() {
    let output = infra::scan(&scan_config("ModsConfig_steam_only.xml")).expect("scan must succeed");

    assert!(output.missing_mods.is_empty());
    assert_eq!(output.scanned_mods.len(), 2);

    let foo_bar = output
        .scanned_mods
        .iter()
        .find(|sm| sm.info.id == ModId::new("foo.bar_steam"))
        .expect("bare 'foo.bar_steam' ModsConfig entry resolves to the workshop copy");
    assert!(
        foo_bar
            .info
            .path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("222222"),
        "expected the workshop copy's path, got {}",
        foo_bar.info.path.display()
    );

    let report = analysis::build(output, &run_context("ModsConfig_steam_only.xml"));

    // dep.mod's modDependencies names the bare "foo.bar" (About.xml never
    // carries the _steam suffix) — the edge must still point at the exact
    // active id "foo.bar_steam" so it evaluates against the load order
    // instead of falling out as Unevaluated.
    let dep_edge = report
        .edges
        .iter()
        .find(|e| e.edge.kind == EdgeKind::ModDependency)
        .expect("modDependencies produces an edge");
    assert_eq!(dep_edge.edge.before, ModId::new("foo.bar_steam"));
    assert_eq!(dep_edge.edge.after, ModId::new("dep.mod"));
    assert_ne!(dep_edge.status, EdgeStatus::Unevaluated);
    assert!(report.missing_dependencies.is_empty());
}

// -- `_steam` in `inactive_mods` -----------------------------------------

/// With only the local `dup.mod` copy active, the shadowed Workshop copy must
/// still appear in `inactive_mods` — under the `_steam`-suffixed id, since
/// the bare id already belongs to the active local copy.
#[test]
fn inactive_mods_suffixes_a_shadowed_workshop_copy_when_only_the_local_copy_is_active() {
    let output =
        infra::scan(&scan_config("ModsConfig_dup_local_only.xml")).expect("scan must succeed");

    let ids: Vec<ModId> = output.inactive_mods.iter().map(|m| m.id.clone()).collect();
    assert!(ids.contains(&ModId::new("dup.mod_steam")));
    assert!(!ids.contains(&ModId::new("dup.mod")));
}

/// With neither copy active (`ModsConfig_steam_only.xml` activates
/// `foo.bar_steam`/`dep.mod`, not `dup.mod`), both copies of the shadowed
/// pair appear in `inactive_mods` — `dup.mod` and `dup.mod_steam`.
#[test]
fn inactive_mods_lists_both_copies_of_a_shadowed_pair_when_neither_is_active() {
    let output = infra::scan(&scan_config("ModsConfig_steam_only.xml")).expect("scan must succeed");

    let ids: Vec<ModId> = output.inactive_mods.iter().map(|m| m.id.clone()).collect();
    assert!(ids.contains(&ModId::new("dup.mod")));
    assert!(ids.contains(&ModId::new("dup.mod_steam")));

    // The same fixture's other shadowed pair, `foo.bar`/`foo.bar_steam`, is
    // only *half* inactive here — the workshop copy is active
    // (`ModsConfig_steam_only.xml`'s own `foo.bar_steam` entry) while the
    // local copy is not. Asserting both sides in one test makes the
    // disjointness rule (`Discovered::inactive_excluding`) observable on this
    // fixture: a folder resolved into `to_scan` must never also appear
    // inactive.
    assert!(ids.contains(&ModId::new("foo.bar")));
    assert!(!ids.contains(&ModId::new("foo.bar_steam")));
}
