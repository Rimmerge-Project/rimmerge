//! Coverage and export DTOs: existing matches, winners, row intents, coverage rows, skips, and the export report.

use rim_resolve::domain::{Coverage, CoverageRow, RowIntent, RowKey, Winner};
use rim_session::use_cases::AssignmentSkip;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::rows::TargetRefDto;

/// Mirrors [`rim_resolve::domain::ExistingMatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExistingMatchDto {
    /// The instance's owning mod.
    pub owner: String,
    /// The instance's own `defName`.
    pub instance_def_name: String,
    /// The `TargetKey` field the match was found through, by its
    /// canonical path text.
    pub key_field: String,
}

impl From<&rim_resolve::domain::ExistingMatch> for ExistingMatchDto {
    fn from(value: &rim_resolve::domain::ExistingMatch) -> Self {
        Self {
            owner: value.owner.as_str().to_string(),
            instance_def_name: value.instance_def_name.clone(),
            key_field: value.key_field.to_string(),
        }
    }
}

/// Mirrors [`Winner`]; tagged `"kind"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum WinnerDto {
    /// See [`Winner::Existing`].
    Existing {
        /// The winning existing match.
        #[serde(rename = "match")]
        match_: ExistingMatchDto,
    },
    /// See [`Winner::ThisProject`].
    ThisProject,
}

impl From<&Winner> for WinnerDto {
    fn from(value: &Winner) -> Self {
        match value {
            Winner::Existing(existing_match) => Self::Existing {
                match_: existing_match.into(),
            },
            Winner::ThisProject => Self::ThisProject,
        }
    }
}

/// Mirrors [`RowIntent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RowIntentDto {
    /// See [`RowIntent::Cover`].
    Cover,
    /// See [`RowIntent::Override`].
    Override,
}

impl From<RowIntent> for RowIntentDto {
    fn from(value: RowIntent) -> Self {
        match value {
            RowIntent::Cover => Self::Cover,
            RowIntent::Override => Self::Override,
        }
    }
}

/// Mirrors [`CoverageRow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CoverageRowDto {
    /// The target, matched through one key field.
    pub target: TargetRefDto,
    /// The target def's own owning mod.
    pub owner: String,
    /// Existing instances naming this target, in the selected order.
    pub matches: Vec<ExistingMatchDto>,
    /// `Cover` when `matches` is empty, `Override` otherwise.
    pub intent: RowIntentDto,
    /// The verdict under a known precedence rule, if one exists for this
    /// def type.
    pub winner: Option<WinnerDto>,
    /// Whether this project already has a row for this target.
    pub has_row: bool,
}

impl From<&CoverageRow> for CoverageRowDto {
    fn from(value: &CoverageRow) -> Self {
        Self {
            target: (&value.target).into(),
            owner: value.owner.as_str().to_string(),
            matches: value.matches.iter().map(Into::into).collect(),
            intent: value.intent.into(),
            winner: value.winner.as_ref().map(Into::into),
            has_row: value.has_row,
        }
    }
}

/// Mirrors [`Coverage`]: the whole work queue, uncovered-first. No
/// filter/paging — see this module's own doc comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CoverageDto {
    /// `false` for a standalone ("new def") project — coverage has no
    /// meaning without a `TargetKey` field to match a target through. The
    /// editor shows a "not applicable" message rather than an empty queue
    /// when this is `false`.
    pub applicable: bool,
    /// Every coverage row, in display order. Always empty when
    /// `applicable` is `false`.
    pub rows: Vec<CoverageRowDto>,
}

impl From<&Coverage> for CoverageDto {
    fn from(value: &Coverage) -> Self {
        Self {
            applicable: value.applicable,
            rows: value.rows.iter().map(Into::into).collect(),
        }
    }
}

// -- list_assignment_items -------------------------------------------------

/// Request shape for `export_assignment`. Mirrors
/// [`super::patch::ExportPatchRequestDto`](crate::dto::patch::ExportPatchRequestDto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportAssignmentRequestDto {
    /// Which project to export.
    pub assignment_id: String,
    /// The folder to render the project into — never the game's `Mods`
    /// folder or a subfolder of it.
    pub out_dir: String,
    /// Also copy the same rendered folder into the game's `Mods` folder
    /// and activate it.
    pub install: bool,
    /// Skip the running-game refusal on `install`.
    pub force: bool,
}

/// One field an export skipped. Mirrors [`AssignmentSkip`]
/// (`rim_merge::assign::SkippedField`) — exactly one of [`Self::target`]/
/// [`Self::own_def_name`] is ever set, matching the skip's own [`RowKey`].
/// An honest addressing pair rather than a synthesized [`TargetRefDto`]
/// for a free-standing skip (which would stuff the skipped field's own
/// path into `keyField` and reuse the section's def type for
/// `target.def.defType`), so no skip pretends to be target-shaped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentSkipDto {
    /// The section's own def type this skip happened in.
    pub def_type: String,
    /// The target this skip was on, for a target-keyed row.
    pub target: Option<TargetRefDto>,
    /// The free-standing row's own `defName`, for a free-standing row.
    pub own_def_name: Option<String>,
    /// The field that was skipped, by its canonical path text — the
    /// zero-segment path (an empty string) means the whole row was
    /// skipped, not one field of it.
    pub path: String,
    /// Why it was skipped.
    pub reason: String,
}

impl From<&AssignmentSkip> for AssignmentSkipDto {
    fn from(value: &AssignmentSkip) -> Self {
        let (target, own_def_name) = match &value.row {
            RowKey::Target(target) => (Some(target.into()), None),
            RowKey::Own(def_name) => (None, Some(def_name.clone())),
        };
        Self {
            def_type: value.def_type.clone(),
            target,
            own_def_name,
            path: value.path.to_string(),
            reason: value.reason.clone(),
        }
    }
}

/// What `export_assignment` did. Mirrors
/// [`super::patch::ExportPatchReportDto`](crate::dto::patch::ExportPatchReportDto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportAssignmentReportDto {
    /// Where the project was written, for display.
    pub export_path: String,
    /// Where the project was installed, when `install` was set.
    pub installed_path: Option<String>,
    /// The `ModsConfig.xml` backup path, when `install` wrote it.
    pub mods_config_backup_path: Option<String>,
    /// Every field this export couldn't render across the project's rows.
    pub skipped: Vec<AssignmentSkipDto>,
    /// Every written file's path, relative to `export_path`.
    pub files: Vec<String>,
    /// Canonical hash of the project this export was rendered from.
    pub content_sha256: String,
}
