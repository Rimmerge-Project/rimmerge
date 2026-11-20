//! [`RemoveUninstalledMembers`]: "Remove mods that aren't installed" on a
//! group and its subgroups.

use std::collections::BTreeSet;

use crate::Session;
use crate::organization::{
    EditOutcome, GroupId, OrganizationEdit, OrganizationQueryError, OrganizedModId,
};
use crate::ports::OrganizationStore;
use crate::use_cases::{EditOrganization, EditOrganizationError};

/// Why removing the uninstalled members did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoveUninstalledMembersError {
    /// Organization is unavailable or the group does not exist.
    #[error(transparent)]
    Query(#[from] OrganizationQueryError),
    /// The removal itself was refused or its save failed.
    #[error(transparent)]
    Edit(#[from] EditOrganizationError),
}

/// Removes from a group and all its subgroups the members the user was
/// shown as not installed, in one save.
///
/// The ids the caller names are a *request*, not a command: the use case
/// re-resolves the scope against the install as it is now and removes only
/// the requested ids that are still found nowhere on disk. A mod that was
/// installed again between the confirmation and the click therefore keeps
/// its memberships. Installed mods and every group outside the scope are
/// never touched.
pub struct RemoveUninstalledMembers<Store> {
    store: Store,
}

impl<Store: OrganizationStore> RemoveUninstalledMembers<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// [`RemoveUninstalledMembersError::Query`] when organization is
    /// unavailable or `group` does not exist, and
    /// [`RemoveUninstalledMembersError::Edit`] when the save fails (the
    /// session is then rolled back, see [`EditOrganization`]).
    pub fn execute(
        &self,
        session: &mut Session,
        group: GroupId,
        requested: &BTreeSet<OrganizedModId>,
    ) -> Result<EditOutcome, RemoveUninstalledMembersError> {
        let resolution = session.resolve_group_scope(group)?;
        let mods = resolution
            .resolved
            .not_installed
            .intersection(requested)
            .cloned()
            .collect();
        let edit = OrganizationEdit::RemoveMembersFromGroups {
            groups: resolution.scope.groups,
            mods,
        };
        Ok(EditOrganization::new(&self.store).execute(session, edit)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::{
        GroupName, MembershipRemoval, ModOrganization, OrganizationIssue, OrganizationState,
    };
    use crate::test_support::{
        InMemoryOrganizationStore, organized, session_fixture_with_inactive,
    };

    fn mods(ids: &[&str]) -> BTreeSet<OrganizedModId> {
        ids.iter().map(|id| organized(id)).collect()
    }

    fn name(text: &str) -> GroupName {
        GroupName::new(text).expect("valid name")
    }

    /// Active `a`, inactive `idle`; "Parent" holds `a`, `gone`; its child
    /// "Child" holds `idle`, `gone2`; "Other" holds `gone`.
    fn session_with_missing_members() -> (Session, GroupId, GroupId) {
        let mut session = session_fixture_with_inactive(&["a"], &["idle"]);
        let mut organization = ModOrganization::new();
        let parent = organization
            .create_group(name("Parent"), None)
            .expect("create");
        let child = organization
            .create_group(name("Child"), Some(parent))
            .expect("create");
        let other = organization
            .create_group(name("Other"), None)
            .expect("create");
        organization
            .add_members(parent, &mods(&["a", "gone"]))
            .expect("seed");
        organization
            .add_members(child, &mods(&["idle", "gone2"]))
            .expect("seed");
        organization
            .add_members(other, &mods(&["gone"]))
            .expect("seed");
        session.set_organization_state(OrganizationState::Loaded(organization));
        (session, parent, other)
    }

    fn members_of(session: &Session, group: GroupId) -> BTreeSet<OrganizedModId> {
        session
            .organization()
            .loaded()
            .expect("loaded")
            .scope(group)
            .expect("group exists")
            .mods
    }

    #[test]
    fn removes_the_requested_missing_mods_from_the_whole_scope_in_one_save() {
        let (mut session, parent, other) = session_with_missing_members();
        let store = InMemoryOrganizationStore::new();

        let outcome = RemoveUninstalledMembers::new(&store)
            .execute(&mut session, parent, &mods(&["gone", "gone2"]))
            .expect("removal succeeds");

        assert_eq!(
            outcome,
            EditOutcome::Removed(MembershipRemoval {
                removed: 2,
                absent: 0
            })
        );
        assert_eq!(store.save_count(), 1);
        assert_eq!(members_of(&session, parent), mods(&["a", "idle"]));
        assert_eq!(
            members_of(&session, other),
            mods(&["gone"]),
            "a group outside the scope keeps its member"
        );
    }

    #[test]
    fn a_requested_mod_that_is_installed_now_is_kept() {
        let (mut session, parent, _) = session_with_missing_members();
        let store = InMemoryOrganizationStore::new();

        RemoveUninstalledMembers::new(&store)
            .execute(&mut session, parent, &mods(&["gone", "idle", "a"]))
            .expect("removal succeeds");

        assert_eq!(
            members_of(&session, parent),
            mods(&["a", "idle", "gone2"]),
            "idle and a are installed, gone2 was never requested"
        );
    }

    #[test]
    fn requesting_nothing_removes_nothing_and_writes_nothing() {
        let (mut session, parent, _) = session_with_missing_members();
        let store = InMemoryOrganizationStore::new();

        RemoveUninstalledMembers::new(&store)
            .execute(&mut session, parent, &BTreeSet::new())
            .expect("an empty request is fine");

        assert_eq!(store.save_count(), 0);
    }

    #[test]
    fn a_missing_group_and_a_read_only_organization_are_refused() {
        let (mut session, _, _) = session_with_missing_members();
        let store = InMemoryOrganizationStore::new();
        let ghost = GroupId::new(99).expect("non-zero");
        let missing =
            RemoveUninstalledMembers::new(&store).execute(&mut session, ghost, &mods(&["gone"]));
        session.set_organization_state(OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        });
        let read_only = RemoveUninstalledMembers::new(&store).execute(
            &mut session,
            GroupId::new(1).expect("non-zero"),
            &mods(&["gone"]),
        );

        assert!(matches!(
            missing,
            Err(RemoveUninstalledMembersError::Query(
                OrganizationQueryError::Rejected(_)
            ))
        ));
        assert!(matches!(
            read_only,
            Err(RemoveUninstalledMembersError::Query(
                OrganizationQueryError::Unavailable(_)
            ))
        ));
        assert_eq!(store.save_count(), 0);
    }

    #[test]
    fn a_failed_save_rolls_the_removal_back() {
        let (mut session, parent, _) = session_with_missing_members();
        let before = session.organization().clone();
        let store = InMemoryOrganizationStore::new();
        store.fail_next_save();

        let result =
            RemoveUninstalledMembers::new(&store).execute(&mut session, parent, &mods(&["gone"]));

        assert!(matches!(
            result,
            Err(RemoveUninstalledMembersError::Edit(_))
        ));
        assert_eq!(session.organization(), &before);
    }
}
