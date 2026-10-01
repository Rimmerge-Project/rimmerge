//! End-to-end test of `infra::scan` (discovery + per-mod extraction) over
//! a tiny, checked-in fixture mod tree — `tests/fixtures/sample_game/`.
//! Exercises `LoadFolders.xml` `IfModActive` gating, a `Defs` file, two
//! `Patches` files (one with a nested `PatchOperationFindMod`) to prove
//! file-then-document patch-op order, and a texture, with no `.dll` (so
//! assembly extraction is exercised as "none found").

use std::collections::BTreeSet;
use std::path::PathBuf;

use rim_analyzer::domain::{
    FindModGate, FolderPolicy, GameVersion, GeneratedKind, ModId, ScanStage,
};
use rim_analyzer::{analysis, infra};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_game")
}

fn scan_config() -> infra::ScanConfig {
    let game_dir = fixture_dir();
    infra::ScanConfig {
        mods_config_path: game_dir.join("ModsConfig.xml"),
        workshop_dir: game_dir.join("workshop_does_not_exist"),
        game_dir,
        game_version: GameVersion::new(1, 6),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    }
}

#[test]
fn scans_the_fixture_mod_end_to_end() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");

    assert!(output.missing_mods.is_empty());
    assert_eq!(output.scanned_mods.len(), 1);
    let scanned = &output.scanned_mods[0];

    assert_eq!(scanned.info.id, ModId::new("sample.mod"));
    assert_eq!(scanned.info.name, "Sample Mod");
    assert!(scanned.assemblies.is_empty());

    // Defs/ThingDefs.xml.
    assert!(
        scanned
            .defs
            .iter()
            .any(|d| d.def_type == "ThingDef" && d.def_name == "SampleWall")
    );

    // Textures/Things/Wall.png, normalized.
    assert!(scanned.textures.contains_key("things/wall"));

    // LoadFolders.xml: the plain "1.6" entry is loaded, the
    // IfModActive="some.other.mod"-gated "1.6/Compat" entry is not (that
    // mod isn't active).
    assert_eq!(scanned.info.loaded_folders.len(), 1);
    assert!(
        scanned.info.loaded_folders[0]
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with("SampleMod/1.6")
    );

    // Patches/AAA_Earlier.xml (2 mutating leaves) + Patches/Patch.xml (two
    // PatchOperationFindMod control-flow nodes, non-mutating, plus one
    // mutating leaf with a two-level nested find_mod_context) = 5 ops.
    assert_eq!(scanned.patch_ops.len(), 5);
    let leaf = scanned
        .patch_ops
        .iter()
        .find(|op| op.is_mutating && !op.find_mod_context.is_empty())
        .expect("exactly one mutating op sits under nested FindMod gates in the fixture");
    assert_eq!(
        leaf.find_mod_context,
        vec![
            FindModGate::AnyActive(vec!["Sample Mod".to_string()]),
            FindModGate::AnyActive(vec!["Nonexistent Mod".to_string()]),
        ]
    );
    assert_eq!(
        leaf.target.as_ref().map(|t| t.def_name.as_str()),
        Some("SampleWall")
    );
}

/// `SampleMod` carries a `rimmerge.json` marker at its root, so a scan
/// over the fixture must set [`rim_analyzer::domain::Mod::generated`] from
/// it — the end-to-end wiring `infra::discovery` -> `infra::mod_scan`
/// carries the marker through, on top of the pure-parser unit tests in
/// `extract::rimmerge_marker` and the `infra::discovery` unit tests.
#[test]
fn scanned_mod_carries_its_generated_marker() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = &output.scanned_mods[0];

    let marker = scanned
        .info
        .generated
        .as_ref()
        .expect("SampleMod's rimmerge.json marker must be parsed");
    assert_eq!(marker.kind, GeneratedKind::Patch);
    assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
    assert_eq!(
        marker.scope,
        Some(BTreeSet::from([
            ModId::new("fixture.moda"),
            ModId::new("fixture.modb"),
        ]))
    );
}

/// `patch_ops` order is file order (per `engine_enumeration_order`'s
/// breadth-first, NTFS-collation walk — both file names here are plain
/// root-level ASCII, so that order coincides with a plain alphabetical
/// sort for this fixture) then document order within each file — never
/// scan-declared or alphabetical-by-content order. `AAA_Earlier.xml`
/// sorts before `Patch.xml`, so its two ops (in their own document order)
/// must come first, ahead of every op `Patch.xml` contributes.
#[test]
fn patch_ops_are_ordered_file_then_document() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = &output.scanned_mods[0];

    let sub_paths: Vec<Option<&str>> = scanned
        .patch_ops
        .iter()
        .map(|op| op.target.as_ref().and_then(|t| t.sub_path.as_deref()))
        .collect();

    assert_eq!(
        sub_paths,
        vec![
            // AAA_Earlier.xml, document order.
            Some("statBases"),
            Some("tradeTags"),
            // Patch.xml: the outer/inner FindMod wrappers have no
            // `<xpath>` of their own (`None`), then the nested leaf.
            None,
            None,
            Some("comps"),
        ]
    );
}

#[test]
fn full_report_builds_without_panicking_over_the_fixture() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let context = analysis::RunContext {
        game_dir: fixture_dir(),
        workshop_dir: fixture_dir().join("workshop_does_not_exist"),
        mods_config_path: fixture_dir().join("ModsConfig.xml"),
        game_version: GameVersion::new(1, 6),
    };

    let report = analysis::build(output, &context);

    assert_eq!(report.mods.len(), 1);
    assert_eq!(report.metadata.active_mod_count, 1);
    // The inner PatchOperationFindMod names a mod ("Nonexistent Mod")
    // that resolves to no active mod's display name.
    assert!(
        report
            .unresolved_find_mod_names
            .iter()
            .any(|u| u.display_name == "Nonexistent Mod")
    );
}

#[test]
fn scan_with_progress_reports_every_stage_and_matches_scan() {
    let mut stages = Vec::new();
    let output = infra::scan_with_progress(&scan_config(), &mut |p| stages.push(p))
        .expect("fixture scan must succeed");

    // One mod in the fixture: Scanning must report exactly one tick,
    // ending at done == total, bracketed by a start/end pair for the two
    // non-parallel stages.
    assert_eq!(
        stages,
        vec![
            infra::ScanProgress {
                stage: ScanStage::Discovering,
                done: 0,
                total: 1
            },
            infra::ScanProgress {
                stage: ScanStage::Discovering,
                done: 1,
                total: 1
            },
            infra::ScanProgress {
                stage: ScanStage::Scanning,
                done: 1,
                total: 1
            },
            infra::ScanProgress {
                stage: ScanStage::Analyzing,
                done: 0,
                total: 1
            },
            infra::ScanProgress {
                stage: ScanStage::Analyzing,
                done: 1,
                total: 1
            },
        ]
    );
    assert_eq!(output.scanned_mods.len(), 1);
}

#[test]
fn scan_delegates_to_scan_with_progress_with_a_no_op_hook() {
    let mut calls = 0usize;
    let via_progress = infra::scan_with_progress(&scan_config(), &mut |_| calls += 1)
        .expect("fixture scan must succeed");
    let via_scan = infra::scan(&scan_config()).expect("fixture scan must succeed");

    assert!(calls > 0, "scan_with_progress must call the hook at all");
    assert_eq!(via_progress.scanned_mods.len(), via_scan.scanned_mods.len());
    assert_eq!(via_progress.missing_mods, via_scan.missing_mods);
}

#[test]
fn scan_fails_fast_when_game_dir_is_missing() {
    let mut config = scan_config();
    config.game_dir = fixture_dir().join("does_not_exist");

    // `ScanOutput` isn't `Debug` (it isn't part of the serialized report),
    // so match the error out by hand rather than `expect_err`.
    let Err(err) = infra::scan(&config) else {
        panic!("scan with a missing game_dir must fail");
    };

    assert!(err.to_string().contains("game_dir"));
}

// -- ScanCost / ModCost ------------------------------------------------

/// `SampleMod` ships exactly one texture file (`Textures/Things/Wall.png`,
/// 8 bytes), no `.dds`, and no `Assemblies/` folder at all — `ScanCost`
/// must report the raw file/byte counts the walk already reads for free.
#[test]
fn fixture_mod_scan_cost_counts_its_one_shipped_texture() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = &output.scanned_mods[0];

    assert_eq!(scanned.scan_cost.texture_files, 1);
    assert_eq!(scanned.scan_cost.texture_bytes, 8);
    assert_eq!(scanned.scan_cost.dds_files, 0);
    assert_eq!(scanned.scan_cost.assembly_bytes, 0);
}

/// `ScannedMod::textures` carries the same byte total as `ScanCost`
/// (single file, no other texture sharing its normalized key).
#[test]
fn fixture_mod_textures_map_carries_the_shipped_files_byte_size() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = &output.scanned_mods[0];

    assert_eq!(scanned.textures.get("things/wall"), Some(&8));
}

fn run_context() -> analysis::RunContext {
    let game_dir = fixture_dir();
    analysis::RunContext {
        workshop_dir: game_dir.join("workshop_does_not_exist"),
        mods_config_path: game_dir.join("ModsConfig.xml"),
        game_dir,
        game_version: GameVersion::new(1, 6),
    }
}

/// `ModsConfig_three_mods.xml` — a second `ModsConfig.xml` variant
/// checked into the *same* `sample_game` fixture directory (the
/// `steam_suffix_game`/`ModsConfig_dup_active.xml` convention
/// `tests/steam_suffix_scan.rs` already uses), naming `zzz.mod`,
/// `aaa.mod`, `sample.mod` — deliberately not alphabetical. `ZzzMod`/
/// `AaaMod` are two more checked-in mod folders alongside `SampleMod`
/// (`About.xml` only, no `Defs`/`Textures` — this variant exists purely
/// to pin ordering, not to add scan-content coverage `SampleMod` already
/// provides). Using a second `ModsConfig*.xml` rather than editing the
/// shared default one keeps every other test in this file — and every
/// other crate's own copy of `sample_game`
/// (`rim-io::apply_end_to_end`, `apps/cli::apply_dry_run`, the desktop
/// e2e smoke fixture) — on the original single-mod scan they already
/// assert against untouched.
fn scan_config_three_mods() -> infra::ScanConfig {
    let mut config = scan_config();
    config.mods_config_path = fixture_dir().join("ModsConfig_three_mods.xml");
    config
}

fn run_context_three_mods() -> analysis::RunContext {
    let mut context = run_context();
    context.mods_config_path = fixture_dir().join("ModsConfig_three_mods.xml");
    context
}

/// Scanning only one active mod would let `report_one.mod_costs ==
/// report_two.mod_costs` pass even if `mod_cost::compute` collected through
/// an unordered `HashMap` or otherwise lost the load order. Scanning three
/// mods whose `ModsConfig.xml` order is deliberately not alphabetical and
/// asserting the emitted `mod_id` sequence matches that exact order defends
/// the load-order claim directly; comparing that same three-row `Vec` across
/// two independent scans defends determinism without being satisfiable by a
/// coincidentally-matching one-element `Vec`.
#[test]
fn scanning_three_mods_twice_produces_identical_mod_costs_in_mods_config_order() {
    let context = run_context_three_mods();
    let report_one = analysis::build(infra::scan(&scan_config_three_mods()).unwrap(), &context);
    let report_two = analysis::build(infra::scan(&scan_config_three_mods()).unwrap(), &context);

    let expected_order = vec![
        ModId::new("zzz.mod"),
        ModId::new("aaa.mod"),
        ModId::new("sample.mod"),
    ];
    assert_eq!(
        report_one
            .mod_costs
            .iter()
            .map(|c| c.mod_id.clone())
            .collect::<Vec<_>>(),
        expected_order,
        "mod_costs must follow ModsConfig.xml's own order, not alphabetical"
    );
    assert_eq!(report_one.mod_costs, report_two.mod_costs);
}

/// End-to-end sanity: the one scanned mod's `ModCost` row matches its own
/// `ScanCost` and patch/def counts.
#[test]
fn fixture_mod_cost_row_matches_the_scanned_mods_own_counts() {
    let context = run_context();
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = output.scanned_mods[0].clone();
    let report = analysis::build(output, &context);

    assert_eq!(report.mod_costs.len(), 1);
    let cost = &report.mod_costs[0];
    assert_eq!(cost.mod_id, ModId::new("sample.mod"));
    assert_eq!(cost.texture_files, scanned.scan_cost.texture_files);
    assert_eq!(cost.texture_bytes, scanned.scan_cost.texture_bytes);
    assert_eq!(cost.dds_files, scanned.scan_cost.dds_files);
    assert_eq!(cost.assembly_bytes, scanned.scan_cost.assembly_bytes);
    assert_eq!(cost.assembly_count, scanned.assemblies.len());
    assert_eq!(cost.def_count, scanned.defs.len());
    assert_eq!(
        cost.patch_ops,
        scanned.patch_ops.iter().filter(|op| op.is_mutating).count()
    );
}

// -- `ScanOutput.inactive_mods`/`discovered_mod_count` ---------------------

/// The default single-mod `ModsConfig.xml` activates only `sample.mod`;
/// `AaaMod`/`ZzzMod` are on disk (the three-mod fixture above) but not
/// active, so they must show up as inactive, sorted by id.
#[test]
fn inactive_mods_lists_the_discovered_but_inactive_fixture_mods() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");

    assert_eq!(output.discovered_mod_count, 3);
    let ids: Vec<ModId> = output.inactive_mods.iter().map(|m| m.id.clone()).collect();
    assert_eq!(ids, vec![ModId::new("aaa.mod"), ModId::new("zzz.mod")]);
}

/// With every fixture mod active (`ModsConfig_three_mods.xml`),
/// `inactive_mods` is empty — `discovered_mod_count` is unchanged (it
/// counts every directory found, active or not).
#[test]
fn inactive_mods_is_empty_when_every_discovered_mod_is_active() {
    let output = infra::scan(&scan_config_three_mods()).expect("fixture scan must succeed");

    assert_eq!(output.discovered_mod_count, 3);
    assert!(output.inactive_mods.is_empty());
}

// -- `ScanConfig.active_mods` override ---------------------------------

/// `active_mods: Some(..)` is scanned verbatim, in the order given,
/// instead of the file's own `<activeMods>` — even though the fixture's
/// default `ModsConfig.xml` only activates `sample.mod`.
#[test]
fn active_mods_override_is_scanned_verbatim_instead_of_the_file() {
    let mut config = scan_config();
    config.active_mods = Some(vec![ModId::new("zzz.mod"), ModId::new("aaa.mod")]);

    let output = infra::scan(&config).expect("fixture scan must succeed");

    let ids: Vec<ModId> = output
        .scanned_mods
        .iter()
        .map(|sm| sm.info.id.clone())
        .collect();
    assert_eq!(ids, vec![ModId::new("zzz.mod"), ModId::new("aaa.mod")]);
    assert!(output.missing_mods.is_empty());
    let inactive_ids: Vec<ModId> = output.inactive_mods.iter().map(|m| m.id.clone()).collect();
    assert_eq!(inactive_ids, vec![ModId::new("sample.mod")]);
}

/// An override naming an id with no directory on disk lands in
/// `missing_mods`, exactly like a file-sourced active id would.
#[test]
fn active_mods_override_naming_an_unknown_id_becomes_missing() {
    let mut config = scan_config();
    config.active_mods = Some(vec![ModId::new("does.not.exist")]);

    let output = infra::scan(&config).expect("fixture scan must succeed");

    assert!(output.scanned_mods.is_empty());
    assert_eq!(output.missing_mods, vec![ModId::new("does.not.exist")]);
}

// -- `infra::inventory` (discovery-only) -------------------------------

/// `inventory` reads the file's own active list, discovers every mod on
/// disk (active or not), and never scans defs/patches/assemblies — the
/// default single-mod `ModsConfig.xml` activates only `sample.mod`, so
/// `AaaMod`/`ZzzMod` are `discovered` but not `active`.
#[test]
fn inventory_discovers_every_mod_without_scanning_defs() {
    let output = infra::inventory(&scan_config()).expect("fixture inventory must succeed");

    assert_eq!(output.active, vec![ModId::new("sample.mod")]);
    assert!(output.missing.is_empty());
    let mut discovered_ids: Vec<ModId> = output.discovered.iter().map(|m| m.id.clone()).collect();
    discovered_ids.sort();
    assert_eq!(
        discovered_ids,
        vec![
            ModId::new("aaa.mod"),
            ModId::new("sample.mod"),
            ModId::new("zzz.mod"),
        ],
        "discovered lists every mod on disk, active or not"
    );
}

/// `active_mods: Some(..)` overrides the file the same way it does for a
/// full [`infra::scan`], and an override naming an id with no directory
/// on disk lands in `missing`.
#[test]
fn inventory_honours_the_active_mods_override_and_reports_missing() {
    let mut config = scan_config();
    config.active_mods = Some(vec![ModId::new("zzz.mod"), ModId::new("does.not.exist")]);

    let output = infra::inventory(&config).expect("fixture inventory must succeed");

    assert_eq!(
        output.active,
        vec![ModId::new("zzz.mod"), ModId::new("does.not.exist")]
    );
    assert_eq!(output.missing, vec![ModId::new("does.not.exist")]);
}

/// A scratch dir under the OS temp root, cleared on entry — the same
/// manual (no `tempfile` dependency in this crate's own `tests/`)
/// scratch-tree pattern `src/infra/discovery.rs`'s own `#[cfg(test)]
/// mod tests` uses. Not a `#[test]` function itself, so
/// `clippy::unwrap_used`'s test allowance doesn't apply here
/// (`apps/cli/tests/common/mod.rs`'s `copy_dir_recursive` names the same
/// rule) — every fallible step panics via `unwrap_or_else` instead.
fn inventory_scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rim-analyzer-fixture-scan-inventory-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create {dir:?}: {error}"));
    dir
}

fn write_about_xml(mod_dir: &std::path::Path, package_id: &str) {
    let about_dir = mod_dir.join("About");
    std::fs::create_dir_all(&about_dir)
        .unwrap_or_else(|error| panic!("create {about_dir:?}: {error}"));
    std::fs::write(
        about_dir.join("About.xml"),
        format!(
            "<ModMetaData><packageId>{package_id}</packageId><name>{package_id}</name></ModMetaData>"
        ),
    )
    .unwrap_or_else(|error| panic!("write About.xml for {package_id}: {error}"));
}

fn write_mods_config_xml(game_dir: &std::path::Path, active: &[&str]) {
    let lis: String = active.iter().map(|id| format!("<li>{id}</li>")).collect();
    std::fs::write(
        game_dir.join("ModsConfig.xml"),
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><ModsConfigData><version>1.6.0</version><activeMods>{lis}</activeMods></ModsConfigData>"
        ),
    )
    .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));
}

/// A Workshop-only mod (no local/`Mods` copy at all) active under its
/// `_steam`-suffixed id must be `discovered` exactly once, keyed by that
/// exact active id — not under the bare id `Discovered::all()`'s own
/// `active_mods_id` re-derivation would (wrongly) assign it, since that
/// re-derivation only adds the `_steam` suffix when a *primary* copy also
/// shadows the folder. Keyed `x.mod` instead, the same physical folder would
/// read as simultaneously active (under `x.mod_steam`) and, via `mods list
/// --all`, inactive and missing too, and `mods activate`/`deactivate
/// x.mod_steam` would fail with "not a known mod" because
/// `ModInventory::contains` would never see that exact id.
#[test]
fn inventory_keys_an_active_steam_suffixed_workshop_only_mod_under_its_active_id() {
    let root = inventory_scratch_dir("workshop-only-steam-active");
    let workshop = root.join("workshop");
    write_about_xml(&workshop.join("333333"), "x.mod");
    write_mods_config_xml(&root, &["x.mod_steam"]);

    let config = infra::ScanConfig {
        game_dir: root.clone(),
        workshop_dir: workshop,
        mods_config_path: root.join("ModsConfig.xml"),
        game_version: GameVersion::new(1, 6),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    };

    let output = infra::inventory(&config).expect("inventory must succeed");

    assert!(
        output.missing.is_empty(),
        "x.mod_steam resolves to a real folder, so nothing is missing"
    );
    let ids: Vec<ModId> = output.discovered.iter().map(|m| m.id.clone()).collect();
    assert_eq!(
        ids,
        vec![ModId::new("x.mod_steam")],
        "exactly one discovered entry, keyed by the active id — never also \
         listed under the bare id"
    );

    let _ = std::fs::remove_dir_all(&root);
}
