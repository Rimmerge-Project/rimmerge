//! DTOs for the notification system: `list_notifications`/
//! `dismiss_notification`/`mute_notification_kind`/
//! `unmute_notification_kind`/`list_muted_notification_kinds`/
//! `complete_welcome`/`check_for_update`/`run_launch_network_checks`.
//! Mirrors
//! [`rim_session::notifications::Notification`] and its companion types
//! — every field is data or a code, never rendered prose (see
//! `apps/desktop/CLAUDE.md`'s "Backend text reaching the UI arrives as
//! codes/enums" rule).

use std::collections::BTreeMap;

use rim_session::notifications::{
    Dismissal, Freshness, Notification, NotificationAction, NotificationKey, NotificationKind,
    RefreshMode, Severity, SourceSetup, StaleSource,
};
use rim_session::use_cases::{
    CheckForUpdateOutcome, LaunchNetworkChecksOutcome, UpdateCheckRunOutcome, UpdateCheckSkipReason,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::fetch_failure::FetchFailureDto;
use crate::dto::rule_databases::{RefreshOutcomeDto, RuleDatabaseDto};
use crate::dto::settings::NetworkPolicyDto;

/// Mirrors [`NotificationKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum NotificationKindDto {
    /// See [`NotificationKind::Welcome`].
    Welcome,
    /// See [`NotificationKind::GameVersionChanged`].
    GameVersionChanged,
    /// See [`NotificationKind::UpdateAvailable`].
    UpdateAvailable,
    /// See [`NotificationKind::RuleDatabasesStale`].
    RuleDatabasesStale,
    /// See [`NotificationKind::ImportedRulesOutdated`].
    ImportedRulesOutdated,
    /// See [`NotificationKind::RecommendedSourcesIncomplete`].
    RecommendedSourcesIncomplete,
}

impl From<NotificationKind> for NotificationKindDto {
    fn from(value: NotificationKind) -> Self {
        match value {
            NotificationKind::Welcome => Self::Welcome,
            NotificationKind::GameVersionChanged => Self::GameVersionChanged,
            NotificationKind::UpdateAvailable => Self::UpdateAvailable,
            NotificationKind::RuleDatabasesStale => Self::RuleDatabasesStale,
            NotificationKind::ImportedRulesOutdated => Self::ImportedRulesOutdated,
            NotificationKind::RecommendedSourcesIncomplete => Self::RecommendedSourcesIncomplete,
        }
    }
}

impl From<NotificationKindDto> for NotificationKind {
    fn from(value: NotificationKindDto) -> Self {
        match value {
            NotificationKindDto::Welcome => Self::Welcome,
            NotificationKindDto::GameVersionChanged => Self::GameVersionChanged,
            NotificationKindDto::UpdateAvailable => Self::UpdateAvailable,
            NotificationKindDto::RuleDatabasesStale => Self::RuleDatabasesStale,
            NotificationKindDto::ImportedRulesOutdated => Self::ImportedRulesOutdated,
            NotificationKindDto::RecommendedSourcesIncomplete => Self::RecommendedSourcesIncomplete,
        }
    }
}

/// Mirrors [`Severity`] — PrimeVue's own `"info"`/`"warn"` severities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SeverityDto {
    /// See [`Severity::Info`].
    Info,
    /// See [`Severity::Warn`].
    Warn,
}

impl From<Severity> for SeverityDto {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Info => Self::Info,
            Severity::Warn => Self::Warn,
        }
    }
}

/// Mirrors [`NotificationAction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum NotificationActionDto {
    /// See [`NotificationAction::KeepNetworkSettings`].
    KeepNetworkSettings,
    /// See [`NotificationAction::TurnOffNetwork`].
    TurnOffNetwork,
    /// See [`NotificationAction::ApplyRecommendedSettings`].
    ApplyRecommendedSettings,
    /// See [`NotificationAction::RefreshRuleDatabases`].
    RefreshRuleDatabases,
    /// See [`NotificationAction::OpenRuleDatabases`].
    OpenRuleDatabases,
    /// See [`NotificationAction::OpenSettings`].
    OpenSettings,
    /// See [`NotificationAction::ShowReleasePage`].
    ShowReleasePage,
    /// See [`NotificationAction::OpenPatches`].
    OpenPatches,
    /// See [`NotificationAction::EnableRecommendedSources`].
    EnableRecommendedSources,
}

impl From<NotificationAction> for NotificationActionDto {
    fn from(value: NotificationAction) -> Self {
        match value {
            NotificationAction::KeepNetworkSettings => Self::KeepNetworkSettings,
            NotificationAction::TurnOffNetwork => Self::TurnOffNetwork,
            NotificationAction::ApplyRecommendedSettings => Self::ApplyRecommendedSettings,
            NotificationAction::RefreshRuleDatabases => Self::RefreshRuleDatabases,
            NotificationAction::OpenRuleDatabases => Self::OpenRuleDatabases,
            NotificationAction::OpenSettings => Self::OpenSettings,
            NotificationAction::ShowReleasePage => Self::ShowReleasePage,
            NotificationAction::OpenPatches => Self::OpenPatches,
            NotificationAction::EnableRecommendedSources => Self::EnableRecommendedSources,
        }
    }
}

/// Mirrors [`Dismissal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DismissalDto {
    /// See [`Dismissal::Occurrence`].
    Occurrence,
    /// See [`Dismissal::OccurrenceOrMute`].
    OccurrenceOrMute,
}

impl From<Dismissal> for DismissalDto {
    fn from(value: Dismissal) -> Self {
        match value {
            Dismissal::Occurrence => Self::Occurrence,
            Dismissal::OccurrenceOrMute => Self::OccurrenceOrMute,
        }
    }
}

/// Mirrors [`NotificationKey`] — request shape for `dismiss_notification`
/// as well as this notice's own `key` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct NotificationKeyDto {
    /// Which kind this occurrence belongs to.
    pub kind: NotificationKindDto,
    /// This occurrence's own identity within its kind.
    pub fingerprint: String,
}

impl From<NotificationKey> for NotificationKeyDto {
    fn from(value: NotificationKey) -> Self {
        Self {
            kind: value.kind.into(),
            fingerprint: value.fingerprint,
        }
    }
}

impl From<NotificationKeyDto> for NotificationKey {
    fn from(value: NotificationKeyDto) -> Self {
        Self {
            kind: value.kind.into(),
            fingerprint: value.fingerprint,
        }
    }
}

/// Mirrors [`Freshness`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FreshnessDto {
    /// See [`Freshness::NeverFetched`].
    NeverFetched,
    /// See [`Freshness::FetchedAt`]. RFC 3339 UTC.
    FetchedAt {
        /// When the last successful fetch happened.
        at: String,
    },
}

impl From<Freshness> for FreshnessDto {
    fn from(value: Freshness) -> Self {
        match value {
            Freshness::NeverFetched => Self::NeverFetched,
            Freshness::FetchedAt(at) => Self::FetchedAt { at: at.to_string() },
        }
    }
}

/// Mirrors [`RefreshMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RefreshModeDto {
    /// See [`RefreshMode::Automatic`].
    Automatic,
    /// See [`RefreshMode::Manual`].
    Manual,
}

impl From<RefreshMode> for RefreshModeDto {
    fn from(value: RefreshMode) -> Self {
        match value {
            RefreshMode::Automatic => Self::Automatic,
            RefreshMode::Manual => Self::Manual,
        }
    }
}

/// Mirrors [`StaleSource`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StaleSourceDto {
    /// See [`StaleSource::freshness`].
    pub freshness: FreshnessDto,
    /// See [`StaleSource::refresh`].
    pub refresh: RefreshModeDto,
    /// See [`StaleSource::last_failure`] — a closed cause the frontend
    /// translates, with the English text as a technical detail.
    pub last_failure: Option<FetchFailureDto>,
}

impl From<StaleSource> for StaleSourceDto {
    fn from(value: StaleSource) -> Self {
        Self {
            freshness: value.freshness.into(),
            refresh: value.refresh.into(),
            last_failure: value.last_failure.map(Into::into),
        }
    }
}

/// Mirrors [`SourceSetup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SourceSetupDto {
    /// See [`SourceSetup::Off`].
    Off,
    /// See [`SourceSetup::NotDownloaded`].
    NotDownloaded,
}

impl From<SourceSetup> for SourceSetupDto {
    fn from(value: SourceSetup) -> Self {
        match value {
            SourceSetup::Off => Self::Off,
            SourceSetup::NotDownloaded => Self::NotDownloaded,
        }
    }
}

/// The parsed running/latest version pair, as plain semver strings — the
/// frontend never parses or compares these itself; `evaluate()` already
/// decided this notice belongs in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct UpdateAvailableDto {
    /// The version currently running.
    pub running: String,
    /// The newer published version.
    pub latest_version: String,
    /// RFC 3339 UTC.
    pub latest_published_at: String,
}

/// Mirrors [`Notification`]'s variant-specific payload — the fields
/// common to every notice (severity/actions/dismissal/key) live on
/// [`NotificationDto`] itself instead of being repeated per variant
/// here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum NotificationDataDto {
    /// See [`Notification::Welcome`].
    Welcome(WelcomeDto),
    /// See [`Notification::GameVersionChanged`].
    GameVersionChanged(GameVersionChangedDto),
    /// See [`Notification::UpdateAvailable`].
    UpdateAvailable(UpdateAvailableDto),
    /// See [`Notification::RuleDatabasesStale`].
    RuleDatabasesStale(RuleDatabasesStaleDto),
    /// See [`Notification::ImportedRulesOutdated`].
    ImportedRulesOutdated(ImportedRulesOutdatedDto),
    /// See [`Notification::RecommendedSourcesIncomplete`].
    RecommendedSourcesIncomplete(RecommendedSourcesIncompleteDto),
}

/// [`Notification::Welcome`]'s payload. A named struct, not an inline
/// enum-variant body — **a struct *variant*'s own fields need their own
/// `rename_all`** (an internally-tagged enum's `rename_all` only renames
/// the variant names that become each tag value, never a struct
/// variant's own field names — see `apps/desktop/CLAUDE.md`'s own
/// `CommandErrorDetail` trap for the identical gotcha); a named struct
/// carrying its own `#[serde(rename_all = "camelCase")]` sidesteps that
/// trap entirely rather than relying on every variant remembering the
/// attribute individually.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct WelcomeDto {
    /// The current app-global network policy.
    pub network: NetworkPolicyDto,
    /// Whether this profile's settings already match the recommended
    /// defaults.
    pub settings_match_recommended: bool,
}

/// [`Notification::GameVersionChanged`]'s payload — see [`WelcomeDto`]'s
/// own doc comment for why this is a named struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameVersionChangedDto {
    /// The `major.minor` version this profile last acknowledged.
    pub acknowledged: String,
    /// The `major.minor` version currently in use.
    pub current: String,
}

/// [`Notification::RuleDatabasesStale`]'s payload — see [`WelcomeDto`]'s
/// own doc comment for why this is a named struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RuleDatabasesStaleDto {
    /// Every stale, enabled source's own detail.
    pub sources: BTreeMap<RuleDatabaseDto, StaleSourceDto>,
}

/// [`Notification::ImportedRulesOutdated`]'s payload — see
/// [`WelcomeDto`]'s own doc comment for why this is a named struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportedRulesOutdatedDto {
    /// Every source with unreimported cached content, mapped to that
    /// content's own cached sha256 — the frontend renders only the keys
    /// (which sources); the value exists so the notice's own dismissal
    /// fingerprint (computed in Rust, never here) tracks the cached
    /// content itself, not merely which sources are currently outdated.
    pub sources: BTreeMap<RuleDatabaseDto, String>,
}

/// [`Notification::RecommendedSourcesIncomplete`]'s payload — see
/// [`WelcomeDto`]'s own doc comment for why this is a named struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RecommendedSourcesIncompleteDto {
    /// Every recommended source that is not set up, and why.
    pub sources: BTreeMap<RuleDatabaseDto, SourceSetupDto>,
}

/// One active notice, as `list_notifications` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct NotificationDto {
    /// This occurrence's identity — the argument `dismiss_notification`
    /// takes.
    pub key: NotificationKeyDto,
    /// This notice's urgency.
    pub severity: SeverityDto,
    /// The buttons to show, in order.
    pub actions: Vec<NotificationActionDto>,
    /// Whether "Don't remind me again" is offered alongside dismiss.
    pub dismissal: DismissalDto,
    /// The variant-specific payload.
    pub data: NotificationDataDto,
}

impl From<Notification> for NotificationDto {
    fn from(notification: Notification) -> Self {
        let key = notification.key().into();
        let severity = notification.severity().into();
        let actions = notification.actions().into_iter().map(Into::into).collect();
        let dismissal = notification.dismissal().into();
        let data = match notification {
            Notification::Welcome {
                network,
                settings_match_recommended,
            } => NotificationDataDto::Welcome(WelcomeDto {
                network: network.into(),
                settings_match_recommended,
            }),
            Notification::GameVersionChanged {
                acknowledged,
                current,
            } => NotificationDataDto::GameVersionChanged(GameVersionChangedDto {
                acknowledged: acknowledged.to_string(),
                current: current.to_string(),
            }),
            Notification::UpdateAvailable { running, latest } => {
                NotificationDataDto::UpdateAvailable(UpdateAvailableDto {
                    running: running.to_string(),
                    latest_version: latest.version.to_string(),
                    latest_published_at: latest.published_at.to_string(),
                })
            }
            Notification::RuleDatabasesStale { sources } => {
                NotificationDataDto::RuleDatabasesStale(RuleDatabasesStaleDto {
                    sources: sources
                        .into_iter()
                        .map(|(database, detail)| (database.into(), detail.into()))
                        .collect(),
                })
            }
            Notification::ImportedRulesOutdated { sources } => {
                NotificationDataDto::ImportedRulesOutdated(ImportedRulesOutdatedDto {
                    sources: sources
                        .into_iter()
                        .map(|(database, sha256)| (database.into(), sha256))
                        .collect(),
                })
            }
            Notification::RecommendedSourcesIncomplete { sources } => {
                NotificationDataDto::RecommendedSourcesIncomplete(RecommendedSourcesIncompleteDto {
                    sources: sources
                        .into_iter()
                        .map(|(database, setup)| (database.into(), setup.into()))
                        .collect(),
                })
            }
        };
        Self {
            key,
            severity,
            actions,
            dismissal,
            data,
        }
    }
}

/// Mirrors [`UpdateCheckSkipReason`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UpdateCheckSkipReasonDto {
    /// See [`UpdateCheckSkipReason::AwaitingFirstRun`].
    AwaitingFirstRun,
    /// See [`UpdateCheckSkipReason::AlreadyRanThisLaunch`].
    AlreadyRanThisLaunch,
    /// See [`UpdateCheckSkipReason::NetworkDisabled`].
    NetworkDisabled,
    /// See [`UpdateCheckSkipReason::CheckDisabled`].
    CheckDisabled,
    /// See [`UpdateCheckSkipReason::NotDue`].
    NotDue,
    /// See [`UpdateCheckSkipReason::RateLimitedUntil`]. RFC 3339 UTC.
    RateLimitedUntil {
        /// When it is safe to try again.
        until: String,
    },
}

impl From<UpdateCheckSkipReason> for UpdateCheckSkipReasonDto {
    fn from(value: UpdateCheckSkipReason) -> Self {
        match value {
            UpdateCheckSkipReason::AwaitingFirstRun => Self::AwaitingFirstRun,
            UpdateCheckSkipReason::AlreadyRanThisLaunch => Self::AlreadyRanThisLaunch,
            UpdateCheckSkipReason::NetworkDisabled => Self::NetworkDisabled,
            UpdateCheckSkipReason::CheckDisabled => Self::CheckDisabled,
            UpdateCheckSkipReason::NotDue => Self::NotDue,
            UpdateCheckSkipReason::RateLimitedUntil(until) => Self::RateLimitedUntil {
                until: until.to_string(),
            },
        }
    }
}

/// Mirrors [`UpdateCheckRunOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UpdateCheckRunOutcomeDto {
    /// See [`UpdateCheckRunOutcome::Updated`].
    Updated {
        /// The newly-recorded version. **`rename` is required here even
        /// under the enum's own `rename_all = "camelCase"`** — that
        /// attribute only renames variant tags, never a struct variant's
        /// own field names (see [`WelcomeDto`]'s doc comment for the
        /// full explanation of this trap); without it this field ships
        /// snake_case on the wire despite every other DTO in this crate
        /// being camelCase.
        #[serde(rename = "latestVersion")]
        latest_version: String,
    },
    /// See [`UpdateCheckRunOutcome::Unchanged`].
    Unchanged,
    /// See [`UpdateCheckRunOutcome::Failed`].
    Failed {
        /// Why the attempt failed.
        failure: FetchFailureDto,
    },
}

impl From<UpdateCheckRunOutcome> for UpdateCheckRunOutcomeDto {
    fn from(value: UpdateCheckRunOutcome) -> Self {
        match value {
            UpdateCheckRunOutcome::Updated { latest } => Self::Updated {
                latest_version: latest.version.to_string(),
            },
            UpdateCheckRunOutcome::Unchanged => Self::Unchanged,
            UpdateCheckRunOutcome::Failed { failure } => Self::Failed {
                failure: failure.into(),
            },
        }
    }
}

/// Mirrors [`CheckForUpdateOutcome`] — `check_for_update`'s own result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CheckForUpdateOutcomeDto {
    /// No transport call was made.
    Skipped {
        /// Why.
        reason: UpdateCheckSkipReasonDto,
    },
    /// A transport call was made; here's what it did.
    Ran {
        /// What happened.
        outcome: UpdateCheckRunOutcomeDto,
    },
}

impl From<CheckForUpdateOutcome> for CheckForUpdateOutcomeDto {
    fn from(value: CheckForUpdateOutcome) -> Self {
        match value {
            CheckForUpdateOutcome::Skipped(reason) => Self::Skipped {
                reason: reason.into(),
            },
            CheckForUpdateOutcome::Ran(outcome) => Self::Ran {
                outcome: outcome.into(),
            },
        }
    }
}

/// `run_launch_network_checks`'s own result — mirrors
/// [`LaunchNetworkChecksOutcome`]. Never rendered as a notice or a toast
/// on its own; both halves are informational, for a caller that wants to
/// log them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LaunchNetworkChecksOutcomeDto {
    /// What the update check did.
    pub update_check: CheckForUpdateOutcomeDto,
    /// What the automatic rule-database refresh did, per source — empty
    /// when skipped by the shared gate.
    pub database_refresh: Vec<(RuleDatabaseDto, RefreshOutcomeDto)>,
}

impl From<LaunchNetworkChecksOutcome> for LaunchNetworkChecksOutcomeDto {
    fn from(value: LaunchNetworkChecksOutcome) -> Self {
        Self {
            update_check: value.update_check.into(),
            database_refresh: value
                .database_refresh
                .into_iter()
                .map(|(database, outcome)| (database.into(), outcome.into()))
                .collect(),
        }
    }
}
