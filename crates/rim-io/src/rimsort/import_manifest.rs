//! [`JsonImportManifestStore`]: `rim_session::ports::ImportManifestStore`'s
//! real, file-backed implementation — `<profile>/imports/manifest.json`.
//! Mirrors the rule-database cache's manifest envelope shape and this crate's
//! existing `VersionProbe`-then-deserialize pattern
//! (`crate::databases::manifest`, `crate::rules`) rather than inventing a
//! second style. `ImportRecord` (`rim_session::ports`) has no
//! `Serialize`/`Deserialize` of its own — this module mirrors its shape by
//! hand into a private, serde-derived [`ImportRecordDto`], the same "domain
//! type stays serde-free, an adapter-local DTO carries the wire shape"
//! convention `rules.rs`'s `SettingsDto` already uses for
//! `rim_session::Settings`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rim_session::ports::{ImportManifestStore, ImportRecord, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "manifest.json";
const SCHEMA_VERSION: u32 = 1;

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full shape (mirrors
/// `crate::databases::manifest`'s own `VersionProbe`).
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// The on-disk mirror of [`ImportRecord`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ImportRecordDto {
    file: String,
    sha256: String,
    bytes: usize,
    imported_at: String,
}

impl From<&ImportRecord> for ImportRecordDto {
    fn from(value: &ImportRecord) -> Self {
        Self {
            file: value.file.clone(),
            sha256: value.sha256.clone(),
            bytes: value.bytes,
            imported_at: value.imported_at.clone(),
        }
    }
}

impl From<ImportRecordDto> for ImportRecord {
    fn from(value: ImportRecordDto) -> Self {
        Self {
            file: value.file,
            sha256: value.sha256,
            bytes: value.bytes,
            imported_at: value.imported_at,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManifestFile {
    version: u32,
    #[serde(default)]
    sources: BTreeMap<String, ImportRecordDto>,
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

/// Loads `<profile_dir>/imports/manifest.json`. Never fails: a missing
/// file, an unreadable one, malformed JSON, or an unrecognized `version`
/// all come back as an empty envelope — it records snapshot facts, not
/// data of its own, so losing it costs nothing but a stale-until-the-
/// next-import badge (the same reasoning
/// `crate::databases::manifest::Manifest::load` uses for the cache
/// manifest).
fn load_file(profile_dir: &Path) -> ManifestFile {
    let path = profile_dir.join("imports").join(FILE_NAME);
    let Ok(bytes) = fs::read(&path) else {
        return ManifestFile::default();
    };
    let Ok(probe) = serde_json::from_slice::<VersionProbe>(&bytes) else {
        return ManifestFile::default();
    };
    if probe.version != SCHEMA_VERSION {
        return ManifestFile::default();
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// `rim_session::ports::ImportManifestStore`'s real, file-backed
/// implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonImportManifestStore;

impl JsonImportManifestStore {
    /// Builds the store. Stateless — every call re-reads/writes the
    /// directory it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ImportManifestStore for JsonImportManifestStore {
    fn load(&self, profile_dir: &Path) -> BTreeMap<String, ImportRecord> {
        load_file(profile_dir)
            .sources
            .into_iter()
            .map(|(key, record)| (key, record.into()))
            .collect()
    }

    fn save(
        &self,
        profile_dir: &Path,
        records: &BTreeMap<String, ImportRecord>,
    ) -> Result<(), StoreError> {
        let mut file = load_file(profile_dir);
        for (key, record) in records {
            file.sources.insert(key.clone(), record.into());
        }
        file.version = SCHEMA_VERSION;

        let path = profile_dir.join("imports").join(FILE_NAME);
        // `?`, not a hardcoded-empty-envelope fallback: writing
        // `{"version":1,"sources":{}}"` on a serialization failure would
        // silently discard every record already on file
        // (`crates/rim-io/src/databases/manifest.rs`'s own `save` follows
        // the same rule).
        let json =
            serde_json::to_string_pretty(&file).map_err(|error| to_store_error(&path, error))?;
        write_atomically(&path, json.as_bytes()).map_err(|error| to_store_error(&path, error))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn record(sha256: &str) -> ImportRecord {
        ImportRecord {
            file: "userRules.json".to_string(),
            sha256: sha256.to_string(),
            bytes: 42,
            imported_at: "2026-09-09T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn a_missing_manifest_reports_no_record_for_any_source() {
        let dir = tempdir().expect("tempdir");
        let store = JsonImportManifestStore::new();

        assert!(store.load(dir.path()).is_empty());
    }

    #[test]
    fn a_truncated_manifest_is_treated_as_no_prior_import_not_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("imports")).expect("mkdir");
        fs::write(dir.path().join("imports").join(FILE_NAME), b"{not json").expect("write");
        let store = JsonImportManifestStore::new();

        assert!(store.load(dir.path()).is_empty());
    }

    #[test]
    fn an_unrecognized_version_is_treated_as_no_prior_import() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("imports")).expect("mkdir");
        fs::write(
            dir.path().join("imports").join(FILE_NAME),
            br#"{"version":99,"sources":{}}"#,
        )
        .expect("write");
        let store = JsonImportManifestStore::new();

        assert!(store.load(dir.path()).is_empty());
    }

    #[test]
    fn save_then_load_round_trips_a_record() {
        let dir = tempdir().expect("tempdir");
        let store = JsonImportManifestStore::new();

        store
            .save(
                dir.path(),
                &BTreeMap::from([("user_rules".to_string(), record("abc123"))]),
            )
            .expect("save");

        let loaded = store.load(dir.path());
        let saved = loaded.get("user_rules").expect("must be recorded");
        assert_eq!(saved.sha256, "abc123");
        assert_eq!(saved.bytes, 42);
        assert_eq!(saved.file, "userRules.json");
    }

    /// The whole point of this store's per-source semantics: saving one
    /// source must never disturb another source's own previous record — a
    /// source not part of *this* save simply isn't in `records`, and
    /// keeps whatever it had.
    #[test]
    fn saving_one_source_leaves_another_sources_earlier_record_untouched() {
        let dir = tempdir().expect("tempdir");
        let store = JsonImportManifestStore::new();
        store
            .save(
                dir.path(),
                &BTreeMap::from([("user_rules".to_string(), record("user-sha"))]),
            )
            .expect("save");

        store
            .save(
                dir.path(),
                &BTreeMap::from([("community_rules".to_string(), record("community-sha"))]),
            )
            .expect("save");

        let loaded = store.load(dir.path());
        assert_eq!(
            loaded.get("user_rules").expect("still recorded").sha256,
            "user-sha",
            "a later save of a different source must not clear this one's record"
        );
        assert_eq!(
            loaded.get("community_rules").expect("recorded").sha256,
            "community-sha"
        );
    }
}
