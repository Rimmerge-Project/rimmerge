//! Owners in load order, scope partitioning, and building each owner's version of the def.

use std::collections::BTreeSet;

use rim_analyzer::domain::{LoadOrder, ModId};
use rim_merge::diff::OwnerVersion;
use rim_merge::inherit::{self};
use rim_resolve::domain::PatchScope;

use super::context::PlanMergeError;
use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources::{self};

/// The owners of `owners`/`mods` that are actually active, in the
/// selected order (earliest first).
pub(super) fn ordered(members: &BTreeSet<ModId>, order: &LoadOrder) -> Vec<ModId> {
    order
        .as_slice()
        .iter()
        .filter(|id| members.contains(id))
        .cloned()
        .collect()
}

/// RimWorld's own package id, duplicated here (`rim_resolve::domain::patch`
/// keeps its own copy private) because the scoped participant rule
/// needs the same "Core is never
/// outside a scope" exception `PatchScope::membership` already applies when
/// deciding which owners' *versions* enter the diff, not just which
/// `FindingKey`s the scope admits.
const CORE_MOD_ID: &str = "ludeon.rimworld";

/// Splits `owners_in_order` (already filtered to active owners, earliest
/// first) into diff participants (scope members, plus Core whenever it's
/// an owner) and everyone else, preserving order in
/// both halves.
pub(super) fn partition_by_scope(
    owners_in_order: &[ModId],
    scope: &PatchScope,
) -> (Vec<ModId>, Vec<ModId>) {
    let mut inside = Vec::with_capacity(owners_in_order.len());
    let mut outside = Vec::new();
    for owner in owners_in_order {
        if scope.contains(owner) || owner.base().as_str() == CORE_MOD_ID {
            inside.push(owner.clone());
        } else {
            outside.push(owner.clone());
        }
    }
    (inside, outside)
}

/// Builds each of `participants`' own [`OwnerVersion`] (raw, resolved,
/// inherited-only) for `def_type`/`def_name`, in the order given —
/// [`PlanMerge::plan_def_override`](crate::use_cases::plan_merge::PlanMerge::plan_def_override)'s own per-participant construction,
/// factored out so
/// [`MergeCoverage`](crate::use_cases::merge_coverage::MergeCoverage)'s own structural-guard
/// measurement can build the identical data without re-implementing this
/// loop: it needs the same raw/resolved trees to run
/// [`rim_merge::diff::structural_change`] over, which a [`MergePreview`](crate::merge_workspace::MergePreview)
/// alone doesn't carry (only its own [`ThreeWayDiff`](rim_merge::diff::ThreeWayDiff), not the owners'
/// raw XML).
pub(crate) fn build_owner_versions<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    def_type: &str,
    def_name: &str,
    participants: &[ModId],
) -> Result<Vec<OwnerVersion>, PlanMergeError> {
    let mut owner_versions = Vec::with_capacity(participants.len());
    for owner in participants {
        let raw = def_sources::read_owner_def_raw(reader, session, def_type, def_name, owner)?;
        let templates = def_sources::template_set(
            reader,
            session,
            order,
            def_type,
            owner,
            raw.parent_name.as_deref(),
        )?;
        let resolved = inherit::resolve(&raw, &templates)?;
        let inherited = inherit::resolve_inherited_only(&raw, &templates)?;
        owner_versions.push(OwnerVersion {
            mod_id: owner.clone(),
            raw,
            resolved,
            inherited,
        });
    }
    Ok(owner_versions)
}
