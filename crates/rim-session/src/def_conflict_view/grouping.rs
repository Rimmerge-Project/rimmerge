//! Grouping rows: container conflicts explained by their list children, duplicate list entries, and
//! the after-merge tree.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_merge::diff::{DiffClass, FieldDiff, ThreeWayDiff, Value};
use rim_merge::effective::{EffectiveDef, Provenance};
use rim_merge::tree::{FieldPath, FieldTree, ItemId, PathSegment};
use rim_resolve::domain::MergeChoice;

use super::rows::{is_list_item, value_at};
use super::{FieldRow, FieldRowKind};

/// Every list-item identity a container-shaped candidate subtree's own
/// children resolve to — wraps `value`'s node (when it's a whole-subtree
/// [`Value::Item`]) as a synthetic one-level [`FieldTree`] so
/// [`FieldTree::leaves`] walks its own children as `li[...]` entries,
/// reusing the exact same [`ItemIdentity`]/duplicate-collapse machinery
/// the real resolved tree's own paths come from (never a private,
/// slightly-different reimplementation). `None` when `value` isn't a
/// container at all (a leaf, or absent).
fn candidate_list_item_ids(value: &Value) -> Option<BTreeSet<ItemId>> {
    let Value::Item(node) = value else {
        return None;
    };
    let synthetic = FieldTree {
        root: node.clone(),
        parent_name: None,
        name: None,
    };
    Some(
        synthetic
            .leaves()
            .filter_map(|(path, _)| match path.segments() {
                [PathSegment::Item(id)] => Some(id.clone()),
                _ => None,
            })
            .collect(),
    )
}

/// A `Conflict` row on a container path (e.g. `comps`) whose whole
/// apparent disagreement is really just "each contributor added its own,
/// distinct-identity list item" isn't a real conflict — the game merges
/// them cleanly and each item already has its own [`FieldRowKind::ListEntry`]
/// row. This is a real artifact of [`FieldDiff`]'s own container-as-one-
/// opaque-`Value::Item` comparison (the engine's own per-mod candidate
/// replay backs a single-field diff here, so the same set of
/// list items can render as different whole-subtree values just from
/// reordering — a genuine `Conflict` by that diff's own rules, even
/// though the real, full replay never contests anything).
///
/// True when `row` is a `Conflict`, `path` doesn't itself name a list
/// item, [`value_at`] reads a whole subtree there (`Value::Item`, never a
/// leaf or absent), and — read from the diff's own candidates, not from a
/// blanket scan of [`EffectiveDef::provenance`]'s survivors (that scan
/// alone can't tell "every contributor's own item cleanly merged" from "a
/// later `Replace` wholesale clobbered an earlier contributor's own item",
/// since a clobbered contribution simply leaves no descendant behind to
/// check at all) — every list-item identity any member's own candidate
/// subtree contains:
/// - is a *stable* identity (`Class`/`Key`/`Text`), never
///   [`ItemId::Position`]: a `Position` fallback is
///   [`rim_merge::tree::identify_all_li`]'s own signal that two siblings
///   *in that one snapshot* share a declared identity it can't otherwise
///   tell apart — since a `Position` numbering is relative to whichever
///   sibling set produced it, it can never be safely compared against a
///   *different* snapshot's own numbering (another candidate's, or the
///   real resolved tree's), and two mods sharing one declared identity is
///   its own `Conflict` case, not a clean merge;
/// - actually survives in `effective.resolved` under `path` (a
///   `Replace` loading after another contributor's own `Add`/`Replace`
///   can wholesale discard it, so "present in a candidate" alone proves
///   nothing about the real, final tree); and
/// - is credited there via [`Provenance::Patch`] to *some* member (not
///   necessarily the same one the candidate implied) — the container's
///   own value is fully accounted for by real, attributable contributions,
///   not merely by items neither toucher actually owns.
fn is_container_conflict_explained_by_list_children(
    row: &FieldRow,
    effective: &EffectiveDef,
) -> bool {
    if row.kind != FieldRowKind::Conflict || is_list_item(&row.path) {
        return false;
    }
    if !matches!(value_at(&effective.resolved, &row.path), Value::Item(_)) {
        return false;
    }

    let mut candidate_items: BTreeSet<ItemId> = BTreeSet::new();
    for (_, value) in &row.values {
        let Some(items) = candidate_list_item_ids(value) else {
            return false;
        };
        if items.iter().any(|id| matches!(id, ItemId::Position(_))) {
            return false;
        }
        candidate_items.extend(items);
    }
    if candidate_items.is_empty() {
        return false;
    }

    let members: BTreeSet<&ModId> = row.values.iter().map(|(id, _)| id).collect();
    let mut any_patch_credited = false;
    let every_item_survives = candidate_items.iter().all(|item_id| {
        let mut child_segments = row.path.segments().to_vec();
        child_segments.push(PathSegment::Item(item_id.clone()));
        let child_path = FieldPath::new(child_segments);
        match effective.provenance.get(&child_path) {
            Some(Provenance::Patch { mod_id, .. }) if members.contains(mod_id) => {
                any_patch_credited = true;
                true
            }
            Some(_) => true,
            None => false,
        }
    });

    every_item_survives && any_patch_credited
}

/// Drops every row [`is_container_conflict_explained_by_list_children`]
/// flags — called after the caller has already captured every row's own
/// path into its own `covered` set (a dropped row is still *explained*,
/// just not worth displaying on its own — see the caller's own comment).
pub(super) fn drop_container_conflicts_explained_by_list_children(
    rows: &mut Vec<FieldRow>,
    effective: &EffectiveDef,
) {
    rows.retain(|row| !is_container_conflict_explained_by_list_children(row, effective));
}

/// Which mod's value a complete merge would use at `field`, absent a
/// stored `choice` — mirrors `rim_merge::plan`'s own private
/// `resolve_choice`'s mod-attribution half (that function's value half is
/// instead read straight off `after_tree`, built by the public
/// [`rim_merge::plan::build_resolved_node`] — see [`after_merge_for`]).
/// `None` for a genuine [`DiffClass::Conflict`] with no choice (nothing
/// to attribute) and for [`MergeChoice::Drop`] (nothing left to
/// attribute either).
fn after_merge_mod(
    diff: &ThreeWayDiff,
    field: &FieldDiff,
    choice: Option<&MergeChoice>,
    winner: &ModId,
) -> Option<ModId> {
    if let Some(choice) = choice {
        return match choice {
            MergeChoice::From { mod_id } => Some(mod_id.clone()),
            // A free-text value carries no owner of its own; attribute it
            // to the winner, whose raw node the merge's ops apply against.
            MergeChoice::Value { .. } => Some(winner.clone()),
            MergeChoice::Drop => None,
        };
    }
    match &field.class {
        DiffClass::Unchanged => Some(diff.base.clone()),
        DiffClass::OneSided { by } => Some(by.clone()),
        DiffClass::Agreeing { by } => by.iter().next().cloned(),
        DiffClass::Conflict { .. } => None,
    }
}

/// Builds the node a complete merge would leave `def_type` at — thin
/// wrapper around the public [`rim_merge::plan::build_resolved_node`] so
/// [`FieldTree::get`] can be used to read a value back out by path. Only
/// meaningful when the cached preview is [`MergeState::Complete`] — see
/// that function's own doc comment.
pub(super) fn after_merge_tree(
    def_type: &str,
    diff: &ThreeWayDiff,
    choices: &BTreeMap<FieldPath, MergeChoice>,
) -> FieldTree {
    FieldTree {
        root: rim_merge::plan::build_resolved_node(def_type, diff, choices),
        parent_name: None,
        name: None,
    }
}

pub(super) fn after_merge_for(
    diff: &ThreeWayDiff,
    field: &FieldDiff,
    choice: Option<&MergeChoice>,
    winner: &ModId,
    after_tree: &FieldTree,
) -> Option<(ModId, Value)> {
    let mod_id = after_merge_mod(diff, field, choice, winner)?;
    let value = value_at(after_tree, &field.path);
    if value == Value::Absent {
        return None;
    }
    Some((mod_id, value))
}

/// Folds duplicate positional list-item entries — several contributors
/// independently adding the *exact same* `li` item under a colliding
/// identity (see [`rim_merge::effective::duplicate_of_earlier_sibling`]) —
/// into the earlier sibling's own row instead of a second,
/// indistinguishable-looking one: the "list case" dedup. Returns the set of
/// paths that must not get their own row (folded into an earlier one) and,
/// keyed by each surviving row's own path, the extra mods to list as
/// [`FieldRow::agreed_by`], in the order [`EffectiveDef::provenance`]
/// itself iterates (already load order, since a later op always gets a
/// higher `op_index`/later `BTreeMap` path). `eligible` mirrors whichever
/// provenance-walking row-builder is calling this — [`context_rows_from_provenance`]'s
/// own `mods`-and-[`Provenance::Patch`]-only membership rule, or
/// [`fields_from_provenance_only`]'s every-variant one — so the two stay in
/// lockstep with their own row membership; `covered` is honored the same
/// way both callers' own row loops already do: a duplicate whose *earlier*
/// sibling is itself `covered` (already rendered as its own row elsewhere,
/// e.g. the collision's own contested field) folds into nothing here and
/// simply gets its own default row instead, rather than silently losing
/// the "also added by" fact.
///
/// **The earlier sibling must itself pass `eligible` too** — not just
/// `covered`-free. [`rim_merge::effective::duplicate_of_earlier_sibling`]
/// finds the earliest byte-identical sibling *by content alone*, with no
/// idea which mod this particular caller actually cares about: for
/// [`context_rows_from_provenance`], that earlier sibling can be the def's
/// own [`Provenance::Owner`]/[`Provenance::Inherited`] content (a real,
/// non-empty base list an in-scope patcher's own add happens to duplicate)
/// or a [`Provenance::Patch`] by a mod outside this collision's own `mods`
/// — neither of which `eligible` ever lets produce a row of its own here.
/// Folding into either would silently drop the later, in-scope
/// contribution: `duplicate_paths` excludes its own path, but nothing
/// downstream ever builds a row for the `covered`-free-yet-ineligible
/// `original_path` its `agreed_by` entry was attached to, so the
/// contribution vanishes with no trace at all. This function's own answer
/// (not the only possible one, but the one that keeps every
/// contribution visible and needs no new row shape): decline the fold
/// whenever the earlier sibling isn't itself `eligible`, so the later path
/// falls through to its own ordinary row instead — the in-scope mod's own
/// contribution renders exactly as it would if it didn't happen to
/// duplicate content this caller doesn't otherwise show, rather than
/// crediting a mod/value this view never surfaces on its own. A future
/// caller that wants to say "already declared by the owner" explicitly
/// would need a new, additive way to say so — not this function silently
/// reassigning `values`' own membership rule.
///
/// For [`fields_from_provenance_only`] specifically, this guard is
/// symmetry/future-proofing, not a live fix: its own `eligible` rejects
/// only [`Provenance::UnattributedTemplate`], and
/// [`rim_merge::effective::duplicate_of_earlier_sibling`] already skips
/// that variant itself when searching for an earlier match (it has no mod
/// to report), so `original_path`'s own provenance there is always one of
/// `Owner`/`Patch`/`Inherited` — already eligible under that closure by
/// construction. The check can never actually fire for that caller today;
/// it costs nothing to keep both call sites protected by the identical
/// rule rather than one relying on an invariant that happens to hold
/// elsewhere.
pub(super) fn fold_duplicate_list_entries(
    effective: &EffectiveDef,
    covered: &BTreeSet<FieldPath>,
    eligible: impl Fn(&Provenance) -> Option<ModId>,
) -> (BTreeSet<FieldPath>, BTreeMap<FieldPath, Vec<ModId>>) {
    let mut duplicate_paths: BTreeSet<FieldPath> = BTreeSet::new();
    let mut agreed_by: BTreeMap<FieldPath, Vec<ModId>> = BTreeMap::new();
    for (path, provenance) in &effective.provenance {
        if covered.contains(path) {
            continue;
        }
        let Some(mod_id) = eligible(provenance) else {
            continue;
        };
        let Some((original_path, _)) =
            rim_merge::effective::duplicate_of_earlier_sibling(effective, path)
        else {
            continue;
        };
        if covered.contains(original_path) {
            continue;
        }
        let Some(original_provenance) = effective.provenance.get(original_path) else {
            continue;
        };
        if eligible(original_provenance).is_none() {
            continue;
        }
        duplicate_paths.insert(path.clone());
        agreed_by
            .entry(original_path.clone())
            .or_default()
            .push(mod_id);
    }
    (duplicate_paths, agreed_by)
}
