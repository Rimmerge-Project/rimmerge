//! Regenerates the compat-patch golden export fixture
//!
//! `tests/fixtures/patch_export/` — the expected output of exporting a
//! compat patch over the `merge_game` fixture's `Fixture_Wall` (`ModA`/
//! `ModB`, scope `{fixture.moda, fixture.modb}`, one field choice).
//!
//! Run from the workspace root after a deliberate change to `emit`'s
//! patch-mode output:
//!
//! ```text
//! cargo run -p rim-merge --example gen_patch_export_fixture
//! ```
//!
//! `tests/patch_export_golden.rs` renders the identical scenario and
//! diffs it against these files byte-for-byte — review `git diff` on this
//! fixture the same way an `insta` `.snap.new` gets reviewed, never
//! regenerate and commit without reading the diff.

#[path = "../tests/support/patch_export_scenario.rs"]
mod patch_export_scenario;

use std::fs;
use std::path::{Path, PathBuf};

use rim_merge::emit::{FileContent, render};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_export")
}

fn main() {
    let scenario = patch_export_scenario::build();
    let rendered = render(&scenario.input()).unwrap_or_else(|error| panic!("{error:?}"));

    let dir = fixture_dir();
    for file in &rendered.files {
        let FileContent::Text(text) = &file.content else {
            panic!(
                "{}: the fixture scenario has no binary assets to write",
                file.relative_path.display()
            );
        };
        let path = dir.join(&file.relative_path);
        let Some(parent) = path.parent() else {
            panic!("{}: every rendered file has a parent", path.display());
        };
        fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{}: {error}", parent.display()));
        fs::write(&path, text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        println!("wrote {}", path.display());
    }
}
