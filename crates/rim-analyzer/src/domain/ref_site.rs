//! [`RefSite`]: one candidate def-name reference recorded during the
//! `Defs/`/`Patches/` scan — the raw material `analysis::references`'
//! vote-based dangling-reference check turns into a
//! [`super::conflict::DanglingDefReference`].

use std::sync::Arc;

use super::locator::XmlLocator;
use super::mod_id::ModId;

/// The minimum number of distinct *resolved* values a `(def_type, field
/// path)` needs before it's trusted as a reference field.
///
/// Shared with `rim-resolve`'s assignment-schema vote
/// (`rim_resolve::domain::assignment::MIN_RESOLVED_DISTINCT` re-exports
/// this exact constant rather than keeping a second copy): the crate
/// graph is `rim-analyzer` -> `rim-resolve`, so the shared floor has to
/// live at the lower layer — the same pattern
/// `extract::patches::toggle_default` already established for a rule
/// both `rim-analyzer` and `rim-merge` need.
pub const MIN_RESOLVED_DISTINCT: usize = 5;

/// How a [`RefSite`]'s value was written — see `analysis::references`'
/// own doc comment for the full reference-site inference rule each shape
/// feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefSiteShape {
    /// A `<li>` list item's own leaf text.
    ListItem,
    /// A scalar leaf field's own text (never inside a `<li>` — a `<li>`
    /// carrying its own scalar children is `ListItem`'s job, one level
    /// up, not this shape's).
    Scalar,
    /// A keyed-dictionary child element inside a non-`li` container: the
    /// element's own *tag name* is the reference (`<statOffsets><MoveSpeed>0.1</MoveSpeed></statOffsets>`,
    /// `<costList><Steel>10</Steel></costList>`).
    KeyedElement,
    /// A `<descriptionHyperlinks>` child: the element's own tag names the
    /// target's def *type* (not recorded: resolution ignores types), and
    /// its text is the def *name*. Confirmed engine-typed and
    /// data-independent (`DescriptionHyperlink`'s own XML loader), so this
    /// shape needs no vote at all — every occurrence is a reference.
    Hyperlink,
}

/// What a [`RefSite`] belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefSiteOwner {
    /// A concrete def's own field tree.
    Def {
        def_name: String,
        /// The def's own top-level `MayRequire` — whether *this def*
        /// loads at all (distinct from [`RefSite::may_require`], the
        /// site's own gate, which only a `ListItem`/`KeyedElement` site
        /// carries).
        may_require: Vec<String>,
        may_require_any_of: Vec<String>,
    },
    /// A `Name`-attributed template's own field tree — a reference found
    /// here is inherited by every concrete descendant, so
    /// `analysis::references` attributes it to the template once rather
    /// than separately to each descendant.
    Template {
        name: String,
        may_require: Vec<String>,
    },
    /// An active mutating patch op's own `<value>`, at the def path the
    /// value lands on.
    Patch {
        mod_id: ModId,
        def_name: String,
        locator: XmlLocator,
    },
}

/// One candidate reference-site value found on a def, template, or patch
/// op's own `<value>` — see `analysis::references` for how these become a
/// vote and, ultimately, a [`super::conflict::DanglingDefReference`].
///
/// A real install records millions of these (every leaf of every def,
/// twice for a non-`li` leaf — see `extract::ref_sites::collect`), so the
/// text that repeats across sites — the def type, the field path, and the
/// owner every site of one def/template/op shares — is held behind an
/// [`Arc`], one allocation per distinct value rather than one per site.
/// A site carries no locator of its own: nothing reads one, and at this
/// count it cost a fifth of a real-install scan's peak memory. Its owner
/// already locates it to the def, template, or op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSite {
    /// The owning def/template's own element tag (`ThingDef`, ...), for
    /// every shape.
    pub def_type: Arc<str>,
    /// Field path from the owning def/template/patch-value's own root,
    /// with every `li` segment collapsed to the literal string `"li"` so
    /// every list occurrence votes under one path — the vote key
    /// alongside `def_type`.
    pub field_path: Arc<str>,
    pub shape: RefSiteShape,
    pub value: String,
    pub owner: Arc<RefSiteOwner>,
    /// `MayRequire`/`MayRequireAnyOf` ids read directly off this site's
    /// own element. Only [`RefSiteShape::ListItem`]/[`RefSiteShape::KeyedElement`]
    /// ever carry one — the shapes `Verse.DirectXmlToObject.ListFromXml`
    /// actually reads the attribute on; always empty for
    /// [`RefSiteShape::Scalar`]/[`RefSiteShape::Hyperlink`] — a boxed
    /// slice rather than a `Vec`, since nearly every site's is empty and
    /// the spare capacity field alone adds up across millions of sites.
    pub may_require: Box<[String]>,
    pub may_require_any_of: Box<[String]>,
}
