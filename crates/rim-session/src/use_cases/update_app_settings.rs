//! [`UpdateAppSettings`]: replaces the app-global `AppSettings`, saved
//! through [`AppSettingsStore`]. [`ResetNetworkPolicy`]: restores just
//! the network half to its defaults, leaving `reminders` untouched —
//! the "Restore network defaults" action, kept separate from a plain
//! `UpdateAppSettings` call so the UI never has to know
//! [`NetworkPolicy::default`] itself.

use std::path::Path;

use crate::app_settings::{AppSettings, NetworkPolicy};
use crate::ports::{AppSettingsLoad, AppSettingsStore, StoreError};

/// Replaces `<base>/app-settings.json` wholesale.
pub struct UpdateAppSettings<Store> {
    store: Store,
}

impl<Store: AppSettingsStore> UpdateAppSettings<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, base: &Path, settings: AppSettings) -> Result<(), StoreError> {
        self.store.save(base, &settings)
    }
}

/// Restores `network` to [`NetworkPolicy::default`] — every switch back
/// on — while leaving `reminders` exactly as it was.
pub struct ResetNetworkPolicy<Store> {
    store: Store,
}

impl<Store: AppSettingsStore> ResetNetworkPolicy<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, base: &Path) -> Result<AppSettings, StoreError> {
        let current = self.store.load(base).settings();
        let reset = AppSettings {
            network: NetworkPolicy::default(),
            reminders: current.reminders,
        };
        self.store.save(base, &reset)?;
        Ok(reset)
    }
}

/// Turns on every recommended rule-database source and returns the saved
/// settings. Never touches `allow_network` (see
/// [`NetworkPolicy::with_recommended_sources`]) and never overwrites a
/// damaged settings file: that is repaired explicitly, from Settings.
/// Downloading is a separate step ([`crate::use_cases::RefreshRuleDatabases`]);
/// the caller's click is the consent for both.
pub struct EnableRecommendedSources<Store> {
    store: Store,
}

impl<Store: AppSettingsStore> EnableRecommendedSources<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the settings file is damaged (it is
    /// left as it is) or the write fails.
    pub fn execute(&self, base: &Path) -> Result<AppSettings, StoreError> {
        let current = match self.store.load(base) {
            AppSettingsLoad::Loaded(settings) => settings,
            AppSettingsLoad::Missing => AppSettings::default(),
            AppSettingsLoad::Recovered { reason } => {
                return Err(StoreError(format!(
                    "app-settings.json is damaged and was left unchanged: {reason}"
                )));
            }
        };
        let updated = AppSettings {
            network: current.network.with_recommended_sources(),
            reminders: current.reminders,
        };
        self.store.save(base, &updated)?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;
    use crate::app_settings::{ReminderPolicy, StaleAfterDays};

    #[derive(Default)]
    struct InMemoryAppSettingsStore {
        saved: RefCell<Option<AppSettings>>,
        is_recovered: bool,
    }

    impl AppSettingsStore for InMemoryAppSettingsStore {
        fn load(&self, _base: &Path) -> AppSettingsLoad {
            if self.is_recovered {
                return AppSettingsLoad::Recovered {
                    reason: "corrupt".to_string(),
                };
            }
            self.saved
                .borrow()
                .map_or(AppSettingsLoad::Missing, AppSettingsLoad::Loaded)
        }

        fn save(&self, _base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
            *self.saved.borrow_mut() = Some(*settings);
            Ok(())
        }

        fn save_if_missing(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
            if self.saved.borrow().is_some() {
                return Ok(());
            }
            self.save(base, settings)
        }
    }

    #[test]
    fn update_app_settings_saves_whatever_it_is_given() {
        let store = InMemoryAppSettingsStore::default();
        let use_case = UpdateAppSettings::new(store);
        let mut settings = AppSettings::default();
        settings.network.allow_network = false;

        use_case
            .execute(&PathBuf::from("base"), settings)
            .expect("save must succeed");

        assert_eq!(
            use_case.store.load(&PathBuf::from("base")).settings(),
            settings
        );
    }

    #[test]
    fn reset_network_policy_restores_network_but_never_touches_reminders() {
        let store = InMemoryAppSettingsStore::default();
        store
            .save(
                &PathBuf::from("base"),
                &AppSettings {
                    network: NetworkPolicy {
                        allow_network: false,
                        ..NetworkPolicy::default()
                    },
                    reminders: ReminderPolicy {
                        rule_databases_stale_after_days: StaleAfterDays::new(7).unwrap(),
                    },
                },
            )
            .expect("seed");
        let use_case = ResetNetworkPolicy::new(store);

        let result = use_case
            .execute(&PathBuf::from("base"))
            .expect("reset must succeed");

        assert_eq!(result.network, NetworkPolicy::default());
        assert_eq!(
            result.reminders.rule_databases_stale_after_days.get(),
            7,
            "reminders must never be touched by a network-only reset"
        );
    }

    #[test]
    fn enable_recommended_sources_turns_sources_on_but_never_internet_access() {
        let store = InMemoryAppSettingsStore::default();
        let mut seeded = AppSettings::default();
        seeded.network.allow_network = false;
        seeded.network.fetch_steam_workshop = false;
        seeded.network.fetch_community_rules = false;
        store.save(&PathBuf::from("base"), &seeded).expect("seed");
        let use_case = EnableRecommendedSources::new(store);

        let result = use_case
            .execute(&PathBuf::from("base"))
            .expect("must succeed");

        assert!(result.network.fetch_steam_workshop);
        assert!(result.network.fetch_community_rules);
        assert!(
            !result.network.allow_network,
            "internet access is not touched"
        );
        assert_eq!(
            use_case.store.load(&PathBuf::from("base")).settings(),
            result,
            "the result is what was saved"
        );
    }

    #[test]
    fn enable_recommended_sources_pins_defaults_when_the_file_is_missing() {
        let use_case = EnableRecommendedSources::new(InMemoryAppSettingsStore::default());

        let result = use_case
            .execute(&PathBuf::from("base"))
            .expect("must succeed");

        assert_eq!(result, AppSettings::default());
    }

    #[test]
    fn enable_recommended_sources_refuses_to_overwrite_a_damaged_file() {
        let store = InMemoryAppSettingsStore {
            is_recovered: true,
            ..InMemoryAppSettingsStore::default()
        };
        let use_case = EnableRecommendedSources::new(store);

        let result = use_case.execute(&PathBuf::from("base"));

        assert!(result.is_err());
        assert_eq!(*use_case.store.saved.borrow(), None, "nothing was written");
    }
}
