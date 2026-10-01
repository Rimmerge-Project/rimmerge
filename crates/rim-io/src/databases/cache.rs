//! [`DatabaseSpec`]/[`refresh`]/[`status`]: fetch -> validate -> commit
//! for one rule database, against the [`HttpGet`] seam so every test
//! proves this module's own logic with no socket ever opened. The
//! host+scheme allowlist check itself lives at the shared
//! [`crate::net::http::checked_get`] choke point, not here — every fetch
//! safety rule that isn't already the transport's job lives here: size
//! bounded twice, parse-before-commit, atomic replace, and bounded
//! failure text (`format!` with no response body ever interpolated in).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use rim_session::ports::{
    DatabaseStatus, FetchFailure, FetchFailureCause, RefreshOutcome, RuleDatabase,
};
use sha2::{Digest, Sha256};

use super::manifest::{FetchSuccess, Manifest};
use crate::atomic::write_atomically;
use crate::net::allowlist::{AllowedHost, Endpoint, RequestProfile, Timeouts};
use crate::net::http::{GetError, HttpError, HttpGet, HttpRequest};

/// Connect timeout for every rule-database fetch.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whole-request timeout for the two small files (community rules,
/// rimmerge-rules). **Dropped from 300 s to 60 s**: both now run
/// automatically in the background (`RefreshRuleDatabases::execute_automatic`),
/// so a stuck connection shouldn't be held for five minutes the way a
/// once-in-a-while manual click could tolerate.
const SMALL_FILE_TIMEOUT: Duration = Duration::from_secs(60);

/// Whole-request timeout for the Steam Workshop database — unchanged:
/// this source is manual-only regardless of size (see
/// `RuleDatabase::is_auto_refresh_eligible`), so the generous budget for a
/// slow connection on a ~49 MB file still applies.
const STEAM_WORKSHOP_TIMEOUT: Duration = Duration::from_secs(300);

/// One database's fetch parameters: its compiled-in [`Endpoint`] plus the
/// cache file name — all `const` — no setting, environment variable, or
/// CLI flag can change any of these.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DatabaseSpec {
    pub(crate) database: RuleDatabase,
    pub(crate) endpoint: Endpoint,
    pub(crate) file_name: &'static str,
}

impl DatabaseSpec {
    /// Runs the downloaded bytes through the *existing*
    /// `rimsort::rules_file`/`rimsort::steam_db` parser for this
    /// database, reused verbatim
    /// as a pure syntactic-validity check: an empty active-mod set means
    /// every relation in the file is reported "skipped" and the
    /// resulting `Rule`s are always empty, but a body that doesn't even
    /// deserialize into the parser's own shape still returns `Err` —
    /// which is all this cache needs to know before committing anything.
    /// The real, active-mod-filtered parse happens later, per profile,
    /// when `ImportRimSort` reads this cached file back — this cache has no
    /// active-mod list of its own to filter by (it's app-global, not
    /// per-profile).
    fn validates(&self, bytes: &[u8]) -> bool {
        match self.database {
            RuleDatabase::CommunityRules => crate::rimsort::rules_file::parse(
                bytes,
                rim_resolve::domain::RuleOrigin::RimSortCommunity,
                &BTreeSet::new(),
            )
            .is_ok(),
            RuleDatabase::SteamWorkshop => {
                crate::rimsort::steam_db::parse(bytes, &BTreeMap::new()).is_ok()
            }
            // Parsed by the same loader that will read it back at
            // `LoadProject` time (`crate::mod_knowledge`), so a body that
            // doesn't even deserialize into the envelope never reaches
            // the cache. Individual rows this binary doesn't understand
            // are *not* a parse failure — they are ignored with a warning
            // at load time, so a newer data file never breaks an older
            // binary.
            RuleDatabase::RimmergeRules => crate::mod_knowledge::parses(bytes),
        }
    }
}

pub(crate) const COMMUNITY_RULES: DatabaseSpec = DatabaseSpec {
    database: RuleDatabase::CommunityRules,
    endpoint: Endpoint {
        host: AllowedHost::GithubRaw,
        url: "https://raw.githubusercontent.com/RimSort/Community-Rules-Database/main/communityRules.json",
        // ~20x today's real 394 KB file.
        max_bytes: 8 * 1024 * 1024,
        timeouts: Timeouts {
            connect: CONNECT_TIMEOUT,
            whole_request: SMALL_FILE_TIMEOUT,
        },
        profile: RequestProfile::RawFile,
    },
    file_name: "communityRules.json",
};

pub(crate) const STEAM_WORKSHOP: DatabaseSpec = DatabaseSpec {
    database: RuleDatabase::SteamWorkshop,
    endpoint: Endpoint {
        host: AllowedHost::GithubRaw,
        url: "https://raw.githubusercontent.com/RimSort/Steam-Workshop-Database/main/steamDB.json",
        // ~4x today's real ~49 MB file.
        max_bytes: 192 * 1024 * 1024,
        timeouts: Timeouts {
            connect: CONNECT_TIMEOUT,
            whole_request: STEAM_WORKSHOP_TIMEOUT,
        },
        profile: RequestProfile::RawFile,
    },
    file_name: "steamDB.json",
};

/// This project's own rules file. **One file, not one per section**: the
/// rules repo authors its five sections (the four this binary reads, plus the
/// tag rules it does not) separately and its own CI concatenates them into
/// the single committed `rimmerge-rules.json` this URL serves — one URL, one sha, one manifest
/// row, one settings toggle, one cache file, one Databases-card row.
/// [`AllowedHost::GithubRaw`] is unchanged; no new host is introduced.
pub(crate) const RIMMERGE_RULES: DatabaseSpec = DatabaseSpec {
    database: RuleDatabase::RimmergeRules,
    endpoint: Endpoint {
        host: AllowedHost::GithubRaw,
        url: "https://raw.githubusercontent.com/Rimmerge-Project/rimmerge-rules/main/rimmerge-rules.json",
        // Generous headroom over a file that is a few KB today.
        max_bytes: 4 * 1024 * 1024,
        timeouts: Timeouts {
            connect: CONNECT_TIMEOUT,
            whole_request: SMALL_FILE_TIMEOUT,
        },
        profile: RequestProfile::RawFile,
    },
    file_name: "rimmergeRules.json",
};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Records `failure` as `database`'s failure in the manifest (best
/// effort — if even that write fails there is nothing further to do) and
/// returns the matching [`RefreshOutcome::Failed`]. The cache file itself
/// is never touched by this path.
fn fail(
    cache_dir: &Path,
    manifest: &mut Manifest,
    database: RuleDatabase,
    cause: FetchFailureCause,
    detail: String,
) -> RefreshOutcome {
    let failure = FetchFailure { cause, detail };
    manifest.record_failure(database, &failure, jiff::Timestamp::now());
    let _ = manifest.save(cache_dir);
    RefreshOutcome::Failed { failure }
}

/// Fetches, validates, and — on success — commits one database into
/// `cache_dir`. See this module's own doc comment for which numbered
/// safety rule each step below enforces.
pub(crate) fn refresh(
    http: &impl HttpGet,
    cache_dir: &Path,
    spec: &DatabaseSpec,
) -> RefreshOutcome {
    let mut manifest = Manifest::load(cache_dir);

    let etag = manifest.etag(spec.database).map(str::to_string);
    let response = match crate::net::http::checked_get(
        http,
        &HttpRequest {
            endpoint: &spec.endpoint,
            etag: etag.as_deref(),
        },
    ) {
        Ok(response) => response,
        // A refused request never reached a server, so "couldn't reach
        // GitHub" would be a false statement of what happened.
        Err(GetError::Refused(reason)) => {
            return fail(
                cache_dir,
                &mut manifest,
                spec.database,
                FetchFailureCause::Unclassified,
                reason,
            );
        }
        Err(GetError::Transport(HttpError(reason))) => {
            return fail(
                cache_dir,
                &mut manifest,
                spec.database,
                FetchFailureCause::Transport,
                reason,
            );
        }
    };

    if response.status == 304 {
        return match manifest.sha256(spec.database).map(str::to_string) {
            Some(sha256) => {
                manifest.record_unchanged(spec.database, jiff::Timestamp::now());
                let _ = manifest.save(cache_dir);
                RefreshOutcome::Unchanged { sha256 }
            }
            // A conditional GET only ever sends `If-None-Match` when a
            // previous success recorded an etag, so a compliant server
            // never returns 304 with nothing cached to confirm — treated
            // as a failure rather than fabricating a sha256 that was
            // never actually verified.
            None => fail(
                cache_dir,
                &mut manifest,
                spec.database,
                FetchFailureCause::NotModifiedWithoutCache,
                "server returned 304 Not Modified with no prior cached copy".to_string(),
            ),
        };
    }

    if response.status != 200 {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::HttpStatus {
                status: response.status,
            },
            format!("unexpected HTTP status {}", response.status),
        );
    }

    if let Some(length) = response.content_length
        && length > spec.endpoint.max_bytes
    {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::TooLarge,
            format!(
                "response declared {length} bytes, over the {} byte limit",
                spec.endpoint.max_bytes
            ),
        );
    }

    let mut bytes = Vec::new();
    let mut bounded = response.body.take(spec.endpoint.max_bytes + 1);
    if let Err(error) = bounded.read_to_end(&mut bytes) {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::ReadFailed,
            format!("reading response body: {error}"),
        );
    }
    if bytes.len() as u64 > spec.endpoint.max_bytes {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::TooLarge,
            format!(
                "response exceeded the {} byte limit",
                spec.endpoint.max_bytes
            ),
        );
    }

    if !spec.validates(&bytes) {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::InvalidContent,
            "downloaded file failed to parse".to_string(),
        );
    }

    let sha256 = sha256_hex(&bytes);
    if manifest.sha256(spec.database) == Some(sha256.as_str()) {
        // Same content, no matching etag (or none sent yet) — still
        // `Unchanged`, and the file on disk is never rewritten.
        manifest.record_unchanged(spec.database, jiff::Timestamp::now());
        let _ = manifest.save(cache_dir);
        return RefreshOutcome::Unchanged { sha256 };
    }

    let file_path = cache_dir.join(spec.file_name);
    if let Err(error) = write_atomically(&file_path, &bytes) {
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::CacheWriteFailed,
            format!("writing cache file: {error}"),
        );
    }

    manifest.record_updated(
        spec.database,
        &FetchSuccess {
            url: spec.endpoint.url,
            file_name: spec.file_name,
            sha256: sha256.clone(),
            etag: response.etag,
            bytes: bytes.len(),
            fetched_at: jiff::Timestamp::now(),
        },
    );
    if let Err(error) = manifest.save(cache_dir) {
        // The data file is already safely on disk, but claiming
        // `Updated` when the manifest write that records its sha/etag
        // didn't land would be dishonest — the next refresh simply
        // re-fetches (no etag to send), which is wasteful but never
        // wrong.
        return fail(
            cache_dir,
            &mut manifest,
            spec.database,
            FetchFailureCause::CacheWriteFailed,
            format!("writing manifest: {error}"),
        );
    }

    RefreshOutcome::Updated {
        sha256,
        bytes: bytes.len(),
    }
}

/// Reads `spec`'s cache status from the manifest — no network.
pub(crate) fn status(cache_dir: &Path, spec: &DatabaseSpec, enabled: bool) -> DatabaseStatus {
    let manifest = Manifest::load(cache_dir);
    DatabaseStatus {
        database: spec.database,
        enabled,
        path: cache_dir.join(spec.file_name),
        cached: manifest.cached(spec.database),
        last_failure: manifest.last_failure(spec.database),
        last_attempt_at: manifest.last_attempt_at(spec.database),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::net::http::fake::{FakeCall, FakeHttpGet};

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join(name),
        )
        .expect("fixture must exist")
    }

    #[test]
    fn a_successful_fetch_writes_the_cache_file_and_records_the_manifest() {
        let dir = tempdir().expect("tempdir");
        let body = fixture("communityRules.json");
        let http = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: Some("\"real-etag\"".to_string()),
            content_length: Some(body.len() as u64),
            rate_limit: None,
            retry_after: None,
            body: body.clone(),
        }]);

        let outcome = refresh(&http, dir.path(), &COMMUNITY_RULES);

        match outcome {
            RefreshOutcome::Updated { sha256, bytes } => {
                assert_eq!(bytes, body.len());
                assert_eq!(sha256, sha256_hex(&body));
            }
            other => panic!("expected Updated, got {other:?}"),
        }
        assert_eq!(
            fs::read(dir.path().join("communityRules.json")).expect("cache file must exist"),
            body
        );
        let status = status(dir.path(), &COMMUNITY_RULES, true);
        let cached = status.cached.expect("must be cached after a success");
        assert_eq!(cached.bytes, body.len());
        assert!(status.last_failure.is_none());
    }

    #[test]
    fn a_304_response_leaves_the_file_untouched_and_advances_fetched_at() {
        let dir = tempdir().expect("tempdir");
        let body = fixture("communityRules.json");
        let first = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: Some("\"real-etag\"".to_string()),
            content_length: Some(body.len() as u64),
            rate_limit: None,
            retry_after: None,
            body: body.clone(),
        }]);
        refresh(&first, dir.path(), &COMMUNITY_RULES);
        let before = status(dir.path(), &COMMUNITY_RULES, true)
            .cached
            .expect("cached after first fetch");

        let second = FakeHttpGet::new(vec![FakeCall::Response {
            status: 304,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]);
        let outcome = refresh(&second, dir.path(), &COMMUNITY_RULES);

        assert_eq!(
            outcome,
            RefreshOutcome::Unchanged {
                sha256: before.sha256.clone()
            }
        );
        assert_eq!(
            fs::read(dir.path().join("communityRules.json")).expect("file must be untouched"),
            body
        );
        assert_eq!(
            second.requests()[0].1.as_deref(),
            Some("\"real-etag\""),
            "the second call must send back the etag the first call recorded"
        );
    }

    #[test]
    fn identical_body_with_no_etag_is_unchanged_and_the_file_is_not_rewritten() {
        let dir = tempdir().expect("tempdir");
        let body = fixture("communityRules.json");
        let first = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(body.len() as u64),
            rate_limit: None,
            retry_after: None,
            body: body.clone(),
        }]);
        refresh(&first, dir.path(), &COMMUNITY_RULES);
        let file_path = dir.path().join("communityRules.json");
        let mtime_before = fs::metadata(&file_path).expect("metadata").modified().ok();

        let second = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(body.len() as u64),
            rate_limit: None,
            retry_after: None,
            body: body.clone(),
        }]);
        let outcome = refresh(&second, dir.path(), &COMMUNITY_RULES);

        assert!(matches!(outcome, RefreshOutcome::Unchanged { .. }));
        if let (Some(before), Ok(after)) = (
            mtime_before,
            fs::metadata(&file_path).and_then(|m| m.modified()),
        ) {
            assert_eq!(before, after, "the file must not be rewritten");
        }
    }

    #[test]
    fn a_body_that_fails_to_parse_never_replaces_a_good_cache_file() {
        let dir = tempdir().expect("tempdir");
        let good = fixture("communityRules.json");
        let first = FakeHttpGet::new(vec![FakeCall::ok(good.clone())]);
        refresh(&first, dir.path(), &COMMUNITY_RULES);

        let second = FakeHttpGet::new(vec![FakeCall::ok(b"not valid json at all".to_vec())]);
        let outcome = refresh(&second, dir.path(), &COMMUNITY_RULES);

        assert!(matches!(outcome, RefreshOutcome::Failed { .. }));
        assert_eq!(
            fs::read(dir.path().join("communityRules.json")).expect("old file must survive"),
            good,
            "a body that fails to parse must never overwrite a good cache file"
        );
        let last_failure = status(dir.path(), &COMMUNITY_RULES, true).last_failure;
        assert!(last_failure.is_some());
    }

    #[test]
    fn an_oversized_content_length_is_rejected_without_reading_the_body() {
        let dir = tempdir().expect("tempdir");
        let http = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(COMMUNITY_RULES.endpoint.max_bytes + 1),
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]);

        let outcome = refresh(&http, dir.path(), &COMMUNITY_RULES);

        assert!(matches!(outcome, RefreshOutcome::Failed { .. }));
        assert!(!dir.path().join("communityRules.json").exists());
    }

    #[test]
    fn a_lying_content_length_with_an_over_limit_body_is_rejected_by_the_take_bound() {
        let dir = tempdir().expect("tempdir");
        let spec = DatabaseSpec {
            endpoint: Endpoint {
                max_bytes: 16,
                ..COMMUNITY_RULES.endpoint
            },
            ..COMMUNITY_RULES
        };
        let oversized_body = vec![b'a'; 64];
        let http = FakeHttpGet::new(vec![FakeCall::Response {
            status: 200,
            etag: None,
            content_length: Some(4), // lies about the real size
            rate_limit: None,
            retry_after: None,
            body: oversized_body,
        }]);

        let outcome = refresh(&http, dir.path(), &spec);

        assert!(matches!(outcome, RefreshOutcome::Failed { .. }));
        assert!(!dir.path().join(spec.file_name).exists());
    }

    #[test]
    fn a_non_allowlisted_host_is_rejected_without_ever_calling_the_transport() {
        let dir = tempdir().expect("tempdir");
        let spec = DatabaseSpec {
            endpoint: Endpoint {
                url: "https://evil.example/communityRules.json",
                ..COMMUNITY_RULES.endpoint
            },
            ..COMMUNITY_RULES
        };
        let http = FakeHttpGet::new(Vec::new());

        let outcome = refresh(&http, dir.path(), &spec);

        let RefreshOutcome::Failed { failure } = outcome else {
            panic!("expected a failure, got {outcome:?}");
        };
        assert_eq!(
            failure.cause,
            FetchFailureCause::Unclassified,
            "a refused request never reached GitHub"
        );
        assert_eq!(http.call_count(), 0, "the transport must never be reached");
    }

    #[test]
    fn a_plain_http_url_is_rejected_without_ever_calling_the_transport() {
        let dir = tempdir().expect("tempdir");
        let spec = DatabaseSpec {
            endpoint: Endpoint {
                url: "http://raw.githubusercontent.com/RimSort/Community-Rules-Database/main/communityRules.json",
                ..COMMUNITY_RULES.endpoint
            },
            ..COMMUNITY_RULES
        };
        let http = FakeHttpGet::new(Vec::new());

        let outcome = refresh(&http, dir.path(), &spec);

        assert!(matches!(outcome, RefreshOutcome::Failed { .. }));
        assert_eq!(http.call_count(), 0);
    }

    #[test]
    fn a_redirect_status_is_a_failure_not_a_hop() {
        let dir = tempdir().expect("tempdir");
        let http = FakeHttpGet::new(vec![FakeCall::Response {
            status: 302,
            etag: None,
            content_length: None,
            rate_limit: None,
            retry_after: None,
            body: Vec::new(),
        }]);

        let outcome = refresh(&http, dir.path(), &COMMUNITY_RULES);

        assert!(matches!(outcome, RefreshOutcome::Failed { .. }));
        assert!(!dir.path().join("communityRules.json").exists());
    }

    #[test]
    fn a_transport_failure_is_reported_without_touching_the_cache() {
        let dir = tempdir().expect("tempdir");
        let http = FakeHttpGet::new(vec![FakeCall::Failure("dns error".to_string())]);

        let outcome = refresh(&http, dir.path(), &COMMUNITY_RULES);

        assert_eq!(
            outcome,
            RefreshOutcome::Failed {
                failure: FetchFailure {
                    cause: FetchFailureCause::Transport,
                    detail: "dns error".to_string()
                }
            }
        );
        assert!(!dir.path().join("communityRules.json").exists());
    }

    #[test]
    fn status_on_an_empty_cache_reports_never_fetched() {
        let dir = tempdir().expect("tempdir");

        let status = status(dir.path(), &COMMUNITY_RULES, false);

        assert_eq!(status.database, RuleDatabase::CommunityRules);
        assert!(!status.enabled);
        assert!(status.cached.is_none());
        assert!(status.last_failure.is_none());
    }

    #[test]
    fn the_real_steam_workshop_fixture_validates_through_the_same_path() {
        let dir = tempdir().expect("tempdir");
        let body = fixture("steamDB.json");
        let http = FakeHttpGet::new(vec![FakeCall::ok(body.clone())]);

        let outcome = refresh(&http, dir.path(), &STEAM_WORKSHOP);

        assert!(matches!(outcome, RefreshOutcome::Updated { .. }));
        assert_eq!(
            fs::read(dir.path().join("steamDB.json")).expect("cache file"),
            body
        );
    }

    // The allowlist's own host/scheme parsing edge cases (userinfo,
    // subdomain, and plain-http lookalikes) are covered once, in
    // `crate::net::allowlist`'s own tests — this module's
    // `a_non_allowlisted_host_is_rejected_...` and
    // `a_plain_http_url_is_rejected_...` tests above cover the
    // integration with `refresh` instead of restating those cases here.
}
