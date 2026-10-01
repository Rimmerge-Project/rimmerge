//! `manifest.json`: the versioned envelope over each rule database's cache
//! facts — the same
//! `VersionProbe`-then-deserialize pattern [`crate::rules::JsonRuleStore`]
//! uses. An unreadable, truncated, or unrecognized-version manifest is
//! treated as "never fetched", never a hard error: it is a cache, so
//! losing it costs a re-download, never data.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rim_session::ports::{CachedDatabase, FetchFailure, FetchFailureCause, RuleDatabase};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "manifest.json";
const SCHEMA_VERSION: u32 = 1;

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full shape (mirrors
/// [`crate::rules::JsonRuleStore`]'s own `VersionProbe`).
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// One source's persisted record. Every field but `url`/`file` is
/// `Option` because a source can be recorded with no successful fetch at
/// all yet — a refresh's very first attempt failing must still be able to
/// record `last_failure` — so "has this ever succeeded" is "`sha256`,
/// `bytes`, and `fetched_at` are all present", checked once in
/// [`Manifest::cached`] rather than trusted field-by-field at every call
/// site.
///
/// `url` is stored for a human reading the file by hand; nothing in this
/// crate reads it back — the URL an actual refresh uses always comes from
/// the hardcoded `DatabaseSpec` constant, never from this file.
/// The persisted form of a refresh failure's cause: only the causes a cache
/// refresh can produce. A record with no cause (written before causes were
/// kept) reads back as [`FetchFailureCause::Unclassified`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FailureCauseRecord {
    Transport,
    HttpStatus { status: u16 },
    TooLarge,
    InvalidContent,
    NotModifiedWithoutCache,
    ReadFailed,
    CacheWriteFailed,
}

impl FailureCauseRecord {
    /// `None` for a cause no cache refresh produces (a rate limit, an
    /// unpublished release, ...): it is persisted as "no cause".
    fn of(cause: FetchFailureCause) -> Option<Self> {
        match cause {
            FetchFailureCause::Transport => Some(Self::Transport),
            FetchFailureCause::HttpStatus { status } => Some(Self::HttpStatus { status }),
            FetchFailureCause::TooLarge => Some(Self::TooLarge),
            FetchFailureCause::InvalidContent => Some(Self::InvalidContent),
            FetchFailureCause::NotModifiedWithoutCache => Some(Self::NotModifiedWithoutCache),
            FetchFailureCause::ReadFailed => Some(Self::ReadFailed),
            FetchFailureCause::CacheWriteFailed => Some(Self::CacheWriteFailed),
            FetchFailureCause::RateLimited { .. }
            | FetchFailureCause::NotPublished
            | FetchFailureCause::NotAStableVersion
            | FetchFailureCause::Unclassified => None,
        }
    }

    fn cause(self) -> FetchFailureCause {
        match self {
            Self::Transport => FetchFailureCause::Transport,
            Self::HttpStatus { status } => FetchFailureCause::HttpStatus { status },
            Self::TooLarge => FetchFailureCause::TooLarge,
            Self::InvalidContent => FetchFailureCause::InvalidContent,
            Self::NotModifiedWithoutCache => FetchFailureCause::NotModifiedWithoutCache,
            Self::ReadFailed => FetchFailureCause::ReadFailed,
            Self::CacheWriteFailed => FetchFailureCause::CacheWriteFailed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SourceRecord {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    bytes: Option<usize>,
    /// RFC 3339 UTC, `jiff::Timestamp`'s own `Display`/`FromStr` — jiff
    /// has no `serde` feature enabled in this workspace, so every other
    /// timestamp field in this crate (`assignment_store.rs`,
    /// `patches.rs`, ...) round-trips through a plain `String` the same
    /// way.
    #[serde(default)]
    fetched_at: Option<String>,
    #[serde(default)]
    last_failure: Option<String>,
    /// The closed cause behind `last_failure`'s text. `#[serde(default)]`
    /// addition: an older manifest loads with `None` and its failure reads
    /// back as unclassified until the next attempt. A cause written by a
    /// newer binary that this one does not know also reads as `None`: a
    /// downgrade must not discard the sha/etag/fetched_at beside it.
    #[serde(default, deserialize_with = "lenient_failure_cause")]
    last_failure_cause: Option<FailureCauseRecord>,
    /// When this source was last *attempted* (fetched, confirmed
    /// unchanged, or failed) — unlike `fetched_at`, which only advances
    /// on success. `#[serde(default)]` addition: an older manifest still
    /// loads, just with `None` here until the next attempt. See
    /// [`Manifest::last_attempt_at`] for why this exists.
    #[serde(default)]
    last_attempt_at: Option<String>,
}

/// Reads a [`FailureCauseRecord`], turning an unknown variant into `None`
/// instead of failing the whole manifest load.
fn lenient_failure_cause<'de, D>(deserializer: D) -> Result<Option<FailureCauseRecord>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|raw| serde_json::from_value(raw).ok()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManifestFile {
    version: u32,
    #[serde(default)]
    sources: BTreeMap<String, SourceRecord>,
}

/// The manifest's own stable key for `database` — deliberately not
/// `RuleDatabase`'s `Debug` output, so a future `Debug` wording change
/// can never silently change what's already on disk.
fn key_for(database: RuleDatabase) -> &'static str {
    match database {
        RuleDatabase::CommunityRules => "community_rules",
        RuleDatabase::SteamWorkshop => "steam_workshop",
        RuleDatabase::RimmergeRules => "rimmerge_rules",
    }
}

/// Everything [`Manifest::record_updated`] needs to record one successful
/// fetch — grouped into a struct rather than seven positional parameters
/// (clippy's `too_many_arguments`).
#[derive(Debug, Clone)]
pub(crate) struct FetchSuccess<'a> {
    pub(crate) url: &'a str,
    pub(crate) file_name: &'a str,
    pub(crate) sha256: String,
    pub(crate) etag: Option<String>,
    pub(crate) bytes: usize,
    pub(crate) fetched_at: jiff::Timestamp,
}

/// The cache manifest for one `cache_dir`, loaded once and mutated in
/// memory; [`Manifest::save`] writes the whole file back atomically.
#[derive(Debug, Default)]
pub(crate) struct Manifest {
    file: ManifestFile,
}

impl Manifest {
    /// Loads `<cache_dir>/manifest.json`. Never fails: a missing file, an
    /// unreadable one, malformed JSON, or an unrecognized `version` all
    /// come back as an empty manifest — see this module's own doc
    /// comment for why that's correct for a cache.
    pub(crate) fn load(cache_dir: &Path) -> Self {
        let path = cache_dir.join(FILE_NAME);
        let Ok(bytes) = fs::read(&path) else {
            return Self::default();
        };
        let Ok(probe) = serde_json::from_slice::<VersionProbe>(&bytes) else {
            return Self::default();
        };
        if probe.version != SCHEMA_VERSION {
            return Self::default();
        }
        let file = serde_json::from_slice(&bytes).unwrap_or_default();
        Self { file }
    }

    fn record(&self, database: RuleDatabase) -> Option<&SourceRecord> {
        self.file.sources.get(key_for(database))
    }

    fn record_mut(&mut self, database: RuleDatabase) -> &mut SourceRecord {
        self.file
            .sources
            .entry(key_for(database).to_string())
            .or_default()
    }

    /// The etag to send as `If-None-Match` on the next fetch, if a
    /// previous fetch recorded one.
    pub(crate) fn etag(&self, database: RuleDatabase) -> Option<&str> {
        self.record(database)?.etag.as_deref()
    }

    /// The sha256 of the bytes already on disk, if any — used to collapse
    /// an identical body with no matching etag into
    /// [`rim_session::ports::RefreshOutcome::Unchanged`] instead of
    /// rewriting a byte-identical file.
    pub(crate) fn sha256(&self, database: RuleDatabase) -> Option<&str> {
        self.record(database)?.sha256.as_deref()
    }

    /// The last recorded failure, if the most recent attempt failed
    /// (cleared on the next success).
    pub(crate) fn last_failure(&self, database: RuleDatabase) -> Option<FetchFailure> {
        let record = self.record(database)?;
        let detail = record.last_failure.clone()?;
        Some(FetchFailure {
            cause: record
                .last_failure_cause
                .map_or(FetchFailureCause::Unclassified, FailureCauseRecord::cause),
            detail,
        })
    }

    /// When this source's last attempt (success, unchanged, or failure)
    /// happened — `None` when there has never been one. Unlike
    /// [`Self::cached`]'s `fetched_at`, this advances on a failed
    /// attempt too, which is exactly what
    /// `RefreshRuleDatabases::execute_automatic`'s once-a-day cadence
    /// needs: without it, an offline machine would retry at every
    /// launch instead of once a day. A malformed timestamp string
    /// parses as `None` rather than panicking or erroring — the same
    /// "a cache never hard-fails" rule this whole module follows.
    pub(crate) fn last_attempt_at(&self, database: RuleDatabase) -> Option<jiff::Timestamp> {
        self.record(database)?
            .last_attempt_at
            .as_deref()?
            .parse()
            .ok()
    }

    /// The last successful fetch, if there has ever been one — `None`
    /// unless `sha256`/`bytes`/`fetched_at` are all present (see this
    /// module's own doc comment on [`SourceRecord`]).
    pub(crate) fn cached(&self, database: RuleDatabase) -> Option<CachedDatabase> {
        let record = self.record(database)?;
        Some(CachedDatabase {
            sha256: record.sha256.clone()?,
            bytes: record.bytes?,
            fetched_at: record.fetched_at.as_deref()?.parse().ok()?,
        })
    }

    /// Records a freshly-downloaded, freshly-validated body: new
    /// `sha256`/`etag`/`bytes`/`fetched_at`, and `last_failure` cleared
    /// (a success always supersedes whatever the previous attempt was).
    pub(crate) fn record_updated(&mut self, database: RuleDatabase, success: &FetchSuccess) {
        let record = self.record_mut(database);
        record.url = Some(success.url.to_string());
        record.file = Some(success.file_name.to_string());
        record.sha256 = Some(success.sha256.clone());
        record.etag = success.etag.clone();
        record.bytes = Some(success.bytes);
        record.fetched_at = Some(success.fetched_at.to_string());
        record.last_failure = None;
        record.last_failure_cause = None;
        record.last_attempt_at = Some(success.fetched_at.to_string());
    }

    /// Records a `304`/identical-body confirmation: only `fetched_at`
    /// advances (the response carried no new body), and any
    /// previous `last_failure` is cleared (this attempt succeeded).
    pub(crate) fn record_unchanged(&mut self, database: RuleDatabase, fetched_at: jiff::Timestamp) {
        let record = self.record_mut(database);
        record.fetched_at = Some(fetched_at.to_string());
        record.last_failure = None;
        record.last_failure_cause = None;
        record.last_attempt_at = Some(fetched_at.to_string());
    }

    /// Records a failed attempt. Deliberately touches only
    /// `last_failure` — `sha256`/`etag`/`bytes`/`fetched_at` are left
    /// exactly as they were (a failure never disturbs
    /// a good cached copy, including the manifest's own record of it),
    /// even when there was no previous success at all (a brand-new
    /// record with only `last_failure` set — [`Manifest::cached`] still
    /// correctly reports `None` for it).
    pub(crate) fn record_failure(
        &mut self,
        database: RuleDatabase,
        failure: &FetchFailure,
        attempted_at: jiff::Timestamp,
    ) {
        let record = self.record_mut(database);
        record.last_failure = Some(failure.detail.clone());
        record.last_failure_cause = FailureCauseRecord::of(failure.cause);
        record.last_attempt_at = Some(attempted_at.to_string());
    }

    /// Writes the manifest back atomically. Always writes
    /// [`SCHEMA_VERSION`], regardless of what was loaded.
    ///
    /// # Errors
    ///
    /// Returns an error when `self.file` fails to serialize, or when the
    /// atomic write itself fails. Neither case writes anything: falling
    /// back to a hardcoded empty `{"version":1,"sources":{}}"` on a
    /// serialization failure would silently discard every record already
    /// on file (`crates/rim-io/src/rimsort/import_manifest.rs`'s own `save`
    /// follows the same rule).
    pub(crate) fn save(&self, cache_dir: &Path) -> std::io::Result<()> {
        let path = cache_dir.join(FILE_NAME);
        let file = ManifestFile {
            version: SCHEMA_VERSION,
            sources: self.file.sources.clone(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        write_atomically(&path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn a_missing_manifest_reports_never_fetched_for_every_source() {
        let dir = tempdir().expect("tempdir");
        let manifest = Manifest::load(dir.path());

        assert!(manifest.cached(RuleDatabase::CommunityRules).is_none());
        assert!(manifest.cached(RuleDatabase::SteamWorkshop).is_none());
        assert!(manifest.etag(RuleDatabase::CommunityRules).is_none());
        assert!(
            manifest
                .last_failure(RuleDatabase::CommunityRules)
                .is_none()
        );
    }

    #[test]
    fn a_truncated_manifest_is_treated_as_never_fetched_not_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME), b"{not json").expect("write");

        let manifest = Manifest::load(dir.path());

        assert!(manifest.cached(RuleDatabase::CommunityRules).is_none());
    }

    #[test]
    fn an_unrecognized_manifest_version_is_treated_as_never_fetched() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join(FILE_NAME),
            br#"{"version":99,"sources":{}}"#,
        )
        .expect("write");

        let manifest = Manifest::load(dir.path());

        assert!(manifest.cached(RuleDatabase::CommunityRules).is_none());
    }

    #[test]
    fn record_updated_round_trips_through_save_and_load() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());
        let now = jiff::Timestamp::UNIX_EPOCH;

        manifest.record_updated(
            RuleDatabase::CommunityRules,
            &FetchSuccess {
                url: "https://example.invalid/communityRules.json",
                file_name: "communityRules.json",
                sha256: "abc123".to_string(),
                etag: Some("\"abc123\"".to_string()),
                bytes: 42,
                fetched_at: now,
            },
        );
        manifest.save(dir.path()).expect("save");

        let reloaded = Manifest::load(dir.path());
        let cached = reloaded
            .cached(RuleDatabase::CommunityRules)
            .expect("must be cached after an update");
        assert_eq!(cached.sha256, "abc123");
        assert_eq!(cached.bytes, 42);
        assert_eq!(cached.fetched_at, now);
        assert_eq!(
            reloaded.etag(RuleDatabase::CommunityRules),
            Some("\"abc123\"")
        );
        assert!(
            reloaded
                .last_failure(RuleDatabase::CommunityRules)
                .is_none()
        );
    }

    #[test]
    fn an_unknown_failure_cause_from_a_newer_binary_keeps_the_cache_facts() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(FILE_NAME),
            r#"{
                 "version": 1,
                 "sources": {
                   "community_rules": {
                     "sha256": "good-sha",
                     "etag": "\"e\"",
                     "bytes": 7,
                     "fetched_at": "1970-01-01T00:00:00Z",
                     "last_failure": "boom",
                     "last_failure_cause": { "kind": "a_cause_from_the_future" }
                   }
                 }
               }"#,
        )
        .expect("write manifest");

        let manifest = Manifest::load(dir.path());

        let cached = manifest
            .cached(RuleDatabase::CommunityRules)
            .expect("an unknown cause must not wipe the cache facts");
        assert_eq!(cached.sha256, "good-sha");
        assert_eq!(manifest.etag(RuleDatabase::CommunityRules), Some("\"e\""));
        assert_eq!(
            manifest.last_failure(RuleDatabase::CommunityRules),
            Some(FetchFailure {
                cause: FetchFailureCause::Unclassified,
                detail: "boom".to_string(),
            })
        );
    }

    #[test]
    fn record_failure_leaves_an_existing_cached_entry_untouched() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());
        let now = jiff::Timestamp::UNIX_EPOCH;
        manifest.record_updated(
            RuleDatabase::SteamWorkshop,
            &FetchSuccess {
                url: "https://example.invalid/steamDB.json",
                file_name: "steamDB.json",
                sha256: "good-sha".to_string(),
                etag: None,
                bytes: 7,
                fetched_at: now,
            },
        );

        manifest.record_failure(
            RuleDatabase::SteamWorkshop,
            &FetchFailure {
                cause: FetchFailureCause::Transport,
                detail: "connection timed out".to_string(),
            },
            now,
        );

        let cached = manifest
            .cached(RuleDatabase::SteamWorkshop)
            .expect("a failure must not clear a previous success");
        assert_eq!(cached.sha256, "good-sha");
        assert_eq!(
            manifest.last_failure(RuleDatabase::SteamWorkshop),
            Some(FetchFailure {
                cause: FetchFailureCause::Transport,
                detail: "connection timed out".to_string(),
            })
        );
    }

    #[test]
    fn record_failure_with_no_prior_success_reports_no_cached_entry() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());

        manifest.record_failure(
            RuleDatabase::CommunityRules,
            &FetchFailure {
                cause: FetchFailureCause::HttpStatus { status: 503 },
                detail: "unexpected HTTP status 503".to_string(),
            },
            jiff::Timestamp::now(),
        );

        assert!(manifest.cached(RuleDatabase::CommunityRules).is_none());
        assert_eq!(
            manifest.last_failure(RuleDatabase::CommunityRules),
            Some(FetchFailure {
                cause: FetchFailureCause::HttpStatus { status: 503 },
                detail: "unexpected HTTP status 503".to_string(),
            })
        );
    }

    #[test]
    fn a_failure_written_before_causes_were_kept_reads_back_unclassified() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(FILE_NAME),
            r#"{"version":1,"sources":{"community_rules":{"last_failure":"dns error"}}}"#,
        )
        .expect("seed manifest");

        let manifest = Manifest::load(dir.path());

        assert_eq!(
            manifest.last_failure(RuleDatabase::CommunityRules),
            Some(FetchFailure::unclassified("dns error"))
        );
    }

    #[test]
    fn a_failure_cause_survives_a_save_and_reload() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());
        manifest.record_failure(
            RuleDatabase::RimmergeRules,
            &FetchFailure {
                cause: FetchFailureCause::TooLarge,
                detail: "response exceeded the limit".to_string(),
            },
            jiff::Timestamp::UNIX_EPOCH,
        );
        manifest.save(dir.path()).expect("save");

        let reloaded = Manifest::load(dir.path());

        assert_eq!(
            reloaded
                .last_failure(RuleDatabase::RimmergeRules)
                .map(|failure| failure.cause),
            Some(FetchFailureCause::TooLarge)
        );
    }

    #[test]
    fn record_unchanged_advances_fetched_at_and_keeps_sha_and_etag() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());
        manifest.record_updated(
            RuleDatabase::CommunityRules,
            &FetchSuccess {
                url: "https://example.invalid/communityRules.json",
                file_name: "communityRules.json",
                sha256: "same-sha".to_string(),
                etag: Some("\"same-sha\"".to_string()),
                bytes: 10,
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            },
        );

        let later = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(1);
        manifest.record_unchanged(RuleDatabase::CommunityRules, later);

        let cached = manifest
            .cached(RuleDatabase::CommunityRules)
            .expect("cached");
        assert_eq!(cached.sha256, "same-sha");
        assert_eq!(cached.fetched_at, later);
        assert_eq!(
            manifest.etag(RuleDatabase::CommunityRules),
            Some("\"same-sha\"")
        );
    }

    #[test]
    fn the_two_databases_are_recorded_independently() {
        let dir = tempdir().expect("tempdir");
        let mut manifest = Manifest::load(dir.path());

        manifest.record_updated(
            RuleDatabase::CommunityRules,
            &FetchSuccess {
                url: "https://example.invalid/communityRules.json",
                file_name: "communityRules.json",
                sha256: "community-sha".to_string(),
                etag: None,
                bytes: 1,
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            },
        );

        assert!(manifest.cached(RuleDatabase::SteamWorkshop).is_none());
        assert_eq!(
            manifest
                .cached(RuleDatabase::CommunityRules)
                .expect("community must be cached")
                .sha256,
            "community-sha"
        );
    }
}
