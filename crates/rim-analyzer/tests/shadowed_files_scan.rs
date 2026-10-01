//! End-to-end test of the same-relative-path folder-shadowing rule (see
//! `crates/rim-analyzer/CLAUDE.md`'s "Engine facts" section, "Same
//! relative path across loaded folders") over `tests/fixtures/
//! shadowing_game/` — four tiny fixture mods, one per rule variant, all
//! scanned together so a regression in one mod's dedup can't hide behind
//! another mod's passing assertions.

use std::path::PathBuf;

use rim_analyzer::domain::{FolderPolicy, GameVersion, ModId};
use rim_analyzer::infra;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shadowing_game")
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

fn def_names(scanned: &rim_analyzer::domain::ScannedMod) -> Vec<&str> {
    scanned.defs.iter().map(|d| d.def_name.as_str()).collect()
}

/// Same relative path (`Defs/Shadow.xml`) shipped in both a version folder
/// (`1.6/`) and the mod root, no explicit `LoadFolders.xml` (so the
/// default folder rule applies): only the version folder's copy loads —
/// the root copy's `RootDef` must never appear, and `Defs/Unique.xml`
/// (a *different* relative path, root-only) must still be scanned, proving
/// the dedup is keyed by relative path and doesn't just drop "everything
/// from the root". The `Patches/P.xml` pair pins the same rule for
/// `Patches/`, targeting each candidate's own def so a stray survivor is
/// directly attributable. This is the regression case: before the fix,
/// both `Defs/Shadow.xml` copies (and both `Patches/P.xml` copies) were
/// scanned, producing a phantom `RootDef`/duplicate def and a patch op
/// against a target the real engine never loads.
#[test]
fn version_folder_shadows_the_same_relative_path_at_the_root() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = output
        .scanned_mods
        .iter()
        .find(|m| m.info.id == ModId::new("shadow.versionroot"))
        .expect("shadow.versionroot must be scanned");

    let names = def_names(scanned);
    assert!(
        names.contains(&"VersionDef"),
        "the version folder's copy must be scanned: {names:?}"
    );
    assert!(
        !names.contains(&"RootDef"),
        "the root's shadowed copy must never be scanned: {names:?}"
    );
    assert!(
        names.contains(&"RootOnlyDef"),
        "a root-only file at a different relative path is unaffected: {names:?}"
    );

    let sub_paths: Vec<Option<&str>> = scanned
        .patch_ops
        .iter()
        .map(|op| op.target.as_ref().and_then(|t| t.sub_path.as_deref()))
        .collect();
    assert_eq!(
        sub_paths,
        vec![Some("statBases")],
        "only the version folder's Patches/P.xml (targeting VersionDef) survives; \
         the root's shadowed Patches/P.xml (targeting RootDef) must not be scanned"
    );
}

/// Same relative path in `Common/` and the mod root (no version folder on
/// disk at all): `Common/` wins, matching the default rule's own
/// construction order (version folder, then `Common`, then root).
#[test]
fn common_folder_shadows_the_same_relative_path_at_the_root() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = output
        .scanned_mods
        .iter()
        .find(|m| m.info.id == ModId::new("shadow.commonroot"))
        .expect("shadow.commonroot must be scanned");

    let names = def_names(scanned);
    assert_eq!(
        names,
        vec!["CommonDef"],
        "Common/ wins over the root for the same relative path: {names:?}"
    );
}

/// An explicit `LoadFolders.xml` listing the root first and the version
/// folder second (`<li>/</li><li>1.6</li>`, the common two-entry shape)
/// must resolve to the **last-listed** entry winning — `1.6`, not the document-order-first
/// root — per `Verse.ModContentPack.InitLoadFolders`'s own reversal (see
/// `mod_scan.rs::resolve_loaded_folders`'s doc comment).
#[test]
fn explicit_load_folders_xml_lets_the_last_listed_entry_win() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = output
        .scanned_mods
        .iter()
        .find(|m| m.info.id == ModId::new("shadow.loadfoldersorder"))
        .expect("shadow.loadfoldersorder must be scanned");

    let names = def_names(scanned);
    assert_eq!(
        names,
        vec!["VersionWinsDef"],
        "the last-listed <li> (1.6) must win, not the first-listed root: {names:?}"
    );
}

/// `Defs/Shadow.xml` (version folder) and `Defs/shadow.xml` (root,
/// lowercase file name) differ only by case: RimWorld's own
/// `Dictionary<string, FileInfo>` key comparer is ordinal (case-sensitive,
/// no folding — see `mod_scan.rs::shadowed_paths`'s doc comment), so these
/// are two distinct keys and **both** must be scanned, not deduped.
#[test]
fn a_relative_path_differing_only_by_case_is_not_treated_as_the_same_key() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let scanned = output
        .scanned_mods
        .iter()
        .find(|m| m.info.id == ModId::new("shadow.casesensitive"))
        .expect("shadow.casesensitive must be scanned");

    let mut names = def_names(scanned);
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["RootCaseDef", "VersionCaseDef"],
        "case-differing relative paths are distinct keys, so neither shadows the other: {names:?}"
    );
}
