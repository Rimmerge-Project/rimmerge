//! End-to-end: an edit saved while a scan ran survives the swap and the next
//! save, through `LoadProject`, `Session::reload_persisted` and the real JSON
//! stores, against a scratch copy of `rim-analyzer`'s sample game tree.
//! Everything (game folder, `ModsConfig.xml`, profile) lives under a temp
//! directory; nothing outside it is read or written.
//!
//! The "scan" here is a session built by `LoadProject` before the late edit
//! is saved, and the "live" session is a second one the user edits through
//! the ordinary use cases, which is the order of events the desktop has
//! when an edit lands while a rescan is still running.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_io::{
    AnalyzerScanner, JsonAssignmentProjectStore, JsonDecisionStore, JsonPatchProjectStore,
    JsonRuleStore, ModsConfigFileStore,
};
use rim_resolve::domain::{
    Action, Decision, FindingKey, Placement, PlacementRule, Rule, RuleOrigin,
};
use rim_session::ports::{DecisionStore, PersistedKind, RuleStore};
use rim_session::use_cases::{Decide, LoadProject, LoadProjectError, UpsertRule};
use rim_session::{PersistedStores, ProjectPaths, Session};
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

type RealLoadProject = LoadProject<
    AnalyzerScanner,
    ModsConfigFileStore,
    JsonDecisionStore,
    JsonRuleStore,
    JsonPatchProjectStore,
    JsonAssignmentProjectStore,
    rim_session::test_support::FakeModKnowledgeStore,
>;

fn load_project() -> RealLoadProject {
    LoadProject::new(
        AnalyzerScanner::new(),
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        JsonPatchProjectStore::new(),
        JsonAssignmentProjectStore::new(),
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    )
}

/// Two sessions over one profile, loaded before any edit: the scan about to
/// be swapped in, and the live one the user edits.
struct ScanAndLive {
    scanned: Session,
    live: Session,
}

/// A `Result` (not unwrapped here) so the `expect` lives in the test
/// function, where `clippy::expect_used`'s test allowance applies.
fn scan_and_live(paths: &ProjectPaths) -> Result<ScanAndLive, LoadProjectError> {
    let use_case = load_project();
    Ok(ScanAndLive {
        scanned: use_case.execute(paths.clone(), &mut |_| {})?,
        live: use_case.execute(paths.clone(), &mut |_| {})?,
    })
}

fn reload(session: &mut Session) -> Vec<PersistedKind> {
    session.reload_persisted(PersistedStores {
        decisions: &JsonDecisionStore::new(),
        rules: &JsonRuleStore::new(),
        patches: &JsonPatchProjectStore::new(),
        assignments: &JsonAssignmentProjectStore::new(),
    })
}

fn top_placement(mod_id: &ModId) -> Rule {
    Rule::Placement(PlacementRule {
        mod_id: mod_id.clone(),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })
}

fn ignore_decision(mod_id: &ModId) -> Decision {
    Decision {
        key: FindingKey::UnsupportedVersion {
            mod_id: mod_id.clone(),
        },
        action: Action::Ignore,
        note: None,
        decided_at: jiff::Timestamp::UNIX_EPOCH,
    }
}

/// Two ids to name in an edit: the sample game's first mod, and one that is
/// not installed (a rule or decision only needs an id to name).
fn two_mod_ids(session: &Session) -> (ModId, ModId) {
    (
        session.report().mods[0].id.clone(),
        ModId::new("sample.other"),
    )
}

#[test]
fn a_rule_saved_during_the_scan_is_in_the_swapped_in_session_and_survives_its_next_save() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    let ScanAndLive {
        mut scanned,
        mut live,
    } = scan_and_live(&paths).expect("both loads must succeed");
    let (first, second) = two_mod_ids(&live);
    UpsertRule::new(JsonRuleStore::new())
        .execute(&mut live, top_placement(&first))
        .expect("the late edit saves");

    let changed = reload(&mut scanned);
    UpsertRule::new(JsonRuleStore::new())
        .execute(&mut scanned, top_placement(&second))
        .expect("the next save from the new session");

    assert_eq!(changed, [PersistedKind::Rules]);
    let on_disk = JsonRuleStore::new()
        .load(&paths.profile_dir)
        .expect("rules.json loads back")
        .rules;
    let kept: Vec<_> = on_disk.placements.iter().map(|p| &p.mod_id).collect();
    assert!(
        kept.contains(&&first) && kept.contains(&&second),
        "the late edit and the new one are both on disk: {kept:?}"
    );
}

#[test]
fn a_decision_saved_during_the_scan_is_in_the_swapped_in_session_and_survives_its_next_save() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    let ScanAndLive {
        mut scanned,
        mut live,
    } = scan_and_live(&paths).expect("both loads must succeed");
    let (first, second) = two_mod_ids(&live);
    Decide::new(JsonDecisionStore::new())
        .execute(&mut live, ignore_decision(&first))
        .expect("the late edit saves");

    let changed = reload(&mut scanned);
    Decide::new(JsonDecisionStore::new())
        .execute(&mut scanned, ignore_decision(&second))
        .expect("the next save from the new session");

    assert_eq!(changed, [PersistedKind::Decisions]);
    let on_disk = JsonDecisionStore::new()
        .load(&paths.profile_dir)
        .expect("decisions.json loads back");
    assert!(on_disk.get(&ignore_decision(&first).key).is_some());
    assert!(on_disk.get(&ignore_decision(&second).key).is_some());
}

#[test]
fn a_reload_with_no_late_edit_changes_nothing() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    let ScanAndLive { mut scanned, .. } = scan_and_live(&paths).expect("both loads must succeed");

    let changed = reload(&mut scanned);

    assert!(changed.is_empty());
}
