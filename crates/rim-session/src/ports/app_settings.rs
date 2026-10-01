//! [`AppSettingsStore`]: the port for `<base>/app-settings.json`.
//! Implemented by `rim_io::app_settings::JsonAppSettingsStore`.

use std::path::Path;

use crate::app_settings::AppSettings;
use crate::ports::StoreError;

/// The outcome of loading `<base>/app-settings.json`. Distinct from a
/// plain `AppSettings`, because *how* the load turned out changes what
/// the UI shows — see each variant's own doc comment and
/// [`Self::settings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppSettingsLoad {
    /// The file existed, parsed, and its `schema` was recognized.
    Loaded(AppSettings),
    /// No file exists yet — safe to treat as [`AppSettings::default`]:
    /// nothing automatic runs before the first-run notice is answered
    /// regardless (`crate::use_cases::RunLaunchNetworkChecks`), so a
    /// fresh machine defaulting to "network on" never actually contacts
    /// anything before the user has had the chance to say no.
    Missing,
    /// The file exists but couldn't be read, parsed, or carried an
    /// unknown `schema`. **Fails closed, deliberately deviating from
    /// `config.json`'s "broken file loads as default"**: the default
    /// here means *network on*, and a corrupted privacy preference must
    /// never silently turn networking back on. `network` loads with
    /// every switch off; `reminders` loads as its own default. `reason`
    /// is a short, human-readable, already-bounded explanation, safe to
    /// show verbatim.
    Recovered {
        /// Why the file couldn't be used.
        reason: String,
    },
}

impl AppSettingsLoad {
    /// The effective [`AppSettings`] this load result should be treated
    /// as — [`AppSettings::default`] for [`Self::Missing`], every
    /// network switch off (reminders default) for
    /// [`Self::Recovered`].
    #[must_use]
    pub fn settings(&self) -> AppSettings {
        match self {
            Self::Loaded(settings) => *settings,
            Self::Missing => AppSettings::default(),
            Self::Recovered { .. } => AppSettings {
                network: crate::app_settings::NetworkPolicy {
                    allow_network: false,
                    check_for_updates: false,
                    auto_refresh_rule_databases: false,
                    fetch_community_rules: false,
                    fetch_steam_workshop: false,
                    fetch_rimmerge_rules: false,
                },
                reminders: crate::app_settings::ReminderPolicy::default(),
            },
        }
    }
}

/// Reads and writes `<base>/app-settings.json`, the app-global network
/// and reminder preferences. Implemented by
/// `rim_io::app_settings::JsonAppSettingsStore`; a fake in tests.
pub trait AppSettingsStore {
    /// Loads the file under `base`. Never fails outright — every failure
    /// mode is a variant of [`AppSettingsLoad`] instead, since a broken
    /// preferences file must degrade to a safe, usable state rather than
    /// stopping the app from starting.
    fn load(&self, base: &Path) -> AppSettingsLoad;

    /// Writes `settings` to `base`, atomically, preserving any unknown
    /// keys the file already had (forward compatibility with a newer
    /// version that added a field this binary doesn't know about).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError>;

    /// Writes `settings` to `base` only when no `app-settings.json` exists
    /// there yet, as **one atomic create-if-absent** rather than a `load`
    /// followed by a `save`: two writers racing to create the file (two
    /// app instances answering the first-run notice, or the notice and a
    /// Settings save) must never have the later one replace the earlier
    /// one's choice. A file that already exists — valid or corrupt, whoever
    /// wrote it, however a moment ago — is left exactly as it is and the
    /// call succeeds: losing the race means the other writer's choice
    /// stands, which is all the caller wanted to guarantee.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file does not exist and cannot be
    /// created.
    fn save_if_missing(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_default_to_network_on() {
        assert_eq!(AppSettingsLoad::Missing.settings(), AppSettings::default());
    }

    #[test]
    fn recovered_settings_fail_closed_with_every_network_switch_off() {
        let settings = AppSettingsLoad::Recovered {
            reason: "corrupt".to_string(),
        }
        .settings();

        assert!(!settings.network.allow_network);
        assert!(!settings.network.check_for_updates);
        assert!(!settings.network.auto_refresh_rule_databases);
        assert!(!settings.network.fetch_community_rules);
        assert!(!settings.network.fetch_steam_workshop);
        assert!(!settings.network.fetch_rimmerge_rules);
        assert_eq!(
            settings.reminders,
            crate::app_settings::ReminderPolicy::default()
        );
    }

    #[test]
    fn loaded_settings_pass_through_unchanged() {
        let mut custom = AppSettings::default();
        custom.network.allow_network = false;
        assert_eq!(AppSettingsLoad::Loaded(custom).settings(), custom);
    }
}
