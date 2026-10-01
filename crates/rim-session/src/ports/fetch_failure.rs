//! [`FetchFailure`]: why one fetch from GitHub failed, as a closed cause an
//! interface can render (and translate) itself plus the adapter's own
//! English text kept only as a technical detail. Shared by the update
//! check ([`crate::ports::ReleaseFeed`]) and the rule-database refresh
//! ([`crate::ports::RuleDatabaseFetcher`]): both fetch one file from a
//! fixed GitHub host, so one set of causes covers both.

use crate::ports::ReleaseFeedError;

/// The reason a fetch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchFailureCause {
    /// A transport-level failure: DNS, connect, TLS, or a timeout.
    Transport,
    /// An HTTP status other than the ones the endpoint understands.
    HttpStatus {
        /// The status code the server answered with.
        status: u16,
    },
    /// GitHub's rate limit was reached.
    RateLimited {
        /// When it is safe to try again.
        until: jiff::Timestamp,
    },
    /// `404` on the release feed: no release has ever been published.
    NotPublished,
    /// The response was larger than the endpoint's byte cap.
    TooLarge,
    /// The response arrived but did not parse as what the endpoint serves.
    InvalidContent,
    /// The latest release's tag is a pre-release or build, not a stable
    /// version.
    NotAStableVersion,
    /// The server answered `304 Not Modified` although nothing was cached
    /// to confirm.
    NotModifiedWithoutCache,
    /// The response body could not be read to the end.
    ReadFailed,
    /// The fetched file could not be written to the local cache.
    CacheWriteFailed,
    /// A failure with no recorded cause (an attempt recorded before causes
    /// were kept, or a fetcher that reported nothing for a source).
    Unclassified,
}

/// One failed fetch: the closed [`FetchFailureCause`] and the adapter's own
/// already-bounded English text, which an interface shows only as a
/// "technical details" line (never a response body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchFailure {
    /// Why the fetch failed.
    pub cause: FetchFailureCause,
    /// A short, already-bounded English description of the failure.
    pub detail: String,
}

impl FetchFailure {
    /// A failure recorded without a known cause.
    #[must_use]
    pub fn unclassified(detail: impl Into<String>) -> Self {
        Self {
            cause: FetchFailureCause::Unclassified,
            detail: detail.into(),
        }
    }
}

impl From<&ReleaseFeedError> for FetchFailure {
    fn from(error: &ReleaseFeedError) -> Self {
        let cause = match error {
            ReleaseFeedError::Transport(_) => FetchFailureCause::Transport,
            ReleaseFeedError::NotPublished => FetchFailureCause::NotPublished,
            ReleaseFeedError::HttpStatus(status) => {
                FetchFailureCause::HttpStatus { status: *status }
            }
            ReleaseFeedError::RateLimited { until } => {
                FetchFailureCause::RateLimited { until: *until }
            }
            ReleaseFeedError::TooLarge => FetchFailureCause::TooLarge,
            ReleaseFeedError::Malformed => FetchFailureCause::InvalidContent,
            ReleaseFeedError::NotAStableVersion => FetchFailureCause::NotAStableVersion,
        };
        Self {
            cause,
            detail: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limited_feed_error_keeps_its_retry_instant() {
        let until: jiff::Timestamp = "2026-01-01T00:00:00Z".parse().expect("timestamp");

        let failure = FetchFailure::from(&ReleaseFeedError::RateLimited { until });

        assert_eq!(failure.cause, FetchFailureCause::RateLimited { until });
        assert!(failure.detail.contains("rate limit"));
    }

    #[test]
    fn every_other_feed_error_maps_to_its_own_cause() {
        let cases = [
            (
                ReleaseFeedError::Transport("dns".to_string()),
                FetchFailureCause::Transport,
            ),
            (
                ReleaseFeedError::NotPublished,
                FetchFailureCause::NotPublished,
            ),
            (
                ReleaseFeedError::HttpStatus(500),
                FetchFailureCause::HttpStatus { status: 500 },
            ),
            (ReleaseFeedError::TooLarge, FetchFailureCause::TooLarge),
            (
                ReleaseFeedError::Malformed,
                FetchFailureCause::InvalidContent,
            ),
            (
                ReleaseFeedError::NotAStableVersion,
                FetchFailureCause::NotAStableVersion,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(FetchFailure::from(&error).cause, expected);
        }
    }
}
