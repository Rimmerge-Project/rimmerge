//! End-to-end test of the scan-time `unloaded_defs_dirs_by_mod` wiring
//! (`scan_one_mod` -> `infra::scan`'s per-mod map -> the dangling-reference
//! explanation) over `tests/fixtures/unloaded_game/`: one mod loading
//! `1.6/` whose `1.5/Defs/` is present on disk but never loaded.

use std::path::PathBuf;

use rim_analyzer::domain::{
    Conflict, DanglingCause, DanglingDefReference, FolderPolicy, GameVersion, ModId,
};
use rim_analyzer::infra;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unloaded_game")
}

fn scan_fixture() -> rim_analyzer::domain::ScanOutput {
    let game_dir = fixture_dir();
    infra::scan(&infra::ScanConfig {
        mods_config_path: game_dir.join("ModsConfig.xml"),
        workshop_dir: game_dir.join("workshop_does_not_exist"),
        game_dir,
        game_version: GameVersion::new(1, 6),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    })
    .expect("fixture scan must succeed")
}

#[test]
fn scan_records_each_mods_unloaded_defs_folders() {
    let output = scan_fixture();

    let dirs = output
        .unloaded_defs_dirs_by_mod
        .get(&ModId::new("unloaded.oldversion"))
        .expect("every scanned mod has an entry");

    assert_eq!(
        dirs,
        &vec![
            fixture_dir()
                .join("Mods")
                .join("OldVersion")
                .join("1.5")
                .join("Defs")
        ]
    );
}

#[test]
fn a_name_defined_only_in_an_unloaded_folder_is_explained_after_a_real_scan() {
    let output = scan_fixture();
    let mut conflicts = vec![Conflict::DanglingDefReference(DanglingDefReference {
        name: "GhostDef".to_string(),
        referrers: Vec::new(),
        truncated_referrers: 0,
        cause: DanglingCause::Unexplained,
        likely_sound: false,
    })];

    infra::explain_dangling_references(&mut conflicts, &output);

    let Conflict::DanglingDefReference(explained) = &conflicts[0] else {
        panic!("the conflict kind never changes");
    };
    assert_eq!(
        explained.cause,
        DanglingCause::OnlyInUnloadedFolder {
            mod_id: ModId::new("unloaded.oldversion"),
            folder: "1.5".to_string(),
        }
    );
}
