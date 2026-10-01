//! [`DeletePatch`]: removes a compat patch project. Never touches a
//! previously exported folder — deleting the project is purely bookkeeping.

use rim_resolve::domain::{PatchId, PatchProject};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{Session, UnknownPatch};

/// Everything that can go wrong deleting a patch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeletePatchError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// Removing the persisted file failed.
    #[error("deleting the patch: {0}")]
    Store(StoreError),
}

/// Deletes a patch project: `store.delete` first, then removes it from the
/// session — a failed delete never removes the in-memory project, so a
/// project that couldn't be deleted from disk doesn't silently vanish from
/// the list either.
pub struct DeletePatch<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> DeletePatch<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`DeletePatchError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
    ) -> Result<PatchProject, DeletePatchError> {
        if session.patch(id).is_none() {
            return Err(DeletePatchError::Unknown(UnknownPatch(id.clone())));
        }
        if let Err(error) = self.store.delete(&session.paths().profile_dir, id) {
            return Err(DeletePatchError::Store(error));
        }
        Ok(session
            .remove_patch(id)
            .unwrap_or_else(|| unreachable!("just checked it exists above")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryPatchProjectStore, patch_fixture, session_fixture};

    #[test]
    fn deletes_a_known_patch() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let store = InMemoryPatchProjectStore::new();
        let use_case = DeletePatch::new(store);

        let removed = use_case
            .execute(&mut session, &id)
            .expect("deleting a known patch must succeed");

        assert_eq!(removed.id(), &id);
        assert!(session.patch(&id).is_none());
        assert!(use_case.store.was_deleted(&id));
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = DeletePatch::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus);

        assert!(matches!(result, Err(DeletePatchError::Unknown(_))));
    }

    #[test]
    fn a_failed_delete_leaves_the_patch_in_place() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_delete();
        let use_case = DeletePatch::new(store);

        let result = use_case.execute(&mut session, &id);

        assert!(matches!(result, Err(DeletePatchError::Store(_))));
        assert!(session.patch(&id).is_some());
    }
}
