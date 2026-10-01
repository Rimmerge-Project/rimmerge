//! DTOs for `load_project`/`get_default_paths` and the `project://progress`/
//! `session://changed` events.

use rim_session::ProjectPaths;
use rim_session::ports::{
    ModKnowledgeValueKind, RulesLoadWarning, ScanProgress, ScanProgressStage,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::OrderSourceDto;

/// Request shape for `load_project`. Mirrors [`ProjectPaths`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ProjectPathsDto {
    /// The RimWorld install directory.
    pub game_dir: String,
    /// The Steam workshop content folder.
    pub workshop_dir: String,
    /// The active `ModsConfig.xml`.
    pub mods_config: String,
    /// This project's profile directory.
    pub profile_dir: String,
}

impl From<ProjectPathsDto> for ProjectPaths {
    fn from(value: ProjectPathsDto) -> Self {
        Self {
            game_dir: value.game_dir.into(),
            workshop_dir: value.workshop_dir.into(),
            mods_config: value.mods_config.into(),
            profile_dir: value.profile_dir.into(),
        }
    }
}

impl From<&ProjectPaths> for ProjectPathsDto {
    fn from(value: &ProjectPaths) -> Self {
        Self {
            game_dir: value.game_dir.display().to_string(),
            workshop_dir: value.workshop_dir.display().to_string(),
            mods_config: value.mods_config.display().to_string(),
            profile_dir: value.profile_dir.display().to_string(),
        }
    }
}

/// A closed, machine-readable reason for [`DefaultPathsDto::warning`] —
/// today there is exactly one, [`rim_io::not_an_install_warning`]'s own
/// sentence, but this stays a `kind`-tagged union (not a bare marker
/// field) so a second warning shape is an additive variant, never a
/// breaking rename. The frontend renders this, localized; `warning`
/// (the English sentence) stays as the "technical details" fallback —
/// see `apps/desktop/CLAUDE.md`'s i18n conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DefaultPathsWarningDto {
    /// A resolved install directory that doesn't look like one (no
    /// `Version.txt`/`Data/Core/`) — see [`rim_io::not_an_install_warning`].
    NotAnInstall {
        /// The directory in question. A file path: never translated,
        /// rendered in `<code>`/monospace.
        #[serde(rename = "gameDir")]
        game_dir: String,
    },
}

/// A closed, machine-readable reason `get_default_paths` could not
/// resolve everything — the structured half of [`DefaultPathsDto::error`],
/// which stays as the "technical details" fallback. Mirrors
/// [`rim_io::PathResolutionError`]'s two ladder-exhaustion variants;
/// [`NoProfileBase`](DefaultPathsErrorDto::NoProfileBase) is this
/// command's own third case (neither `RIMMERGE_PROFILE_DIR` nor
/// `LOCALAPPDATA` is set), which happens before the ladder ever starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DefaultPathsErrorDto {
    /// Neither `RIMMERGE_PROFILE_DIR` nor `LOCALAPPDATA` is set, so
    /// Rimmerge has nowhere to keep its own files.
    NoProfileBase,
    /// No RimWorld install directory could be found — see
    /// [`rim_io::PathResolutionError::GameDirNotFound`].
    GameDirNotFound {
        /// Every candidate location that was looked at. File paths:
        /// never translated, rendered in `<code>`/monospace.
        candidates: Vec<String>,
    },
    /// No `ModsConfig.xml` path could be worked out — see
    /// [`rim_io::PathResolutionError::ModsConfigNotFound`].
    ModsConfigNotFound {
        /// Why the derivation failed — an OS-level technical detail,
        /// shown as such, never translated.
        reason: String,
    },
}

/// The default paths `get_default_paths` prefills the setup page with —
/// the result of the one precedence ladder
/// (`rim_io::resolve_project_paths`) `apps/cli`'s own `resolve_paths` also
/// goes through.
///
/// Every field is optional and `error`/`errorCode` carry the reason when
/// the ladder ran out of rungs, rather than the command failing
/// outright: the setup page is exactly where a user with no detectable
/// install needs to land, and it needs to render its (empty) form *and*
/// tell them where was looked. `error` is the full
/// [`rim_io::PathResolutionError`] text, which names every candidate on
/// its own line; `errorCode`/`warningCode` are its localizable,
/// structured counterparts (kept alongside the strings for one release
/// as the "technical details" fallback — see
/// `apps/desktop/CLAUDE.md`'s i18n conventions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefaultPathsDto {
    /// The resolved RimWorld install directory, if one was found.
    pub game_dir: Option<String>,
    /// The resolved Steam workshop content folder for that install.
    pub workshop_dir: Option<String>,
    /// The resolved `ModsConfig.xml` path.
    pub mods_config: Option<String>,
    /// The profile directory `mods_config` hashes to.
    pub profile_dir: Option<String>,
    /// A **non-fatal** complaint about what *was* resolved — today, an
    /// explicit field, `RIMMERGE_GAME_DIR`, or a pinned `config.json`
    /// entry naming a directory that isn't a RimWorld install
    /// ([`rim_io::ResolvedPaths::warning`]). The fields above are still
    /// filled in: the user may be pointing at a drive that isn't mounted
    /// yet, and refusing would be worse than saying so.
    pub warning: Option<String>,
    /// [`DefaultPathsWarningDto`] version of `warning`. `Some` exactly
    /// when `warning` is.
    pub warning_code: Option<DefaultPathsWarningDto>,
    /// Why nothing could be resolved, including every candidate location
    /// that was looked at. `None` when resolution succeeded.
    pub error: Option<String>,
    /// [`DefaultPathsErrorDto`] version of `error`. `Some` exactly when
    /// `error` is.
    pub error_code: Option<DefaultPathsErrorDto>,
}

/// The three paths `save_app_config` pins into `<base>/config.json`
/// ([`rim_io::AppConfig`]). An omitted or empty field unpins that path,
/// so the Setup page can hand over exactly what its form holds without
/// having to distinguish "cleared" from "never set".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AppConfigDto {
    /// The RimWorld install directory to pin.
    pub game_dir: Option<String>,
    /// The Steam workshop content folder to pin.
    pub workshop_dir: Option<String>,
    /// The `ModsConfig.xml` path to pin.
    pub mods_config: Option<String>,
}

/// One non-fatal note from the scan, e.g. a mod file that had to be
/// skipped. Mirrors [`rim_analyzer::domain::Warning`]: `mod_id` is `None`
/// only when the scan genuinely couldn't attribute the note to one mod
/// (see that type's own doc comment) — the frontend never parses `message`
/// to recover it. Shown on the dashboard's "Scan notes" card and on the
/// named mod's own detail page, never as a boot-time toast (there can be
/// dozens; see `ProjectSummaryDto::warnings`'s own doc comment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ScanWarningDto {
    /// The mod this note is about, when the scan could attribute it to one.
    pub mod_id: Option<String>,
    /// The note's text, verbatim from the scan.
    pub message: String,
}

impl From<&rim_analyzer::domain::Warning> for ScanWarningDto {
    fn from(value: &rim_analyzer::domain::Warning) -> Self {
        Self {
            mod_id: value.mod_id.as_ref().map(|id| id.as_str().to_string()),
            message: value.message.clone(),
        }
    }
}

/// What `load_project` returns on success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ProjectSummaryDto {
    /// How many active mods were found on disk.
    pub mod_count: usize,
    /// The game version the scan ran against.
    pub game_version: String,
    /// How long the scan took, in milliseconds. `#[ts(type = "number")]`
    /// overrides ts-rs's default `u64` -> `bigint` mapping: `serde_json`
    /// still serializes this as a plain JSON number on the wire (Tauri's
    /// IPC never produces an actual JS `bigint`), and an elapsed-ms value
    /// never remotely approaches `Number.MAX_SAFE_INTEGER`.
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    /// Analyzer notes collected during the scan — can run to dozens on a
    /// large install, so the frontend shows one "Scan finished with N
    /// notes" toast plus a collapsible list, never one toast per entry.
    pub warnings: Vec<ScanWarningDto>,
    /// Rules-file load warnings, e.g. a pre-migration `rules.json` whose
    /// cluster rules were dropped (`RulesLoadWarning`). Kept separate
    /// from `warnings`: there's normally at most one, it's actionable (the
    /// user may want to re-add the dropped rules), and it would otherwise
    /// be silently buried in a "Scan notes" list the user has no reason to
    /// open — so the frontend keeps toasting these individually.
    pub rule_warnings: Vec<RuleWarningDto>,
    /// The order the session has selected once the load (or rescan) is
    /// done. The frontend store takes its selection from here, never from
    /// a second hard-coded default of its own.
    pub selected: OrderSourceDto,
}

/// One non-fatal condition noticed while loading the rules file or the
/// mod-knowledge data. Mirrors [`RulesLoadWarning`]; the frontend renders
/// the sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RuleWarningDto {
    /// See [`RulesLoadWarning::DroppedClusterRules`].
    DroppedClusterRules {
        /// The dropped rules' own ids (or positional `#<index>`
        /// placeholders). Struct variant fields need their own `rename`
        /// for camelCase (see `CommandErrorDetail`).
        #[serde(rename = "ruleIds")]
        rule_ids: Vec<String>,
    },
    /// See [`RulesLoadWarning::UnknownModKnowledgeValue`].
    UnknownModKnowledgeValue {
        /// Which section of the data file the row came from.
        section: String,
        /// What kind of value was not understood.
        what: ModKnowledgeValueKindDto,
        /// The value itself, as written in the data.
        value: String,
    },
    /// See [`RulesLoadWarning::ModKnowledgeRowsOverRoleLimit`].
    ModKnowledgeRowsOverRoleLimit {
        /// Which section of the data file the rows came from.
        section: String,
        /// The role whose row limit was exceeded.
        role: String,
        /// How many rows were ignored.
        ignored: usize,
    },
    /// See [`RulesLoadWarning::PrecedenceRuleMissingFramework`].
    PrecedenceRuleMissingFramework {
        /// The def type the rule was for.
        #[serde(rename = "defType")]
        def_type: String,
    },
    /// See [`RulesLoadWarning::ModKnowledgeCacheUnreadable`].
    ModKnowledgeCacheUnreadable {
        /// The underlying error's English text — a technical-details
        /// line only.
        reason: String,
    },
}

impl From<&RulesLoadWarning> for RuleWarningDto {
    fn from(value: &RulesLoadWarning) -> Self {
        match value {
            RulesLoadWarning::DroppedClusterRules { rule_ids } => Self::DroppedClusterRules {
                rule_ids: rule_ids.clone(),
            },
            RulesLoadWarning::UnknownModKnowledgeValue {
                section,
                what,
                value,
            } => Self::UnknownModKnowledgeValue {
                section: section.clone(),
                what: (*what).into(),
                value: value.clone(),
            },
            RulesLoadWarning::ModKnowledgeRowsOverRoleLimit {
                section,
                role,
                ignored,
            } => Self::ModKnowledgeRowsOverRoleLimit {
                section: section.clone(),
                role: role.clone(),
                ignored: *ignored,
            },
            RulesLoadWarning::PrecedenceRuleMissingFramework { def_type } => {
                Self::PrecedenceRuleMissingFramework {
                    def_type: def_type.clone(),
                }
            }
            RulesLoadWarning::ModKnowledgeCacheUnreadable(reason) => {
                Self::ModKnowledgeCacheUnreadable {
                    reason: reason.clone(),
                }
            }
        }
    }
}

/// What kind of value of a mod-knowledge row was not understood. Mirrors
/// [`ModKnowledgeValueKind`]; the frontend renders the noun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModKnowledgeValueKindDto {
    /// A precedence `rule` discriminant.
    PrecedenceRule,
    /// A patch-operation `behaviour`.
    Behaviour,
    /// A `match` mode of a patch-operation, def-cache or log-shape row.
    MatchMode,
    /// A `match` mode of a patch-operation gate.
    GateMatchMode,
    /// A `behaviour` of a patch-operation gate.
    GateBehaviour,
    /// A conditional operation's type.
    ConditionalType,
    /// A top-level section of the data file.
    TopLevelSection,
    /// A log-shape role.
    LogShapeRole,
    /// A template placeholder type.
    PlaceholderType,
    /// A log-shape template that is empty, too long, or malformed.
    TemplateInvalid,
    /// A log-shape template lacking a capture its role needs.
    CaptureMissing,
    /// A log-shape marker string outside its length bounds.
    MarkerLength,
    /// A log-shape row id over the length bound.
    IdTooLong,
    /// A log-shape template that does not compile within the size limits.
    TemplateUncompilable,
}

impl From<ModKnowledgeValueKind> for ModKnowledgeValueKindDto {
    fn from(value: ModKnowledgeValueKind) -> Self {
        match value {
            ModKnowledgeValueKind::PrecedenceRule => Self::PrecedenceRule,
            ModKnowledgeValueKind::Behaviour => Self::Behaviour,
            ModKnowledgeValueKind::MatchMode => Self::MatchMode,
            ModKnowledgeValueKind::GateMatchMode => Self::GateMatchMode,
            ModKnowledgeValueKind::GateBehaviour => Self::GateBehaviour,
            ModKnowledgeValueKind::ConditionalType => Self::ConditionalType,
            ModKnowledgeValueKind::TopLevelSection => Self::TopLevelSection,
            ModKnowledgeValueKind::LogShapeRole => Self::LogShapeRole,
            ModKnowledgeValueKind::PlaceholderType => Self::PlaceholderType,
            ModKnowledgeValueKind::TemplateInvalid => Self::TemplateInvalid,
            ModKnowledgeValueKind::CaptureMissing => Self::CaptureMissing,
            ModKnowledgeValueKind::MarkerLength => Self::MarkerLength,
            ModKnowledgeValueKind::IdTooLong => Self::IdTooLong,
            ModKnowledgeValueKind::TemplateUncompilable => Self::TemplateUncompilable,
        }
    }
}

/// Payload of the `project://progress` event. Mirrors [`ScanProgress`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ProgressEventDto {
    /// The scan phase this tick belongs to; the frontend renders (and
    /// translates) the label.
    pub stage: ScanStageDto,
    /// How many units of this stage are complete.
    pub done: usize,
    /// The total units in this stage.
    pub total: usize,
}

/// Which phase of a project scan a [`ProgressEventDto`] reports. Mirrors
/// [`ScanProgressStage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ScanStageDto {
    /// Walking the install to find mod folders.
    Discovering,
    /// Scanning each active mod's files.
    Scanning,
    /// Analyzing the whole scanned set.
    Analyzing,
    /// Collecting the tag evidence the tagging rules read.
    CollectingTagEvidence,
    /// The scan produced every artifact.
    Done,
}

impl From<ScanProgressStage> for ScanStageDto {
    fn from(value: ScanProgressStage) -> Self {
        match value {
            ScanProgressStage::Discovering => Self::Discovering,
            ScanProgressStage::Scanning => Self::Scanning,
            ScanProgressStage::Analyzing => Self::Analyzing,
            ScanProgressStage::CollectingTagEvidence => Self::CollectingTagEvidence,
            ScanProgressStage::Done => Self::Done,
        }
    }
}

impl From<ScanProgress> for ProgressEventDto {
    fn from(value: ScanProgress) -> Self {
        Self {
            stage: value.stage.into(),
            done: value.done,
            total: value.total,
        }
    }
}

/// Payload of the `session://changed` event, emitted after any command
/// that mutates the session (`decide`/`revert_decision`/`upsert_rule`/
/// `delete_rule`/`set_manual_tag`/`import_rimsort`/`set_settings`/
/// `apply`/`set_merge_choices`/`create_patch`/`update_patch`/
/// `delete_patch`/`decide_patch`/`revert_patch_decision`/
/// `import_profile_decisions`/`prune_patch_decisions`/`export_patch`/
/// `create_assignment`/`update_assignment`/`set_assignment_row`/
/// `clear_assignment_row`/`delete_assignment`/`export_assignment`/
/// `rescan_project`/`activate_mods`/`deactivate_mods`) so the frontend
/// invalidates its queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SessionChangeReasonDto {
    /// A finding was decided.
    Decided,
    /// A decision was reverted.
    Reverted,
    /// A rule was added or replaced.
    RuleUpserted,
    /// A rule was removed.
    RuleDeleted,
    /// A manual tag was set.
    TagSet,
    /// RimSort rules were imported.
    Imported,
    /// Settings changed.
    SettingsChanged,
    /// The session was applied.
    Applied,
    /// A merge's field choices were stored.
    MergeChanged,
    /// A compat patch project was created.
    PatchCreated,
    /// A compat patch project's identity, scope, name, author, or
    /// description changed — or its orphaned decisions were pruned.
    PatchChanged,
    /// A compat patch project was deleted.
    PatchDeleted,
    /// The working active-mod set was rescanned
    /// — every
    /// query, including the order/findings/dashboard, refreshes against
    /// the new report.
    Rescanned,
    /// The working active-mod set changed (`activate_mods`/
    /// `deactivate_mods`) — no report/order/findings change yet, only
    /// `get_pending_active_changes`'s own `unscanned` diff.
    ActiveSetChanged,
    /// One of a compat patch's own findings was decided (including via
    /// `import_profile_decisions` or an extended `set_merge_choices`
    /// naming a patch).
    PatchDecided,
    /// A decision on one of a compat patch's own findings was reverted.
    PatchReverted,
    /// A compat patch was exported.
    PatchExported,
    /// An assignment (patch maker) project was created.
    AssignmentCreated,
    /// An assignment project's identity, R, T, or schema changed.
    AssignmentChanged,
    /// An assignment project was deleted.
    AssignmentDeleted,
    /// One of an assignment project's rows was set or cleared.
    AssignmentRowChanged,
    /// An assignment project was exported.
    AssignmentExported,
    /// A notification was dismissed, muted, unmuted, or the first-run
    /// notice was completed (`dismiss_notification`/
    /// `mute_notification_kind`/`unmute_notification_kind`/
    /// `complete_welcome`) — never emitted for `run_launch_network_checks`
    /// or `check_for_update`, which touch no session state at all and
    /// invalidate the notifications query directly through their own
    /// command result instead.
    NotificationsChanged,
}

/// Payload of the `session://changed` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SessionChangedEventDto {
    /// Why the session changed.
    pub reason: SessionChangeReasonDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_value_warning_carries_its_closed_kind_not_prose() {
        let warning = RulesLoadWarning::UnknownModKnowledgeValue {
            section: "patch-operations".to_string(),
            what: ModKnowledgeValueKind::GateBehaviour,
            value: "x".to_string(),
        };

        assert_eq!(
            RuleWarningDto::from(&warning),
            RuleWarningDto::UnknownModKnowledgeValue {
                section: "patch-operations".to_string(),
                what: ModKnowledgeValueKindDto::GateBehaviour,
                value: "x".to_string(),
            }
        );
    }

    #[test]
    fn the_row_limit_and_missing_framework_warnings_have_their_own_arms() {
        let over_limit = RulesLoadWarning::ModKnowledgeRowsOverRoleLimit {
            section: "log-shapes".to_string(),
            role: "texture_fallbacks".to_string(),
            ignored: 3,
        };
        let missing = RulesLoadWarning::PrecedenceRuleMissingFramework {
            def_type: "example.PartDef".to_string(),
        };

        assert_eq!(
            RuleWarningDto::from(&over_limit),
            RuleWarningDto::ModKnowledgeRowsOverRoleLimit {
                section: "log-shapes".to_string(),
                role: "texture_fallbacks".to_string(),
                ignored: 3,
            }
        );
        assert_eq!(
            RuleWarningDto::from(&missing),
            RuleWarningDto::PrecedenceRuleMissingFramework {
                def_type: "example.PartDef".to_string(),
            }
        );
    }
}
