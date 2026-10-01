//! [`PrunePatchDecisions`]: removes every decision a patch's own scoped
//! ledger currently considers orphaned.

use rim_resolve::domain::{Decision, PatchId};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{Session, UnknownPatch};

/// Everything that can go wrong pruning a patch's orphaned decisions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrunePatchDecisionsError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// Persisting the pruned project failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

/// Removes every decision `id`'s own scoped ledger currently considers
/// orphaned — a key no longer live, or no longer admitted by the patch's
/// current scope.
pub struct PrunePatchDecisions<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> PrunePatchDecisions<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Returns every pruned decision.
    ///
    /// # Errors
    ///
    /// See [`PrunePatchDecisionsError`]. On [`PrunePatchDecisionsError::Store`],
    /// the prune is rolled back.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
    ) -> Result<Vec<Decision>, PrunePatchDecisionsError> {
        let snapshot = session
            .patch_snapshot(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let source = session.selected();

        let pruned = session.patch_prune_orphaned(id, source)?;

        let project = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("still loaded throughout this call"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, project) {
            session.restore_patch(snapshot);
            return Err(PrunePatchDecisionsError::Store(error));
        }
        Ok(pruned)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, DefKey, FindingKey};

    use super::*;
    use crate::test_support::{InMemoryPatchProjectStore, patch_fixture};

    fn stale_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "NeverScanned".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        }
    }

    /// A decision on a key the report never actually produced as a live
    /// finding is orphaned the moment it's made (the scoped ledger, built
    /// from the profile's own, never contains an entry for a key with no
    /// real finding behind it) — `prune_orphaned` must remove it.
    #[test]
    fn prunes_a_decision_whose_key_is_not_a_live_finding() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        session
            .patch_decide(
                &id,
                Decision {
                    key: stale_key(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("the scope admits this key even though it's not a live finding");
        let use_case = PrunePatchDecisions::new(InMemoryPatchProjectStore::new());

        let pruned = use_case
            .execute(&mut session, &id)
            .expect("pruning must succeed");

        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].key, stale_key());
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&stale_key())
                .is_none()
        );
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let (mut session, _id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = PrunePatchDecisions::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus);

        assert!(matches!(result, Err(PrunePatchDecisionsError::Unknown(_))));
    }

    #[test]
    fn a_failed_save_rolls_back_the_prune() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        session
            .patch_decide(
                &id,
                Decision {
                    key: stale_key(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("the scope admits this key");
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let use_case = PrunePatchDecisions::new(store);

        let result = use_case.execute(&mut session, &id);

        assert!(matches!(result, Err(PrunePatchDecisionsError::Store(_))));
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&stale_key())
                .is_some(),
            "the prune must be rolled back when the save fails"
        );
    }
}
