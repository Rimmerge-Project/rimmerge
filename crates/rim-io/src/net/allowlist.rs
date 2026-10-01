//! The closed set of hosts Rimmerge may ever contact ([`AllowedHost`]), the
//! compiled-in [`Endpoint`] shape every request is built from, and
//! [`is_allowed`] — the one check every request passes through before
//! [`super::http::HttpGet::get`] is ever called. See this crate's own
//! `CLAUDE.md` for why a third host is a security-relevant design change,
//! never a routine one.

use std::time::Duration;

/// Every host Rimmerge may ever contact. Closed: a third variant is a
/// code change reviewed as a security-relevant change, never a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllowedHost {
    /// Serves the three rule-database files this app fetches.
    GithubRaw,
    /// Serves the release-check endpoint only.
    GithubApi,
}

impl AllowedHost {
    /// This host's own DNS name, compared case-insensitively against a
    /// request URL's own authority.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::GithubRaw => "raw.githubusercontent.com",
            Self::GithubApi => "api.github.com",
        }
    }
}

/// Connect and whole-request timeouts for one [`Endpoint`]. Every
/// endpoint carries its own — there is no shared/global fallback, so a
/// caller can never forget to bound a new request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timeouts {
    pub(crate) connect: Duration,
    pub(crate) whole_request: Duration,
}

/// Which fixed set of headers a request sends — see [`super::http`]'s own
/// header table. Closed, and there is no generic header map: a caller
/// cannot smuggle an arbitrary header onto a request by constructing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestProfile {
    /// A plain `raw.githubusercontent.com` file fetch: `User-Agent` and,
    /// when a stored `ETag` exists, `If-None-Match`.
    RawFile,
    /// A `api.github.com` REST call: the same two headers, plus `Accept`
    /// and `X-GitHub-Api-Version`.
    GithubApi,
}

/// One compiled-in request target. Every field is set from a `const`
/// context; no setting, environment variable, CLI flag, or fetched value
/// can ever produce an [`Endpoint`] — see this crate's own `CLAUDE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) host: AllowedHost,
    pub(crate) url: &'static str,
    pub(crate) max_bytes: u64,
    pub(crate) timeouts: Timeouts,
    pub(crate) profile: RequestProfile,
}

/// Whether `endpoint.url` is `https://` and its host is exactly
/// `endpoint.host`'s own name (case-insensitively, matching real hostname
/// comparison) — manual parsing rather than a `url`-crate dependency,
/// since this only ever checks our own hardcoded constants, never
/// attacker-controlled input: a userinfo (`user@host`) or port
/// (`host:port`) suffix on the host portion is stripped before comparing,
/// so neither can be used to smuggle a different real host past a naive
/// prefix check.
///
/// A URL is valid only for the **one host its own endpoint declares**,
/// never for "any allowed host" — an endpoint pointed at the wrong member
/// of [`AllowedHost`] still fails this check, so a typo can never point a
/// rule-database fetch at `api.github.com` (or the release check at
/// `raw.githubusercontent.com`) and have it silently succeed.
pub(crate) fn is_allowed(endpoint: &Endpoint) -> bool {
    let Some(rest) = endpoint.url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_and_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_and_port.split(':').next().unwrap_or(host_and_port);
    host.eq_ignore_ascii_case(endpoint.host.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(host: AllowedHost, url: &'static str) -> Endpoint {
        Endpoint {
            host,
            url,
            max_bytes: 1,
            timeouts: Timeouts {
                connect: Duration::from_secs(1),
                whole_request: Duration::from_secs(1),
            },
            profile: RequestProfile::RawFile,
        }
    }

    #[test]
    fn allows_the_declared_host_over_https() {
        assert!(is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "https://raw.githubusercontent.com/x"
        )));
        assert!(is_allowed(&endpoint(
            AllowedHost::GithubApi,
            "https://api.github.com/x"
        )));
    }

    #[test]
    fn rejects_a_url_valid_only_for_the_other_declared_host() {
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubApi,
            "https://raw.githubusercontent.com/x"
        )));
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "https://api.github.com/x"
        )));
    }

    #[test]
    fn rejects_lookalike_hosts_via_userinfo_subdomain_or_plain_http() {
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "https://raw.githubusercontent.com.evil.example/x"
        )));
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "https://evil.example/raw.githubusercontent.com/x"
        )));
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "https://raw.githubusercontent.com@evil.example/x"
        )));
        assert!(!is_allowed(&endpoint(
            AllowedHost::GithubRaw,
            "http://raw.githubusercontent.com/x"
        )));
    }

    #[test]
    fn every_allowed_host_variant_has_a_distinct_lowercase_dns_name() {
        assert_eq!(AllowedHost::GithubRaw.name(), "raw.githubusercontent.com");
        assert_eq!(AllowedHost::GithubApi.name(), "api.github.com");
    }
}
