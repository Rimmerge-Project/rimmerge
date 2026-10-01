//! End-to-end: `LoadProject` then `Apply`, both from `rim-session`, wired
//! to every real `rim-io` adapter (`AnalyzerScanner`, `ModsConfigFileStore`,
//! `JsonDecisionStore`, `JsonRuleStore`) against a scratch copy of
//! `rim-analyzer`'s checked-in fixture game tree — never the real game
//! install, never the real `ModsConfig.xml`, never the real profile
//! directory. Complements the narrower per-adapter unit tests elsewhere in
//! this crate by proving the whole stack (scan -> sort -> ledger -> write)
//! actually round-trips together, the way `apps/cli`'s and
//! `apps/desktop`'s own `apply` commands use it.

use std::fs;
use std::path::{Path, PathBuf};

use rim_io::{
    AnalyzerScanner, FileAssetLocator, FileDefSourceReader, JsonDecisionStore, JsonRuleStore,
    MergeModFolderWriter, ModsConfigFileStore,
};
use rim_resolve::domain::OrderSource;
use rim_session::ProjectPaths;
use rim_session::use_cases::{Apply, ApplyOptions, LoadProject};
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

/// Builds a scratch `ProjectPaths` from a fresh copy of the fixture game
/// tree under `root` — `game_dir`/`mods_config` live inside `root`, never
/// touching anything outside the temp directory the caller owns. Returns
/// a `Result` (rather than unwrapping itself) so the `expect()` that
/// turns a failure into a test failure lives in the `#[test]` function
/// itself, where `clippy::expect_used`'s test allowance actually applies.
fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
    let game_dir = root.join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir)?;
    // `AnalyzerScanner` resolves the game version from `Version.txt`,
    // which the shared fixture doesn't ship (its own crate's tests pass
    // the version explicitly instead).
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590")?;

    Ok(ProjectPaths {
        workshop_dir: root.join("workshop_does_not_exist"),
        mods_config: game_dir.join("ModsConfig.xml"),
        profile_dir: root.join("profile"),
        game_dir,
    })
}

#[test]
fn load_project_then_apply_writes_a_backed_up_mods_config_through_real_adapters() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");
    let original_mods_config =
        fs::read_to_string(&paths.mods_config).expect("read seeded ModsConfig.xml");

    let load_project = LoadProject::new(
        AnalyzerScanner::new(),
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        rim_session::ports::NoPatchStore,
        rim_session::ports::NoAssignmentStore,
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    );
    let mut session = load_project
        .execute(paths.clone(), &mut |_progress| {})
        .expect("loading the scratch project must succeed");
    session.select(OrderSource::Suggested);

    let apply = Apply::new(
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        MergeModFolderWriter::new(),
        FileDefSourceReader::new(),
        FileAssetLocator::new(),
    );
    let outcome = apply
        .execute(
            &mut session,
            ApplyOptions {
                source: OrderSource::Suggested,
                write_mods_config: true,
                write_merge_mod: false,
            },
        )
        .expect("applying the session must succeed");

    assert!(outcome.wrote_mods_config);
    let backup_path = outcome
        .backup_path
        .expect("writing ModsConfig.xml must produce a backup path");
    assert!(
        backup_path.exists(),
        "the backup file must actually exist on disk"
    );
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read backup file"),
        original_mods_config,
        "the backup must hold the pre-apply ModsConfig.xml contents verbatim"
    );

    let rewritten = fs::read_to_string(&paths.mods_config).expect("re-read ModsConfig.xml");
    assert!(
        rewritten.contains("sample.mod"),
        "the rewritten ModsConfig.xml must still list the fixture's one active mod"
    );
    assert_ne!(
        rewritten, original_mods_config,
        "apply must actually re-render the file (the fixture's seed has no <knownExpansions> \
         element at all; ModsConfigFileStore's own writer always emits one), not just leave the \
         original bytes in place"
    );

    // Read the directory rather than trusting `outcome.backup_path` alone
    // — proves the backup is a real, independently discoverable file,
    // the way a user recovering from a bad apply would go looking for it.
    let backup_in_dir = fs::read_dir(
        paths
            .mods_config
            .parent()
            .expect("mods_config has a parent"),
    )
    .expect("read game dir")
    .filter_map(Result::ok)
    .find(|entry| entry.file_name().to_string_lossy().contains(".bak-"));
    assert!(
        backup_in_dir.is_some(),
        "a ModsConfig.xml.bak-* file must be discoverable by reading the game directory"
    );

    assert!(
        paths.profile_dir.join("decisions.json").exists(),
        "decisions.json must have been saved alongside the ModsConfig.xml write"
    );
    assert!(
        paths.profile_dir.join("rules.json").exists(),
        "rules.json must have been saved alongside the ModsConfig.xml write"
    );
}

#[test]
fn a_second_apply_without_write_mods_config_only_touches_the_profile_files() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");
    let original_mods_config =
        fs::read_to_string(&paths.mods_config).expect("read seeded ModsConfig.xml");

    let load_project = LoadProject::new(
        AnalyzerScanner::new(),
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        rim_session::ports::NoPatchStore,
        rim_session::ports::NoAssignmentStore,
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    );
    let mut session = load_project
        .execute(paths.clone(), &mut |_progress| {})
        .expect("loading the scratch project must succeed");

    let apply = Apply::new(
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        MergeModFolderWriter::new(),
        FileDefSourceReader::new(),
        FileAssetLocator::new(),
    );
    let outcome = apply
        .execute(
            &mut session,
            ApplyOptions {
                source: OrderSource::Current,
                write_mods_config: false,
                write_merge_mod: false,
            },
        )
        .expect("applying (export-only) must succeed");

    assert!(!outcome.wrote_mods_config);
    assert!(outcome.backup_path.is_none());
    assert_eq!(
        fs::read_to_string(&paths.mods_config).expect("re-read ModsConfig.xml"),
        original_mods_config,
        "an export-only apply must never touch ModsConfig.xml"
    );
    assert!(paths.profile_dir.join("decisions.json").exists());
    assert!(paths.profile_dir.join("rules.json").exists());
}
