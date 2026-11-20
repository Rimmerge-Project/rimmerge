//! [`OrganizationEdit`]: every change a user can make to the organization,
//! as one closed command type the use case applies and persists once.

use std::collections::BTreeSet;

use super::{
    ColorChange, ColorToken, DeleteMode, DeleteOutcome, GroupId, GroupName, MembershipAdd,
    MembershipMove, MembershipRemoval, ModOrganization, OrganizationError, OrganizedModId,
};

/// One organization change. Already-parsed data: ids and names are value
/// objects, never raw text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizationEdit {
    /// Create an empty group.
    CreateGroup {
        /// Its name.
        name: GroupName,
        /// Its parent, `None` for top level.
        parent: Option<GroupId>,
    },
    /// Rename a group.
    RenameGroup {
        /// The group.
        id: GroupId,
        /// Its new name.
        name: GroupName,
    },
    /// Move a group, with its subtree.
    ReparentGroup {
        /// The group.
        id: GroupId,
        /// Its new parent, `None` for top level.
        parent: Option<GroupId>,
    },
    /// Delete a group.
    DeleteGroup {
        /// The group.
        id: GroupId,
        /// What happens to its subgroups.
        mode: DeleteMode,
    },
    /// Set or clear a group's colour.
    SetGroupColor {
        /// The group.
        id: GroupId,
        /// The colour, `None` to clear.
        color: Option<ColorToken>,
    },
    /// Add mods to a group.
    AddMembers {
        /// The group.
        id: GroupId,
        /// The mods.
        mods: BTreeSet<OrganizedModId>,
    },
    /// Remove mods from a group.
    RemoveMembers {
        /// The group.
        id: GroupId,
        /// The mods.
        mods: BTreeSet<OrganizedModId>,
    },
    /// Move mods from one group to another.
    MoveMembers {
        /// The source group.
        from: GroupId,
        /// The target group.
        to: GroupId,
        /// The mods.
        mods: BTreeSet<OrganizedModId>,
    },
    /// Set or clear mods' own colour.
    SetModColors {
        /// The mods.
        mods: BTreeSet<OrganizedModId>,
        /// The colour, `None` to clear.
        color: Option<ColorToken>,
    },
}

/// What an [`OrganizationEdit`] did, in the counts its action reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    /// A group was created.
    Created(GroupId),
    /// A rename, move or colour change of a group: nothing to count.
    Done,
    /// A group was deleted.
    Deleted(DeleteOutcome),
    /// Mods were added to a group.
    Added(MembershipAdd),
    /// Mods were removed from a group.
    Removed(MembershipRemoval),
    /// Mods were moved between groups.
    Moved(MembershipMove),
    /// Mods were coloured or reset.
    Colored(ColorChange),
}

impl OrganizationEdit {
    /// Applies the edit. On error the organization is unchanged.
    ///
    /// # Errors
    ///
    /// Whatever [`OrganizationError`] the underlying mutator refuses with.
    pub fn apply(
        self,
        organization: &mut ModOrganization,
    ) -> Result<EditOutcome, OrganizationError> {
        match self {
            Self::CreateGroup { name, parent } => organization
                .create_group(name, parent)
                .map(EditOutcome::Created),
            Self::RenameGroup { id, name } => organization
                .rename_group(id, name)
                .map(|()| EditOutcome::Done),
            Self::ReparentGroup { id, parent } => organization
                .reparent_group(id, parent)
                .map(|()| EditOutcome::Done),
            Self::DeleteGroup { id, mode } => organization
                .delete_group(id, mode)
                .map(EditOutcome::Deleted),
            Self::SetGroupColor { id, color } => organization
                .set_group_color(id, color)
                .map(|()| EditOutcome::Done),
            Self::AddMembers { id, mods } => {
                organization.add_members(id, &mods).map(EditOutcome::Added)
            }
            Self::RemoveMembers { id, mods } => organization
                .remove_members(id, &mods)
                .map(EditOutcome::Removed),
            Self::MoveMembers { from, to, mods } => organization
                .move_members(from, to, &mods)
                .map(EditOutcome::Moved),
            Self::SetModColors { mods, color } => organization
                .set_mod_colors(&mods, color)
                .map(EditOutcome::Colored),
        }
    }
}
