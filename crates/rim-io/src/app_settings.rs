//! [`JsonAppSettingsStore`]: `<base>/app-settings.json`, the app-global
//! network and reminder preferences — a sibling of `config.json`,
//! `profiles/`, and `databases/`.
//!
//! **Fails closed, deliberately deviating from this crate's usual
//! "unknown version is a hard error" rule and from `config.json`'s own
//! "broken file loads as default"**: this file's default means *network
//! on*, and a corrupted privacy preference must never silently turn
//! networking back on. A missing file loads as [`AppSettings::default`]
//! (safe, because nothing automatic runs before the first-run notice is
//! answered); a present-but-broken file loads with every network switch
//! off — see [`AppSettingsLoad::Recovered`]'s own doc comment. This is
//! one of the two exceptions `crates/rim-io/CLAUDE.md` names (the other
//! is `notifications.rs`, for the opposite reason — see that module's
//! own doc comment).

use std::path::{Path, PathBuf};

use rim_session::app_settings::{AppSettings, NetworkPolicy, ReminderPolicy, StaleAfterDays};
use rim_session::ports::{AppSettingsLoad, AppSettingsStore, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::{write_atomically, write_if_absent};

const FILE_NAME: &str = "app-settings.json";
const SCHEMA_VERSION: u32 = 1;

/// `<base>/app-settings.json`.
#[must_use]
pub fn app_settings_path(base: &Path) -> PathBuf {
    base.join(FILE_NAME)
}

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full shape.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    schema: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkPolicyDto {
    allow_network: bool,
    check_for_updates: bool,
    auto_refresh_rule_databases: bool,
    fetch_community_rules: bool,
    fetch_steam_workshop: bool,
    fetch_rimmerge_rules: bool,
}

impl From<NetworkPolicy> for NetworkPolicyDto {
    fn from(policy: NetworkPolicy) -> Self {
        Self {
            allow_network: policy.allow_network,
            check_for_updates: policy.check_for_updates,
            auto_refresh_rule_databases: policy.auto_refresh_rule_databases,
            fetch_community_rules: policy.fetch_community_rules,
            fetch_steam_workshop: policy.fetch_steam_workshop,
            fetch_rimmerge_rules: policy.fetch_rimmerge_rules,
        }
    }
}

impl From<NetworkPolicyDto> for NetworkPolicy {
    fn from(dto: NetworkPolicyDto) -> Self {
        Self {
            allow_network: dto.allow_network,
            check_for_updates: dto.check_for_updates,
            auto_refresh_rule_databases: dto.auto_refresh_rule_databases,
            fetch_community_rules: dto.fetch_community_rules,
            fetch_steam_workshop: dto.fetch_steam_workshop,
            fetch_rimmerge_rules: dto.fetch_rimmerge_rules,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ReminderPolicyDto {
    rule_databases_stale_after_days: u16,
}

impl From<ReminderPolicy> for ReminderPolicyDto {
    fn from(policy: ReminderPolicy) -> Self {
        Self {
            rule_databases_stale_after_days: policy.rule_databases_stale_after_days.get(),
        }
    }
}

/// The on-disk envelope. Deliberately **not** `deny_unknown_fields`:
/// `extra` captures every key this binary doesn't recognize (a newer
/// version's own addition) and writes it straight back on the next save
/// — forward compatibility, the same convention `mod_knowledge.rs`
/// follows for its own envelope.
#[derive(Debug, Serialize, Deserialize)]
struct AppSettingsFile {
    schema: u32,
    network: NetworkPolicyDto,
    reminders: ReminderPolicyDto,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// Reads and writes `<base>/app-settings.json`. Stateless.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonAppSettingsStore;

impl JsonAppSettingsStore {
    /// Builds the store. Stateless — every call reads/writes `base`
    /// fresh.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl AppSettingsStore for JsonAppSettingsStore {
    fn load(&self, base: &Path) -> AppSettingsLoad {
        let path = app_settings_path(base);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return AppSettingsLoad::Missing;
            }
            Err(error) => {
                return AppSettingsLoad::Recovered {
                    reason: format!("reading {}: {error}", path.display()),
                };
            }
        };

        let probe: VersionProbe = match serde_json::from_slice(&bytes) {
            Ok(probe) => probe,
            Err(error) => {
                return AppSettingsLoad::Recovered {
                    reason: format!("parsing {}: {error}", path.display()),
                };
            }
        };
        if probe.schema != SCHEMA_VERSION {
            return AppSettingsLoad::Recovered {
                reason: format!(
                    "{} has schema {}, this build only understands {SCHEMA_VERSION}",
                    path.display(),
                    probe.schema
                ),
            };
        }

        let file: AppSettingsFile = match serde_json::from_slice(&bytes) {
            Ok(file) => file,
            Err(error) => {
                return AppSettingsLoad::Recovered {
                    reason: format!("parsing {}: {error}", path.display()),
                };
            }
        };
        let stale_after_days =
            match StaleAfterDays::new(file.reminders.rule_databases_stale_after_days) {
                Ok(value) => value,
                Err(error) => {
                    return AppSettingsLoad::Recovered {
                        reason: format!("{}: {error}", path.display()),
                    };
                }
            };

        AppSettingsLoad::Loaded(AppSettings {
            network: file.network.into(),
            reminders: ReminderPolicy {
                rule_databases_stale_after_days: stale_after_days,
            },
        })
    }

    fn save(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
        let path = app_settings_path(base);
        // Unknown keys from an existing file are preserved across a save
        // — the same forward-compatibility convention `mod_knowledge.rs`
        // uses for its own envelope: a newer build's addition survives a
        // round trip through an older one.
        let extra = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<AppSettingsFile>(&bytes).ok())
            .map(|file| file.extra)
            .unwrap_or_default();
        let bytes = render(&path, settings, extra)?;
        write_atomically(&path, &bytes)
            .map_err(|error| StoreError(format!("writing {}: {error}", path.display())))
    }

    fn save_if_missing(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
        let path = app_settings_path(base);
        let bytes = render(&path, settings, serde_json::Map::new())?;
        // `Created` and `AlreadyExisted` are both success: the second means
        // another writer's choice is on disk, and it stands.
        write_if_absent(&path, &bytes)
            .map(|_outcome| ())
            .map_err(|error| StoreError(format!("writing {}: {error}", path.display())))
    }
}

/// The file's bytes for `settings`, carrying `extra` (unknown keys a newer
/// build wrote) through unchanged.
fn render(
    path: &Path,
    settings: &AppSettings,
    extra: serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, StoreError> {
    let file = AppSettingsFile {
        schema: SCHEMA_VERSION,
        network: settings.network.into(),
        reminders: settings.reminders.into(),
        extra,
    };
    serde_json::to_vec_pretty(&file)
        .map_err(|error| StoreError(format!("serializing {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn a_missing_file_loads_as_default() {
        let dir = tempdir().expect("tempdir");
        let loaded = JsonAppSettingsStore::new().load(dir.path());
        assert_eq!(loaded, AppSettingsLoad::Missing);
        assert_eq!(loaded.settings(), AppSettings::default());
    }

    #[test]
    fn a_corrupt_file_loads_with_every_network_switch_off() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(app_settings_path(dir.path()), b"not json at all").expect("write");

        let loaded = JsonAppSettingsStore::new().load(dir.path());

        assert!(matches!(loaded, AppSettingsLoad::Recovered { .. }));
        assert!(!loaded.settings().network.allow_network);
    }

    #[test]
    fn an_unknown_schema_loads_with_network_off() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"schema":99,"network":{},"reminders":{}}"#,
        )
        .expect("write");

        let loaded = JsonAppSettingsStore::new().load(dir.path());

        assert!(matches!(loaded, AppSettingsLoad::Recovered { .. }));
        assert!(!loaded.settings().network.allow_network);
    }

    #[test]
    fn save_then_load_round_trips_custom_values() {
        let dir = tempdir().expect("tempdir");
        let store = JsonAppSettingsStore::new();
        let mut settings = AppSettings::default();
        settings.network.allow_network = false;
        settings.network.fetch_steam_workshop = true;
        settings.reminders.rule_databases_stale_after_days = StaleAfterDays::new(7).unwrap();

        store.save(dir.path(), &settings).expect("save");
        let loaded = store.load(dir.path());

        assert_eq!(loaded, AppSettingsLoad::Loaded(settings));
    }

    #[test]
    fn save_if_missing_creates_the_file_when_there_is_none() {
        let dir = tempdir().expect("tempdir");
        let store = JsonAppSettingsStore::new();

        store
            .save_if_missing(dir.path(), &AppSettings::default())
            .expect("create");

        assert_eq!(
            store.load(dir.path()),
            AppSettingsLoad::Loaded(AppSettings::default())
        );
    }

    #[test]
    fn save_if_missing_never_overwrites_a_file_that_is_already_there() {
        let dir = tempdir().expect("tempdir");
        let store = JsonAppSettingsStore::new();
        let mut chosen = AppSettings::default();
        chosen.network.allow_network = false;
        store.save(dir.path(), &chosen).expect("seed");

        store
            .save_if_missing(dir.path(), &AppSettings::default())
            .expect("an existing file is success");

        assert_eq!(store.load(dir.path()), AppSettingsLoad::Loaded(chosen));
    }

    #[test]
    fn save_if_missing_leaves_a_corrupt_file_for_the_user_to_repair() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(app_settings_path(dir.path()), b"not json at all").expect("seed");

        JsonAppSettingsStore::new()
            .save_if_missing(dir.path(), &AppSettings::default())
            .expect("an existing file is success");

        assert_eq!(
            std::fs::read(app_settings_path(dir.path())).expect("read"),
            b"not json at all"
        );
    }

    #[test]
    fn racing_save_if_missing_calls_leave_exactly_one_complete_file() {
        let dir = tempdir().expect("tempdir");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (1..=8u16)
            .map(|days| {
                let base = dir.path().to_path_buf();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut settings = AppSettings::default();
                    settings.reminders.rule_databases_stale_after_days =
                        StaleAfterDays::new(days).expect("in range");
                    barrier.wait();
                    JsonAppSettingsStore::new().save_if_missing(&base, &settings)
                })
            })
            .collect();

        for handle in handles {
            handle
                .join()
                .expect("thread")
                .expect("every racer succeeds");
        }

        let AppSettingsLoad::Loaded(winner) = JsonAppSettingsStore::new().load(dir.path()) else {
            panic!("the winner's file must be complete and valid");
        };
        assert!(winner.reminders.rule_databases_stale_after_days.get() <= 8);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"schema":1,"network":{"allow_network":true,"check_for_updates":true,"auto_refresh_rule_databases":true,"fetch_community_rules":true,"fetch_steam_workshop":false,"fetch_rimmerge_rules":true},"reminders":{"rule_databases_stale_after_days":30},"a_future_field":42}"#,
        )
        .expect("write");
        let store = JsonAppSettingsStore::new();
        let loaded = store.load(dir.path());
        let AppSettingsLoad::Loaded(settings) = loaded else {
            panic!("expected Loaded");
        };

        store.save(dir.path(), &settings).expect("save");

        let raw = std::fs::read_to_string(app_settings_path(dir.path())).expect("read back");
        assert!(raw.contains("a_future_field"));
    }
}
