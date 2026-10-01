//! Where each of an import's three sources comes from — shared by every
//! composition root's own `import` command (`apps/cli`, `apps/desktop`)
//! so the cache-vs-RimSort resolution order is defined exactly once
//! rather than copied into two interfaces (otherwise one interface could
//! fetch the cache but have no way to import from it).

use std::collections::BTreeMap;
use std::path::Path;

use crate::NetworkPolicy;
use crate::ports::{RimSortPaths, RuleDatabase, RuleDatabaseFetcher};

/// A source is only read from the cache when its own `NetworkPolicy` fetch
/// toggle is on *and* its cache file actually exists on disk —
/// **an interpretation, not a stated rule**: the resolution order
/// ("explicit `--rimsort-dir` > the global cache > nothing") doesn't say
/// what happens to a source that's disabled or missing from the cache,
/// and importing one anyway would risk resurrecting stale or deliberately
/// toggled-off rules on the next `apply_import` — precisely the class of
/// bug `ImportedRules`' `None`-vs-`Some(vec![])` distinction exists to
/// prevent.
#[must_use]
pub fn should_import_from_cache(source_enabled: bool, cache_file_exists: bool) -> bool {
    source_enabled && cache_file_exists
}

/// `RimSortPaths` built straight from RimSort's own on-disk layout under
/// `dir`: `userRules.json` directly, `communityRules.json`/`steamDB.json`
/// under their own subfolders. `user_rules_override`, when given, wins
/// over `dir`'s own `userRules.json` — a caller can point `--user-rules`
/// somewhere else while still reading community/steam from `dir`.
#[must_use]
pub fn resolve_from_rimsort_dir(dir: &Path, user_rules_override: Option<&Path>) -> RimSortPaths {
    let user_rules =
        user_rules_override.map_or_else(|| dir.join("userRules.json"), Path::to_path_buf);
    RimSortPaths {
        user_rules: Some(user_rules),
        community_rules: Some(
            dir.join("Community-Rules-Database")
                .join("communityRules.json"),
        ),
        steam_db: Some(dir.join("Steam-Workshop-Database").join("steamDB.json")),
    }
}

/// `RimSortPaths` built from the global rule-database cache: community/
/// steam come from whichever of the two is both enabled in `policy`
/// and actually present on disk ([`should_import_from_cache`]).
/// `userRules.json` has no cache of its own — this is
/// `Some(path)` only when `user_rules_override` is given, `None`
/// otherwise (not part of this import, never a silent zero — the
/// `RimSortPaths::user_rules` doc comment covers why `None` is a
/// supported state, not an error).
///
/// Uses [`RuleDatabaseFetcher::status`] directly — not
/// [`crate::use_cases::RefreshRuleDatabases::status`], which also joins
/// the import manifest to compute the unrelated `needs_reimport`
/// signal — since resolving an import path has no use for that
/// comparison. `RuleDatabaseFetcher::status` alone touches no import
/// manifest and performs no network call, exactly the narrow "where does
/// this database's cache file live" question this function needs
/// answered.
#[must_use]
pub fn resolve_from_cache(
    fetcher: &impl RuleDatabaseFetcher,
    policy: &NetworkPolicy,
    cache_dir: &Path,
    user_rules_override: Option<&Path>,
) -> RimSortPaths {
    let enabled = BTreeMap::from([
        (
            RuleDatabase::CommunityRules,
            policy.fetches(RuleDatabase::CommunityRules),
        ),
        (
            RuleDatabase::SteamWorkshop,
            policy.fetches(RuleDatabase::SteamWorkshop),
        ),
    ]);
    let mut community_rules = None;
    let mut steam_db = None;
    for status in fetcher.status(cache_dir, &enabled) {
        if !should_import_from_cache(status.enabled, status.path.is_file()) {
            continue;
        }
        match status.database {
            RuleDatabase::CommunityRules => community_rules = Some(status.path),
            RuleDatabase::SteamWorkshop => steam_db = Some(status.path),
            // Not a RimSort file and not part of a RimSort import at all:
            // its four load-time sections are read straight from the
            // cache by `ModKnowledgeStore` (never through an import), and
            // its fifth (tag rules) has no importer yet.
            RuleDatabase::RimmergeRules => {}
        }
    }
    RimSortPaths {
        user_rules: user_rules_override.map(Path::to_path_buf),
        community_rules,
        steam_db,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    // -- `should_import_from_cache` --

    #[test]
    fn enabled_and_present_is_imported_from_cache() {
        assert!(should_import_from_cache(true, true));
    }

    #[test]
    fn disabled_source_is_never_imported_from_cache_even_if_the_file_exists() {
        assert!(!should_import_from_cache(false, true));
    }

    #[test]
    fn enabled_source_with_no_cache_file_is_not_imported() {
        assert!(!should_import_from_cache(true, false));
    }

    #[test]
    fn disabled_and_absent_is_not_imported() {
        assert!(!should_import_from_cache(false, false));
    }

    // -- `resolve_from_rimsort_dir` --

    #[test]
    fn resolve_from_rimsort_dir_builds_all_three_paths_under_the_given_directory() {
        let paths = resolve_from_rimsort_dir(Path::new("C:/rimsort"), None);
        assert_eq!(
            paths.user_rules,
            Some(PathBuf::from("C:/rimsort/userRules.json"))
        );
        assert_eq!(
            paths.community_rules,
            Some(PathBuf::from(
                "C:/rimsort/Community-Rules-Database/communityRules.json"
            ))
        );
        assert_eq!(
            paths.steam_db,
            Some(PathBuf::from(
                "C:/rimsort/Steam-Workshop-Database/steamDB.json"
            ))
        );
    }

    #[test]
    fn resolve_from_rimsort_dir_lets_an_explicit_user_rules_override_win() {
        let paths = resolve_from_rimsort_dir(
            Path::new("C:/rimsort"),
            Some(Path::new("C:/elsewhere/userRules.json")),
        );
        assert_eq!(
            paths.user_rules,
            Some(PathBuf::from("C:/elsewhere/userRules.json"))
        );
    }

    // -- `resolve_from_cache` --

    struct StubFetcher {
        statuses: Vec<crate::ports::DatabaseStatus>,
    }

    impl RuleDatabaseFetcher for StubFetcher {
        fn status(
            &self,
            _cache_dir: &Path,
            _enabled: &BTreeMap<RuleDatabase, bool>,
        ) -> Vec<crate::ports::DatabaseStatus> {
            self.statuses.clone()
        }

        fn refresh(
            &self,
            _cache_dir: &Path,
            _databases: &[RuleDatabase],
        ) -> Vec<(RuleDatabase, crate::ports::RefreshOutcome)> {
            Vec::new()
        }
    }

    fn status(database: RuleDatabase, enabled: bool, path: &Path) -> crate::ports::DatabaseStatus {
        crate::ports::DatabaseStatus {
            database,
            enabled,
            path: path.to_path_buf(),
            cached: None,
            last_failure: None,
            last_attempt_at: None,
        }
    }

    fn policy(fetch_community: bool, fetch_steam: bool) -> NetworkPolicy {
        NetworkPolicy {
            fetch_community_rules: fetch_community,
            fetch_steam_workshop: fetch_steam,
            ..NetworkPolicy::default()
        }
    }

    #[test]
    fn resolve_from_cache_reads_only_enabled_sources_whose_file_really_exists() {
        // `resolve_from_cache` checks `status.path.is_file()` for real
        // (matching `should_import_from_cache`'s own contract), so this
        // needs genuine files on disk, not just a scripted `enabled` flag.
        let dir = tempfile::tempdir().expect("tempdir");
        let community_path = dir.path().join("communityRules.json");
        let steam_path = dir.path().join("steamDB.json");
        std::fs::write(&community_path, b"{}").expect("write community fixture");
        // `steam_path` is deliberately never written — `enabled: true` on
        // its own status must not be enough without a real file present.

        let fetcher = StubFetcher {
            statuses: vec![
                status(RuleDatabase::CommunityRules, true, &community_path),
                status(RuleDatabase::SteamWorkshop, true, &steam_path),
            ],
        };
        let paths = resolve_from_cache(&fetcher, &policy(true, true), dir.path(), None);
        assert_eq!(paths.community_rules, Some(community_path));
        assert_eq!(
            paths.steam_db, None,
            "enabled is true but the cache file was never written"
        );
        assert_eq!(
            paths.user_rules, None,
            "no override was given, so user rules are not part of this import"
        );
    }

    #[test]
    fn resolve_from_cache_never_imports_a_disabled_source_even_with_a_real_cache_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let community_path = dir.path().join("communityRules.json");
        std::fs::write(&community_path, b"{}").expect("write community fixture");

        let fetcher = StubFetcher {
            statuses: vec![status(RuleDatabase::CommunityRules, false, &community_path)],
        };
        let paths = resolve_from_cache(&fetcher, &policy(false, false), dir.path(), None);
        assert_eq!(
            paths.community_rules, None,
            "the port reported this source disabled, even though its file exists"
        );
    }

    #[test]
    fn resolve_from_cache_honours_an_explicit_user_rules_override() {
        let fetcher = StubFetcher { statuses: vec![] };
        let paths = resolve_from_cache(
            &fetcher,
            &policy(true, true),
            Path::new("cache"),
            Some(Path::new("C:/RimSort/dbs/userRules.json")),
        );
        assert_eq!(
            paths.user_rules,
            Some(PathBuf::from("C:/RimSort/dbs/userRules.json"))
        );
    }
}
