//! Folding contributions into the effective def: shadows, leaf sources, and inheritance.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::DefKey;

use super::Provenance;
use crate::inherit::{self, InheritError, TemplateSet};
use crate::tree::{self, Content, FieldNode, FieldPath, FieldTree, ItemId, PathSegment};

/// `tree`'s leaves, as an owned `path -> node` map — the shape [`compute`]
/// needs to diff one stage's output against the previous one field by
/// field (a leaf whose whole [`FieldNode`] — tag, attributes, and content
/// alike — is unchanged didn't move at this stage; anything else did).
pub(super) fn owned_leaves(tree: &FieldTree) -> BTreeMap<FieldPath, FieldNode> {
    tree.leaves()
        .map(|(path, node)| (path, node.clone()))
        .collect()
}

/// A value-free mirror of [`FieldNode::content`], carrying a
/// [`Provenance`] at every leaf/`li` position instead of real field data.
/// [`Shadow::of`] builds one matching a [`FieldNode`]'s exact shape; the
/// inheritance fold in [`fold_inheritance`] then combines two shadows via
/// [`merge_shadow`] every time it combines two real nodes via
/// [`inherit::merge_over`], so a leaf's shadow always sits at the exact
/// position the real leaf ends up at — including a `li` whose identity
/// only collides (and falls back to position, per
/// [`crate::tree::identify_all_li`]) once a later layer's own item is
/// appended, since [`zip_leaves`] reads the shadow off by *position*, not
/// by re-deriving a path early and hoping it still matches at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Shadow {
    /// One field's own provenance.
    Leaf(Provenance),
    /// One element's children, same order and length as the real
    /// [`FieldNode`]'s own [`Content::Children`] (never built for
    /// [`Content::Empty`]/[`Content::Text`], nor for a technically-legal
    /// but never-produced-in-practice [`Content::Children`] holding zero
    /// elements — [`crate::xml::parse`] only ever constructs the variant
    /// when there is at least one child, the same assumption
    /// [`crate::tree::collect_leaves`] already relies on).
    Children(Vec<Shadow>),
}

/// Where [`Shadow::of`] should attribute a leaf it discovers.
enum LeafSource<'a> {
    /// Every leaf gets the same attribution — one `ParentName` ancestor's
    /// own layer, or a leaf newly appended wholesale from one.
    Uniform(Provenance),
    /// Look up each leaf's attribution by its own path in a map that
    /// already has one for every leaf of the node [`Shadow::of`] is
    /// walking — the winner's raw-plus-patched tree, whose every leaf
    /// [`compute`]'s Owner/Patch fold already attributed before
    /// inheritance ever runs.
    ByPath {
        /// The already-known attribution for every leaf of the node this
        /// source is paired with.
        provenance: &'a BTreeMap<FieldPath, Provenance>,
        /// Attributed to [`Provenance::Owner`] if a path is somehow
        /// missing — defensive only: every leaf of the paired node was
        /// inserted into `provenance` by [`compute`]'s own fold before
        /// [`fold_inheritance`] ever runs, so this is unreachable in
        /// practice, but nothing here can prove that invariant to the
        /// compiler.
        fallback: &'a ModId,
    },
}

impl LeafSource<'_> {
    fn resolve(&self, path: &[PathSegment]) -> Provenance {
        match self {
            LeafSource::Uniform(provenance) => provenance.clone(),
            LeafSource::ByPath {
                provenance,
                fallback,
            } => provenance
                .get(&FieldPath::new(path.to_vec()))
                .cloned()
                .unwrap_or_else(|| Provenance::Owner((*fallback).clone())),
        }
    }
}

impl Shadow {
    /// Builds a shadow matching `node`'s exact shape, attributing every
    /// leaf via `source` — the same leaf/container split
    /// [`crate::tree::collect_leaves`] uses (a `li` item is always one
    /// leaf, whatever its own internal structure).
    fn of(node: &FieldNode, source: &LeafSource<'_>, path: &mut Vec<PathSegment>) -> Self {
        let Content::Children(children) = &node.content else {
            return Shadow::Leaf(source.resolve(path));
        };
        if children.is_empty() {
            return Shadow::Leaf(source.resolve(path));
        }
        let li_identities: BTreeMap<usize, ItemId> =
            tree::identify_all_li(children).into_iter().collect();
        let shadows = children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                if let Some(item_id) = li_identities.get(&index) {
                    path.push(PathSegment::Item(item_id.clone()));
                    let shadow = Shadow::Leaf(source.resolve(path));
                    path.pop();
                    return shadow;
                }
                path.push(PathSegment::Child(child.tag.clone()));
                let shadow = Shadow::of(child, source, path);
                path.pop();
                shadow
            })
            .collect();
        Shadow::Children(shadows)
    }
}

/// Combines `parent_shadow`/`child_shadow` the same way
/// [`inherit::merge_over`] combines `parent`/`child` themselves — kept in
/// sync with that function's own branches by hand (mirroring its match on
/// `(parent.content, child.content)` exactly, arm for arm) since a
/// [`Shadow`] carries no content of its own to dispatch on; see this
/// module's own doc comment for why a diff-based alternative can't
/// replace this. [`inherit::merge_over`]'s doc comment is the source of
/// truth this must track.
pub(super) fn merge_shadow(
    parent: &FieldNode,
    parent_shadow: &Shadow,
    child: &FieldNode,
    child_shadow: &Shadow,
) -> Shadow {
    if inherit::is_inherit_false(child) {
        return child_shadow.clone();
    }
    match (&parent.content, &child.content) {
        // Not an override — the parent's own shadow (whatever supplied
        // it) stands untouched, exactly as its content does.
        (_, Content::Empty) => parent_shadow.clone(),
        // A leaf child's content replaces the parent's outright,
        // regardless of whether the two happen to render identically
        // (a redeclaration with the same value is still an override).
        (_, Content::Text(_)) => child_shadow.clone(),
        (Content::Children(parent_children), Content::Children(child_children)) => {
            // Hardening: when *both* sides' `Content::Children` are empty,
            // falling through to `merge_children_shadow` below with an
            // empty slice substituted for both sides would return
            // `Shadow::Children(vec![])`. But `Shadow::of` never builds
            // that shape: its own empty-`Content::Children` rule always
            // returns `Shadow::Leaf` instead (this match's whole point is
            // to track that rule in lockstep — see this function's own
            // doc comment), and `zip_leaves` relies on that invariant
            // unconditionally (its own `Content::Children`-empty arm
            // unwraps straight to `Shadow::Leaf`, `unreachable!`
            // otherwise). Today's two real call sites (`fold_inheritance`)
            // only ever hand `parent` a raw, unpatched ancestor —
            // `xml::parse` never itself produces an empty
            // `Content::Children` (only `Content::Empty`, see
            // `Shadow::Children`'s own doc comment), so only a
            // `PatchOperationRemove`-emptied *child* side reaches this arm
            // with an empty `Content::Children` today, never the parent
            // side too — this is defense-in-depth against the day that
            // stops being true (a future ancestor-chain fold, or a
            // relaxed "templates are consumed unpatched" assumption),
            // not a fix for a currently-live panic; see
            // `merge_shadow_of_a_container_emptied_by_a_remove_on_both_sides...`
            // below, which calls this function directly since `compute`'s
            // own public API can't reach the state today. Returning
            // `child_shadow` directly mirrors the `Content::Text` arm's own
            // "a redeclaration is still an override" rule above;
            // `child_shadow` is guaranteed already `Shadow::Leaf` here —
            // `Shadow::of`, and this same rule recursively, never build
            // anything else for an empty `Content::Children`.
            if parent_children.is_empty() && child_children.is_empty() {
                return child_shadow.clone();
            }
            // `Shadow::of` builds `Shadow::Leaf` for an *empty*
            // `Content::Children`, same as for `Content::Empty`/
            // `Content::Text` (its own doc comment says so explicitly) —
            // so this arm checks emptiness, not only the *variant*: an
            // empty-children side must never unwrap a `Shadow::Leaf` as
            // if it were `Shadow::Children`. `inherit::merge_over`
            // itself never needs this distinction: its own
            // `merge_children(parent_children, child_children)` already
            // does the right thing when either list is empty (an empty
            // `child_children` loop body never touches `result`; an empty
            // `parent_children` start just means every `child_children`
            // entry gets matched-or-pushed against nothing, i.e. against
            // an initially-empty accumulator) — so `merge_children_shadow`
            // only needs the *same* empty slice substituted for whichever
            // side has no real children to unwrap a shadow list from;
            // there are no leaves under an empty list to lose either way.
            let empty: Vec<Shadow> = Vec::new();
            let parent_shadows = if parent_children.is_empty() {
                &empty
            } else {
                let Shadow::Children(v) = parent_shadow else {
                    unreachable!(
                        "Shadow::of only builds Shadow::Children for a non-empty Content::Children"
                    )
                };
                v
            };
            let child_shadows = if child_children.is_empty() {
                &empty
            } else {
                let Shadow::Children(v) = child_shadow else {
                    unreachable!(
                        "Shadow::of only builds Shadow::Children for a non-empty Content::Children"
                    )
                };
                v
            };
            Shadow::Children(merge_children_shadow(
                parent_children,
                parent_shadows,
                child_children,
                child_shadows,
            ))
        }
        // The child introduces structure the parent didn't have — its
        // own shadow (already built wholesale by `Shadow::of`) stands as
        // given, exactly as its content does.
        (_, Content::Children(_)) => child_shadow.clone(),
    }
}

/// [`merge_shadow`] alongside [`inherit::merge_children`]'s own fold:
/// same iteration, same tag-matching, so the vector it builds always stays
/// index-aligned with the real children [`inherit::merge_children`]
/// produces.
///
/// **Each matching `child_children` entry merges against the
/// progressively-updated node** [`inherit::merge_children`] itself merges
/// against (`result[position]`, re-read fresh on every match), never
/// against `parent_children[position]`/`parent_shadows[position]` — the
/// *original*, unmodified parent (a real-install shape, `FactionDef/Insect`
/// among others). The two agree for
/// a `child_children` with at most one entry per tag, which is almost
/// always true — but a same-tag entry can appear more than once (a
/// mod's own `PatchOperationAdd`/`Insert` matching several existing
/// nodes at once credits each match its own leaf, all under one
/// top-level op, and RimWorld's own `XmlInheritance` never rejects a
/// duplicate top-level tag either). When it does, merging each one
/// independently against the stale original would silently drop whichever
/// merge didn't happen to run last, while the real fold chains them, so
/// the two could disagree on both *value* and *shape* at that position:
/// exactly the `Shadow::Leaf`-vs-`Shadow::Children` mismatch `zip_leaves`
/// catches. A parallel, equally-progressive `result_nodes` is tracked
/// alongside the shadow `result`, mirroring [`inherit::merge_children`]'s
/// own `result[position]` re-read exactly.
fn merge_children_shadow(
    parent_children: &[FieldNode],
    parent_shadows: &[Shadow],
    child_children: &[FieldNode],
    child_shadows: &[Shadow],
) -> Vec<Shadow> {
    let mut result_nodes = parent_children.to_vec();
    let mut result_shadows = parent_shadows.to_vec();
    for (child, child_shadow) in child_children.iter().zip(child_shadows) {
        if child.tag == "li" {
            result_shadows.push(child_shadow.clone());
            continue;
        }
        match result_nodes.iter().position(|node| node.tag == child.tag) {
            Some(position) => {
                result_shadows[position] = merge_shadow(
                    &result_nodes[position],
                    &result_shadows[position],
                    child,
                    child_shadow,
                );
                result_nodes[position] = inherit::merge_over(&result_nodes[position], child);
            }
            None => {
                result_shadows.push(child_shadow.clone());
                result_nodes.push(child.clone());
            }
        }
    }
    result_shadows
}

/// Walks `node`/`shadow` in lockstep, exactly the way
/// [`crate::tree::FieldTree::leaves`] walks a tree alone (same
/// leaf/container split, same [`crate::tree::identify_all_li`] identity
/// assignment) — so the paths produced here are always exactly
/// [`FieldTree::leaves`]'s own path set for `node`, no separate trimming
/// pass required, reading each leaf's [`Provenance`] off `shadow` at the
/// position [`Shadow::of`]/[`merge_shadow`] put it.
fn zip_leaves(
    node: &FieldNode,
    shadow: &Shadow,
    path: &mut Vec<PathSegment>,
    out: &mut BTreeMap<FieldPath, Provenance>,
) {
    let Content::Children(children) = &node.content else {
        let Shadow::Leaf(provenance) = shadow else {
            unreachable!(
                "Shadow::of/merge_shadow only put Shadow::Leaf at a non-container position"
            )
        };
        out.insert(FieldPath::new(path.clone()), provenance.clone());
        return;
    };
    if children.is_empty() {
        let Shadow::Leaf(provenance) = shadow else {
            unreachable!(
                "Shadow::of/merge_shadow only put Shadow::Leaf at a non-container position"
            )
        };
        out.insert(FieldPath::new(path.clone()), provenance.clone());
        return;
    }
    let Shadow::Children(shadows) = shadow else {
        unreachable!(
            "Shadow::of/merge_shadow only put Shadow::Children at a Content::Children position"
        )
    };
    let li_identities: BTreeMap<usize, ItemId> =
        tree::identify_all_li(children).into_iter().collect();
    for (index, (child, child_shadow)) in children.iter().zip(shadows).enumerate() {
        if let Some(item_id) = li_identities.get(&index) {
            path.push(PathSegment::Item(item_id.clone()));
            let Shadow::Leaf(provenance) = child_shadow else {
                unreachable!("a li item is always one leaf, per Shadow::of")
            };
            out.insert(FieldPath::new(path.clone()), provenance.clone());
            path.pop();
            continue;
        }
        path.push(PathSegment::Child(child.tag.clone()));
        match &child.content {
            Content::Children(grandchildren) if !grandchildren.is_empty() => {
                zip_leaves(child, child_shadow, path, out);
            }
            _ => {
                let Shadow::Leaf(provenance) = child_shadow else {
                    unreachable!("Shadow::of only builds Shadow::Leaf for a non-container child")
                };
                out.insert(FieldPath::new(path.clone()), provenance.clone());
            }
        }
        path.pop();
    }
}

/// What [`fold_inheritance`] produced.
pub(super) struct FoldedInheritance {
    /// The resolved tree — [`inherit::resolve`]'s own value, computed via
    /// the exact same [`inherit::merge_over`] fold (never reimplemented),
    /// so it can never disagree with `resolved` elsewhere in this crate.
    pub(super) resolved: FieldTree,
    /// Every leaf of [`Self::resolved`], attributed — always exactly
    /// [`FieldTree::leaves`]'s own path set for it, by construction (see
    /// [`zip_leaves`]).
    pub(super) provenance: BTreeMap<FieldPath, Provenance>,
}

/// Resolves `patched`'s `ParentName` chain against `templates`, folding
/// root-most ancestor down to `patched` itself exactly the way
/// [`inherit::resolve`] does (ancestor chain via [`inherit::ancestor_chain`],
/// each step via [`inherit::merge_over`], `patched` itself as the fold's
/// final "child" layer, then [`inherit::strip_inherit_recursive`]) —
/// except every [`inherit::merge_over`] call runs alongside a
/// [`merge_shadow`] call on a parallel [`Shadow`], attributing each layer:
/// an ancestor's own leaves go to [`Provenance::Inherited`] (or
/// [`Provenance::UnattributedTemplate`] when `template_owners` has no
/// entry for it — never silently dropped), and `patched`'s own leaves
/// keep whatever [`Provenance::Owner`]/[`Provenance::Patch`] `provenance`
/// already recorded for them (looked up by path via
/// [`LeafSource::ByPath`], since `patched` is exactly the tree whose
/// leaves `provenance` was built from).
///
/// # Errors
///
/// Propagates [`inherit::ancestor_chain`]'s [`InheritError`] — a
/// `ParentName` chain that can't be walked at all (missing template, or a
/// cycle) has no fold to run.
pub(super) fn fold_inheritance(
    patched: &FieldTree,
    templates: &TemplateSet,
    template_owners: &BTreeMap<(String, String), ModId>,
    provenance: &BTreeMap<FieldPath, Provenance>,
    winner: &ModId,
) -> Result<FoldedInheritance, InheritError> {
    let chain =
        inherit::ancestor_chain(patched.parent_name.as_deref(), &patched.root.tag, templates)?;

    let mut accumulated: Option<(FieldNode, Shadow)> = None;
    for (ancestor, key) in chain {
        let source = LeafSource::Uniform(match template_owners.get(&key) {
            Some(owner) => Provenance::Inherited {
                template: DefKey {
                    def_type: key.0.clone(),
                    def_name: key.1.clone(),
                },
                owner: owner.clone(),
            },
            None => Provenance::UnattributedTemplate {
                template: DefKey {
                    def_type: key.0.clone(),
                    def_name: key.1.clone(),
                },
            },
        });
        let child_shadow = Shadow::of(&ancestor.root, &source, &mut Vec::new());
        accumulated = Some(match accumulated {
            None => (ancestor.root.clone(), child_shadow),
            Some((parent_node, parent_shadow)) => {
                let node = inherit::merge_over(&parent_node, &ancestor.root);
                let shadow =
                    merge_shadow(&parent_node, &parent_shadow, &ancestor.root, &child_shadow);
                (node, shadow)
            }
        });
    }

    let child_source = LeafSource::ByPath {
        provenance,
        fallback: winner,
    };
    let child_shadow = Shadow::of(&patched.root, &child_source, &mut Vec::new());
    let (mut root, shadow) = match accumulated {
        None => (patched.root.clone(), child_shadow),
        Some((parent_node, parent_shadow)) => {
            let node = inherit::merge_over(&parent_node, &patched.root);
            let shadow = merge_shadow(&parent_node, &parent_shadow, &patched.root, &child_shadow);
            (node, shadow)
        }
    };
    inherit::strip_inherit_recursive(&mut root);

    let mut resolved_provenance = BTreeMap::new();
    zip_leaves(&root, &shadow, &mut Vec::new(), &mut resolved_provenance);

    Ok(FoldedInheritance {
        resolved: FieldTree {
            root,
            parent_name: None,
            name: None,
        },
        provenance: resolved_provenance,
    })
}

/// When replaying one contribution introduces a new `li` sibling that
/// collides with an existing one's own declared identity,
/// [`tree::identify_all_li`]'s own duplicate-identity fallback renumbers
/// *every* item sharing that identity to [`ItemId::Position`] — including
/// one this contribution never touched. Diffing `before`/`after` by path
/// alone then sees that untouched item's own path vanish and a
/// same-content one appear under a new positional path, and would
/// misattribute it to the mod that merely triggered the renumbering (the
/// "list case" quirk: same-identity items ending up credited to whichever
/// mod happened to load last).
///
/// Finds the one path `before` had — not already [`claimed`] by an earlier
/// match in this same pass, and no longer present in `after` (still
/// present would mean it's a *different* surviving item, not this one
/// renamed) — whose node is *exactly* `node` and which sits under the same
/// parent container as `path`, restricted to `li` items on both sides: a
/// coincidental match on an unrelated field must never borrow that field's
/// own attribution.
pub(super) fn renamed_list_item_source<'a>(
    before: &'a BTreeMap<FieldPath, FieldNode>,
    after: &BTreeMap<FieldPath, FieldNode>,
    path: &FieldPath,
    node: &FieldNode,
    claimed: &BTreeSet<FieldPath>,
) -> Option<&'a FieldPath> {
    // `split_last` over `segments[..segments.len() - 1]` for the same
    // reason `assign::accumulate_leaf` uses it: the parent slice falls
    // out of the match itself rather than a length subtraction the
    // pattern match has to be trusted to keep in sync with.
    let Some((PathSegment::Item(_), parent)) = path.segments().split_last() else {
        return None;
    };
    for (candidate, candidate_node) in before {
        if after.contains_key(candidate) || claimed.contains(candidate) {
            continue;
        }
        let Some((PathSegment::Item(_), candidate_parent)) = candidate.segments().split_last()
        else {
            continue;
        };
        if candidate_parent != parent {
            continue;
        }
        if candidate_node == node {
            return Some(candidate);
        }
    }
    None
}
