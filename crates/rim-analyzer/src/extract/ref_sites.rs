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
/// this module's own doc comment. Borrowed: `def_type` from the caller,
/// `field_path` from the walk's own reused path buffer, and `value` from the
/// parsed document, so a caller keeps (and allocates) only what it stores.
pub struct RawRefSite<'a> {
    /// The owner's own element tag (`ThingDef`, ...), for every shape.
    pub def_type: &'a str,
    pub field_path: &'a str,
    pub shape: RefSiteShape,
    pub value: &'a str,
    pub may_require: Vec<String>,
    pub may_require_any_of: Vec<String>,
}

/// Walks `owner_element`'s own field tree, handing every [`RawRefSite`] to
/// `sink` in document order — see [`RefSiteShape`] for the shapes recognized
/// and `analysis::references` for how these candidates become a vote.
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
    sink: &mut dyn FnMut(RawRefSite<'_>),
    budget: &mut usize,
    truncated: &mut bool,
) {
    let mut walk = Walk {
        def_type,
        path: String::new(),
        sink,
        budget,
        truncated,
    };
    walk.container(owner_element, 0);
}

/// The state [`collect`]'s recursion threads through every level: the
/// field path is one buffer, extended by a segment on the way down and cut
/// back on the way up, instead of a fresh string per element.
struct Walk<'w, 'f> {
    def_type: &'w str,
    path: String,
    sink: &'w mut (dyn FnMut(RawRefSite<'_>) + 'f),
    budget: &'w mut usize,
    truncated: &'w mut bool,
}

impl Walk<'_, '_> {
    /// One container level of [`collect`]'s own walk. `self.path` holds
    /// `container`'s own field path (`li` segments already collapsed to the
    /// literal `"li"`) on entry and on return.
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
    fn container(&mut self, container: Node, depth: usize) {
        if depth >= MAX_REF_SITE_DEPTH || *self.truncated {
            return;
        }
        let is_hyperlinks = container.tag_name().name() == "descriptionHyperlinks";

        for child in container.children().filter(Node::is_element) {
            if *self.budget >= MAX_REF_SITES_PER_FILE {
                *self.truncated = true;
                return;
            }
            if is_hyperlinks {
                self.hyperlink(child);
                continue;
            }
            let prefix_len = self.path.len();
            if prefix_len > 0 {
                self.path.push('/');
            }
            self.path.push_str(child.tag_name().name());
            self.field(child, prefix_len, depth);
            self.path.truncate(prefix_len);
        }
    }

    fn emit(
        &mut self,
        field_path_len: usize,
        shape: RefSiteShape,
        value: &str,
        gated_by: Option<Node>,
    ) {
        (self.sink)(RawRefSite {
            def_type: self.def_type,
            field_path: &self.path[..field_path_len],
            shape,
            value,
            may_require: gated_by.map_or_else(Vec::new, |node| csv_attr(node, "MayRequire")),
            may_require_any_of: gated_by
                .map_or_else(Vec::new, |node| csv_attr(node, "MayRequireAnyOf")),
        });
        *self.budget += 1;
    }

    fn hyperlink(&mut self, child: Node) {
        if child.children().any(|c| c.is_element()) {
            return;
        }
        if let Some(text) = leaf_text(child) {
            self.emit(self.path.len(), RefSiteShape::Hyperlink, text, None);
        }
    }

    /// `child`, whose own field path is `self.path` and whose container's is
    /// `self.path[..prefix_len]`.
    fn field(&mut self, child: Node, prefix_len: usize, depth: usize) {
        if child.children().any(|c| c.is_element()) {
            self.container(child, depth + 1);
            return;
        }
        let Some(text) = leaf_text(child) else {
            return;
        };
        let tag = child.tag_name().name();
        if tag == "li" {
            self.emit(self.path.len(), RefSiteShape::ListItem, text, Some(child));
            return;
        }
        self.emit(self.path.len(), RefSiteShape::Scalar, text, None);
        if *self.budget < MAX_REF_SITES_PER_FILE {
            self.emit(prefix_len, RefSiteShape::KeyedElement, tag, Some(child));
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
