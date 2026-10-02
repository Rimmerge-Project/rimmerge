//! [`SkipRecommendedRules`]: remembers, per profile, that the user skipped
//! the Dashboard's "Get the recommended rules" step.

use std::path::Path;

use crate::ports::{ProfileNotificationStateStore, StoreError};

/// Records the skip in the profile's `notifications.json`. Touches no
/// `Session` state, so there is nothing to snapshot or roll back (the same
/// reason `SyncGameVersionAcknowledgement` has none).
pub struct SkipRecommendedRules<Store> {
    store: Store,
}

impl<Store: ProfileNotificationStateStore> SkipRecommendedRules<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Sets `recommended_rules_skipped_at` to `now`. Idempotent: an
    /// existing timestamp is kept, not bumped, and nothing is written.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails; nothing is skipped.
    pub fn execute(&self, profile_dir: &Path, now: jiff::Timestamp) -> Result<(), StoreError> {
        let mut state = self.store.load(profile_dir);
        if state.recommended_rules_skipped_at.is_some() {
            return Ok(());
        }
        state.recommended_rules_skipped_at = Some(now);
        self.store.save(profile_dir, &state)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ports::ProfileNotificationState;
    use crate::test_support::InMemoryProfileNotificationStateStore;

    fn at(seconds: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(seconds).expect("valid timestamp")
    }

    #[test]
    fn skip_sets_the_flag_once_and_keeps_the_first_timestamp() {
        let use_case = SkipRecommendedRules::new(InMemoryProfileNotificationStateStore::new());
        let profile_dir = PathBuf::from("profile");

        use_case.execute(&profile_dir, at(100)).expect("first skip");
        use_case
            .execute(&profile_dir, at(200))
            .expect("second skip");

        assert_eq!(
            use_case.store.save_count(),
            1,
            "a repeat skip writes nothing"
        );
        assert_eq!(
            use_case
                .store
                .load(&profile_dir)
                .recommended_rules_skipped_at,
            Some(at(100))
        );
    }

    #[test]
    fn skip_keeps_the_acknowledged_game_version() {
        let store = InMemoryProfileNotificationStateStore::new();
        store.seed(ProfileNotificationState {
            acknowledged_game_version: Some("1.6".to_string()),
            recommended_rules_skipped_at: None,
        });
        let use_case = SkipRecommendedRules::new(store);
        let profile_dir = PathBuf::from("profile");

        use_case.execute(&profile_dir, at(1)).expect("skip");

        assert_eq!(
            use_case
                .store
                .load(&profile_dir)
                .acknowledged_game_version
                .as_deref(),
            Some("1.6")
        );
    }

    #[test]
    fn a_failed_save_surfaces_and_skips_nothing() {
        let store = InMemoryProfileNotificationStateStore::new();
        store.fail_next_save();
        let use_case = SkipRecommendedRules::new(store);
        let profile_dir = PathBuf::from("profile");

        let result = use_case.execute(&profile_dir, at(1));

        assert!(result.is_err());
        assert_eq!(
            use_case
                .store
                .load(&profile_dir)
                .recommended_rules_skipped_at,
            None
        );
    }
}
