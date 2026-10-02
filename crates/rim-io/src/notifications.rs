//! [`JsonNotificationStateStore`]: `<base>/notifications.json` — a
//! sibling of `app-settings.json`, `config.json`, `profiles/`, and
//! `databases/`.
//!
//! **Loaded leniently, deliberately deviating from this crate's own
//! "unknown version is a hard error" rule** — the second such exception
//! (`app_settings.rs`'s own module doc names the first, network policy,
//! for the opposite reason: that file's default means *network on* and
//! must fail closed; this one's default is simply "nothing has happened
//! yet", so losing it costs at most re-showing already-seen notices —
//! see `rim_session::ports::NotificationStateStore::load`'s own doc
//! comment for why that's safe even for the first-run notice).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rim_session::notifications::{AppVersion, LatestRelease, NotificationKind};
use rim_session::ports::{
    NotificationState, NotificationStateStore, ProfileNotificationState,
    ProfileNotificationStateStore, StoreError, UpdateCheckFailure, UpdateCheckState,
    UpdateCheckSuccess,
};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "notifications.json";
const SCHEMA_VERSION: u32 = 1;

/// `<base>/notifications.json`.
#[must_use]
pub fn notifications_path(base: &Path) -> PathBuf {
    base.join(FILE_NAME)
}

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full shape.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    schema: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateCheckSuccessDto {
    checked_at: String,
    /// `AppVersion`'s own `Display`, reparsed with
    /// [`AppVersion::published`] on load — never a bare semver crate
    /// dependency duplicated here.
    latest_version: String,
    latest_published_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateCheckFailureDto {
    checked_at: String,
    reason: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateCheckStateDto {
    #[serde(default)]
    last_attempt_at: Option<String>,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    retry_not_before: Option<String>,
    #[serde(default)]
    last_success: Option<UpdateCheckSuccessDto>,
    #[serde(default)]
    last_failure: Option<UpdateCheckFailureDto>,
}

/// The on-disk envelope. `dismissed`/`muted` are flat string arrays —
/// `"<kind>:<fingerprint>"` and `"<kind>"` respectively, not the
/// per-kind map [`NotificationState`] itself keeps in memory; splitting
/// and grouping happens at the boundary here, once. Deliberately **not**
/// `deny_unknown_fields`: `extra` captures every key this binary doesn't
/// recognize and writes it straight back on the next save — the same
/// forward-compatibility convention `mod_knowledge.rs`/`app_settings.rs`
/// follow.
#[derive(Debug, Default, Serialize, Deserialize)]
struct NotificationStateFile {
    schema: u32,
    #[serde(default)]
    welcome_completed_at: Option<String>,
    #[serde(default)]
    dismissed: Vec<String>,
    #[serde(default)]
    muted: Vec<String>,
    #[serde(default)]
    update_check: UpdateCheckStateDto,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// [`NotificationKind`]'s stable on-disk spelling — deliberately not
/// `Debug`'s output, for the same reason `databases/manifest.rs`'s own
/// `key_for` isn't: a future `Debug` wording change must never silently
/// change what's already stored (which would re-show every dismissed/
/// muted notice at once).
fn kind_key(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::Welcome => "welcome",
        NotificationKind::GameVersionChanged => "game_version_changed",
        NotificationKind::UpdateAvailable => "update_available",
        NotificationKind::RuleDatabasesStale => "rule_databases_stale",
        NotificationKind::ImportedRulesOutdated => "imported_rules_outdated",
        NotificationKind::RecommendedSourcesIncomplete => "recommended_sources_incomplete",
    }
}

/// The reverse of [`kind_key`] — `None` for anything this build doesn't
/// recognize, which a lenient load simply drops (a newer version's own
/// kind is not something an older binary could show anyway).
fn parse_kind_key(raw: &str) -> Option<NotificationKind> {
    match raw {
        "welcome" => Some(NotificationKind::Welcome),
        "game_version_changed" => Some(NotificationKind::GameVersionChanged),
        "update_available" => Some(NotificationKind::UpdateAvailable),
        "rule_databases_stale" => Some(NotificationKind::RuleDatabasesStale),
        "imported_rules_outdated" => Some(NotificationKind::ImportedRulesOutdated),
        "recommended_sources_incomplete" => Some(NotificationKind::RecommendedSourcesIncomplete),
        _ => None,
    }
}

fn parse_timestamp(raw: &Option<String>) -> Option<jiff::Timestamp> {
    raw.as_deref()?.parse().ok()
}

fn dto_to_state(file: NotificationStateFile) -> NotificationState {
    let mut dismissed: BTreeMap<NotificationKind, Vec<String>> = BTreeMap::new();
    for entry in file.dismissed {
        let Some((kind_raw, fingerprint)) = entry.split_once(':') else {
            continue;
        };
        let Some(kind) = parse_kind_key(kind_raw) else {
            continue;
        };
        dismissed
            .entry(kind)
            .or_default()
            .push(fingerprint.to_string());
    }
    // Defensive, in case a hand-edited or older-version file exceeds the
    // cap — this crate's own writer never produces more than this, but a
    // load must still be safe against one that does. The file lists each
    // kind oldest first, so the overflow dropped is the oldest: keeping
    // the first entries instead would re-show the newest dismissals.
    for fingerprints in dismissed.values_mut() {
        let overflow = fingerprints
            .len()
            .saturating_sub(NotificationState::MAX_DISMISSED_PER_KIND);
        fingerprints.drain(..overflow);
    }

    let muted = file
        .muted
        .iter()
        .filter_map(|raw| parse_kind_key(raw))
        .collect();

    let last_success = file.update_check.last_success.and_then(|dto| {
        let version = AppVersion::published(&dto.latest_version).ok()?;
        Some(UpdateCheckSuccess {
            checked_at: dto.checked_at.parse().ok()?,
            latest: LatestRelease {
                version,
                published_at: dto.latest_published_at.parse().ok()?,
            },
        })
    });
    let last_failure = file.update_check.last_failure.and_then(|dto| {
        Some(UpdateCheckFailure {
            checked_at: dto.checked_at.parse().ok()?,
            reason: dto.reason,
        })
    });

    NotificationState {
        welcome_completed_at: parse_timestamp(&file.welcome_completed_at),
        dismissed,
        muted,
        update_check: UpdateCheckState {
            last_attempt_at: parse_timestamp(&file.update_check.last_attempt_at),
            etag: file.update_check.etag,
            retry_not_before: parse_timestamp(&file.update_check.retry_not_before),
            last_success,
            last_failure,
        },
    }
}

fn state_to_dto(state: &NotificationState) -> NotificationStateFile {
    let dismissed = state
        .dismissed
        .iter()
        .flat_map(|(kind, fingerprints)| {
            fingerprints
                .iter()
                .map(move |fingerprint| format!("{}:{fingerprint}", kind_key(*kind)))
        })
        .collect();
    let muted = state
        .muted
        .iter()
        .map(|kind| kind_key(*kind).to_string())
        .collect();

    NotificationStateFile {
        schema: SCHEMA_VERSION,
        welcome_completed_at: state.welcome_completed_at.map(|at| at.to_string()),
        dismissed,
        muted,
        update_check: UpdateCheckStateDto {
            last_attempt_at: state.update_check.last_attempt_at.map(|at| at.to_string()),
            etag: state.update_check.etag.clone(),
            retry_not_before: state.update_check.retry_not_before.map(|at| at.to_string()),
            last_success: state.update_check.last_success.as_ref().map(|success| {
                UpdateCheckSuccessDto {
                    checked_at: success.checked_at.to_string(),
                    latest_version: success.latest.version.to_string(),
                    latest_published_at: success.latest.published_at.to_string(),
                }
            }),
            last_failure: state.update_check.last_failure.as_ref().map(|failure| {
                UpdateCheckFailureDto {
                    checked_at: failure.checked_at.to_string(),
                    reason: failure.reason.clone(),
                }
            }),
        },
        extra: serde_json::Map::new(),
    }
}

/// Reads and writes `<base>/notifications.json`. Stateless.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonNotificationStateStore;

impl JsonNotificationStateStore {
    /// Builds the store. Stateless — every call reads/writes `base`
    /// fresh.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl NotificationStateStore for JsonNotificationStateStore {
    fn load(&self, base: &Path) -> NotificationState {
        let path = notifications_path(base);
        let Ok(bytes) = std::fs::read(&path) else {
            return NotificationState::default();
        };
        let Ok(probe) = serde_json::from_slice::<VersionProbe>(&bytes) else {
            return NotificationState::default();
        };
        if probe.schema != SCHEMA_VERSION {
            return NotificationState::default();
        }
        let Ok(file) = serde_json::from_slice::<NotificationStateFile>(&bytes) else {
            return NotificationState::default();
        };
        dto_to_state(file)
    }

    fn save(&self, base: &Path, state: &NotificationState) -> Result<(), StoreError> {
        let path = notifications_path(base);
        // Unknown keys from an existing file are preserved across a save
        // — the same forward-compatibility convention `app_settings.rs`/
        // `mod_knowledge.rs` both use: a newer build's own addition
        // survives a round trip through an older one. `state_to_dto`
        // always writes an empty `extra`, since `NotificationState`
        // itself carries no such field — this is the one place that gap
        // is closed, by re-reading the file's own current extras rather
        // than threading them through the domain type.
        let extra = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<NotificationStateFile>(&bytes).ok())
            .map(|file| file.extra)
            .unwrap_or_default();
        let mut file = state_to_dto(state);
        file.extra = extra;
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| StoreError(format!("serializing {}: {error}", path.display())))?;
        write_atomically(&path, &bytes)
            .map_err(|error| StoreError(format!("writing {}: {error}", path.display())))
    }
}

/// Just enough of the per-profile envelope to check the schema version
/// before attempting to deserialize the full shape — mirrors
/// [`VersionProbe`], kept separate since the two files share no other
/// shape.
#[derive(Debug, Deserialize)]
struct ProfileVersionProbe {
    schema: u32,
}

/// The per-profile on-disk envelope — `<profile>/notifications.json`.
/// Deliberately **not** `deny_unknown_fields`, for the identical
/// forward-compatibility reason [`NotificationStateFile`] isn't.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileNotificationStateFile {
    schema: u32,
    #[serde(default)]
    acknowledged_game_version: Option<String>,
    /// RFC 3339 timestamp of the user skipping the Dashboard's "Get the
    /// recommended rules" step for this profile. Lives in the
    /// notifications file rather than a new one: it is app-remembered
    /// guide state like `acknowledged_game_version`, loads leniently, and
    /// one timestamp is not worth a new port and adapter. Additive:
    /// `#[serde(default)]`, so older files load as not skipped and older
    /// binaries keep the key through `extra`.
    #[serde(default)]
    recommended_rules_skipped_at: Option<String>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// Reads and writes `<profile>/notifications.json` — see
/// [`rim_session::ports::ProfileNotificationStateStore`]'s own doc
/// comment for why this is a separate file from the app-global
/// [`JsonNotificationStateStore`]. Stateless, and loaded **leniently**
/// for the identical reason its app-global sibling is.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonProfileNotificationStateStore;

impl JsonProfileNotificationStateStore {
    /// Builds the store. Stateless — every call reads/writes
    /// `profile_dir` fresh.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ProfileNotificationStateStore for JsonProfileNotificationStateStore {
    fn load(&self, profile_dir: &Path) -> ProfileNotificationState {
        let path = notifications_path(profile_dir);
        let Ok(bytes) = std::fs::read(&path) else {
            return ProfileNotificationState::default();
        };
        let Ok(probe) = serde_json::from_slice::<ProfileVersionProbe>(&bytes) else {
            return ProfileNotificationState::default();
        };
        if probe.schema != SCHEMA_VERSION {
            return ProfileNotificationState::default();
        }
        let Ok(file) = serde_json::from_slice::<ProfileNotificationStateFile>(&bytes) else {
            return ProfileNotificationState::default();
        };
        ProfileNotificationState {
            acknowledged_game_version: file.acknowledged_game_version,
            recommended_rules_skipped_at: parse_timestamp(&file.recommended_rules_skipped_at),
        }
    }

    fn save(&self, profile_dir: &Path, state: &ProfileNotificationState) -> Result<(), StoreError> {
        let path = notifications_path(profile_dir);
        // Preserve unknown keys across a save, the same
        // forward-compatibility convention `JsonNotificationStateStore::save`
        // follows.
        let extra = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ProfileNotificationStateFile>(&bytes).ok())
            .map(|file| file.extra)
            .unwrap_or_default();
        let file = ProfileNotificationStateFile {
            schema: SCHEMA_VERSION,
            acknowledged_game_version: state.acknowledged_game_version.clone(),
            recommended_rules_skipped_at: state
                .recommended_rules_skipped_at
                .map(|at| at.to_string()),
            extra,
        };
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| StoreError(format!("serializing {}: {error}", path.display())))?;
        write_atomically(&path, &bytes)
            .map_err(|error| StoreError(format!("writing {}: {error}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn a_missing_file_loads_as_default() {
        let dir = tempdir().expect("tempdir");
        let store = JsonNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), NotificationState::default());
    }

    #[test]
    fn a_corrupt_file_loads_as_default_never_errors() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(notifications_path(dir.path()), b"not json at all").expect("write");
        let store = JsonNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), NotificationState::default());
    }

    #[test]
    fn an_unknown_schema_version_loads_as_default() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 999, "welcome_completed_at": "2026-01-01T00:00:00Z"}"#,
        )
        .expect("write");
        let store = JsonNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), NotificationState::default());
    }

    #[test]
    fn saving_then_loading_round_trips_every_field() {
        let dir = tempdir().expect("tempdir");
        let store = JsonNotificationStateStore::new();
        let now = jiff::Timestamp::UNIX_EPOCH;
        let state = NotificationState {
            welcome_completed_at: Some(now),
            dismissed: BTreeMap::from([
                (
                    NotificationKind::UpdateAvailable,
                    vec!["0.1.0".to_string(), "0.2.0".to_string()],
                ),
                (
                    NotificationKind::RuleDatabasesStale,
                    vec!["community_rules@2026-08-01T00:00:00Z".to_string()],
                ),
            ]),
            muted: BTreeSet::from([NotificationKind::ImportedRulesOutdated]),
            update_check: UpdateCheckState {
                last_attempt_at: Some(now),
                etag: Some("W/\"abc\"".to_string()),
                retry_not_before: None,
                last_success: Some(UpdateCheckSuccess {
                    checked_at: now,
                    latest: LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: now,
                    },
                }),
                last_failure: Some(UpdateCheckFailure {
                    checked_at: now,
                    reason: "rate limited".to_string(),
                }),
            },
        };

        store.save(dir.path(), &state).expect("save must succeed");
        let loaded = store.load(dir.path());

        assert_eq!(loaded, state);
    }

    #[test]
    fn an_over_long_dismissed_list_keeps_the_newest_entries_on_load() {
        let dir = tempdir().expect("tempdir");
        let total = NotificationState::MAX_DISMISSED_PER_KIND + 6;
        let entries: Vec<String> = (0..total)
            .map(|index| format!("\"rule_databases_stale:fp{index}\""))
            .collect();
        std::fs::write(
            notifications_path(dir.path()),
            format!(r#"{{"schema": 1, "dismissed": [{}]}}"#, entries.join(",")),
        )
        .expect("seed file");

        let state = JsonNotificationStateStore::new().load(dir.path());

        let kept = &state.dismissed[&NotificationKind::RuleDatabasesStale];
        assert_eq!(kept.len(), NotificationState::MAX_DISMISSED_PER_KIND);
        assert_eq!(kept.first().map(String::as_str), Some("fp6"));
        assert_eq!(kept.last(), Some(&format!("fp{}", total - 1)));
    }

    #[test]
    fn a_dismissed_entry_naming_an_unknown_kind_is_dropped_silently() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 1, "dismissed": ["some_future_kind:abc", "welcome:welcome"]}"#,
        )
        .expect("write");
        let store = JsonNotificationStateStore::new();

        let state = store.load(dir.path());

        assert_eq!(state.dismissed.len(), 1);
        assert!(state.dismissed.contains_key(&NotificationKind::Welcome));
    }

    #[test]
    fn an_unknown_top_level_key_is_preserved_across_a_resave() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 1, "future_field": "kept"}"#,
        )
        .expect("write");
        let store = JsonNotificationStateStore::new();

        let state = store.load(dir.path());
        store.save(dir.path(), &state).expect("resave must succeed");

        let raw = std::fs::read_to_string(notifications_path(dir.path())).expect("read back");
        assert!(
            raw.contains("future_field"),
            "an unknown field must survive a load/save round trip: {raw}"
        );
    }

    #[test]
    fn a_dismissed_fingerprint_containing_a_colon_survives_the_round_trip() {
        // Every RFC 3339 timestamp contains ':', so a `RuleDatabasesStale`
        // fingerprint (`source@<timestamp>`) must split only on the
        // *first* ':'.
        let dir = tempdir().expect("tempdir");
        let store = JsonNotificationStateStore::new();
        let state = NotificationState {
            dismissed: BTreeMap::from([(
                NotificationKind::RuleDatabasesStale,
                vec!["community_rules@2026-08-01T00:00:00Z".to_string()],
            )]),
            ..NotificationState::default()
        };

        store.save(dir.path(), &state).expect("save must succeed");
        let loaded = store.load(dir.path());

        assert_eq!(
            loaded.dismissed[&NotificationKind::RuleDatabasesStale],
            vec!["community_rules@2026-08-01T00:00:00Z".to_string()]
        );
    }

    #[test]
    fn profile_store_a_missing_file_loads_as_default() {
        let dir = tempdir().expect("tempdir");
        let store = JsonProfileNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), ProfileNotificationState::default());
    }

    #[test]
    fn profile_store_a_corrupt_file_loads_as_default_never_errors() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(notifications_path(dir.path()), b"not json at all").expect("write");
        let store = JsonProfileNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), ProfileNotificationState::default());
    }

    #[test]
    fn profile_store_an_unknown_schema_version_loads_as_default() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 999, "acknowledged_game_version": "1.6"}"#,
        )
        .expect("write");
        let store = JsonProfileNotificationStateStore::new();

        assert_eq!(store.load(dir.path()), ProfileNotificationState::default());
    }

    #[test]
    fn profile_store_saving_then_loading_round_trips() {
        let dir = tempdir().expect("tempdir");
        let store = JsonProfileNotificationStateStore::new();
        let state = ProfileNotificationState {
            acknowledged_game_version: Some("1.7".to_string()),
            recommended_rules_skipped_at: None,
        };

        store.save(dir.path(), &state).expect("save must succeed");

        assert_eq!(store.load(dir.path()), state);
    }

    #[test]
    fn profile_store_an_unknown_top_level_key_is_preserved_across_a_resave() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 1, "future_field": "kept"}"#,
        )
        .expect("write");
        let store = JsonProfileNotificationStateStore::new();

        let state = store.load(dir.path());
        store.save(dir.path(), &state).expect("resave must succeed");

        let raw = std::fs::read_to_string(notifications_path(dir.path())).expect("read back");
        assert!(
            raw.contains("future_field"),
            "an unknown field must survive a load/save round trip: {raw}"
        );
    }

    #[test]
    fn profile_notifications_round_trip_the_skip_timestamp() {
        let dir = tempdir().expect("tempdir");
        let store = JsonProfileNotificationStateStore::new();
        let state = ProfileNotificationState {
            acknowledged_game_version: None,
            recommended_rules_skipped_at: Some(
                "2026-10-01T12:30:00Z".parse().expect("valid timestamp"),
            ),
        };

        store.save(dir.path(), &state).expect("save must succeed");

        assert_eq!(store.load(dir.path()), state);
    }

    #[test]
    fn an_older_profile_file_without_the_field_loads_not_skipped() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 1, "acknowledged_game_version": "1.6"}"#,
        )
        .expect("write");

        let state = JsonProfileNotificationStateStore::new().load(dir.path());

        assert_eq!(state.recommended_rules_skipped_at, None);
        assert_eq!(state.acknowledged_game_version.as_deref(), Some("1.6"));
    }

    #[test]
    fn an_unknown_key_survives_a_save_that_sets_the_skip() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            notifications_path(dir.path()),
            br#"{"schema": 1, "future_field": "kept"}"#,
        )
        .expect("write");
        let store = JsonProfileNotificationStateStore::new();
        let mut state = store.load(dir.path());
        state.recommended_rules_skipped_at = Some(jiff::Timestamp::UNIX_EPOCH);

        store.save(dir.path(), &state).expect("save must succeed");

        let raw = std::fs::read_to_string(notifications_path(dir.path())).expect("read back");
        assert!(raw.contains("future_field"), "unknown key lost: {raw}");
        assert_eq!(store.load(dir.path()), state);
    }
}
