//! [`ManifestOrder`]: the raw, not-yet-resolved load-order hints a mod
//! declares in `About/Manifest.xml` — the community-mod-manager sibling of
//! `About.xml`'s own [`super::DeclaredOrder`].

/// One mod's own `About/Manifest.xml`, unresolved: each list is exactly
/// the raw `<li>` text `extract::manifest_xml::parse` found, in document
/// order. Deliberately `Vec<String>`, not `Vec<ModId>` like
/// [`super::DeclaredOrder`]'s lists — a Manifest.xml entry may name
/// another mod by its display name *or* its packageId, and telling those
/// apart needs the cross-mod `name_map`/`ActiveMods` this type has no
/// access to; see `analysis::edges::manifest_order_edges`, the sole
/// consumer, for the resolution itself. Empty lists for a mod with no
/// `Manifest.xml`, or one whose relevant tag is absent/empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestOrder {
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    /// From `<dependencies>` — becomes `EdgeKind::ModDependency` edges, same
    /// as `about_xml`'s own `modDependencies`. The source XML may be a flat
    /// `<li>` text list or `about_xml`'s nested
    /// `<li><packageId/><displayName/></li>` shape — real installed mods ship
    /// both (see `extract::manifest_xml::dependency_entries`, which handles
    /// both) — so only the *resulting* flat `Vec<String>` here (never a
    /// `displayName` to carry alongside a packageId) is guaranteed, not the
    /// source XML shape.
    pub dependencies: Vec<String>,
}
