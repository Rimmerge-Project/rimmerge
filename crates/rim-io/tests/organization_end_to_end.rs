//! End-to-end: organization through `LoadProject` and the real
//! `JsonOrganizationStore`, against a scratch copy of `rim-analyzer`'s
//! sample game tree. Everything (game folder, `ModsConfig.xml`, profile)
//! lives under a temp directory; nothing outside it is read or written.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rim_io::{
    AnalyzerScanner, JsonDecisionStore, JsonOrganizationStore, JsonRuleStore, ModsConfigFileStore,
};
use rim_session::ProjectPaths;
use rim_session::organization::{
    EditOutcome, GroupName, OrganizationEdit, OrganizationIssue, OrganizationState, OrganizedModId,
};
use rim_session::use_cases::{
    EditOrganization, EditOrganizationError, LoadProject, ResetOrganization,
};
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
    rim_session::ports::NoPatchStore,
    rim_session::ports::NoAssignmentStore,
    rim_session::test_support::FakeModKnowledgeStore,
>;

fn load_project() -> RealLoadProject {
    LoadProject::new(
        AnalyzerScanner::new(),
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        rim_session::ports::NoPatchStore,
        rim_session::ports::NoAssignmentStore,
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    )
}

/// A `Result` (not unwrapped here) so the `expect` lives in the `#[test]`
/// function, where `clippy::expect_used`'s test allowance applies.
fn name(text: &str) -> Result<GroupName, rim_session::organization::InvalidGroupName> {
    GroupName::new(text)
}

#[test]
fn groups_persist_to_organization_json_and_survive_a_rescan_without_touching_anything_else() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    let mods_config_before = fs::read(&paths.mods_config).expect("read ModsConfig.xml");
    let store = Arc::new(JsonOrganizationStore::new());
    let use_case = load_project().with_organization_store(store.clone());
    let mut session = use_case
        .execute(paths.clone(), &mut |_| {})
        .expect("load must succeed");
    let some_mod = session
        .working()
        .ids()
        .first()
        .cloned()
        .expect("the sample game has active mods");
    let edit = EditOrganization::new(store.clone());

    let EditOutcome::Created(group) = edit
        .execute(
            &mut session,
            OrganizationEdit::CreateGroup {
                name: name("Favourites").expect("valid name"),
                parent: None,
            },
        )
        .expect("create")
    else {
        panic!("creating a group reports its id");
    };
    edit.execute(
        &mut session,
        OrganizationEdit::AddMembers {
            id: group,
            mods: BTreeSet::from([OrganizedModId::new(&some_mod).expect("valid id")]),
        },
    )
    .expect("add");

    assert!(paths.profile_dir.join("organization.json").is_file());
    assert_eq!(
        fs::read(&paths.mods_config).expect("read ModsConfig.xml"),
        mods_config_before,
        "organization never writes ModsConfig.xml"
    );
    assert!(!paths.profile_dir.join("rules.json").exists());
    assert!(!paths.profile_dir.join("decisions.json").exists());

    let rescanned = use_case
        .execute_with_active_set(paths.clone(), session.working().ids().to_vec(), &mut |_| {})
        .expect("rescan must succeed");
    let organization = rescanned.organization().loaded().expect("loaded again");
    assert_eq!(organization.group_count(), 1);
    assert_eq!(
        organization
            .groups_of(&OrganizedModId::new(&some_mod).expect("valid id"))
            .len(),
        1
    );
}

#[test]
fn a_damaged_file_comes_up_recovered_refuses_edits_and_start_over_keeps_a_copy() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    fs::create_dir_all(&paths.profile_dir).expect("profile dir");
    let damaged = "{ this is not json, but it is the user's file }";
    fs::write(paths.profile_dir.join("organization.json"), damaged).expect("seed damaged file");
    let store = Arc::new(JsonOrganizationStore::new());
    let use_case = load_project().with_organization_store(store.clone());
    let mut session = use_case
        .execute(paths.clone(), &mut |_| {})
        .expect("a damaged organization file must not fail the load");

    assert_eq!(
        session.organization(),
        &OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed
        }
    );
    let refused = EditOrganization::new(store.clone()).execute(
        &mut session,
        OrganizationEdit::CreateGroup {
            name: name("Nope").expect("valid name"),
            parent: None,
        },
    );
    assert!(matches!(
        refused,
        Err(EditOrganizationError::Unavailable(_))
    ));
    assert_eq!(
        fs::read_to_string(paths.profile_dir.join("organization.json")).expect("read"),
        damaged,
        "a refused edit leaves the file alone"
    );

    ResetOrganization::new(store.clone())
        .execute(&mut session)
        .expect("start over");

    let archives: Vec<_> = fs::read_dir(&paths.profile_dir)
        .expect("list profile")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|file| file.to_str())
                .is_some_and(|file| file.starts_with("organization.json.unusable-"))
        })
        .collect();
    assert_eq!(archives.len(), 1);
    assert_eq!(
        fs::read_to_string(&archives[0]).expect("read copy"),
        damaged
    );
    assert!(matches!(
        session.organization(),
        OrganizationState::Loaded(_)
    ));
    let reloaded = use_case
        .execute(paths, &mut |_| {})
        .expect("load must succeed");
    assert!(matches!(
        reloaded.organization(),
        OrganizationState::Loaded(organization) if organization.group_count() == 0
    ));
}

#[test]
fn an_edit_that_would_write_more_than_the_reader_accepts_is_refused_and_rolled_back() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("scratch paths");
    let store = Arc::new(JsonOrganizationStore::new());
    let use_case = load_project().with_organization_store(store.clone());
    let mut session = use_case
        .execute(paths.clone(), &mut |_| {})
        .expect("load must succeed");
    let edit = EditOrganization::new(store.clone());
    let mut saved_groups = 0_usize;

    // Each group takes the most members, with the longest ids, so the file
    // outgrows the reader's cap after a handful.
    let refusal = loop {
        let outcome = edit.execute(
            &mut session,
            OrganizationEdit::CreateGroup {
                name: name(&format!("group {saved_groups}")).expect("valid name"),
                parent: None,
            },
        );
        let Ok(EditOutcome::Created(group)) = outcome else {
            panic!("creating an empty group is small enough: {outcome:?}");
        };
        let members: BTreeSet<_> = (0
            ..rim_session::organization::OrganizationLimits::MAX_MEMBERS_PER_GROUP)
            .map(|member| {
                let id = format!("{saved_groups:02}.{member:05}.{}", "x".repeat(110));
                OrganizedModId::new(&rim_analyzer::domain::ModId::new(&id)).expect("valid id")
            })
            .collect();
        match edit.execute(
            &mut session,
            OrganizationEdit::AddMembers {
                id: group,
                mods: members,
            },
        ) {
            Ok(_) => saved_groups += 1,
            Err(error) => break error,
        }
        assert!(
            saved_groups < 50,
            "the cap must bite long before the limits"
        );
    };

    assert!(
        matches!(refusal, EditOrganizationError::Store(_)),
        "{refusal:?}"
    );
    let in_memory = session.organization().loaded().expect("still loaded");
    assert_eq!(
        in_memory
            .groups()
            .map(|(_, group)| group.members().len())
            .filter(|count| *count > 0)
            .count(),
        saved_groups,
        "the refused add left no members behind"
    );
    let reloaded = use_case
        .execute(paths, &mut |_| {})
        .expect("the file on disk must still load");
    assert_eq!(
        reloaded.organization(),
        session.organization(),
        "memory and disk agree after the refusal"
    );
}
