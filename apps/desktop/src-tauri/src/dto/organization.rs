//! DTOs for the library organization commands (groups and mod colours).
//! Ids cross as plain numbers and strings and are parsed into domain value
//! objects here, at the boundary: a bad id, name or colour never reaches a
//! use case.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use rim_session::GroupLabel;
use rim_session::organization::{
    ColorChange, ColorToken, DeleteMode, DeleteOutcome, EditOutcome, GroupId, GroupName, GroupNode,
    GroupScopeResolution, MembershipAdd, MembershipMove, MembershipRemoval,
    OrganizationDocumentError, OrganizationEdit, OrganizationError, OrganizationIssue,
    OrganizationLimits, OrganizationState, OrganizedModId,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::CommandError;

/// One of the twelve palette colours. Mirrors [`ColorToken`]; the wire
/// name is the token's snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ColorTokenDto {
    /// Red.
    Red,
    /// Orange.
    Orange,
    /// Amber.
    Amber,
    /// Lime.
    Lime,
    /// Green.
    Green,
    /// Teal.
    Teal,
    /// Cyan.
    Cyan,
    /// Blue.
    Blue,
    /// Indigo.
    Indigo,
    /// Violet.
    Violet,
    /// Pink.
    Pink,
    /// Slate.
    Slate,
}

impl From<ColorToken> for ColorTokenDto {
    fn from(value: ColorToken) -> Self {
        match value {
            ColorToken::Red => Self::Red,
            ColorToken::Orange => Self::Orange,
            ColorToken::Amber => Self::Amber,
            ColorToken::Lime => Self::Lime,
            ColorToken::Green => Self::Green,
            ColorToken::Teal => Self::Teal,
            ColorToken::Cyan => Self::Cyan,
            ColorToken::Blue => Self::Blue,
            ColorToken::Indigo => Self::Indigo,
            ColorToken::Violet => Self::Violet,
            ColorToken::Pink => Self::Pink,
            ColorToken::Slate => Self::Slate,
        }
    }
}

impl From<ColorTokenDto> for ColorToken {
    fn from(value: ColorTokenDto) -> Self {
        match value {
            ColorTokenDto::Red => Self::Red,
            ColorTokenDto::Orange => Self::Orange,
            ColorTokenDto::Amber => Self::Amber,
            ColorTokenDto::Lime => Self::Lime,
            ColorTokenDto::Green => Self::Green,
            ColorTokenDto::Teal => Self::Teal,
            ColorTokenDto::Cyan => Self::Cyan,
            ColorTokenDto::Blue => Self::Blue,
            ColorTokenDto::Indigo => Self::Indigo,
            ColorTokenDto::Violet => Self::Violet,
            ColorTokenDto::Pink => Self::Pink,
            ColorTokenDto::Slate => Self::Slate,
        }
    }
}

/// The organization rule an unusable file broke, as a code the interface
/// words. Mirrors [`OrganizationDocumentError`] and, through it,
/// [`OrganizationError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum OrganizationRuleDto {
    /// A group names a group that does not exist.
    GroupNotFound,
    /// A group would sit inside itself.
    WouldCreateCycle,
    /// Groups nest too deep.
    TooDeep,
    /// Too many groups.
    TooManyGroups,
    /// Two siblings share a name.
    NameTaken,
    /// A move named the same group twice.
    SameGroup,
    /// Too many mods in groups.
    TooManyMembers,
    /// Too many coloured mods.
    TooManyColors,
    /// A group names a parent that does not exist.
    DanglingParent,
    /// A group id is used twice.
    DuplicateGroupId,
    /// A group id, or the next id to hand out, is out of range.
    GroupIdOutOfRange,
    /// The next id to hand out would reuse an existing id.
    NextIdBehind,
}

impl From<&OrganizationDocumentError> for OrganizationRuleDto {
    fn from(value: &OrganizationDocumentError) -> Self {
        match value {
            OrganizationDocumentError::DanglingParent { .. } => Self::DanglingParent,
            OrganizationDocumentError::DuplicateGroupId(_) => Self::DuplicateGroupId,
            OrganizationDocumentError::GroupIdOutOfRange { .. } => Self::GroupIdOutOfRange,
            OrganizationDocumentError::NextIdBehind { .. } => Self::NextIdBehind,
            OrganizationDocumentError::Rule(rule) => rule.into(),
        }
    }
}

impl From<&OrganizationError> for OrganizationRuleDto {
    fn from(value: &OrganizationError) -> Self {
        match value {
            OrganizationError::GroupNotFound(_) => Self::GroupNotFound,
            OrganizationError::WouldCreateCycle => Self::WouldCreateCycle,
            OrganizationError::TooDeep { .. } => Self::TooDeep,
            // A file never produces this (it is an edit-time refusal); the
            // nearest rule a document could break is the group limit.
            OrganizationError::TooManyGroups { .. } | OrganizationError::GroupIdsExhausted => {
                Self::TooManyGroups
            }
            OrganizationError::NameTaken { .. } => Self::NameTaken,
            OrganizationError::SameGroup => Self::SameGroup,
            OrganizationError::TooManyMembers { .. } => Self::TooManyMembers,
            OrganizationError::TooManyColors { .. } => Self::TooManyColors,
        }
    }
}

/// Why `organization.json` could not be used. Mirrors
/// [`OrganizationIssue`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum OrganizationIssueDto {
    /// The file exists but could not be read.
    Unreadable,
    /// The file is larger than the reader accepts.
    #[serde(rename_all = "camelCase")]
    TooLarge {
        /// The most bytes the reader accepts.
        limit_bytes: usize,
    },
    /// Not valid JSON of the expected shape, or a value is not valid.
    Malformed,
    /// Written by a newer version of the app.
    NewerVersion {
        /// The version the file states.
        found: u32,
    },
    /// Parsed, but describes a forest that breaks a rule.
    Invalid {
        /// The rule it breaks.
        rule: OrganizationRuleDto,
    },
}

impl From<&OrganizationIssue> for OrganizationIssueDto {
    fn from(value: &OrganizationIssue) -> Self {
        match value {
            OrganizationIssue::Unreadable => Self::Unreadable,
            OrganizationIssue::TooLarge { limit_bytes } => Self::TooLarge {
                limit_bytes: *limit_bytes,
            },
            OrganizationIssue::Malformed => Self::Malformed,
            OrganizationIssue::NewerVersion { found } => Self::NewerVersion { found: *found },
            OrganizationIssue::Invalid(error) => Self::Invalid { rule: error.into() },
        }
    }
}

/// One group in the tree. Mirrors [`GroupNode`]; the tree arrives depth
/// first, siblings by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupNodeDto {
    /// The group's id.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// Its colour.
    pub color: Option<ColorTokenDto>,
    /// Its parent, `null` for a top-level group.
    pub parent: Option<u32>,
    /// Its level, 1 for a top-level group.
    pub depth: usize,
    /// Its direct subgroups.
    pub child_count: usize,
    /// Its direct members.
    pub direct_count: usize,
    /// Its members counting subgroups, each mod once.
    pub scope_count: usize,
}

impl From<&GroupNode> for GroupNodeDto {
    fn from(value: &GroupNode) -> Self {
        Self {
            id: value.id.get(),
            name: value.name.to_string(),
            color: value.color.map(Into::into),
            parent: value.parent.map(GroupId::get),
            depth: value.depth,
            child_count: value.child_count,
            direct_count: value.direct_count,
            scope_count: value.scope_count,
        }
    }
}

/// The organization as `get_organization` answers: one variant per state
/// of [`OrganizationState`], each carrying only its own data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum OrganizationDto {
    /// Read, or fresh: the group tree.
    #[serde(rename_all = "camelCase")]
    Loaded {
        /// The group tree, depth first, siblings by name.
        groups: Vec<GroupNodeDto>,
        /// How many mods carry a colour of their own.
        colored_mod_count: usize,
    },
    /// The session has no organization store.
    NotLoaded,
    /// The file is unusable; organization is read-only until the user
    /// starts over.
    Recovered {
        /// Why the file could not be used.
        issue: OrganizationIssueDto,
    },
}

impl From<&OrganizationState> for OrganizationDto {
    fn from(value: &OrganizationState) -> Self {
        match value {
            OrganizationState::NotLoaded => Self::NotLoaded,
            OrganizationState::Recovered { issue } => Self::Recovered {
                issue: issue.into(),
            },
            OrganizationState::Loaded(organization) => Self::Loaded {
                groups: organization.tree().iter().map(Into::into).collect(),
                colored_mod_count: organization.mod_color_count(),
            },
        }
    }
}

/// One group a mod is a direct member of, as a list row shows it. Mirrors
/// [`GroupLabel`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupLabelDto {
    /// The group's id.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// Its colour.
    pub color: Option<ColorTokenDto>,
}

impl From<&GroupLabel> for GroupLabelDto {
    fn from(value: &GroupLabel) -> Self {
        Self {
            id: value.id.get(),
            name: value.name.to_string(),
            color: value.color.map(Into::into),
        }
    }
}

/// How a deleted group's subgroups are treated. Mirrors [`DeleteMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DeleteModeDto {
    /// Subgroups move up to the deleted group's parent.
    LiftChildren,
    /// The whole subtree is deleted.
    WithSubgroups,
}

impl From<DeleteModeDto> for DeleteMode {
    fn from(value: DeleteModeDto) -> Self {
        match value {
            DeleteModeDto::LiftChildren => Self::LiftChildren,
            DeleteModeDto::WithSubgroups => Self::WithSubgroups,
        }
    }
}

/// What an organization edit did, in the counts the interface reports.
/// Mirrors [`EditOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum OrganizationEditOutcomeDto {
    /// A group was created.
    Created {
        /// Its id.
        id: u32,
    },
    /// A rename, move or colour change of a group.
    Done,
    /// A group was deleted.
    #[serde(rename_all = "camelCase")]
    Deleted {
        /// Groups removed, the deleted group included.
        groups_removed: usize,
        /// Subgroups moved up a level.
        children_lifted: usize,
    },
    /// Mods were added to a group.
    Added {
        /// Mods that became members.
        added: usize,
        /// Mods that already were.
        already: usize,
    },
    /// Mods were removed from a group.
    Removed {
        /// Mods that stopped being members.
        removed: usize,
        /// Mods that were not members.
        absent: usize,
    },
    /// Mods were moved between groups.
    #[serde(rename_all = "camelCase")]
    Moved {
        /// Mods that left the source and joined the target.
        moved: usize,
        /// Mods that left the source; the target already held them.
        already_in_target: usize,
        /// Mods that were not direct members of the source.
        not_in_source: usize,
    },
    /// Mods were coloured or reset.
    Colored {
        /// Mods whose colour changed.
        changed: usize,
        /// Mods that already had that colour.
        unchanged: usize,
    },
}

impl From<EditOutcome> for OrganizationEditOutcomeDto {
    fn from(value: EditOutcome) -> Self {
        match value {
            EditOutcome::Created(id) => Self::Created { id: id.get() },
            EditOutcome::Done => Self::Done,
            EditOutcome::Deleted(DeleteOutcome {
                groups_removed,
                children_lifted,
            }) => Self::Deleted {
                groups_removed,
                children_lifted,
            },
            EditOutcome::Added(MembershipAdd { added, already }) => Self::Added { added, already },
            EditOutcome::Removed(MembershipRemoval { removed, absent }) => {
                Self::Removed { removed, absent }
            }
            EditOutcome::Moved(MembershipMove {
                moved,
                already_in_target,
                not_in_source,
            }) => Self::Moved {
                moved,
                already_in_target,
                not_in_source,
            },
            EditOutcome::Colored(ColorChange { changed, unchanged }) => {
                Self::Colored { changed, unchanged }
            }
        }
    }
}

/// A group scope's mods resolved against the install: what a group-level
/// activate or deactivate plans over. Mirrors [`GroupScopeResolution`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupScopePlanDto {
    /// The group and its subgroups.
    pub group_count: usize,
    /// The deduplicated mods in scope, installed or not.
    pub mod_count: usize,
    /// In-scope mods in the working set, by exact id, in load order.
    pub active_ids: Vec<String>,
    /// In-scope installed mods outside the working set, by exact id.
    pub inactive_ids: Vec<String>,
    /// How many in-scope mods are found nowhere on disk.
    pub not_installed: usize,
    /// Those mods, by base id: what "Remove mods that aren't installed"
    /// would remove.
    pub not_installed_ids: Vec<String>,
}

impl From<&GroupScopeResolution> for GroupScopePlanDto {
    fn from(value: &GroupScopeResolution) -> Self {
        Self {
            group_count: value.scope.groups.len(),
            mod_count: value.scope.mods.len(),
            active_ids: value
                .resolved
                .active
                .iter()
                .map(ToString::to_string)
                .collect(),
            inactive_ids: value
                .resolved
                .inactive
                .iter()
                .map(ToString::to_string)
                .collect(),
            not_installed: value.resolved.not_installed_count(),
            not_installed_ids: value
                .resolved
                .not_installed
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

/// Request shape for `create_group`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CreateGroupRequestDto {
    /// The new group's name.
    pub name: String,
    /// Its parent, `null` for top level.
    pub parent: Option<u32>,
}

/// Request shape for `rename_group`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RenameGroupRequestDto {
    /// The group.
    pub id: u32,
    /// Its new name.
    pub name: String,
}

/// Request shape for `reparent_group`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ReparentGroupRequestDto {
    /// The group.
    pub id: u32,
    /// Its new parent, `null` for top level.
    pub parent: Option<u32>,
}

/// Request shape for `delete_group`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DeleteGroupRequestDto {
    /// The group.
    pub id: u32,
    /// What happens to its subgroups.
    pub mode: DeleteModeDto,
}

/// Request shape for `set_group_color`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetGroupColorRequestDto {
    /// The group.
    pub id: u32,
    /// The colour, `null` to clear it.
    pub color: Option<ColorTokenDto>,
}

/// Request shape for `add_group_members`/`remove_group_members`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupMembersRequestDto {
    /// The group.
    pub id: u32,
    /// The mods, by any id (a `_steam` id names the same mod as its base).
    pub mod_ids: Vec<String>,
}

/// Request shape for `move_group_members`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MoveGroupMembersRequestDto {
    /// The source group.
    pub from: u32,
    /// The target group.
    pub to: u32,
    /// The mods, by any id.
    pub mod_ids: Vec<String>,
}

/// Request shape for `set_mod_colors`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetModColorsRequestDto {
    /// The mods, by any id.
    pub mod_ids: Vec<String>,
    /// The colour, `null` to clear it.
    pub color: Option<ColorTokenDto>,
}

/// Request shape for `plan_group_scope`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupScopeRequestDto {
    /// The group.
    pub id: u32,
}

/// Request shape for `remove_uninstalled_members`: the group and the mods the
/// user confirmed as not installed (as of the last scan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RemoveUninstalledMembersRequestDto {
    /// The group; its subgroups are included.
    pub id: u32,
    /// The mods confirmed as not installed, by base id. The backend removes
    /// only those that are still found nowhere on disk.
    pub mod_ids: Vec<String>,
}

/// A group id from the wire: never zero.
///
/// # Errors
///
/// [`CommandError::invalid_input`] for `0`.
pub fn parse_group_id(raw: u32) -> Result<GroupId, CommandError> {
    GroupId::new(raw).ok_or_else(|| CommandError::invalid_input("a group id is never 0"))
}

fn parse_group_name(raw: &str) -> Result<GroupName, CommandError> {
    GroupName::new(raw).map_err(|error| CommandError::invalid_input(error.to_string()))
}

/// The organized (base id) mods of a request, deduplicated. At most
/// [`OrganizationLimits::MAX_TOTAL_MEMBERSHIPS`] ids are accepted.
///
/// # Errors
///
/// [`CommandError::invalid_input`] for too many ids or an id a file could
/// not hold (empty, overlong, control characters).
pub fn parse_mod_ids(raw: &[String]) -> Result<BTreeSet<OrganizedModId>, CommandError> {
    if raw.len() > OrganizationLimits::MAX_TOTAL_MEMBERSHIPS {
        return Err(CommandError::invalid_input(format!(
            "at most {} mods per request",
            OrganizationLimits::MAX_TOTAL_MEMBERSHIPS
        )));
    }
    raw.iter()
        .map(|id| {
            OrganizedModId::new(&ModId::new(id))
                .map_err(|error| CommandError::invalid_input(error.to_string()))
        })
        .collect()
}

impl TryFrom<CreateGroupRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: CreateGroupRequestDto) -> Result<Self, CommandError> {
        Ok(Self::CreateGroup {
            name: parse_group_name(&value.name)?,
            parent: value.parent.map(parse_group_id).transpose()?,
        })
    }
}

impl TryFrom<RenameGroupRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: RenameGroupRequestDto) -> Result<Self, CommandError> {
        Ok(Self::RenameGroup {
            id: parse_group_id(value.id)?,
            name: parse_group_name(&value.name)?,
        })
    }
}

impl TryFrom<ReparentGroupRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: ReparentGroupRequestDto) -> Result<Self, CommandError> {
        Ok(Self::ReparentGroup {
            id: parse_group_id(value.id)?,
            parent: value.parent.map(parse_group_id).transpose()?,
        })
    }
}

impl TryFrom<DeleteGroupRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: DeleteGroupRequestDto) -> Result<Self, CommandError> {
        Ok(Self::DeleteGroup {
            id: parse_group_id(value.id)?,
            mode: value.mode.into(),
        })
    }
}

impl TryFrom<SetGroupColorRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: SetGroupColorRequestDto) -> Result<Self, CommandError> {
        Ok(Self::SetGroupColor {
            id: parse_group_id(value.id)?,
            color: value.color.map(Into::into),
        })
    }
}

impl GroupMembersRequestDto {
    /// The edit that adds these mods to the group.
    ///
    /// # Errors
    ///
    /// [`CommandError::invalid_input`] for a bad group or mod id.
    pub fn into_add(self) -> Result<OrganizationEdit, CommandError> {
        Ok(OrganizationEdit::AddMembers {
            id: parse_group_id(self.id)?,
            mods: parse_mod_ids(&self.mod_ids)?,
        })
    }

    /// The edit that removes these mods from the group.
    ///
    /// # Errors
    ///
    /// [`CommandError::invalid_input`] for a bad group or mod id.
    pub fn into_remove(self) -> Result<OrganizationEdit, CommandError> {
        Ok(OrganizationEdit::RemoveMembers {
            id: parse_group_id(self.id)?,
            mods: parse_mod_ids(&self.mod_ids)?,
        })
    }
}

impl TryFrom<MoveGroupMembersRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: MoveGroupMembersRequestDto) -> Result<Self, CommandError> {
        Ok(Self::MoveMembers {
            from: parse_group_id(value.from)?,
            to: parse_group_id(value.to)?,
            mods: parse_mod_ids(&value.mod_ids)?,
        })
    }
}

impl TryFrom<SetModColorsRequestDto> for OrganizationEdit {
    type Error = CommandError;

    fn try_from(value: SetModColorsRequestDto) -> Result<Self, CommandError> {
        Ok(Self::SetModColors {
            mods: parse_mod_ids(&value.mod_ids)?,
            color: value.color.map(Into::into),
        })
    }
}
