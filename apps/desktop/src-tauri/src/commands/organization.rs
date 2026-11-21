//! The library organization commands: groups and per-mod colours.
//!
//! Every mutating command parses its request into an [`OrganizationEdit`]
//! at the boundary, hands it to `EditOrganization` (one save, rollback on
//! failure) and, on success, emits `session://changed` with
//! `organizationChanged`. None of them touches the working set, the orders,
//! the sort, the ledger or `ModsConfig.xml`; group-level activate and
//! deactivate are the frontend's `plan_group_scope` followed by the
//! existing `plan_activate_mods`/`plan_deactivate_mods` dialogs.

use rim_session::organization::OrganizationEdit;
use rim_session::use_cases::{EditOrganization, RemoveUninstalledMembers, ResetOrganization};

use crate::commands::emit_session_changed;
use crate::dto::organization::{
    CreateGroupRequestDto, DeleteGroupRequestDto, GroupMembersRequestDto, GroupMembershipCountDto,
    GroupMembershipCountsRequestDto, GroupScopePlanDto, GroupScopeRequestDto,
    MoveGroupMembersRequestDto, OrganizationDto, OrganizationEditOutcomeDto,
    RemoveUninstalledMembersRequestDto, RenameGroupRequestDto, ReparentGroupRequestDto,
    SetGroupColorRequestDto, SetModColorsRequestDto, parse_group_id, parse_mod_ids,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::{CommandError, CommandErrorCode};
use crate::state::{AppState, with_session};

/// The organization: its availability and the group tree.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_organization_inner(
    state: &AppState,
) -> Result<OrganizationDto, CommandError> {
    with_session(state, |session| Ok(session.organization().into())).await
}

/// See `get_organization_inner`.
///
/// # Errors
///
/// See `get_organization_inner`.
#[tauri::command]
pub async fn get_organization(
    state: tauri::State<'_, AppState>,
) -> Result<OrganizationDto, CommandError> {
    get_organization_inner(&state).await
}

/// Applies one parsed edit and saves `organization.json` once.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// the `organization_unavailable` code unless organization is loaded, the
/// code of the rule that refused the edit, or `profile_io_failed` when the
/// save fails (the edit is then rolled back).
async fn edit_inner(
    state: &AppState,
    edit: OrganizationEdit,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let store = state.adapters.organization_store.clone();
    with_session(state, move |session| {
        let outcome = EditOrganization::new(store).execute(session, edit)?;
        Ok(outcome.into())
    })
    .await
}

/// Creates an empty group.
///
/// # Errors
///
/// See `edit_inner`; also [`CommandError::invalid_input`] for a bad name
/// or id.
pub(crate) async fn create_group_inner(
    state: &AppState,
    request: CreateGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Renames a group.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn rename_group_inner(
    state: &AppState,
    request: RenameGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Moves a group, with its subtree.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn reparent_group_inner(
    state: &AppState,
    request: ReparentGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Deletes a group. No mod is touched.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn delete_group_inner(
    state: &AppState,
    request: DeleteGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Sets or clears a group's colour.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn set_group_color_inner(
    state: &AppState,
    request: SetGroupColorRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Adds mods to a group. Memberships in other groups are untouched.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn add_group_members_inner(
    state: &AppState,
    request: GroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.into_add()?).await
}

/// Removes mods from a group.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn remove_group_members_inner(
    state: &AppState,
    request: GroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.into_remove()?).await
}

/// The explicit replace: moves mods from one group to another.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn move_group_members_inner(
    state: &AppState,
    request: MoveGroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Sets or clears mods' own colour.
///
/// # Errors
///
/// See `create_group_inner`.
pub(crate) async fn set_mod_colors_inner(
    state: &AppState,
    request: SetModColorsRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    edit_inner(state, request.try_into()?).await
}

/// Removes the confirmed mods that are still found nowhere on disk from a
/// group and its subgroups in one save ("Remove mods that aren't
/// installed"). Installed mods, mods the request did not name and every other
/// group are untouched.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] for group id `0` or a bad mod id, or the
/// `organization_unavailable`/`group_not_found`/`profile_io_failed` code.
pub(crate) async fn remove_uninstalled_members_inner(
    state: &AppState,
    request: RemoveUninstalledMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let group = parse_group_id(request.id)?;
    let requested = parse_mod_ids(&request.mod_ids)?;
    let store = state.adapters.organization_store.clone();
    with_session(state, move |session| {
        let outcome = RemoveUninstalledMembers::new(store).execute(session, group, &requested)?;
        Ok(outcome.into())
    })
    .await
}

/// A group's mods (subgroups included) resolved against the install: the
/// active and inactive ids a group-level activate or deactivate plans
/// over. Read-only.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] for group id `0`, or the
/// `organization_unavailable`/`group_not_found` code.
pub(crate) async fn plan_group_scope_inner(
    state: &AppState,
    request: GroupScopeRequestDto,
) -> Result<GroupScopePlanDto, CommandError> {
    let group = parse_group_id(request.id)?;
    with_session(state, move |session| {
        let resolution = session.resolve_group_scope(group)?;
        Ok((&resolution).into())
    })
    .await
}

/// How many of the requested mods each group holds directly: the sources the Move to group
/// dialog can offer. A read; nothing is saved or emitted.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] for a bad or too many ids, or the
/// `organization_unavailable` code.
pub(crate) async fn group_membership_counts_inner(
    state: &AppState,
    request: GroupMembershipCountsRequestDto,
) -> Result<Vec<GroupMembershipCountDto>, CommandError> {
    let mods = parse_mod_ids(&request.mod_ids)?;
    with_session(state, move |session| {
        let counts = session.group_membership_counts(&mods)?;
        Ok(counts
            .into_iter()
            .map(|(group, count)| GroupMembershipCountDto {
                id: group.get(),
                count,
            })
            .collect())
    })
    .await
}

/// "Start over" after an unusable `organization.json`: keeps a copy of the
/// file, then saves an empty organization.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] unless the file is unusable, or
/// `profile_io_failed` when the copy or the save fails.
pub(crate) async fn reset_organization_inner(
    state: &AppState,
) -> Result<OrganizationDto, CommandError> {
    let store = state.adapters.organization_store.clone();
    with_session(state, move |session| {
        ResetOrganization::new(store).execute(session)?;
        Ok(session.organization().into())
    })
    .await
}

/// See `create_group_inner`; also emits `session://changed`
/// (`organizationChanged`) on success.
///
/// # Errors
///
/// See `create_group_inner`.
#[tauri::command]
pub async fn create_group(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: CreateGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = create_group_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `rename_group_inner`; also emits `organizationChanged` on success.
///
/// # Errors
///
/// See `rename_group_inner`.
#[tauri::command]
pub async fn rename_group(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: RenameGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = rename_group_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `reparent_group_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `reparent_group_inner`.
#[tauri::command]
pub async fn reparent_group(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ReparentGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = reparent_group_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `delete_group_inner`; also emits `organizationChanged` on success.
///
/// # Errors
///
/// See `delete_group_inner`.
#[tauri::command]
pub async fn delete_group(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DeleteGroupRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = delete_group_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `set_group_color_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `set_group_color_inner`.
#[tauri::command]
pub async fn set_group_color(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetGroupColorRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = set_group_color_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `add_group_members_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `add_group_members_inner`.
#[tauri::command]
pub async fn add_group_members(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = add_group_members_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `remove_group_members_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `remove_group_members_inner`.
#[tauri::command]
pub async fn remove_group_members(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = remove_group_members_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `move_group_members_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `move_group_members_inner`.
#[tauri::command]
pub async fn move_group_members(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: MoveGroupMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = move_group_members_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `set_mod_colors_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `set_mod_colors_inner`.
#[tauri::command]
pub async fn set_mod_colors(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetModColorsRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = set_mod_colors_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `remove_uninstalled_members_inner`; also emits
/// `organizationChanged` on success.
///
/// # Errors
///
/// See `remove_uninstalled_members_inner`.
#[tauri::command]
pub async fn remove_uninstalled_members(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: RemoveUninstalledMembersRequestDto,
) -> Result<OrganizationEditOutcomeDto, CommandError> {
    let result = remove_uninstalled_members_inner(&state, request).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// See `plan_group_scope_inner`.
///
/// # Errors
///
/// See `plan_group_scope_inner`.
#[tauri::command]
pub async fn plan_group_scope(
    state: tauri::State<'_, AppState>,
    request: GroupScopeRequestDto,
) -> Result<GroupScopePlanDto, CommandError> {
    plan_group_scope_inner(&state, request).await
}

/// See `group_membership_counts_inner`.
///
/// # Errors
///
/// See `group_membership_counts_inner`.
#[tauri::command]
pub async fn group_membership_counts(
    state: tauri::State<'_, AppState>,
    request: GroupMembershipCountsRequestDto,
) -> Result<Vec<GroupMembershipCountDto>, CommandError> {
    group_membership_counts_inner(&state, request).await
}

/// See `reset_organization_inner`; also emits `organizationChanged` on
/// success.
///
/// # Errors
///
/// See `reset_organization_inner`.
#[tauri::command]
pub async fn reset_organization(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<OrganizationDto, CommandError> {
    let result = reset_organization_inner(&state).await;
    emit_when_stale_or_changed(&app, &result);
    result
}

/// Whether the frontend should refetch organization after this result: the
/// command worked, or it named a group that is gone, which means the
/// frontend's tree is stale (another edit or a rescan removed it). The
/// `group_not_found` message promises "the groups have been refreshed", so
/// this keeps that true.
fn should_signal_change<T>(result: &Result<T, CommandError>) -> bool {
    match result {
        Ok(_) => true,
        Err(error) => error.code == CommandErrorCode::GroupNotFound,
    }
}

/// Signals the frontend that organization changed (or turned out stale).
fn emit_when_stale_or_changed<T>(app: &tauri::AppHandle, result: &Result<T, CommandError>) {
    if should_signal_change(result) {
        emit_session_changed(app, SessionChangeReasonDto::OrganizationChanged);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rim_session::organization::{
        ModOrganization, OrganizationIssue, OrganizationLimits, OrganizationState,
    };
    use rim_session::test_support::{
        InMemoryOrganizationStore, session_fixture_with_inactive, set_organization_state,
    };

    use super::*;
    use crate::dto::mod_lists::{GroupFilterDto, ModListQueryDto};
    use crate::dto::organization::{
        ColorTokenDto, DeleteModeDto, GroupNodeDto, OrganizationIssueDto,
    };
    use crate::state::Adapters;

    /// An app over active `a`, `b_steam`, inactive `idle`, with a loaded
    /// (empty) organization and an in-memory store the test can inspect.
    fn app() -> (AppState, Arc<InMemoryOrganizationStore>) {
        let store = Arc::new(InMemoryOrganizationStore::new());
        let state = AppState {
            adapters: Adapters {
                organization_store: store.clone(),
                ..Adapters::default()
            },
            ..AppState::default()
        };
        let mut session = session_fixture_with_inactive(&["a", "b_steam"], &["idle"]);
        set_organization_state(
            &mut session,
            OrganizationState::Loaded(ModOrganization::new()),
        );
        *state.session.write().expect("lock") = Some(session);
        (state, store)
    }

    /// The tree of a `Loaded` answer; any other state fails the test.
    fn loaded_groups(organization: &OrganizationDto) -> Vec<GroupNodeDto> {
        match organization {
            OrganizationDto::Loaded { groups, .. } => groups.clone(),
            other => panic!("expected a loaded organization, got {other:?}"),
        }
    }

    fn mod_ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    async fn create(state: &AppState, name: &str, parent: Option<u32>) -> u32 {
        let outcome = create_group_inner(
            state,
            CreateGroupRequestDto {
                name: name.to_string(),
                parent,
                mod_ids: None,
            },
        )
        .await
        .expect("create group");
        match outcome {
            OrganizationEditOutcomeDto::Created { id, .. } => id,
            other => panic!("expected a created group, got {other:?}"),
        }
    }

    fn code_of<T: std::fmt::Debug>(result: Result<T, CommandError>) -> CommandErrorCode {
        result.expect_err("the command must fail").code
    }

    #[tokio::test]
    async fn a_session_without_a_store_reports_not_loaded_and_refuses_edits() {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture_with_inactive(&["a"], &[]));

        let organization = get_organization_inner(&state).await.expect("get");
        let refused = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "G".to_string(),
                parent: None,
                mod_ids: None,
            },
        )
        .await;

        assert_eq!(organization, OrganizationDto::NotLoaded);
        assert_eq!(code_of(refused), CommandErrorCode::OrganizationUnavailable);
    }

    #[tokio::test]
    async fn a_recovered_session_reports_the_issue_and_refuses_edits_until_reset() {
        let (state, store) = app();
        if let Some(session) = state.session.write().expect("lock").as_mut() {
            set_organization_state(
                session,
                OrganizationState::Recovered {
                    issue: OrganizationIssue::Malformed,
                },
            );
        }

        let organization = get_organization_inner(&state).await.expect("get");
        let refused = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "G".to_string(),
                parent: None,
                mod_ids: None,
            },
        )
        .await;
        let reset = reset_organization_inner(&state).await.expect("reset");
        let created = create(&state, "G", None).await;

        assert_eq!(
            organization,
            OrganizationDto::Recovered {
                issue: OrganizationIssueDto::Malformed
            }
        );
        assert_eq!(code_of(refused), CommandErrorCode::OrganizationUnavailable);
        assert_eq!(loaded_groups(&reset), Vec::<GroupNodeDto>::new());
        assert_eq!(store.archive_count(), 1);
        assert!(created > 0);
    }

    #[tokio::test]
    async fn resetting_a_loaded_organization_is_refused() {
        let (state, store) = app();

        let result = reset_organization_inner(&state).await;

        assert_eq!(code_of(result), CommandErrorCode::InvalidInput);
        assert_eq!(store.archive_count() + store.save_count(), 0);
    }

    #[tokio::test]
    async fn creating_renaming_and_coloring_a_group_persists_and_shows_in_the_tree() {
        let (state, store) = app();
        let weapons = create(&state, "Weapons", None).await;
        let kid = create(&state, "Kid", Some(weapons)).await;

        rename_group_inner(
            &state,
            RenameGroupRequestDto {
                id: kid,
                name: "Blades".to_string(),
            },
        )
        .await
        .expect("rename");
        set_group_color_inner(
            &state,
            SetGroupColorRequestDto {
                id: weapons,
                color: Some(ColorTokenDto::Teal),
            },
        )
        .await
        .expect("colour");
        let tree = get_organization_inner(&state).await.expect("get");

        assert_eq!(store.save_count(), 4, "one save per command");
        let groups = loaded_groups(&tree);
        let names: Vec<(&str, usize, Option<ColorTokenDto>)> = groups
            .iter()
            .map(|node| (node.name.as_str(), node.depth, node.color))
            .collect();
        assert_eq!(
            names,
            [
                ("Weapons", 1, Some(ColorTokenDto::Teal)),
                ("Blades", 2, None)
            ]
        );
    }

    #[tokio::test]
    async fn each_refusal_maps_to_its_own_code() {
        let (state, store) = app();
        let a = create(&state, "A", None).await;
        let kid = create(&state, "Kid", Some(a)).await;
        let saves_before = store.save_count();

        let taken = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "a".to_string(),
                parent: None,
                mod_ids: None,
            },
        )
        .await;
        let cycle = reparent_group_inner(
            &state,
            ReparentGroupRequestDto {
                id: a,
                parent: Some(kid),
            },
        )
        .await;
        let missing = rename_group_inner(
            &state,
            RenameGroupRequestDto {
                id: 999,
                name: "X".to_string(),
            },
        )
        .await;
        let zero = rename_group_inner(
            &state,
            RenameGroupRequestDto {
                id: 0,
                name: "X".to_string(),
            },
        )
        .await;
        let blank = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "   ".to_string(),
                parent: None,
                mod_ids: None,
            },
        )
        .await;
        let same = move_group_members_inner(
            &state,
            MoveGroupMembersRequestDto {
                from: a,
                to: a,
                mod_ids: mod_ids(&["a"]),
            },
        )
        .await;

        assert_eq!(code_of(taken), CommandErrorCode::GroupNameTaken);
        assert_eq!(code_of(cycle), CommandErrorCode::GroupCycle);
        assert_eq!(code_of(missing), CommandErrorCode::GroupNotFound);
        assert_eq!(code_of(zero), CommandErrorCode::InvalidInput);
        assert_eq!(code_of(blank), CommandErrorCode::InvalidInput);
        assert_eq!(code_of(same), CommandErrorCode::InvalidInput);
        assert_eq!(store.save_count(), saves_before, "a refusal saves nothing");
    }

    #[tokio::test]
    async fn the_depth_limit_has_its_own_code() {
        let (state, _) = app();
        let mut parent = None;
        for level in 0..OrganizationLimits::MAX_DEPTH {
            parent = Some(create(&state, &format!("level {level}"), parent).await);
        }

        let result = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "too deep".to_string(),
                parent,
                mod_ids: None,
            },
        )
        .await;

        assert_eq!(code_of(result), CommandErrorCode::OrganizationLimit);
    }

    #[tokio::test]
    async fn a_failed_save_is_a_profile_error_and_the_edit_is_rolled_back() {
        let (state, store) = app();
        store.fail_next_save();

        let result = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "G".to_string(),
                parent: None,
                mod_ids: None,
            },
        )
        .await;

        assert_eq!(code_of(result), CommandErrorCode::ProfileIoFailed);
        assert!(loaded_groups(&get_organization_inner(&state).await.expect("get")).is_empty());
    }

    #[tokio::test]
    async fn membership_commands_report_counts_and_key_mods_by_base_id() {
        let (state, _) = app();
        let weapons = create(&state, "Weapons", None).await;
        let armor = create(&state, "Armor", None).await;

        let added = add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: weapons,
                mod_ids: mod_ids(&["b_steam", "b", "a"]),
            },
        )
        .await
        .expect("add");
        let moved = move_group_members_inner(
            &state,
            MoveGroupMembersRequestDto {
                from: weapons,
                to: armor,
                mod_ids: mod_ids(&["a", "idle"]),
            },
        )
        .await
        .expect("move");
        let removed = remove_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: weapons,
                mod_ids: mod_ids(&["b_steam", "never"]),
            },
        )
        .await
        .expect("remove");

        assert_eq!(
            added,
            OrganizationEditOutcomeDto::Added {
                added: 2,
                already: 0
            },
            "b_steam and b are one mod"
        );
        assert_eq!(
            moved,
            OrganizationEditOutcomeDto::Moved {
                moved: 1,
                already_in_target: 0,
                not_in_source: 1
            }
        );
        assert_eq!(
            removed,
            OrganizationEditOutcomeDto::Removed {
                removed: 1,
                absent: 1
            }
        );
    }

    #[tokio::test]
    async fn creating_a_group_with_members_is_one_save_and_a_refusal_leaves_nothing() {
        let (state, store) = app();
        let parent = create(&state, "Parent", None).await;
        let saves = store.save_count();

        let created = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "New".to_string(),
                parent: Some(parent),
                mod_ids: Some(mod_ids(&["a", "b_steam", "b"])),
            },
        )
        .await
        .expect("create with members");
        let clash = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "new".to_string(),
                parent: Some(parent),
                mod_ids: Some(mod_ids(&["a"])),
            },
        )
        .await;
        let bad_id = create_group_inner(
            &state,
            CreateGroupRequestDto {
                name: "Other".to_string(),
                parent: None,
                mod_ids: Some(mod_ids(&[""])),
            },
        )
        .await;

        assert!(
            matches!(
                created,
                OrganizationEditOutcomeDto::Created { members: 2, .. }
            ),
            "the outcome counts mods, so b_steam and b are one: {created:?}"
        );
        assert_eq!(store.save_count(), saves + 1, "one save for create and add");
        assert_eq!(code_of(clash), CommandErrorCode::GroupNameTaken);
        assert_eq!(code_of(bad_id), CommandErrorCode::InvalidInput);
        let groups = loaded_groups(&get_organization_inner(&state).await.expect("get"));
        assert_eq!(groups.len(), 2, "the refusals created nothing");
        let new_group = groups.iter().find(|g| g.name == "New").expect("created");
        assert_eq!(new_group.direct_count, 2, "b_steam and b are one mod");
    }

    #[tokio::test]
    async fn a_mod_id_a_file_could_not_hold_is_rejected_at_the_boundary() {
        let (state, store) = app();
        let group = create(&state, "G", None).await;
        let saves = store.save_count();

        let empty = add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: group,
                mod_ids: mod_ids(&[""]),
            },
        )
        .await;
        let overlong = set_mod_colors_inner(
            &state,
            SetModColorsRequestDto {
                mod_ids: vec!["x".repeat(OrganizationLimits::MAX_ID_CHARS + 1)],
                color: Some(ColorTokenDto::Red),
            },
        )
        .await;

        assert_eq!(code_of(empty), CommandErrorCode::InvalidInput);
        assert_eq!(code_of(overlong), CommandErrorCode::InvalidInput);
        assert_eq!(store.save_count(), saves);
    }

    #[tokio::test]
    async fn the_lists_carry_labels_colours_and_honour_the_group_filter() {
        let (state, _) = app();
        let weapons = create(&state, "Weapons", None).await;
        let kid = create(&state, "Kid", Some(weapons)).await;
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: kid,
                mod_ids: mod_ids(&["b_steam"]),
            },
        )
        .await
        .expect("add");
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: weapons,
                mod_ids: mod_ids(&["idle"]),
            },
        )
        .await
        .expect("add");
        set_mod_colors_inner(
            &state,
            SetModColorsRequestDto {
                mod_ids: mod_ids(&["a"]),
                color: Some(ColorTokenDto::Amber),
            },
        )
        .await
        .expect("colour");
        let scoped = ModListQueryDto {
            group: Some(GroupFilterDto::InScope { group_id: weapons }),
            ..ModListQueryDto::default()
        };

        let everything =
            crate::commands::mods::list_working_mods_inner(&state, ModListQueryDto::default())
                .await
                .expect("list");
        let active = crate::commands::mods::list_working_mods_inner(&state, scoped.clone())
            .await
            .expect("list");
        let inactive = crate::commands::active_set::list_inactive_mods_inner(&state, scoped)
            .await
            .expect("list");
        let stale = crate::commands::mods::list_working_mods_inner(
            &state,
            ModListQueryDto {
                group: Some(GroupFilterDto::InScope { group_id: 999 }),
                ..ModListQueryDto::default()
            },
        )
        .await;

        assert_eq!(everything.items[0].color, Some(ColorTokenDto::Amber));
        assert_eq!(everything.items[1].groups.len(), 1);
        assert_eq!(everything.items[1].groups[0].name, "Kid");
        assert_eq!(
            active
                .items
                .iter()
                .map(|row| row.mod_id.as_str())
                .collect::<Vec<_>>(),
            ["b_steam"]
        );
        assert!(active.reorderable);
        assert_eq!(
            inactive
                .items
                .iter()
                .map(|row| row.mod_id.as_str())
                .collect::<Vec<_>>(),
            ["idle"]
        );
        assert_eq!(code_of(stale), CommandErrorCode::GroupNotFound);
    }

    #[tokio::test]
    async fn remove_uninstalled_members_removes_only_missing_mods_from_the_whole_scope() {
        let (state, store) = app();
        let parent = create(&state, "Parent", None).await;
        let child = create(&state, "Child", Some(parent)).await;
        let other = create(&state, "Other", None).await;
        for (group, ids) in [
            (parent, &["a", "gone"][..]),
            (child, &["idle", "gone2"][..]),
            (other, &["gone"][..]),
        ] {
            add_group_members_inner(
                &state,
                GroupMembersRequestDto {
                    id: group,
                    mod_ids: mod_ids(ids),
                },
            )
            .await
            .expect("add");
        }
        let saves_before = store.save_count();

        let outcome = remove_uninstalled_members_inner(
            &state,
            RemoveUninstalledMembersRequestDto {
                id: parent,
                mod_ids: mod_ids(&["gone", "gone2", "idle"]),
            },
        )
        .await
        .expect("remove");
        let after = plan_group_scope_inner(&state, GroupScopeRequestDto { id: parent })
            .await
            .expect("plan");
        let untouched = plan_group_scope_inner(&state, GroupScopeRequestDto { id: other })
            .await
            .expect("plan");

        assert_eq!(
            outcome,
            OrganizationEditOutcomeDto::Removed {
                removed: 2,
                absent: 0
            }
        );
        assert_eq!(
            store.save_count(),
            saves_before + 1,
            "one save for the scope"
        );
        assert_eq!(after.not_installed, 0);
        assert_eq!(after.mod_count, 2, "installed members stay");
        assert_eq!(
            untouched.not_installed, 1,
            "a group outside the scope keeps its member"
        );
    }

    #[tokio::test]
    async fn remove_uninstalled_members_refuses_a_missing_group_and_a_read_only_organization() {
        let (state, _) = app();
        let stale = remove_uninstalled_members_inner(
            &state,
            RemoveUninstalledMembersRequestDto {
                id: 99,
                mod_ids: mod_ids(&["gone"]),
            },
        )
        .await;

        let unavailable_state = AppState::default();
        *unavailable_state.session.write().expect("lock") =
            Some(session_fixture_with_inactive(&["a"], &[]));
        let unavailable = remove_uninstalled_members_inner(
            &unavailable_state,
            RemoveUninstalledMembersRequestDto {
                id: 1,
                mod_ids: mod_ids(&["gone"]),
            },
        )
        .await;

        assert_eq!(code_of(stale), CommandErrorCode::GroupNotFound);
        assert_eq!(
            code_of(unavailable),
            CommandErrorCode::OrganizationUnavailable
        );
    }

    #[tokio::test]
    async fn an_organization_too_big_for_its_file_is_the_limit_code() {
        let (state, store) = app();
        let group = create(&state, "G", None).await;
        store.fail_next_save_too_large();

        let refused = add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: group,
                mod_ids: mod_ids(&["a"]),
            },
        )
        .await;

        assert_eq!(code_of(refused), CommandErrorCode::OrganizationLimit);
    }

    #[tokio::test]
    async fn plan_group_scope_resolves_active_inactive_and_missing_mods() {
        let (state, _) = app();
        let group = create(&state, "G", None).await;
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: group,
                mod_ids: mod_ids(&["a", "idle", "gone"]),
            },
        )
        .await
        .expect("add");

        let plan = plan_group_scope_inner(&state, GroupScopeRequestDto { id: group })
            .await
            .expect("plan");
        let missing = plan_group_scope_inner(&state, GroupScopeRequestDto { id: 999 }).await;

        assert_eq!(plan.group_count, 1);
        assert_eq!(plan.mod_count, 3);
        assert_eq!(plan.active_ids, ["a"]);
        assert_eq!(plan.inactive_ids, ["idle"]);
        assert_eq!(plan.not_installed, 1);
        assert_eq!(plan.not_installed_ids, ["gone"]);
        assert_eq!(code_of(missing), CommandErrorCode::GroupNotFound);
    }

    #[tokio::test]
    async fn membership_counts_cover_only_groups_holding_a_requested_mod() {
        let (state, store) = app();
        let weapons = create(&state, "Weapons", None).await;
        let other = create(&state, "Other", None).await;
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: weapons,
                mod_ids: mod_ids(&["a", "b_steam"]),
            },
        )
        .await
        .expect("add");
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: other,
                mod_ids: mod_ids(&["idle"]),
            },
        )
        .await
        .expect("add");
        let saves = store.save_count();

        let counts = group_membership_counts_inner(
            &state,
            GroupMembershipCountsRequestDto {
                mod_ids: mod_ids(&["a", "b", "zzz"]),
            },
        )
        .await
        .expect("counts");

        assert_eq!(
            counts,
            vec![GroupMembershipCountDto {
                id: weapons,
                count: 2
            }]
        );
        assert_eq!(store.save_count(), saves, "a read saves nothing");
    }

    #[tokio::test]
    async fn membership_counts_refuse_when_organization_is_unavailable() {
        let (state, _) = app();
        {
            // Scoped so the lock is released before the first await.
            let mut guard = state.session.write().expect("lock");
            set_organization_state(
                guard.as_mut().expect("session"),
                OrganizationState::NotLoaded,
            );
        }

        let refused = group_membership_counts_inner(
            &state,
            GroupMembershipCountsRequestDto {
                mod_ids: mod_ids(&["a"]),
            },
        )
        .await;

        assert_eq!(code_of(refused), CommandErrorCode::OrganizationUnavailable);
    }

    #[tokio::test]
    async fn deleting_a_group_reports_the_counts() {
        let (state, _) = app();
        let top = create(&state, "Top", None).await;
        create(&state, "Kid", Some(top)).await;

        let outcome = delete_group_inner(
            &state,
            DeleteGroupRequestDto {
                id: top,
                mode: DeleteModeDto::WithSubgroups,
            },
        )
        .await
        .expect("delete");

        assert_eq!(
            outcome,
            OrganizationEditOutcomeDto::Deleted {
                groups_removed: 2,
                children_lifted: 0
            }
        );
    }

    #[tokio::test]
    async fn no_organization_command_changes_the_working_set_orders_or_selection() {
        let (state, _) = app();
        let snapshot = |state: &AppState| {
            let guard = state.session.read().expect("lock");
            let session = guard.as_ref().expect("a session");
            (
                session.working().ids().to_vec(),
                session.orders().current.as_slice().to_vec(),
                session.orders().suggested.as_slice().to_vec(),
                session.file_active_mods().to_vec(),
                format!("{:?}", session.selected()),
            )
        };
        let before = snapshot(&state);

        let group = create(&state, "G", None).await;
        add_group_members_inner(
            &state,
            GroupMembersRequestDto {
                id: group,
                mod_ids: mod_ids(&["a", "idle"]),
            },
        )
        .await
        .expect("add");
        set_mod_colors_inner(
            &state,
            SetModColorsRequestDto {
                mod_ids: mod_ids(&["a"]),
                color: Some(ColorTokenDto::Pink),
            },
        )
        .await
        .expect("colour");
        delete_group_inner(
            &state,
            DeleteGroupRequestDto {
                id: group,
                mode: DeleteModeDto::LiftChildren,
            },
        )
        .await
        .expect("delete");

        assert_eq!(snapshot(&state), before);
    }

    /// An organization holding one group, to stand for an edit that landed
    /// while a scan was still running.
    fn organization_with_a_late_group() -> ModOrganization {
        let mut organization = ModOrganization::new();
        organization
            .create_group(
                rim_session::organization::GroupName::new("Landed late").expect("valid name"),
                None,
            )
            .expect("create");
        organization
    }

    fn state_over_a_scratch_game(
        store: &Arc<InMemoryOrganizationStore>,
    ) -> (
        tempfile::TempDir,
        AppState,
        crate::dto::project::ProjectPathsDto,
    ) {
        let (scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState {
            adapters: Adapters {
                organization_store: store.clone(),
                ..Adapters::default()
            },
            ..AppState::default()
        };
        (scratch, state, paths)
    }

    fn live_organization(state: &AppState) -> OrganizationState {
        let guard = state.session.read().expect("lock");
        guard
            .as_ref()
            .expect("a session is loaded")
            .organization()
            .clone()
    }

    #[tokio::test]
    async fn an_edit_that_lands_after_load_project_read_the_file_is_in_the_swapped_in_session() {
        let store = Arc::new(InMemoryOrganizationStore::new());
        let (_scratch, state, paths) = state_over_a_scratch_game(&store);
        store.change_after_next_load(organization_with_a_late_group());

        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("load");

        assert_eq!(
            live_organization(&state),
            OrganizationState::Loaded(organization_with_a_late_group()),
            "the read inside the swap sees what the first read missed"
        );
    }

    #[tokio::test]
    async fn an_edit_that_lands_after_a_rescan_read_the_file_is_in_the_swapped_in_session() {
        let store = Arc::new(InMemoryOrganizationStore::new());
        let (_scratch, state, paths) = state_over_a_scratch_game(&store);
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("load");
        store.change_after_next_load(organization_with_a_late_group());

        crate::commands::active_set::rescan_project_inner(&state, |_| {})
            .await
            .expect("rescan");

        assert_eq!(
            live_organization(&state),
            OrganizationState::Loaded(organization_with_a_late_group())
        );
    }

    #[tokio::test]
    async fn a_swap_never_erases_the_late_edit_on_the_next_save() {
        let store = Arc::new(InMemoryOrganizationStore::new());
        let (_scratch, state, paths) = state_over_a_scratch_game(&store);
        store.change_after_next_load(organization_with_a_late_group());
        crate::commands::project::load_project_inner(&state, paths, |_| {})
            .await
            .expect("load");

        create(&state, "Next", None).await;

        let saved = store.last_saved().expect("the edit saved");
        assert_eq!(saved.group_count(), 2, "the late group survived the save");
    }

    #[test]
    fn the_frontend_is_told_to_refetch_after_a_success_and_after_a_vanished_group_only() {
        let ok: Result<(), CommandError> = Ok(());
        let vanished: Result<(), CommandError> =
            Err(CommandError::new(CommandErrorCode::GroupNotFound, "gone"));
        let taken: Result<(), CommandError> =
            Err(CommandError::new(CommandErrorCode::GroupNameTaken, "taken"));

        assert!(should_signal_change(&ok));
        assert!(
            should_signal_change(&vanished),
            "a stale tree must refresh, as the message promises"
        );
        assert!(
            !should_signal_change(&taken),
            "nothing changed or went stale"
        );
    }

    #[test]
    fn a_request_with_an_unknown_field_is_rejected() {
        let parsed = serde_json::from_str::<CreateGroupRequestDto>(
            r#"{"name":"G","parent":null,"surprise":1}"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn the_session_change_reason_is_organization_changed() {
        let wire =
            serde_json::to_string(&SessionChangeReasonDto::OrganizationChanged).expect("serialize");

        assert_eq!(wire, "\"organizationChanged\"");
    }

    #[test]
    fn colour_names_are_snake_case_on_the_wire_and_round_trip() {
        for token in rim_session::organization::ColorToken::ALL {
            let dto = ColorTokenDto::from(token);
            let wire = serde_json::to_string(&dto).expect("serialize");

            assert_eq!(wire, format!("\"{}\"", token.as_str()));
            assert_eq!(rim_session::organization::ColorToken::from(dto), token);
        }
    }
}
