//! Indexes `Defs/**/*.xml` files: root `<Defs>`, each child element is one
//! def whose tag name is the def type and whose `<defName>` child gives
//! its name. `Name`-attributed nodes (whether or not `Abstract="True"`)
//! are indexed separately as templates; a node with `Abstract="True"` and
//! no `<defName>` is a template only, never a def.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;

use thiserror::Error;

use crate::domain::{
    DefEntry, RefSite, RefSiteOwner, TemplateEntry, TexturePathCandidate, XmlLocator,
    hash_node_path,
};

use super::patches::{li_predicate_identity, li_predicate_suffix};
use super::ref_sites;
use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, collect_class_strings, csv_attr, decode_lossy, direct_child,
    raw_element_nesting_exceeds,
};

#[derive(Debug, Error)]
pub enum DefsError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <Defs>")]
    WrongRoot,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// One `Defs/**/*.xml` file's indexed content: every concrete def plus
/// every `Name`-attributed template node. A single parse produces both —
/// a concrete def that also carries a `Name` attribute lands in both
/// lists, since RimWorld allows a def to be its own children's template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefsFile {
    pub defs: Vec<DefEntry>,
    pub templates: Vec<TemplateEntry>,
    /// Every fully-qualified type string named anywhere in this file (an
    /// element `Class` attribute or a `*Class` element's text — see
    /// [`collect_class_strings`]), across every def and template alike. Used to
    /// tell whether a patch-injected type pre-exists inline somewhere, which
    /// demotes what would otherwise be a `PatchInjectedNode` edge to advisory
    /// `UsesType`.
    pub inline_types: BTreeSet<String>,
    /// A hash of `"{def_type}/{def_name}/{relative_path}"` for every descendant
    /// *element* of every concrete, named def in this file (the class-string
    /// check above answers a *different* question and can't stand in for this
    /// one), plus `"{def_type}/@{name}/{relative_path}"` so a `[@Name="…"]`
    /// target can be answered too, for every descendant element of every
    /// `Name`-attributed template element in this file (abstract or not — a
    /// template's own literal inline nodes, never its inherited ones, see
    /// `index`'s own comment at the call site), `relative_path` being the
    /// `/`-joined tag names down to that descendant, bounded to
    /// [`MAX_INLINE_NODE_PATH_DEPTH`]. Tells whether a specific XML node (not
    /// just a type string) already exists inline under a specific def or
    /// template — the check the path-shape injected-node pass needs to tell a
    /// patch-injected element-shape node that could only exist because of the
    /// patch from one that already ships inline. Hashed, not the path text
    /// itself — see [`hash_node_path`]'s own doc comment for the memory
    /// rationale and the (safe-direction) consequence of a collision.
    pub inline_node_path_hashes: HashSet<u64>,
    /// A hash of `"{def_type}/{def_name}/{relative_path}={text}"` for every
    /// **leaf** descendant (no child *elements* of its own) of every concrete,
    /// named def in this file, at relative depth 1 or 2 only (`race` and
    /// `race/intelligence`, never a third level) — the exact two shapes
    /// [`super::xpath_expr::Predicate::ChildText`]/
    /// [`super::xpath_expr::Predicate::NestedChildText`] can test, now asked in
    /// **head** position
    /// (`ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]`, a
    /// real install's own badge-fork gap) rather than only as a root/step
    /// predicate on an already-identified def. A `Name`-attributed template's
    /// own literal nodes are included too, under the same
    /// `"{def_type}/@{name}"` prefix [`Self::inline_node_path_hashes`] already
    /// uses (never its inherited ones — same rationale as that field). Hashed,
    /// not the path/value text itself, for the identical memory reason
    /// [`hash_node_path`] documents — this index only ever answers a
    /// **membership test against one already-named candidate def**
    /// (`analysis::edges::child_value_targets` enumerates every def of the
    /// head's own type from `SourceIndex::owners_by_def`, then tests each by
    /// name), never an enumeration of the hash set itself, so a collision's
    /// only possible consequence is a false membership hit — the *unsafe*
    /// direction for this particular consumer (it decides which defs an
    /// operation is indexed against), unlike
    /// [`Self::inline_node_path_hashes`]'s own safe-direction collision
    /// consequence. Accepted anyway: a 64-bit hash collision across the handful
    /// of real content-predicate queries this resolves is not a realistic risk,
    /// and the alternative (raw `String` keys) would cost real memory across
    /// every def in a full install for a shape this narrow (it is rare on real
    /// installs).
    pub child_value_hashes: HashSet<u64>,
    /// Every `texPath`/`texPathFemale`/`iconPath`/`uiIconPath` field value on
    /// every concrete def in this file — see [`TexturePathCandidate`]'s own
    /// doc comment.
    pub texture_path_candidates: Vec<TexturePathCandidate>,
    /// A `<Defs>` child that is not `Abstract="True"`, carries no `Name`
    /// attribute, and has no non-empty `<defName>` — real content this
    /// indexer cannot represent at all (neither [`Self::defs`] nor
    /// [`Self::templates`] gets an entry for it), yet RimWorld itself loads
    /// it fine: vanilla's own `Data/Core/Defs/Misc/SongDefs/
    /// Songs_Gameplay.xml` uses this exact shape (a `<SongDef>` with
    /// `<clipPath>`/`<volume>` and no `<defName>`). Counted, not indexed — a
    /// caller with no way to check such a def's own override status must
    /// treat its mere presence as "this mod ships real, unverifiable content"
    /// rather than silently seeing nothing at all (otherwise a
    /// music-expansion mod reads as a false positive for
    /// `rim_session::use_cases::ContributesNothing`).
    pub nameless_def_count: usize,
    /// Every `MayRequire`/`MayRequireAnyOf` id read off a **descendant**
    /// element of a top-level `<Defs>` child — a `<li>` or other
    /// list-item-shaped element nested anywhere inside a def or
    /// template's own field tree — a keyed-dictionary element like
    /// `<need MayRequire="...">Bladder</need>` is honoured exactly like a
    /// plain `<li>`, since both sit in a `List<T>`-shaped field. The
    /// top-level element's own root-level `MayRequire` is **not**
    /// included here — that's [`DefEntry::may_require`]/
    /// [`TemplateEntry::may_require`], a different honoured shape (a def
    /// node directly under `<Defs>`, not a list item). Collected
    /// regardless of whether the descendant field genuinely holds a
    /// `List<T>` or is a bare non-def scalar field the game silently
    /// ignores (`DirectXmlToObject`'s own list/field split isn't
    /// reconstructable from tag shape alone without ECMA-335 field
    /// metadata) — a scalar-field false positive here is disclosed, not
    /// fixed, and costs only a possibly-uninteresting near-miss
    /// suggestion, never a fabricated `Hard` fact.
    pub nested_may_require: Vec<(String, XmlLocator)>,
    /// Every candidate def-name reference site found on a concrete def or
    /// a `Name`-attributed template in this file — see
    /// [`RefSite`]/`analysis::references`. Bounded at
    /// `ref_sites::MAX_REF_SITES_PER_FILE`; see [`Self::ref_sites_truncated`].
    pub ref_sites: Vec<RefSite>,
    /// Whether [`Self::ref_sites`] hit `ref_sites::MAX_REF_SITES_PER_FILE`
    /// and stopped collecting early.
    pub ref_sites_truncated: bool,
}

/// How many `/`-segments deep (relative to the def element) node paths are
/// indexed. Measured sufficiency on a real install: the deepest real
/// `PatchInjectedNode`/`UsesType` subject in the report is depth 2
/// (`apparel/tags`, `structureMemeWeights/ExampleMeme_Structure`,
/// `recipeMaker/researchPrerequisite`), so 3 is deliberate headroom, not a
/// guess. A path deeper than this is treated as unknown by the consumer
/// ([`crate::analysis::edges`]'s `emit_path_injected_node_edge`) and demoted —
/// the safe direction, documented there.
pub const MAX_INLINE_NODE_PATH_DEPTH: usize = 3;

/// How many `/`-segments deep (relative to the def element) a leaf's own text
/// is captured into [`DefsFile::child_value_hashes`] — matches
/// [`super::xpath_expr::Predicate::ChildText`] (one level) and
/// [`super::xpath_expr::Predicate::NestedChildText`] (two), the deepest shape
/// that grammar's "equality against a literal only" scope ever models (in root,
/// step, and head position). A value at depth 3+ is never a shape any caller
/// can query for, so capturing it would only cost memory for no possible
/// reader.
const MAX_CHILD_VALUE_DEPTH: usize = 2;

/// Indexes one `Defs/**/*.xml` file's def and template elements.
/// `file` becomes every produced [`XmlLocator::file`] — shared, never
/// cloned per element.
pub fn index(bytes: &[u8], file: &Arc<Path>) -> Result<DefsFile, DefsError> {
    let text = decode_lossy(bytes);
    // `roxmltree::Document::parse` below is a recursive-descent parser
    // over raw element nesting and can overflow the stack on
    // pathologically deep mod-provided XML before this crate's own
    // post-parse depth caps (`MAX_INLINE_NODE_PATH_DEPTH`,
    // `MAX_NESTED_MAY_REQUIRE_DEPTH`) ever run — see
    // `xml_util::MAX_RAW_ELEMENT_DEPTH`'s own doc comment.
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(DefsError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("Defs") {
        return Err(DefsError::WrongRoot);
    }

    let mut defs = Vec::new();
    let mut templates = Vec::new();
    let mut inline_types = BTreeSet::new();
    let mut inline_node_path_hashes = HashSet::new();
    let mut child_value_hashes = HashSet::new();
    let mut texture_path_candidates = Vec::new();
    let mut nameless_def_count = 0usize;
    let mut nested_may_require = Vec::new();
    let mut ref_sites = Vec::new();
    let mut ref_sites_truncated = false;
    let mut ref_site_budget = 0usize;
    let mut ref_site_text = ref_sites::TextInterner::default();
    collect_class_strings(root, &mut inline_types);

    for (position, element) in root
        .children()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        let locator = XmlLocator::new(Arc::clone(file), vec![position as u32]);
        let def_type = element.tag_name().name().to_string();
        let parent_name = element.attribute("ParentName").map(str::to_string);
        let is_abstract = is_abstract(element);

        // Every descendant's own `MayRequire`/`MayRequireAnyOf` — see
        // `DefsFile::nested_may_require`'s own doc comment. Walked
        // unconditionally (whether or not this top-level element ends up
        // a def, a template, both, or neither) since a nameless def can
        // still carry real, honoured `MayRequire` gates on its own
        // fields.
        collect_nested_may_require(element, &[position as u32], file, &mut nested_may_require);

        if let Some(name) = element.attribute("Name") {
            templates.push(TemplateEntry {
                def_type: def_type.clone(),
                name: name.to_string(),
                parent_name: parent_name.clone(),
                may_require: csv_attr(element, "MayRequire"),
                graphic_class: declared_graphic_class(element),
                is_abstract,
                locator: locator.clone(),
            });
            // A reference found in a template's own field tree is
            // inherited by every concrete descendant — attributed to the
            // template itself, not re-walked per descendant. Independent
            // of the `is_abstract` skip below, same reasoning as the
            // inline-node-path index just above: whether this node's own
            // literal fields carry a reference has nothing to do with
            // whether the template itself is concrete.
            if !ref_sites_truncated {
                let owner = Arc::new(RefSiteOwner::Template {
                    name: name.to_string(),
                    may_require: csv_attr(element, "MayRequire"),
                });
                let mut raw = Vec::new();
                ref_sites::collect(
                    element,
                    &def_type,
                    &mut raw,
                    &mut ref_site_budget,
                    &mut ref_sites_truncated,
                );
                ref_sites.extend(
                    raw.into_iter()
                        .map(|site| wrap_ref_site(site, &owner, &mut ref_site_text)),
                );
            }
            // Without this, a `[@Name="…"]` target's inline-node lookup always
            // reads "absent", wrongly asserting `Hard` edges; see
            // `analysis::edges::emit_path_injected_node_edge`'s own doc
            // comment. Indexed under the same `"{def_type}/@{name}"` prefix
            // `DefTarget::match_key` builds for a `Selector::NameAttr` target —
            // the `@` keeps a template named `X` from colliding with a concrete
            // `defName="X"` def in this same hash set (they're deliberately
            // checked separately: see `pre_exists_inline` in
            // `analysis::edges`). Deliberately placed **before** the
            // `is_abstract` skip below (most real templates, e.g.
            // `MechGestatorBase`, *are* abstract) and independent of it:
            // whether a node ships inline here has nothing to do with whether
            // the template itself is concrete.
            //
            // Deliberately NOT walking inherited (`ParentName`) nodes —
            // RimWorld resolves `ParentName` inheritance *after*
            // patching, so only a template's own literal inline nodes
            // belong in this index; a node the template only inherits
            // isn't "already there" from the patch's point of view.
            let template_base_path = format!("{def_type}/@{name}");
            collect_inline_node_paths(
                element,
                &template_base_path,
                0,
                &mut inline_node_path_hashes,
                &mut child_value_hashes,
            );
        }

        if is_abstract {
            continue;
        }

        let Some(def_name) = direct_child(element, "defName")
            .and_then(|n| n.text())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            // Not abstract, no `<defName>` — real content this indexer
            // can't represent at all unless it's also `Name`-attributed
            // (already indexed as a template above). See
            // `DefsFile::nameless_def_count`'s own doc comment.
            if element.attribute("Name").is_none() {
                nameless_def_count += 1;
            }
            continue;
        };

        let base_path = format!("{def_type}/{def_name}");
        collect_inline_node_paths(
            element,
            &base_path,
            0,
            &mut inline_node_path_hashes,
            &mut child_value_hashes,
        );
        collect_texture_path_candidates(element, &def_type, def_name, &mut texture_path_candidates);
        if !ref_sites_truncated {
            let owner = Arc::new(RefSiteOwner::Def {
                def_name: def_name.to_string(),
                may_require: csv_attr(element, "MayRequire"),
                may_require_any_of: csv_attr(element, "MayRequireAnyOf"),
            });
            let mut raw = Vec::new();
            ref_sites::collect(
                element,
                &def_type,
                &mut raw,
                &mut ref_site_budget,
                &mut ref_sites_truncated,
            );
            ref_sites.extend(
                raw.into_iter()
                    .map(|site| wrap_ref_site(site, &owner, &mut ref_site_text)),
            );
        }

        defs.push(DefEntry {
            def_type,
            def_name: def_name.to_string(),
            may_require: csv_attr(element, "MayRequire"),
            may_require_any_of: csv_attr(element, "MayRequireAnyOf"),
            parent_name,
            locator,
        });
    }

    Ok(DefsFile {
        defs,
        templates,
        inline_types,
        inline_node_path_hashes,
        child_value_hashes,
        texture_path_candidates,
        nameless_def_count,
        nested_may_require,
        ref_sites,
        ref_sites_truncated,
    })
}

/// Attaches `owner` to one [`ref_sites::RawRefSite`], turning it into a
/// full [`RefSite`] — the def/template owner is known at each of
/// [`index`]'s own two call sites, not inside the owner-agnostic shared
/// walk itself. Repeated def types and field paths are shared through
/// `text`, one per file.
fn wrap_ref_site(
    site: ref_sites::RawRefSite,
    owner: &Arc<RefSiteOwner>,
    text: &mut ref_sites::TextInterner,
) -> RefSite {
    RefSite {
        def_type: text.intern(&site.def_type),
        field_path: text.intern(&site.field_path),
        shape: site.shape,
        value: site.value,
        owner: Arc::clone(owner),
        may_require: site.may_require.into_boxed_slice(),
        may_require_any_of: site.may_require_any_of.into_boxed_slice(),
    }
}

/// How many `/`-segments deep (relative to the top-level `<Defs>` child)
/// [`collect_nested_may_require`] descends — generous headroom over
/// [`MAX_INLINE_NODE_PATH_DEPTH`], since a `MayRequire`-gated list item
/// can sit deeper than the shallow content-predicate shapes that bound
/// governs (e.g. `comps/li/props/...`), while still bounding the walk
/// against pathological XML depth.
const MAX_NESTED_MAY_REQUIRE_DEPTH: usize = 16;

/// See [`DefsFile::nested_may_require`]. `path` is `element`'s own
/// ordinal path from the `<Defs>` root, extended by one ordinal per
/// recursion level so every collected id keeps a real [`XmlLocator`]
/// pointing at the exact attribute it came from.
fn collect_nested_may_require(
    element: roxmltree::Node,
    path: &[u32],
    file: &Arc<Path>,
    out: &mut Vec<(String, XmlLocator)>,
) {
    if path.len() > MAX_NESTED_MAY_REQUIRE_DEPTH {
        return;
    }
    for (index, child) in element
        .children()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        let mut child_path = path.to_vec();
        child_path.push(index as u32);
        for id in csv_attr(child, "MayRequire")
            .into_iter()
            .chain(csv_attr(child, "MayRequireAnyOf"))
        {
            out.push((id, XmlLocator::new(Arc::clone(file), child_path.clone())));
        }
        collect_nested_may_require(child, &child_path, file, out);
    }
}

/// Four fixed field names (`texPath`/`texPathFemale`/`iconPath`/`uiIconPath`,
/// exact case, scalar-only, never a `<li>` list) — a closed, engine-level
/// vocabulary that needs no inference at all. A `Def`/`Defs`-suffix
/// structural rule for def references does not work the same way: RimWorld's
/// Def-typed field names are an open, per-mod-extensible vocabulary a
/// tag-suffix guess cannot reliably predict (`soundMeleeHit`,
/// `startingResearchTags`, `researchPrerequisites`, `requiredBuildings` are
/// all real, unsuffixed reference fields), and such a rule also fires on
/// Core/DLC and on boolean-literal fields whose own name happens to end
/// `Def`/`Defs`.
fn collect_texture_path_candidates(
    def: roxmltree::Node,
    def_type: &str,
    def_name: &str,
    texture_paths: &mut Vec<TexturePathCandidate>,
) {
    const TEXTURE_PATH_FIELDS: [&str; 4] = ["texPath", "texPathFemale", "iconPath", "uiIconPath"];

    for element in def.descendants().filter(roxmltree::Node::is_element) {
        let tag = element.tag_name().name();
        if !TEXTURE_PATH_FIELDS.contains(&tag) {
            continue;
        }
        if element.children().any(|c| c.is_element()) {
            continue;
        }
        let Some(value) = element.text().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        texture_paths.push(TexturePathCandidate {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            field: tag.to_string(),
            path: value.to_string(),
            // The `<graphicClass>` sits beside this field in the same
            // `<graphicData>` block; absent for `iconPath`-family fields,
            // for any def whose own code reads the path, and for a def
            // that inherits its graphic class from a `ParentName`
            // template (`analysis::conflicts` resolves that case, which
            // needs the container tag below to know the field is part of
            // a graphic at all).
            graphic_class: element
                .parent()
                .and_then(|parent| direct_child(parent, "graphicClass"))
                .and_then(|node| node.text())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string),
            container_tag: element
                .parent()
                .filter(|parent| parent.is_element())
                .map(|parent| parent.tag_name().name().to_string()),
        });
    }
}

/// The `<graphicData><graphicClass>` text a def or template node declares
/// literally, if any — the shape a `ParentName` child inherits (see
/// [`TemplateEntry::graphic_class`](crate::domain::TemplateEntry::graphic_class)).
fn declared_graphic_class(element: roxmltree::Node) -> Option<String> {
    direct_child(element, "graphicData")
        .and_then(|graphic_data| direct_child(graphic_data, "graphicClass"))
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Records a hash of `"{prefix}/{tag}"` for every descendant element of
/// `node`, recursing down to [`MAX_INLINE_NODE_PATH_DEPTH`] — `prefix`
/// starts as `"{def_type}/{def_name}"` (see [`DefsFile::inline_node_path_hashes`]'s
/// own doc comment) and grows by one tag name per level. Every
/// descendant at every depth up to the bound is recorded, not just leaf
/// elements — a patch-injected path can name an intermediate container
/// node (e.g. `apparel`) just as often as a leaf one (`apparel/tags`).
///
/// An `<li>` with a recognizable identity (`Class` attribute, or bare text)
/// is additionally recorded under `"{prefix}/li[@Class=\"X\"]"` /
/// `"{prefix}/li[text()=\"v\"]"` — the normalized keys the patch side builds —
/// so a predicate-selected list item that a Def already ships reads as
/// present. `MayRequire` is deliberately ignored (no active-mod set here); a
/// gated item counting as present only loses a `Hard` edge.
///
/// Also feeds `child_values`: whenever the descendant being recorded is itself
/// at relative depth 1 or 2 ([`MAX_CHILD_VALUE_DEPTH`]) *and* is a genuine leaf
/// with non-empty text, an additional `"{path}={text}"` hash goes into
/// `child_values` — see [`DefsFile::child_value_hashes`]'s own doc comment for
/// why only leaves, only this shallow, and why hashed rather than kept as text.
fn collect_inline_node_paths(
    node: roxmltree::Node,
    prefix: &str,
    depth: usize,
    out: &mut HashSet<u64>,
    child_values: &mut HashSet<u64>,
) {
    if depth >= MAX_INLINE_NODE_PATH_DEPTH {
        return;
    }
    for child in node.children().filter(roxmltree::Node::is_element) {
        let path = format!("{prefix}/{}", child.tag_name().name());
        out.insert(hash_node_path(&path));
        if child.tag_name().name() == "li"
            && let Some(identity) = li_predicate_identity(child)
        {
            // The patch side keys a predicate-selected list item by this same
            // normalized suffix; without it, a `li[...]` path can never read
            // as already shipping inline.
            out.insert(hash_node_path(&format!(
                "{prefix}/{}",
                li_predicate_suffix(&identity)
            )));
        }
        if depth < MAX_CHILD_VALUE_DEPTH
            && let Some(text) = leaf_text(child)
        {
            child_values.insert(hash_node_path(&format!("{path}={text}")));
        }
        collect_inline_node_paths(child, &path, depth + 1, out, child_values);
    }
}

/// `node`'s own trimmed text, but only when `node` is a genuine leaf — no
/// child *elements* of its own (a comment/PI/whitespace-only text node
/// doesn't disqualify it) — and that text is non-empty. The exact shape
/// [`super::xpath_expr::Predicate::ChildText`]/[`super::xpath_expr::Predicate::NestedChildText`]
/// test against, so a container element (`race`, which has its own child
/// elements) is never recorded as if its own text mattered.
fn leaf_text<'a, 'input>(node: roxmltree::Node<'a, 'input>) -> Option<&'a str> {
    if node.children().any(|c| c.is_element()) {
        return None;
    }
    node.text().map(str::trim).filter(|s| !s.is_empty())
}

fn is_abstract(def: roxmltree::Node) -> bool {
    def.attribute("Abstract")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file() -> Arc<Path> {
        Arc::from(Path::new("test.xml"))
    }

    #[test]
    fn indexes_simple_and_namespaced_def_types() {
        let xml = br#"<Defs>
              <ThingDef><defName>Wall</defName></ThingDef>
              <example.PartDef><defName>Human_PartA</defName></example.PartDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.defs.len(), 2);
        assert_eq!(result.defs[0].def_type, "ThingDef");
        assert_eq!(result.defs[0].def_name, "Wall");
        assert_eq!(result.defs[1].def_type, "example.PartDef");
        assert_eq!(result.defs[1].def_name, "Human_PartA");
    }

    /// Replaces the old "skips_abstract_defs" test: an `Abstract="True"`
    /// node with no `Name` produces no template either (nothing else can
    /// reference it), and never a def.
    #[test]
    fn abstract_node_with_no_name_lands_in_neither_list() {
        let xml = br#"<Defs>
              <ThingDef Abstract="True"><defName>BaseTemplate</defName></ThingDef>
              <ThingDef><defName>Concrete</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.defs.len(), 1);
        assert_eq!(result.defs[0].def_name, "Concrete");
        assert!(result.templates.is_empty());
    }

    /// The actual real-world shape: `Abstract="True" Name="..."` nodes
    /// land in `templates`, carrying their `parent_name`, and never in
    /// `defs` (no `<defName>` at all in this fixture).
    #[test]
    fn named_abstract_nodes_land_in_templates_with_parent_name() {
        let xml = br#"<Defs>
              <HediffDef Name="ImplantHediffBase" Abstract="True">
                <hediffClass>Hediff_Implant</hediffClass>
              </HediffDef>
              <HediffDef Name="AddedBodyPartBase" ParentName="ImplantHediffBase" Abstract="True">
                <hediffClass>Hediff_AddedPart</hediffClass>
              </HediffDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.defs.is_empty());
        assert_eq!(result.templates.len(), 2);
        assert_eq!(result.templates[0].name, "ImplantHediffBase");
        assert!(result.templates[0].is_abstract);
        assert_eq!(result.templates[0].parent_name, None);
        assert_eq!(result.templates[1].name, "AddedBodyPartBase");
        assert_eq!(
            result.templates[1].parent_name.as_deref(),
            Some("ImplantHediffBase")
        );
    }

    /// A concrete, non-abstract def that also carries a `Name` attribute
    /// is its own children's template *and* a def in its own right —
    /// RimWorld allows this, so one parse must produce it in both lists.
    #[test]
    fn concrete_def_with_a_name_attribute_appears_in_both_lists() {
        let xml = br#"<Defs>
              <ThingDef Name="WallBase"><defName>Wall</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.defs.len(), 1);
        assert_eq!(result.defs[0].def_name, "Wall");
        assert_eq!(result.templates.len(), 1);
        assert_eq!(result.templates[0].name, "WallBase");
        assert!(!result.templates[0].is_abstract);
        assert_eq!(result.defs[0].locator, result.templates[0].locator);
    }

    #[test]
    fn skips_defs_with_no_def_name() {
        let xml = br#"<Defs><ThingDef><label>no def name here</label></ThingDef></Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.defs.is_empty());
    }

    /// `inline_types` collects `Class` attributes and `*Class` element text
    /// from anywhere in the file, def and template alike.
    #[test]
    fn inline_types_collects_class_strings_across_the_whole_file() {
        let xml = br#"<Defs>
              <ThingDef><defName>Wall</defName><modExtensions>
                <li Class="Example.Weapons.HeavyWeapon"/>
              </modExtensions></ThingDef>
              <HediffDef Name="Base" Abstract="True">
                <hediffClass>Hediff_Implant</hediffClass>
              </HediffDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(
            result.inline_types,
            BTreeSet::from([
                "Example.Weapons.HeavyWeapon".to_string(),
                "Hediff_Implant".to_string(),
            ])
        );
    }

    /// Every descendant element of a def, at every depth up to the bound, is
    /// hashed and recorded — a container element (`ingestible`) as much as a
    /// leaf one nested inside it (`ingestible/foodType`), since a
    /// patch-injected path can name either shape.
    #[test]
    fn inline_node_path_hashes_records_every_descendant_up_to_the_depth_bound() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>MealNutrientPaste</defName>
                <ingestible>
                  <foodType>Processed</foodType>
                </ingestible>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/MealNutrientPaste/ingestible"))
        );
        assert!(result.inline_node_path_hashes.contains(&hash_node_path(
            "ThingDef/MealNutrientPaste/ingestible/foodType"
        )));
    }

    /// An inline `<li>` is recorded under the normalized predicate keys the
    /// patch side builds (`li[text()="v"]`, `li[@Class="X"]`), alongside the
    /// bare `li` path; a container-only `<li>` with no `Class` has no key.
    #[test]
    fn inline_node_path_hashes_records_predicate_keys_for_list_items() {
        let xml = br#"<Defs>
              <XenotypeDef>
                <defName>A</defName>
                <genes><li>GeneX</li><li>  Padded </li></genes>
                <comps><li Class="Example.CompX"><a>1</a></li><li><a>2</a></li></comps>
              </XenotypeDef>
            </Defs>"#;
        let hashes = index(xml, &test_file()).unwrap().inline_node_path_hashes;
        for key in [
            "XenotypeDef/A/genes/li",
            r#"XenotypeDef/A/genes/li[text()="GeneX"]"#,
            r#"XenotypeDef/A/comps/li[@Class="Example.CompX"]"#,
        ] {
            assert!(hashes.contains(&hash_node_path(key)), "missing {key}");
        }
        // The game's `text()="X"` compares the text as written: padded text
        // is keyed verbatim, never under the trimmed value.
        assert!(hashes.contains(&hash_node_path(
            "XenotypeDef/A/genes/li[text()=\"  Padded \"]"
        )));
        assert!(!hashes.contains(&hash_node_path(
            r#"XenotypeDef/A/genes/li[text()="Padded"]"#
        )));
        assert_eq!(
            hashes.len(),
            // defName, genes, genes/li, genes/li[text()=GeneX],
            // genes/li[text()=padded], comps, comps/li, comps/li[@Class],
            // comps/li/a
            9,
            "a keyless <li> must add no predicate entry"
        );
    }

    /// A template's own `<li>` gets the predicate key under the `@` prefix.
    #[test]
    fn inline_node_path_hashes_records_predicate_keys_under_templates() {
        let xml = br#"<Defs>
              <ThingDef Name="BedBase" Abstract="True">
                <comps><li Class="Example.CompX"/></comps>
              </ThingDef>
            </Defs>"#;
        let hashes = index(xml, &test_file()).unwrap().inline_node_path_hashes;
        assert!(hashes.contains(&hash_node_path(
            r#"ThingDef/@BedBase/comps/li[@Class="Example.CompX"]"#
        )));
    }

    /// A path deeper than [`MAX_INLINE_NODE_PATH_DEPTH`] is never
    /// recorded at all — the consumer ([`crate::analysis::edges`]'s
    /// `emit_path_injected_node_edge`) treats an unindexed depth as
    /// unknown and demotes, rather than trusting a lookup miss here as
    /// proof the node doesn't exist.
    #[test]
    fn inline_node_path_hashes_is_bounded_to_the_max_depth() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Wall</defName>
                <a><b><c><d>too deep</d></c></b></a>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/a"))
        );
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/a/b"))
        );
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/a/b/c"))
        );
        assert!(
            !result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/a/b/c/d")),
            "depth 4 must not be recorded — MAX_INLINE_NODE_PATH_DEPTH is 3"
        );
    }

    /// A `Name`-attributed template's own inline nodes must be indexed too,
    /// under `"{def_type}/@{name}"` — the same `@`-prefix convention
    /// `DefTarget::match_key` uses — regardless of `Abstract`, since most real
    /// templates are abstract. Modeled on the real
    /// `ThingDef[@Name="MechGestatorBase"]` subject
    /// (`Data/Biotech/Defs/ThingDefs_Buildings/Buildings_Production.xml`):
    /// without the template index, every such lookup reads "absent" and wrongly
    /// asserts a `Hard` `PatchInjectedNode` edge.
    #[test]
    fn inline_node_path_hashes_records_a_name_attributed_templates_own_inline_nodes() {
        let xml = br#"<Defs>
              <ThingDef Name="MechGestatorBase" Abstract="True">
                <comps>
                  <li Class="CompProperties_Gestator"/>
                </comps>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/@MechGestatorBase/comps"))
        );
    }

    /// The `@`-prefix keeps a `Name`-attributed template's inline-node index
    /// entries from colliding with a concrete `defName`-bearing def of the
    /// identical literal name — a template named `Wall` shipping `<comps>`
    /// inline must not make a *different*, concrete `ThingDef[defName="Wall"]`
    /// read as "ships `<comps>` inline" too.
    #[test]
    fn name_attr_and_def_name_inline_node_paths_occupy_disjoint_key_spaces() {
        let xml = br#"<Defs>
              <ThingDef Name="Wall" Abstract="True">
                <comps><li Class="SomeComp"/></comps>
              </ThingDef>
              <ThingDef><defName>Wall</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/@Wall/comps"))
        );
        assert!(
            !result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/comps")),
            "the concrete def never actually ships <comps> inline — only the decoy template does"
        );
    }

    // -- child_value_hashes ----------------------------------------------

    /// A real install's own badge-fork shape: a two-step relative path
    /// (`race/intelligence`) leaf value is recorded, hashed the same way
    /// `analysis::edges::child_value_targets` will re-derive it at query
    /// time (`"{def_type}/{def_name}/{path}={value}"`).
    #[test]
    fn child_value_hashes_records_a_two_step_leaf_value() {
        let xml = br#"<Defs>
              <ExampleRace.ThingDef_ExampleRace>
                <defName>Human</defName>
                <race>
                  <intelligence>Humanlike</intelligence>
                </race>
              </ExampleRace.ThingDef_ExampleRace>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.child_value_hashes.contains(&hash_node_path(
            "ExampleRace.ThingDef_ExampleRace/Human/race/intelligence=Humanlike"
        )));
    }

    /// The single-step form (`Predicate::ChildText`'s own shape) is
    /// recorded too, at relative depth 1.
    #[test]
    fn child_value_hashes_records_a_single_step_leaf_value() {
        let xml = br#"<Defs>
              <RoomRequirementDef>
                <defName>X</defName>
                <thingDef>Column</thingDef>
              </RoomRequirementDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .child_value_hashes
                .contains(&hash_node_path("RoomRequirementDef/X/thingDef=Column"))
        );
    }

    /// `leaf_text` refuses a container element outright — `<race>` has a
    /// child element, so it is never treated as a leaf, whatever
    /// roxmltree's own `.text()` would otherwise report for it (mixed
    /// content is not a shape this grammar's `ChildText`/`NestedChildText`
    /// predicates model at all).
    #[test]
    fn leaf_text_refuses_a_container_element() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Human</defName>
                <race><intelligence>Humanlike</intelligence></race>
              </ThingDef>
            </Defs>"#;
        let doc = roxmltree::Document::parse(std::str::from_utf8(xml).unwrap()).unwrap();
        let race = doc
            .descendants()
            .find(|n| n.tag_name().name() == "race")
            .unwrap();
        assert_eq!(leaf_text(race), None);
        let intelligence = doc
            .descendants()
            .find(|n| n.tag_name().name() == "intelligence")
            .unwrap();
        assert_eq!(leaf_text(intelligence), Some("Humanlike"));
    }

    /// A value at relative depth 3 is outside [`MAX_CHILD_VALUE_DEPTH`]
    /// and must not be recorded, even though the *path* itself is still
    /// within [`MAX_INLINE_NODE_PATH_DEPTH`] and so still lands in
    /// `inline_node_path_hashes`.
    #[test]
    fn child_value_hashes_is_bounded_to_two_levels() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Wall</defName>
                <a><b><c>too deep</c></b></a>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .inline_node_path_hashes
                .contains(&hash_node_path("ThingDef/Wall/a/b/c")),
            "the path itself is within MAX_INLINE_NODE_PATH_DEPTH"
        );
        assert!(
            !result
                .child_value_hashes
                .contains(&hash_node_path("ThingDef/Wall/a/b/c=too deep")),
            "depth 3 is outside MAX_CHILD_VALUE_DEPTH (2)"
        );
    }

    /// A `Name`-attributed template's own literal leaf values are indexed
    /// too, under the same `@`-prefixed key space
    /// [`DefsFile::inline_node_path_hashes`] already uses — never
    /// confusable with a concrete def of the same literal name.
    #[test]
    fn child_value_hashes_records_a_templates_own_leaf_values_under_the_at_prefix() {
        let xml = br#"<Defs>
              <ExampleRace.ThingDef_ExampleRace Name="RaceBase" Abstract="True">
                <race><intelligence>Humanlike</intelligence></race>
              </ExampleRace.ThingDef_ExampleRace>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.child_value_hashes.contains(&hash_node_path(
            "ExampleRace.ThingDef_ExampleRace/@RaceBase/race/intelligence=Humanlike"
        )));
    }

    // -- collect_texture_path_candidates ----------------------------------

    /// `texPath` and its three siblings are fixed field names, no inference
    /// needed, unlike a def-suffix guess for def references (see
    /// `collect_texture_path_candidates`'s own doc comment).
    #[test]
    fn texture_path_candidates_finds_the_four_known_fields() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Colonist</defName>
                <texPath>Things/Pawn/Colonist</texPath>
                <texPathFemale>Things/Pawn/ColonistFemale</texPathFemale>
                <iconPath>UI/Icons/Colonist</iconPath>
                <uiIconPath>UI/Icons/ColonistUi</uiIconPath>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.texture_path_candidates.len(), 4);
        assert!(
            result
                .texture_path_candidates
                .iter()
                .any(|c| c.field == "texPath" && c.path == "Things/Pawn/Colonist")
        );
    }

    /// A field that merely *contains* one of the four known names, or
    /// differs in case, is not a match — exact tag text only.
    #[test]
    fn texture_path_candidates_requires_an_exact_field_name() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Colonist</defName>
                <texpath>Things/Pawn/Colonist</texpath>
                <secondaryTexPath>Things/Pawn/Other</secondaryTexPath>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.texture_path_candidates.is_empty());
    }

    /// An abstract template contributes no candidates — extraction is
    /// scoped to concrete defs only.
    #[test]
    fn texture_path_candidates_skips_abstract_templates() {
        let xml = br#"<Defs>
              <ThingDef Name="Base" Abstract="True">
                <texPath>Things/Base</texPath>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(result.texture_path_candidates.is_empty());
    }

    /// A `texPath`-family field nested several levels deep is still
    /// found — the walk covers the whole subtree, not just direct
    /// children.
    #[test]
    fn texture_path_candidates_finds_a_nested_field() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Colonist</defName>
                <comps>
                  <li Class="CompProperties_Something">
                    <texPath>Things/Nested</texPath>
                  </li>
                </comps>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert!(
            result
                .texture_path_candidates
                .iter()
                .any(|c| c.field == "texPath" && c.path == "Things/Nested")
        );
    }

    /// The `<graphicClass>` beside a `texPath` decides whether
    /// `Verse.Graphic_Collection.Init` is what resolves the path, which is the
    /// only case the strict direct-child folder rule applies to
    /// (`analysis::conflicts::texture_path_resolves`).
    #[test]
    fn texture_path_candidates_carry_the_sibling_graphic_class() {
        let xml = br#"<Defs>
              <ThingDef>
                <defName>Plant</defName>
                <graphicData>
                  <texPath>Plant/Sigillaria</texPath>
                  <graphicClass>Graphic_Random</graphicClass>
                </graphicData>
                <uiIconPath>UI/Icon</uiIconPath>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();

        let tex = result
            .texture_path_candidates
            .iter()
            .find(|c| c.field == "texPath")
            .expect("texPath candidate");
        assert_eq!(tex.graphic_class.as_deref(), Some("Graphic_Random"));
        let icon = result
            .texture_path_candidates
            .iter()
            .find(|c| c.field == "uiIconPath")
            .expect("uiIconPath candidate");
        assert_eq!(
            icon.graphic_class, None,
            "an icon field has no graphicClass sibling"
        );
    }

    #[test]
    fn wrong_root_is_an_error() {
        let xml = br#"<NotDefs/>"#;
        assert!(matches!(
            index(xml, &test_file()),
            Err(DefsError::WrongRoot)
        ));
    }

    /// The actual reported attack shape — a ~300 KB Workshop `Defs/*.xml`
    /// file with 50,000 nested elements — must never even reach
    /// `roxmltree::Document::parse`, whose own recursive-descent parser
    /// overflows a 1 MiB stack well before it would ever return this
    /// function's ordinary parse-failure branch. Run on a 1 MiB stack (the
    /// CLI's own main thread size); without the pre-parse guard, this test
    /// aborts the whole test process rather than failing an assertion.
    #[test]
    fn pathologically_deep_defs_xml_is_rejected_before_parsing_not_crashed() {
        let mut inner = "<defName>X</defName>".to_string();
        for _ in 0..50_000 {
            inner = format!("<a>{inner}</a>");
        }
        let xml = format!("<Defs><ThingDef>{inner}</ThingDef></Defs>");

        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let err = match index(xml.as_bytes(), &test_file()) {
                    Err(err) => err,
                    Ok(_) => panic!("50,000 levels of nesting must be refused, not parsed"),
                };
                assert!(
                    matches!(err, DefsError::TooDeep(depth) if depth == MAX_RAW_ELEMENT_DEPTH),
                    "expected TooDeep({MAX_RAW_ELEMENT_DEPTH}), got {err:?}"
                );
            })
            .expect("spawning the probe thread")
            .join()
            .expect(
                "a pathologically deep Defs/*.xml file overflowed a 1 MiB stack instead of \
                 being rejected cleanly",
            );
    }

    /// `XmlInheritance.TryRegister` reads the `MayRequire` attribute off the
    /// template node itself and registers nothing when it is unsatisfied, so
    /// the gate has to reach `TemplateEntry` and not only `DefEntry`.
    #[test]
    fn parses_may_require_on_a_template_node() {
        let xml = br#"<Defs>
              <ThingDef Name="GatedBase" Abstract="True" MayRequire="a.b, c.d" />
              <ThingDef Name="PlainBase" Abstract="True" />
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();

        assert_eq!(
            result.templates[0].may_require,
            vec!["a.b".to_string(), "c.d".to_string()]
        );
        assert!(result.templates[1].may_require.is_empty());
    }

    #[test]
    fn parses_may_require_and_may_require_any_of() {
        let xml = br#"<Defs>
              <ThingDef MayRequire="a.b, c.d"><defName>Gated</defName></ThingDef>
              <ThingDef MayRequireAnyOf="e.f,g.h"><defName>AnyOfGated</defName></ThingDef>
              <ThingDef><defName>Ungated</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(
            result.defs[0].may_require,
            vec!["a.b".to_string(), "c.d".to_string()]
        );
        assert!(result.defs[0].may_require_any_of.is_empty());
        assert_eq!(
            result.defs[1].may_require_any_of,
            vec!["e.f".to_string(), "g.h".to_string()]
        );
        assert!(result.defs[2].may_require.is_empty());
        assert!(result.defs[2].may_require_any_of.is_empty());
    }

    #[test]
    fn parses_parent_name() {
        let xml = br#"<Defs>
              <ThingDef ParentName="WallBase"><defName>Wall</defName></ThingDef>
              <ThingDef><defName>Standalone</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.defs[0].parent_name.as_deref(), Some("WallBase"));
        assert_eq!(result.defs[1].parent_name, None);
    }

    /// The golden fixture (a trimmed copy of two real Core templates)
    /// indexes as two templates with the documented `ParentName` chain —
    /// the real-world shape `TemplateEntry` exists for, not just the
    /// hand-rolled fixtures above.
    #[test]
    fn golden_fixture_indexes_the_core_hediff_template_chain() {
        let xml = include_bytes!("../../tests/fixtures/xml/core_hediff_bases.xml");
        let result = index(xml, &test_file()).unwrap();

        assert!(result.defs.is_empty());
        assert_eq!(result.templates.len(), 2);

        let base = &result.templates[0];
        assert_eq!(base.def_type, "HediffDef");
        assert_eq!(base.name, "ImplantHediffBase");
        assert_eq!(base.parent_name, None);
        assert!(base.is_abstract);

        let added_body_part = &result.templates[1];
        assert_eq!(added_body_part.name, "AddedBodyPartBase");
        assert_eq!(
            added_body_part.parent_name.as_deref(),
            Some("ImplantHediffBase")
        );
        assert!(added_body_part.is_abstract);
    }

    /// `XmlLocator` round trip: re-parsing the same file bytes and walking
    /// the element path a locator recorded must land back on an element
    /// with the same tag and `defName` — the whole point of ordinal paths
    /// over byte offsets.
    #[test]
    fn locator_round_trips_to_the_same_element() {
        let xml = br#"<Defs>
              <ThingDef><defName>Wall</defName></ThingDef>
              <ThingDef><defName>Door</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        let locator = &result.defs[1].locator;
        assert_eq!(locator.element_path, vec![1]);

        // Re-parse the same bytes and walk the recorded element path back
        // down from the root, exactly as a real reader would.
        let text = decode_lossy(xml);
        let doc = roxmltree::Document::parse(&text).unwrap();
        let mut node = doc.root_element();
        for &index in &locator.element_path {
            node = node
                .children()
                .filter(roxmltree::Node::is_element)
                .nth(index as usize)
                .unwrap();
        }
        assert_eq!(node.tag_name().name(), "ThingDef");
        assert_eq!(
            direct_child(node, "defName").and_then(|n| n.text()),
            Some("Door")
        );
    }

    /// The round trip must hold even when the file has a leading UTF-8
    /// BOM and an invalid UTF-8 byte inside an *earlier* def's text —
    /// `decode_lossy`'s BOM-strip and lossy replacement change byte
    /// offsets and text content, but never which ordinal position an
    /// element sits at among its parent's children.
    #[test]
    fn locator_round_trips_with_a_bom_and_an_invalid_utf8_byte_before_the_target() {
        let mut xml = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        xml.extend_from_slice(b"<Defs><ThingDef><defName>Ju");
        xml.push(0xFF); // invalid UTF-8 byte, inside the first def's text
        xml.extend_from_slice(
            b"nk</defName></ThingDef><ThingDef><defName>Target</defName></ThingDef></Defs>",
        );

        let result = index(&xml, &test_file()).unwrap();
        assert_eq!(result.defs.len(), 2);
        let locator = &result.defs[1].locator;
        assert_eq!(locator.element_path, vec![1]);

        let text = decode_lossy(&xml);
        let doc = roxmltree::Document::parse(&text).unwrap();
        let mut node = doc.root_element();
        for &index in &locator.element_path {
            node = node
                .children()
                .filter(roxmltree::Node::is_element)
                .nth(index as usize)
                .unwrap();
        }
        assert_eq!(node.tag_name().name(), "ThingDef");
        assert_eq!(
            direct_child(node, "defName").and_then(|n| n.text()),
            Some("Target")
        );
    }

    /// The real-world gap this collector closes: a keyed list-item element
    /// nested deep inside a def's own field tree (`<need MayRequire="…">`,
    /// not a plain `<li>`) still gates the same way — the game reads
    /// `MayRequire` off the element regardless of its tag name.
    #[test]
    fn nested_may_require_is_collected_off_a_keyed_list_item() {
        let xml = br#"<Defs>
              <GeneDef>
                <defName>Example_Gene</defName>
                <needsDefs>
                  <need MayRequire="some.other.mod">Bladder</need>
                </needsDefs>
              </GeneDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(
            result.nested_may_require,
            vec![(
                "some.other.mod".to_string(),
                // [0, 1, 0]: GeneDef (0), then needsDefs — the second
                // child element after <defName> (1), then need (0).
                XmlLocator::new(test_file(), vec![0, 1, 0])
            )]
        );
    }

    /// A root-level `MayRequire` on the top-level def element itself is a
    /// different, already-honoured shape ([`DefEntry::may_require`]) and
    /// must not be double-counted here.
    #[test]
    fn root_level_may_require_is_not_duplicated_into_nested_may_require() {
        let xml = br#"<Defs>
              <ThingDef MayRequire="root.mod"><defName>Wall</defName></ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(result.defs[0].may_require, vec!["root.mod".to_string()]);
        assert!(result.nested_may_require.is_empty());
    }

    /// `MayRequireAnyOf` is collected the same way as `MayRequire`, and a
    /// comma-separated value expands into one entry per id (`csv_attr`'s
    /// own behavior, exercised here at the nested level).
    #[test]
    fn nested_may_require_any_of_expands_comma_separated_ids() {
        let xml = br#"<Defs>
              <ThingDef><defName>Wall</defName>
                <comps>
                  <li MayRequireAnyOf="mod.a,mod.b"/>
                </comps>
              </ThingDef>
            </Defs>"#;
        let result = index(xml, &test_file()).unwrap();
        assert_eq!(
            result.nested_may_require,
            vec![
                // [0, 1, 0]: ThingDef (0), then comps — the second child
                // element after <defName> (1), then li (0).
                (
                    "mod.a".to_string(),
                    XmlLocator::new(test_file(), vec![0, 1, 0])
                ),
                (
                    "mod.b".to_string(),
                    XmlLocator::new(test_file(), vec![0, 1, 0])
                ),
            ]
        );
    }
}
