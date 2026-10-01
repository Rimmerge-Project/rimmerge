//! [`UpsertRule`]: adds or replaces a rule, then saves the rules file.

use rim_resolve::domain::Rule;

use crate::Session;
use crate::ports::{RuleStore, StoreError};

/// Adds or replaces a rule, then saves the rules file through
/// [`RuleStore`].
pub struct UpsertRule<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> UpsertRule<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the
    /// upsert is rolled back so the session never runs ahead of disk.
    pub fn execute(&self, session: &mut Session, rule: Rule) -> Result<(), StoreError> {
        let snapshot = session.rules_snapshot();
        session.upsert_rule(rule);
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
    use rim_resolve::domain::{Placement, PlacementRule, RuleOrigin};

    use super::*;
    use crate::test_support::{InMemoryRuleStore, session_fixture};

    #[test]
    fn adds_the_rule_and_persists_it() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryRuleStore::new();
        let use_case = UpsertRule::new(store);

        use_case
            .execute(
                &mut session,
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new("a"),
                    placement: Placement::Top,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                }),
            )
            .expect("upsert must succeed");

        assert_eq!(session.rules().placements.len(), 1);
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .placements
                .len(),
            1
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_upsert() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = UpsertRule::new(store);

        let result = use_case.execute(
            &mut session,
            Rule::Placement(PlacementRule {
                mod_id: ModId::new("a"),
                placement: Placement::Top,
                origin: RuleOrigin::UserDecision,
                comment: None,
            }),
        );

        assert!(result.is_err());
        assert!(
            session.rules().placements.is_empty(),
            "the rule must be rolled back in memory when the save fails"
        );
    }
}
