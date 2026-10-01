//! Real-network verification: GETs the real GitHub-hosted rule databases
//! for real,
//! over HTTPS, through the real [`rim_io::GithubRuleDatabaseFetcher`] —
//! the one place in this crate's test suite that actually opens a socket.
//!
//! `#[ignore]`d so `cargo nextest run --workspace --all-features` (the
//! default gate) never depends on network access or GitHub's own
//! availability, per the root `CLAUDE.md`'s network hard rule. Run
//! explicitly with:
//! `cargo nextest run -p rim-io --all-features --run-ignored ignored-only -E 'binary(real_network_databases)'`
//!
//! Bands, not exact counts: these are live third-party
//! files that change over time.

use rim_session::ports::{RefreshOutcome, RuleDatabase, RuleDatabaseFetcher};
use tempfile::tempdir;

/// The community rules file's entry count, as a wide band, since the repo
/// can change.
const COMMUNITY_RULE_COUNT_BAND: std::ops::RangeInclusive<usize> = 500..=800;

/// The live Steam database has tens of thousands of entries (matching
/// `crates/rim-io/src/rimsort/steam_db.rs`'s own doc comment), not hundreds
/// of thousands. A wide lower bound only — an upper bound would make this
/// test fragile against the database's own continued growth.
const STEAM_ENTRY_COUNT_MIN: usize = 40_000;

#[test]
#[ignore = "hits the real network; run explicitly, see this file's own doc comment"]
fn community_rules_fetches_and_parses_within_the_expected_band() {
    let cache_dir = tempdir().expect("tempdir");
    let fetcher = rim_io::GithubRuleDatabaseFetcher::new();

    let outcomes = fetcher.refresh(cache_dir.path(), &[RuleDatabase::CommunityRules]);

    assert_eq!(outcomes.len(), 1);
    let (database, outcome) = &outcomes[0];
    assert_eq!(*database, RuleDatabase::CommunityRules);
    let bytes = match outcome {
        RefreshOutcome::Updated { bytes, .. } => *bytes,
        other => panic!("expected Updated fetching the real community rules file, got {other:?}"),
    };
    assert!(bytes > 0);

    let raw = std::fs::read(cache_dir.path().join("communityRules.json")).expect("cache file");
    let file: serde_json::Value = serde_json::from_slice(&raw).expect("must parse as JSON");
    let rule_count = file
        .get("rules")
        .and_then(serde_json::Value::as_object)
        .map(serde_json::Map::len)
        .unwrap_or(0);
    assert!(
        COMMUNITY_RULE_COUNT_BAND.contains(&rule_count),
        "expected {} rule entries in the expected band {:?}",
        rule_count,
        COMMUNITY_RULE_COUNT_BAND
    );
}

#[test]
#[ignore = "hits the real network; run explicitly, see this file's own doc comment"]
fn steam_workshop_fetches_and_parses_within_the_expected_band() {
    let cache_dir = tempdir().expect("tempdir");
    let fetcher = rim_io::GithubRuleDatabaseFetcher::new();

    let outcomes = fetcher.refresh(cache_dir.path(), &[RuleDatabase::SteamWorkshop]);

    assert_eq!(outcomes.len(), 1);
    let (database, outcome) = &outcomes[0];
    assert_eq!(*database, RuleDatabase::SteamWorkshop);
    let bytes = match outcome {
        RefreshOutcome::Updated { bytes, .. } => *bytes,
        other => panic!("expected Updated fetching the real steam workshop file, got {other:?}"),
    };
    assert!(bytes > 0);

    let raw = std::fs::read(cache_dir.path().join("steamDB.json")).expect("cache file");
    let file: serde_json::Value = serde_json::from_slice(&raw).expect("must parse as JSON");
    let entry_count = file
        .get("database")
        .and_then(serde_json::Value::as_object)
        .map(serde_json::Map::len)
        .unwrap_or(0);
    assert!(
        entry_count >= STEAM_ENTRY_COUNT_MIN,
        "expected at least {STEAM_ENTRY_COUNT_MIN} database entries, got {entry_count}"
    );
}

#[test]
#[ignore = "hits the real network; run explicitly, see this file's own doc comment"]
fn a_second_refresh_with_the_recorded_etag_gets_a_304() {
    let cache_dir = tempdir().expect("tempdir");
    let fetcher = rim_io::GithubRuleDatabaseFetcher::new();

    let first = fetcher.refresh(cache_dir.path(), &[RuleDatabase::CommunityRules]);
    assert!(matches!(first[0].1, RefreshOutcome::Updated { .. }));

    let second = fetcher.refresh(cache_dir.path(), &[RuleDatabase::CommunityRules]);
    assert!(
        matches!(second[0].1, RefreshOutcome::Unchanged { .. }),
        "a conditional GET against an unchanged real file must come back Unchanged, got {:?}",
        second[0].1
    );
}
