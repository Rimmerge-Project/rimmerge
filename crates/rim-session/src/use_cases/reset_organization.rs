//! [`ResetOrganization`]: "start over" after an unusable `organization.json`.

use crate::Session;
use crate::organization::{ModOrganization, OrganizationState};
use crate::ports::{OrganizationStore, StoreError};

/// Why "start over" did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResetOrganizationError {
    /// The organization is not in the recovered state, so there is nothing
    /// to start over from: a loaded organization is never wiped this way.
    #[error("the organization file is not in need of a reset")]
    NotRecovered,
    /// Keeping a copy of the unusable file failed; the file was left alone.
    #[error("keeping a copy of the unusable organization file: {0}")]
    Archive(StoreError),
    /// Saving the fresh, empty organization failed.
    #[error("saving the organization: {0}")]
    Store(StoreError),
}

/// Copies the unusable file aside, then saves an empty organization over
/// it, then makes the session `Loaded` again. The only way out of the
/// recovered state besides fixing the file by hand and rescanning.
pub struct ResetOrganization<Store> {
    store: Store,
}

impl<Store: OrganizationStore> ResetOrganization<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// [`ResetOrganizationError::NotRecovered`] unless the session is in the
    /// recovered state; [`ResetOrganizationError::Archive`] when the copy
    /// fails (the unusable file is then untouched); and
    /// [`ResetOrganizationError::Store`] when the save fails (the copy
    /// exists, the session stays recovered).
    pub fn execute(&self, session: &mut Session) -> Result<(), ResetOrganizationError> {
        if !matches!(session.organization(), OrganizationState::Recovered { .. }) {
            return Err(ResetOrganizationError::NotRecovered);
        }
        let profile_dir = session.paths().profile_dir.clone();
        self.store
            .archive_unusable(&profile_dir)
            .map_err(ResetOrganizationError::Archive)?;
        let fresh = ModOrganization::new();
        self.store
            .save(&profile_dir, &fresh)
            .map_err(ResetOrganizationError::Store)?;
        session.set_organization_state(OrganizationState::Loaded(fresh));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::OrganizationIssue;
    use crate::test_support::{InMemoryOrganizationStore, session_fixture};

    fn recovered_session() -> Session {
        let mut session = session_fixture(&["a"]);
        session.set_organization_state(OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        });
        session
    }

    #[test]
    fn start_over_archives_the_file_then_saves_an_empty_organization() {
        let mut session = recovered_session();
        let store = InMemoryOrganizationStore::new();
        let use_case = ResetOrganization::new(&store);

        use_case.execute(&mut session).expect("reset must succeed");

        assert_eq!(store.archive_count(), 1);
        assert_eq!(store.last_saved(), Some(ModOrganization::new()));
        assert_eq!(
            session.organization(),
            &OrganizationState::Loaded(ModOrganization::new())
        );
    }

    #[test]
    fn a_failed_archive_leaves_the_file_and_the_session_alone() {
        let mut session = recovered_session();
        let store = InMemoryOrganizationStore::new();
        store.fail_next_archive();

        let result = ResetOrganization::new(&store).execute(&mut session);

        assert!(matches!(result, Err(ResetOrganizationError::Archive(_))));
        assert_eq!(
            store.save_count(),
            0,
            "the unusable file must not be overwritten"
        );
        assert!(matches!(
            session.organization(),
            OrganizationState::Recovered { .. }
        ));
    }

    #[test]
    fn a_failed_save_keeps_the_session_recovered() {
        let mut session = recovered_session();
        let store = InMemoryOrganizationStore::new();
        store.fail_next_save();

        let result = ResetOrganization::new(&store).execute(&mut session);

        assert!(matches!(result, Err(ResetOrganizationError::Store(_))));
        assert!(matches!(
            session.organization(),
            OrganizationState::Recovered { .. }
        ));
    }

    #[test]
    fn a_loaded_organization_is_never_wiped() {
        let mut session = session_fixture(&["a"]);
        session.set_organization_state(OrganizationState::Loaded(ModOrganization::new()));
        let store = InMemoryOrganizationStore::new();

        let result = ResetOrganization::new(&store).execute(&mut session);

        assert_eq!(result, Err(ResetOrganizationError::NotRecovered));
        assert_eq!(store.archive_count() + store.save_count(), 0);
    }
}
