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

/// A store that counts every read, so a test can prove none happened.
#[derive(Clone, Default)]
struct CountingStore {
    loads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl OrganizationStore for CountingStore {
    fn load(&self, _dir: &Path) -> rim_session::organization::OrganizationLoad {
        self.loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        rim_session::organization::OrganizationLoad::Missing
    }

    fn save(
        &self,
        _dir: &Path,
        _organization: &ModOrganization,
    ) -> Result<(), rim_session::ports::OrganizationSaveError> {
        panic!("a refused path never saves");
    }

    fn archive_unusable(&self, _dir: &Path) -> Result<(), rim_session::ports::StoreError> {
        panic!("a copy never archives");
    }
}

/// A network share is refused from the text alone, before the session is
/// even consulted (an empty state answers with the path refusal, not
/// `no_project_loaded`) and before the store reads anything, so no host is
/// ever contacted.
#[cfg(windows)]
#[tokio::test]
async fn a_network_share_is_refused_before_anything_is_touched() {
    for raw in [
        r"\\server\share\profiles",
        r"//server/share/profiles",
        r"\\?\UNC\server\share\x",
        r"\\server@SSL\DavWWWRoot\x",
    ] {
        let state = AppState::default();
        let store = CountingStore::default();
        let request = || CopyOrganizationRequestDto {
            source_dir: raw.to_string(),
            mode: CopyModeDto::Merge,
        };

        let previewed = preview_copy_organization_with(&state, request(), store.clone()).await;
        let copied = copy_organization_with(&state, request(), store.clone()).await;

        for result in [previewed, copied] {
            assert_eq!(
                result.expect_err("refused").code,
                CommandErrorCode::NetworkPathRefused,
                "{raw}"
            );
        }
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "{raw}"
        );
    }
}

#[tokio::test]
async fn a_drive_letter_and_an_extended_length_drive_path_are_accepted() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());
    let canonical = other.path().canonicalize().expect("canonical");

    let plain = preview_copy_organization_inner(&state, request(other.path(), CopyModeDto::Merge))
        .await
        .expect("drive path");
    let verbatim = preview_copy_organization_inner(&state, request(&canonical, CopyModeDto::Merge))
        .await
        .expect("verbatim drive path");

    assert!(matches!(plain, CopyAnswerDto::Report { .. }));
    assert!(matches!(verbatim, CopyAnswerDto::Report { .. }));
}

/// The real JSON store whose `load` waits at a gate.
#[derive(Clone)]
struct GatedStore {
    gate: std::sync::Arc<crate::test_support::Gate>,
    inner: std::sync::Arc<rim_io::JsonOrganizationStore>,
}

impl GatedStore {
    fn new(gate: &std::sync::Arc<crate::test_support::Gate>) -> Self {
        Self {
            gate: std::sync::Arc::clone(gate),
            inner: std::sync::Arc::new(rim_io::JsonOrganizationStore::new()),
        }
    }
}

impl OrganizationStore for GatedStore {
    fn load(&self, dir: &Path) -> rim_session::organization::OrganizationLoad {
        self.gate.block();
        self.inner.load(dir)
    }

    fn save(
        &self,
        dir: &Path,
        organization: &ModOrganization,
    ) -> Result<(), rim_session::ports::OrganizationSaveError> {
        self.inner.save(dir, organization)
    }

    fn archive_unusable(&self, dir: &Path) -> Result<(), rim_session::ports::StoreError> {
        self.inner.archive_unusable(dir)
    }
}

/// While the source folder is being read another session command must run.
/// Were the lock held across the read, the second command would wait for a
/// release only it can send, and the read would give up on its timeout.
#[tokio::test]
async fn the_session_lock_is_free_while_the_source_is_read() {
    for is_copy in [false, true] {
        let profile = tempfile::tempdir().expect("tempdir");
        let other = tempfile::tempdir().expect("tempdir");
        write_source(other.path());
        let state = app(profile.path());
        let gate = crate::test_support::Gate::new();
        let store = GatedStore::new(&gate);
        let wanted = request(other.path(), CopyModeDto::Merge);

        let command = async {
            if is_copy {
                copy_organization_with(&state, wanted, store).await
            } else {
                preview_copy_organization_with(&state, wanted, store).await
            }
        };
        let other_command = async {
            gate.wait_until_blocked().await;
            let meanwhile = with_session(&state, |session| Ok(session.working().ids().len())).await;
            gate.release();
            meanwhile
        };
        let (answer, meanwhile) = tokio::join!(command, other_command);

        assert_eq!(meanwhile.expect("a second command ran"), 2, "{is_copy}");
        assert!(gate.was_released(), "{is_copy}: the lock blocked the read");
        assert!(matches!(
            answer.expect("answer"),
            CopyAnswerDto::Report { .. }
        ));
        assert_eq!(group_names(&state).len(), usize::from(is_copy));
    }
}

/// An organization edit that lands while the source is being read is merged
/// with by the commit, not overwritten: the plan reads this profile under the
/// lock, at the commit.
#[tokio::test]
async fn an_edit_made_while_the_source_is_read_is_kept_by_the_copy() {
    let profile = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(profile.path());
    let gate = crate::test_support::Gate::new();
    let store = GatedStore::new(&gate);

    let command = copy_organization_with(&state, request(other.path(), CopyModeDto::Merge), store);
    let edit = async {
        gate.wait_until_blocked().await;
        let edited = with_session(&state, |session| {
            let mut organization = ModOrganization::new();
            organization
                .create_group(GroupName::new("Mine").expect("name"), None)
                .expect("create");
            set_organization_state(session, OrganizationState::Loaded(organization));
            Ok(())
        })
        .await;
        gate.release();
        edited
    };
    let (answer, edited) = tokio::join!(command, edit);

    edited.expect("the edit ran");
    assert!(matches!(
        answer.expect("answer"),
        CopyAnswerDto::Report { .. }
    ));
    let mut names = group_names(&state);
    names.sort();
    assert_eq!(names, ["Mine", "Weapons"]);
}

/// A project loaded while the source was read invalidates the checks that
/// were made against the old profile folder.
#[tokio::test]
async fn a_project_swapped_while_the_source_is_read_is_refused() {
    let first = tempfile::tempdir().expect("tempdir");
    let second = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    write_source(other.path());
    let state = app(first.path());
    let gate = crate::test_support::Gate::new();
    let store = GatedStore::new(&gate);

    let command = copy_organization_with(&state, request(other.path(), CopyModeDto::Merge), store);
    let swap = async {
        gate.wait_until_blocked().await;
        let mut replacement = session_fixture_in_profile(&["a", "b"], second.path());
        set_organization_state(
            &mut replacement,
            OrganizationState::Loaded(ModOrganization::new()),
        );
        *state.session.write().expect("lock") = Some(replacement);
        gate.release();
    };
    let (result, ()) = tokio::join!(command, swap);

    assert_eq!(
        result.expect_err("refused").code,
        CommandErrorCode::ProjectChanged
    );
    assert!(group_names(&state).is_empty());
    assert_eq!(fs::read_dir(second.path()).expect("list").count(), 0);
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
