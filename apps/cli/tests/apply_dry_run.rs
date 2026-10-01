//! `rimmerge apply --dry-run` end to end, against a scratch copy of
//! `rim-analyzer`'s checked-in fixture game tree — never the real
//! `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

mod common;
use common::copy_dir_recursive;

fn sample_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-analyzer")
        .join("tests")
        .join("fixtures")
        .join("sample_game")
}

/// A CRLF-terminated `ModsConfig.xml` naming the fixture's one mod
/// (`sample.mod`) — RimWorld's own line-ending convention (see
/// `rim-io`'s `mods_config` tests for the real file this mirrors).
const CRLF_MODS_CONFIG: &str = "<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.0</version>\r\n  <activeMods>\r\n    <li>sample.mod</li>\r\n  </activeMods>\r\n  <knownExpansions>\r\n  </knownExpansions>\r\n</ModsConfigData>\r\n";

#[test]
fn apply_dry_run_reports_and_writes_nothing_against_a_scratch_game_tree() {
    let scratch = tempdir().expect("tempdir");
    let game_dir = scratch.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");

    // `AnalyzerScanner` resolves the game version from `Version.txt`,
    // which the fixture (shared with rim-analyzer's own tests, which
    // pass the version explicitly instead) doesn't ship.
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");

    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, CRLF_MODS_CONFIG).expect("write CRLF ModsConfig.xml");
    let original_bytes = fs::read(&mods_config).expect("read seeded ModsConfig.xml");

    let workshop_dir = scratch.path().join("workshop_does_not_exist");
    let profile_dir = scratch.path().join("profile");

    let mut cmd = common::rimmerge();
    cmd.arg("apply")
        .arg("--dry-run")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(&workshop_dir)
        .arg("--mods-config")
        .arg(&mods_config)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .assert()
        .success()
        .stdout(predicates::str::contains("Dry run:"))
        .stdout(predicates::str::contains(
            "nothing written — this is a dry run",
        ));

    assert_eq!(
        fs::read(&mods_config).expect("re-read ModsConfig.xml"),
        original_bytes,
        "a dry run must never modify ModsConfig.xml"
    );
    let backups: Vec<_> = fs::read_dir(&game_dir)
        .expect("read game dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
        .collect();
    assert!(
        backups.is_empty(),
        "a dry run must never create a backup file"
    );
}

/// The dry-run diff states
/// which settings produced the order, so a large disturbance (e.g. the
/// first `Rebuild` of a list RimSort produced) is explained rather than
/// alarming. `use_imported_pairs=true` is the default — see
/// `Settings::default`'s own doc comment.
#[test]
fn apply_dry_run_states_the_sort_provenance() {
    let scratch = tempdir().expect("tempdir");
    let game_dir = scratch.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");

    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, CRLF_MODS_CONFIG).expect("write CRLF ModsConfig.xml");

    let mut cmd = common::rimmerge();
    cmd.arg("apply")
        .arg("--dry-run")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(scratch.path().join("workshop_does_not_exist"))
        .arg("--mods-config")
        .arg(&mods_config)
        .arg("--profile-dir")
        .arg(scratch.path().join("profile"))
        .assert()
        .success()
        .stdout(predicates::str::contains("provenance: tie_break=Rebuild"))
        .stdout(predicates::str::contains("use_imported_pairs=true"))
        .stdout(predicates::str::contains("use_imported_placements=true"));
}

/// Seeds `<profile_dir>/rules.json` with non-default `tie_break`/
/// `use_imported_pairs` settings (mirrors `promote_cli.rs`'s own
/// `seed_imported_pair_rule` seeder) — proves the dry-run provenance line
/// is genuinely read off `Settings`, not a hardcoded string that would
/// happen to match the defaults asserted by
/// `apply_dry_run_states_the_sort_provenance` above. `use_imported_pairs:
/// false` is the non-default side.
fn seed_rules_with_non_default_sort_settings(profile_dir: &Path) {
    fs::create_dir_all(profile_dir).unwrap_or_else(|error| panic!("create profile dir: {error}"));
    fs::write(
        profile_dir.join("rules.json"),
        r#"{
            "version": 2,
            "pairs": [],
            "placements": [],
            "incompatibles": [],
            "tag_rules": [],
            "manual_tags": [],
            "settings": {
                "threshold": 80,
                "enforce_soft": false,
                "enforce_awareness": false,
                "suggest_merge_when_clean": true,
                "tie_break": "preserve_current",
                "use_imported_pairs": false,
                "use_imported_placements": true
            }
        }"#,
    )
    .unwrap_or_else(|error| panic!("seed rules.json: {error}"));
}

/// The dry-run provenance line must reflect a seeded, non-default
/// `Settings` rather than always printing the defaults — see this file's
/// `seed_rules_with_non_default_sort_settings` doc comment.
#[test]
fn apply_dry_run_states_a_seeded_non_default_sort_provenance() {
    let scratch = tempdir().expect("tempdir");
    let game_dir = scratch.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");

    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, CRLF_MODS_CONFIG).expect("write CRLF ModsConfig.xml");

    let profile_dir = scratch.path().join("profile");
    seed_rules_with_non_default_sort_settings(&profile_dir);

    let mut cmd = common::rimmerge();
    cmd.arg("apply")
        .arg("--dry-run")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(scratch.path().join("workshop_does_not_exist"))
        .arg("--mods-config")
        .arg(&mods_config)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "provenance: tie_break=PreserveCurrent",
        ))
        .stdout(predicates::str::contains("use_imported_pairs=false"))
        .stdout(predicates::str::contains("use_imported_placements=true"));
}

/// `ModsConfig.xml` naming the fixture's mod plus one that is not on disk.
const MODS_CONFIG_WITH_A_MISSING_MOD: &str = "<?xml version=\"1.0\" ?>
<ModsConfigData>
  <version>1.6.0</version>
  <activeMods>
    <li>sample.mod</li>
    <li>absent.example.mod</li>
  </activeMods>
  <knownExpansions>
  </knownExpansions>
</ModsConfigData>
";

#[test]
fn apply_dry_run_prints_hard_problems_without_writing() {
    let scratch = tempdir().expect("tempdir");
    let game_dir = scratch.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");
    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, MODS_CONFIG_WITH_A_MISSING_MOD).expect("write ModsConfig.xml");
    let original_bytes = fs::read(&mods_config).expect("read seeded ModsConfig.xml");

    common::rimmerge()
        .arg("apply")
        .arg("--dry-run")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(scratch.path().join("workshop_does_not_exist"))
        .arg("--mods-config")
        .arg(&mods_config)
        .arg("--profile-dir")
        .arg(scratch.path().join("profile"))
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "hard problems in the Suggested order (1):",
        ))
        .stdout(predicates::str::contains(
            "  missing mod: absent.example.mod is not installed; removed from the active list",
        ));

    assert_eq!(
        fs::read(&mods_config).expect("re-read ModsConfig.xml"),
        original_bytes,
        "a dry run must never modify ModsConfig.xml"
    );
}

#[test]
fn apply_prints_hard_problems_and_still_exits_zero() {
    let scratch = tempdir().expect("tempdir");
    let game_dir = scratch.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");
    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, MODS_CONFIG_WITH_A_MISSING_MOD).expect("write ModsConfig.xml");

    // `--force`: the default gate must not depend on whether a real
    // RimWorld process happens to be running on this machine. The write
    // goes to the scratch copy only.
    common::rimmerge()
        .arg("apply")
        .arg("--force")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(scratch.path().join("workshop_does_not_exist"))
        .arg("--mods-config")
        .arg(&mods_config)
        .arg("--profile-dir")
        .arg(scratch.path().join("profile"))
        .assert()
        .success()
        .stdout(predicates::str::contains("missing mod: absent.example.mod"))
        .stdout(predicates::str::contains("wrote ModsConfig.xml: true"));

    let written = fs::read_to_string(&mods_config).expect("read the written ModsConfig.xml");
    assert!(
        !written.contains("absent.example.mod"),
        "the suggested order drops the missing mod: {written}"
    );
}
