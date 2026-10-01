//! Real-network verification: GETs the real
//! `GET /repos/Rimmerge-Project/rimmerge/releases/latest` for real, over
//! HTTPS, through the real [`rim_io::GithubReleaseFeed`] — sits beside
//! `real_network_databases.rs` in the same style, the other test in this
//! crate that actually opens a socket.
//!
//! `#[ignore]`d so `cargo nextest run --workspace --all-features` (the
//! default gate) never depends on network access or GitHub's own
//! availability, per the root `CLAUDE.md`'s network hard rule. Run
//! explicitly, on its own, with:
//! `cargo nextest run -p rim-io --all-features --run-ignored ignored-only -E 'binary(real_network_release_feed)'`
//!
//! **Before this project's first published release, the first test
//! below fails with `ReleaseFeedError::NotPublished`** (GitHub's own
//! `404` for a repository with no releases yet) — that is the correct,
//! expected result until a release exists, not a bug in this test or in
//! `rim_io::GithubReleaseFeed`.

use rim_session::ports::{FeedResponse, ReleaseFeed};

#[test]
#[ignore = "hits the real network; run explicitly, see this file's own doc comment"]
fn the_latest_release_fetches_and_parses_as_a_stable_semver() {
    let feed = rim_io::GithubReleaseFeed::new();

    let response = feed.latest(None).unwrap_or_else(|error| {
        panic!(
            "expected a published release to fetch and parse; got {error} — see this file's own \
             doc comment if this project has no release yet"
        )
    });

    // A stable semver, never an exact pinned version — this is a live,
    // changing third-party (well, our own, but still moving) value.
    let FeedResponse::Fresh { release, .. } = response else {
        panic!("expected Fresh on a first, unconditional request, got {response:?}");
    };
    let version = release.version.to_string();
    assert!(
        version
            .trim_start_matches('v')
            .split('.')
            .all(|part| part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty()),
        "expected a plain X.Y.Z semver (after stripping a leading 'v'), got {version}"
    );
}

#[test]
#[ignore = "hits the real network; run explicitly, see this file's own doc comment"]
fn a_second_request_with_the_recorded_etag_gets_a_304() {
    let feed = rim_io::GithubReleaseFeed::new();

    let first = feed.latest(None).unwrap_or_else(|error| {
        panic!(
            "expected a published release to fetch and parse; got {error} — see this file's own \
             doc comment if this project has no release yet"
        )
    });
    let FeedResponse::Fresh { etag, .. } = first else {
        panic!("expected Fresh on a first, unconditional request, got {first:?}");
    };
    let etag = etag.expect("GitHub's releases API sends an ETag on every response");

    let second = feed.latest(Some(&etag));
    assert!(
        matches!(second, Ok(FeedResponse::NotModified)),
        "a conditional GET with an unchanged ETag must come back NotModified, got {second:?}"
    );
}
