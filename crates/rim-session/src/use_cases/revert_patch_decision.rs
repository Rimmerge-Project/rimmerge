//! [`RevertPatchDecision`]: the patch twin of [`super::RevertDecision`] —
//! removes a decision from one of a patch's own findings, persisting the
//! change through [`PatchProjectStore`] before returning.

use rim_resolve::domain::{Decision, FindingKey, PatchId};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{Session, UnknownPatch};

/// Removes a decision from one of a patch's own findings, persisting the
/// change through [`PatchProjectStore`] before returning.
pub struct RevertPatchDecision<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> RevertPatchDecision<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Returns the removed decision, if any. A key with no recorded
    /// decision is a no-op: nothing is persisted, so reverting an unknown
    /// key never rewrites the patch file.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded, or
    /// [`StoreError`] when persisting the removal fails — in which case
    /// the removal is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
        key: &FindingKey,
    ) -> Result<Option<Decision>, RevertPatchDecisionError> {
        let snapshot = session
            .patch_snapshot(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let removed = session.patch_revert(id, key)?;
        if removed.is_none() {
            return Ok(None);
        }
        let project = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("still loaded right after patch_revert succeeded"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, project) {
            session.restore_patch(snapshot);
            return Err(RevertPatchDecisionError::Store(error));
        }
        Ok(removed)
    }
}

/// Everything that can go wrong reverting one of a patch's decisions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RevertPatchDecisionError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// Persisting the removal failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, DefKey};

    use super::*;
    use crate::test_support::{InMemoryPatchProjectStore, patch_fixture};
    use crate::use_cases::DecidePatch;

    fn wall_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        }
    }

    #[test]
    fn removes_a_previously_recorded_decision() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        DecidePatch::new(InMemoryPatchProjectStore::new())
            .execute(
                &mut session,
                &id,
                Decision {
                    key: wall_key(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        let store = InMemoryPatchProjectStore::new();
        let removed = RevertPatchDecision::new(store)
            .execute(&mut session, &id, &wall_key())
            .expect("revert must succeed");

        assert!(removed.is_some());
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&wall_key())
                .is_none()
        );
    }

    #[test]
    fn reverting_an_absent_decision_is_a_no_op_and_never_saves() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = RevertPatchDecision::new(InMemoryPatchProjectStore::new());

        let removed = use_case
            .execute(&mut session, &id, &wall_key())
            .expect("revert must succeed even with nothing to remove");

        assert!(removed.is_none());
        assert!(use_case.store.last_saved(&id).is_none());
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let (mut session, _id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = RevertPatchDecision::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, &wall_key());

        assert!(matches!(result, Err(RevertPatchDecisionError::Unknown(_))));
    }

    #[test]
    fn a_failed_save_rolls_back_the_removal() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        DecidePatch::new(InMemoryPatchProjectStore::new())
            .execute(
                &mut session,
                &id,
                Decision {
                    key: wall_key(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let result = RevertPatchDecision::new(store).execute(&mut session, &id, &wall_key());

        assert!(matches!(result, Err(RevertPatchDecisionError::Store(_))));
        assert_eq!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&wall_key())
                .map(|d| &d.action),
            Some(&Action::Ignore),
            "the removed decision must be restored when the save fails"
        );
    }
}
