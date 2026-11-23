//! DTOs for copying groups and colours from another profile
//! (`preview_copy_organization`, `copy_organization`).

use rim_session::organization::{CopyMode, CopyReport, GroupColorClash};
use rim_session::use_cases::CopyRefusal;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::organization::{ColorTokenDto, OrganizationIssueDto, OrganizationRuleDto};

/// How a copy combines the source with this profile. Mirrors [`CopyMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CopyModeDto {
    /// Add the source's groups, members and colours to what is here.
    Merge,
    /// Discard what is here; the source becomes the whole organization.
    Replace,
}

impl From<CopyModeDto> for CopyMode {
    fn from(value: CopyModeDto) -> Self {
        match value {
            CopyModeDto::Merge => Self::Merge,
            CopyModeDto::Replace => Self::Replace,
        }
    }
}

impl From<CopyMode> for CopyModeDto {
    fn from(value: CopyMode) -> Self {
        match value {
            CopyMode::Merge => Self::Merge,
            CopyMode::Replace => Self::Replace,
        }
    }
}

/// The request of both copy commands: the folder the user picked and the
/// mode. The folder is parsed at the command boundary; only
/// `organization.json` inside it is ever read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CopyOrganizationRequestDto {
    /// The other profile's folder (an absolute path).
    pub source_dir: String,
    /// Merge or replace.
    pub mode: CopyModeDto,
}

/// A group both profiles have, at the same path, with different colours:
/// this profile's colour is kept. Mirrors [`GroupColorClash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupColorClashDto {
    /// The group's names from the top level.
    pub path: Vec<String>,
    /// The colour kept (this profile's).
    pub kept: ColorTokenDto,
    /// The source's colour, not applied.
    pub offered: ColorTokenDto,
}

impl From<&GroupColorClash> for GroupColorClashDto {
    fn from(value: &GroupColorClash) -> Self {
        Self {
            path: value.path.iter().map(ToString::to_string).collect(),
            kept: value.kept.into(),
            offered: value.offered.into(),
        }
    }
}

/// What a copy does (the preview) or did (the result). Mirrors
/// [`CopyReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CopyReportDto {
    /// The mode planned.
    pub mode: CopyModeDto,
    /// Source groups created here.
    pub groups_added: usize,
    /// Source groups merged into a group here at the same path.
    pub groups_merged: usize,
    /// Memberships that did not exist here and now do.
    pub memberships_added: usize,
    /// Source memberships a merged group already held.
    pub memberships_already: usize,
    /// Mod colours set here that were not.
    pub mod_colors_added: usize,
    /// Mods coloured in both profiles with different colours; here wins.
    pub mod_colors_kept: usize,
    /// Merged groups that had no colour here and took the source's.
    pub group_colors_adopted: usize,
    /// Merged groups whose colour differs; here wins.
    pub clashes: Vec<GroupColorClashDto>,
    /// Groups a replace discards (0 for a merge).
    pub discarded_groups: usize,
    /// Memberships a replace discards (0 for a merge).
    pub discarded_memberships: usize,
    /// Mod colours a replace discards (0 for a merge).
    pub discarded_mod_colors: usize,
    /// Distinct mods the source puts in groups that this install lacks.
    pub not_installed_mods: usize,
}

impl From<&CopyReport> for CopyReportDto {
    fn from(value: &CopyReport) -> Self {
        Self {
            mode: value.mode.into(),
            groups_added: value.groups_added,
            groups_merged: value.groups_merged,
            memberships_added: value.memberships_added,
            memberships_already: value.memberships_already,
            mod_colors_added: value.mod_colors_added,
            mod_colors_kept: value.mod_colors_kept,
            group_colors_adopted: value.group_colors_adopted,
            clashes: value.clashes.iter().map(Into::into).collect(),
            discarded_groups: value.discarded.groups,
            discarded_memberships: value.discarded.memberships,
            discarded_mod_colors: value.discarded.mod_colors,
            not_installed_mods: value.not_installed_mods,
        }
    }
}

/// Why a copy was refused. Nothing was changed. Mirrors [`CopyRefusal`],
/// plus the two refusals only the command boundary can make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CopyRefusalDto {
    /// The chosen folder is this profile's own folder.
    SameProfile,
    /// The chosen path is not a folder.
    SourceNotAFolder,
    /// The folder holds no `organization.json`.
    SourceMissing,
    /// The folder's file cannot be used.
    SourceUnusable {
        /// Why.
        issue: OrganizationIssueDto,
    },
    /// The source has no groups and no colours.
    SourceEmpty,
    /// The source is exactly this profile's organization: nothing to copy.
    NothingToCopy,
    /// The result would break an organization limit.
    Limit {
        /// The rule.
        rule: OrganizationRuleDto,
    },
    /// The result would be larger than the file may be.
    TooLargeToSave {
        /// The size the document would have.
        bytes: usize,
        /// The most bytes the file may hold.
        limit: usize,
    },
}

impl From<&CopyRefusal> for CopyRefusalDto {
    fn from(value: &CopyRefusal) -> Self {
        match value {
            CopyRefusal::SourceMissing => Self::SourceMissing,
            CopyRefusal::SourceUnusable(issue) => Self::SourceUnusable {
                issue: issue.into(),
            },
            CopyRefusal::SourceEmpty => Self::SourceEmpty,
            CopyRefusal::NothingToCopy => Self::NothingToCopy,
            CopyRefusal::Limit(rule) => Self::Limit { rule: rule.into() },
            CopyRefusal::TooLargeToSave { bytes, limit } => Self::TooLargeToSave {
                bytes: *bytes,
                limit: *limit,
            },
        }
    }
}

/// The answer of both copy commands: the report, or the reason nothing
/// happened. A refusal is an answer, not an error: the dialog words it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CopyAnswerDto {
    /// What the copy does (preview) or did (copy).
    Report {
        /// The counts and clashes.
        report: CopyReportDto,
    },
    /// Nothing was copied, and nothing changed.
    Refused {
        /// Why.
        reason: CopyRefusalDto,
    },
}
