//! `rim_io::discover_inventory` end to end — the discovery-only inventory
//! path, wired to the real
//! `rim_analyzer::infra::inventory` against a scratch copy of
//! `rim-analyzer`'s checked-in fixture game tree.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_session::ProjectPaths;
use tempfile::tempdir;

fn sample_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rim-analyzer")
        .join("tests")
        .join("fixtures")
        .join("sample_game")
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
    let game_dir = root.join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir)?;
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590")?;

    Ok(ProjectPaths {
        workshop_dir: root.join("workshop_does_not_exist"),
        mods_config: game_dir.join("ModsConfig.xml"),
        profile_dir: root.join("profile"),
        game_dir,
    })
}

/// The fixture's `ModsConfig.xml` activates only `sample.mod`;
/// `aaa.mod`/`zzz.mod` are on disk but inactive. `discover_inventory`
/// must find all three with no full scan — every discovered mod (active
/// or not) resolves in the returned `ModInventory`, and `active` mirrors
/// the file exactly.
#[test]
fn discover_inventory_finds_every_mod_and_the_files_active_list() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");

    let (inventory, active) =
        rim_io::discover_inventory(&paths, None).expect("discovery-only inventory must succeed");

    assert_eq!(active, vec![ModId::new("sample.mod")]);
    for id in ["sample.mod", "aaa.mod", "zzz.mod"] {
        let entry = inventory
            .entry(&ModId::new(id))
            .unwrap_or_else(|| panic!("{id} must resolve in the inventory"));
        assert!(entry.present_on_disk, "{id} is on disk");
    }
    assert!(
        inventory.entry(&ModId::new("does.not.exist")).is_none(),
        "an id nothing on disk or in the active list names must not resolve"
    );
}

/// `active_override` is scanned verbatim instead of re-reading the file's
/// own `<activeMods>` — the "read the file once" path every `mods`
/// subcommand takes (`apps/cli/src/commands/mods.rs`'s own `discover`):
/// the caller already read `ModsConfig.xml` through `ModsConfigStore::read`
/// for `version`/`knownExpansions` and hands the exact same `active_mods`
/// back in here, rather than this function reading the file a second time.
#[test]
fn discover_inventory_honours_the_active_override_instead_of_re_reading_the_file() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");

    let (inventory, active) = rim_io::discover_inventory(
        &paths,
        Some(&[ModId::new("zzz.mod"), ModId::new("aaa.mod")]),
    )
    .expect("discovery-only inventory must succeed");

    assert_eq!(active, vec![ModId::new("zzz.mod"), ModId::new("aaa.mod")]);
    // The file's own active list (just "sample.mod") is not consulted —
    // aaa.mod/zzz.mod resolve as active-shaped entries regardless.
    for id in ["aaa.mod", "zzz.mod"] {
        let entry = inventory
            .entry(&ModId::new(id))
            .unwrap_or_else(|| panic!("{id} must resolve in the inventory"));
        assert!(entry.present_on_disk, "{id} is on disk");
    }
}
