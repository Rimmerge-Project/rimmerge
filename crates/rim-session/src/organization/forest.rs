//! [`ModOrganization`]: the aggregate. A forest of groups (each with
//! **direct** members only) plus per-mod colours, every mutator checking
//! the invariants before it changes anything.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::group::{Group, GroupParts};
use super::{ColorToken, GroupId, GroupName, OrganizationLimits, OrganizedModId};

/// An organization rule was refused. The aggregate is unchanged whenever
/// one of these is returned.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationError {
    /// No group has this id.
    #[error("no group {0}")]
    GroupNotFound(GroupId),
    /// The move would put a group inside itself or its own subgroup.
    #[error("a group cannot go inside itself or one of its own subgroups")]
    WouldCreateCycle,
    /// The result would nest deeper than allowed.
    #[error("groups nest at most {limit} levels")]
    TooDeep {
        /// [`OrganizationLimits::MAX_DEPTH`].
        limit: usize,
    },
    /// There would be more groups than allowed.
    #[error("there can be at most {limit} groups")]
    TooManyGroups {
        /// [`OrganizationLimits::MAX_GROUPS`].
        limit: usize,
    },
    /// Every group number the file may hand out has been used (deleted
    /// groups never give theirs back), so no group can be created, though
    /// the group count is within its limit.
    #[error("every group number has been used, so no new group can be created")]
    GroupIdsExhausted,
    /// A sibling already has this name (compared case-insensitively).
    #[error("a group named {name:?} already exists here")]
    NameTaken {
        /// The refused name, as written.
        name: String,
    },
    /// A move named the same group as source and target.
    #[error("the source and the target are the same group")]
    SameGroup,
    /// A group or the whole organization would hold too many memberships.
    #[error("too many mods in groups (limit {limit})")]
    TooManyMembers {
        /// The limit that was hit.
        limit: usize,
    },
    /// Too many mods would carry a colour.
    #[error("at most {limit} mods can carry a colour")]
    TooManyColors {
        /// [`OrganizationLimits::MAX_MOD_COLORS`].
        limit: usize,
    },
}

/// A persisted document was refused. Separate from [`OrganizationError`]
/// because these can only come out of reading a file: an edit of a loaded
/// organization never reports one, so the interface's edit mapping has no
/// arm for them.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationDocumentError {
    /// A group names a parent that no group has.
    #[error("group {group} names a parent {parent} that does not exist")]
    DanglingParent {
        /// The group.
        group: GroupId,
        /// The missing parent.
        parent: GroupId,
    },
    /// The document uses one group id twice.
    #[error("group id {0} appears more than once")]
    DuplicateGroupId(GroupId),
    /// A group id, or the next id to hand out, is beyond
    /// [`OrganizationLimits::MAX_GROUP_ID`].
    #[error("group id {id} is above the limit {limit}")]
    GroupIdOutOfRange {
        /// The offending id.
        id: u32,
        /// The largest id allowed (for the next id, one more).
        limit: u32,
    },
    /// The next id to hand out would reuse an id already in the document.
    #[error("the next group id {next_id} is behind the existing ids (at least {minimum})")]
    NextIdBehind {
        /// The document's next id.
        next_id: u32,
        /// The smallest next id the groups allow.
        minimum: u32,
    },
    /// The groups, colours or memberships break an organization rule.
    #[error(transparent)]
    Rule(#[from] OrganizationError),
}

/// How [`ModOrganization::delete_group`] treats the deleted group's
/// subgroups. Neither mode touches a mod: memberships go with the groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    /// The subgroups move up to the deleted group's own parent.
    LiftChildren,
    /// The whole subtree is deleted.
    WithSubgroups,
}

/// What [`ModOrganization::delete_group`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteOutcome {
    /// Groups removed, the deleted group included.
    pub groups_removed: usize,
    /// Subgroups moved up a level.
    pub children_lifted: usize,
}

/// What [`ModOrganization::add_members`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembershipAdd {
    /// Mods that became members.
    pub added: usize,
    /// Mods that already were.
    pub already: usize,
}

/// What [`ModOrganization::remove_members`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembershipRemoval {
    /// Mods that stopped being members.
    pub removed: usize,
    /// Mods that were not members.
    pub absent: usize,
}

/// What [`ModOrganization::move_members`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembershipMove {
    /// Mods that left the source and joined the target.
    pub moved: usize,
    /// Mods that left the source; the target already held them.
    pub already_in_target: usize,
    /// Mods that were not direct members of the source and so stayed put.
    pub not_in_source: usize,
}

/// What [`ModOrganization::set_mod_colors`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorChange {
    /// Mods whose colour changed.
    pub changed: usize,
    /// Mods that already had that colour (or none, for a reset).
    pub unchanged: usize,
}

/// A group's mods, descendants included, as [`ModOrganization::scope`]
/// derives them. Never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupScope {
    /// The group and every group below it.
    pub groups: BTreeSet<GroupId>,
    /// The deduplicated union of those groups' direct members.
    pub mods: BTreeSet<OrganizedModId>,
}

/// One group as the tree lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupNode {
    /// The group.
    pub id: GroupId,
    /// Its name.
    pub name: GroupName,
    /// Its colour.
    pub color: Option<ColorToken>,
    /// Its parent.
    pub parent: Option<GroupId>,
    /// Its level, 1 for a top-level group.
    pub depth: usize,
    /// Its direct subgroups.
    pub child_count: usize,
    /// Its direct members.
    pub direct_count: usize,
    /// Its members counting subgroups, each mod once.
    pub scope_count: usize,
}

/// The forest of groups and the per-mod colours. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModOrganization {
    groups: BTreeMap<GroupId, Group>,
    mod_colors: BTreeMap<OrganizedModId, ColorToken>,
    /// The next group id to hand out. Monotonic and persisted with the
    /// document, so a deleted group's id is never reused, not even after
    /// the file is read again.
    next_id: u32,
}

impl Default for ModOrganization {
    fn default() -> Self {
        Self::new()
    }
}

impl ModOrganization {
    /// An organization with no groups and no colours.
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: BTreeMap::new(),
            mod_colors: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Builds an organization from a persisted document, rejecting the
    /// whole document when any invariant fails. `next_id` is the id the
    /// document says to hand out next.
    ///
    /// # Errors
    ///
    /// Returns the first [`OrganizationDocumentError`] found: an id out of
    /// range or used twice, a `next_id` behind the existing ids, a dangling
    /// parent, or a broken [`OrganizationError`] rule (a cycle, too deep a
    /// chain, a duplicate sibling name, a limit exceeded).
    pub fn from_parts(
        parts: Vec<GroupParts>,
        mod_colors: BTreeMap<OrganizedModId, ColorToken>,
        next_id: u32,
    ) -> Result<Self, OrganizationDocumentError> {
        if parts.len() > OrganizationLimits::MAX_GROUPS {
            return Err(OrganizationError::TooManyGroups {
                limit: OrganizationLimits::MAX_GROUPS,
            }
            .into());
        }
        if mod_colors.len() > OrganizationLimits::MAX_MOD_COLORS {
            return Err(OrganizationError::TooManyColors {
                limit: OrganizationLimits::MAX_MOD_COLORS,
            }
            .into());
        }
        let mut groups = BTreeMap::new();
        for part in parts {
            let id_limit = OrganizationLimits::MAX_GROUP_ID;
            if part.id.get() > id_limit {
                return Err(OrganizationDocumentError::GroupIdOutOfRange {
                    id: part.id.get(),
                    limit: id_limit,
                });
            }
            let group = Group {
                name: part.name,
                parent: part.parent,
                color: part.color,
                members: part.members,
            };
            if groups.insert(part.id, group).is_some() {
                return Err(OrganizationDocumentError::DuplicateGroupId(part.id));
            }
        }
        // Ids are capped at `MAX_GROUP_ID`, so `+ 1` cannot overflow.
        let minimum = groups.keys().next_back().map_or(1, |last| last.get() + 1);
        if next_id > OrganizationLimits::MAX_GROUP_ID + 1 {
            return Err(OrganizationDocumentError::GroupIdOutOfRange {
                id: next_id,
                limit: OrganizationLimits::MAX_GROUP_ID + 1,
            });
        }
        if next_id < minimum {
            return Err(OrganizationDocumentError::NextIdBehind { next_id, minimum });
        }
        let organization = Self {
            groups,
            mod_colors,
            next_id,
        };
        organization.check_loaded_forest()?;
        Ok(organization)
    }

    fn check_loaded_forest(&self) -> Result<(), OrganizationDocumentError> {
        for (&id, group) in &self.groups {
            if let Some(parent) = group.parent
                && !self.groups.contains_key(&parent)
            {
                return Err(OrganizationDocumentError::DanglingParent { group: id, parent });
            }
        }
        for &id in self.groups.keys() {
            self.check_chain_to_root(id)?;
        }
        let mut seen_names = BTreeSet::new();
        for group in self.groups.values() {
            if !seen_names.insert((group.parent, group.name.uniqueness_key())) {
                return Err(OrganizationError::NameTaken {
                    name: group.name.to_string(),
                }
                .into());
            }
        }
        Ok(self.check_member_limits()?)
    }

    /// Walks from `start` to a root with a visited set, so a cycle in a
    /// hostile document ends in an error instead of a loop.
    fn check_chain_to_root(&self, start: GroupId) -> Result<(), OrganizationError> {
        let mut visited = BTreeSet::from([start]);
        let mut current = start;
        while let Some(parent) = self.groups.get(&current).and_then(|group| group.parent) {
            if !visited.insert(parent) {
                return Err(OrganizationError::WouldCreateCycle);
            }
            if visited.len() > OrganizationLimits::MAX_DEPTH {
                return Err(OrganizationError::TooDeep {
                    limit: OrganizationLimits::MAX_DEPTH,
                });
            }
            current = parent;
        }
        Ok(())
    }

    fn check_member_limits(&self) -> Result<(), OrganizationError> {
        let over_group_limit = self
            .groups
            .values()
            .any(|group| group.members.len() > OrganizationLimits::MAX_MEMBERS_PER_GROUP);
        if over_group_limit {
            return Err(OrganizationError::TooManyMembers {
                limit: OrganizationLimits::MAX_MEMBERS_PER_GROUP,
            });
        }
        if self.membership_count() > OrganizationLimits::MAX_TOTAL_MEMBERSHIPS {
            return Err(OrganizationError::TooManyMembers {
                limit: OrganizationLimits::MAX_TOTAL_MEMBERSHIPS,
            });
        }
        Ok(())
    }

    // ---- reads ---------------------------------------------------------

    /// The group with this id.
    #[must_use]
    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(&id)
    }

    /// The id the next created group gets. Persisted with the document so a
    /// deleted group's id is never handed out again.
    #[must_use]
    pub fn next_group_id(&self) -> u32 {
        self.next_id
    }

    /// Every group, in id order.
    pub fn groups(&self) -> impl Iterator<Item = (GroupId, &Group)> {
        self.groups.iter().map(|(id, group)| (*id, group))
    }

    /// How many groups there are.
    #[must_use]
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Whether there is nothing at all: no group and no colour.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.mod_colors.is_empty()
    }

    /// How many (group, mod) memberships exist in total.
    #[must_use]
    pub fn membership_count(&self) -> usize {
        self.groups.values().map(|group| group.members.len()).sum()
    }

    /// Every mod colour, by id.
    pub fn mod_colors(&self) -> impl Iterator<Item = (&OrganizedModId, ColorToken)> {
        self.mod_colors.iter().map(|(id, color)| (id, *color))
    }

    /// How many mods carry a colour.
    #[must_use]
    pub fn mod_color_count(&self) -> usize {
        self.mod_colors.len()
    }

    /// A mod's own colour. A mod never inherits a group's colour.
    #[must_use]
    pub fn color_of(&self, mod_id: &OrganizedModId) -> Option<ColorToken> {
        self.mod_colors.get(mod_id).copied()
    }

    /// The groups a mod is a **direct** member of.
    #[must_use]
    pub fn groups_of(&self, mod_id: &OrganizedModId) -> BTreeSet<GroupId> {
        self.groups
            .iter()
            .filter(|(_, group)| group.members.contains(mod_id))
            .map(|(id, _)| *id)
            .collect()
    }

    /// A group's mods with its descendants', each mod once. Derived on
    /// every call: no ancestor membership is ever stored.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationError::GroupNotFound`] for an unknown group.
    pub fn scope(&self, id: GroupId) -> Result<GroupScope, OrganizationError> {
        self.require(id)?;
        let groups = self.subtree(id);
        let mods = groups
            .iter()
            .filter_map(|member| self.groups.get(member))
            .flat_map(|group| group.members.iter().cloned())
            .collect();
        Ok(GroupScope { groups, mods })
    }

    /// The forest depth first, siblings by name (lowercased, then as
    /// written, then id).
    #[must_use]
    pub fn tree(&self) -> Vec<GroupNode> {
        let children = self.children_index();
        let mut nodes = Vec::with_capacity(self.groups.len());
        self.push_nodes(None, 1, &children, &mut nodes);
        nodes
    }

    fn push_nodes(
        &self,
        parent: Option<GroupId>,
        depth: usize,
        children: &BTreeMap<Option<GroupId>, Vec<GroupId>>,
        nodes: &mut Vec<GroupNode>,
    ) {
        let Some(siblings) = children.get(&parent) else {
            return;
        };
        for &id in siblings {
            let Some(group) = self.groups.get(&id) else {
                continue;
            };
            nodes.push(GroupNode {
                id,
                name: group.name.clone(),
                color: group.color,
                parent: group.parent,
                depth,
                child_count: children.get(&Some(id)).map_or(0, Vec::len),
                direct_count: group.members.len(),
                scope_count: self.scope_members(id, children).len(),
            });
            self.push_nodes(Some(id), depth + 1, children, nodes);
        }
    }

    fn scope_members<'a>(
        &'a self,
        id: GroupId,
        children: &BTreeMap<Option<GroupId>, Vec<GroupId>>,
    ) -> BTreeSet<&'a OrganizedModId> {
        let mut members: BTreeSet<&OrganizedModId> = self
            .groups
            .get(&id)
            .map(|group| group.members.iter().collect())
            .unwrap_or_default();
        for &child in children.get(&Some(id)).into_iter().flatten() {
            members.extend(self.scope_members(child, children));
        }
        members
    }

    /// Every target group `id` may be moved under (`None` is the top
    /// level), the current parent included: the ones that keep the forest
    /// acyclic, within the depth limit and free of a sibling name clash.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationError::GroupNotFound`] for an unknown group.
    pub fn valid_parents(
        &self,
        id: GroupId,
    ) -> Result<BTreeSet<Option<GroupId>>, OrganizationError> {
        let group = self.require(id)?;
        let own_subtree = self.subtree(id);
        let height = self.height_of(id);
        let is_free = |parent: Option<GroupId>| {
            self.is_name_free(parent, &group.name, Some(id)) || group.parent == parent
        };
        let mut valid = BTreeSet::new();
        if is_free(None) && height <= OrganizationLimits::MAX_DEPTH {
            valid.insert(None);
        }
        for &candidate in self.groups.keys() {
            let fits = !own_subtree.contains(&candidate)
                && self.depth_of(candidate) + height <= OrganizationLimits::MAX_DEPTH;
            if fits && is_free(Some(candidate)) {
                valid.insert(Some(candidate));
            }
        }
        Ok(valid)
    }

    // ---- group mutators ------------------------------------------------

    /// Creates an empty group under `parent` (`None` for top level).
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`], [`OrganizationError::TooDeep`],
    /// [`OrganizationError::TooManyGroups`] or
    /// [`OrganizationError::NameTaken`].
    pub fn create_group(
        &mut self,
        name: GroupName,
        parent: Option<GroupId>,
    ) -> Result<GroupId, OrganizationError> {
        let too_many = OrganizationError::TooManyGroups {
            limit: OrganizationLimits::MAX_GROUPS,
        };
        if self.groups.len() >= OrganizationLimits::MAX_GROUPS {
            return Err(too_many);
        }
        let depth = match parent {
            Some(parent) => {
                self.require(parent)?;
                self.depth_of(parent) + 1
            }
            None => 1,
        };
        if depth > OrganizationLimits::MAX_DEPTH {
            return Err(OrganizationError::TooDeep {
                limit: OrganizationLimits::MAX_DEPTH,
            });
        }
        self.ensure_name_free(parent, &name, None)?;
        if self.next_id > OrganizationLimits::MAX_GROUP_ID {
            return Err(OrganizationError::GroupIdsExhausted);
        }
        let id = GroupId::new(self.next_id).ok_or(OrganizationError::GroupIdsExhausted)?;
        let next_id = self.next_id + 1;
        self.groups.insert(
            id,
            Group {
                name,
                parent,
                color: None,
                members: BTreeSet::new(),
            },
        );
        self.next_id = next_id;
        Ok(id)
    }

    /// Renames a group.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`] or
    /// [`OrganizationError::NameTaken`].
    pub fn rename_group(&mut self, id: GroupId, name: GroupName) -> Result<(), OrganizationError> {
        let parent = self.require(id)?.parent;
        self.ensure_name_free(parent, &name, Some(id))?;
        self.require_mut(id)?.name = name;
        Ok(())
    }

    /// Moves a group (with its subtree) under `parent`, `None` for top
    /// level. Moving a group where it already is changes nothing.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`],
    /// [`OrganizationError::WouldCreateCycle`],
    /// [`OrganizationError::TooDeep`] or [`OrganizationError::NameTaken`].
    pub fn reparent_group(
        &mut self,
        id: GroupId,
        parent: Option<GroupId>,
    ) -> Result<(), OrganizationError> {
        let group = self.require(id)?;
        if group.parent == parent {
            return Ok(());
        }
        let parent_depth = match parent {
            Some(parent) => {
                self.require(parent)?;
                if self.subtree(id).contains(&parent) {
                    return Err(OrganizationError::WouldCreateCycle);
                }
                self.depth_of(parent)
            }
            None => 0,
        };
        if parent_depth + self.height_of(id) > OrganizationLimits::MAX_DEPTH {
            return Err(OrganizationError::TooDeep {
                limit: OrganizationLimits::MAX_DEPTH,
            });
        }
        self.ensure_name_free(parent, &group.name, Some(id))?;
        self.require_mut(id)?.parent = parent;
        Ok(())
    }

    /// Deletes a group, per `mode`. No mod is touched.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`], or
    /// [`OrganizationError::NameTaken`] when lifting a subgroup would
    /// clash with a sibling of the deleted group (nothing is changed).
    pub fn delete_group(
        &mut self,
        id: GroupId,
        mode: DeleteMode,
    ) -> Result<DeleteOutcome, OrganizationError> {
        let parent = self.require(id)?.parent;
        match mode {
            DeleteMode::LiftChildren => {
                let children = self.child_ids(id);
                for &child in &children {
                    let name = &self.require(child)?.name;
                    self.ensure_name_free(parent, name, Some(id))?;
                }
                for &child in &children {
                    self.require_mut(child)?.parent = parent;
                }
                self.groups.remove(&id);
                Ok(DeleteOutcome {
                    groups_removed: 1,
                    children_lifted: children.len(),
                })
            }
            DeleteMode::WithSubgroups => {
                let doomed = self.subtree(id);
                for group in &doomed {
                    self.groups.remove(group);
                }
                Ok(DeleteOutcome {
                    groups_removed: doomed.len(),
                    children_lifted: 0,
                })
            }
        }
    }

    /// Sets or clears a group's colour.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`].
    pub fn set_group_color(
        &mut self,
        id: GroupId,
        color: Option<ColorToken>,
    ) -> Result<(), OrganizationError> {
        self.require_mut(id)?.color = color;
        Ok(())
    }

    // ---- membership mutators -------------------------------------------

    /// Adds `mods` as direct members of `id`; mods already in it are
    /// counted, not duplicated. Memberships in other groups are untouched.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`] or
    /// [`OrganizationError::TooManyMembers`].
    pub fn add_members(
        &mut self,
        id: GroupId,
        mods: &BTreeSet<OrganizedModId>,
    ) -> Result<MembershipAdd, OrganizationError> {
        let group = self.require(id)?;
        let added = mods.difference(&group.members).count();
        self.ensure_room(id, added)?;
        self.require_mut(id)?.members.extend(mods.iter().cloned());
        Ok(MembershipAdd {
            added,
            already: mods.len() - added,
        })
    }

    /// Removes `mods` from the direct members of `id`.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`].
    pub fn remove_members(
        &mut self,
        id: GroupId,
        mods: &BTreeSet<OrganizedModId>,
    ) -> Result<MembershipRemoval, OrganizationError> {
        let group = self.require_mut(id)?;
        let removed = mods.intersection(&group.members).count();
        group.members.retain(|member| !mods.contains(member));
        Ok(MembershipRemoval {
            removed,
            absent: mods.len() - removed,
        })
    }

    /// Removes `mods` from every group in `groups` at once (a group scope:
    /// a group and its subgroups). `removed` counts mods that left at least
    /// one group, each once.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::GroupNotFound`] for the first missing group;
    /// nothing changes then.
    pub fn remove_members_from_groups(
        &mut self,
        groups: &BTreeSet<GroupId>,
        mods: &BTreeSet<OrganizedModId>,
    ) -> Result<MembershipRemoval, OrganizationError> {
        for id in groups {
            self.require(*id)?;
        }
        let mut gone: BTreeSet<&OrganizedModId> = BTreeSet::new();
        for id in groups {
            let group = self.require_mut(*id)?;
            gone.extend(mods.iter().filter(|member| group.members.contains(*member)));
            group.members.retain(|member| !mods.contains(member));
        }
        Ok(MembershipRemoval {
            removed: gone.len(),
            absent: mods.len() - gone.len(),
        })
    }

    /// The explicit replace: the `mods` that are direct members of `from`
    /// leave it and join `to`. Other memberships of those mods, and mods
    /// that are not in `from`, are untouched.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::SameGroup`],
    /// [`OrganizationError::GroupNotFound`] or
    /// [`OrganizationError::TooManyMembers`].
    pub fn move_members(
        &mut self,
        from: GroupId,
        to: GroupId,
        mods: &BTreeSet<OrganizedModId>,
    ) -> Result<MembershipMove, OrganizationError> {
        if from == to {
            return Err(OrganizationError::SameGroup);
        }
        let source = self.require(from)?;
        let target = self.require(to)?;
        let leaving: BTreeSet<OrganizedModId> =
            mods.intersection(&source.members).cloned().collect();
        let joining = leaving.difference(&target.members).count();
        self.ensure_room(to, joining)?;
        self.require_mut(from)?
            .members
            .retain(|member| !leaving.contains(member));
        self.require_mut(to)?
            .members
            .extend(leaving.iter().cloned());
        Ok(MembershipMove {
            moved: joining,
            already_in_target: leaving.len() - joining,
            not_in_source: mods.len() - leaving.len(),
        })
    }

    /// Sets (`Some`) or clears (`None`) the colour of every mod in `mods`.
    ///
    /// Colours are keyed by [`OrganizedModId`] (the base id), so the
    /// Workshop copy `x_steam` and the local copy `x` of one package share
    /// one colour: a request naming both collapses to a single mod before
    /// it gets here, and colouring either colours both. The counts are of
    /// distinct mods, not of ids in the request.
    ///
    /// # Errors
    ///
    /// [`OrganizationError::TooManyColors`].
    pub fn set_mod_colors(
        &mut self,
        mods: &BTreeSet<OrganizedModId>,
        color: Option<ColorToken>,
    ) -> Result<ColorChange, OrganizationError> {
        let changed = mods
            .iter()
            .filter(|id| self.mod_colors.get(*id).copied() != color)
            .count();
        let new_entries = match color {
            Some(_) => mods
                .iter()
                .filter(|id| !self.mod_colors.contains_key(*id))
                .count(),
            None => 0,
        };
        if self.mod_colors.len() + new_entries > OrganizationLimits::MAX_MOD_COLORS {
            return Err(OrganizationError::TooManyColors {
                limit: OrganizationLimits::MAX_MOD_COLORS,
            });
        }
        for id in mods {
            match color {
                Some(color) => {
                    self.mod_colors.insert(id.clone(), color);
                }
                None => {
                    self.mod_colors.remove(id);
                }
            }
        }
        Ok(ColorChange {
            changed,
            unchanged: mods.len() - changed,
        })
    }

    // ---- internals -------------------------------------------------------

    fn require(&self, id: GroupId) -> Result<&Group, OrganizationError> {
        self.groups
            .get(&id)
            .ok_or(OrganizationError::GroupNotFound(id))
    }

    fn require_mut(&mut self, id: GroupId) -> Result<&mut Group, OrganizationError> {
        self.groups
            .get_mut(&id)
            .ok_or(OrganizationError::GroupNotFound(id))
    }

    fn child_ids(&self, parent: GroupId) -> Vec<GroupId> {
        self.groups
            .iter()
            .filter(|(_, group)| group.parent == Some(parent))
            .map(|(id, _)| *id)
            .collect()
    }

    /// `id` and every group below it.
    fn subtree(&self, id: GroupId) -> BTreeSet<GroupId> {
        let mut found = BTreeSet::from([id]);
        let mut pending = vec![id];
        while let Some(next) = pending.pop() {
            for child in self.child_ids(next) {
                if found.insert(child) {
                    pending.push(child);
                }
            }
        }
        found
    }

    /// The level of `id`: 1 for a top-level group. The forest is acyclic by
    /// invariant, so the walk ends.
    fn depth_of(&self, id: GroupId) -> usize {
        let mut depth = 1;
        let mut current = id;
        while let Some(parent) = self.groups.get(&current).and_then(|group| group.parent) {
            depth += 1;
            current = parent;
        }
        depth
    }

    /// How many levels the subtree rooted at `id` spans (1 for a leaf).
    fn height_of(&self, id: GroupId) -> usize {
        1 + self
            .child_ids(id)
            .into_iter()
            .map(|child| self.height_of(child))
            .max()
            .unwrap_or(0)
    }

    fn is_name_free(
        &self,
        parent: Option<GroupId>,
        name: &GroupName,
        except: Option<GroupId>,
    ) -> bool {
        let key = name.uniqueness_key();
        !self.groups.iter().any(|(id, group)| {
            group.parent == parent && Some(*id) != except && group.name.uniqueness_key() == key
        })
    }

    fn ensure_name_free(
        &self,
        parent: Option<GroupId>,
        name: &GroupName,
        except: Option<GroupId>,
    ) -> Result<(), OrganizationError> {
        if self.is_name_free(parent, name, except) {
            Ok(())
        } else {
            Err(OrganizationError::NameTaken {
                name: name.to_string(),
            })
        }
    }

    /// Whether `extra` more members fit in group `id` and in total.
    fn ensure_room(&self, id: GroupId, extra: usize) -> Result<(), OrganizationError> {
        let in_group = self.require(id)?.members.len();
        if in_group + extra > OrganizationLimits::MAX_MEMBERS_PER_GROUP {
            return Err(OrganizationError::TooManyMembers {
                limit: OrganizationLimits::MAX_MEMBERS_PER_GROUP,
            });
        }
        if self.membership_count() + extra > OrganizationLimits::MAX_TOTAL_MEMBERSHIPS {
            return Err(OrganizationError::TooManyMembers {
                limit: OrganizationLimits::MAX_TOTAL_MEMBERSHIPS,
            });
        }
        Ok(())
    }

    /// Children by parent, each list ordered by name key then id.
    fn children_index(&self) -> BTreeMap<Option<GroupId>, Vec<GroupId>> {
        let mut index: BTreeMap<Option<GroupId>, Vec<GroupId>> = BTreeMap::new();
        for (id, group) in &self.groups {
            index.entry(group.parent).or_default().push(*id);
        }
        for siblings in index.values_mut() {
            siblings.sort_by(|left, right| self.compare_siblings(*left, *right));
        }
        index
    }

    fn compare_siblings(&self, left: GroupId, right: GroupId) -> Ordering {
        let by_name = match (self.groups.get(&left), self.groups.get(&right)) {
            (Some(left_group), Some(right_group)) => {
                left_group.name.display_order(&right_group.name)
            }
            _ => Ordering::Equal,
        };
        by_name.then_with(|| left.cmp(&right))
    }
}
