//! [`DecidePatch`]: the patch twin of [`super::Decide`] — validates and
//! records a decision on one of a patch's own findings, persisting it
//! through [`PatchProjectStore`] before returning.

use rim_resolve::domain::{Decision, LedgerStats, PatchId};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{PatchDecideError, Session};

/// Everything that can go wrong deciding one of a patch's findings.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecidePatchError {
    /// The decision was rejected — no such patch, or the decision failed
    /// [`rim_resolve::domain::PatchProject::decide`]'s own validation.
    #[error(transparent)]
    Patch(#[from] PatchDecideError),
    /// Persisting the decision failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

/// Validates and records a decision on one of `id`'s own findings,
/// persisting it through [`PatchProjectStore`] before returning that
/// patch's freshly recomputed [`LedgerStats`].
pub struct DecidePatch<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> DecidePatch<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`DecidePatchError`]. On [`DecidePatchError::Store`], the
    /// decision is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
        decision: Decision,
    ) -> Result<LedgerStats, DecidePatchError> {
        let snapshot = session.patch_snapshot(id);
        session.patch_decide(id, decision)?;
        let snapshot =
            snapshot.unwrap_or_else(|| unreachable!("patch_decide above already validated the id"));

        let project = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("still loaded right after patch_decide succeeded"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, project) {
            session.restore_patch(snapshot);
            return Err(DecidePatchError::Store(error));
        }

        let source = session.selected();
        let stats = session
            .patch_ledger(id, source)
            .unwrap_or_else(|_| unreachable!("still loaded right after the save above"))
            .stats;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, DefKey, FindingKey};

    use super::*;
    use crate::test_support::{InMemoryPatchProjectStore, patch_fixture};

    fn wall_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        }
    }

    fn decision(action: Action) -> Decision {
        Decision {
            key: wall_key(),
            action,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn records_and_persists_a_decision() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let store = InMemoryPatchProjectStore::new();
        let use_case = DecidePatch::new(store);

        use_case
            .execute(&mut session, &id, decision(Action::Ignore))
            .expect("ignore is always a valid patch action");

        let project = session.patch(&id).expect("still loaded");
        assert_eq!(
            project.decisions().get(&wall_key()).map(|d| &d.action),
            Some(&Action::Ignore)
        );
        assert!(use_case.store.last_saved(&id).is_some());
    }

    /// A patch decision ("a patch decision never touches the sort or the
    /// profile ledger") must leave
    /// the profile's own ledger stats and sort outcome byte-identical —
    /// `Session::invalidate_patch_caches` only ever clears that one
    /// patch's own scoped caches, never `self.ledgers`/`self.sort`.
    #[test]
    fn a_patch_decision_never_changes_the_profiles_own_ledger_or_sort() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let source = session.selected();
        let stats_before = session.ledger(source).stats;
        let sort_before = session.sort_outcome().clone();
        let use_case = DecidePatch::new(InMemoryPatchProjectStore::new());

        use_case
            .execute(&mut session, &id, decision(Action::Ignore))
            .expect("ignore is always a valid patch action");

        assert_eq!(
            session.ledger(source).stats,
            stats_before,
            "a patch decision must never touch the profile's own ledger stats"
        );
        assert_eq!(
            session.sort_outcome(),
            &sort_before,
            "a patch decision must never re-sort the profile"
        );
    }

    #[test]
    fn rejects_reorder_as_unpatchable() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = DecidePatch::new(InMemoryPatchProjectStore::new());

        let result = use_case.execute(
            &mut session,
            &id,
            decision(Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            }),
        );

        assert!(matches!(
            result,
            Err(DecidePatchError::Patch(PatchDecideError::Invalid(
                rim_resolve::domain::PatchDecisionError::NotPatchable(
                    rim_resolve::domain::UnpatchableAction::Reorder
                )
            )))
        ));
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let (mut session, _id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = DecidePatch::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, decision(Action::Ignore));

        assert!(matches!(
            result,
            Err(DecidePatchError::Patch(PatchDecideError::Unknown(_)))
        ));
    }

    #[test]
    fn a_failed_save_rolls_back_the_decision() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let use_case = DecidePatch::new(store);

        let result = use_case.execute(&mut session, &id, decision(Action::Ignore));

        assert!(matches!(result, Err(DecidePatchError::Store(_))));
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&wall_key())
                .is_none(),
            "the decision must be rolled back when the save fails"
        );
    }
}
