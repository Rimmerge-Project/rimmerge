//! [`Session`]'s organization: the profile's groups and mod colours.
//! Held beside the sort and the ledger, never read by either.

use super::Session;
use crate::mod_inventory::ModInventory;
use crate::organization::{
    GroupId, GroupScopeResolution, ModOrganization, OrganizationQueryError, OrganizationState,
    OrganizationUnavailable, resolve_scope,
};
use crate::ports::OrganizationStore;

impl Session {
    /// The organization state: loaded, never loaded, or recovered from an
    /// unusable file.
    #[must_use]
    pub fn organization(&self) -> &OrganizationState {
        &self.organization
    }

    /// Replaces the organization state. Set by
    /// [`crate::use_cases::LoadProject`] right after construction (so a
    /// rescan or an import, which build a new session, reload it from
    /// disk) and by [`crate::use_cases::ResetOrganization`]. Outside the
    /// crate only [`Self::reload_organization`] refreshes it (and the
    /// `test-support` helper builds fixtures).
    pub(crate) fn set_organization_state(&mut self, state: OrganizationState) {
        self.organization = state;
    }

    /// Re-reads the organization from `store`, replacing the state. A
    /// session that never loaded it (`NotLoaded`, built without a store)
    /// is left alone, so this can never conjure organization into a
    /// session that must refuse edits.
    ///
    /// The desktop calls this on a freshly built session **under the
    /// session write lock, just before it is swapped in**: every
    /// organization edit saves while holding that lock, so an edit that
    /// landed after `LoadProject` read the file is on disk by now and
    /// this read sees it. Without it the swap would drop that edit from
    /// memory and the next save would erase it from disk.
    pub fn reload_organization(&mut self, store: &dyn OrganizationStore) {
        if matches!(self.organization, OrganizationState::NotLoaded) {
            return;
        }
        self.organization = store.load(&self.paths.profile_dir).into();
    }

    /// The organization, for a use case to edit. Refused unless `Loaded`,
    /// so nothing can save over a file the session could not read.
    pub(crate) fn organization_for_edit(
        &mut self,
    ) -> Result<&mut ModOrganization, OrganizationUnavailable> {
        self.organization.loaded_mut()
    }

    /// A group's mods (descendants included) resolved against this
    /// install: what a group-level activate or deactivate plans over.
    /// Pure (`&self`); changes nothing.
    ///
    /// # Errors
    ///
    /// [`OrganizationQueryError`] when organization is unavailable or the
    /// group does not exist.
    pub fn resolve_group_scope(
        &self,
        group: GroupId,
    ) -> Result<GroupScopeResolution, OrganizationQueryError> {
        let scope = self.organization.loaded()?.scope(group)?;
        let inventory = ModInventory::from_report(&self.report);
        let resolved = resolve_scope(&scope.mods, &inventory, &self.working);
        Ok(GroupScopeResolution { scope, resolved })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::{GroupName, OrganizationIssue};
    use crate::test_support::{InMemoryOrganizationStore, session_fixture};

    fn organization_with_group(text: &str) -> ModOrganization {
        let mut organization = ModOrganization::new();
        organization
            .create_group(GroupName::new(text).expect("valid name"), None)
            .expect("create");
        organization
    }

    #[test]
    fn reloading_picks_up_an_edit_that_landed_after_the_first_read() {
        let store = InMemoryOrganizationStore::new();
        store.change_after_next_load(organization_with_group("Late edit"));
        let mut session = session_fixture(&["a"]);
        let first_read = store.load(&session.paths().profile_dir);
        session.set_organization_state(first_read.into());
        assert_eq!(
            session.organization(),
            &OrganizationState::Loaded(ModOrganization::new()),
            "the first read predates the edit"
        );

        session.reload_organization(&store);

        assert_eq!(
            session.organization(),
            &OrganizationState::Loaded(organization_with_group("Late edit"))
        );
    }

    #[test]
    fn reloading_a_recovered_session_follows_the_file_again() {
        let store = InMemoryOrganizationStore::new();
        let mut session = session_fixture(&["a"]);
        session.set_organization_state(OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        });

        session.reload_organization(&store);

        assert_eq!(
            session.organization(),
            &OrganizationState::Loaded(ModOrganization::new())
        );
    }

    #[test]
    fn a_session_built_without_a_store_stays_not_loaded() {
        let store = InMemoryOrganizationStore::seeded(
            crate::organization::OrganizationLoad::Loaded(organization_with_group("Elsewhere")),
        );
        let mut session = session_fixture(&["a"]);

        session.reload_organization(&store);

        assert_eq!(session.organization(), &OrganizationState::NotLoaded);
    }
}
