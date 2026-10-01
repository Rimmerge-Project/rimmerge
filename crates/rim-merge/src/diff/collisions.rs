//! The fields two colliding owners both touch.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;

use super::field_diff::{EntryKind, FieldDiff, collect_map_keys, diff_one_field};
use super::values::is_confirmed_keyed_map;
use crate::tree::{FieldPath, FieldTree, PathSegment};

/// Builds one [`FieldDiff`] per key of a [`ContainerKind::KeyedMap`](crate::tree::ContainerKind::KeyedMap)
/// collision at `sub_path`, or the single whole-subtree field
/// [`crate::plan::plan_patch_collision`] compares when `sub_path`'s node
/// isn't (consistently) a keyed map. The one candidate-builder shared by
/// this crate's own `plan_patch_collision` and `rim-session`'s merge
/// preview.
///
/// `target_raw` is the def owner's own node before any patch ran (used
/// for `base`); `final_tree` is the real, full-order replay, consulted
/// only to decide *whether* to expand and, when expanding, to seed the
/// key union in its own real document order (ties broken by each
/// `per_mod_trees` entry's own order for a key `final_tree` doesn't have
/// at all — an add a later wholesale replace clobbered, see
/// [`crate::plan::Caveat::ClobberedMapEntry`]). Each `per_mod_trees`
/// entry is that one mod's own contribution to the union and to
/// per-entry attribution; **the caller decides what replay each entry
/// embodies** — `plan_patch_collision` passes its own `move_mod_last`-
/// reordered full replays for an ordinary (non-map) field, preserving
/// that path's long-tested semantics exactly, and *isolated*, single-mod
/// replays once [`is_keyed_map_at`](crate::diff::values::is_keyed_map_at) shows `sub_path` is a keyed map in
/// the real, full-order tree — isolated replay is what a disjoint-key
/// union needs: under `move_mod_last`, a key only mod A ever added shows
/// up in *every* reordering's own candidate (nothing else in the replay
/// ever removes it), misclassifying a clean disjoint add as
/// `DiffClass::Agreeing { by: everyone }` instead of
/// `DiffClass::OneSided { by: A }`.
///
/// Expansion requires `sub_path`'s node to classify
/// [`ContainerKind::KeyedMap`](crate::tree::ContainerKind::KeyedMap) in `final_tree` *and* in every
/// `per_mod_trees` entry that has a node there at all (a mod that
/// doesn't touch this container at all is not disqualifying) — otherwise
/// this returns the single whole-subtree field, built from whatever
/// `per_mod_trees` the caller passed (a record misread as a map is
/// harmless: a whole-subtree compare is exactly what a non-map container
/// gets anyway).
#[must_use]
pub fn collision_fields(
    target_raw: &FieldTree,
    final_tree: &FieldTree,
    per_mod_trees: &[(ModId, FieldTree)],
    sub_path: &FieldPath,
) -> Vec<FieldDiff> {
    if !is_confirmed_keyed_map(final_tree, per_mod_trees, sub_path) {
        let is_list_item = matches!(sub_path.segments().last(), Some(PathSegment::Item(_)));
        let entry = if is_list_item {
            EntryKind::ListItem
        } else {
            EntryKind::Leaf
        };
        return vec![diff_one_field(
            sub_path.clone(),
            entry,
            is_list_item,
            target_raw,
            per_mod_trees,
        )];
    }

    let mut order: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    collect_map_keys(final_tree, sub_path, &mut seen, &mut order);
    for (_, tree) in per_mod_trees {
        collect_map_keys(tree, sub_path, &mut seen, &mut order);
    }

    order
        .into_iter()
        .map(|key| {
            let mut segments = sub_path.segments().to_vec();
            segments.push(PathSegment::Child(key));
            let path = FieldPath::new(segments);
            let entry = EntryKind::MapEntry {
                container: sub_path.clone(),
            };
            diff_one_field(path, entry, false, target_raw, per_mod_trees)
        })
        .collect()
}
