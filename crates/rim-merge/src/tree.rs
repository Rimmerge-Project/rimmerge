//! [`FieldTree`]: one def's (or template's) XML modeled as an addressable
//! tree, and the list-item identity heuristic ([`ItemIdentity::identify`])
//! that makes a [`FieldPath`] stable across owners.
//!
//! [`FieldPath`]/[`PathSegment`]/[`ItemId`] themselves live in
//! `rim_resolve::domain` (re-exported here) — see that module's own doc
//! comment for why: a [`FieldPath`] is persisted inside a decision, so the
//! type has to live wherever decisions do. An inherent
//! `impl ItemId { fn of(...) }` is not possible here: `ItemId` is a
//! foreign type from `rim-resolve`'s point of view —
//! Rust's orphan rule forbids an inherent impl (or even a new trait) on a
//! type this crate doesn't own unless the trait is also local, so the
//! heuristic is [`ItemIdentity`], a local extension trait implemented for
//! [`FieldNode`], called as `node.identify(position)` rather than
//! `ItemId::of(node, position)`.

use std::collections::{BTreeMap, BTreeSet};

pub use rim_resolve::domain::{FieldPath, ItemId, PathSegment};

/// The well-known `li` key children tried, in this order, before falling
/// back to text or position identity — see [`ItemIdentity::identify`].
const KEY_CHILDREN: [&str; 12] = [
    "defName",
    "def",
    "key",
    "stat",
    "compClass",
    "thingDef",
    "hediffDef",
    "hediff",
    "plant",
    "animal",
    "tag",
    "label",
];

/// One XML element of a def: a tag, its attributes verbatim (`Class`,
/// `MayRequire`, `Inherit`, ...), and either text or element children.
/// RimWorld def XML never has mixed content — [`crate::xml::parse`]
/// rejects it rather than silently dropping one side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldNode {
    /// The element's tag name.
    pub tag: String,
    /// Attributes, verbatim, keyed by name — ordered for deterministic
    /// rendering.
    pub attrs: BTreeMap<String, String>,
    /// The element's content: nothing, leaf text, or element children.
    pub content: Content,
}

/// One [`FieldNode`]'s content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// No children and no (non-whitespace) text — `<foo/>` or `<foo></foo>`.
    Empty,
    /// Leaf text content, trimmed.
    Text(String),
    /// Element children, in document order.
    Children(Vec<FieldNode>),
}

/// One def's (or template's) tree. The def/template element itself is
/// [`Self::root`]. [`crate::xml::parse`] strips `Name`/`ParentName` off
/// the root and keeps them here instead (in [`Self::name`]/
/// [`Self::parent_name`]), so a diff or a patch replay never mistakes
/// them for ordinary fields; `Abstract`/`Inherit` are stripped from the
/// root too but simply discarded (never kept anywhere) — neither is
/// meaningful once a tree exists as data: `Abstract` only ever gated
/// whether the analyzer indexed this element as a def or a template, and
/// a root element's own `Inherit` is meaningless (only a *field*'s
/// `Inherit="False"`, handled by [`crate::inherit::resolve`], matters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTree {
    /// The def/template element, with `Name`/`ParentName`/`Abstract`/
    /// `Inherit` already stripped from its own attributes (the first two
    /// moved to [`Self::name`]/[`Self::parent_name`], the last two
    /// discarded — see this struct's doc comment).
    pub root: FieldNode,
    /// The `ParentName` attribute, if this tree inherits from a template.
    pub parent_name: Option<String>,
    /// The `Name` attribute, if this tree is itself a template.
    pub name: Option<String>,
}

/// How a container's own children classify for merge purposes: a
/// contested `<wildAnimals>`-shaped container (RimWorld's
/// `Dictionary<TKey, TValue>`, children keyed by tag name, no `li`) needs
/// a per-key diff/merge instead of the whole-subtree comparison every
/// other [`Content::Children`] node gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// Any `li` child — an ordered list, addressed by [`ItemId`] as
    /// today; untouched by this module's keyed-map handling.
    List,
    /// Non-empty, no `li` child, every child tag distinct, every child
    /// leaf content (`Content::Text`/`Content::Empty`) — RimWorld's
    /// `Dictionary<TKey, TValue>` shape, [`keyed_map_entries`] returns
    /// `Some` for exactly this kind.
    KeyedMap,
    /// Everything else: an empty container, nested subtrees, or a
    /// duplicate child tag (never guessed at as a map — a duplicate tag
    /// under a real RimWorld `Dictionary` field is a loader error, not
    /// last-wins).
    Record,
}

/// Classifies `node`'s own children per [`ContainerKind`] — `None` when
/// `node` is a leaf (`Content::Text`/`Content::Empty`, no element
/// children at all). A leaf-only record (e.g. `addedPartProps`)
/// classifies [`ContainerKind::KeyedMap`] too, deliberately: per-key
/// diff, per-key `Add`/`Replace`, and child-row rendering are all
/// equally right for it, so this function keeps no table of "known map
/// tags" — `statBases`/`costList`/`skillGains`/`statOffsets`/
/// `equippedStatOffsets` fall out of the same rule `wildAnimals` does.
#[must_use]
pub fn container_kind(node: &FieldNode) -> Option<ContainerKind> {
    let Content::Children(children) = &node.content else {
        return None;
    };
    if children.is_empty() {
        return Some(ContainerKind::Record);
    }
    if children.iter().any(|child| child.tag == "li") {
        return Some(ContainerKind::List);
    }
    let mut tags: BTreeSet<&str> = BTreeSet::new();
    let every_child_is_a_leaf_with_a_distinct_tag = children.iter().all(|child| {
        matches!(child.content, Content::Text(_) | Content::Empty) && tags.insert(&child.tag)
    });
    Some(if every_child_is_a_leaf_with_a_distinct_tag {
        ContainerKind::KeyedMap
    } else {
        ContainerKind::Record
    })
}

/// Every child of a [`ContainerKind::KeyedMap`] node, as `(tag, node)`
/// pairs in document order — `Some` only when [`container_kind`] would
/// return `Some(ContainerKind::KeyedMap)` for `node`.
#[must_use]
pub fn keyed_map_entries(node: &FieldNode) -> Option<Vec<(&str, &FieldNode)>> {
    if container_kind(node) != Some(ContainerKind::KeyedMap) {
        return None;
    }
    let Content::Children(children) = &node.content else {
        return None;
    };
    Some(
        children
            .iter()
            .map(|child| (child.tag.as_str(), child))
            .collect(),
    )
}

/// How a `li` item is told apart from its siblings, tried in this order
/// (first hit wins): the `Class` attribute, a well-known key child (see
/// [`KEY_CHILDREN`]), the item's own text content, or — last resort — its
/// 0-based position among the `li` siblings of the same container.
///
/// The orphan-rule workaround for `ItemId::of` described in this module's
/// doc comment: implemented on [`FieldNode`], called as
/// `node.identify(position)`.
pub trait ItemIdentity {
    /// Computes this node's [`ItemId`], given its 0-based position among
    /// the `li`-tagged siblings of its container.
    fn identify(&self, position: u32) -> ItemId;
}

impl ItemIdentity for FieldNode {
    fn identify(&self, position: u32) -> ItemId {
        if let Some(class) = self.attrs.get("Class") {
            return ItemId::Class(class.clone());
        }
        if let Content::Children(children) = &self.content {
            for key in KEY_CHILDREN {
                let found = children
                    .iter()
                    .find(|child| child.tag == key)
                    .and_then(|child| match &child.content {
                        Content::Text(value) => Some(value.clone()),
                        _ => None,
                    });
                if let Some(value) = found {
                    return ItemId::Key {
                        child: key.to_string(),
                        value,
                    };
                }
            }
        }
        if let Content::Text(text) = &self.content {
            return ItemId::Text(text.clone());
        }
        ItemId::Position(position)
    }
}

/// Every `li` child of one container, as `(index_in_children, identity)`
/// pairs in document order — [`ItemIdentity::identify`]'s heuristic, but
/// any identity that two or more `li` siblings *in this same container*
/// compute falls back to [`ItemId::Position`] for **all** of them:
/// neither RimWorld's own patch xpaths nor this crate's path addressing
/// can otherwise tell such items apart, and aliasing two different items
/// under one identity would silently corrupt whichever mutation reached
/// for "the" item by that name. `pub(crate)` and shared by every place
/// that walks a container's `li` children by identity (this module's own
/// `find_child`/`get_mut`/`remove_at`/`collect_leaves`, and
/// `patch_eval::select`) so they all agree on the same identities for the
/// same tree.
pub(crate) fn identify_all_li(children: &[FieldNode]) -> Vec<(usize, ItemId)> {
    let li: Vec<(usize, ItemId)> = children
        .iter()
        .enumerate()
        .filter(|(_, child)| child.tag == "li")
        .enumerate()
        .map(|(position, (index, child))| (index, child.identify(position as u32)))
        .collect();

    // Duplicate detection by sorting a list of positions-into-`li` by the
    // `ItemId` each refers to, then scanning for adjacent equal runs —
    // every comparison borrows, never clones, an `ItemId`.
    let mut order: Vec<usize> = (0..li.len()).collect();
    order.sort_by(|&a, &b| li[a].1.cmp(&li[b].1));
    let mut is_duplicate = vec![false; li.len()];
    for window in order.windows(2) {
        if li[window[0]].1 == li[window[1]].1 {
            is_duplicate[window[0]] = true;
            is_duplicate[window[1]] = true;
        }
    }

    li.into_iter()
        .enumerate()
        .map(|(position, (index, id))| {
            if is_duplicate[position] {
                (index, ItemId::Position(position as u32))
            } else {
                (index, id)
            }
        })
        .collect()
}

fn find_li_index(children: &[FieldNode], item_id: &ItemId) -> Option<usize> {
    identify_all_li(children)
        .into_iter()
        .find(|(_, id)| id == item_id)
        .map(|(index, _)| index)
}

/// Finds `node`'s direct child matching `segment`, if any. `li` items are
/// matched via [`identify_all_li`] — the same heuristic (and the same
/// duplicate-identity fallback) that produced the segment in the first
/// place, so a self-consistent [`ItemId`] always matches its own node.
fn find_child<'a>(node: &'a FieldNode, segment: &PathSegment) -> Option<&'a FieldNode> {
    let Content::Children(children) = &node.content else {
        return None;
    };
    match segment {
        PathSegment::Child(tag) => children.iter().find(|child| &child.tag == tag),
        PathSegment::Item(item_id) => {
            find_li_index(children, item_id).map(|index| &children[index])
        }
    }
}

/// [`crate::patch_eval`] and [`crate::plan`] both need to mutate a tree
/// by [`FieldPath`] (patch replay; reconstructing a container for an
/// `Inherit="False"` drop) — `pub(crate)` since path-addressed mutation
/// isn't part of this module's public, read-only contract.
pub(crate) fn get_mut<'a>(
    node: &'a mut FieldNode,
    segments: &[PathSegment],
) -> Option<&'a mut FieldNode> {
    let Some((first, rest)) = segments.split_first() else {
        return Some(node);
    };
    let Content::Children(children) = &mut node.content else {
        return None;
    };
    let child_index = match first {
        PathSegment::Child(tag) => children.iter().position(|child| &child.tag == tag)?,
        PathSegment::Item(item_id) => find_li_index(children, item_id)?,
    };
    get_mut(&mut children[child_index], rest)
}

/// Removes the descendant of `node` addressed by `segments`, if present
/// — exactly one node (by index, per [`identify_all_li`]), never every
/// node that happens to share a duplicate-collision identity. Returns
/// whether anything was removed. Used by [`crate::plan`] (dropping an
/// inherited item under a reconstructed `Inherit="False"` container) and
/// [`crate::patch_eval`] (`PatchOperationRemove`).
pub(crate) fn remove_at(node: &mut FieldNode, segments: &[PathSegment]) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return false;
    };
    let Content::Children(children) = &mut node.content else {
        return false;
    };
    let child_index = match first {
        PathSegment::Child(tag) => children.iter().position(|child| &child.tag == tag),
        PathSegment::Item(item_id) => find_li_index(children, item_id),
    };
    let Some(child_index) = child_index else {
        return false;
    };
    if rest.is_empty() {
        children.remove(child_index);
        return true;
    }
    remove_at(&mut children[child_index], rest)
}

impl FieldTree {
    /// Looks up the node at `path`, walking from [`Self::root`]. An empty
    /// path returns the root itself.
    #[must_use]
    pub fn get(&self, path: &FieldPath) -> Option<&FieldNode> {
        let mut node = &self.root;
        for segment in path.segments() {
            node = find_child(node, segment)?;
        }
        Some(node)
    }

    /// Every leaf field and every `li` item, as `(path, node)` pairs, in
    /// document order. A named subtree is addressed by its own leaves (so
    /// two owners changing different leaves of the same subtree merge
    /// cleanly); a `li` item is yielded as one unit, never decomposed
    /// further — RimWorld has no per-field merge inside a list item, so
    /// the item's whole subtree is what a choice targets.
    pub fn leaves(&self) -> impl Iterator<Item = (FieldPath, &FieldNode)> {
        let mut out = Vec::new();
        collect_leaves(&self.root, &mut Vec::new(), &mut out);
        out.into_iter()
    }

    /// The longest prefix of `path` that exists in this tree — `path`
    /// itself when it fully exists, the empty path when not even its
    /// first segment does. Used by [`crate::plan`] to decide whether a
    /// chosen value needs a `Replace` (prefix == path) or an `Add` of the
    /// missing tail (prefix shorter).
    #[must_use]
    pub fn longest_existing_prefix(&self, path: &FieldPath) -> FieldPath {
        let mut found = Vec::new();
        let mut node = &self.root;
        for segment in path.segments() {
            match find_child(node, segment) {
                Some(child) => {
                    found.push(segment.clone());
                    node = child;
                }
                None => break,
            }
        }
        FieldPath::new(found)
    }
}

fn collect_leaves<'a>(
    node: &'a FieldNode,
    prefix: &mut Vec<PathSegment>,
    out: &mut Vec<(FieldPath, &'a FieldNode)>,
) {
    let Content::Children(children) = &node.content else {
        return;
    };
    let li_identities: BTreeMap<usize, ItemId> = identify_all_li(children).into_iter().collect();
    for (index, child) in children.iter().enumerate() {
        if let Some(item_id) = li_identities.get(&index) {
            prefix.push(PathSegment::Item(item_id.clone()));
            out.push((FieldPath::new(prefix.clone()), child));
            prefix.pop();
            continue;
        }
        prefix.push(PathSegment::Child(child.tag.clone()));
        match &child.content {
            Content::Children(grandchildren) if !grandchildren.is_empty() => {
                collect_leaves(child, prefix, out);
            }
            _ => out.push((FieldPath::new(prefix.clone()), child)),
        }
        prefix.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(tag: &str, text: &str) -> FieldNode {
        FieldNode {
            tag: tag.to_string(),
            attrs: BTreeMap::new(),
            content: Content::Text(text.to_string()),
        }
    }

    fn container(tag: &str, children: Vec<FieldNode>) -> FieldNode {
        FieldNode {
            tag: tag.to_string(),
            attrs: BTreeMap::new(),
            content: Content::Children(children),
        }
    }

    fn li_with_class(class: &str, children: Vec<FieldNode>) -> FieldNode {
        let mut attrs = BTreeMap::new();
        attrs.insert("Class".to_string(), class.to_string());
        FieldNode {
            tag: "li".to_string(),
            attrs,
            content: Content::Children(children),
        }
    }

    fn li_text(text: &str) -> FieldNode {
        FieldNode {
            tag: "li".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Text(text.to_string()),
        }
    }

    fn tree(root: FieldNode) -> FieldTree {
        FieldTree {
            root,
            parent_name: None,
            name: None,
        }
    }

    #[test]
    fn identify_prefers_class_over_key_child_over_text_over_position() {
        let by_class = li_with_class("Foo.Bar", vec![leaf("defName", "X")]);
        assert_eq!(by_class.identify(0), ItemId::Class("Foo.Bar".to_string()));

        let by_key = container("li", vec![leaf("defName", "Plant_Grass")]);
        assert_eq!(
            by_key.identify(0),
            ItemId::Key {
                child: "defName".to_string(),
                value: "Plant_Grass".to_string()
            }
        );

        let by_text = li_text("ImplantEmpireCommon");
        assert_eq!(
            by_text.identify(0),
            ItemId::Text("ImplantEmpireCommon".to_string())
        );

        let by_position = FieldNode {
            tag: "li".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Empty,
        };
        assert_eq!(by_position.identify(3), ItemId::Position(3));
    }

    #[test]
    fn identify_tries_key_children_in_declared_order() {
        // `defName` beats `stat` when both are present.
        let node = container("li", vec![leaf("stat", "Mass"), leaf("defName", "Steel")]);
        assert_eq!(
            node.identify(0),
            ItemId::Key {
                child: "defName".to_string(),
                value: "Steel".to_string()
            }
        );
    }

    #[test]
    fn get_walks_named_children_and_a_class_identified_item() {
        let root = container(
            "HediffDef",
            vec![
                leaf("label", "bionic heart"),
                container(
                    "comps",
                    vec![li_with_class(
                        "ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust",
                        vec![leaf("scaleAdjustment", "0.20")],
                    )],
                ),
            ],
        );
        let tree = tree(root);

        let label: FieldPath = "label".parse().unwrap();
        assert_eq!(
            tree.get(&label).unwrap().content,
            Content::Text("bionic heart".to_string())
        );

        let scale: FieldPath =
            "comps/li[@Class=ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust]/scaleAdjustment"
                .parse()
                .unwrap();
        assert_eq!(
            tree.get(&scale).unwrap().content,
            Content::Text("0.20".to_string())
        );

        let missing: FieldPath = "nope".parse().unwrap();
        assert!(tree.get(&missing).is_none());
    }

    #[test]
    fn leaves_yields_every_leaf_and_treats_li_items_as_one_unit() {
        let root = container(
            "HediffDef",
            vec![
                leaf("label", "bionic heart"),
                container(
                    "addedPartProps",
                    vec![leaf("solid", "true"), leaf("partEfficiency", "1.25")],
                ),
                container(
                    "comps",
                    vec![li_with_class("Foo", vec![leaf("scaleAdjustment", "0.20")])],
                ),
            ],
        );
        let tree = tree(root);

        let paths: Vec<String> = tree.leaves().map(|(path, _)| path.to_string()).collect();
        assert_eq!(
            paths,
            vec![
                "label".to_string(),
                "addedPartProps/solid".to_string(),
                "addedPartProps/partEfficiency".to_string(),
                "comps/li[@Class=Foo]".to_string(),
            ]
        );
    }

    #[test]
    fn longest_existing_prefix_stops_at_the_first_missing_segment() {
        let root = container("HediffDef", vec![leaf("label", "x")]);
        let tree = tree(root);

        let path: FieldPath = "defaultLabelColor".parse().unwrap();
        assert_eq!(tree.longest_existing_prefix(&path), FieldPath::new(vec![]));

        let full: FieldPath = "label".parse().unwrap();
        assert_eq!(tree.longest_existing_prefix(&full), full);
    }

    #[test]
    fn duplicate_identities_in_one_container_fall_back_to_position_for_both() {
        let root = container(
            "ThingDef",
            vec![container(
                "comps",
                vec![
                    li_with_class("Foo", vec![leaf("a", "1")]),
                    li_with_class("Foo", vec![leaf("a", "2")]),
                    li_with_class("Bar", vec![leaf("a", "3")]),
                ],
            )],
        );
        let tree = tree(root);

        let paths: Vec<String> = tree.leaves().map(|(path, _)| path.to_string()).collect();
        // The two `Class="Foo"` items collide, so both fall back to
        // position; the unique `Bar` item keeps its class identity.
        assert_eq!(
            paths,
            vec![
                "comps/li[#0]".to_string(),
                "comps/li[#1]".to_string(),
                "comps/li[@Class=Bar]".to_string(),
            ]
        );

        // Each positional path still resolves to its own distinct node.
        let first = tree.get(&"comps/li[#0]".parse().unwrap()).unwrap();
        let second = tree.get(&"comps/li[#1]".parse().unwrap()).unwrap();
        assert_eq!(first.content, Content::Children(vec![leaf("a", "1")]));
        assert_eq!(second.content, Content::Children(vec![leaf("a", "2")]));
    }

    #[test]
    fn container_kind_classifies_a_tag_keyed_dictionary_shape_as_keyed_map() {
        let wild_animals = container(
            "wildAnimals",
            vec![leaf("Donkey", "0.2"), leaf("XBM_Theropod", "0.6")],
        );
        assert_eq!(container_kind(&wild_animals), Some(ContainerKind::KeyedMap));

        let entries = keyed_map_entries(&wild_animals).expect("a keyed map");
        assert_eq!(
            entries.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(),
            vec!["Donkey", "XBM_Theropod"]
        );
    }

    #[test]
    fn container_kind_classifies_any_li_child_as_list_even_alongside_other_tags() {
        let comps = container(
            "comps",
            vec![
                li_with_class("Foo", vec![leaf("a", "1")]),
                leaf("stray", "x"),
            ],
        );
        assert_eq!(container_kind(&comps), Some(ContainerKind::List));
        assert_eq!(keyed_map_entries(&comps), None);
    }

    #[test]
    fn container_kind_classifies_nested_subtrees_as_record() {
        let props = container(
            "props",
            vec![container("nested", vec![leaf("x", "1")]), leaf("y", "2")],
        );
        assert_eq!(container_kind(&props), Some(ContainerKind::Record));
        assert_eq!(keyed_map_entries(&props), None);
    }

    #[test]
    fn container_kind_classifies_a_duplicate_tag_as_record_never_a_map() {
        // A real `Dictionary` field can never legally have a
        // duplicate key, so this shape is never guessed at as one.
        let malformed = container(
            "wildAnimals",
            vec![leaf("Donkey", "0.2"), leaf("Donkey", "0.4")],
        );
        assert_eq!(container_kind(&malformed), Some(ContainerKind::Record));
        assert_eq!(keyed_map_entries(&malformed), None);
    }

    #[test]
    fn container_kind_classifies_an_empty_container_as_record() {
        let empty = FieldNode {
            tag: "wildAnimals".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Children(Vec::new()),
        };
        assert_eq!(container_kind(&empty), Some(ContainerKind::Record));
    }

    #[test]
    fn container_kind_is_none_for_a_leaf() {
        assert_eq!(container_kind(&leaf("label", "x")), None);
        let empty_leaf = FieldNode {
            tag: "label".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Empty,
        };
        assert_eq!(container_kind(&empty_leaf), None);
    }

    #[test]
    fn container_kind_treats_a_leaf_only_record_as_a_keyed_map_too() {
        // `addedPartProps`-shaped: distinct leaf tags, no `li` — the same
        // rule as `wildAnimals`, deliberately, per this function's own
        // doc comment (no "known map tags" table).
        let added_part_props = container(
            "addedPartProps",
            vec![leaf("solid", "true"), leaf("partEfficiency", "1.25")],
        );
        assert_eq!(
            container_kind(&added_part_props),
            Some(ContainerKind::KeyedMap)
        );
    }

    #[test]
    fn get_mut_and_remove_at_reach_the_same_item_get_finds() {
        let root = container(
            "HediffDef",
            vec![container(
                "comps",
                vec![li_with_class("Foo", vec![leaf("x", "1")])],
            )],
        );
        let mut tree = tree(root);
        let path: FieldPath = "comps/li[@Class=Foo]".parse().unwrap();
        assert!(tree.get(&path).is_some());

        assert!(get_mut(&mut tree.root, path.segments()).is_some());
        assert!(remove_at(&mut tree.root, path.segments()));
        assert!(tree.get(&path).is_none());
        // Removing again finds nothing left to remove.
        assert!(!remove_at(&mut tree.root, path.segments()));
    }
}
