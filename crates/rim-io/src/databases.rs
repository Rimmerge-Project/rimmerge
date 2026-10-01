//! [`GithubRuleDatabaseFetcher`]: `rim-session`'s `RuleDatabaseFetcher`
//! port — fetches RimSort's two published rule databases, plus this
//! project's own rimmerge-rules file, straight from GitHub over HTTPS into
//! an app-global cache (`rim_io::databases_dir`), so a local RimSort
//! install is not a prerequisite for community rules.
//!
//! This module is wiring only; the real logic lives in its own
//! [`cache`] submodule (fetch -> validate -> commit for one database: the
//! host allowlist check, size bounds, parse-before-commit, and atomic
//! replace that implement the fetch safety rules) and [`manifest`] (the
//! versioned `manifest.json` envelope), against the transport seam in
//! `net::http` — `net` (crate-internal) is this crate's only network
//! dependency; nothing outside `#[cfg(test)]` ever constructs the real
//! transport, so `cargo nextest run --workspace --all-features` opens no
//! socket.
//!
//! `rim-session`'s `RefreshRuleDatabases` is the only intended caller,
//! and the `NetworkPolicy::allow_network` guard lives in that use case
//! rather than here (see [`rim_session::ports::RuleDatabaseFetcher::refresh`]'s
//! own doc comment).

mod cache;
mod manifest;

use std::collections::BTreeMap;
use std::path::Path;

use rim_session::ports::{DatabaseStatus, RefreshOutcome, RuleDatabase, RuleDatabaseFetcher};

use crate::net::http::{HttpGet, UreqHttpGet};

use cache::DatabaseSpec;

/// Every [`RuleDatabase`] variant, in the stable order both
/// [`GithubRuleDatabaseFetcher::status`] and a caller's own `databases: &[RuleDatabase]`
/// list to [`GithubRuleDatabaseFetcher::refresh`] read naturally in —
/// declaration order, matching [`RuleDatabase`]'s own `Ord`.
const ALL_DATABASES: [RuleDatabase; 3] = [
    RuleDatabase::CommunityRules,
    RuleDatabase::SteamWorkshop,
    RuleDatabase::RimmergeRules,
];

/// The rimmerge-rules cache file's own name, **derived from the spec**
/// rather than restated: `crate::mod_knowledge` reads the
/// file the fetcher writes, and two independent string literals for one
/// file name is a rename away from a silently-never-read cache.
pub(crate) const RIMMERGE_RULES_FILE: &str = cache::RIMMERGE_RULES.file_name;

fn spec_for(database: RuleDatabase) -> &'static DatabaseSpec {
    match database {
        RuleDatabase::CommunityRules => &cache::COMMUNITY_RULES,
        RuleDatabase::SteamWorkshop => &cache::STEAM_WORKSHOP,
        RuleDatabase::RimmergeRules => &cache::RIMMERGE_RULES,
    }
}

/// Fetches every [`RuleDatabase`] variant
/// over HTTPS into an app-global cache directory, implementing
/// `rim-session`'s [`RuleDatabaseFetcher`] port. Generic over the
/// transport ([`HttpGet`]) so tests substitute a scripted fake and never
/// open a socket — [`GithubRuleDatabaseFetcher::new`] wires in the real
/// `UreqHttpGet`; nothing outside this crate's own tests constructs any
/// other transport (`HttpGet` is `pub(crate)`, so `with_transport`
/// is too).
// Deliberately no `H: HttpGet` bound on the struct's own generic
// parameter: `HttpGet` is `pub(crate)`, and a bound here would leak that
// private trait into this `pub` struct's public signature
// (`private_bounds`). The bound belongs on each `impl` below instead,
// where it's only ever exercised from inside this crate anyway.
#[derive(Debug)]
pub struct GithubRuleDatabaseFetcher<H = UreqHttpGet> {
    http: H,
}

impl GithubRuleDatabaseFetcher<UreqHttpGet> {
    /// Builds the fetcher with the real transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: UreqHttpGet::new(),
        }
    }
}

impl Default for GithubRuleDatabaseFetcher<UreqHttpGet> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the fetcher with a scripted transport — this crate's own tests
/// only; see [`crate::net::http::fake::FakeHttpGet`]. A free function, not an
/// inherent method: an `impl<H: HttpGet> GithubRuleDatabaseFetcher<H>`
/// block would put the private [`HttpGet`] bound on an impl of a `pub`
/// type (`private_bounds`), even with the method itself `pub(crate)` —
/// the lint looks at the impl block's own reachability, not its items'.
#[cfg(test)]
pub(crate) fn with_transport<H: HttpGet>(http: H) -> GithubRuleDatabaseFetcher<H> {
    GithubRuleDatabaseFetcher { http }
}

impl<H: HttpGet> RuleDatabaseFetcher for GithubRuleDatabaseFetcher<H> {
    fn status(
        &self,
        cache_dir: &Path,
        enabled: &BTreeMap<RuleDatabase, bool>,
    ) -> Vec<DatabaseStatus> {
        ALL_DATABASES
            .into_iter()
            .map(|database| {
                let is_enabled = enabled.get(&database).copied().unwrap_or(false);
                cache::status(cache_dir, spec_for(database), is_enabled)
            })
            .collect()
    }

    fn refresh(
        &self,
        cache_dir: &Path,
        databases: &[RuleDatabase],
    ) -> Vec<(RuleDatabase, RefreshOutcome)> {
        databases
            .iter()
            .map(|&database| {
                let outcome = cache::refresh(&self.http, cache_dir, spec_for(database));
                (database, outcome)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::net::http::fake::{FakeCall, FakeHttpGet};

    use super::*;

    fn enabled_map(community: bool, steam: bool) -> BTreeMap<RuleDatabase, bool> {
        // The third source is left out on purpose here — a database
        // missing from the map must read as disabled, never enabled by
        // assumption (the next test pins that directly).
        [
            (RuleDatabase::CommunityRules, community),
            (RuleDatabase::SteamWorkshop, steam),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn status_returns_one_row_per_database_with_the_given_enabled_flags() {
        let dir = tempdir().expect("tempdir");
        let fetcher = with_transport(FakeHttpGet::new(Vec::new()));

        let rows = fetcher.status(dir.path(), &enabled_map(true, false));

        assert_eq!(rows.len(), 3);
        let community = rows
            .iter()
            .find(|row| row.database == RuleDatabase::CommunityRules)
            .expect("community row");
        assert!(community.enabled);
        let steam = rows
            .iter()
            .find(|row| row.database == RuleDatabase::SteamWorkshop)
            .expect("steam row");
        assert!(!steam.enabled);
    }

    #[test]
    fn status_reports_a_missing_map_entry_as_disabled_never_enabled_by_assumption() {
        let dir = tempdir().expect("tempdir");
        let fetcher = with_transport(FakeHttpGet::new(Vec::new()));

        let rows = fetcher.status(dir.path(), &BTreeMap::new());

        assert!(rows.iter().all(|row| !row.enabled));
    }

    #[test]
    fn refresh_fetches_every_requested_database_in_order_and_never_skips() {
        let dir = tempdir().expect("tempdir");
        let http = FakeHttpGet::new(vec![
            FakeCall::ok(b"{\"rules\":{}}".to_vec()),
            FakeCall::Failure("offline".to_string()),
        ]);
        let fetcher = with_transport(http);

        let outcomes = fetcher.refresh(
            dir.path(),
            &[RuleDatabase::CommunityRules, RuleDatabase::SteamWorkshop],
        );

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].0, RuleDatabase::CommunityRules);
        assert!(matches!(outcomes[0].1, RefreshOutcome::Updated { .. }));
        assert_eq!(outcomes[1].0, RuleDatabase::SteamWorkshop);
        assert!(matches!(outcomes[1].1, RefreshOutcome::Failed { .. }));
    }

    #[test]
    fn refresh_with_an_empty_list_calls_the_transport_zero_times() {
        let dir = tempdir().expect("tempdir");
        let http = FakeHttpGet::new(Vec::new());
        let fetcher = with_transport(http);

        let outcomes = fetcher.refresh(dir.path(), &[]);

        assert!(outcomes.is_empty());
        assert_eq!(fetcher.http.call_count(), 0);
    }
}
