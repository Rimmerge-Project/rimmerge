//! Shared, owner-agnostic walk for candidate def-name reference sites —
//! used by [`super::defs`] (a def/template's own field tree) and
//! [`super::patches`] (a mutating op's own `<value>`). Owner-independent:
//! neither caller's own "who owns this site" concept (a def, a template,
//! or a patch op with its own mod) is known here, so this produces
//! [`RawRefSite`]s and leaves wrapping one into a
//! [`crate::domain::RefSite`] to the caller, who alone has that context.

use std::collections::HashSet;
use std::sync::Arc;

use roxmltree::Node;

use crate::domain::RefSiteShape;

use super::xml_util::csv_attr;

/// How many `/`-segments deep [`collect`] descends — generous headroom
/// over the shallow content-predicate shapes elsewhere in this crate
/// (`MAX_INLINE_NODE_PATH_DEPTH`), since a reference field can sit well
/// below a shallow container (`comps/li/props/thingDef`), while still
/// bounding the walk against pathological XML depth.
pub const MAX_REF_SITE_DEPTH: usize = 16;

/// The most [`RawRefSite`]s one file contributes before a caller's own
/// truncation flag should be set — a defensive bound, not a measured
/// real-install size.
pub const MAX_REF_SITES_PER_FILE: usize = 20_000;

/// Hands out one shared [`Arc<str>`] per distinct text — how a caller
/// turning [`RawRefSite`]s into [`crate::domain::RefSite`]s shares each
/// repeated def type and field path instead of allocating it per site.
#[derive(Debug, Default)]
pub struct TextInterner {
    seen: HashSet<Arc<str>>,
}

impl TextInterner {
    /// The shared copy of `text`, allocated on first sight.
    pub fn intern(&mut self, text: &str) -> Arc<str> {
        if let Some(shared) = self.seen.get(text) {
            return Arc::clone(shared);
        }
        let shared: Arc<str> = Arc::from(text);
        self.seen.insert(Arc::clone(&shared));
        shared
    }
}

/// One candidate reference-site value, with no owner attached yet — see
/// this module's own doc comment.
pub struct RawRefSite {
    /// The owner's own element tag (`ThingDef`, ...), for every shape.
    pub def_type: String,
    pub field_path: String,
    pub shape: RefSiteShape,
    pub value: String,
    pub may_require: Vec<String>,
    pub may_require_any_of: Vec<String>,
}

/// Walks `owner_element`'s own field tree, collecting every [`RawRefSite`]
/// into `out` — see [`RefSiteShape`] for the shapes recognized and
/// `analysis::references` for how these candidates become a vote.
///
/// `budget` is the running count of sites produced *so far in the whole
/// file* (not just this call's own `out`) — a caller collecting from
/// several defs/templates/ops in one file threads the same counter
/// through every call, incrementing it here, so
/// [`MAX_REF_SITES_PER_FILE`] bounds the *file*, not each individual
/// owner. Once `budget` would exceed the cap, `*truncated` is set and the
/// walk stops early.
pub fn collect(
    owner_element: Node,
    def_type: &str,
    out: &mut Vec<RawRefSite>,
    budget: &mut usize,
    truncated: &mut bool,
) {
    walk_container(owner_element, def_type, "", 0, out, budget, truncated);
}

/// One container level of [`collect`]'s own walk. `prefix` is
/// `container`'s own field path (`li` segments already collapsed to the
/// literal `"li"`), built up one segment per recursion level.
///
/// A leaf child is recorded two ways at once, deliberately — which one
/// (if either) is genuinely a reference is a vote `analysis::references`
/// runs later, not something this purely structural walk can decide on
/// its own:
/// - as [`RefSiteShape::ListItem`] when its own tag is `li`, else as
///   [`RefSiteShape::Scalar`] under its own full path (`kindDef`, a
///   single named field whose own value is the reference); and,
///   additionally for a non-`li` leaf,
/// - as [`RefSiteShape::KeyedElement`] under the *parent* container's own
///   path, with the child's own tag name as the value (`costList`'s
///   `<Steel>10</Steel>`, where the reference is the tag, not the text).
///
/// A container literally named `descriptionHyperlinks` is a third, fixed
/// shape (confirmed engine-typed): every leaf child becomes a
/// [`RefSiteShape::Hyperlink`] whose text is the def name — no vote
/// needed, so this container's children never also go through the
/// ordinary leaf handling above. The child's own tag names the *target's*
/// def type; nothing reads it (resolution is type-agnostic), so it is not
/// recorded, and `def_type` stays the owner's, which is what a referrer
/// label and a template's descendant check need.
#[allow(clippy::too_many_arguments)]
fn walk_container(
    container: Node,
    def_type: &str,
    prefix: &str,
    depth: usize,
    out: &mut Vec<RawRefSite>,
    budget: &mut usize,
    truncated: &mut bool,
) {
    if depth >= MAX_REF_SITE_DEPTH || *truncated {
        return;
    }
    let is_hyperlinks = container.tag_name().name() == "descriptionHyperlinks";

    for child in container.children().filter(Node::is_element) {
        if *budget >= MAX_REF_SITES_PER_FILE {
            *truncated = true;
            return;
        }
        let tag = child.tag_name().name();
        let has_element_children = child.children().any(|c| c.is_element());

        if is_hyperlinks {
            if !has_element_children && let Some(text) = leaf_text(child) {
                out.push(RawRefSite {
                    def_type: def_type.to_string(),
                    field_path: prefix.to_string(),
                    shape: RefSiteShape::Hyperlink,
                    value: text.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                });
                *budget += 1;
            }
            continue;
        }

        let segment = if tag == "li" { "li" } else { tag };
        let child_field_path = if prefix.is_empty() {
            segment.to_string()
        } else {
            format!("{prefix}/{segment}")
        };

        if has_element_children {
            walk_container(
                child,
                def_type,
                &child_field_path,
                depth + 1,
                out,
                budget,
                truncated,
            );
            continue;
        }
        let Some(text) = leaf_text(child) else {
            continue;
        };

        if tag == "li" {
            out.push(RawRefSite {
                def_type: def_type.to_string(),
                field_path: child_field_path,
                shape: RefSiteShape::ListItem,
                value: text.to_string(),
                may_require: csv_attr(child, "MayRequire"),
                may_require_any_of: csv_attr(child, "MayRequireAnyOf"),
            });
            *budget += 1;
            continue;
        }

        out.push(RawRefSite {
            def_type: def_type.to_string(),
            field_path: child_field_path,
            shape: RefSiteShape::Scalar,
            value: text.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
        });
        *budget += 1;
        if *budget < MAX_REF_SITES_PER_FILE {
            out.push(RawRefSite {
                def_type: def_type.to_string(),
                field_path: prefix.to_string(),
                shape: RefSiteShape::KeyedElement,
                value: tag.to_string(),
                may_require: csv_attr(child, "MayRequire"),
                may_require_any_of: csv_attr(child, "MayRequireAnyOf"),
            });
            *budget += 1;
        }
    }
}

/// `node`'s own trimmed text, but only when `node` is a genuine leaf — no
/// child *elements* of its own — and that text is non-empty.
fn leaf_text<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    if node.children().any(|c| c.is_element()) {
        return None;
    }
    node.text().map(str::trim).filter(|s| !s.is_empty())
}
