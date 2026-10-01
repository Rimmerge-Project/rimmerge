//! [`FetchFailureDto`]: why a fetch from GitHub failed, crossing IPC as a
//! closed cause the frontend renders (and translates) plus the adapter's
//! English text, shown only as a "technical details" line. Shared by the
//! update check and the rule-database refresh — see
//! `apps/desktop/CLAUDE.md`'s "Backend text reaching the UI arrives as
//! codes/enums" rule.

use rim_session::ports::{FetchFailure, FetchFailureCause};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Mirrors [`FetchFailureCause`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FetchFailureCauseDto {
    /// See [`FetchFailureCause::Transport`].
    Transport,
    /// See [`FetchFailureCause::HttpStatus`].
    HttpStatus {
        /// The status code the server answered with.
        status: u16,
    },
    /// See [`FetchFailureCause::RateLimited`].
    RateLimited {
        /// When it is safe to try again, RFC 3339 UTC.
        until: String,
    },
    /// See [`FetchFailureCause::NotPublished`].
    NotPublished,
    /// See [`FetchFailureCause::TooLarge`].
    TooLarge,
    /// See [`FetchFailureCause::InvalidContent`].
    InvalidContent,
    /// See [`FetchFailureCause::NotAStableVersion`].
    NotAStableVersion,
    /// See [`FetchFailureCause::NotModifiedWithoutCache`].
    NotModifiedWithoutCache,
    /// See [`FetchFailureCause::ReadFailed`].
    ReadFailed,
    /// See [`FetchFailureCause::CacheWriteFailed`].
    CacheWriteFailed,
    /// See [`FetchFailureCause::Unclassified`].
    Unclassified,
}

impl From<FetchFailureCause> for FetchFailureCauseDto {
    fn from(value: FetchFailureCause) -> Self {
        match value {
            FetchFailureCause::Transport => Self::Transport,
            FetchFailureCause::HttpStatus { status } => Self::HttpStatus { status },
            FetchFailureCause::RateLimited { until } => Self::RateLimited {
                until: until.to_string(),
            },
            FetchFailureCause::NotPublished => Self::NotPublished,
            FetchFailureCause::TooLarge => Self::TooLarge,
            FetchFailureCause::InvalidContent => Self::InvalidContent,
            FetchFailureCause::NotAStableVersion => Self::NotAStableVersion,
            FetchFailureCause::NotModifiedWithoutCache => Self::NotModifiedWithoutCache,
            FetchFailureCause::ReadFailed => Self::ReadFailed,
            FetchFailureCause::CacheWriteFailed => Self::CacheWriteFailed,
            FetchFailureCause::Unclassified => Self::Unclassified,
        }
    }
}

/// Mirrors [`FetchFailure`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FetchFailureDto {
    /// Why the fetch failed; the frontend renders the sentence.
    pub cause: FetchFailureCauseDto,
    /// The adapter's short, bounded English text — a technical-details
    /// line only, never the headline.
    pub detail: String,
}

impl From<FetchFailure> for FetchFailureDto {
    fn from(value: FetchFailure) -> Self {
        Self {
            cause: value.cause.into(),
            detail: value.detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limited_failure_serializes_its_instant_under_the_camel_case_tag() {
        let until: jiff::Timestamp = "2026-01-01T00:00:00Z".parse().expect("timestamp");
        let dto = FetchFailureDto::from(FetchFailure {
            cause: FetchFailureCause::RateLimited { until },
            detail: "rate limited".to_string(),
        });

        let wire = serde_json::to_value(&dto).expect("serializes");

        assert_eq!(wire["cause"]["kind"], "rateLimited");
        assert_eq!(wire["cause"]["until"], "2026-01-01T00:00:00Z");
        assert_eq!(wire["detail"], "rate limited");
    }

    #[test]
    fn an_http_status_failure_carries_the_status_code() {
        let dto = FetchFailureDto::from(FetchFailure {
            cause: FetchFailureCause::HttpStatus { status: 503 },
            detail: "unexpected HTTP status 503".to_string(),
        });

        assert_eq!(dto.cause, FetchFailureCauseDto::HttpStatus { status: 503 });
    }
}
