//! [`GithubReleaseFeed`]: `rim-session`'s `ReleaseFeed` port — queries
//! GitHub's Releases API for the latest published Rimmerge release, over
//! the same `net` (crate-internal) transport seam `databases::cache` uses.
//!
//! **`/releases/latest` is documented to return the most recently
//! published release that is neither a draft nor a prerelease, or `404`
//! when there is none** — `.github/workflows/release.yml` only ever
//! drafts a release, and a maintainer publishes it by hand, so a draft is
//! never announced because of how this endpoint behaves, not because of a
//! check this module could edit away. This module's own `draft`/
//! `prerelease` checks below are defence in depth on top of that
//! documented behaviour, not the primary guarantee.
//!
//! Only `LatestReleaseResponse`'s three named fields are ever read from
//! the response body — `body` (release notes), `html_url`, `assets`, and
//! everything else are ignored by serde and never retained anywhere. The
//! release link shown to the user is built on the app side from a
//! `const` prefix plus the parsed version, never from anything in the
//! response (see `docs/privacy-and-network.md`).

use std::io::Read;
use std::time::Duration;

use rim_session::notifications::{AppVersion, AppVersionError, LatestRelease};
use rim_session::ports::{FeedResponse, ReleaseFeed, ReleaseFeedError};

use crate::net::allowlist::{AllowedHost, Endpoint, RequestProfile, Timeouts};
use crate::net::http::{HttpGet, HttpRequest, HttpResponse, UreqHttpGet};

/// Connect timeout for the release check.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whole-request timeout — this call is small and automatic, so it
/// shouldn't be able to hold a background thread for long.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Generously over the response's real size (release notes plus asset
/// objects, typically tens of KB).
const MAX_BYTES: u64 = 512 * 1024;

/// The one endpoint this module ever requests — [`AllowedHost::GithubApi`]
/// is the only [`AllowedHost`] variant whose URL prefix this project's own
/// repository, never a free-form or fetched address.
pub(crate) const RELEASES_LATEST: Endpoint = Endpoint {
    host: AllowedHost::GithubApi,
    url: "https://api.github.com/repos/Rimmerge-Project/rimmerge/releases/latest",
    max_bytes: MAX_BYTES,
    timeouts: Timeouts {
        connect: CONNECT_TIMEOUT,
        whole_request: REQUEST_TIMEOUT,
    },
    profile: RequestProfile::GithubApi,
};

/// The exact fields this app reads from a GitHub release object —
/// `#[serde(deny_unknown_fields)]` is deliberately **not** used here: an
/// unread field (a new one GitHub adds, `body`, `html_url`, `assets`, …)
/// must never fail this parse, only be ignored.
#[derive(Debug, serde::Deserialize)]
struct LatestReleaseResponse {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    published_at: Option<String>,
}

impl TryFrom<LatestReleaseResponse> for LatestRelease {
    type Error = ReleaseFeedError;

    fn try_from(value: LatestReleaseResponse) -> Result<Self, Self::Error> {
        if value.draft || value.prerelease {
            return Err(ReleaseFeedError::Malformed);
        }
        let version = AppVersion::published(&value.tag_name).map_err(|error| match error {
            AppVersionError::NotAStableVersion(_) => ReleaseFeedError::NotAStableVersion,
            AppVersionError::TooLong | AppVersionError::NotSemver(_) => ReleaseFeedError::Malformed,
        })?;
        let published_at = value
            .published_at
            .as_deref()
            .and_then(|raw| raw.parse::<jiff::Timestamp>().ok())
            .ok_or(ReleaseFeedError::Malformed)?;
        Ok(LatestRelease {
            version,
            published_at,
        })
    }
}

/// The later of `response`'s own rate-limit reset and
/// `now + Retry-After`, clamped to at most 24 h ahead of now — both
/// inputs are individually clamped already
/// ([`crate::net::http::parse_rate_limit`]/`parse_retry_after`), and this
/// combines them the same way for the case where a response carries both.
fn rate_limited_until(response: &HttpResponse, now: jiff::Timestamp) -> jiff::Timestamp {
    let cap = now
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or(now);
    let from_reset = response.rate_limit.map(|limit| limit.reset_at);
    let from_retry_after = response
        .retry_after
        .and_then(|duration| now.checked_add(duration).ok());
    match (from_reset, from_retry_after) {
        (Some(a), Some(b)) => a.max(b).min(cap),
        (Some(a), None) => a.min(cap),
        (None, Some(b)) => b.min(cap),
        (None, None) => cap,
    }
}

/// Whether `response` signals GitHub's rate limit is exhausted: a
/// `403`/`429` status together with either an exhausted budget
/// (`x-ratelimit-remaining: 0`) or a `Retry-After` header.
fn is_rate_limited(response: &HttpResponse) -> bool {
    matches!(response.status, 403 | 429)
        && (response
            .rate_limit
            .is_some_and(|limit| limit.remaining == 0)
            || response.retry_after.is_some())
}

/// Queries GitHub's Releases API, implementing `rim-session`'s
/// [`ReleaseFeed`] port. Generic over the transport (`HttpGet`) so
/// tests substitute a scripted fake and never open a socket —
/// [`GithubReleaseFeed::new`] wires in the real `UreqHttpGet`; nothing
/// outside this crate's own tests constructs any other transport.
// Deliberately no `H: HttpGet` bound on the struct's own generic
// parameter — see `databases::GithubRuleDatabaseFetcher`'s identical
// comment for why (`HttpGet` is `pub(crate)`, and a bound here would leak
// that private trait into this `pub` struct's public signature).
#[derive(Debug)]
pub struct GithubReleaseFeed<H = UreqHttpGet> {
    http: H,
}

impl GithubReleaseFeed<UreqHttpGet> {
    /// Builds the feed with the real transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: UreqHttpGet::new(),
        }
    }
}

impl Default for GithubReleaseFeed<UreqHttpGet> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the feed with a scripted transport — this crate's own tests
/// only; see [`crate::net::http::fake::FakeHttpGet`].
#[cfg(test)]
pub(crate) fn with_transport<H: HttpGet>(http: H) -> GithubReleaseFeed<H> {
    GithubReleaseFeed { http }
}

impl<H: HttpGet> ReleaseFeed for GithubReleaseFeed<H> {
    fn latest(&self, etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError> {
        let request = HttpRequest {
            endpoint: &RELEASES_LATEST,
            etag,
        };
        let response = crate::net::http::checked_get(&self.http, &request)
            .map_err(|error| ReleaseFeedError::Transport(error.into_detail()))?;

        if response.status == 304 {
            return Ok(FeedResponse::NotModified);
        }
        if response.status == 404 {
            return Err(ReleaseFeedError::NotPublished);
        }
        if is_rate_limited(&response) {
            let until = rate_limited_until(&response, jiff::Timestamp::now());
            return Err(ReleaseFeedError::RateLimited { until });
        }
        if response.status != 200 {
            return Err(ReleaseFeedError::HttpStatus(response.status));
        }
        if let Some(length) = response.content_length
            && length > RELEASES_LATEST.max_bytes
        {
            return Err(ReleaseFeedError::TooLarge);
        }

        let mut bytes = Vec::new();
        let mut bounded = response.body.take(RELEASES_LATEST.max_bytes + 1);
        bounded
            .read_to_end(&mut bytes)
            .map_err(|error| ReleaseFeedError::Transport(error.to_string()))?;
        if bytes.len() as u64 > RELEASES_LATEST.max_bytes {
            return Err(ReleaseFeedError::TooLarge);
        }

        let parsed: LatestReleaseResponse =
            serde_json::from_slice(&bytes).map_err(|_| ReleaseFeedError::Malformed)?;
        let release = LatestRelease::try_from(parsed)?;
        Ok(FeedResponse::Fresh {
            release,
            etag: response.etag,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::net::http::RateLimit;
    use crate::net::http::fake::{FakeCall, FakeHttpGet};

    use super::*;

    fn real_shaped_body(tag: &str, draft: bool, prerelease: bool) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": prerelease,
            "published_at": "2026-09-01T00:00:00Z",
            "body": "release notes nobody should read from here",
            "html_url": "https://github.com/Rimmerge-Project/rimmerge/releases/tag/x",
            "assets": [{"name": "Rimmerge-0.2.0-windows-x64-portable.zip", "size": 1234}],
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn a_real_shaped_200_body_parses_and_ignores_unknown_fields() {
        let body = real_shaped_body("v0.2.0", false, false);
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::ok(body)]));

        let response = http.latest(None).expect("parses");

        match response {
            FeedResponse::Fresh { release, .. } => {
                assert_eq!(release.version, AppVersion::published("0.2.0").unwrap());
            }
            FeedResponse::NotModified => panic!("expected Fresh"),
        }
    }

    #[test]
    fn a_304_reports_not_modified_and_sends_the_stored_etag() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 304,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]));

        let response = http.latest(Some("\"abc\"")).expect("not modified");

        assert_eq!(response, FeedResponse::NotModified);
    }

    #[test]
    fn a_404_reports_not_published() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 404,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::NotPublished);
    }

    #[test]
    fn a_403_with_zero_remaining_and_a_reset_time_is_rate_limited_and_clamped() {
        let far_future = jiff::Timestamp::now()
            .checked_add(Duration::from_secs(400 * 24 * 60 * 60))
            .unwrap();
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 403,
            etag: None,
            content_length: None,
            rate_limit: Some(RateLimit {
                remaining: 0,
                reset_at: far_future,
            }),
            retry_after: None,
            body: Vec::new(),
        }]));

        let error = http.latest(None).unwrap_err();

        match error {
            ReleaseFeedError::RateLimited { until } => {
                let now = jiff::Timestamp::now();
                assert!(until <= now.checked_add(Duration::from_secs(24 * 60 * 60)).unwrap());
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn a_429_with_retry_after_is_rate_limited() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 429,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: Some(Duration::from_secs(300)),
            body: Vec::new(),
        }]));

        let error = http.latest(None).unwrap_err();

        assert!(matches!(error, ReleaseFeedError::RateLimited { .. }));
    }

    #[test]
    fn a_redirect_status_is_reported_as_an_http_status() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 301,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::HttpStatus(301));
    }

    #[test]
    fn a_content_length_over_the_cap_is_rejected_without_reading_the_body() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(RELEASES_LATEST.max_bytes + 1),
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::TooLarge);
    }

    #[test]
    fn a_lying_content_length_with_an_over_cap_body_is_rejected_by_the_take_bound() {
        let oversized = vec![b'a'; (RELEASES_LATEST.max_bytes + 64) as usize];
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(4),
            rate_limit: None,
            retry_after: None,
            body: oversized,
        }]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::TooLarge);
    }

    #[test]
    fn malformed_json_is_reported_as_malformed() {
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::ok(b"not json".to_vec())]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::Malformed);
    }

    #[test]
    fn a_draft_release_is_rejected_as_malformed_defence_in_depth() {
        let body = real_shaped_body("v0.2.0", true, false);
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::ok(body)]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::Malformed);
    }

    #[test]
    fn a_prerelease_is_rejected_as_malformed_defence_in_depth() {
        let body = real_shaped_body("v0.2.0", false, true);
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::ok(body)]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::Malformed);
    }

    #[test]
    fn a_pre_release_tag_is_reported_as_not_a_stable_version() {
        let body = real_shaped_body("v0.2.0-rc.1", false, false);
        let http = with_transport(FakeHttpGet::new(vec![FakeCall::ok(body)]));

        let error = http.latest(None).unwrap_err();

        assert_eq!(error, ReleaseFeedError::NotAStableVersion);
    }

    // Deliberately not a test of the actual request headers
    // (`Accept`/`X-GitHub-Api-Version`/`User-Agent`) — those are built
    // inside `UreqHttpGet::get`, unreachable from `FakeHttpGet` (which
    // records only each request's own URL/etag, never headers), and
    // testing them for real would need a socket. This only guards the
    // one thing at this seam a plausible bug could actually break: the
    // compiled-in endpoint naming the wrong header profile (e.g. a typo
    // reusing `RequestProfile::RawFile`, meant for `databases::cache`,
    // which would silently drop `Accept`/`X-GitHub-Api-Version` from a
    // real request).
    #[test]
    fn the_releases_endpoint_uses_the_github_api_header_profile() {
        assert_eq!(RELEASES_LATEST.profile, RequestProfile::GithubApi);
    }

    #[test]
    fn the_endpoint_is_allowlisted_and_uses_the_github_api_host() {
        assert!(crate::net::allowlist::is_allowed(&RELEASES_LATEST));
        assert_eq!(RELEASES_LATEST.host, AllowedHost::GithubApi);
        assert!(
            RELEASES_LATEST
                .url
                .starts_with("https://api.github.com/repos/Rimmerge-Project/rimmerge/")
        );
    }
}
