//! [`SetManualTag`]: sets (or replaces) a manual tag override, then saves
//! the rules file.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Tag, TagMode};

use crate::Session;
use crate::ports::{RuleStore, StoreError};

/// Sets (or replaces) a manual tag override, then saves the rules file
/// through [`RuleStore`].
pub struct SetManualTag<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> SetManualTag<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the tag
    /// change is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        mod_id: ModId,
        tag: Tag,
        mode: TagMode,
    ) -> Result<(), StoreError> {
        let snapshot = session.rules_snapshot();
        session.set_manual_tag(mod_id, tag, mode);
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
    use super::*;
    use crate::test_support::{InMemoryRuleStore, session_fixture};

    #[test]
    fn adds_a_manual_tag_and_persists_it() {
        let mut session = session_fixture(&["a"]);
        let use_case = SetManualTag::new(InMemoryRuleStore::new());
        let tag = Tag::new("framework").expect("valid tag");

        use_case
            .execute(&mut session, ModId::new("a"), tag.clone(), TagMode::Add)
            .expect("set manual tag must succeed");

        assert!(session.tagging().tags_of(&ModId::new("a")).contains(&tag));
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .manual_tags
                .len(),
            1
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_manual_tag() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = SetManualTag::new(store);
        let tag = Tag::new("framework").expect("valid tag");

        let result = use_case.execute(&mut session, ModId::new("a"), tag.clone(), TagMode::Add);

        assert!(result.is_err());
        assert!(
            !session.tagging().tags_of(&ModId::new("a")).contains(&tag),
            "the manual tag must be rolled back when the save fails"
        );
    }
}
