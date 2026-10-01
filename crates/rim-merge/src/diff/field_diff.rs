//! The three-way, per-field diff itself: owner versions, field diffs, and their classification.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::Confidence;

use super::structural::BaseNotAnOwner;
use super::values::{entry_kind_for, field_value, value_from_leaves};
use crate::tree::{FieldNode, FieldPath, FieldTree, keyed_map_entries};

/// One owner's contribution to a contested def: its raw XML node, its
/// fully `ParentName`-resolved tree, and (when it has a parent at all)
/// what its ancestor chain alone resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerVersion {
    /// The owning mod.
    pub mod_id: ModId,
    /// The owner's own, unresolved def element.
    pub raw: FieldTree,
    /// The owner's def, `ParentName` chain applied.
    pub resolved: FieldTree,
    /// [`crate::inherit::resolve_inherited_only`] over `raw` — what this
    /// owner's fields would be if `raw` contributed nothing itself.
    /// `None` when `raw.parent_name` is `None` (nothing to inherit).
    /// [`crate::plan::plan_def_override`]'s drop rule needs this to tell
    /// "raw is the only source of this item" apart from "an ancestor
    /// independently supplies it too" (in which case removing raw's own
    /// copy alone would let the ancestor's resurface).
    pub inherited: Option<FieldTree>,
}

/// One field's value: leaf text, a whole `li` item (compared
/// structurally — a list item is a unit, never decomposed further), or
/// absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Leaf text.
    Leaf(String),
    /// A whole `li` item's subtree.
    Item(FieldNode),
    /// The field doesn't exist in this owner's resolved tree.
    Absent,
}

/// Where one field's own leaf sits, structurally
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// An ordinary named field — not a `li` item, not a child of a
    /// [`ContainerKind::KeyedMap`](crate::tree::ContainerKind::KeyedMap) container.
    Leaf,
    /// A `li` item. Kept as its own variant rather than folded into
    /// [`Self::Leaf`]: a list item always compares as [`Value::Item`]
    /// (the whole subtree), never [`Value::Leaf`].
    ListItem,
    /// A child of a [`ContainerKind::KeyedMap`](crate::tree::ContainerKind::KeyedMap) container — this entry's
    /// own tag is the map's key; `container` is the map's own address
    /// (this entry's `path` is `container` plus one more `Child`
    /// segment).
    MapEntry {
        /// The keyed map's own address.
        container: FieldPath,
    },
}

/// One field's three-way comparison across every owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDiff {
    /// The field's address under the def root.
    pub path: FieldPath,
    /// The base owner's value.
    pub base: Value,
    /// Every owner's value, keyed by owner (display order follows the
    /// selected order, not this map's own key order).
    pub candidates: BTreeMap<ModId, Value>,
    /// How the candidates relate to `base`.
    pub class: DiffClass,
    /// Whether this field is a `li` item (as opposed to a named leaf) —
    /// now derived from [`Self::entry`] (`true` iff
    /// [`EntryKind::ListItem`]), kept as its own field since every
    /// existing caller already reads it directly.
    pub is_list_item: bool,
    /// Where this field sits, structurally.
    pub entry: EntryKind,
}

/// How a field's candidates relate to its base value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffClass {
    /// Every candidate equals `base`.
    Unchanged,
    /// Exactly one candidate differs from `base`.
    OneSided {
        /// The one differing owner.
        by: ModId,
    },
    /// More than one candidate differs from `base`, all to the same value.
    Agreeing {
        /// Every differing owner.
        by: BTreeSet<ModId>,
    },
    /// More than one candidate differs from `base`, to different values.
    Conflict {
        /// Every differing owner.
        by: BTreeSet<ModId>,
    },
}

impl DiffClass {
    /// The confidence the editor pre-fills this field's auto result with.
    /// `Unchanged` has no displayed confidence in the editor (the row is
    /// hidden), but a real value is still needed here since callers (a
    /// collapsed-row threshold check) can't special-case "no confidence" —
    /// 100 reads as "nothing to review", consistent with the row being
    /// hidden by default.
    #[must_use]
    pub fn confidence(&self) -> Confidence {
        let percent = match self {
            Self::Unchanged => 100,
            Self::OneSided { .. } => 95,
            Self::Agreeing { .. } => 90,
            Self::Conflict { .. } => 0,
        };
        #[expect(
            clippy::unwrap_used,
            reason = "every arm above is a fixed literal in 0..=100"
        )]
        Confidence::new(percent).unwrap()
    }
}

/// The whole three-way diff for one contested def.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeWayDiff {
    /// The base owner (earliest in the selected order).
    pub base: ModId,
    /// Every field, in path order (the base owner's own document order,
    /// then any field only other owners have, in the order those owners
    /// were given).
    pub fields: Vec<FieldDiff>,
}

/// Three-way diffs every owner's resolved tree against `base`'s. `owners`
/// must be in the selected order (display columns for
/// `OneSided`/`Conflict` follow that order); `base` is expected to be the
/// earliest owner in it.
///
/// Builds one `path -> node` map per owner from
/// [`FieldTree::leaves`] up front, so every `(path, owner)` lookup below
/// is O(log n) instead of re-walking the tree from the root each time —
/// significant once a def has thousands of fields (a large `statBases`
/// or `comps` list).
///
/// # Errors
///
/// [`BaseNotAnOwner`] when `base` isn't the `mod_id` of any entry in
/// `owners`.
pub fn three_way(owners: &[OwnerVersion], base: &ModId) -> Result<ThreeWayDiff, BaseNotAnOwner> {
    if !owners.iter().any(|owner| &owner.mod_id == base) {
        return Err(BaseNotAnOwner(base.clone()));
    }

    // `leaves()` itself yields document order; collect it into an owned
    // `Vec` *once* per owner (preserving that order for `ordered_paths`
    // below) before also indexing it into a `BTreeMap` for the O(log n)
    // lookups every `(path, owner)` pair needs afterward — a `BTreeMap`'s
    // own key iteration order is alphabetical, not document order, so it
    // can't be used for both.
    let owner_leaves: BTreeMap<&ModId, Vec<(FieldPath, &FieldNode)>> = owners
        .iter()
        .map(|owner| (&owner.mod_id, owner.resolved.leaves().collect()))
        .collect();
    let leaf_maps: BTreeMap<&ModId, BTreeMap<FieldPath, &FieldNode>> = owner_leaves
        .iter()
        .map(|(id, leaves)| (*id, leaves.iter().cloned().collect()))
        .collect();

    let mut ordered_paths: Vec<FieldPath> = Vec::new();
    let mut seen: BTreeSet<FieldPath> = BTreeSet::new();
    for owner in owners {
        for (path, _) in &owner_leaves[&owner.mod_id] {
            if seen.insert(path.clone()) {
                ordered_paths.push(path.clone());
            }
        }
    }

    let fields = ordered_paths
        .into_iter()
        .map(|path| {
            let entry = entry_kind_for(&path, owners);
            let list_item = matches!(entry, EntryKind::ListItem);
            let base_value = value_from_leaves(&leaf_maps[base], &path, &entry);

            let candidates: BTreeMap<ModId, Value> = owners
                .iter()
                .map(|owner| {
                    (
                        owner.mod_id.clone(),
                        value_from_leaves(&leaf_maps[&owner.mod_id], &path, &entry),
                    )
                })
                .collect();

            let differing: Vec<(&ModId, &Value)> = owners
                .iter()
                .filter(|owner| &owner.mod_id != base)
                .map(|owner| (&owner.mod_id, &candidates[&owner.mod_id]))
                .filter(|(_, value)| *value != &base_value)
                .collect();

            let class = classify(&differing);

            FieldDiff {
                path,
                base: base_value,
                candidates,
                class,
                is_list_item: list_item,
                entry,
            }
        })
        .collect();

    Ok(ThreeWayDiff {
        base: base.clone(),
        fields,
    })
}

fn classify(differing: &[(&ModId, &Value)]) -> DiffClass {
    match differing {
        [] => DiffClass::Unchanged,
        [(by, _)] => DiffClass::OneSided { by: (*by).clone() },
        rest => {
            let first_value = rest[0].1;
            let by: BTreeSet<ModId> = rest.iter().map(|(id, _)| (*id).clone()).collect();
            if rest.iter().all(|(_, value)| *value == first_value) {
                DiffClass::Agreeing { by }
            } else {
                DiffClass::Conflict { by }
            }
        }
    }
}

/// Classifies one `(base, candidates)` pair against each other — shared
/// by [`collision_fields`](crate::diff::collisions::collision_fields)'s two branches, factored out only so the
/// map-entry loop and the whole-subtree fallback build a [`FieldDiff`]
/// the identical way.
pub(super) fn diff_one_field(
    path: FieldPath,
    entry: EntryKind,
    is_list_item: bool,
    target_raw: &FieldTree,
    per_mod_trees: &[(ModId, FieldTree)],
) -> FieldDiff {
    let base = field_value(target_raw, &path, &entry);
    let candidates: BTreeMap<ModId, Value> = per_mod_trees
        .iter()
        .map(|(id, tree)| (id.clone(), field_value(tree, &path, &entry)))
        .collect();
    let differing: Vec<(&ModId, &Value)> = candidates
        .iter()
        .filter(|(_, value)| *value != &base)
        .collect();
    let class = classify(&differing);
    FieldDiff {
        path,
        base,
        candidates,
        class,
        is_list_item,
        entry,
    }
}

/// Collects every key of the [`ContainerKind::KeyedMap`](crate::tree::ContainerKind::KeyedMap) at `sub_path` in
/// `tree`, in that tree's own document order, appending only keys not
/// already `seen`.
pub(super) fn collect_map_keys(
    tree: &FieldTree,
    sub_path: &FieldPath,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) {
    let Some(entries) = tree.get(sub_path).and_then(keyed_map_entries) else {
        return;
    };
    for (tag, _) in entries {
        if seen.insert(tag.to_string()) {
            order.push(tag.to_string());
        }
    }
}
