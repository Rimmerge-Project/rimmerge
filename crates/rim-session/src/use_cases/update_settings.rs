//! [`UpdateSettings`]: replaces the session's settings, then saves the
//! rules file (settings ride along with rules — see
//! [`crate::ports::StoredRules`]). [`ResetSettings`]: the "Use
//! recommended settings" action from the `Welcome` notice — a thin wrapper over
//! [`UpdateSettings`] with [`crate::Settings::default`], kept as its own
//! use case for the identical reason `ResetNetworkPolicy` is its own use
//! case beside `UpdateAppSettings`: the interface layer has no business
//! knowing what "recommended" means.

use crate::Session;
use crate::Settings;
use crate::ports::{RuleStore, StoreError};

/// Replaces the session's settings, then saves the rules file through
/// [`RuleStore`].
pub struct UpdateSettings<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> UpdateSettings<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the
    /// settings change is rolled back so the session never runs ahead of
    /// disk.
    pub fn execute(&self, session: &mut Session, settings: Settings) -> Result<(), StoreError> {
        let snapshot = session.rules_snapshot();
        session.update_settings(settings);
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

/// Resets the session's settings to [`crate::Settings::default`] — "Use
/// recommended settings" on the `Welcome` notice.
pub struct ResetSettings<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> ResetSettings<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case the
    /// settings change is rolled back — the identical shape
    /// [`UpdateSettings::execute`] follows (kept independent rather than
    /// composed, so this use case's own port bound stays exactly
    /// `RuleStore`, the same as every other single-port use case in this
    /// crate).
    pub fn execute(&self, session: &mut Session) -> Result<(), StoreError> {
        let snapshot = session.rules_snapshot();
        session.update_settings(Settings::default());
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
    use rim_resolve::domain::Confidence;

    use super::*;
    use crate::test_support::{InMemoryRuleStore, session_fixture};

    #[test]
    fn replaces_settings_and_persists_them() {
        let mut session = session_fixture(&["a"]);
        let use_case = UpdateSettings::new(InMemoryRuleStore::new());
        let settings = Settings {
            threshold: Confidence::new(50).expect("valid confidence"),
            enforce: rim_resolve::sort::EnforcedLayers {
                soft: true,
                awareness: false,
                inferred: true,
            },
            suggest_merge_when_clean: true,
            ..Settings::default()
        };

        use_case
            .execute(&mut session, settings)
            .expect("update settings must succeed");

        assert_eq!(session.settings().threshold.percent(), 50);
        assert!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .settings
                .enforce
                .soft
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_settings() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = UpdateSettings::new(store);
        let original = session.settings().threshold.percent();

        let result = use_case.execute(
            &mut session,
            Settings {
                threshold: Confidence::new(50).expect("valid confidence"),
                enforce: rim_resolve::sort::EnforcedLayers::default(),
                suggest_merge_when_clean: true,
                ..Settings::default()
            },
        );

        assert!(result.is_err());
        assert_eq!(
            session.settings().threshold.percent(),
            original,
            "settings must be rolled back when the save fails"
        );
    }

    #[test]
    fn reset_settings_restores_the_default_and_persists_it() {
        let mut session = session_fixture(&["a"]);
        UpdateSettings::new(InMemoryRuleStore::new())
            .execute(
                &mut session,
                Settings {
                    threshold: Confidence::new(50).expect("valid confidence"),
                    ..Settings::default()
                },
            )
            .expect("seed a non-default settings value");
        assert_ne!(session.settings(), Settings::default());

        let store = InMemoryRuleStore::new();
        let use_case = ResetSettings::new(store);

        use_case.execute(&mut session).expect("reset must succeed");

        assert_eq!(session.settings(), Settings::default());
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .settings,
            Settings::default()
        );
    }

    #[test]
    fn a_failed_reset_save_rolls_back_the_settings() {
        let mut session = session_fixture(&["a"]);
        UpdateSettings::new(InMemoryRuleStore::new())
            .execute(
                &mut session,
                Settings {
                    threshold: Confidence::new(50).expect("valid confidence"),
                    ..Settings::default()
                },
            )
            .expect("seed a non-default settings value");
        let original = session.settings();

        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = ResetSettings::new(store);

        let result = use_case.execute(&mut session);

        assert!(result.is_err());
        assert_eq!(session.settings(), original);
    }
}
