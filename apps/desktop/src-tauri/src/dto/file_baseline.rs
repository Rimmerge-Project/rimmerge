//! DTOs for the live `ModsConfig.xml` baseline: whether the latest read
//! worked, what someone else changed, and how a Mods page row differs from
//! the file. Every enum crosses as a code the interface words.

use rim_resolve::domain::OrderSource;
use rim_session::{BaselineIssue, ExternalFileChange, FileBaseline, MembershipMark, Session};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a row's membership differs from the file's active list, by base id.
/// Mirrors [`MembershipMark`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MembershipMarkDto {
    /// Same membership as the file.
    None,
    /// Active here, not in the file.
    Added,
    /// In the file, not active here.
    Removed,
}

impl From<MembershipMark> for MembershipMarkDto {
    fn from(value: MembershipMark) -> Self {
        match value {
            MembershipMark::None => Self::None,
            MembershipMark::Added => Self::Added,
            MembershipMark::Removed => Self::Removed,
        }
    }
}

/// Why the latest read of the file failed. Mirrors [`BaselineIssue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum BaselineIssueDto {
    /// The file does not exist.
    Missing,
    /// The file could not be read.
    Unreadable,
    /// The file is not a usable active list (truncated, wrong shape, too
    /// large).
    Malformed,
}

impl From<BaselineIssue> for BaselineIssueDto {
    fn from(value: BaselineIssue) -> Self {
        match value {
            BaselineIssue::Missing => Self::Missing,
            BaselineIssue::Unreadable => Self::Unreadable,
            BaselineIssue::Malformed => Self::Malformed,
        }
    }
}

/// Whether the baseline is what the file holds now. Mirrors
/// [`FileBaseline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FileBaselineStateDto {
    /// The latest read worked.
    Fresh,
    /// The latest read failed; the markers use the last list that read
    /// fine.
    Stale {
        /// What went wrong.
        reason: BaselineIssueDto,
    },
}

impl From<FileBaseline> for FileBaselineStateDto {
    fn from(value: FileBaseline) -> Self {
        match value {
            FileBaseline::Fresh => Self::Fresh,
            FileBaseline::Stale { reason } => Self::Stale {
                reason: reason.into(),
            },
        }
    }
}

/// What someone else did to the file since Rimmerge last loaded or wrote
/// it. Mirrors [`ExternalFileChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ExternalFileChangeDto {
    /// The file is what Rimmerge last loaded or wrote.
    None,
    /// Ids the file lists now that it did not, and ids it no longer lists,
    /// counted by exact id (swapping `x` for `x_steam` is one of each).
    Membership {
        /// Mods the file lists now that it did not.
        added: usize,
        /// Mods the file no longer lists.
        removed: usize,
    },
    /// The same mods in another order.
    Reordered,
}

impl From<ExternalFileChange> for ExternalFileChangeDto {
    fn from(value: ExternalFileChange) -> Self {
        match value {
            ExternalFileChange::None => Self::None,
            ExternalFileChange::Membership { added, removed } => {
                Self::Membership { added, removed }
            }
            ExternalFileChange::Reordered => Self::Reordered,
        }
    }
}

/// `get_mods_config_baseline`/`refresh_mods_config_baseline`'s result: the
/// baseline's status, the outside change and whether the file already holds
/// the selected order, which together drive the outside-change notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FileBaselineDto {
    /// Whether the latest read worked.
    pub state: FileBaselineStateDto,
    /// What someone else changed in the file's membership.
    pub external_change: ExternalFileChangeDto,
    /// Whether the file already lists exactly the selected order's ids, the
    /// generated merge mod included. When true, applying it overwrites nothing,
    /// so a note must not say it would. Live: it follows the file and the
    /// selection.
    pub file_matches_selected: bool,
    /// Whether the file already lists exactly Current's ids (merge mod
    /// included), whichever order is selected: an install export writes
    /// Current, so when this is true it overwrites no outside change.
    pub file_holds_current_exactly: bool,
}

impl From<&Session> for FileBaselineDto {
    fn from(session: &Session) -> Self {
        Self {
            state: session.file_baseline().into(),
            external_change: session.external_file_change().into(),
            file_matches_selected: session.file_holds_selected_exactly(),
            file_holds_current_exactly: session.file_holds_exactly(OrderSource::Current),
        }
    }
}
