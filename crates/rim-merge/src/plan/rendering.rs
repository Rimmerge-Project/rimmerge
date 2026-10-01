//! Rendering resolved values back into nodes, xpath-safe paths, and the chain that anchors them.

use std::collections::BTreeMap;

use rim_resolve::domain::ItemId;
pub use rim_resolve::domain::MergeChoice;

use super::PlanOp;
use super::choices::{ChoiceOutcome, resolve_choice};
use crate::diff::{FieldDiff, ThreeWayDiff, Value};
use crate::tree::{Content, FieldNode, FieldPath, FieldTree, PathSegment};

/// Whether an identity value is safe to embed in a quoted xpath predicate
/// — see [`Caveat::UnsafeXpathValue`] for the two independent hazards
/// this rules out.
fn is_safe_identity_value(value: &str) -> bool {
    !(value.contains('"') && value.contains('\'')) && !value.contains("..") && !value.contains('|')
}

fn item_identity_value(item_id: &ItemId) -> Option<&str> {
    match item_id {
        ItemId::Class(value) | ItemId::Text(value) => Some(value.as_str()),
        ItemId::Key { value, .. } => Some(value.as_str()),
        ItemId::Position(_) => None,
    }
}

/// Whether every segment of `path` can be safely rendered into an xpath
/// this crate's own [`crate::patch_eval`] (and
/// `rim_analyzer::extract::xpath_expr`, which it reuses) can parse back.
pub(super) fn path_is_xpath_safe(path: &FieldPath) -> bool {
    path.segments().iter().all(|segment| match segment {
        PathSegment::Child(_) => true,
        PathSegment::Item(item_id) => {
            item_identity_value(item_id).is_none_or(is_safe_identity_value)
        }
    })
}

pub(super) fn ends_in_position(path: &FieldPath) -> bool {
    matches!(
        path.segments().last(),
        Some(PathSegment::Item(ItemId::Position(_)))
    )
}

/// Builds the [`FieldNode`] a leaf/item value renders as under its own
/// tag (the path's last segment): a plain text leaf for [`Value::Leaf`],
/// the item's own subtree (already tagged `li`) for [`Value::Item`].
/// `attrs` is attached to a [`Value::Leaf`]'s own node — see
/// [`attrs_for_entry`]'s own doc comment for where it comes from and why
/// it's always empty for an ordinary leaf.
pub(super) fn build_node_for(
    path: &FieldPath,
    value: &Value,
    attrs: &BTreeMap<String, String>,
) -> Option<FieldNode> {
    match value {
        Value::Absent => None,
        Value::Item(node) => Some(node.clone()),
        Value::Leaf(text) => {
            let PathSegment::Child(tag) = path.segments().last()? else {
                // A `Leaf` value can only ever be chosen for a `Child`
                // path — `li` paths always carry `Value::Item`.
                return None;
            };
            Some(FieldNode {
                tag: tag.clone(),
                attrs: attrs.clone(),
                content: Content::Text(text.clone()),
            })
        }
    }
}

/// The attribute set a [`MergeChoice::Value`] free-text choice's own
/// emitted node should carry: [`value_from_free_text`] parses free text
/// to a bare [`Value::Leaf`], which has no attribute syntax of its own, so
/// a `<Donkey MayRequire="...">` keyed entry resolved by typing a value
/// would otherwise emit an attribute-less `Replace` — silently dropping
/// `MayRequire` and making the key apply on an install that doesn't have
/// the gated content active. Recovers the attrs from
/// whichever of `field`'s own [`Value::Item`] representations (`base` or
/// any candidate) carries them — [`field_value`]'s own
/// [`EntryKind::MapEntry`] rule already renders an attributed keyed entry
/// as `Value::Item` (attributes preserved) rather than `Value::Leaf`, so
/// this is exactly the same attrs a `MergeChoice::From`/auto-resolved
/// choice on this same field already keeps verbatim via
/// [`build_node_for`]'s own `Value::Item` arm. An ordinary
/// [`EntryKind::Leaf`] field's `base`/candidates are never `Value::Item`
/// at all (attributes silently drop there too — a documented gap for
/// every plain leaf), so this only ever
/// contributes something non-empty for a keyed-map entry
/// ([`EntryKind::MapEntry`]).
pub(super) fn attrs_for_entry(field: &FieldDiff) -> BTreeMap<String, String> {
    std::iter::once(&field.base)
        .chain(field.candidates.values())
        .find_map(|value| match value {
            Value::Item(node) if !node.attrs.is_empty() => Some(node.attrs.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Wraps `inner` under each middle segment of `path` between `prefix` and
/// `path`'s own last segment (already reflected in `inner`'s own tag),
/// innermost first — the chain [`PlanOp::Add`] adds under `prefix`.
/// `None` when any middle segment can't be safely reconstructed (see
/// [`wrap`]).
///
/// Preconditions `path` must satisfy for this to mean anything: `prefix`'s
/// segments are a literal prefix of `path`'s own, with at least one more
/// segment beyond it (`path`'s own last segment, the one already
/// reflected in `inner`'s tag). Every real caller already guarantees
/// this — [`replace_or_add`]'s `prefix` comes from
/// `FieldTree::longest_existing_prefix(path)`, always strictly shorter
/// than `path` in the branch that calls this. A caller that violates it
/// (say, both `path` and `prefix` empty for a whole-def field) gets `None`
/// here rather than an arithmetic-overflow panic from slicing
/// `path.segments().len() - 1`.
pub(super) fn build_chain(
    path: &FieldPath,
    prefix: &FieldPath,
    inner: FieldNode,
) -> Option<FieldNode> {
    let (_last, middle) = path
        .segments()
        .get(prefix.segments().len()..)?
        .split_last()?;
    let mut node = inner;
    for segment in middle.iter().rev() {
        node = wrap(segment, node)?;
    }
    Some(node)
}

/// Builds the contested def's node the way a merge with no unresolved
/// fields would leave it — every field's chosen value (auto-resolved, or
/// picked via `choices`), assembled under `def_type`. Only meaningful once
/// `diff`'s plan is [`rim_resolve::domain::MergeState::Complete`] (every
/// field resolves to something other than [`ChoiceOutcome::NoChoice`]/
/// [`ChoiceOutcome::Invalid`] — see [`resolve_choice`]'s match arms and
/// `rim_session::use_cases::plan_merge::state_from_plan`, which derives
/// `Complete` from exactly that); a field that doesn't resolve here is
/// silently left out rather than surfacing an error this function has no
/// way to report — callers own checking `Complete` first.
///
/// Deliberately reuses the exact machinery [`plan_def_override`](super::plan_def_override) itself
/// builds ops from — [`resolve_choice`] for the per-field value,
/// [`build_node_for`]/[`build_chain`] to turn a `(path, value)` pair into
/// the single-field fragment it would sit in — plus
/// [`crate::inherit::merge_children`] (the same fold `ParentName`
/// resolution uses) to combine every field's own fragment into one tree.
/// This is deliberately *not* a second, xpath-based applier over the raw
/// winner node (render the ops, replay them back per `tests/closure.rs`):
/// that path exists to prove the emitter and the evaluator agree, not to
/// serve a preview on every `get_merge_preview` call, and reusing it here
/// would mean re-rendering and re-parsing a whole merge mod just to show
/// one def's text.
#[must_use]
pub fn build_resolved_node(
    def_type: &str,
    diff: &ThreeWayDiff,
    choices: &BTreeMap<FieldPath, MergeChoice>,
) -> FieldNode {
    let root_path = FieldPath::new(vec![]);
    let mut children: Vec<FieldNode> = Vec::new();

    for field in &diff.fields {
        let ChoiceOutcome::Resolved(value) = resolve_choice(field, choices.get(&field.path)) else {
            continue;
        };
        if value == Value::Absent {
            continue;
        }
        if field.path.segments().is_empty() {
            // A patch collision whose op targets the def node itself
            // (`sub_path: None`, `plan_merge.rs`'s `path_key` default) —
            // this field's own resolved value already *is* the whole def's
            // content, not a fragment to fold under `def_type` via
            // `build_chain` (which requires a strictly-longer `path` than
            // `root_path` to slice a last segment off of). Retag it to
            // `def_type` (defensive: the replayed root's own tag should
            // already match) and use it directly; a whole-def field is
            // always this diff's only field (`collision_fields` never
            // expands an empty `sub_path`, so it produces exactly one), so
            // there is nothing else to fold it with.
            return match value {
                // Already filtered out above; kept exhaustive rather than
                // matching only the two live arms so a future `Value`
                // variant can't silently fall through unhandled.
                Value::Absent => FieldNode {
                    tag: def_type.to_string(),
                    attrs: BTreeMap::new(),
                    content: Content::Empty,
                },
                Value::Item(node) => FieldNode {
                    tag: def_type.to_string(),
                    attrs: node.attrs,
                    content: node.content,
                },
                Value::Leaf(text) => FieldNode {
                    tag: def_type.to_string(),
                    attrs: BTreeMap::new(),
                    content: Content::Text(text),
                },
            };
        }
        let Some(node) = build_node_for(&field.path, &value, &attrs_for_entry(field)) else {
            continue;
        };
        let Some(fragment) = build_chain(&field.path, &root_path, node) else {
            continue;
        };
        children = crate::inherit::merge_children(&children, std::slice::from_ref(&fragment));
    }

    FieldNode {
        tag: def_type.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(children),
    }
}

/// Reconstructs one missing ancestor `li` a chosen value must be added
/// under. Only a `Class`-identified item is reconstructable — its
/// identity is a real, static attribute this function can just re-attach.
/// `Key`/`Text`/`Position` identities carry no data this function can
/// safely turn back into an element (`Key` would need a sibling key
/// child alongside `inner`; `Text` and `Position` are structurally
/// incompatible with a middle segment at all — see the doc comments on
/// [`rim_resolve::domain::ItemId`]). None of these are exercised by any
/// scenario in; refusing them (`None`, which
/// the caller turns into [`Caveat::UnreconstructableChain`]) is safer
/// than emitting a broken or misleading element.
fn wrap(segment: &PathSegment, inner: FieldNode) -> Option<FieldNode> {
    match segment {
        PathSegment::Child(tag) => Some(FieldNode {
            tag: tag.clone(),
            attrs: BTreeMap::new(),
            content: Content::Children(vec![inner]),
        }),
        PathSegment::Item(ItemId::Class(class)) => {
            let mut attrs = BTreeMap::new();
            attrs.insert("Class".to_string(), class.clone());
            Some(FieldNode {
                tag: "li".to_string(),
                attrs,
                content: Content::Children(vec![inner]),
            })
        }
        PathSegment::Item(ItemId::Key { .. } | ItemId::Text(_) | ItemId::Position(_)) => None,
    }
}

/// The ordinary field rules, and the "Replace when raw has C, else Add"
/// half of rule 4's `From`/`Value` branch: `path` exists verbatim in
/// `raw` -> [`PlanOp::Replace`]; otherwise -> [`PlanOp::Add`] under the
/// longest existing prefix, wrapping `node` in whatever intermediate
/// elements the gap needs (`None` when [`build_chain`] can't).
pub(super) fn replace_or_add(path: &FieldPath, raw: &FieldTree, node: FieldNode) -> Option<PlanOp> {
    let prefix = raw.longest_existing_prefix(path);
    if &prefix == path {
        return Some(PlanOp::Replace {
            path: path.clone(),
            node,
        });
    }
    build_chain(path, &prefix, node).map(|chain| PlanOp::Add {
        parent: prefix,
        node: chain,
    })
}
