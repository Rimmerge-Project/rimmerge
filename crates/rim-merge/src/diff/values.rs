//! Reading one field's value, and telling keyed maps apart from plain lists.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;

use super::field_diff::{EntryKind, OwnerVersion, Value};
use crate::tree::{
    ContainerKind, Content, FieldNode, FieldPath, FieldTree, PathSegment, container_kind,
};

/// A node's content, as the [`Value`] shape a [`EntryKind::Leaf`]/
/// [`EntryKind::MapEntry`] entry reads it: leaf text for
/// `Content::Text`/`Content::Empty`, the whole subtree for
/// `Content::Children` (the "a `Record` fallback stays whole-subtree"
/// rule).
fn leaf_or_item(node: &FieldNode) -> Value {
    match &node.content {
        Content::Text(text) => Value::Leaf(text.clone()),
        Content::Empty => Value::Leaf(String::new()),
        Content::Children(_) => Value::Item(node.clone()),
    }
}

/// Reads `tree`'s value at `path`, as the [`Value`] shape `entry` calls
/// for. `pub(crate)` — [`crate::plan`] reuses it to compare a chosen
/// value against what the winner's own resolved tree already produces.
///
/// [`EntryKind::MapEntry`]'s own rule: a keyed child carrying
/// attributes (`<Donkey MayRequire="x">0.2</Donkey>`) compares as
/// [`Value::Item`] (structural, attributes included) rather than
/// [`Value::Leaf`], which would silently drop them on emit — a plain
/// leaf has this same gap ([`leaf_or_item`] never looks at `node.attrs`
/// either), but only a map entry's own attributes are ever load-bearing
/// (`MayRequire`, chiefly), so only this variant preserves them.
pub(crate) fn field_value(tree: &FieldTree, path: &FieldPath, entry: &EntryKind) -> Value {
    let Some(node) = tree.get(path) else {
        return Value::Absent;
    };
    match entry {
        EntryKind::ListItem => Value::Item(node.clone()),
        EntryKind::Leaf => leaf_or_item(node),
        EntryKind::MapEntry { .. } => {
            if node.attrs.is_empty() {
                leaf_or_item(node)
            } else {
                Value::Item(node.clone())
            }
        }
    }
}

/// Reads a value out of a precomputed leaves map (see [`three_way`](crate::diff::field_diff::three_way)) —
/// the same shape [`field_value`] produces, but from an O(log n) map
/// lookup instead of an O(depth) tree walk, since `three_way` does this
/// for every `(path, owner)` pair.
pub(super) fn value_from_leaves(
    leaves: &BTreeMap<FieldPath, &FieldNode>,
    path: &FieldPath,
    entry: &EntryKind,
) -> Value {
    let Some(node) = leaves.get(path).copied() else {
        return Value::Absent;
    };
    match entry {
        EntryKind::ListItem => Value::Item(node.clone()),
        EntryKind::Leaf => leaf_or_item(node),
        EntryKind::MapEntry { .. } => {
            if node.attrs.is_empty() {
                leaf_or_item(node)
            } else {
                Value::Item(node.clone())
            }
        }
    }
}

/// Whether `tree`'s node at `path` is a [`ContainerKind::KeyedMap`] —
/// `pub(crate)` so [`crate::plan::plan_patch_collision`] can decide its
/// own per-mod replay strategy (isolated vs. its private `move_mod_last`)
/// before calling [`collision_fields`](crate::diff::collisions::collision_fields), which repeats this same check
/// internally to decide whether to expand.
///
/// A def's own root (`path` empty — a `sub_path: None` collision) is never
/// itself a `Dictionary<TKey,TValue>` field: RimWorld's own map
/// classification only ever applies to a *named* field under a def, never
/// the def's own top-level, serialized-fields root. Structurally the two
/// are indistinguishable to [`crate::tree::container_kind`] alone — a def
/// whose every top-level field happens to be a distinct leaf tag (no nested
/// record, no `li`, e.g. `<ThingDef><defName/><label/></ThingDef>`)
/// satisfies [`ContainerKind::KeyedMap`]'s own rule exactly — so this
/// function special-cases the empty path outright rather than leaving every
/// caller to remember the distinction itself (and duplicate the def-root
/// diff logic at its own crate boundary).
pub(crate) fn is_keyed_map_at(tree: &FieldTree, path: &FieldPath) -> bool {
    if path.segments().is_empty() {
        return false;
    }
    tree.get(path)
        .is_some_and(|node| container_kind(node) == Some(ContainerKind::KeyedMap))
}

/// Whether `sub_path`'s node is *confirmed* a [`ContainerKind::KeyedMap`]
/// collision worth expanding: a [`KeyedMap`](ContainerKind::KeyedMap) in
/// `final_tree` **and** in every `per_mod_trees` entry that has a node
/// there at all (a mod that doesn't touch this container at all is not
/// disqualifying) — shared by [`collision_fields`](crate::diff::collisions::collision_fields) (the actual expand
/// decision) and [`crate::plan::plan_patch_collision`] (which per-mod
/// replay strategy to build `per_mod_trees` with in the first place —
/// see that function's own doc comment for why deciding from `final_tree`
/// alone isn't enough: the per-mod trees it goes on to build might
/// themselves disagree, and *that* disagreement is exactly what
/// disqualifies an expand).
pub(crate) fn is_confirmed_keyed_map(
    final_tree: &FieldTree,
    per_mod_trees: &[(ModId, FieldTree)],
    sub_path: &FieldPath,
) -> bool {
    is_keyed_map_at(final_tree, sub_path)
        && per_mod_trees
            .iter()
            .all(|(_, tree)| tree.get(sub_path).is_none() || is_keyed_map_at(tree, sub_path))
}

/// The [`EntryKind`] a leaf at `path` gets, given `owners`' own resolved
/// trees: a `li` item is always [`EntryKind::ListItem`]; otherwise, the
/// first owner (in the order given) whose resolved tree has a node at
/// `path`'s own parent decides — [`EntryKind::MapEntry`] when that
/// parent is a *confirmed* [`ContainerKind::KeyedMap`] (via
/// [`is_keyed_map_at`], not a bare `container_kind` check —
/// [`EntryKind::Leaf`] otherwise, including when no owner has the parent
/// at all, `path` itself has no parent at all (an empty `path`), or
/// `path`'s parent *is* the def's own root (a top-level field): a flat
/// def whose own fields are all distinct leaf tags structurally satisfies
/// `KeyedMap`'s own rule exactly (a real shape on real installs:
/// `HeadTypeDef`'s own `texPath`/`shader`/`shaderColorOverride`).
///
/// Only the *first* owner with the parent is ever consulted — a
/// deliberate simplification (a def override's own per-key classification
/// doesn't need to change at all),
/// not a guarantee every owner agrees. A container's own shape can
/// legitimately differ between owners (one owner's record has a nested
/// child under the same tag another owner leaves a plain leaf under,
/// say), so if owner 1's own node at the parent is a plain
/// [`ContainerKind::Record`] but owner 2's is a
/// [`ContainerKind::KeyedMap`], every child at this path compares as
/// [`EntryKind::Leaf`] for *every* owner, including owner 2's — an
/// attributed child of owner 2's own map entry silently drops its
/// attributes the same way an ordinary leaf always has.
pub(super) fn entry_kind_for(path: &FieldPath, owners: &[OwnerVersion]) -> EntryKind {
    let Some((last, parent_segments)) = path.segments().split_last() else {
        return EntryKind::Leaf;
    };
    if matches!(last, PathSegment::Item(_)) {
        return EntryKind::ListItem;
    }
    let container = FieldPath::new(parent_segments.to_vec());
    // Delegates to `is_keyed_map_at` (not a direct `container_kind` check)
    // so this shares its def-root rule: an empty `container` — a
    // top-level field's own parent is the def's own root — is never a
    // keyed map, even when every one of the def's own fields happens to
    // be a distinct leaf tag (any flat def with no nested/`li` fields at
    // all structurally satisfies `ContainerKind::KeyedMap`'s own rule).
    // Without it, every one of a flat def's own fields (a real-install
    // shape: `HeadTypeDef`'s own `texPath`/`shader`/`shaderColorOverride`)
    // would compare as `EntryKind::MapEntry`, silently folding an
    // `Agreeing` field's other contributors into `agreed_by` instead of
    // `values` (`FieldRowKind::MapEntry`'s own display rule).
    let is_map = owners
        .iter()
        .find(|owner| owner.resolved.get(&container).is_some())
        .is_some_and(|owner| is_keyed_map_at(&owner.resolved, &container));
    if is_map {
        EntryKind::MapEntry { container }
    } else {
        EntryKind::Leaf
    }
}
