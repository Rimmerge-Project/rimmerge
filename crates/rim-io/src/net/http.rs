//! [`HttpGet`]: the network seam every fetch in this crate goes through —
//! a `pub(crate)` transport trait with exactly one production implementor
//! ([`UreqHttpGet`]) and one test fake ([`fake::FakeHttpGet`], below), so
//! every test in this crate proves its caller's own logic (allowlist,
//! size bounds, parse-before-commit, `304` handling, rate-limit parsing,
//! ...) with no socket ever opened — the default `cargo nextest run
//! --workspace --all-features` gate stays hermetic.

use std::io::Read;
use std::time::Duration;

use jiff::Timestamp;
use ureq::Agent;
use ureq::http::HeaderMap;
use ureq::tls::{RootCerts, TlsConfig};

use super::allowlist::{self, Endpoint, RequestProfile};

/// GitHub's remaining-request budget for the window a response's own
/// `x-ratelimit-remaining`/`x-ratelimit-reset` headers describe.
/// `reset_at` is clamped to at most 24 h ahead of when it was parsed, so a
/// hostile or broken header can never park a check further out than that
/// — see [`parse_rate_limit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RateLimit {
    pub(crate) remaining: u32,
    pub(crate) reset_at: Timestamp,
}

/// One GET's outcome from the transport's own point of view: a status
/// code plus the headers a caller needs (`ETag`, `Content-Length`, the
/// rate-limit budget, `Retry-After`), and a body reader that isn't
/// buffered here — the caller bounds it itself with `Read::take`, so a
/// lying `Content-Length` can't force an allocation before the caller
/// ever gets to check anything.
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) etag: Option<String>,
    pub(crate) content_length: Option<u64>,
    pub(crate) rate_limit: Option<RateLimit>,
    pub(crate) retry_after: Option<Duration>,
    pub(crate) body: Box<dyn Read>,
}

/// A transport-level failure: DNS, connect, TLS, or a timeout. Never
/// constructed for an HTTP error status — those come back as an ordinary
/// [`HttpResponse`] with that status (`http_status_as_error` is off), so
/// a caller can tell "the server said no" from "never reached a server"
/// apart if it ever needs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpError(pub(crate) String);

/// Why [`checked_get`] returned no response: the request never left the
/// process, or the transport failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GetError {
    /// The endpoint is not on the allowlist; the transport was never called.
    Refused(String),
    /// The transport failed.
    Transport(HttpError),
}

impl GetError {
    /// The English text, for a technical-details line.
    pub(crate) fn into_detail(self) -> String {
        match self {
            Self::Refused(detail) | Self::Transport(HttpError(detail)) => detail,
        }
    }
}

/// One request: which compiled-in [`Endpoint`] to hit, plus an optional
/// stored `ETag` to send as `If-None-Match`. There is nothing else a
/// caller can set — no arbitrary header, no different URL, no different
/// host than the one `endpoint.host` names.
pub(crate) struct HttpRequest<'a> {
    pub(crate) endpoint: &'a Endpoint,
    pub(crate) etag: Option<&'a str>,
}

/// The network seam. An implementation never follows redirects and never
/// retries — both are the caller's call, not the transport's.
pub(crate) trait HttpGet {
    /// Sends `request`, honouring its endpoint's own timeouts, headers,
    /// and (when given) conditional-GET `ETag`.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError`] only for a transport-level failure. An HTTP
    /// error status (`4xx`/`5xx`), a redirect (`3xx`), and `304` all come
    /// back as `Ok` — the caller decides what each status means.
    fn get(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, HttpError>;
}

/// The one choke point both `databases::cache` and `release_feed` send
/// every request through — checks `request.endpoint` against
/// [`allowlist::is_allowed`] before ever calling `http.get`, so no caller
/// can forget the check (unlike a per-caller pre-check, which a new
/// caller could simply omit). Never actually false for any of this
/// crate's own compiled-in `Endpoint` constants — defence in depth — and
/// testable through `fake::FakeHttpGet` with no socket ever opened, since
/// `http: &impl HttpGet` accepts either transport identically.
///
/// # Errors
///
/// Returns [`GetError::Refused`] when the allowlist check fails (`http.get`
/// is never called), or [`GetError::Transport`] with whatever `http.get`
/// itself returns.
pub(crate) fn checked_get(
    http: &impl HttpGet,
    request: &HttpRequest<'_>,
) -> Result<HttpResponse, GetError> {
    if !allowlist::is_allowed(request.endpoint) {
        return Err(GetError::Refused(format!(
            "refusing to fetch from a non-allowlisted URL: {}",
            request.endpoint.url
        )));
    }
    http.get(request).map_err(GetError::Transport)
}

/// The real transport. Every call builds a fresh, one-shot [`Agent`]
/// rather than sharing a lazily-built one: even with the update check
/// added, this runs a handful of times per launch at most, so paying
/// connection-setup cost each time is not worth the added state of a
/// shared, reused agent.
#[derive(Debug, Default, Clone, Copy)]
pub struct UreqHttpGet;

impl UreqHttpGet {
    /// Builds the transport. Stateless — every call configures and uses
    /// its own [`Agent`], sized to that call's own endpoint.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The one place every fetch safety rule that belongs at the transport
    /// level is configured: HTTPS only (enforced here in addition to the
    /// URL constants already being `https://`), TLS verification against
    /// the Windows system trust store rather than a bundled root list
    /// (`RootCerts::PlatformVerifier` — see the workspace `Cargo.toml`'s
    /// own comment on the `ureq` dependency for why: the bundled
    /// alternative's `webpki-roots` crate fails `cargo deny check`), never
    /// disabled, no redirects followed, `endpoint`'s own connect/
    /// whole-request timeouts, and raw status codes returned as `Ok`
    /// (`http_status_as_error(false)`) so a `3xx`/`4xx`/`5xx` is inspected
    /// by the caller rather than turned into an `Err` that loses the
    /// status code.
    fn agent(endpoint: &Endpoint) -> Agent {
        let tls_config = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let config = Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_connect(Some(endpoint.timeouts.connect))
            .timeout_global(Some(endpoint.timeouts.whole_request))
            .http_status_as_error(false)
            .tls_config(tls_config)
            .build();
        Agent::new_with_config(config)
    }
}

impl HttpGet for UreqHttpGet {
    fn get(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, HttpError> {
        let endpoint = request.endpoint;
        let agent = Self::agent(endpoint);
        let mut builder = agent
            .get(endpoint.url)
            // A bare product name, the same for every user and every
            // version — see `docs/privacy-and-network.md` and this
            // crate's own `CLAUDE.md`. ureq's own default is
            // `ureq/<version>`; GitHub's API requires a `User-Agent` at
            // all, so this is set explicitly for both profiles rather
            // than relying on ureq's default staying acceptable.
            .header("User-Agent", "rimmerge");
        if let Some(etag) = request.etag {
            builder = builder.header("If-None-Match", etag);
        }
        if matches!(endpoint.profile, RequestProfile::GithubApi) {
            builder = builder
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
        }
        let response = builder.call().map_err(|e| HttpError(e.to_string()))?;

        let status = response.status().as_u16();
        let headers = response.headers();
        let etag = header_str(headers, "etag").map(str::to_string);
        let rate_limit = parse_rate_limit(headers);
        let retry_after = parse_retry_after(headers);
        let content_length = response.body().content_length();
        let body = response.into_body().into_reader();

        Ok(HttpResponse {
            status,
            etag,
            content_length,
            rate_limit,
            retry_after,
            body: Box::new(body),
        })
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Parses `x-ratelimit-remaining`/`x-ratelimit-reset` (Unix seconds) into
/// a [`RateLimit`], clamping `reset_at` to at most 24 h ahead of now. A
/// missing or malformed header pair — including a `reset_at` jiff itself
/// refuses to represent — produces `None` rather than a default value a
/// caller could mistake for a real budget.
fn parse_rate_limit(headers: &HeaderMap) -> Option<RateLimit> {
    let remaining: u32 = header_str(headers, "x-ratelimit-remaining")?.parse().ok()?;
    let reset_epoch: i64 = header_str(headers, "x-ratelimit-reset")?.parse().ok()?;
    let reset_at = Timestamp::from_second(reset_epoch).ok()?;
    let cap = Timestamp::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .ok()?;
    Some(RateLimit {
        remaining,
        reset_at: reset_at.min(cap),
    })
}

/// Parses `Retry-After` in its seconds form (GitHub never sends the
/// HTTP-date form for this endpoint), clamped to at most 24 h.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds: u64 = header_str(headers, "retry-after")?.parse().ok()?;
    Some(Duration::from_secs(seconds.min(24 * 60 * 60)))
}

/// A scripted [`HttpGet`] fake — no [`ureq`] involved at all, so nothing
/// under `#[cfg(test)]` in this crate can accidentally open a socket.
#[cfg(test)]
pub(crate) mod fake {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io::Cursor;

    use super::{Duration, HttpError, HttpGet, HttpRequest, HttpResponse, RateLimit};

    /// One scripted response (or transport failure) [`FakeHttpGet`] hands
    /// back for its next [`HttpGet::get`] call, in the order given to
    /// [`FakeHttpGet::new`].
    pub(crate) enum FakeCall {
        Response {
            status: u16,
            etag: Option<String>,
            content_length: Option<u64>,
            rate_limit: Option<RateLimit>,
            retry_after: Option<Duration>,
            body: Vec<u8>,
        },
        Failure(String),
    }

    impl FakeCall {
        /// A plain `200` with `body`'s own length as `Content-Length` and
        /// no `ETag`/rate-limit/retry headers — the common case for most
        /// tests.
        pub(crate) fn ok(body: impl Into<Vec<u8>>) -> Self {
            let body = body.into();
            Self::Response {
                status: 200,
                etag: None,
                content_length: Some(body.len() as u64),
                rate_limit: None,
                retry_after: None,
                body,
            }
        }
    }

    /// A scripted [`HttpGet`] fake: queues one [`FakeCall`] per expected
    /// request, in order, and records every request it was asked for —
    /// the whole [`HttpRequest`] shape (endpoint URL and etag), not just
    /// the URL, so a test can assert the exact headers/profile a call
    /// would have produced.
    ///
    /// [`FakeHttpGet::call_count`] is the load-bearing assertion for the
    /// network-off switch: an outcome-only test would still pass if that
    /// guard moved into the adapter and the port were called anyway, so
    /// the offline-switch test asserts this count is zero, not merely
    /// that the outcome says `Skipped`.
    ///
    /// `RefCell`, not a lock: every test drives this from one thread, and
    /// [`HttpGet`] itself carries no `Send`/`Sync` bound (composition
    /// roots add that at the `dyn` usage site if they ever need it, the
    /// same convention `AssetLocator`/`DefSourceReader` already use).
    pub(crate) struct FakeHttpGet {
        calls: RefCell<VecDeque<FakeCall>>,
        requests: RefCell<Vec<(String, Option<String>)>>,
    }

    impl FakeHttpGet {
        pub(crate) fn new(calls: Vec<FakeCall>) -> Self {
            Self {
                calls: RefCell::new(calls.into()),
                requests: RefCell::new(Vec::new()),
            }
        }

        /// How many times [`HttpGet::get`] was actually called.
        pub(crate) fn call_count(&self) -> usize {
            self.requests.borrow().len()
        }

        /// Every `(url, etag)` pair requested so far, in order.
        pub(crate) fn requests(&self) -> Vec<(String, Option<String>)> {
            self.requests.borrow().clone()
        }
    }

    impl HttpGet for FakeHttpGet {
        fn get(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, HttpError> {
            self.requests.borrow_mut().push((
                request.endpoint.url.to_string(),
                request.etag.map(str::to_string),
            ));
            match self.calls.borrow_mut().pop_front() {
                Some(FakeCall::Response {
                    status,
                    etag,
                    content_length,
                    rate_limit,
                    retry_after,
                    body,
                }) => Ok(HttpResponse {
                    status,
                    etag,
                    content_length,
                    rate_limit,
                    retry_after,
                    body: Box::new(Cursor::new(body)),
                }),
                Some(FakeCall::Failure(reason)) => Err(HttpError(reason)),
                None => Err(HttpError("FakeHttpGet: no more scripted calls".to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeCall, FakeHttpGet};
    use super::*;
    use crate::net::allowlist::Timeouts;

    fn endpoint(url: &'static str, host: allowlist::AllowedHost) -> Endpoint {
        Endpoint {
            host,
            url,
            max_bytes: 1024,
            timeouts: Timeouts {
                connect: Duration::from_secs(1),
                whole_request: Duration::from_secs(1),
            },
            profile: RequestProfile::RawFile,
        }
    }

    /// The choke point every caller (`databases::cache`, `release_feed`)
    /// sends its request through — a non-allowlisted endpoint must never
    /// reach the transport at all, regardless of which caller built the
    /// request.
    #[test]
    fn checked_get_rejects_a_non_allowlisted_endpoint_without_calling_the_transport() {
        let bad = endpoint("https://evil.example/x", allowlist::AllowedHost::GithubRaw);
        let http = FakeHttpGet::new(Vec::new());

        let result = checked_get(
            &http,
            &HttpRequest {
                endpoint: &bad,
                etag: None,
            },
        );

        assert!(matches!(result, Err(GetError::Refused(_))));
        assert_eq!(http.call_count(), 0, "the transport must never be reached");
    }

    #[test]
    fn checked_get_delegates_to_the_transport_for_an_allowlisted_endpoint() {
        let good = endpoint(
            "https://raw.githubusercontent.com/x",
            allowlist::AllowedHost::GithubRaw,
        );
        let http = FakeHttpGet::new(vec![FakeCall::ok(b"ok".to_vec())]);

        let result = checked_get(
            &http,
            &HttpRequest {
                endpoint: &good,
                etag: None,
            },
        );

        assert!(result.is_ok());
        assert_eq!(http.call_count(), 1);
    }
}
