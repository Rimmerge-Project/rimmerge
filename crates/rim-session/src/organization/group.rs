//! One group of the forest: its name, place, colour and direct members.

use std::collections::BTreeSet;

use super::{ColorToken, GroupId, GroupName, OrganizedModId};

/// One group. Fields are private so every change goes through
/// [`super::ModOrganization`], which keeps the forest's invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub(super) name: GroupName,
    pub(super) parent: Option<GroupId>,
    pub(super) color: Option<ColorToken>,
    pub(super) members: BTreeSet<OrganizedModId>,
}

impl Group {
    /// The group's name.
    #[must_use]
    pub fn name(&self) -> &GroupName {
        &self.name
    }

    /// Its parent, `None` for a top-level group.
    #[must_use]
    pub fn parent(&self) -> Option<GroupId> {
        self.parent
    }

    /// Its colour, when it has one.
    #[must_use]
    pub fn color(&self) -> Option<ColorToken> {
        self.color
    }

    /// The mods that are **direct** members; a subgroup's members are not
    /// listed here (see [`super::ModOrganization::scope`]).
    #[must_use]
    pub fn members(&self) -> &BTreeSet<OrganizedModId> {
        &self.members
    }
}

/// One group as a persisted document states it, before the aggregate has
/// checked the forest it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupParts {
    /// The group's id.
    pub id: GroupId,
    /// Its name.
    pub name: GroupName,
    /// Its parent.
    pub parent: Option<GroupId>,
    /// Its colour.
    pub color: Option<ColorToken>,
    /// Its direct members.
    pub members: BTreeSet<OrganizedModId>,
}
