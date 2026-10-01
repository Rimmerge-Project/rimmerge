//! Shared fixture helpers for this crate's own `#[cfg(test)]` unit-test
//! modules (`commands::apply`, `commands::patch`). Integration tests
//! under `tests/` are a separate compilation unit and can't reach into
//! `src/` at all — they get their own copy at `tests/common/mod.rs`.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

/// `crates/rim-io/tests/fixtures/merge_game` — the two-mod fixture with a
/// contested `ThingDef` override, shared by `commands::patch`'s own
/// export tests.
pub(crate) fn merge_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-io")
        .join("tests")
        .join("fixtures")
        .join("merge_game")
}

/// Recursively copies `src` into `dst`, creating directories as needed —
/// every test that needs a scratch, disposable game tree starts from a
/// copy of a checked-in fixture rather than touching it in place.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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
