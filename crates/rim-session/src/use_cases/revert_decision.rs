//! [`RevertDecision`]: removes a user's decision on a finding, persisting
//! the change through [`DecisionStore`] before returning.

use rim_resolve::domain::{Decision, FindingKey};

use crate::Session;
use crate::ports::{DecisionStore, StoreError};

/// Removes a decision, persisting the change through [`DecisionStore`]
/// before returning.
pub struct RevertDecision<Decisions> {
    decision_store: Decisions,
}

impl<Decisions: DecisionStore> RevertDecision<Decisions> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(decision_store: Decisions) -> Self {
        Self { decision_store }
    }

    /// Returns the removed decision, if any.
    ///
    /// A key with no recorded decision is a no-op: nothing is persisted,
    /// so reverting an unknown key never rewrites the decisions file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when persisting fails, in which case the
    /// removal is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        key: &FindingKey,
    ) -> Result<Option<Decision>, StoreError> {
        let snapshot = session.decisions_snapshot();
        let removed = session.revert_decision(key);
        if removed.is_none() {
            return Ok(None);
        }
        if let Err(error) = self
            .decision_store
            .save(&session.paths().profile_dir, session.decisions())
        {
            session.restore_decisions(snapshot);
            return Err(error);
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::Action;

    use super::*;
    use crate::test_support::{InMemoryDecisionStore, session_fixture};
    use crate::use_cases::Decide;

    #[test]
    fn removes_a_previously_recorded_decision() {
        let mut session = session_fixture(&["a"]);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };
        Decide::new(InMemoryDecisionStore::new())
            .execute(
                &mut session,
                Decision {
                    key: key.clone(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        let store = InMemoryDecisionStore::new();
        let removed = RevertDecision::new(store)
            .execute(&mut session, &key)
            .expect("revert must succeed");

        assert!(removed.is_some());
        assert!(session.decisions().get(&key).is_none());
    }

    #[test]
    fn reverting_an_absent_decision_is_a_no_op() {
        let mut session = session_fixture(&["a"]);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };

        let removed = RevertDecision::new(InMemoryDecisionStore::new())
            .execute(&mut session, &key)
            .expect("revert must succeed even with nothing to remove");

        assert!(removed.is_none());
    }

    #[test]
    fn reverting_an_absent_decision_never_saves() {
        let mut session = session_fixture(&["a"]);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };
        let use_case = RevertDecision::new(InMemoryDecisionStore::new());

        use_case
            .execute(&mut session, &key)
            .expect("revert must succeed even with nothing to remove");

        assert!(
            use_case.decision_store.last_saved().is_none(),
            "reverting an unknown key must not touch the decision store"
        );
    }

    #[test]
    fn reverting_a_recorded_decision_saves_the_updated_set() {
        let mut session = session_fixture(&["a"]);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };
        Decide::new(InMemoryDecisionStore::new())
            .execute(
                &mut session,
                Decision {
                    key: key.clone(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        let use_case = RevertDecision::new(InMemoryDecisionStore::new());
        use_case
            .execute(&mut session, &key)
            .expect("revert must succeed");

        assert!(
            use_case
                .decision_store
                .last_saved()
                .is_some_and(|set| set.get(&key).is_none()),
            "reverting a known key must persist the updated decision set"
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_removal() {
        let mut session = session_fixture(&["a"]);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };
        Decide::new(InMemoryDecisionStore::new())
            .execute(
                &mut session,
                Decision {
                    key: key.clone(),
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        let store = InMemoryDecisionStore::new();
        store.fail_next_save();
        let result = RevertDecision::new(store).execute(&mut session, &key);

        assert!(result.is_err());
        assert_eq!(
            session.decisions().get(&key).map(|d| &d.action),
            Some(&Action::Ignore),
            "the removed decision must be restored when the save fails"
        );
    }
}
