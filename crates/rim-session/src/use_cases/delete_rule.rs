//! [`DeleteRule`]: removes a rule, then saves the rules file.

use rim_resolve::domain::RuleOrigin;

use crate::ports::{RuleStore, StoreError};
use crate::{RuleKey, Session};

/// Removes the rule matching a [`RuleKey`], then saves the rules file
/// through [`RuleStore`].
pub struct DeleteRule<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> DeleteRule<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// Removes the rule at `key`, restricted to `origin` when given (every
    /// rule at `key` regardless of origin when `None` — see
    /// [`Session::delete_rule`]'s own doc comment on why a `Some` caller
    /// matters once a rules-page UI passes one: a promoted rule and its
    /// imported original share one [`RuleKey`]).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the
    /// deletion is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        key: &RuleKey,
        origin: Option<RuleOrigin>,
    ) -> Result<(), StoreError> {
        let snapshot = session.rules_snapshot();
        session.delete_rule(key, origin);
        if let Err(error) = self
            .rule_store
            .save(&session.paths().profile_dir, session.rules())
        {
            session.restore_rules(snapshot);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Placement, PlacementRule, Rule, RuleOrigin};

    use super::*;
    use crate::test_support::{InMemoryRuleStore, session_fixture};

    #[test]
    fn removes_the_matching_rule_and_persists_the_change() {
        let mut session = session_fixture(&["a"]);
        session.upsert_rule(Rule::Placement(PlacementRule {
            mod_id: ModId::new("a"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }));

        let use_case = DeleteRule::new(InMemoryRuleStore::new());
        use_case
            .execute(
                &mut session,
                &RuleKey::Placement {
                    mod_id: ModId::new("a"),
                },
                None,
            )
            .expect("delete must succeed");

        assert!(session.rules().placements.is_empty());
        assert!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .placements
                .is_empty()
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_deletion() {
        let mut session = session_fixture(&["a"]);
        session.upsert_rule(Rule::Placement(PlacementRule {
            mod_id: ModId::new("a"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }));

        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let result = DeleteRule::new(store).execute(
            &mut session,
            &RuleKey::Placement {
                mod_id: ModId::new("a"),
            },
            None,
        );

        assert!(result.is_err());
        assert_eq!(
            session.rules().placements.len(),
            1,
            "the deleted rule must be restored when the save fails"
        );
    }

    /// Regression test: after promoting an
    /// imported pair rule, deleting the imported original by its own
    /// origin must leave the promoted `UserDecision` copy in place — not
    /// wipe both rows sharing the one [`RuleKey`].
    #[test]
    fn deleting_by_origin_leaves_the_promoted_copy_in_place() {
        use rim_resolve::domain::PairRule;

        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
            overrides_declared: false,
        }));
        let key = RuleKey::Pair {
            after: ModId::new("a"),
            before: ModId::new("b"),
        };
        assert!(
            session.promote_imported_rule(&key),
            "promotion must succeed"
        );
        assert_eq!(
            session.rules().pairs.len(),
            2,
            "the imported rule and its promoted copy must both be listed"
        );

        let use_case = DeleteRule::new(InMemoryRuleStore::new());
        use_case
            .execute(&mut session, &key, Some(RuleOrigin::RimSortCommunity))
            .expect("delete must succeed");

        let remaining = &session.rules().pairs;
        assert_eq!(
            remaining.len(),
            1,
            "only the RimSortCommunity-origin row must be removed: {remaining:?}"
        );
        assert_eq!(remaining[0].origin, RuleOrigin::UserDecision);
    }
}
