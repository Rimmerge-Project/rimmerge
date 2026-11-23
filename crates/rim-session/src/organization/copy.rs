//! Copying another profile's organization into this one: the pure plan.
//!
//! [`plan_copy`] never mutates its inputs. It builds the organization the
//! copy would leave behind (through the aggregate's own mutators, so every
//! rule and limit that guards an edit guards the copy too) and reports what
//! the copy does. The caller decides whether to save the result.
//!
//! **Merge** matches a source group to an existing group by its *path*, the
//! chain of names from the top level (compared case-insensitively, like
//! sibling uniqueness). A match takes the source's members; a source group
//! with no match is created under the parent its own parent matched or
//! created. The same name at a different depth is a different group. This
//! profile's colours win: a group colour or a mod colour already set here
//! is kept (a clash, reported), and only an unset one is filled from the
//! source.
//!
//! **Replace** builds the result from the source alone, starting from an
//! empty organization that keeps this one's next group id so no id this
//! profile ever handed out is reused.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ColorToken, Group, GroupId, GroupName, ModOrganization, OrganizationDocumentError,
    OrganizationError, OrganizedModId,
};

/// How a copy combines the source with this profile's organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    /// Add the source's groups, members and colours to what is here.
    Merge,
    /// Discard what is here; the source becomes the whole organization.
    Replace,
}

/// A group both profiles have, at the same path, that carry different
/// colours: this profile's colour is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupColorClash {
    /// The group's names from the top level, as this profile spells them.
    pub path: Vec<GroupName>,
    /// The colour kept (this profile's).
    pub kept: ColorToken,
    /// The source's colour that was not applied.
    pub offered: ColorToken,
}

/// What this profile's organization loses in a [`CopyMode::Replace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Discarded {
    /// Groups removed.
    pub groups: usize,
    /// Memberships removed.
    pub memberships: usize,
    /// Mod colours removed.
    pub mod_colors: usize,
}

/// What a copy does, in the counts and clashes the interface shows before
/// and after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyReport {
    /// The mode that was planned.
    pub mode: CopyMode,
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
    /// Merged groups whose colour differs; here wins. In tree order.
    pub clashes: Vec<GroupColorClash>,
    /// What a replace discards; all zero for a merge.
    pub discarded: Discarded,
    /// Distinct mods the source puts in groups that this install does not
    /// have on disk. Their memberships are kept, as for any group.
    /// Filled in by the use case, which knows the install; `0` from
    /// [`plan_copy`].
    pub not_installed_mods: usize,
}

/// The organization a copy leaves behind and its report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPlan {
    /// The whole organization after the copy.
    pub result: ModOrganization,
    /// What the copy did.
    pub report: CopyReport,
}

impl CopyReport {
    fn new(mode: CopyMode, discarded: Discarded) -> Self {
        Self {
            mode,
            groups_added: 0,
            groups_merged: 0,
            memberships_added: 0,
            memberships_already: 0,
            mod_colors_added: 0,
            mod_colors_kept: 0,
            group_colors_adopted: 0,
            clashes: Vec::new(),
            discarded,
            not_installed_mods: 0,
        }
    }
}

/// Plans copying `source` into `target`. Neither is changed.
///
/// # Errors
///
/// The [`OrganizationError`] of the first rule the result would break (too
/// many groups, members or coloured mods, or group ids used up). The copy
/// is all or nothing, so a refusal means nothing at all would be copied.
pub fn plan_copy(
    target: &ModOrganization,
    source: &ModOrganization,
    mode: CopyMode,
) -> Result<CopyPlan, OrganizationError> {
    let (mut result, discarded) = match mode {
        CopyMode::Merge => (target.clone(), Discarded::default()),
        CopyMode::Replace => (empty_keeping_ids(target)?, discarded_by(target)),
    };
    let mut report = CopyReport::new(mode, discarded);
    copy_groups(&mut result, source, &mut report)?;
    copy_mod_colors(&mut result, source, &mut report)?;
    Ok(CopyPlan { result, report })
}

fn discarded_by(target: &ModOrganization) -> Discarded {
    Discarded {
        groups: target.group_count(),
        memberships: target.membership_count(),
        mod_colors: target.mod_color_count(),
    }
}

/// An organization with no groups that hands out ids from where `target`
/// stopped.
fn empty_keeping_ids(target: &ModOrganization) -> Result<ModOrganization, OrganizationError> {
    ModOrganization::from_parts(Vec::new(), BTreeMap::new(), target.next_group_id()).map_err(
        |error| match error {
            OrganizationDocumentError::Rule(rule) => rule,
            // An id this profile handed out is in range by construction;
            // the nearest rule is "no ids left".
            OrganizationDocumentError::DanglingParent { .. }
            | OrganizationDocumentError::DuplicateGroupId(_)
            | OrganizationDocumentError::GroupIdOutOfRange { .. }
            | OrganizationDocumentError::NextIdBehind { .. } => {
                OrganizationError::GroupIdsExhausted
            }
        },
    )
}

/// The group in `organization` under `parent` named like `name`.
fn find_child(
    organization: &ModOrganization,
    parent: Option<GroupId>,
    name: &GroupName,
) -> Option<GroupId> {
    let key = name.uniqueness_key();
    organization
        .groups()
        .find(|(_, group)| group.parent() == parent && group.name().uniqueness_key() == key)
        .map(|(id, _)| id)
}

/// The names from the top level down to `id`.
fn path_of(organization: &ModOrganization, id: GroupId) -> Vec<GroupName> {
    let mut path = Vec::new();
    let mut current = Some(id);
    while let Some(group) = current.and_then(|id| organization.group(id)) {
        path.push(group.name().clone());
        current = group.parent();
    }
    path.reverse();
    path
}

fn copy_groups(
    result: &mut ModOrganization,
    source: &ModOrganization,
    report: &mut CopyReport,
) -> Result<(), OrganizationError> {
    // Parents come before children in tree order, so a parent's place in
    // `result` is always known by the time its children are placed.
    let mut placed: BTreeMap<GroupId, GroupId> = BTreeMap::new();
    for node in source.tree() {
        let group = source
            .group(node.id)
            .ok_or(OrganizationError::GroupNotFound(node.id))?;
        let parent = node
            .parent
            .map(|parent| {
                placed
                    .get(&parent)
                    .copied()
                    .ok_or(OrganizationError::GroupNotFound(parent))
            })
            .transpose()?;
        let here = match find_child(result, parent, &node.name) {
            Some(existing) => {
                merge_into(result, existing, group, report)?;
                existing
            }
            None => {
                let created =
                    result.create_group_with_members(node.name.clone(), parent, group.members())?;
                result.set_group_color(created, group.color())?;
                report.groups_added += 1;
                report.memberships_added += group.members().len();
                created
            }
        };
        placed.insert(node.id, here);
    }
    Ok(())
}

fn merge_into(
    result: &mut ModOrganization,
    existing: GroupId,
    source_group: &Group,
    report: &mut CopyReport,
) -> Result<(), OrganizationError> {
    let added = result.add_members(existing, source_group.members())?;
    report.groups_merged += 1;
    report.memberships_added += added.added;
    report.memberships_already += added.already;
    let kept = result.group(existing).and_then(Group::color);
    match (kept, source_group.color()) {
        (None, Some(color)) => {
            result.set_group_color(existing, Some(color))?;
            report.group_colors_adopted += 1;
        }
        (Some(kept), Some(offered)) if kept != offered => report.clashes.push(GroupColorClash {
            path: path_of(result, existing),
            kept,
            offered,
        }),
        (Some(_) | None, Some(_) | None) => {}
    }
    Ok(())
}

fn copy_mod_colors(
    result: &mut ModOrganization,
    source: &ModOrganization,
    report: &mut CopyReport,
) -> Result<(), OrganizationError> {
    let mut to_set: BTreeMap<ColorToken, BTreeSet<OrganizedModId>> = BTreeMap::new();
    for (id, color) in source.mod_colors() {
        match result.color_of(id) {
            None => {
                to_set.entry(color).or_default().insert(id.clone());
                report.mod_colors_added += 1;
            }
            Some(kept) if kept != color => report.mod_colors_kept += 1,
            Some(_) => {}
        }
    }
    for (color, mods) in to_set {
        result.set_mod_colors(&mods, Some(color))?;
    }
    Ok(())
}
