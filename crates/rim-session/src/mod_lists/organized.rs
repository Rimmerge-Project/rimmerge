//! The organization join of the two Mods lists: group scope filter, group
//! labels and mod colour on each row. Built once per list call.

use std::collections::{BTreeMap, BTreeSet};

use super::{GroupFilter, GroupLabel, ModListError, ModListRow};
use crate::organization::{GroupId, ModOrganization, OrganizationState, OrganizedModId};

/// What a group filter admits.
enum Admit {
    /// No filter: every row.
    Everything,
    /// Rows whose base id is in the scope's mods.
    InScope(BTreeSet<OrganizedModId>),
    /// Rows that are in no group.
    Ungrouped,
}

/// The organization as one list call reads it.
pub(super) struct ListOrganization<'a> {
    organization: &'a ModOrganization,
    /// Each mod's direct groups in group-tree order (depth first, siblings
    /// by name then id), the order the groups panel lists them in.
    memberships: BTreeMap<&'a OrganizedModId, Vec<GroupId>>,
    admit: Admit,
}

impl<'a> ListOrganization<'a> {
    /// Opens the organization for one list call.
    ///
    /// `None` when organization is unavailable and no filter needs it: the
    /// rows then carry no labels. A filter with unavailable organization,
    /// or one naming a missing group, is an error, never "no filter".
    pub(super) fn open(
        state: &'a OrganizationState,
        filter: Option<GroupFilter>,
    ) -> Result<Option<Self>, ModListError> {
        let organization = match state.loaded() {
            Ok(organization) => organization,
            Err(unavailable) => {
                return match filter {
                    Some(_) => Err(ModListError::OrganizationUnavailable(unavailable)),
                    None => Ok(None),
                };
            }
        };
        let admit = match filter {
            None => Admit::Everything,
            Some(GroupFilter::Ungrouped) => Admit::Ungrouped,
            Some(GroupFilter::InScope(id)) => match organization.scope(id) {
                Ok(scope) => Admit::InScope(scope.mods),
                // `scope` refuses only with `GroupNotFound`.
                Err(_) => return Err(ModListError::UnknownGroup(id)),
            },
        };
        Ok(Some(Self {
            organization,
            memberships: Self::memberships_in_tree_order(organization),
            admit,
        }))
    }

    fn memberships_in_tree_order(
        organization: &'a ModOrganization,
    ) -> BTreeMap<&'a OrganizedModId, Vec<GroupId>> {
        let mut memberships: BTreeMap<&OrganizedModId, Vec<GroupId>> = BTreeMap::new();
        for node in organization.tree() {
            let Some(group) = organization.group(node.id) else {
                continue;
            };
            for member in group.members() {
                memberships.entry(member).or_default().push(node.id);
            }
        }
        memberships
    }

    /// Gives `row` its group labels and its own colour.
    pub(super) fn decorate(this: Option<&Self>, mut row: ModListRow) -> ModListRow {
        let Some(this) = this else {
            return row;
        };
        let Ok(base) = OrganizedModId::new(&row.mod_id) else {
            return row;
        };
        row.color = this.organization.color_of(&base);
        row.groups = this
            .memberships
            .get(&base)
            .into_iter()
            .flatten()
            .filter_map(|id| {
                this.organization.group(*id).map(|group| GroupLabel {
                    id: *id,
                    name: group.name().clone(),
                    color: group.color(),
                })
            })
            .collect();
        row
    }

    /// Whether the group filter lets `row` through.
    pub(super) fn admits(this: Option<&Self>, row: &ModListRow) -> bool {
        let Some(this) = this else {
            return true;
        };
        let base = OrganizedModId::new(&row.mod_id).ok();
        match &this.admit {
            Admit::Everything => true,
            Admit::InScope(mods) => base.is_some_and(|base| mods.contains(&base)),
            Admit::Ungrouped => base.is_none_or(|base| !this.memberships.contains_key(&base)),
        }
    }
}
