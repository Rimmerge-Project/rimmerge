//! DTOs for the rules page's Databases card: `get_rule_databases`/`refresh_rule_databases`. Mirrors
//! [`rim_session::use_cases::RuleDatabaseView`]/[`RefreshOutcome`] —
//! neither `RuleDatabase -> IMPORT_SOURCE_*` mapping nor the
//! `needs_reimport` comparison is re-derived here; both are already
//! computed by `rim_session::use_cases::RefreshRuleDatabases` before this
//! module ever sees them (interfaces are composition roots with no
//! business logic).

use rim_session::ports::{CachedDatabase, RefreshOutcome, RuleDatabase, SkipReason};
use rim_session::use_cases::RuleDatabaseView;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::fetch_failure::FetchFailureDto;

/// Mirrors [`RuleDatabase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RuleDatabaseDto {
    /// See [`RuleDatabase::CommunityRules`].
    Community,
    /// See [`RuleDatabase::SteamWorkshop`].
    Steam,
    /// See [`RuleDatabase::RimmergeRules`].
    Rimmerge,
}

impl From<RuleDatabase> for RuleDatabaseDto {
    fn from(value: RuleDatabase) -> Self {
        match value {
            RuleDatabase::CommunityRules => Self::Community,
            RuleDatabase::SteamWorkshop => Self::Steam,
            RuleDatabase::RimmergeRules => Self::Rimmerge,
        }
    }
}

impl From<RuleDatabaseDto> for RuleDatabase {
    fn from(value: RuleDatabaseDto) -> Self {
        match value {
            RuleDatabaseDto::Community => Self::CommunityRules,
            RuleDatabaseDto::Steam => Self::SteamWorkshop,
            RuleDatabaseDto::Rimmerge => Self::RimmergeRules,
        }
    }
}

/// `refresh_rule_databases`'s optional argument: exactly the sources to
/// refresh. The command refreshes every enabled source when this is
/// absent (the Databases card's Refresh button); a caller that names
/// sources gets those and nothing else, so a notice that lists two
/// sources never downloads a third it did not mention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RefreshRuleDatabasesRequestDto {
    /// The sources to refresh; must not be empty. A repeated source is
    /// refreshed once.
    pub sources: Vec<RuleDatabaseDto>,
}

/// Mirrors [`CachedDatabase`]. `fetched_at` crosses as RFC 3339 text
/// (`jiff::Timestamp`'s own `Display`), like every other timestamp this
/// workspace persists or serializes — the frontend never parses it itself,
/// only displays it via `Intl` relative-time formatting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CachedDatabaseDto {
    /// The sha256 of the cached bytes.
    pub sha256: String,
    /// The cached file's byte count.
    #[ts(type = "number")]
    pub bytes: usize,
    /// RFC 3339 UTC.
    pub fetched_at: String,
}

impl From<&CachedDatabase> for CachedDatabaseDto {
    fn from(value: &CachedDatabase) -> Self {
        Self {
            sha256: value.sha256.clone(),
            bytes: value.bytes,
            fetched_at: value.fetched_at.to_string(),
        }
    }
}

/// One rule database's row on the Databases card. Mirrors
/// [`RuleDatabaseView`] (itself wrapping [`rim_session::ports::DatabaseStatus`])
/// plus `is_stale`, computed here against a real `jiff::Timestamp::now()` —
/// the one place in this command that reads the system clock, since
/// [`rim_session::ports::DatabaseStatus::is_stale`] itself takes an **injected** clock
/// and a Tauri command is
/// exactly the composition-root edge that convention exists to isolate
/// the real clock to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RuleDatabaseViewDto {
    /// Which database this row is.
    pub database: RuleDatabaseDto,
    /// Whether this source's own fetch toggle
    /// (`fetchCommunityRules`/`fetchSteamWorkshop`) is on.
    pub enabled: bool,
    /// The last successful fetch, if there has ever been one.
    pub cached: Option<CachedDatabaseDto>,
    /// Whether the cached copy is older than the app-global stale
    /// threshold (`AppSettings::reminders.rule_databases_stale_after_days`)
    /// — display only, never a TTL: a stale cache is still used exactly
    /// as a fresh one everywhere except this one label. Always `false`
    /// when `cached` is `None`.
    pub is_stale: bool,
    /// The most recent refresh's failure, if the last attempt failed.
    pub last_failure: Option<FetchFailureDto>,
    /// The sha256 this profile's own import manifest last recorded for
    /// this source, if it has ever imported it.
    pub imported_sha256: Option<String>,
    /// The re-import badge signal:
    /// the cache holds bytes this profile has not imported, or has
    /// imported a different version of. Never triggers anything on its
    /// own — the frontend renders it as an inline "Re-import" hint next
    /// to the import button, nothing more (no modal, no toast, never an
    /// automatic re-import).
    pub needs_reimport: bool,
    /// The embedded snapshot's own sha256 (`rim_io::embedded_bundle_sha256`,
    /// hashed once at compile time) — set only for
    /// [`RuleDatabase::RimmergeRules`], `None` for the other two sources,
    /// which have no compiled-in fallback of their own.
    pub bundled_sha256: Option<String>,
}

/// Builds a [`RuleDatabaseViewDto`] from a [`RuleDatabaseView`] and the
/// `now` this call reads the system clock once for.
#[must_use]
pub fn rule_database_view_dto(
    view: &RuleDatabaseView,
    now: jiff::Timestamp,
    stale_after_days: rim_session::StaleAfterDays,
) -> RuleDatabaseViewDto {
    RuleDatabaseViewDto {
        database: view.status.database.into(),
        enabled: view.status.enabled,
        cached: view.status.cached.as_ref().map(Into::into),
        is_stale: view.status.is_stale(now, stale_after_days),
        last_failure: view.status.last_failure.clone().map(Into::into),
        imported_sha256: view.imported_sha256.clone(),
        needs_reimport: view.needs_reimport,
        bundled_sha256: (view.status.database == RuleDatabase::RimmergeRules)
            .then(|| rim_io::embedded_bundle_sha256().to_string()),
    }
}

/// Mirrors [`SkipReason`] — the four variants must stay distinguishable
/// text/data all the way to the UI: "you turned this database off", "you
/// turned the network off", "you turned automatic refresh off", and "not
/// due yet" each need a different (or no) fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SkipReasonDto {
    /// See [`SkipReason::SourceDisabled`].
    SourceDisabled,
    /// See [`SkipReason::NetworkDisabled`].
    NetworkDisabled,
    /// See [`SkipReason::AutoRefreshDisabled`].
    AutoRefreshDisabled,
    /// See [`SkipReason::NotDue`].
    NotDue,
}

impl From<SkipReason> for SkipReasonDto {
    fn from(value: SkipReason) -> Self {
        match value {
            SkipReason::SourceDisabled => Self::SourceDisabled,
            SkipReason::NetworkDisabled => Self::NetworkDisabled,
            SkipReason::AutoRefreshDisabled => Self::AutoRefreshDisabled,
            SkipReason::NotDue => Self::NotDue,
        }
    }
}

/// Mirrors [`RefreshOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RefreshOutcomeDto {
    /// See [`RefreshOutcome::Updated`].
    Updated {
        /// The newly-fetched bytes' sha256.
        sha256: String,
        /// The newly-fetched byte count.
        #[ts(type = "number")]
        bytes: usize,
    },
    /// See [`RefreshOutcome::Unchanged`].
    Unchanged {
        /// The unchanged bytes' sha256.
        sha256: String,
    },
    /// See [`RefreshOutcome::Failed`].
    Failed {
        /// Why the attempt failed.
        failure: FetchFailureDto,
    },
    /// See [`RefreshOutcome::Skipped`].
    Skipped {
        /// Which switch to flip.
        reason: SkipReasonDto,
    },
}

impl From<RefreshOutcome> for RefreshOutcomeDto {
    fn from(value: RefreshOutcome) -> Self {
        match value {
            RefreshOutcome::Updated { sha256, bytes } => Self::Updated { sha256, bytes },
            RefreshOutcome::Unchanged { sha256 } => Self::Unchanged { sha256 },
            RefreshOutcome::Failed { failure } => Self::Failed {
                failure: failure.into(),
            },
            RefreshOutcome::Skipped { reason } => Self::Skipped {
                reason: reason.into(),
            },
        }
    }
}

/// One source's own refresh result — `refresh_rule_databases`'s response
/// is a list of these, one per requested database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RuleDatabaseRefreshResultDto {
    /// Which database this result is for.
    pub database: RuleDatabaseDto,
    /// What happened.
    pub outcome: RefreshOutcomeDto,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rim_session::ports::DatabaseStatus;

    use super::*;

    #[test]
    fn rule_database_dto_maps_to_the_matching_rule_database() {
        assert_eq!(
            RuleDatabaseDto::from(RuleDatabase::CommunityRules),
            RuleDatabaseDto::Community
        );
        assert_eq!(
            RuleDatabaseDto::from(RuleDatabase::SteamWorkshop),
            RuleDatabaseDto::Steam
        );
    }

    #[test]
    fn every_rule_database_dto_maps_back_to_its_own_rule_database() {
        for database in RuleDatabase::ALL {
            assert_eq!(
                RuleDatabase::from(RuleDatabaseDto::from(database)),
                database
            );
        }
    }

    #[test]
    fn a_refresh_request_rejects_unknown_fields() {
        let parsed = serde_json::from_str::<RefreshRuleDatabasesRequestDto>(
            r#"{"sources":["steam"],"everything":true}"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn a_refresh_request_parses_camel_case_source_names() {
        let parsed: RefreshRuleDatabasesRequestDto =
            serde_json::from_str(r#"{"sources":["community","rimmerge"]}"#).expect("parses");

        assert_eq!(
            parsed.sources,
            [RuleDatabaseDto::Community, RuleDatabaseDto::Rimmerge]
        );
    }

    #[test]
    fn cached_database_dto_carries_the_rfc3339_timestamp_as_text() {
        let cached = CachedDatabase {
            sha256: "abc123".to_string(),
            bytes: 10,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        let dto: CachedDatabaseDto = (&cached).into();
        assert_eq!(dto.sha256, "abc123");
        assert_eq!(dto.bytes, 10);
        assert_eq!(dto.fetched_at, jiff::Timestamp::UNIX_EPOCH.to_string());
    }

    #[test]
    fn rule_database_view_dto_computes_is_stale_against_the_given_now() {
        let fetched_at = jiff::Timestamp::UNIX_EPOCH;
        let view = RuleDatabaseView {
            status: DatabaseStatus {
                database: RuleDatabase::CommunityRules,
                enabled: true,
                path: PathBuf::from("communityRules.json"),
                cached: Some(CachedDatabase {
                    sha256: "abc".to_string(),
                    bytes: 1,
                    fetched_at,
                }),
                last_failure: None,
                last_attempt_at: None,
            },
            imported_sha256: None,
            needs_reimport: true,
        };

        let fresh =
            rule_database_view_dto(&view, fetched_at, rim_session::StaleAfterDays::default());
        assert!(!fresh.is_stale);
        assert!(fresh.needs_reimport);

        let stale_now = fetched_at + jiff::Span::new().hours(31 * 24);
        let stale =
            rule_database_view_dto(&view, stale_now, rim_session::StaleAfterDays::default());
        assert!(stale.is_stale);
    }

    #[test]
    fn refresh_outcome_dto_mirrors_every_variant() {
        assert_eq!(
            RefreshOutcomeDto::from(RefreshOutcome::Updated {
                sha256: "a".to_string(),
                bytes: 1
            }),
            RefreshOutcomeDto::Updated {
                sha256: "a".to_string(),
                bytes: 1
            }
        );
        assert_eq!(
            RefreshOutcomeDto::from(RefreshOutcome::Skipped {
                reason: SkipReason::NetworkDisabled
            }),
            RefreshOutcomeDto::Skipped {
                reason: SkipReasonDto::NetworkDisabled
            }
        );
    }
}
