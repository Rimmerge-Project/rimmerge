//! DTOs for `get_settings`/`set_settings`, `apply`, and the app-global
//! `get_app_settings`/`update_app_settings`/`reset_network_policy`.

use rim_resolve::domain::Confidence;
use rim_resolve::sort::{EnforcedLayers, TieBreak};
use rim_session::Settings;
use rim_session::app_settings::{AppSettings, NetworkPolicy, ReminderPolicy, StaleAfterDays};
use rim_session::ports::AppSettingsLoad;
use rim_session::use_cases::ApplyOutcome;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::OrderSourceDto;
use crate::error::CommandError;

/// Mirrors [`TieBreak`], which doesn't derive `Serialize`/`Deserialize`/
/// `TS` itself (a `rim-resolve` domain type staying serde-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SortTieBreakDto {
    /// See [`TieBreak::PreserveCurrent`].
    PreserveCurrent,
    /// See [`TieBreak::Rebuild`].
    Rebuild,
}

impl From<TieBreak> for SortTieBreakDto {
    fn from(value: TieBreak) -> Self {
        match value {
            TieBreak::PreserveCurrent => Self::PreserveCurrent,
            TieBreak::Rebuild => Self::Rebuild,
        }
    }
}

impl From<SortTieBreakDto> for TieBreak {
    fn from(value: SortTieBreakDto) -> Self {
        match value {
            SortTieBreakDto::PreserveCurrent => Self::PreserveCurrent,
            SortTieBreakDto::Rebuild => Self::Rebuild,
        }
    }
}

/// User-configurable sorter/ledger settings. Mirrors [`Settings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SettingsDto {
    /// The confidence threshold separating `Auto` from `NeedsInput`,
    /// 0..=100.
    pub threshold: u8,
    /// Whether `Soft`-strength engine edges are enforced.
    pub enforce_soft: bool,
    /// Whether `Awareness`-strength engine edges are enforced.
    pub enforce_awareness: bool,
    /// When on, a clean (or
    /// partially-clean) `DefOverride`/`PatchCollision` merge preview
    /// leads the ledger's suggestion instead of staying a user-chosen
    /// alternative.
    pub suggest_merge_when_clean: bool,
    /// Which base key an
    /// unconstrained mod's emission starts from.
    pub tie_break: SortTieBreakDto,
    /// Whether imported pair rules feed the sorter.
    pub use_imported_pairs: bool,
    /// Whether imported placement rules feed the sorter.
    pub use_imported_placements: bool,
    /// Whether `Layer::Inferred` engine edges (heuristic evidence) are
    /// enforced rather than merely advisory. The settings page has its own
    /// toggle for it, and it round-trips through every save so a save
    /// never silently resets it to the default.
    pub enforce_inferred: bool,
    /// Whether the ledger surfaces `DanglingDefReference` findings.
    /// Off by default — see [`Settings::show_dangling_def_references`]'s
    /// own doc comment for the measured false-positive rate. Never
    /// changes sort output.
    pub show_dangling_def_references: bool,
}

impl From<Settings> for SettingsDto {
    fn from(value: Settings) -> Self {
        Self {
            threshold: value.threshold.percent(),
            enforce_soft: value.enforce.soft,
            enforce_awareness: value.enforce.awareness,
            suggest_merge_when_clean: value.suggest_merge_when_clean,
            tie_break: value.tie_break.into(),
            use_imported_pairs: value.use_imported_pairs,
            use_imported_placements: value.use_imported_placements,
            enforce_inferred: value.enforce.inferred,
            show_dangling_def_references: value.show_dangling_def_references,
        }
    }
}

impl TryFrom<SettingsDto> for Settings {
    type Error = CommandError;

    fn try_from(value: SettingsDto) -> Result<Self, Self::Error> {
        Ok(Self {
            threshold: Confidence::new(value.threshold)?,
            enforce: EnforcedLayers {
                soft: value.enforce_soft,
                awareness: value.enforce_awareness,
                inferred: value.enforce_inferred,
            },
            suggest_merge_when_clean: value.suggest_merge_when_clean,
            tie_break: value.tie_break.into(),
            use_imported_pairs: value.use_imported_pairs,
            use_imported_placements: value.use_imported_placements,
            show_dangling_def_references: value.show_dangling_def_references,
        })
    }
}

/// Network access and the automatic-refresh feature toggles — mirrors
/// [`NetworkPolicy`]. App-global (`<base>/app-settings.json`), never
/// part of the per-profile [`SettingsDto`] above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct NetworkPolicyDto {
    /// The master switch.
    pub allow_network: bool,
    /// Automatic, once-per-launch update check.
    pub check_for_updates: bool,
    /// Automatic, at-most-once-a-day refresh of the eligible, enabled
    /// rule databases.
    pub auto_refresh_rule_databases: bool,
    /// Whether a refresh may fetch `communityRules.json`.
    pub fetch_community_rules: bool,
    /// Whether a refresh may fetch `steamDB.json` — never automatic
    /// regardless of this toggle.
    pub fetch_steam_workshop: bool,
    /// Whether a refresh may fetch this project's own
    /// `rimmerge-rules.json`.
    pub fetch_rimmerge_rules: bool,
}

impl From<NetworkPolicy> for NetworkPolicyDto {
    fn from(value: NetworkPolicy) -> Self {
        Self {
            allow_network: value.allow_network,
            check_for_updates: value.check_for_updates,
            auto_refresh_rule_databases: value.auto_refresh_rule_databases,
            fetch_community_rules: value.fetch_community_rules,
            fetch_steam_workshop: value.fetch_steam_workshop,
            fetch_rimmerge_rules: value.fetch_rimmerge_rules,
        }
    }
}

impl From<NetworkPolicyDto> for NetworkPolicy {
    fn from(value: NetworkPolicyDto) -> Self {
        Self {
            allow_network: value.allow_network,
            check_for_updates: value.check_for_updates,
            auto_refresh_rule_databases: value.auto_refresh_rule_databases,
            fetch_community_rules: value.fetch_community_rules,
            fetch_steam_workshop: value.fetch_steam_workshop,
            fetch_rimmerge_rules: value.fetch_rimmerge_rules,
        }
    }
}

/// Mirrors [`ReminderPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ReminderPolicyDto {
    /// See [`StaleAfterDays`]; `1..=365`.
    #[ts(type = "number")]
    pub rule_databases_stale_after_days: u16,
}

impl From<ReminderPolicy> for ReminderPolicyDto {
    fn from(value: ReminderPolicy) -> Self {
        Self {
            rule_databases_stale_after_days: value.rule_databases_stale_after_days.get(),
        }
    }
}

impl TryFrom<ReminderPolicyDto> for ReminderPolicy {
    type Error = CommandError;

    fn try_from(value: ReminderPolicyDto) -> Result<Self, Self::Error> {
        Ok(Self {
            rule_databases_stale_after_days: StaleAfterDays::new(
                value.rule_databases_stale_after_days,
            )
            .map_err(|error| CommandError::invalid_input(error.to_string()))?,
        })
    }
}

/// App-global preferences — mirrors [`AppSettings`]. Persisted at
/// `<base>/app-settings.json`, one per machine, never per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AppSettingsDto {
    /// See [`NetworkPolicyDto`].
    pub network: NetworkPolicyDto,
    /// See [`ReminderPolicyDto`].
    pub reminders: ReminderPolicyDto,
}

impl From<AppSettings> for AppSettingsDto {
    fn from(value: AppSettings) -> Self {
        Self {
            network: value.network.into(),
            reminders: value.reminders.into(),
        }
    }
}

impl TryFrom<AppSettingsDto> for AppSettings {
    type Error = CommandError;

    fn try_from(value: AppSettingsDto) -> Result<Self, Self::Error> {
        Ok(Self {
            network: value.network.into(),
            reminders: value.reminders.try_into()?,
        })
    }
}

/// How `<base>/app-settings.json` loaded — mirrors
/// [`AppSettingsLoad`], carrying only its *kind* (a code, never the
/// `Recovered` reason's prose: the frontend renders its own localized
/// sentence from this alone). `get_app_settings` is the only command
/// that returns this: `update_app_settings`/`reset_network_policy` just
/// wrote the file, so their result is always the freshly saved settings,
/// never a load outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum AppSettingsLoadStatusDto {
    /// The file existed, parsed, and its `schema` was recognized.
    Loaded,
    /// No file exists yet — [`AppSettingsDto`] carries
    /// [`AppSettings::default`]'s values.
    Missing,
    /// The file exists but couldn't be read, parsed, or carried an
    /// unknown `schema` — [`AppSettingsDto`] carries every network
    /// switch off, per [`AppSettingsLoad::settings`]'s fail-closed rule.
    Recovered,
}

impl From<&AppSettingsLoad> for AppSettingsLoadStatusDto {
    fn from(value: &AppSettingsLoad) -> Self {
        match value {
            AppSettingsLoad::Loaded(_) => Self::Loaded,
            AppSettingsLoad::Missing => Self::Missing,
            AppSettingsLoad::Recovered { .. } => Self::Recovered,
        }
    }
}

/// `get_app_settings`'s response: the effective settings plus how the
/// file that produced them loaded. Distinct from [`AppSettingsDto`]
/// alone so the Settings page can show its own "your settings file
/// couldn't be read" message without needing a second round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AppSettingsResponseDto {
    /// The effective settings ([`AppSettingsLoad::settings`]).
    pub settings: AppSettingsDto,
    /// See [`AppSettingsLoadStatusDto`].
    pub load_status: AppSettingsLoadStatusDto,
}

impl From<AppSettingsLoad> for AppSettingsResponseDto {
    fn from(value: AppSettingsLoad) -> Self {
        let load_status = AppSettingsLoadStatusDto::from(&value);
        Self {
            settings: value.settings().into(),
            load_status,
        }
    }
}

/// Request shape for `apply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ApplyRequestDto {
    /// The order to write, named by the caller: the confirmation the user
    /// just read names an order, so the request carries that same one
    /// instead of trusting the session's selection to still agree.
    pub source: OrderSourceDto,
    /// Whether to write the `source` order to `ModsConfig.xml` (as
    /// opposed to only saving decisions/rules).
    pub write_mods_config: bool,
    /// Render the generated merge mod into the game's `Mods/` folder (or
    /// remove it when no `Merge`/`ShipAsset` decision remains), atomically.
    pub write_merge_mod: bool,
    /// Write anyway even if `RimWorldWin64.exe` looks like it's running.
    pub force: bool,
}

/// What `apply` did. Mirrors [`ApplyOutcome`] plus the profile paths
/// decisions/rules were saved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ApplyReportDto {
    /// The `ModsConfig.xml` path, whether or not it was written this
    /// time.
    pub mods_config_path: String,
    /// The backup path `ModsConfig.xml` was copied to, if it was
    /// written.
    pub backup_path: Option<String>,
    /// Where decisions were saved.
    pub decisions_path: String,
    /// Where rules were saved.
    pub rules_path: String,
    /// The generated merge mod's folder, if [`ApplyRequestDto::write_merge_mod`]
    /// resulted in a write.
    pub merge_mod_path: Option<String>,
    /// The previous merge-mod generation's backup path, if one existed to
    /// back up.
    pub merge_mod_backup_path: Option<String>,
    /// Canonical key text of `Merge` decisions skipped because their
    /// preview wasn't complete, and `ShipAsset` decisions skipped because
    /// their mod or texture couldn't be found — the merge mod was still
    /// written (or removed) without them.
    pub skipped_merges: Vec<String>,
}

/// Builds an [`ApplyReportDto`] from the use case's [`ApplyOutcome`] plus
/// the session paths it saved against.
#[must_use]
pub fn apply_report(
    outcome: &ApplyOutcome,
    mods_config_path: &std::path::Path,
    profile_dir: &std::path::Path,
) -> ApplyReportDto {
    ApplyReportDto {
        mods_config_path: mods_config_path.display().to_string(),
        backup_path: outcome
            .backup_path
            .as_ref()
            .map(|p| p.display().to_string()),
        decisions_path: profile_dir.join("decisions.json").display().to_string(),
        rules_path: profile_dir.join("rules.json").display().to_string(),
        merge_mod_path: outcome
            .merge_mod_path
            .as_ref()
            .map(|p| p.display().to_string()),
        merge_mod_backup_path: outcome
            .merge_mod_backup_path
            .as_ref()
            .map(|p| p.display().to_string()),
        skipped_merges: outcome
            .skipped_merges
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_dto_round_trips_through_domain_settings() {
        let settings = Settings {
            threshold: Confidence::new(65).expect("valid confidence"),
            enforce: EnforcedLayers {
                soft: true,
                awareness: false,
                inferred: false,
            },
            suggest_merge_when_clean: false,
            tie_break: TieBreak::PreserveCurrent,
            use_imported_pairs: true,
            use_imported_placements: false,
            show_dangling_def_references: true,
        };
        let dto: SettingsDto = settings.into();
        assert_eq!(dto.threshold, 65);
        assert!(dto.enforce_soft);
        assert!(!dto.enforce_awareness);
        assert!(!dto.suggest_merge_when_clean);
        assert_eq!(dto.tie_break, SortTieBreakDto::PreserveCurrent);
        assert!(dto.use_imported_pairs);
        assert!(!dto.use_imported_placements);
        assert!(!dto.enforce_inferred);
        assert!(dto.show_dangling_def_references);

        let back: Settings = dto.try_into().expect("valid settings dto");
        assert_eq!(back.threshold.percent(), 65);
        assert!(back.enforce.soft);
        assert!(!back.suggest_merge_when_clean);
        assert_eq!(back.tie_break, TieBreak::PreserveCurrent);
        assert!(back.use_imported_pairs);
        assert!(!back.use_imported_placements);
        assert!(!back.enforce.inferred);
        assert!(back.show_dangling_def_references);
    }

    #[test]
    fn settings_dto_rejects_a_threshold_over_100() {
        let dto = SettingsDto {
            threshold: 101,
            enforce_soft: false,
            enforce_awareness: false,
            suggest_merge_when_clean: true,
            tie_break: SortTieBreakDto::Rebuild,
            use_imported_pairs: false,
            use_imported_placements: true,
            enforce_inferred: true,
            show_dangling_def_references: false,
        };
        let result: Result<Settings, CommandError> = dto.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn app_settings_response_reports_loaded_for_a_parsed_file() {
        let response: AppSettingsResponseDto =
            AppSettingsLoad::Loaded(AppSettings::default()).into();
        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Loaded);
        assert!(response.settings.network.allow_network);
    }

    #[test]
    fn app_settings_response_reports_missing_for_no_file() {
        let response: AppSettingsResponseDto = AppSettingsLoad::Missing.into();
        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Missing);
        assert!(response.settings.network.allow_network);
    }

    #[test]
    fn app_settings_response_fails_closed_for_a_recovered_file() {
        let response: AppSettingsResponseDto = AppSettingsLoad::Recovered {
            reason: "corrupt".to_string(),
        }
        .into();
        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Recovered);
        assert!(!response.settings.network.allow_network);
    }
}
