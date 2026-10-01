//! [`PromoteImportedRule`]: "promote to user rule"
//! — copies an imported pair or
//! placement rule to a `UserDecision`-owned one, then saves the rules
//! file.

use crate::ports::{RuleStore, StoreError};
use crate::{RuleKey, Session};

/// Promotes the imported pair or placement rule named by a [`RuleKey`] to
/// a `UserDecision`-owned copy, then saves the rules file through
/// [`RuleStore`].
pub struct PromoteImportedRule<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> PromoteImportedRule<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// Returns whether a promoted copy was actually added — see
    /// [`Session::promote_imported_rule`] for the exact no-op conditions
    /// (no matching imported rule, an existing promoted copy, or an
    /// [`RuleKey::Incompatible`] key). A no-op never touches disk.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the
    /// promotion is rolled back so the session never runs ahead of disk.
    pub fn execute(&self, session: &mut Session, key: &RuleKey) -> Result<bool, StoreError> {
        let snapshot = session.rules_snapshot();
        if !session.promote_imported_rule(key) {
            return Ok(false);
        }
        if let Err(error) = self
            .rule_store
            .save(&session.paths().profile_dir, session.rules())
        {
            session.restore_rules(snapshot);
            return Err(error);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Placement, PlacementRule, Rule, RuleOrigin};

    use super::*;
    use crate::test_support::{InMemoryRuleStore, session_fixture};

    fn imported_pair(after: &str, before: &str) -> Rule {
        Rule::Pair(rim_resolve::domain::PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin: RuleOrigin::RimSortCommunity,
            comment: Some("imported".to_string()),
            overrides_declared: false,
        })
    }

    fn imported_placement(mod_id: &str) -> Rule {
        Rule::Placement(PlacementRule {
            mod_id: ModId::new(mod_id),
            placement: Placement::Bottom,
            origin: RuleOrigin::RimSortCommunity,
            comment: Some("imported".to_string()),
        })
    }

    #[test]
    fn promotes_an_imported_pair_and_persists_both_copies() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let use_case = PromoteImportedRule::new(InMemoryRuleStore::new());

        let promoted = use_case
            .execute(
                &mut session,
                &RuleKey::Pair {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
            )
            .expect("promote must succeed");

        assert!(promoted);
        assert_eq!(
            session.rules().pairs.len(),
            2,
            "the import stays alongside the promoted copy"
        );
        assert!(
            session
                .rules()
                .pairs
                .iter()
                .any(|r| r.origin == RuleOrigin::UserDecision)
        );
        assert!(
            session
                .rules()
                .pairs
                .iter()
                .any(|r| r.origin == RuleOrigin::RimSortCommunity),
            "the imported original must still be listed as its source"
        );
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .pairs
                .len(),
            2
        );
    }

    #[test]
    fn promotes_an_imported_placement() {
        let mut session = session_fixture(&["a"]);
        session.upsert_rule(imported_placement("a"));
        let use_case = PromoteImportedRule::new(InMemoryRuleStore::new());

        let promoted = use_case
            .execute(
                &mut session,
                &RuleKey::Placement {
                    mod_id: ModId::new("a"),
                },
            )
            .expect("promote must succeed");

        assert!(promoted);
        assert_eq!(session.rules().placements.len(), 2);
    }

    #[test]
    fn promoting_a_key_with_no_imported_rule_is_a_no_op() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = PromoteImportedRule::new(InMemoryRuleStore::new());

        let promoted = use_case
            .execute(
                &mut session,
                &RuleKey::Pair {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
            )
            .expect("a no-op promote still returns Ok");

        assert!(!promoted);
        assert!(session.rules().pairs.is_empty());
        assert!(
            use_case.rule_store.last_saved().is_none(),
            "a no-op promote must never write to disk"
        );
    }

    #[test]
    fn promoting_an_already_promoted_pair_is_idempotent() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let use_case = PromoteImportedRule::new(InMemoryRuleStore::new());
        let key = RuleKey::Pair {
            after: ModId::new("a"),
            before: ModId::new("b"),
        };
        use_case
            .execute(&mut session, &key)
            .expect("first promote must succeed");

        let promoted_again = use_case
            .execute(&mut session, &key)
            .expect("a repeat promote still returns Ok");

        assert!(!promoted_again);
        assert_eq!(
            session.rules().pairs.len(),
            2,
            "a repeat promote must not add a second user-owned copy"
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_promotion() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = PromoteImportedRule::new(store);

        let result = use_case.execute(
            &mut session,
            &RuleKey::Pair {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
        );

        assert!(result.is_err());
        assert_eq!(
            session.rules().pairs.len(),
            1,
            "the promoted copy must be rolled back when the save fails"
        );
    }

    #[test]
    fn promoting_an_incompatible_key_is_a_no_op() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = PromoteImportedRule::new(InMemoryRuleStore::new());

        let promoted = use_case
            .execute(
                &mut session,
                &RuleKey::Incompatible {
                    a: ModId::new("a"),
                    b: ModId::new("b"),
                },
            )
            .expect("a no-op promote still returns Ok");

        assert!(!promoted);
    }
}
