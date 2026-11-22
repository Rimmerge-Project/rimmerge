//! The copy commands against the real JSON store and real temp folders.

use std::fs;

use rim_session::organization::{
    ColorToken, GroupName, ModOrganization, OrganizationLimits, OrganizationState,
};
use rim_session::ports::OrganizationStore;
use rim_session::test_support::{organized, session_fixture_in_profile, set_organization_state};

use super::*;
use crate::dto::organization::{OrganizationIssueDto, OrganizationRuleDto};
use crate::dto::organization_copy::CopyModeDto;
use crate::error::CommandErrorCode;

const FILE_NAME: &str = "organization.json";

/// An app whose profile folder is a real temp folder, over the real JSON
/// store, with an empty loaded organization.
fn app(profile: &Path) -> AppState {
    let state = AppState::default();
    let mut session = session_fixture_in_profile(&["a", "b"], profile);
    set_organization_state(
        &mut session,
        OrganizationState::Loaded(ModOrganization::new()),
    );
    *state.session.write().expect("lock") = Some(session);
    state
}

fn request(dir: &Path, mode: CopyModeDto) -> CopyOrganizationRequestDto {
    CopyOrganizationRequestDto {
        source_dir: dir.to_string_lossy().into_owned(),
        mode,
    }
}

fn write_source(dir: &Path) {
    let mut source = ModOrganization::new();
    let group = source
        .create_group(GroupName::new("Weapons").expect("name"), None)
        .expect("create");
    source
        .add_members(group, &[organized("a")].into())
        .expect("add");
    source
        .set_group_color(group, Some(ColorToken::Red))
        .expect("colour");
    rim_io::JsonOrganizationStore::new()
        .save(dir, &source)
        .expect("write the source file");
}

fn refusal(answer: CopyAnswerDto) -> CopyRefusalDto {
    match answer {
        CopyAnswerDto::Refused { reason } => reason,
        other @ CopyAnswerDto::Report { .. } => panic!("expected a refusal, got {other:?}"),
    }
}

fn group_names(state: &AppState) -> Vec<String> {
    let guard = state.session.read().expect("lock");
    let session = guard.as_ref().expect("session");
    session
        .organization()
        .loaded()
        .expect("loaded")
        .tree()
        .iter()
        .map(|node| node.name.to_string())
        .collect()
}

#[tokio::test]
async fn copying_a_real_profile_folder_saves_this_profile_and_leaves_the_source_bytes() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let before = fs::read(other.path().join(FILE_NAME)).expect("read source");
    let state = app(profile.path());

    let answer = copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge))
        .await
        .expect("copy");

    assert!(matches!(answer, CopyAnswerDto::Report { report } if report.groups_added == 1));
    assert_eq!(group_names(&state), ["Weapons"]);
    assert!(
        profile.path().join(FILE_NAME).is_file(),
        "this profile's file is written"
    );
    assert_eq!(
        fs::read(other.path().join(FILE_NAME)).expect("read source"),
        before,
        "the source file is untouched"
    );
    assert_eq!(
        fs::read_dir(other.path()).expect("list").count(),
        1,
        "nothing else is written in the source folder"
    );
}

#[tokio::test]
async fn a_preview_writes_nothing() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());

    let answer =
        preview_copy_organization_inner(&state, request(other.path(), CopyModeDto::Replace))
            .await
            .expect("preview");

    assert!(matches!(answer, CopyAnswerDto::Report { .. }));
    assert_eq!(fs::read_dir(profile.path()).expect("list").count(), 0);
    assert!(group_names(&state).is_empty());
}

#[tokio::test]
async fn a_folder_without_an_organization_file_is_refused_in_both_modes() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    let state = app(profile.path());

    for mode in [CopyModeDto::Merge, CopyModeDto::Replace] {
        let previewed = preview_copy_organization_inner(&state, request(other.path(), mode))
            .await
            .expect("answer");
        let copied = copy_organization_inner(&state, request(other.path(), mode))
            .await
            .expect("answer");

        assert_eq!(refusal(previewed), CopyRefusalDto::SourceMissing);
        assert_eq!(refusal(copied), CopyRefusalDto::SourceMissing);
    }
    assert_eq!(fs::read_dir(profile.path()).expect("list").count(), 0);
}

#[tokio::test]
async fn malformed_oversized_and_rule_breaking_files_are_refused_with_the_reason() {
    let profile = tempfile::tempdir().expect("tempdir");
    let state = app(profile.path());
    let oversized = format!(
        "{{\"version\":1,\"groups\":[],\"mod_colors\":{{}},\"next_group_id\":1}}{}",
        " ".repeat(OrganizationLimits::MAX_FILE_BYTES + 1)
    );
    let cycle = "{\"version\":1,\"groups\":[\
        {\"id\":1,\"name\":\"A\",\"parent\":2,\"color\":null,\"members\":[]},\
        {\"id\":2,\"name\":\"B\",\"parent\":1,\"color\":null,\"members\":[]}],\
        \"mod_colors\":{},\"next_group_id\":3}";
    let cases = [
        ("not json".to_string(), OrganizationIssueDto::Malformed),
        (
            oversized,
            OrganizationIssueDto::TooLarge {
                limit_bytes: OrganizationLimits::MAX_FILE_BYTES,
            },
        ),
        (
            cycle.to_string(),
            OrganizationIssueDto::Invalid {
                rule: OrganizationRuleDto::WouldCreateCycle,
            },
        ),
    ];

    for (text, issue) in cases {
        let other = tempfile::tempdir().expect("tempdir");
        fs::write(other.path().join(FILE_NAME), &text).expect("write");

        let answer = copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge))
            .await
            .expect("answer");

        assert_eq!(refusal(answer), CopyRefusalDto::SourceUnusable { issue });
    }
    assert!(group_names(&state).is_empty());
    assert_eq!(fs::read_dir(profile.path()).expect("list").count(), 0);
}

#[tokio::test]
async fn this_profiles_own_folder_is_refused_however_it_is_spelled() {
    let profile = tempfile::tempdir().expect("tempdir");
    write_source(profile.path());
    let state = app(profile.path());
    let roundabout = profile
        .path()
        .join("..")
        .join(profile.path().file_name().expect("name"));

    for dir in [profile.path().to_path_buf(), roundabout] {
        let answer = copy_organization_inner(&state, request(&dir, CopyModeDto::Merge))
            .await
            .expect("answer");

        assert_eq!(refusal(answer), CopyRefusalDto::SameProfile, "{dir:?}");
    }
    assert!(
        group_names(&state).is_empty(),
        "nothing was copied onto itself"
    );
}

#[tokio::test]
async fn a_path_that_is_not_a_folder_is_refused() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());

    let file = copy_organization_inner(
        &state,
        request(&other.path().join(FILE_NAME), CopyModeDto::Merge),
    )
    .await
    .expect("answer");
    let missing = copy_organization_inner(
        &state,
        request(&other.path().join("nope"), CopyModeDto::Merge),
    )
    .await
    .expect("answer");

    assert_eq!(refusal(file), CopyRefusalDto::SourceNotAFolder);
    assert_eq!(refusal(missing), CopyRefusalDto::SourceNotAFolder);
}

#[tokio::test]
async fn an_empty_or_relative_path_is_invalid_input() {
    let profile = tempfile::tempdir().expect("tempdir");
    let state = app(profile.path());

    for raw in ["", "   ", "relative/dir", "nul\0byte"] {
        let result = copy_organization_inner(
            &state,
            CopyOrganizationRequestDto {
                source_dir: raw.to_string(),
                mode: CopyModeDto::Merge,
            },
        )
        .await;

        assert_eq!(
            result.expect_err("refused").code,
            CommandErrorCode::InvalidInput,
            "{raw:?}"
        );
    }
}

#[tokio::test]
async fn an_unavailable_organization_is_an_error_not_a_refusal() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());
    if let Some(session) = state.session.write().expect("lock").as_mut() {
        set_organization_state(session, OrganizationState::NotLoaded);
    }

    let result = copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge)).await;

    assert_eq!(
        result.expect_err("refused").code,
        CommandErrorCode::OrganizationUnavailable
    );
}

#[tokio::test]
async fn without_a_project_both_commands_say_so() {
    let state = AppState::default();
    let other = tempfile::tempdir().expect("tempdir");

    let previewed =
        preview_copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge)).await;
    let copied = copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge)).await;

    assert_eq!(
        previewed.expect_err("none").code,
        CommandErrorCode::NoProjectLoaded
    );
    assert_eq!(
        copied.expect_err("none").code,
        CommandErrorCode::NoProjectLoaded
    );
}

#[tokio::test]
async fn a_device_namespace_path_is_refused_promptly_without_being_opened() {
    let profile = tempfile::tempdir().expect("tempdir");
    let state = app(profile.path());

    for raw in [
        r"\\.\pipe\rimmerge-copy-test",
        r"//./pipe/rimmerge-copy-test",
        r"\\.\C:",
        r"\\?\GLOBALROOT\Device\Null",
        r"\\?\globalroot\device\null",
        r"\\?\pipe\x",
        r"\\?\CONIN$",
        r"\\?\GLOBALROOT\x",
        r"\\?\PhysicalDrive0",
    ] {
        let request = CopyOrganizationRequestDto {
            source_dir: raw.to_string(),
            mode: CopyModeDto::Merge,
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            copy_organization_inner(&state, request),
        )
        .await
        .expect("refused without blocking");

        assert_eq!(
            result.expect_err("refused").code,
            CommandErrorCode::InvalidInput,
            "{raw}"
        );
    }
}

#[test]
fn drive_letter_extended_and_unc_paths_are_file_system_paths() {
    for raw in [
        r"\\?\C:\profiles\one",
        r"\\server\share\profiles",
        r"\\?\UNC\server\share\x",
        r"C:\profiles",
    ] {
        assert!(is_file_system_path(raw), "{raw}");
    }
}

#[tokio::test]
async fn a_folder_holding_exactly_this_profiles_organization_is_nothing_to_copy() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());
    if let Some(session) = state.session.write().expect("lock").as_mut() {
        let loaded = rim_io::JsonOrganizationStore::new().load(other.path());
        let rim_session::organization::OrganizationLoad::Loaded(same) = loaded else {
            panic!("the source file should load");
        };
        set_organization_state(session, OrganizationState::Loaded(same));
    }

    for mode in [CopyModeDto::Merge, CopyModeDto::Replace] {
        let answer = copy_organization_inner(&state, request(other.path(), mode))
            .await
            .expect("answer");

        assert_eq!(refusal(answer), CopyRefusalDto::NothingToCopy);
    }
    assert_eq!(fs::read_dir(profile.path()).expect("list").count(), 0);
}
