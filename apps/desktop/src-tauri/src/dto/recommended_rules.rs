//! DTOs for the Dashboard's "Get the recommended rules" step:
//! `get_recommended_rules_step`, `get_recommended_rules` and the
//! `rules://recommended-progress` event. Mirrors
//! [`rim_session::RecommendedRulesStep`] and
//! [`rim_session::use_cases::GetRecommendedRules`]'s report; nothing here
//! derives the step (that is `rim_session::recommended_rules_step`).
//!
//! Every tagged enum's payload is a named struct carrying its own
//! `rename_all`: an internally tagged enum's `rename_all` never renames a
//! struct variant's own fields (see [`super::notifications::WelcomeDto`]).

use std::collections::BTreeMap;

use rim_session::ports::RuleDatabase;
use rim_session::use_cases::{
    ImportFailure, ImportStep, RecommendedRulesProgress, RecommendedRulesReport,
};
use rim_session::{RecommendedRulesStep, SourceNeed, Unavailable};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::fetch_failure::FetchFailureDto;
use super::rule::ImportReportDto;
use super::rule_databases::{RuleDatabaseDto, RuleDatabaseRefreshResultDto};

/// Which network gate is closed. Mirrors [`Unavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UnavailableReasonDto {
    /// See [`Unavailable::NetworkOff`].
    NetworkOff,
    /// See [`Unavailable::AwaitingFirstRun`].
    AwaitingFirstRun,
}

impl From<Unavailable> for UnavailableReasonDto {
    fn from(value: Unavailable) -> Self {
        match value {
            Unavailable::NetworkOff => Self::NetworkOff,
            Unavailable::AwaitingFirstRun => Self::AwaitingFirstRun,
        }
    }
}

/// What the step shows. Mirrors [`RecommendedRulesStep`], plus
/// [`Self::InProgress`] while a click is running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RecommendedRulesStepDto {
    /// See [`RecommendedRulesStep::Done`].
    Done(StepDoneDto),
    /// See [`RecommendedRulesStep::Skipped`].
    Skipped(StepSkippedDto),
    /// See [`RecommendedRulesStep::Unavailable`].
    Unavailable(StepUnavailableDto),
    /// See [`RecommendedRulesStep::NeedsAction`].
    NeedsAction(StepNeedsActionDto),
    /// A "Get the recommended rules" click is running in this process.
    InProgress,
}

/// [`RecommendedRulesStepDto::Done`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StepDoneDto {
    /// Whether the profile's settings let imported rules reach the sort.
    pub imported_rules_in_use: bool,
}

/// [`RecommendedRulesStepDto::Skipped`]'s payload: what "Get them now"
/// would do, in the same shape and order as [`StepNeedsActionDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StepSkippedDto {
    /// What each source still needs, in source order.
    pub sources: Vec<SourceNeedEntryDto>,
}

/// [`RecommendedRulesStepDto::Unavailable`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StepUnavailableDto {
    /// Which gate is closed.
    pub reason: UnavailableReasonDto,
}

/// [`RecommendedRulesStepDto::NeedsAction`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StepNeedsActionDto {
    /// What each source still needs, in source order.
    pub sources: Vec<SourceNeedEntryDto>,
}

/// One source and what a click would do for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SourceNeedEntryDto {
    /// The source.
    pub database: RuleDatabaseDto,
    /// What it needs.
    pub need: SourceNeedDto,
}

/// What a click would do for one source. Mirrors [`SourceNeed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SourceNeedDto {
    /// See [`SourceNeed::TurnOn`].
    TurnOn,
    /// See [`SourceNeed::Download`].
    Download(DownloadNeedDto),
    /// See [`SourceNeed::Import`].
    Import,
}

/// [`SourceNeedDto::Download`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DownloadNeedDto {
    /// Why the last download attempt failed, if it did.
    pub last_failure: Option<FetchFailureDto>,
}

impl From<SourceNeed> for SourceNeedDto {
    fn from(value: SourceNeed) -> Self {
        match value {
            SourceNeed::TurnOn => Self::TurnOn,
            SourceNeed::Download { last_failure } => Self::Download(DownloadNeedDto {
                last_failure: last_failure.map(Into::into),
            }),
            SourceNeed::Import => Self::Import,
        }
    }
}

/// A source map as the DTO's ordered list. A `BTreeMap` iterates in
/// `RuleDatabase` order, which is the order the DTO promises.
fn need_entries(sources: BTreeMap<RuleDatabase, SourceNeed>) -> Vec<SourceNeedEntryDto> {
    sources
        .into_iter()
        .map(|(database, need)| SourceNeedEntryDto {
            database: database.into(),
            need: need.into(),
        })
        .collect()
}

impl From<RecommendedRulesStep> for RecommendedRulesStepDto {
    fn from(value: RecommendedRulesStep) -> Self {
        match value {
            RecommendedRulesStep::Done {
                imported_rules_in_use,
            } => Self::Done(StepDoneDto {
                imported_rules_in_use,
            }),
            RecommendedRulesStep::Skipped { sources } => Self::Skipped(StepSkippedDto {
                sources: need_entries(sources),
            }),
            RecommendedRulesStep::Unavailable(reason) => Self::Unavailable(StepUnavailableDto {
                reason: reason.into(),
            }),
            RecommendedRulesStep::NeedsAction { sources } => {
                Self::NeedsAction(StepNeedsActionDto {
                    sources: need_entries(sources),
                })
            }
        }
    }
}

/// One progress tick of a running click, the `rules://recommended-progress`
/// event's payload. Mirrors [`RecommendedRulesProgress`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RecommendedRulesProgressEventDto {
    /// See [`RecommendedRulesProgress::Downloading`].
    Downloading(DownloadingDto),
    /// See [`RecommendedRulesProgress::Importing`].
    Importing(ImportingDto),
}

/// [`RecommendedRulesProgressEventDto::Downloading`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DownloadingDto {
    /// The source being downloaded.
    pub database: RuleDatabaseDto,
}

/// [`RecommendedRulesProgressEventDto::Importing`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportingDto {
    /// The sources being imported, in source order.
    pub databases: Vec<RuleDatabaseDto>,
}

impl From<RecommendedRulesProgress> for RecommendedRulesProgressEventDto {
    fn from(value: RecommendedRulesProgress) -> Self {
        match value {
            RecommendedRulesProgress::Downloading { database } => {
                Self::Downloading(DownloadingDto {
                    database: database.into(),
                })
            }
            RecommendedRulesProgress::Importing { databases } => Self::Importing(ImportingDto {
                databases: databases.into_iter().map(Into::into).collect(),
            }),
        }
    }
}

/// The result of a whole click. Mirrors [`RecommendedRulesReport`]; the
/// import half carries counts only, never the imported rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RecommendedRulesReportDto {
    /// One result per source the click tried to download.
    pub downloads: Vec<RuleDatabaseRefreshResultDto>,
    /// What the import half did.
    pub import: ImportStepDto,
}

/// What the import half did. Mirrors [`ImportStep`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportStepDto {
    /// See [`ImportStep::NotNeeded`].
    NotNeeded,
    /// See [`ImportStep::Imported`].
    Imported(ImportedStepDto),
    /// See [`ImportStep::ImportedManifestNotRecorded`].
    ImportedManifestNotRecorded(ManifestNotRecordedDto),
    /// See [`ImportStep::Failed`].
    Failed(ImportFailedDto),
    /// See [`ImportStep::ProfileChanged`].
    ProfileChanged,
}

/// [`ImportStepDto::Imported`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportedStepDto {
    /// The sources this click imported, in source order.
    pub databases: Vec<RuleDatabaseDto>,
    /// What the importer reported, as counts.
    pub report: ImportReportDto,
}

/// [`ImportStepDto::ImportedManifestNotRecorded`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ManifestNotRecordedDto {
    /// The sources whose rules were saved without an import record.
    pub databases: Vec<RuleDatabaseDto>,
}

/// Which part of an import failed. Mirrors [`ImportFailure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportFailureCodeDto {
    /// See [`ImportFailure::Import`]: a database file could not be read or
    /// parsed.
    ImporterFailed,
    /// See [`ImportFailure::Store`]: saving the merged rules failed and the
    /// import was rolled back.
    SavingRulesFailed,
}

impl From<&ImportFailure> for ImportFailureCodeDto {
    fn from(value: &ImportFailure) -> Self {
        match value {
            ImportFailure::Import(_) => Self::ImporterFailed,
            ImportFailure::Store(_) => Self::SavingRulesFailed,
        }
    }
}

/// [`ImportStepDto::Failed`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportFailedDto {
    /// Which part of the import failed; the interface words it.
    pub code: ImportFailureCodeDto,
    /// The importer's own English text; a technical-details line only.
    pub message: String,
}

impl From<&RecommendedRulesReport> for RecommendedRulesReportDto {
    fn from(value: &RecommendedRulesReport) -> Self {
        Self {
            downloads: value
                .downloads
                .iter()
                .map(|(database, outcome)| RuleDatabaseRefreshResultDto {
                    database: (*database).into(),
                    outcome: outcome.clone().into(),
                })
                .collect(),
            import: (&value.import).into(),
        }
    }
}

impl From<&ImportStep> for ImportStepDto {
    fn from(value: &ImportStep) -> Self {
        match value {
            ImportStep::NotNeeded => Self::NotNeeded,
            ImportStep::Imported {
                databases,
                imported,
            } => Self::Imported(ImportedStepDto {
                databases: databases.iter().copied().map(Into::into).collect(),
                report: imported.into(),
            }),
            ImportStep::ImportedManifestNotRecorded { databases } => {
                Self::ImportedManifestNotRecorded(ManifestNotRecordedDto {
                    databases: databases.iter().copied().map(Into::into).collect(),
                })
            }
            ImportStep::Failed { error } => Self::Failed(ImportFailedDto {
                code: error.into(),
                message: error.to_string(),
            }),
            ImportStep::ProfileChanged => Self::ProfileChanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use rim_session::ports::{
        FetchFailure, ImportError, ImportedRules, RefreshOutcome, RuleDatabase, StoreError,
    };

    use super::*;

    fn set(databases: &[RuleDatabase]) -> BTreeSet<RuleDatabase> {
        databases.iter().copied().collect()
    }

    #[test]
    fn needs_action_lists_sources_in_database_order_with_their_needs() {
        let step = RecommendedRulesStep::NeedsAction {
            sources: BTreeMap::from([
                (RuleDatabase::SteamWorkshop, SourceNeed::Import),
                (
                    RuleDatabase::CommunityRules,
                    SourceNeed::Download {
                        last_failure: Some(FetchFailure::unclassified("offline")),
                    },
                ),
            ]),
        };

        let RecommendedRulesStepDto::NeedsAction(dto) = RecommendedRulesStepDto::from(step) else {
            panic!("expected needsAction");
        };

        assert_eq!(dto.sources.len(), 2);
        assert_eq!(dto.sources[0].database, RuleDatabaseDto::Community);
        assert!(matches!(
            &dto.sources[0].need,
            SourceNeedDto::Download(DownloadNeedDto {
                last_failure: Some(_)
            })
        ));
        assert_eq!(dto.sources[1].database, RuleDatabaseDto::Steam);
        assert_eq!(dto.sources[1].need, SourceNeedDto::Import);
    }

    #[test]
    fn every_other_step_variant_maps_to_its_kind() {
        assert_eq!(
            RecommendedRulesStepDto::from(RecommendedRulesStep::Done {
                imported_rules_in_use: true
            }),
            RecommendedRulesStepDto::Done(StepDoneDto {
                imported_rules_in_use: true
            })
        );
        assert_eq!(
            RecommendedRulesStepDto::from(RecommendedRulesStep::Skipped {
                sources: BTreeMap::from([(RuleDatabase::SteamWorkshop, SourceNeed::TurnOn)])
            }),
            RecommendedRulesStepDto::Skipped(StepSkippedDto {
                sources: vec![SourceNeedEntryDto {
                    database: RuleDatabaseDto::Steam,
                    need: SourceNeedDto::TurnOn,
                }]
            })
        );
        for (reason, expected) in [
            (Unavailable::NetworkOff, UnavailableReasonDto::NetworkOff),
            (
                Unavailable::AwaitingFirstRun,
                UnavailableReasonDto::AwaitingFirstRun,
            ),
        ] {
            assert_eq!(
                RecommendedRulesStepDto::from(RecommendedRulesStep::Unavailable(reason)),
                RecommendedRulesStepDto::Unavailable(StepUnavailableDto { reason: expected })
            );
        }
    }

    #[test]
    fn skipped_lists_sources_in_the_same_order_as_needs_action() {
        let sources = BTreeMap::from([
            (RuleDatabase::SteamWorkshop, SourceNeed::Import),
            (RuleDatabase::CommunityRules, SourceNeed::TurnOn),
        ]);

        let RecommendedRulesStepDto::Skipped(skipped) =
            RecommendedRulesStepDto::from(RecommendedRulesStep::Skipped {
                sources: sources.clone(),
            })
        else {
            panic!("expected skipped");
        };
        let RecommendedRulesStepDto::NeedsAction(offered) =
            RecommendedRulesStepDto::from(RecommendedRulesStep::NeedsAction { sources })
        else {
            panic!("expected needsAction");
        };

        assert_eq!(skipped.sources, offered.sources);
        assert_eq!(skipped.sources[0].database, RuleDatabaseDto::Community);
    }

    #[test]
    fn a_turn_on_need_maps_to_its_kind() {
        assert_eq!(
            SourceNeedDto::from(SourceNeed::TurnOn),
            SourceNeedDto::TurnOn
        );
    }

    #[test]
    fn progress_maps_both_variants() {
        assert_eq!(
            RecommendedRulesProgressEventDto::from(RecommendedRulesProgress::Downloading {
                database: RuleDatabase::SteamWorkshop
            }),
            RecommendedRulesProgressEventDto::Downloading(DownloadingDto {
                database: RuleDatabaseDto::Steam
            })
        );
        assert_eq!(
            RecommendedRulesProgressEventDto::from(RecommendedRulesProgress::Importing {
                databases: set(&[RuleDatabase::SteamWorkshop, RuleDatabase::CommunityRules])
            }),
            RecommendedRulesProgressEventDto::Importing(ImportingDto {
                databases: vec![RuleDatabaseDto::Community, RuleDatabaseDto::Steam]
            })
        );
    }

    #[test]
    fn the_report_carries_downloads_and_import_counts_only() {
        let report = RecommendedRulesReport {
            downloads: BTreeMap::from([(
                RuleDatabase::CommunityRules,
                RefreshOutcome::Unchanged {
                    sha256: "abc".to_string(),
                },
            )]),
            import: ImportStep::Imported {
                databases: set(&[RuleDatabase::CommunityRules]),
                imported: ImportedRules {
                    community_rules: Some(Vec::new()),
                    ..ImportedRules::default()
                },
            },
        };

        let dto = RecommendedRulesReportDto::from(&report);

        assert_eq!(dto.downloads.len(), 1);
        assert_eq!(dto.downloads[0].database, RuleDatabaseDto::Community);
        let ImportStepDto::Imported(imported) = &dto.import else {
            panic!("expected imported");
        };
        assert_eq!(imported.databases, vec![RuleDatabaseDto::Community]);
        assert_eq!(imported.report.community_rules, Some(0));
        assert_eq!(imported.report.steam_dependencies, None);
    }

    #[test]
    fn every_import_error_maps_to_its_own_failure_code() {
        let store = || StoreError("disk".to_string());
        for (error, expected) in [
            (
                ImportFailure::Import(ImportError("boom".to_string())),
                ImportFailureCodeDto::ImporterFailed,
            ),
            (
                ImportFailure::Store(store()),
                ImportFailureCodeDto::SavingRulesFailed,
            ),
        ] {
            assert_eq!(ImportFailureCodeDto::from(&error), expected);
        }
    }

    #[test]
    fn the_import_step_maps_every_variant() {
        let failed = ImportStep::Failed {
            error: ImportFailure::Import(ImportError("boom".to_string())),
        };
        let ImportStepDto::Failed(failure) = ImportStepDto::from(&failed) else {
            panic!("expected failed");
        };
        assert!(failure.message.contains("boom"), "{}", failure.message);
        assert_eq!(failure.code, ImportFailureCodeDto::ImporterFailed);

        assert_eq!(
            ImportStepDto::from(&ImportStep::NotNeeded),
            ImportStepDto::NotNeeded
        );
        assert_eq!(
            ImportStepDto::from(&ImportStep::ProfileChanged),
            ImportStepDto::ProfileChanged
        );
        assert_eq!(
            ImportStepDto::from(&ImportStep::ImportedManifestNotRecorded {
                databases: set(&[RuleDatabase::SteamWorkshop]),
            }),
            ImportStepDto::ImportedManifestNotRecorded(ManifestNotRecordedDto {
                databases: vec![RuleDatabaseDto::Steam]
            })
        );
        assert!(matches!(
            ImportStepDto::from(&ImportStep::Imported {
                databases: set(&[RuleDatabase::SteamWorkshop]),
                imported: ImportedRules::default(),
            }),
            ImportStepDto::Imported(_)
        ));
    }
}
