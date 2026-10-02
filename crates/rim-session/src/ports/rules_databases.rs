//! Rule-database ports: the fetcher, cached-database status, the RimSort importer, and the import
//! manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::Rule;

use super::fetch_failure::FetchFailure;
use super::stores::StoreError;
use crate::app_settings::StaleAfterDays;

/// One of the rule databases this app can fetch from GitHub. Closed on
/// purpose: a new source is a deliberate code change, not a configuration
/// value —
/// the URLs are `const`s in `rim_io::databases::cache`, never settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleDatabase {
    /// `RimSort/Community-Rules-Database`'s `communityRules.json`.
    CommunityRules,
    /// `RimSort/Steam-Workshop-Database`'s `steamDB.json`.
    SteamWorkshop,
    /// This project's own `rimmerge-rules.json` — the mod-specific
    /// knowledge this workspace keeps as data rather than source. Four
    /// sections in one file: precedence, patch-operation behaviours,
    /// def-cache carriers, tag rules. See [`ModKnowledgeStore`](super::stores::ModKnowledgeStore) for which
    /// of them are read at load time and which is not.
    RimmergeRules,
}

impl RuleDatabase {
    /// Every declared source, in declaration order — the one place that
    /// enumerates all variants (Rust has no built-in way to), so a caller
    /// that needs "all of them" (`crate::use_cases::RefreshRuleDatabases::execute_automatic`'s
    /// own auto-eligible subset) derives it from here rather than
    /// hand-listing variants a second time. **Not itself exhaustiveness-
    /// checked** — a fourth variant added to the enum above does not fail
    /// this array to update; `is_auto_refresh_eligible`'s own `match`
    /// (no `_` arm) is what forces every variant to be considered.
    pub const ALL: [RuleDatabase; 3] = [
        Self::CommunityRules,
        Self::SteamWorkshop,
        Self::RimmergeRules,
    ];

    /// Whether the once-a-day automatic refresh
    /// (`crate::use_cases::RefreshRuleDatabases::execute_automatic`) may
    /// fetch this source at all — checked in addition to, never instead
    /// of, `NetworkPolicy::allow_network` and this source's own fetch
    /// toggle. Closed `match`, no `_` arm: a fourth source must decide
    /// this explicitly, not inherit a default.
    #[must_use]
    pub fn is_auto_refresh_eligible(self) -> bool {
        match self {
            Self::CommunityRules | Self::RimmergeRules => true,
            // ~49 MB. An automatic daily download of that size, whenever
            // the file changes, is exactly what
            // `NetworkPolicy::fetch_steam_workshop`'s own doc comment
            // says must never happen without the user asking — manual
            // only, even once enabled.
            Self::SteamWorkshop => false,
        }
    }

    /// Whether this source belongs to the recommended setup — the set
    /// `NetworkPolicy::default` turns on. Recommended is not the same as
    /// automatic: the Steam Workshop database is recommended and on by
    /// default, yet [`Self::is_auto_refresh_eligible`] stays `false` for
    /// it, so it is only ever downloaded by an explicit refresh. Closed
    /// `match`, no `_` arm: a fourth source must decide this explicitly.
    #[must_use]
    pub fn is_recommended(self) -> bool {
        match self {
            Self::CommunityRules | Self::SteamWorkshop | Self::RimmergeRules => true,
        }
    }

    /// Whether this source is imported into a profile (an import record
    /// in `<profile>/imports/manifest.json`, rules merged into
    /// `rules.json`). `RimmergeRules` is not: its sections are read
    /// straight from the cache at profile load. The one authoritative
    /// answer: `import_source_key` and `resolve_from_cache` derive from
    /// it. Closed `match`, no `_` arm: a fourth source must decide this
    /// explicitly.
    #[must_use]
    pub fn is_importable(self) -> bool {
        match self {
            Self::CommunityRules | Self::SteamWorkshop => true,
            Self::RimmergeRules => false,
        }
    }
}

/// Why a source was not requested by [`RuleDatabaseFetcher::refresh`].
/// Four variants, because "you turned this database off", "you turned
/// the network off", "you turned automatic refresh off", and "not due
/// yet" each need different words in the UI and a different fix from the
/// user (the last needs none at all — it's information, not a choice to
/// reconsider).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// This source's own fetch toggle is off.
    SourceDisabled,
    /// The master network switch (`NetworkPolicy::allow_network`) is off.
    NetworkDisabled,
    /// `crate::use_cases::RefreshRuleDatabases::execute_automatic` only:
    /// the master network switch is on, but
    /// `NetworkPolicy::auto_refresh_rule_databases` itself is off —
    /// distinct from [`Self::NetworkDisabled`] so a user who turned off
    /// only automatic refresh (network access itself still on) sees the
    /// switch that actually needs flipping.
    AutoRefreshDisabled,
    /// `crate::use_cases::RefreshRuleDatabases::execute_automatic` only:
    /// this source's last attempt (successful or not) was under 24 h
    /// ago, so today's automatic refresh has nothing to do yet. Never
    /// produced by a manual refresh, which ignores this cadence
    /// entirely — a click is its own consent.
    NotDue,
}

/// What one source's refresh did. Never an `Err` for a network problem —
/// being offline is normal, and the cached copy stays usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// New bytes were fetched, parsed, and written to the cache.
    Updated {
        /// The new cache file's sha256, hex-encoded.
        sha256: String,
        /// The new cache file's size in bytes.
        bytes: usize,
    },
    /// The server said `304 Not Modified`, or the body hashed to what was
    /// already cached.
    Unchanged {
        /// The (unchanged) cache file's sha256, hex-encoded.
        sha256: String,
    },
    /// A network, HTTP, size, or parse failure. The cache is left
    /// untouched; the failure's `detail` is already bounded and safe to
    /// display (never a response body echoed back).
    Failed {
        /// Why the attempt failed.
        failure: FetchFailure,
    },
    /// Nothing was requested: either this source is disabled, or the
    /// network-refresh switch is off. [`RuleDatabaseFetcher::refresh`]
    /// itself never produces this variant (see its own doc comment) —
    /// only the use case that decides what to pass it (`RefreshRuleDatabases`)
    /// does, for the sources it excluded.
    Skipped {
        /// Why the source was not requested.
        reason: SkipReason,
    },
}

/// One source's cache state as recorded the last time it was
/// successfully fetched (or confirmed unchanged).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedDatabase {
    /// The cached file's sha256, hex-encoded — the identity shown to the
    /// user (first 12 hex chars, this repo's existing hash-display
    /// convention).
    pub sha256: String,
    /// The cached file's size in bytes.
    pub bytes: usize,
    /// When this file was last fetched or confirmed unchanged (`304`).
    pub fetched_at: jiff::Timestamp,
}

/// One source's cache status, read from the manifest with no network
/// involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseStatus {
    /// Which database this row describes.
    pub database: RuleDatabase,
    /// Whether this source is currently configured to be fetched. A
    /// caller-supplied policy fact — see [`RuleDatabaseFetcher::status`]'s
    /// own doc comment for why this can't be derived from the cache
    /// manifest alone.
    pub enabled: bool,
    /// Where this source's cache file lives, whether or not it exists
    /// yet.
    pub path: PathBuf,
    /// The last successful fetch (or `304`-confirmed-unchanged), if
    /// there has ever been one.
    pub cached: Option<CachedDatabase>,
    /// The most recent refresh's failure, if the last attempt failed.
    /// Persisted across restarts; cleared on the next success.
    pub last_failure: Option<FetchFailure>,
    /// When this source's last attempt (success, unchanged, or failure)
    /// happened — `None` when there has never been one. Unlike
    /// `cached.fetched_at`, this advances on a failed attempt too,
    /// which is what `RefreshRuleDatabases::execute_automatic`'s
    /// once-a-day cadence gates on: without it, an offline machine
    /// would retry at every launch rather than once a day.
    pub last_attempt_at: Option<jiff::Timestamp>,
}

impl DatabaseStatus {
    /// Whether the last successful fetch is older than `threshold` —
    /// `false` when there has never been one (nothing to be stale).
    /// **Defined once, here**, so `apps/cli`'s `db status`, the desktop
    /// Databases card, and the `RuleDatabasesStale` reminder
    /// (`crate::notifications`) render/gate off the identical threshold
    /// rather than each hardcoding their own copy that could drift.
    ///
    /// `now` is an **injected** clock, never read from the system inside
    /// this method, per the root `CLAUDE.md` determinism rule — this is
    /// what makes the method's own tests deterministic, and it is the
    /// only way a caller could reuse it in a context that needs a fixed
    /// "as of" instant. `threshold` is `AppSettings::reminders`'s own
    /// `StaleAfterDays` — no longer a hardcoded constant, since it's now
    /// a setting. Staleness is display-only: it must never change
    /// behaviour, trigger a fetch, or block an import (no TTL, no
    /// automatic invalidation) — a stale cache is still used exactly as
    /// a fresh one is everywhere except this one label (and the passive
    /// reminder it now also feeds).
    #[must_use]
    pub fn is_stale(&self, now: jiff::Timestamp, threshold: StaleAfterDays) -> bool {
        let threshold_seconds = i64::from(threshold.get()) * 24 * 60 * 60;
        let Some(cached) = &self.cached else {
            return false;
        };
        // A `fetched_at` in the future (the clock moved back since) can never
        // be proven fresh again, so it reads as stale, like every other
        // stored-timestamp check (`notifications::clock::seconds_since`).
        match crate::notifications::clock::seconds_since(now, cached.fetched_at) {
            Some(elapsed) => elapsed > threshold_seconds,
            None => true,
        }
    }
}

/// Fetches rule databases from their fixed, hardcoded source (no setting,
/// environment variable, or flag can change what URL a database is
/// fetched from) into an app-global cache directory
/// (`rim_io::databases_dir`, distinct from any profile directory: the
/// cache is shared across every profile).
/// Implemented by `rim_io::GithubRuleDatabaseFetcher`.
///
/// Called by [`crate::use_cases::RefreshRuleDatabases`] — the only caller,
/// and the only place in this crate that ever touches this port. See
/// [`Self::status`]'s doc comment for why that method takes more than a
/// cache directory.
pub trait RuleDatabaseFetcher {
    /// Reads the cache manifest under `cache_dir`. No network; never
    /// fails on a missing or unreadable manifest — that state is simply
    /// "never fetched", reported as one [`DatabaseStatus`] per
    /// [`RuleDatabase`] variant with `cached: None`.
    ///
    /// **Why `enabled` is a parameter**: [`DatabaseStatus`]
    /// carries an `enabled` field this method has no other way to fill
    /// in — whether a source is fetched is `NetworkPolicy::fetch_community_rules`/
    /// `fetch_steam_workshop`/`fetch_rimmerge_rules`, a `rim-session`
    /// policy concept the cache manifest (a plain infrastructure fact:
    /// what's on disk, when it was fetched, whether the last attempt
    /// failed) neither stores nor should store. `enabled` is threaded in
    /// as a parameter instead, so
    /// `rim-io` stays policy-free while callers still get one
    /// ready-to-render row per source. A [`RuleDatabase`] missing from
    /// `enabled` is reported as disabled — fail safe, matching this
    /// repo's deny-by-default baseline, never enabled by assumption.
    fn status(
        &self,
        cache_dir: &Path,
        enabled: &BTreeMap<RuleDatabase, bool>,
    ) -> Vec<DatabaseStatus>;

    /// Fetches `databases`, in the given order, into `cache_dir`. One
    /// entry per requested database, in the same order — never
    /// [`RefreshOutcome::Skipped`]: a database this method is actually
    /// asked to fetch is, by construction, neither disabled nor blocked
    /// by the network switch. Both checks belong in the caller
    /// (`RefreshRuleDatabases`), which must decide
    /// them *before* calling this method at all — so that turning off
    /// `NetworkPolicy::allow_network` is an enforced guarantee (no URL
    /// built, no socket opened) rather than one merely documented at this
    /// boundary. The only way this can fail wholesale is a `cache_dir` it
    /// cannot create, which surfaces as [`RefreshOutcome::Failed`] on
    /// every requested entry.
    fn refresh(
        &self,
        cache_dir: &Path,
        databases: &[RuleDatabase],
    ) -> Vec<(RuleDatabase, RefreshOutcome)>;
}

/// Where RimSort's three database files live. `None` is a supported
/// state for each field, not an error:
/// `userRules.json` is read live off a
/// RimSort install rather than copied into Rimmerge's own storage, so a
/// user with no RimSort install (or an import that only means to refresh
/// the two fetched databases) has no path to give for one or more of
/// these — [`RimSortImporter::import`] skips a `None` field entirely
/// rather than treating it as a file that failed to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RimSortPaths {
    /// `userRules.json`, when this import includes it.
    pub user_rules: Option<PathBuf>,
    /// `communityRules.json`, when this import includes it.
    pub community_rules: Option<PathBuf>,
    /// `steamDB.json`, when this import includes it.
    pub steam_db: Option<PathBuf>,
}

/// A RimSort import failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ImportError(pub String);

/// [`ImportedRules`]'s own per-source keys — also [`ImportManifestStore`]'s
/// keys, so the two never drift apart. Deliberately plain constants, not
/// an enum: unlike [`RuleDatabase`] (two sources, a closed, code-level
/// choice), these three names are the on-disk identity of a
/// persisted `<profile>/imports/manifest.json` record — changing one
/// would silently orphan every existing profile's own history for that
/// source, so a rename must be a deliberate, visible edit to a `&str`
/// literal, not a `Debug`-derived enum variant a refactor could rename
/// for free.
pub const IMPORT_SOURCE_USER_RULES: &str = "user_rules";

/// See [`IMPORT_SOURCE_USER_RULES`].
pub const IMPORT_SOURCE_COMMUNITY_RULES: &str = "community_rules";

/// See [`IMPORT_SOURCE_USER_RULES`].
pub const IMPORT_SOURCE_STEAM_DEPENDENCIES: &str = "steam_dependencies";

/// One imported source's own provenance: the exact bytes
/// [`RimSortImporter::import`] most recently copied into
/// `<profile>/imports/` for it.
/// Persisted by [`ImportManifestStore`], keyed by
/// [`ImportedRules::provenance`]'s own source-name convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    /// The snapshot's file name under `imports/` (e.g. `"userRules.json"`).
    pub file: String,
    /// The sha256 of the exact bytes imported.
    pub sha256: String,
    /// The byte count of the exact bytes imported.
    pub bytes: usize,
    /// RFC 3339 UTC. A plain `String`, like every other timestamp this
    /// workspace persists (jiff has no `serde` feature enabled).
    pub imported_at: String,
}

/// Persists [`ImportedRules::provenance`] into
/// `<profile>/imports/manifest.json` — the rules-page badge compares the
/// global cache's current sha256 against the sha256 a profile last
/// recorded here for the same source. Implemented by `rim_io`'s
/// `JsonImportManifestStore`.
///
/// **Called only by [`crate::use_cases::ImportRimSort::execute`], only
/// after its own `RuleStore::save` has already succeeded**:
/// `RimSortImporter::import` never writes this manifest itself, since a
/// `rule_store.save` failure afterward rolls the session's rules back but
/// would leave the manifest's own claim on disk unchanged, recording a
/// sha256 for rules that were never actually persisted. Writing only
/// *after* a successful `rule_store.save`, in the same use case that owns
/// that transaction boundary, makes that state unreachable rather than
/// merely rare.
pub trait ImportManifestStore {
    /// Every source's own record currently on file, keyed by
    /// [`ImportedRules::provenance`]'s own convention — empty when there
    /// is no manifest yet, or it's unreadable, malformed, or an
    /// unrecognized version (a snapshot manifest losing itself costs
    /// nothing but a stale badge until the next import, never a hard
    /// error — the same reasoning [`RuleDatabaseFetcher::status`] applies
    /// to the cache manifest).
    fn load(&self, profile_dir: &Path) -> BTreeMap<String, ImportRecord>;

    /// Merges `records` into whatever's already on file — each key
    /// replaces or adds its own record, every other existing key is left
    /// exactly as it was (the same per-source "untouched, not cleared"
    /// semantics [`crate::Session::apply_import`] has for the rules
    /// themselves) — then writes the whole file back
    /// atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(
        &self,
        profile_dir: &Path,
        records: &BTreeMap<String, ImportRecord>,
    ) -> Result<(), StoreError>;
}

/// Borrow-lending impl, the same convention as `DefSourceReader for &T`.
impl<T: ImportManifestStore + ?Sized> ImportManifestStore for &T {
    fn load(&self, profile_dir: &Path) -> BTreeMap<String, ImportRecord> {
        (**self).load(profile_dir)
    }

    fn save(
        &self,
        profile_dir: &Path,
        records: &BTreeMap<String, ImportRecord>,
    ) -> Result<(), StoreError> {
        (**self).save(profile_dir, records)
    }
}

/// The rules imported from RimSort's three database files, split by
/// origin so the caller can report per-file counts.
///
/// Skip counts are reported separately per source rather than summed:
/// `userRules.json`/`communityRules.json` skip individual *relations*
/// (a `loadAfter` entry, an `incompatibleWith` entry, ...), while
/// `steamDB.json` skips whole *entries* (an inactive mod's dependency
/// list) — folding both into one number would make the real-world count
/// dominated by `steamDB.json`'s much larger, mostly-irrelevant database
/// and hide whether the smaller, curated rule files actually matched
/// anything.
///
/// **`None` vs. `Some(vec![])`, per field**: `None` means this source was not part of this import
/// — [`crate::Session::apply_import`] leaves that origin's existing
/// rules alone. `Some(rules)` means this source *was* imported —
/// `apply_import` replaces exactly that origin's rules with `rules`,
/// even when `rules` is empty (a real, distinct state: an import that
/// legitimately found nothing active to rule on). Collapsing the two
/// — e.g. treating a missing `userRules.json` path the same as an empty
/// result — would silently delete the user's own hand-added
/// `RimSortUser` rules on every refresh-then-import-from-cache run; see
/// [`crate::Session::apply_import`]'s own doc comment for the mechanism,
/// and its test module for the regression test written failing-first
/// against exactly this.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedRules {
    /// Rules from `userRules.json`, when it was part of this import.
    pub user_rules: Option<Vec<Rule>>,
    /// Rules from `communityRules.json`, when it was part of this
    /// import.
    pub community_rules: Option<Vec<Rule>>,
    /// Pair rules derived from `steamDB.json`'s dependency data, when it
    /// was part of this import.
    pub steam_dependencies: Option<Vec<Rule>>,
    /// Relations skipped in `userRules.json`/`communityRules.json`
    /// because at least one side wasn't an active mod. Summed only over
    /// the sources actually imported this call (a `None` source
    /// contributes nothing to this count, not even zero-as-a-value).
    pub skipped_inactive_rules: usize,
    /// `steamDB.json` entries skipped because their own `packageId`
    /// wasn't active — counted only for entries that actually had
    /// dependencies to report (an inactive entry with none is not a
    /// meaningfully "skipped" relation). Zero (not summed with anything
    /// else) when `steam_dependencies` is `None`.
    pub skipped_inactive_steam: usize,
    /// Per-source provenance for every source imported this call, keyed
    /// by [`IMPORT_SOURCE_USER_RULES`]/[`IMPORT_SOURCE_COMMUNITY_RULES`]/
    /// [`IMPORT_SOURCE_STEAM_DEPENDENCIES`] — the same keys
    /// [`ImportManifestStore`] persists under. A source with no entry
    /// here was not part of this import, mirroring the `None` arms above
    /// exactly.
    pub provenance: BTreeMap<String, ImportRecord>,
}

/// Borrow-lending impl, the same convention as `DefSourceReader for &T`.
impl<T: RuleDatabaseFetcher + ?Sized> RuleDatabaseFetcher for &T {
    fn status(
        &self,
        cache_dir: &Path,
        enabled: &BTreeMap<RuleDatabase, bool>,
    ) -> Vec<DatabaseStatus> {
        (**self).status(cache_dir, enabled)
    }

    fn refresh(
        &self,
        cache_dir: &Path,
        databases: &[RuleDatabase],
    ) -> Vec<(RuleDatabase, RefreshOutcome)> {
        (**self).refresh(cache_dir, databases)
    }
}

/// Imports RimSort's `userRules.json`/`communityRules.json`/`steamDB.json`.
pub trait RimSortImporter {
    /// Imports every rule naming only mods in `active` — keyed by
    /// [`ModId::base`], mapped to each mod's own workshop id when it has
    /// one (see [`crate::Session::active_mods_by_base_id`]): `steamDB.json`
    /// entries are matched by workshop id first, falling back to
    /// `packageId` only for a mod with none (two uploads of the same
    /// `packageId` can otherwise be confused for one another).
    ///
    /// A `None` field on `paths` is skipped outright — no read, no
    /// parse, no snapshot copy — and reported as `None` on the matching
    /// [`ImportedRules`] field (not-imported, never an imported zero). A
    /// `Some` path that can't be read or parsed is still a hard
    /// [`ImportError`]: a path the caller explicitly gave that
    /// turns out to be missing or malformed is a real failure, not a
    /// "this source wasn't requested" state.
    ///
    /// Snapshots each present source's exact bytes into
    /// `<profile_dir>/imports/` and returns their provenance on
    /// [`ImportedRules::provenance`] — but does **not** write
    /// `<profile_dir>/imports/manifest.json` itself. Persisting that
    /// manifest is [`crate::use_cases::ImportRimSort::execute`]'s own job,
    /// done only after its `RuleStore::save` has already succeeded (see
    /// [`ImportManifestStore`]'s own doc comment for why).
    ///
    /// # Errors
    ///
    /// Returns [`ImportError`] when a `Some` database path can't be read
    /// or parsed.
    fn import(
        &self,
        paths: &RimSortPaths,
        active: &BTreeMap<ModId, Option<u64>>,
    ) -> Result<ImportedRules, ImportError>;
}

/// Borrow-lending impl, the same convention as `DefSourceReader for &T`.
/// Used by tests today (they share one fake importer with the use case
/// under test); no production caller needs it yet.
impl<T: RimSortImporter + ?Sized> RimSortImporter for &T {
    fn import(
        &self,
        paths: &RimSortPaths,
        active: &BTreeMap<ModId, Option<u64>>,
    ) -> Result<ImportedRules, ImportError> {
        (**self).import(paths, active)
    }
}
