//! A scratch copy of `rim-analyzer`'s checked-in fixture game tree, shared by the scanner's unit
//! tests (`src/scanner.rs`) and `tests/scan_progress.rs` through `#[path]`.

use std::fs;
use std::path::{Path, PathBuf};

use rim_session::ProjectPaths;

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

pub fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
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
