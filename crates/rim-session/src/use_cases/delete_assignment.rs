//! [`DeleteAssignment`]: removes an assignment (patch maker) project.
//! Never touches a previously exported folder — deleting the project is
//! purely bookkeeping, mirroring [`super::DeletePatch`] exactly.

use rim_resolve::domain::{AssignmentId, AssignmentProject};

use crate::ports::{AssignmentProjectStore, StoreError};
use crate::{Session, UnknownAssignment};

/// Everything that can go wrong deleting an assignment project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeleteAssignmentError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// Removing the persisted file failed.
    #[error("deleting the assignment project: {0}")]
    Store(StoreError),
}

/// Deletes an assignment project: `store.delete` first, then removes it
/// from the session — a failed delete never removes the in-memory
/// project.
pub struct DeleteAssignment<Store> {
    store: Store,
}

impl<Store: AssignmentProjectStore> DeleteAssignment<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`DeleteAssignmentError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
    ) -> Result<AssignmentProject, DeleteAssignmentError> {
        if session.assignment(id).is_none() {
            return Err(DeleteAssignmentError::Unknown(UnknownAssignment(
                id.clone(),
            )));
        }
        if let Err(error) = self.store.delete(&session.paths().profile_dir, id) {
            return Err(DeleteAssignmentError::Store(error));
        }
        Ok(session
            .remove_assignment(id)
            .unwrap_or_else(|| unreachable!("just checked it exists above")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        InMemoryAssignmentProjectStore, assignment_fixture, session_fixture,
    };

    #[test]
    fn deletes_a_known_assignment() {
        let (mut session, id) = assignment_fixture(&["a", "b"], &["a"], &["b"]);
        let store = InMemoryAssignmentProjectStore::new();
        let use_case = DeleteAssignment::new(store);

        let removed = use_case
            .execute(&mut session, &id)
            .expect("deleting a known project must succeed");

        assert_eq!(removed.id(), &id);
        assert!(session.assignment(&id).is_none());
        assert!(use_case.store.was_deleted(&id));
    }

    #[test]
    fn rejects_an_unknown_assignment() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = DeleteAssignment::new(InMemoryAssignmentProjectStore::new());
        let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus);

        assert!(matches!(result, Err(DeleteAssignmentError::Unknown(_))));
    }

    #[test]
    fn a_failed_delete_leaves_the_assignment_in_place() {
        let (mut session, id) = assignment_fixture(&["a", "b"], &["a"], &["b"]);
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_delete();
        let use_case = DeleteAssignment::new(store);

        let result = use_case.execute(&mut session, &id);

        assert!(matches!(result, Err(DeleteAssignmentError::Store(_))));
        assert!(session.assignment(&id).is_some());
    }
}
