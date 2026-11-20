//! Resolving organized (base) ids against the install: what a group-level
//! activate or deactivate would plan over.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;

use super::{GroupScope, OrganizedModId};
use crate::active_set::ActiveSet;
use crate::mod_inventory::ModInventory;

/// A group scope's mods, split by what they are in this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScope {
    /// Mods in the working set, by exact id, in working order.
    pub active: Vec<ModId>,
    /// Installed mods outside the working set, by the exact id of the copy
    /// [`ModInventory::find_by_base`] picks, in base-id order.
    pub inactive: Vec<ModId>,
    /// Members found nowhere on disk. Their memberships are kept: they
    /// come back when the mod is reinstalled.
    pub not_installed: usize,
}

/// A group's scope and what it resolves to in this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupScopeResolution {
    /// The group, its descendants and the deduplicated members.
    pub scope: GroupScope,
    /// The members split into active, inactive and not installed.
    pub resolved: ResolvedScope,
}

/// Resolves `mods` (base ids) against the install. A copy already in
/// `working` wins over an inactive copy of the same package.
#[must_use]
pub fn resolve_scope(
    mods: &BTreeSet<OrganizedModId>,
    inventory: &ModInventory,
    working: &ActiveSet,
) -> ResolvedScope {
    let bases: BTreeSet<&ModId> = mods.iter().map(OrganizedModId::as_mod_id).collect();
    let working_bases: BTreeSet<ModId> = working.ids().iter().map(ModId::base).collect();
    let active: Vec<ModId> = working
        .ids()
        .iter()
        .filter(|id| bases.contains(&id.base()))
        .cloned()
        .collect();
    let mut inactive = Vec::new();
    let mut not_installed = 0;
    for base in bases {
        if working_bases.contains(base) {
            continue;
        }
        match inventory.find_by_base(base) {
            Some((id, entry)) if entry.present_on_disk => inactive.push(id.clone()),
            Some(_) | None => not_installed += 1,
        }
    }
    ResolvedScope {
        active,
        inactive,
        not_installed,
    }
}
