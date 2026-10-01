//! Golden fixture for `emit`: a compat patch export's files must match
//! `tests/fixtures/patch_export/` byte-for-byte, and rendering the same
//! scenario twice must produce fully identical output — a `CompatPatch`
//! render carries no timestamp, so unlike the profile merge mod's
//! `rimmerge.json` nothing needs excluding from that comparison.
//!
//! Regenerate the fixture (after a deliberate change to `emit`'s
//! patch-mode output) with:
//!
//! ```text
//! cargo run -p rim-merge --example gen_patch_export_fixture
//! ```
//!
//! and review the diff — `git diff` on this fixture is reviewed exactly
//! the way an `insta` `.snap.new` is, never regenerated and committed
//! blind.

#[path = "support/patch_export_scenario.rs"]
mod patch_export_scenario;

use std::fs;
use std::path::{Path, PathBuf};

use rim_merge::emit::{FileContent, render};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_export")
}

#[test]
fn compat_patch_export_matches_the_golden_fixture_byte_for_byte() {
    let scenario = patch_export_scenario::build();

    let first = render(&scenario.input()).expect("the fixture scenario renders");
    let second = render(&scenario.input()).expect("the fixture scenario renders");
    assert_eq!(
        first, second,
        "a CompatPatch render carries no timestamp: two renders must be fully identical"
    );

    let dir = fixture_dir();
    let mut seen_relative_paths = Vec::new();
    for file in &first.files {
        seen_relative_paths.push(file.relative_path.clone());
        let FileContent::Text(text) = &file.content else {
            panic!(
                "{}: the fixture scenario has no binary assets",
                file.relative_path.display()
            );
        };
        let golden_path = dir.join(&file.relative_path);
        let expected = fs::read_to_string(&golden_path).unwrap_or_else(|error| {
            panic!("{}: {error} (regenerate with `cargo run -p rim-merge --example gen_patch_export_fixture`)",
                golden_path.display()
            )
        });
        assert_eq!(
            text,
            &expected,
            "{} drifted from the golden fixture",
            file.relative_path.display()
        );
    }

    assert_eq!(
        seen_relative_paths,
        vec![
            Path::new("About/About.xml"),
            Path::new("Patches/rimmerge_ThingDef.xml"),
            Path::new("rimmerge.json"),
        ],
        "the fixture scenario's own file list must stay in sync with the golden directory"
    );
}
