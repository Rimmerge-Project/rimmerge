//! Per-field planning helpers: contributor filtering, dependencies, drops, and owner lookup.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
pub use rim_resolve::domain::{DefKey, MergeChoice};

use super::{CORE_MOD_ID, Caveat, PlanOp, PlannedOp};
use crate::diff::{DiffClass, FieldDiff, OwnerVersion, ThreeWayDiff};
use crate::tree::{Content, FieldNode, FieldPath, FieldTree, PathSegment};

/// Adds `id`'s base form (never `_steam`-suffixed — matches
/// `PlannedOp::depends_on`'s documented contract) unless it's Core.
fn push_non_core(set: &mut BTreeSet<ModId>, id: &ModId) {
    let base = id.base();
    if base.as_str() != CORE_MOD_ID {
        set.insert(base);
    }
}

/// Every op's `depends_on` is the winner plus
/// whichever mod(s) actually contributed the chosen value — an explicit
/// `From(m)` names `m`; an automatic `OneSided`/`Agreeing` result names
/// its contributor(s); `Value` and `Drop` name only the winner. Core is
/// never included (always active).
pub(super) fn depends_on_for(
    field: &FieldDiff,
    choice: Option<&MergeChoice>,
    winner: &ModId,
) -> BTreeSet<ModId> {
    let mut set = BTreeSet::new();
    push_non_core(&mut set, winner);
    match choice {
        Some(MergeChoice::From { mod_id }) => push_non_core(&mut set, mod_id),
        Some(MergeChoice::Value { .. } | MergeChoice::Drop) => {}
        None => match &field.class {
            DiffClass::OneSided { by } => push_non_core(&mut set, by),
            DiffClass::Agreeing { by } => {
                for id in by {
                    push_non_core(&mut set, id);
                }
            }
            DiffClass::Unchanged | DiffClass::Conflict { .. } => {}
        },
    }
    set
}

pub(super) fn def_key_of(tree: &FieldTree) -> DefKey {
    let def_name = tree
        .get(&FieldPath::new(vec![PathSegment::Child(
            "defName".to_string(),
        )]))
        .and_then(|node| match &node.content {
            Content::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    DefKey {
        def_type: tree.root.tag.clone(),
        def_name,
    }
}

/// What [`plan_drop`] decided.
pub(super) enum DropOutcome {
    /// The drop is representable — here's the op.
    Op(PlannedOp),
    /// RimWorld's patch language has no way to carry out this drop —
    /// `unresolved` gets the path, `caveats` gets this.
    Blocked(Caveat),
}

/// Turns a drop (an explicit [`MergeChoice::Drop`], or an automatic
/// result that evaluates to [`Value::Absent`] — an owner removing a field
/// relative to base looks the same either way) into an op:
///
/// 1. The field exists in the raw winner node *and* no ancestor
///    independently supplies an item under the same identity (checked
///    against [`OwnerVersion::inherited`]) -> [`PlanOp::Remove`]: it's
///    raw's own, sole contribution, so removing it leaves nothing behind.
/// 2. Otherwise — purely inherited, or defined in *both* raw and the
///    ancestor chain under the same identity (in which case a plain
///    `Remove` of raw's own copy would just let the ancestor's resurface)
///    — the only way to drop it is to reconstruct the top-level container
///    with the item removed and `Inherit="False"` set. A one-segment
///    (top-level leaf) path has no container to reconstruct minus
///    itself, so that case is [`DropOutcome::Blocked`]
///    ([`Caveat::UnsettableLeaf`]) instead. Otherwise: the raw winner
///    already defines that container -> [`PlanOp::ReplaceInheritFalse`];
///    it doesn't -> [`PlanOp::Add`] (the Bionics worked example's
///    exact shape: BIONICS's raw node lacks `comps`, so dropping the
///    inherited comp emits an `Add` of `<comps Inherit="False">`).
pub(super) fn plan_drop(
    path: &FieldPath,
    winner: &OwnerVersion,
    depends_on: BTreeSet<ModId>,
) -> DropOutcome {
    let has_raw = winner.raw.get(path).is_some();
    let also_inherited = winner
        .inherited
        .as_ref()
        .is_some_and(|inherited| inherited.get(path).is_some());

    if has_raw && !also_inherited {
        return DropOutcome::Op(PlannedOp {
            op: PlanOp::Remove { path: path.clone() },
            depends_on,
        });
    }

    // `split_first` rather than a `len() == 1` guard plus separate
    // `path.segments()[0]`/`[1..]` indexing: a length check kept apart
    // from the indexing it guards is a latent panic, so the empty-path
    // case (impossible today — every `FieldPath` reaching here comes from
    // `FieldTree::leaves()`, which never yields one, per `tree.rs`'s own
    // `collect_leaves`) is handled by the match falling through to
    // `Blocked` instead of relying on that invariant holding forever.
    let Some((container_segment, rest)) = path.segments().split_first() else {
        return DropOutcome::Blocked(Caveat::UnsettableLeaf { path: path.clone() });
    };
    if rest.is_empty() {
        return DropOutcome::Blocked(Caveat::UnsettableLeaf { path: path.clone() });
    }

    let container_path = FieldPath::new(vec![container_segment.clone()]);
    let mut container_node = winner
        .resolved
        .get(&container_path)
        .cloned()
        .unwrap_or_else(|| FieldNode {
            tag: container_tag(&container_path),
            attrs: BTreeMap::new(),
            content: Content::Children(Vec::new()),
        });
    crate::tree::remove_at(&mut container_node, rest);
    container_node
        .attrs
        .insert("Inherit".to_string(), "False".to_string());

    if winner.raw.get(&container_path).is_some() {
        DropOutcome::Op(PlannedOp {
            op: PlanOp::ReplaceInheritFalse {
                path: container_path,
                node: container_node,
            },
            depends_on,
        })
    } else {
        DropOutcome::Op(PlannedOp {
            op: PlanOp::Add {
                parent: FieldPath::new(vec![]),
                node: container_node,
            },
            depends_on,
        })
    }
}

fn container_tag(path: &FieldPath) -> String {
    match path.segments().first() {
        Some(PathSegment::Child(tag)) => tag.clone(),
        _ => String::new(),
    }
}

pub(super) fn owners_from_diff(diff: &ThreeWayDiff, winner: &ModId) -> Vec<ModId> {
    let mut owners: BTreeSet<ModId> = diff
        .fields
        .iter()
        .flat_map(|field| field.candidates.keys().cloned())
        .collect();
    owners.insert(diff.base.clone());
    owners.insert(winner.clone());
    owners.into_iter().collect()
}
