//! [`TexturePathCandidate`]: one texture-path field value on one concrete
//! def.

/// One of the four fixed texture-path field names
/// (`texPath`/`texPathFemale`/`iconPath`/`uiIconPath` — a fixed field-name
/// list, not an inferred one) on one concrete def. `def_type`/`def_name`
/// identify the def the field lives on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TexturePathCandidate {
    pub def_type: String,
    pub def_name: String,
    pub field: String,
    /// The raw path text as written in XML — normalized the same way
    /// `extract::textures::normalize` normalizes a shipped file path
    /// (backslashes, case) before this is matched against
    /// `Indices.texture_owners`.
    pub path: String,
    /// The `<graphicClass>` named alongside this field in the same
    /// `<graphicData>` block, verbatim (`Graphic_Random`, or a
    /// fully-qualified `Verse.Graphic_Random`/a mod's own type) — `None`
    /// when the field has no such sibling, which is every `iconPath`/
    /// `uiIconPath` and any `texPath` a mod's own code reads directly.
    ///
    /// Needed because the "folder container" resolution rule
    /// (`analysis::conflicts::texture_path_resolves`) is only as strict as
    /// `Verse.Graphic_Collection.Init` is, and that code runs for the
    /// collection graphic classes alone. A `texPath` some mod's own loader
    /// resolves — Example Animation's `HeadTypeDef`/`BrowTypeDef` are the
    /// real-install case, 227 of them — is free to organize its folder
    /// however it likes.
    pub graphic_class: Option<String>,
    /// The tag of the element this field sits directly inside —
    /// `graphicData` for the `texPath` of a `ThingDef`'s graphic, the def
    /// type itself for a `texPath` a mod's own def type reads, `li` for
    /// one nested in a comp list.
    ///
    /// Only a field inside a `<graphicData>` block can inherit its
    /// `graphicClass` from a `ParentName` template, so this is what keeps
    /// `analysis::conflicts`' chain walk from attaching a template's
    /// graphic class to an unrelated `texPath` elsewhere in the same def.
    pub container_tag: Option<String>,
}
