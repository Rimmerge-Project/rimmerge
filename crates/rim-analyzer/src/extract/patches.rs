//! Walks a `Patches/**/*.xml` file's `<Patch>` operation tree into a flat
//! list of [`PatchOp`], threading `PatchOperationFindMod` mod-name context
//! down through `<match>` branches as it goes, and recording each op's
//! [`XmlLocator`] as it descends.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use roxmltree::Node;
use thiserror::Error;

use crate::domain::{
    ConditionalBranch, DefTarget, FindModGate, MAX_VALUE_DIGEST_DEPTH, MAX_VALUE_DIGEST_ENTRIES,
    PatchOp, RefSiteShape, ValueDigest, XmlLocator,
};

use super::ref_sites;
use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, child_text, collect_class_strings, csv_attr, decode_lossy, direct_child,
    direct_element_children, raw_element_nesting_exceeds, strings_from_container,
};
use super::xpath_target;

/// One candidate def-name reference found in an active mutating op's own
/// `<value>` — see `super::ref_sites` for the shapes recognized and
/// `analysis::references` for how these become a vote. Self-contained
/// (carries its own resolved `target` and the op's own [`XmlLocator`])
/// rather than a field on [`PatchOp`] itself:
/// [`PatchOp`] is a `pub` domain type well over a hundred call sites
/// across this workspace construct as full struct literals, and a
/// required field added there would ripple into every one of them for a
/// feature most have no reason to touch — the same reasoning
/// [`crate::domain::ScanOutput::child_value_hashes_by_mod`] documents for
/// keeping that data off [`crate::domain::ScannedMod`].
pub struct PatchValueRefSite {
    /// The def this op's own xpath resolved to — `field_path` is relative
    /// to `<value>`'s own root, so a consumer combines
    /// `target.sub_path` with `field_path` to get the def-relative path
    /// the value actually lands at.
    pub target: DefTarget,
    pub field_path: String,
    pub shape: RefSiteShape,
    pub value: String,
    pub may_require: Vec<String>,
    pub may_require_any_of: Vec<String>,
    /// The owning op's own [`PatchOp::locator`] — lets a caller holding
    /// `scanned_mod.patch_ops` find the matching [`PatchOp`] (by locator
    /// equality) to check `patch_op_active` before trusting this site.
    pub op_locator: XmlLocator,
}

#[derive(Debug, Error)]
pub enum PatchesError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <Patch>")]
    WrongRoot,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// Recursion cap for [`walk_operation`]/[`walk_branch`]'s mutual descent
/// through `<operations>`/`<match>`/`<nomatch>` nesting — mod-provided
/// patch XML, untrusted input, has no engine-enforced nesting limit of its
/// own, and neither function bounded its own recursion before this: a few
/// thousand levels of nested `PatchOperationSequence` (or `<match>`/
/// `<nomatch>` branches) overflows the stack. Matches
/// `rim_merge::xml::MAX_DEPTH`. A node past the cap is still recorded as
/// its own [`PatchOp`] — only descent into its own nested operations
/// stops — since the op itself is a fact about the file regardless of how
/// deep it sits.
const MAX_PATCH_TREE_DEPTH: usize = 64;

/// Operation classes that are control flow only and never mutate the
/// target document themselves — matched by suffix since mods sometimes
/// fully qualify the `Class` attribute.
const NON_MUTATING_SUFFIXES: [&str; 4] = [
    "PatchOperationSequence",
    "PatchOperationConditional",
    "PatchOperationFindMod",
    "PatchOperationTest",
];

/// Per-descent gating context threaded down through `<operations>`/
/// `<match>`/`<nomatch>` nesting — bundled into one struct (rather than
/// several more positional parameters) purely to keep [`walk_operation`]/
/// [`walk_branch`] under clippy's argument-count limit.
struct WalkContext<'a> {
    /// One gate per enclosing `PatchOperationFindMod` (see
    /// [`PatchOp::find_mod_context`]).
    gates: &'a [FindModGate],
    /// The nearest enclosing `PatchOperationConditional`'s own `<xpath>`
    /// text, if any (see [`PatchOp::conditional_xpath`]).
    conditional_xpath: Option<&'a str>,
    /// Which branch of the nearest enclosing `PatchOperationConditional`
    /// this descent is under (see [`PatchOp::conditional_branch`]).
    conditional_branch: Option<ConditionalBranch>,
    /// Whether that same nearest Conditional's own `<nomatch>` branch
    /// holds a creating op (see [`PatchOp::conditional_nomatch_creates`]).
    /// Only meaningful alongside `conditional_branch ==
    /// Some(ConditionalBranch::Match)`.
    conditional_nomatch_creates: bool,
    /// Whether every enclosing `PatchOperationSequence`'s own
    /// `<operations>` list position seen so far, up the ancestry, has
    /// been the *last* item of its list (see [`PatchOp::sequence_tail`]).
    /// Starts `true` (vacuously — no enclosing Sequence yet) and is only
    /// ever narrowed to `false` by a non-last position in one of those
    /// lists; nothing ever flips it back to `true`.
    sequence_tail: bool,
    /// Whether every enclosing mod-setting-gated toggle wrapper
    /// ([`ToggleShape`]) resolves this descent as reachable, at its own
    /// declared default (see [`PatchOp::toggle_active`]). Starts `true`
    /// and is narrowed by an enclosing toggle's own closed branch —
    /// unlike `sequence_tail`, this can be narrowed at more than one
    /// nesting level (a toggle nested inside another toggle), each one
    /// ANDed in.
    toggle_active: bool,
    /// How many `<operations>`/`<match>`/`<nomatch>` levels deep this
    /// descent already is — `0` at the top-level `<Patch>` children, `+1`
    /// each time [`walk_operation`] recurses into a nested operation,
    /// whether through an `<operations>` list item or a `<match>`/
    /// `<nomatch>` branch. See [`MAX_PATCH_TREE_DEPTH`].
    depth: usize,
}

/// Flattens every operation in a `<Patch>` document, recursing through
/// `<operations>`, `<match>`, and `<nomatch>` nesting. `file` becomes
/// every produced [`XmlLocator::file`] — shared, never cloned per op.
pub fn walk(bytes: &[u8], file: &Arc<Path>) -> Result<Vec<PatchOp>, PatchesError> {
    Ok(walk_with_ref_sites(bytes, file)?.0)
}

/// Same as [`walk`], plus every [`PatchValueRefSite`] found along the way
/// (see [`PatchValueRefSite`]'s own doc comment for why this rides
/// alongside rather than becoming a third [`walk`] return value
/// everywhere) and whether the operation-tree depth cap cut off descent
/// into some nested operation.
pub fn walk_with_ref_sites(
    bytes: &[u8],
    file: &Arc<Path>,
) -> Result<(Vec<PatchOp>, Vec<PatchValueRefSite>, bool), PatchesError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(PatchesError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("Patch") {
        return Err(PatchesError::WrongRoot);
    }

    let mut ops = Vec::new();
    let mut ref_sites = Vec::new();
    let mut ref_site_budget = 0usize;
    let mut ref_sites_truncated = false;
    let mut tree_depth_truncated = false;
    let root_context = WalkContext {
        gates: &[],
        conditional_xpath: None,
        conditional_branch: None,
        conditional_nomatch_creates: false,
        sequence_tail: true,
        toggle_active: true,
        depth: 0,
    };
    for (index, op_node) in root
        .children()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        // A direct child of `<Patch>` is a top-level `<Operation>`, never a
        // list item — the game never reads `MayRequire` here; see
        // [`PatchOp::is_list_item`]'s own doc comment.
        walk_operation(
            op_node,
            &[index as u32],
            &root_context,
            file,
            &mut ops,
            false,
            &mut ref_sites,
            &mut ref_site_budget,
            &mut ref_sites_truncated,
            &mut tree_depth_truncated,
        );
    }
    Ok((ops, ref_sites, tree_depth_truncated))
}

/// `path` is this node's own [`XmlLocator::element_path`] — the ordinal
/// path from the document root down to `node` itself. `is_list_item` is
/// this node's own [`PatchOp::is_list_item`] value, decided by the caller:
/// only a `PatchOperationSequence`'s `<operations>` child, or a
/// `<match>`/`<nomatch>` branch that itself holds a list of operations
/// rather than one `Class`-attributed operation, is a real list item.
#[allow(clippy::too_many_arguments)]
fn walk_operation(
    node: Node,
    path: &[u32],
    ctx: &WalkContext,
    file: &Arc<Path>,
    out: &mut Vec<PatchOp>,
    is_list_item: bool,
    out_ref_sites: &mut Vec<PatchValueRefSite>,
    ref_site_budget: &mut usize,
    ref_sites_truncated: &mut bool,
    tree_depth_truncated: &mut bool,
) {
    let Some(class) = node.attribute("Class") else {
        return;
    };
    let is_mutating = !NON_MUTATING_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix));
    let xpath = child_text(node, "xpath");
    let target = xpath.as_deref().and_then(xpath_target::parse);
    let is_find_mod = class.ends_with("PatchOperationFindMod");
    let is_conditional = class.ends_with("PatchOperationConditional");
    let find_mod_names = if is_find_mod {
        find_mod_names_of(node)
    } else {
        Vec::new()
    };
    let mut injected_types = BTreeSet::new();
    if let Some(value) = direct_child(node, "value") {
        collect_class_strings(value, &mut injected_types);
    }
    let mut injected_paths = injected_paths_of(class, node, xpath.as_deref());
    injected_paths.extend(injected_li_predicate_paths_of(
        class,
        node,
        xpath.as_deref(),
    ));
    let injected_template_names = injected_template_names_of(class, node);
    let names_single_def = names_single_def_of(xpath.as_deref(), &target);
    let value_child_names = value_child_names_of(class, node);
    let value_root_names = value_root_names_of(node);
    let value_digest = value_digest_of(node);
    // Structural, name-free toggle detection — only for a class this
    // walk doesn't already recognize as built-in control flow (mirrors
    // `rim-merge`'s own dispatch order; see [`ToggleShape`]'s own doc
    // comment). `is_mutating` is exactly "not one of `NON_MUTATING_SUFFIXES`"
    // here, i.e. "not a recognized built-in".
    let toggle_shape = if is_mutating {
        toggle_shape_of(node, xpath.is_some())
    } else {
        None
    };

    out.push(PatchOp {
        class: class.to_string(),
        xpath: xpath.clone(),
        target: target.clone(),
        find_mod_context: ctx.gates.to_vec(),
        conditional_xpath: ctx.conditional_xpath.map(str::to_string),
        find_mod_names: find_mod_names.clone(),
        may_require: csv_attr(node, "MayRequire"),
        may_require_any_of: csv_attr(node, "MayRequireAnyOf"),
        is_mutating,
        injected_types,
        injected_paths,
        injected_template_names,
        is_list_item,
        load_folder_gate: Vec::new(),
        sequence_tail: ctx.sequence_tail,
        conditional_branch: ctx.conditional_branch,
        conditional_nomatch_creates: ctx.conditional_nomatch_creates,
        names_single_def,
        value_child_names,
        toggle_active: ctx.toggle_active,
        value_root_names,
        value_digest,
        locator: XmlLocator::new(Arc::clone(file), path.to_vec()),
    });

    // This op itself is always recorded above regardless of depth — it's
    // a fact about the file whatever level it sits at. Only descent into
    // its own nested operations (`<operations>`/`<match>`/`<nomatch>`,
    // below) is bounded: mod-provided patch XML has no engine-enforced
    // nesting limit, so an unbounded recursion here is exactly what
    // [`MAX_PATCH_TREE_DEPTH`] exists to prevent.
    if ctx.depth >= MAX_PATCH_TREE_DEPTH {
        *tree_depth_truncated = true;
        return;
    }

    // Case (ii) of the dangling-def-reference rule: an active mutating
    // op's own `<value>`, at the def path it lands on. Only meaningful
    // for an op whose xpath resolved to a known def — an unscoped op has
    // no def path to attribute a reference to at all.
    if is_mutating
        && !*ref_sites_truncated
        && let Some(op_target) = &target
        && let Some((_, value)) = indexed_direct_child(node, "value")
    {
        let mut raw = Vec::new();
        ref_sites::collect(
            value,
            &op_target.def_type,
            &mut raw,
            ref_site_budget,
            ref_sites_truncated,
        );
        let op_locator = XmlLocator::new(Arc::clone(file), path.to_vec());
        out_ref_sites.extend(raw.into_iter().map(|site| PatchValueRefSite {
            target: op_target.clone(),
            field_path: site.field_path,
            shape: site.shape,
            value: site.value,
            may_require: site.may_require,
            may_require_any_of: site.may_require_any_of,
            op_locator: op_locator.clone(),
        }));
    }

    if let Some((operations_index, operations)) = indexed_direct_child(node, "operations") {
        // A *custom* sequence-shaped class (never a real
        // `PatchOperationSequence`, which is already excluded from
        // `toggle_shape` above) gates its whole `<operations>` list on
        // its own declared default — see [`ToggleShape::Sequence`]'s own
        // doc comment.
        let sequence_toggle_open = match toggle_shape {
            Some(ToggleShape::Sequence) => toggle_default(node),
            _ => true,
        };
        let items: Vec<Node> = operations
            .children()
            .filter(roxmltree::Node::is_element)
            .collect();
        let last_index = items.len().saturating_sub(1);
        for (item_index, child) in items.into_iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(operations_index);
            child_path.push(item_index as u32);
            // A `PatchOperationSequence`'s own `<operations>` list is the
            // one place a later sibling can make the engine's own abort on
            // a failed sequence item skip real work — every other nesting
            // (a `<match>`/`<nomatch>` branch's own list) never aborts on
            // a child's failure, so only *this* loop's own position
            // narrows `sequence_tail`.
            let operations_ctx = WalkContext {
                gates: ctx.gates,
                conditional_xpath: ctx.conditional_xpath,
                conditional_branch: ctx.conditional_branch,
                conditional_nomatch_creates: ctx.conditional_nomatch_creates,
                sequence_tail: ctx.sequence_tail && item_index == last_index,
                toggle_active: ctx.toggle_active && sequence_toggle_open,
                depth: ctx.depth + 1,
            };
            // A `PatchOperationSequence`'s `<operations>` child is always a
            // `<li>` list item — the one shape `MayRequire` is actually read
            // on for a patch op (`DirectXmlToObject.ListFromXml`).
            walk_operation(
                child,
                &child_path,
                &operations_ctx,
                file,
                out,
                true,
                out_ref_sites,
                ref_site_budget,
                ref_sites_truncated,
                tree_depth_truncated,
            );
        }
    }

    // A nested Conditional replaces the inherited context for its own
    // `<match>`/`<nomatch>` descendants — "nearest wins" (`PatchOp::conditional_xpath`'s
    // own doc comment); the same rule now also applies to
    // `conditional_branch`/`conditional_nomatch_creates`. Any other node's
    // branches (including a `FindMod`'s) leave all three inherited
    // unchanged. `sequence_tail` is never touched by branch nesting itself
    // (only a Sequence's own `<operations>` list position does that,
    // above) — both branches simply carry the value already reached here
    // through.
    let nomatch_creates = is_conditional && nomatch_branch_creates(node);
    // A Conditional-shaped custom toggle picks its `match`/`operation`
    // branch when its own declared default is `true`, its `nomatch`
    // branch when `false` — exactly like a real Conditional picks a
    // branch by node existence, except the "test" here is the mod
    // setting's own default rather than an xpath lookup. An ordinary
    // node (real Conditional or anything else) leaves both branches at
    // the inherited `ctx.toggle_active` unchanged.
    let (match_toggle_active, nomatch_toggle_active) = match toggle_shape {
        Some(ToggleShape::Conditional) => {
            let enabled = toggle_default(node);
            (ctx.toggle_active && enabled, ctx.toggle_active && !enabled)
        }
        _ => (ctx.toggle_active, ctx.toggle_active),
    };
    let match_ctx = WalkContext {
        gates: ctx.gates,
        conditional_xpath: if is_conditional {
            xpath.as_deref()
        } else {
            ctx.conditional_xpath
        },
        conditional_branch: if is_conditional {
            Some(ConditionalBranch::Match)
        } else {
            ctx.conditional_branch
        },
        conditional_nomatch_creates: if is_conditional {
            nomatch_creates
        } else {
            ctx.conditional_nomatch_creates
        },
        sequence_tail: ctx.sequence_tail,
        toggle_active: match_toggle_active,
        depth: ctx.depth + 1,
    };
    let nomatch_ctx = WalkContext {
        gates: ctx.gates,
        conditional_xpath: match_ctx.conditional_xpath,
        conditional_branch: if is_conditional {
            Some(ConditionalBranch::NoMatch)
        } else {
            ctx.conditional_branch
        },
        // Not meaningful under a `<nomatch>` branch itself (`conditional_nomatch_creates`'s
        // own doc comment) — inherited unchanged rather than recomputed.
        conditional_nomatch_creates: ctx.conditional_nomatch_creates,
        sequence_tail: ctx.sequence_tail,
        toggle_active: nomatch_toggle_active,
        depth: ctx.depth + 1,
    };

    if is_find_mod {
        walk_branch(
            node,
            "match",
            path,
            &match_ctx,
            Some(FindModGate::AnyActive(find_mod_names.clone())),
            file,
            out,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
        walk_branch(
            node,
            "nomatch",
            path,
            &nomatch_ctx,
            Some(FindModGate::NoneActive(find_mod_names)),
            file,
            out,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
    } else {
        walk_branch(
            node,
            "match",
            path,
            &match_ctx,
            None,
            file,
            out,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
        walk_branch(
            node,
            "nomatch",
            path,
            &nomatch_ctx,
            None,
            file,
            out,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
    }
    // A Conditional-shaped custom toggle may instead carry a single
    // `<operation>` child rather than `<match>`/`<nomatch>` — reuse
    // `match_ctx` (the "enabled" context) since that single operation
    // only ever runs when the toggle's own default resolves it on.
    if toggle_shape == Some(ToggleShape::Conditional) {
        walk_branch(
            node,
            "operation",
            path,
            &match_ctx,
            None,
            file,
            out,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
    }
}

/// Recurses into a `<match>`/`<nomatch>` branch, or (for a Conditional-shaped
/// custom toggle, see [`ToggleShape::Conditional`]) its single `<operation>`
/// child — any of which may itself hold a single operation (`Class` on the
/// wrapper) or a list of nested operations. `extra_gate` adds one more gate
/// level for descendants — `AnyActive` for a `FindMod`'s `match` branch,
/// `NoneActive` for its `nomatch` branch, `None` for any other node's
/// `match`/`nomatch`/`operation` (e.g. `PatchOperationConditional`, which
/// isn't `FindMod`-gated). `ctx`'s own `conditional_xpath` and
/// `toggle_active` are the context to hand every descendant reached through
/// this branch — already resolved by the caller (nearest-Conditional
/// override, or toggle default, applied if this branch's own node was one).
#[allow(clippy::too_many_arguments)]
fn walk_branch(
    node: Node,
    tag: &str,
    path: &[u32],
    ctx: &WalkContext,
    extra_gate: Option<FindModGate>,
    file: &Arc<Path>,
    out: &mut Vec<PatchOp>,
    out_ref_sites: &mut Vec<PatchValueRefSite>,
    ref_site_budget: &mut usize,
    ref_sites_truncated: &mut bool,
    tree_depth_truncated: &mut bool,
) {
    let Some((wrapper_index, wrapper)) = indexed_direct_child(node, tag) else {
        return;
    };

    let mut child_gates = ctx.gates.to_vec();
    if let Some(gate) = extra_gate {
        child_gates.push(gate);
    }
    let child_ctx = WalkContext {
        gates: &child_gates,
        conditional_xpath: ctx.conditional_xpath,
        conditional_branch: ctx.conditional_branch,
        conditional_nomatch_creates: ctx.conditional_nomatch_creates,
        sequence_tail: ctx.sequence_tail,
        toggle_active: ctx.toggle_active,
        depth: ctx.depth,
    };

    let mut wrapper_path = path.to_vec();
    wrapper_path.push(wrapper_index);

    if wrapper.attribute("Class").is_some() {
        // The wrapper itself is one operation, not a list — its `MayRequire`
        // is never read (a `<match>`/`<nomatch>` node's own attribute is
        // unhandled by `ModContentPack.LoadPatches`/`DirectXmlToObject.ObjectFromXml`).
        walk_operation(
            wrapper,
            &wrapper_path,
            &child_ctx,
            file,
            out,
            false,
            out_ref_sites,
            ref_site_budget,
            ref_sites_truncated,
            tree_depth_truncated,
        );
    } else {
        for (item_index, child) in wrapper
            .children()
            .filter(roxmltree::Node::is_element)
            .enumerate()
        {
            let mut child_path = wrapper_path.clone();
            child_path.push(item_index as u32);
            // The wrapper holds a list of operations here, so each child is
            // a real `<li>` list item (`DirectXmlToObject.ListFromXml`) even
            // though it's reached through a `<match>`/`<nomatch>` branch.
            walk_operation(
                child,
                &child_path,
                &child_ctx,
                file,
                out,
                true,
                out_ref_sites,
                ref_site_budget,
                ref_sites_truncated,
                tree_depth_truncated,
            );
        }
    }
}

/// Operation classes whose `<value>` genuinely injects new sibling nodes
/// under the target — the only ones the element-injection shape of
/// [`injected_paths_of`] reads. `PatchOperationReplace`'s own `<value>` *is*
/// the replaced node itself, so its top-level element commonly repeats the
/// target's own tag, producing a nonsensical doubled path
/// (`ThingDef/Human/alienRace/alienRace` — tens of thousands of such ops on a
/// real install); `PatchOperationInsert`'s value is positioned relative to an
/// existing sibling, not owned by the target the way an Add's children are.
/// Matched by suffix, same convention as [`NON_MUTATING_SUFFIXES`].
///
/// **`Replace` must stay excluded**: the doubled-path problem above is a
/// *structural* property of every `Replace` (its `<value>` is the replacement
/// for a node the target's own path already names, so the value's top-level
/// tag repeats that path's own last segment by construction), not a corner
/// case a `Class`-attribute gate could narrow around — a `Replace` swapping
/// in a differently-`Class`d `<li>` still produces `.../li/li`. The
/// class-string injector pass (`injected_types`,
/// [`collect_class_strings`](super::xml_util::collect_class_strings)) is the
/// mechanism that already handles a `Replace`'s injected `Class` correctly,
/// unconditionally, with no `Add`-only gate of its own — see
/// `analysis::edges::patch_injected_node_edge_recognizes_a_replace_introduced_class_exactly_like_add`.
const APPEND_SEMANTICS_SUFFIXES: [&str; 2] = ["PatchOperationAdd", "PatchOperationAddModExtension"];

/// Whether `class` has append-to-target semantics (see
/// [`APPEND_SEMANTICS_SUFFIXES`]) — `pub(crate)` so `analysis::conflicts`'
/// per-element collision keying can gate its own per-element split on the
/// identical rule this module's own [`injected_paths_of`] already uses to
/// populate [`PatchOp::injected_paths`], rather than maintaining a second
/// copy of the suffix list that could silently drift from this one.
#[must_use]
pub(crate) fn has_append_semantics(class: &str) -> bool {
    APPEND_SEMANTICS_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix))
}

/// This operation's own `<value>` injected paths — see
/// [`PatchOp::injected_paths`] for the two shapes and their exact format.
/// `class` is the op's own `Class` attribute (gates the element-injection
/// shape to append-semantics classes only, see
/// [`APPEND_SEMANTICS_SUFFIXES`]); `xpath` is the op's own raw `<xpath>` text
/// — used both to detect the whole-`<Defs>`-root shape and, for the
/// element-injection shape, re-parsed via [`xpath_target::parse_all`] (the
/// *single* [`crate::domain::PatchOp::target`] this op resolved to isn't
/// enough — a disjunctive head, `[defName="A" or defName="B"]`, must record
/// the injection under *every* def it names, the same reason
/// `analysis::edges::patch_removed_node_edges` uses
/// `analysis::patch_op_targets` instead of the single `target` too).
fn injected_paths_of(class: &str, node: Node, xpath: Option<&str>) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let Some(value) = direct_child(node, "value") else {
        return paths;
    };

    if matches!(xpath.map(str::trim), Some("Defs" | "/Defs")) {
        for element in direct_element_children(value) {
            let Some(def_name) = child_text(element, "defName") else {
                continue;
            };
            paths.insert(format!("{}/{def_name}", element.tag_name().name()));
        }
        return paths;
    }

    if !has_append_semantics(class) {
        return paths;
    }
    let Some(xpath) = xpath else {
        return paths;
    };
    for target in xpath_target::parse_all(xpath) {
        let base = target.match_key();
        for element in direct_element_children(value) {
            let path = format!("{base}/{}", element.tag_name().name());
            // Drop an xpath-predicate-bearing path (a bracketed `sub_path`,
            // e.g. from a compound predicate like `[not(comps)]`) or one
            // ending in an `li` segment: a list item isn't individually
            // addressable by path (many share the tag `li`), so prefix-
            // matching on one in the path-shape injected-node pass would both
            // false-negative (never match the real injected item) and
            // false-positive (match an unrelated `li` under a different list)
            // — see `PatchOp::injected_paths`'s own doc comment.
            if path.contains(['[', ']']) || path.split('/').any(|segment| segment == "li") {
                continue;
            }
            paths.insert(path);
        }
    }
    paths
}

/// The plain tag name of `target`'s own last path segment — `target.def_type`
/// when there's no `sub_path` (a whole-def target, i.e. this op replaces the
/// entire def), else the last `/`-delimited segment of `sub_path` with any
/// trailing `[...]` predicate stripped. A simple, non-bracket-aware split
/// (unlike `analysis::edges::patches`'s own `split_sub_path_segments`, which
/// this crate's lower `extract` layer cannot depend on): the two shapes this
/// feeds — the "keeps the node" check
/// (`analysis::edges::patches::replace_discards_addition`) and
/// [`injected_li_predicate_paths_of`]'s own Replace case below — only ever
/// need the tag of the node actually being replaced, which never contains a
/// `/` inside a quoted predicate value; a wrong split only ever costs a
/// missed match, never a fabricated one.
pub(crate) fn target_last_step_tag(target: &DefTarget) -> &str {
    match target.sub_path.as_deref() {
        Some(sub_path) => {
            let last = sub_path.rsplit('/').next().unwrap_or(sub_path);
            last.split('[').next().unwrap_or(last)
        }
        None => target.def_type.as_str(),
    }
}

/// `true` when `value`'s own direct element children are exactly one
/// element, and its tag equals `target`'s own last path segment
/// ([`target_last_step_tag`]) — the "the replacement value keeps the
/// replaced node's own tag" shape `replace_discards_addition` needs on
/// the *toucher* side too, for a
/// `PatchOperationReplace` that legitimately still counts as injecting new
/// content into that node (as opposed to renaming or splitting it).
fn replace_value_keeps_node(target: &DefTarget, value: Node) -> bool {
    let mut children = direct_element_children(value);
    let Some(only) = children.next() else {
        return false;
    };
    if children.next().is_some() {
        return false;
    }
    only.tag_name().name() == target_last_step_tag(target)
}

/// One `<li>` element's own identifying attribute or text — the two shapes
/// a RimWorld list item is keyed by in practice: an explicit `Class`
/// attribute (`<li Class="RoomRequirement_ThingAnyOfCount">`, the common
/// "typed list item" idiom), or, for an `<li>` with no element children and
/// no `Class` of its own, its own text **verbatim** (`<li>SomeDefName</li>`,
/// the common "bare defName reference" idiom) — an XPath `text()="X"` compares
/// the text node as written, so `<li> X </li>` does not match `li[text()="X"]`
/// in the game; a whitespace-only text is no identity. `None` for an `<li>` that fits
/// neither — an `<li>` carrying element children but no `Class` has no
/// identifying value this shape can key on, and is silently skipped rather
/// than guessed at.
pub(crate) fn li_predicate_identity<'a>(li: Node<'a, 'a>) -> Option<(Option<&'a str>, String)> {
    if let Some(class) = li.attribute("Class") {
        return Some((Some("Class"), class.to_string()));
    }
    if li.children().any(|child| child.is_element()) {
        return None;
    }
    let text = li.text()?;
    (!text.trim().is_empty()).then(|| (None, text.to_string()))
}

/// Renders a [`li_predicate_identity`] pair as the canonical `li[...]`
/// suffix text both the producer side here and the consumer side
/// (`analysis::edges::patches::li_predicate_lookup_key`) build — the two
/// **must** stay in exact agreement, since [`injected_paths_of`]'s own
/// second consumer pass (`analysis::edges::patch_injected_node_edges`)
/// matches this text by plain equality, not by re-parsing either side. An
/// attribute identity renders `li[@Attr="value"]`; a text identity renders
/// `li[text()="value"]` — always double-quoted, regardless of how the
/// author actually wrote either side's own XML, which is the entire point:
/// this is a normalized key, never the raw source text.
pub(crate) fn li_predicate_suffix(identity: &(Option<&str>, String)) -> String {
    match identity.0 {
        Some(attr) => format!("li[@{attr}=\"{}\"]", identity.1),
        None => format!("li[text()=\"{}\"]", identity.1),
    }
}

/// Extends [`injected_paths_of`] with predicate-keyed `<li>` items a
/// creating op's own `<value>` injects — closing a gap that function's own
/// doc comment discloses (a bracketed or `li`-ending path is dropped
/// outright there, and `Replace` is excluded entirely). Three real-install
/// shapes:
///
/// - **`Add`/`AddModExtension`** ([`has_append_semantics`]): every direct
///   `<li>` child of `<value>` with a recognizable identity
///   ([`li_predicate_identity`]) is recorded as
///   `"{target.match_key()}/{li_predicate_suffix}"` — the same base
///   [`injected_paths_of`]'s own element-injection shape already computes,
///   just refined to the specific list item rather than the unaddressable
///   bare `li` segment.
/// - **`PatchOperationReplace` of a plain container**, when
///   [`replace_value_keeps_node`]: the replacement value still keeps the
///   replaced node's own tag (the same "not a rename" exclusion
///   `replace_discards_addition` uses), so its one kept child's own direct `<li>` children
///   are genuinely new content at that same node — recorded the same way,
///   based on `target.match_key()` (which already ends in the kept node's
///   own tag).
/// - **`PatchOperationReplace` of a predicate-keyed `<li>` itself**
///   ([`target_last_step_is_li_predicate`]): the node being replaced is a
///   specific list item, not a container, so the replacement's own kept
///   child (still checked via [`replace_value_keeps_node`], whose tag
///   equality passes vacuously here — both sides are generically `li`) *is*
///   the new item at that same position, carrying its own identity, which
///   may genuinely differ from the one it replaced (the real-install shape:
///   a compat patch replaces a `RoomRequirement_ThingCount`-classed `<li>`
///   with a differently-classed one, and a third mod's own op selects the
///   *new* class). Recorded one level up (the target's own `sub_path` with
///   that last predicate segment stripped, [`strip_last_sub_path_segment`])
///   plus the new `<li>`'s own identity — a selector reads the document
///   *after* the replace runs, so only the new identity is ever a real
///   dependency.
///
/// The first two shapes are gated to *direct* `<li>` children one level
/// below the node actually being written — deliberately not a deeper
/// recursive walk, to keep this precise (bounded to the documented real
/// shapes) rather than flooding the index with every nested list anywhere
/// in a large injected subtree; the third never descends into the
/// replacement `<li>`'s own children either, for the same reason.
fn injected_li_predicate_paths_of(
    class: &str,
    node: Node,
    xpath: Option<&str>,
) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let Some(value) = direct_child(node, "value") else {
        return paths;
    };
    let Some(xpath) = xpath else {
        return paths;
    };
    let is_replace = class.ends_with("PatchOperationReplace");
    if !has_append_semantics(class) && !is_replace {
        return paths;
    }

    for target in xpath_target::parse_all(xpath) {
        if is_replace && target_last_step_is_li_predicate(&target) {
            // The node being replaced is itself a predicate-keyed `<li>`
            // (e.g. `.../li[@Class="X"][...]`), not a plain container tag
            // — `replace_value_keeps_node`'s own tag-equality check passes
            // vacuously here (both sides are generically `li`), but the
            // real shape is different: the replacement's own kept child
            // *is* the new node at that same list position, carrying its
            // own (possibly different) identity, not a container whose
            // children are the list items. Recorded one level up (the
            // target's own sub_path with that last predicate segment
            // stripped) plus the *new* li's own identity — never the
            // stripped-off old one, since a selector reads the document
            // *after* the replace runs.
            if !replace_value_keeps_node(&target, value) {
                continue;
            }
            let Some(kept) = direct_element_children(value).next() else {
                continue;
            };
            let Some(identity) = li_predicate_identity(kept) else {
                continue;
            };
            let parent = DefTarget {
                def_type: target.def_type.clone(),
                def_name: target.def_name.clone(),
                selector: target.selector,
                sub_path: target
                    .sub_path
                    .as_deref()
                    .and_then(strip_last_sub_path_segment)
                    .map(str::to_string),
            }
            .match_key();
            paths.insert(format!("{parent}/{}", li_predicate_suffix(&identity)));
            continue;
        }

        let base = target.match_key();
        let container = if is_replace {
            if !replace_value_keeps_node(&target, value) {
                continue;
            }
            // `replace_value_keeps_node` already confirmed `value` has
            // exactly one direct element child — that child is the kept
            // node itself, so its own `<li>` children are one level deeper
            // than `value`'s.
            let Some(kept) = direct_element_children(value).next() else {
                continue;
            };
            kept
        } else {
            value
        };
        for li in direct_element_children(container).filter(|el| el.tag_name().name() == "li") {
            let Some(identity) = li_predicate_identity(li) else {
                continue;
            };
            paths.insert(format!(
                "{base}/{}",
                li_predicate_suffix(&(identity.0, identity.1))
            ));
        }
    }
    paths
}

/// Whether `target`'s own `sub_path` ends in a predicate-keyed `<li>`
/// step (`li[...]`) rather than a plain container tag — feeds
/// [`injected_li_predicate_paths_of`]'s own Replace-of-a-list-item
/// shape. A simple, non-bracket-aware last-segment read, the same
/// accepted limitation [`target_last_step_tag`] documents.
fn target_last_step_is_li_predicate(target: &DefTarget) -> bool {
    target
        .sub_path
        .as_deref()
        .and_then(|sub_path| sub_path.rsplit('/').next())
        .is_some_and(|last| last.starts_with("li["))
}

/// `sub_path`'s own text with its last `/`-delimited segment removed —
/// `None` when `sub_path` is a single segment (nothing remains before
/// it). The same simple, non-bracket-aware split
/// [`target_last_step_tag`] already uses, for the identical reason: the
/// one caller here only ever strips a plain predicate-li's own segment,
/// which never itself contains an internal `/`.
fn strip_last_sub_path_segment(sub_path: &str) -> Option<&str> {
    sub_path.rsplit_once('/').map(|(prefix, _)| prefix)
}

/// This op's own `<value>` top-level element names, in document order —
/// feeds [`crate::domain::PatchOp::value_root_names`]. Unlike
/// [`value_child_names_of`], computed for *any* op with a `<value>`,
/// whatever its class.
fn value_root_names_of(node: Node) -> Vec<String> {
    let Some(value) = direct_child(node, "value") else {
        return Vec::new();
    };
    direct_element_children(value)
        .map(|element| element.tag_name().name().to_string())
        .collect()
}

/// This op's own `<value>` structural digest — feeds
/// [`crate::domain::PatchOp::value_digest`]. `None` for an op with no
/// `<value>` at all.
fn value_digest_of(node: Node) -> Option<ValueDigest> {
    let value = direct_child(node, "value")?;
    let mut digest = ValueDigest::default();
    let mut budget = MAX_VALUE_DIGEST_ENTRIES;
    walk_value_digest(value, "", 0, &mut digest, &mut budget);
    Some(digest)
}

/// Recursive worker for [`value_digest_of`]: `path` is the accumulated
/// relative path down to (not including) `node`'s own children. Stops
/// descending (and marks `truncated`) past [`MAX_VALUE_DIGEST_DEPTH`]
/// levels or once `budget` (entries remaining) hits zero.
fn walk_value_digest(
    node: Node,
    path: &str,
    depth: usize,
    digest: &mut ValueDigest,
    budget: &mut usize,
) {
    for child in node.children().filter(roxmltree::Node::is_element) {
        if *budget == 0 {
            digest.truncated = true;
            return;
        }
        let child_path = if path.is_empty() {
            child.tag_name().name().to_string()
        } else {
            format!("{path}/{}", child.tag_name().name())
        };
        let (hash, hash_truncated) = content_hash(child);
        digest.entries.insert((child_path.clone(), hash));
        if hash_truncated {
            digest.truncated = true;
        }
        *budget -= 1;
        let has_element_children = child.children().any(|c| c.is_element());
        if has_element_children {
            if depth + 1 < MAX_VALUE_DIGEST_DEPTH {
                walk_value_digest(child, &child_path, depth + 1, digest, budget);
            } else {
                digest.truncated = true;
            }
        }
    }
}

/// A deterministic structural hash of `node`'s own tag, attributes
/// (name-sorted, so attribute order in the source XML never changes the
/// hash), and either its element children's own hashes (recursively, in
/// document order) or its own trimmed text when it has none. Uses
/// [`std::collections::hash_map::DefaultHasher`] directly (not through a
/// `HashMap`): unlike `HashMap::new()`, `DefaultHasher::new()` uses fixed
/// SipHash keys and is **not** randomized per process, so the hash is
/// reproducible across runs — required for the byte-identical-report
/// invariant this crate's own `CLAUDE.md` documents.
/// Recursion cap for [`hash_node_content`] — independent of
/// [`walk_value_digest`]'s own depth budget, which only bounds how far
/// *that* function itself descends before treating a child as an opaque
/// digest entry: `content_hash` still walks that one child's **entire**
/// subtree to compute its hash, however deep it is. A mod-provided patch
/// XML nesting one element inside itself tens of thousands of levels deep
/// would otherwise overflow the stack right here, unbounded by
/// `MAX_VALUE_DIGEST_DEPTH`. Matches `rim_merge::xml::MAX_DEPTH`.
const MAX_CONTENT_HASH_DEPTH: usize = 64;

/// Returns the hash plus whether it stopped short of the real subtree
/// (past [`MAX_CONTENT_HASH_DEPTH`]) — the caller folds that into its own
/// [`ValueDigest::truncated`], the existing "don't trust this digest to
/// prove absence" signal, rather than a second, parallel truncation flag.
fn content_hash(node: Node) -> (u64, bool) {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut truncated = false;
    hash_node_content(node, 0, &mut hasher, &mut truncated);
    (hasher.finish(), truncated)
}

fn hash_node_content(
    node: Node,
    depth: usize,
    hasher: &mut impl std::hash::Hasher,
    truncated: &mut bool,
) {
    use std::hash::Hash;
    node.tag_name().name().hash(hasher);
    let mut attrs: Vec<(&str, &str)> = node.attributes().map(|a| (a.name(), a.value())).collect();
    attrs.sort_unstable();
    attrs.hash(hasher);
    if depth >= MAX_CONTENT_HASH_DEPTH {
        // Past the cap: hash a fixed marker instead of trusting an
        // attacker-controlled subtree to ever bottom out on its own.
        "…subtree truncated…".hash(hasher);
        *truncated = true;
        return;
    }
    let mut element_children = node
        .children()
        .filter(roxmltree::Node::is_element)
        .peekable();
    if element_children.peek().is_some() {
        for child in element_children {
            hash_node_content(child, depth + 1, hasher, truncated);
        }
    } else if let Some(text) = node.text() {
        text.trim().hash(hasher);
    }
}

/// Operation classes that write an XML *attribute* onto an existing node
/// rather than injecting a subtree — the second way a patch can register
/// an inheritance name, and the one the real install actually uses most
/// (Replace Stuff's `CoolerOverWallPatches.xml` bolts `Name="Cooler"`
/// onto vanilla's own `Cooler` `ThingDef` so its own defs can inherit
/// from it). Matched by suffix, same convention as
/// [`NON_MUTATING_SUFFIXES`].
const ATTRIBUTE_WRITING_SUFFIXES: [&str; 2] =
    ["PatchOperationAttributeAdd", "PatchOperationAttributeSet"];

/// Every inheritance `Name` this operation registers — see
/// [`PatchOp::injected_template_names`] for why a patch-registered name
/// matters at all and why this deliberately over-collects. Two shapes:
///
/// - any `Name` attribute anywhere inside the op's own `<value>` subtree
///   (a patch injecting a whole `Name`-attributed def or template);
/// - an attribute-writing op (see [`ATTRIBUTE_WRITING_SUFFIXES`]) whose
///   `<attribute>` is literally `Name`, in which case the registered name
///   is the op's own `<value>` *text*.
fn injected_template_names_of(class: &str, node: Node) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let Some(value) = direct_child(node, "value") else {
        return names;
    };

    let writes_attribute = ATTRIBUTE_WRITING_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix));
    if writes_attribute && child_text(node, "attribute").as_deref() == Some("Name") {
        // The attribute case's `<value>` is text, never a subtree — a
        // node keeps whatever mod its own asset belongs to, so strictly
        // this registers under *that* mod rather than `mod == null`.
        // Collected here all the same: every consumer only ever uses this
        // set to suppress output, and the overwhelmingly common real case
        // is a mod bolting a `Name` onto a *vanilla* def, which resolves
        // for everybody either way.
        if let Some(text) = value.text().map(str::trim)
            && !text.is_empty()
        {
            names.insert(text.to_string());
        }
        return names;
    }

    for element in value.descendants().filter(roxmltree::Node::is_element) {
        let Some(name) = element.attribute("Name").map(str::trim) else {
            continue;
        };
        if !name.is_empty() {
            names.insert(name.to_string());
        }
    }
    names
}

/// Operation classes treated as "brings a node into existence" —
/// [`APPEND_SEMANTICS_SUFFIXES`] (`Add`/`AddModExtension`) plus `Insert`
/// (positions new content next to an existing sibling), `Replace` (the only
/// way a `<nomatch>` branch's own `Replace` can succeed is against an
/// *ancestor* node that still exists once the tested node itself is gone,
/// which itself recreates the region), and `AddOrReplace` (a common
/// third-party idiom, "create this node if it's missing, replace it if it
/// isn't" — a real-install compat-framework class). Wider than
/// [`APPEND_SEMANTICS_SUFFIXES`] on purpose: every consumer of this list
/// only ever uses it to *skip* emitting an edge or a false-positive
/// classification (see [`PatchOp::conditional_nomatch_creates`]'s own doc
/// comment for the false-positive-only risk (never a fabricated edge)
/// that makes the extra leniency acceptable here, unlike
/// [`injected_paths_of`]'s much stricter
/// gate, and `analysis::edges::patches::remover_recreates_region`'s own doc
/// comment for the sibling real-install case this same list closes).
/// `pub(crate)` so the analysis-level consumer can reuse the identical list
/// rather than maintaining a second copy that could silently drift.
pub(crate) const NODE_CREATING_SUFFIXES: [&str; 5] = [
    "PatchOperationAdd",
    "PatchOperationInsert",
    "PatchOperationAddModExtension",
    "PatchOperationReplace",
    "PatchOperationAddOrReplace",
];

/// Whether `conditional_node`'s own `<nomatch>` child holds at least one
/// op with creating semantics ([`NODE_CREATING_SUFFIXES`]),
/// anywhere in that branch's subtree regardless of nesting — feeds
/// [`PatchOp::conditional_nomatch_creates`]. `conditional_node` is the
/// `PatchOperationConditional` node itself, not yet descended into.
fn nomatch_branch_creates(conditional_node: Node) -> bool {
    let Some(nomatch) = direct_child(conditional_node, "nomatch") else {
        return false;
    };
    nomatch
        .descendants()
        .filter(roxmltree::Node::is_element)
        .any(|element| {
            element.attribute("Class").is_some_and(|class| {
                NODE_CREATING_SUFFIXES
                    .iter()
                    .any(|suffix| class.ends_with(suffix))
            })
        })
}

/// Whether this op's own `<xpath>` names exactly one def — feeds
/// [`PatchOp::names_single_def`]. Mirrors
/// `analysis::patch_op_targets`'s own re-parse-with-fallback logic
/// (duplicated rather than called: `extract` sits below `analysis` in
/// this crate's own layering and cannot depend on it) — re-parses
/// `xpath` via [`xpath_target::parse_all`] for a disjunctive head's full
/// count, falling back to whether the single already-resolved `target`
/// is present when the xpath carries no def-naming head at all.
fn names_single_def_of(xpath: Option<&str>, target: &Option<DefTarget>) -> bool {
    match xpath {
        Some(xpath) => {
            let targets = xpath_target::parse_all(xpath);
            if targets.is_empty() {
                target.is_some()
            } else {
                targets.len() == 1
            }
        }
        None => target.is_some(),
    }
}

/// Classes whose own `<value>` top-level element children are genuinely
/// new content this op adds under its own target — feeds
/// [`PatchOp::value_child_names`]. A real compat mod ships exactly the
/// `AddOrReplace` case: `RR.PatchOperationAddOrReplace` targets a *def*
/// (`Defs/ThingDef[defName="FloodLight"]`, no `sub_path`) whose own
/// `<value>` names the *child* it recreates
/// (`<researchPrerequisites>...</researchPrerequisites>`) — the "add it if
/// missing, replace it if present" idiom, which behaves exactly like a
/// plain `Add` once a sibling `PatchOperationRemove` has already deleted
/// the child. Deliberately **not** `extract::patches::APPEND_SEMANTICS_SUFFIXES`
/// (kept as its own list, on purpose — see [`PatchOp::value_child_names`]'s
/// own doc comment for why sharing that gate would be the wrong call).
const VALUE_CHILD_SUFFIXES: [&str; 3] = [
    "PatchOperationAdd",
    "PatchOperationAddModExtension",
    "PatchOperationAddOrReplace",
];

/// This op's own `<value>` top-level element names, gated to
/// [`VALUE_CHILD_SUFFIXES`] — feeds [`PatchOp::value_child_names`].
fn value_child_names_of(class: &str, node: Node) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if !VALUE_CHILD_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix))
    {
        return names;
    }
    let Some(value) = direct_child(node, "value") else {
        return names;
    };
    for element in direct_element_children(value) {
        names.insert(element.tag_name().name().to_string());
    }
    names
}

fn read_bool_child(node: Node, tag: &str) -> Option<bool> {
    child_text(node, tag).map(|text| text.eq_ignore_ascii_case("true"))
}

/// The declared default of a custom, mod-setting-gated operation class —
/// the first of `<enabled>`, `<defaultValue>`, `<default>` it declares, or
/// `true` when it declares none.
///
/// **Shared with `rim-merge`'s own replay** (`rim_merge::patch_eval::identity`,
/// which re-exports this exact function rather than keeping its own
/// copy): the analyzer's own load-order edges and the replay's own
/// failure predictions must never silently disagree about which mods a
/// default-off toggle actually runs. `rim-merge` already depends on
/// `rim-analyzer` directly (the crate graph is analyzer -> resolve ->
/// merge), so this is the lowest layer both can share from.
#[must_use]
pub fn toggle_default(node: Node) -> bool {
    read_bool_child(node, "enabled")
        .or_else(|| read_bool_child(node, "defaultValue"))
        .or_else(|| read_bool_child(node, "default"))
        .unwrap_or(true)
}

/// Which structural, name-free toggle shape `node` matches, if any —
/// [`walk_operation`] only ever calls this for a node whose own class
/// isn't already one of [`NON_MUTATING_SUFFIXES`] (mirrors `rim-merge`'s
/// own dispatch order: built-in control-flow classes first, this
/// structural fallback only once none of them match — see
/// `rim-merge/CLAUDE.md`'s own "Custom operation classes are data, not
/// code" section for the shared design). Both shapes are facts about
/// RimWorld's own patch grammar, never about any specific mod's class
/// name — an ordinary leaf mutation (`PatchOperationAdd`/`Replace`/...)
/// never matches either, since it always carries its own `<xpath>` and
/// never an `<operations>` child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleShape {
    /// Has its own `<operations>` child — a sequence-shaped custom class
    /// wearing `PatchOperationSequence`'s own clothes. Its declared
    /// default gates the whole list: every op inside runs only when
    /// `true`.
    Sequence,
    /// No `<xpath>` of its own, but has a `<match>`/`<nomatch>` branch or
    /// a single `<operation>` child — a mod-setting-gated custom class
    /// wearing `PatchOperationConditional`'s own clothes. Its declared
    /// default picks `match`/`operation` (when `true`) or `nomatch`
    /// (when `false`), exactly like a real Conditional picks a branch by
    /// node existence.
    Conditional,
}

fn toggle_shape_of(node: Node, has_xpath: bool) -> Option<ToggleShape> {
    if indexed_direct_child(node, "operations").is_some() {
        return Some(ToggleShape::Sequence);
    }
    if !has_xpath
        && (direct_child(node, "match").is_some()
            || direct_child(node, "nomatch").is_some()
            || direct_child(node, "operation").is_some())
    {
        return Some(ToggleShape::Conditional);
    }
    None
}

/// [`direct_child`], plus that child's own ordinal position among
/// `node`'s element children — needed to extend an [`XmlLocator`]'s
/// element path down into it.
fn indexed_direct_child<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
) -> Option<(u32, Node<'a, 'input>)> {
    node.children()
        .filter(roxmltree::Node::is_element)
        .enumerate()
        .find(|(_, c)| c.tag_name().name().eq_ignore_ascii_case(tag))
        .map(|(index, c)| (index as u32, c))
}

fn find_mod_names_of(find_mod_node: Node) -> Vec<String> {
    direct_child(find_mod_node, "mods")
        .map(strings_from_container)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file() -> Arc<Path> {
        Arc::from(Path::new("test.xml"))
    }

    fn walk_test(bytes: &[u8]) -> Vec<PatchOp> {
        walk(bytes, &test_file()).unwrap()
    }

    #[test]
    fn walks_a_single_leaf_operation() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="Wall"]/statBases</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].class, "PatchOperationReplace");
        assert!(ops[0].is_mutating);
        assert_eq!(ops[0].locator.element_path, vec![0]);
        let target = ops[0].target.as_ref().unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
        assert_eq!(target.sub_path.as_deref(), Some("statBases"));
    }

    #[test]
    fn walks_sequence_operations_list() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationSequence">
                <operations>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="A"]</xpath>
                  </li>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="B"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        // Sequence itself (non-mutating) + 2 leaf adds.
        assert_eq!(ops.len(), 3);
        assert!(!ops[0].is_mutating);
        assert_eq!(ops[0].class, "PatchOperationSequence");
        assert_eq!(ops[0].locator.element_path, vec![0]);
        assert!(ops[1].is_mutating);
        assert_eq!(ops[1].target.as_ref().unwrap().def_name, "A");
        // Sequence (0) -> its <operations> (index 0 among the Sequence's
        // own children) -> 1st <li> (index 0).
        assert_eq!(ops[1].locator.element_path, vec![0, 0, 0]);
        assert_eq!(ops[2].target.as_ref().unwrap().def_name, "B");
        assert_eq!(ops[2].locator.element_path, vec![0, 0, 1]);
    }

    #[test]
    fn find_mod_names_live_on_the_find_mod_op_and_gate_its_match_and_nomatch_children() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationFindMod">
                <mods><li>Example Combat Mod</li><li>Example Atomics Mod</li></mods>
                <match Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="A"]</xpath>
                </match>
                <nomatch Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="B"]</xpath>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let names = vec![
            "Example Combat Mod".to_string(),
            "Example Atomics Mod".to_string(),
        ];

        let find_mod = ops
            .iter()
            .find(|op| op.class == "PatchOperationFindMod")
            .unwrap();
        assert_eq!(find_mod.find_mod_names, names);
        assert!(find_mod.find_mod_context.is_empty());

        let matched = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert_eq!(
            matched.find_mod_context,
            vec![FindModGate::AnyActive(names.clone())]
        );
        assert!(matched.find_mod_names.is_empty());
        // `<mods>` is child 0 of the Operation, so `<match>` (a leaf
        // operation itself, `Class` on the wrapper) is child 1.
        assert_eq!(matched.locator.element_path, vec![0, 1]);

        let unmatched = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "B"))
            .unwrap();
        assert_eq!(
            unmatched.find_mod_context,
            vec![FindModGate::NoneActive(names)]
        );
        assert_eq!(unmatched.locator.element_path, vec![0, 2]);
    }

    #[test]
    fn find_mod_with_only_a_nomatch_branch_still_records_its_own_names() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationFindMod">
                <mods><li>Some Mod</li></mods>
                <nomatch Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="A"]</xpath>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);

        let find_mod = ops
            .iter()
            .find(|op| op.class == "PatchOperationFindMod")
            .unwrap();
        assert_eq!(find_mod.find_mod_names, vec!["Some Mod".to_string()]);

        let leaf = ops.iter().find(|op| op.is_mutating).unwrap();
        assert_eq!(
            leaf.find_mod_context,
            vec![FindModGate::NoneActive(vec!["Some Mod".to_string()])]
        );
    }

    #[test]
    fn nested_find_mod_produces_one_gate_per_enclosing_find_mod() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationFindMod">
                <mods><li>Outer Mod</li></mods>
                <match Class="PatchOperationFindMod">
                  <mods><li>Inner Mod</li></mods>
                  <match Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="A"]</xpath>
                  </match>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let leaf = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert_eq!(
            leaf.find_mod_context,
            vec![
                FindModGate::AnyActive(vec!["Outer Mod".to_string()]),
                FindModGate::AnyActive(vec!["Inner Mod".to_string()]),
            ]
        );
        // Outer Operation (0) -> outer <match> (index 1, after <mods>) ->
        // inner <match> (index 1, after its own <mods>).
        assert_eq!(leaf.locator.element_path, vec![0, 1, 1]);
    }

    #[test]
    fn conditional_recurses_into_match_and_nomatch_lists() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
                <match>
                  <li Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="A"]/comps</xpath></li>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(ops.len(), 2);
        assert!(!ops[0].is_mutating);
        assert!(ops[1].is_mutating);
        // Operation (0) -> <match> (index 1, after <xpath>) -> its 1st
        // <li> (index 0).
        assert_eq!(ops[1].locator.element_path, vec![0, 1, 0]);
    }

    #[test]
    fn operation_without_class_is_ignored() {
        let xml =
            br#"<Patch><Operation><xpath>Defs/ThingDef[defName="A"]</xpath></Operation></Patch>"#;
        let ops = walk_test(xml);
        assert!(ops.is_empty());
    }

    #[test]
    fn parses_may_require_and_may_require_any_of_on_an_operation() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd" MayRequire="a.b,c.d">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
              </Operation>
              <Operation Class="PatchOperationAdd" MayRequireAnyOf="e.f, g.h">
                <xpath>Defs/ThingDef[defName="B"]</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].may_require,
            vec!["a.b".to_string(), "c.d".to_string()]
        );
        assert!(ops[0].may_require_any_of.is_empty());
        assert_eq!(
            ops[1].may_require_any_of,
            vec!["e.f".to_string(), "g.h".to_string()]
        );
        assert!(ops[1].may_require.is_empty());
    }

    /// `Class` attributes and `*Class` element text inside a mutating op's
    /// `<value>` are collected into `injected_types`, exact case, regardless
    /// of nesting depth.
    #[test]
    fn injected_types_are_collected_from_the_value_child() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="X"]/modExtensions</xpath>
                <value>
                  <li Class="Example.Weapons.HeavyWeapon">
                    <comps>
                      <li><compClass>Some.Namespace.Comp</compClass></li>
                    </comps>
                  </li>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_types,
            std::collections::BTreeSet::from([
                "Example.Weapons.HeavyWeapon".to_string(),
                "Some.Namespace.Comp".to_string(),
            ])
        );
    }

    /// A patch that adds a `Name`-attributed node registers that name with
    /// `Verse.XmlInheritance` under `mod == null`, so it satisfies every
    /// child's `ParentName` lookup regardless of load order.
    #[test]
    fn injected_template_names_are_collected_from_the_value_child() {
        let xml = br#"<Patch>
            <Operation Class="PatchOperationAdd">
                <xpath>Defs</xpath>
                <value>
                    <ThingDef Name="PatchedBase" Abstract="True">
                        <comps><li Name="NestedName" /></comps>
                    </ThingDef>
                </value>
            </Operation>
        </Patch>"#;

        let ops = walk_test(xml);

        assert_eq!(
            ops[0].injected_template_names,
            BTreeSet::from(["PatchedBase".to_string(), "NestedName".to_string()]),
            "the whole <value> subtree is swept on purpose - see the field's own doc comment"
        );
    }

    /// Real-install shape: a `PatchOperationAttributeAdd` bolting
    /// `Name="Cooler"` onto vanilla's own `Cooler` `ThingDef` is what makes
    /// `ParentName="Cooler"` resolve at all — the `<value>` here is text, not
    /// a subtree.
    #[test]
    fn injected_template_names_reads_an_attribute_add_of_name() {
        let xml = br#"<Patch>
            <Operation Class="PatchOperationAttributeAdd">
                <xpath>Defs/ThingDef[defName="Cooler"]</xpath>
                <attribute>Name</attribute>
                <value>Cooler</value>
            </Operation>
        </Patch>"#;

        let ops = walk_test(xml);

        assert_eq!(
            ops[0].injected_template_names,
            BTreeSet::from(["Cooler".to_string()])
        );
    }

    #[test]
    fn injected_template_names_ignores_an_attribute_add_of_another_attribute() {
        let xml = br#"<Patch>
            <Operation Class="PatchOperationAttributeAdd">
                <xpath>Defs/ThingDef[defName="Cooler"]</xpath>
                <attribute>ParentName</attribute>
                <value>SomeBase</value>
            </Operation>
        </Patch>"#;

        let ops = walk_test(xml);

        assert!(ops[0].injected_template_names.is_empty());
    }

    #[test]
    fn injected_template_names_is_empty_without_a_name_attribute() {
        let xml = br#"<Patch>
            <Operation Class="PatchOperationAdd">
                <xpath>Defs</xpath>
                <value><ThingDef><defName>Plain</defName></ThingDef></value>
            </Operation>
        </Patch>"#;

        let ops = walk_test(xml);

        assert!(ops[0].injected_template_names.is_empty());
    }

    /// An operation with no `<value>` child (e.g. `PatchOperationRemove`)
    /// collects no injected types at all.
    #[test]
    fn injected_types_is_empty_with_no_value_child() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationRemove">
                <xpath>Defs/ThingDef[defName="X"]/comps</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].injected_types.is_empty());
        assert!(ops[0].injected_paths.is_empty());
    }

    /// An op nested under a `PatchOperationConditional`'s `<match>` branch
    /// records that Conditional's own `<xpath>` text.
    #[test]
    fn op_under_a_conditional_records_its_enclosing_xpath() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
                <match>
                  <li Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                  </li>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let conditional = ops
            .iter()
            .find(|op| op.class == "PatchOperationConditional")
            .unwrap();
        assert_eq!(conditional.conditional_xpath, None);
        let remove = ops
            .iter()
            .find(|op| op.class == "PatchOperationRemove")
            .unwrap();
        assert_eq!(
            remove.conditional_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="A"]"#)
        );
    }

    /// A nested Conditional replaces the outer one for its own descendants
    /// — "nearest wins", not a stack.
    #[test]
    fn nested_conditional_wins_over_the_outer_one() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="Outer"]</xpath>
                <match>
                  <li Class="PatchOperationConditional">
                    <xpath>Defs/ThingDef[defName="Inner"]</xpath>
                    <match>
                      <li Class="PatchOperationRemove">
                        <xpath>Defs/ThingDef[defName="Inner"]/comps</xpath>
                      </li>
                    </match>
                  </li>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let remove = ops
            .iter()
            .find(|op| op.class == "PatchOperationRemove")
            .unwrap();
        assert_eq!(
            remove.conditional_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="Inner"]"#)
        );
    }

    /// A sibling op outside any Conditional's `<match>`/`<nomatch>` branch
    /// records `None`, even when another top-level operation in the same
    /// file has one.
    #[test]
    fn sibling_op_outside_a_conditional_records_none() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
                <match>
                  <li Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                  </li>
                </match>
              </Operation>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="B"]</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let sibling = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "B"))
            .unwrap();
        assert_eq!(sibling.conditional_xpath, None);
    }

    /// An op nested under a Conditional's `<nomatch>` branch also records
    /// that Conditional's own xpath — `conditional_xpath` carries only the
    /// xpath text, never which branch it came from (see the field's own doc
    /// comment for the polarity caveat that leaves for
    /// `patch_removed_node_edges`).
    #[test]
    fn op_under_a_conditionals_nomatch_branch_also_records_its_enclosing_xpath() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
                <nomatch>
                  <li Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                  </li>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let remove = ops
            .iter()
            .find(|op| op.class == "PatchOperationRemove")
            .unwrap();
        assert_eq!(
            remove.conditional_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="A"]"#)
        );
    }

    /// A `PatchOperationFindMod` nested inside a Conditional's `<match>`
    /// branch does not reset `conditional_xpath` for its own descendants
    /// — only a Conditional overrides the inherited context, so the
    /// FindMod's own `<match>`/`<nomatch>` branches still inherit the
    /// *outer* Conditional's xpath.
    #[test]
    fn find_mod_nested_inside_a_conditional_inherits_its_conditional_xpath() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
                <match>
                  <li Class="PatchOperationFindMod">
                    <mods><li>Some Mod</li></mods>
                    <match Class="PatchOperationRemove">
                      <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                    </match>
                  </li>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let find_mod = ops
            .iter()
            .find(|op| op.class == "PatchOperationFindMod")
            .unwrap();
        assert_eq!(
            find_mod.conditional_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="A"]"#)
        );
        let remove = ops
            .iter()
            .find(|op| op.class == "PatchOperationRemove")
            .unwrap();
        assert_eq!(
            remove.conditional_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="A"]"#)
        );
    }

    /// An op with a `<value>` but no `DefTarget` and a non-`Defs` xpath
    /// (the recognized-head parser couldn't resolve it) contributes no
    /// `injected_paths` at all — neither shape applies.
    #[test]
    fn injected_paths_is_empty_for_an_unresolved_target_and_non_defs_xpath() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>weird xpath with no recognized head</xpath>
                <value>
                  <someElement/>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].target.is_none());
        assert!(ops[0].injected_paths.is_empty());
    }

    /// Element-injection shape: a top-level element inside `<value>` becomes
    /// `<target path>/<element>`.
    #[test]
    fn injected_paths_for_the_element_injection_shape() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="Human"]</xpath>
                <value>
                  <alienRace>
                    <thingSettings/>
                  </alienRace>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/Human/alienRace".to_string()])
        );
    }

    /// The element-injection shape includes the target's own `sub_path`.
    #[test]
    fn injected_paths_for_the_element_injection_shape_with_a_sub_path() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="Human"]/comps</xpath>
                <value>
                  <someElement/>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/Human/comps/someElement".to_string()])
        );
    }

    /// A disjunctive head must record the element-injection path under
    /// *every* def it names, not just the single [`PatchOp::target`] the op
    /// happened to resolve to — otherwise every edge about the second def
    /// silently vanishes.
    #[test]
    fn injected_paths_covers_every_def_a_disjunctive_head_names() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A" or defName="B"]/comps</xpath>
                <value>
                  <someElement/>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from([
                "ThingDef/A/comps/someElement".to_string(),
                "ThingDef/B/comps/someElement".to_string(),
            ])
        );
    }

    /// The element-injection shape's `match_key` (not `display_path`) is what
    /// feeds this field — a `[@Name="X"]` target's own path carries the `@`
    /// marker, distinguishing it from a `[defName="X"]` def sharing the same
    /// literal name (see [`crate::domain::DefTarget::match_key`]'s own doc
    /// comment).
    #[test]
    fn injected_paths_for_the_element_injection_shape_marks_a_name_attr_target() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[@Name="MechGestatorBase"]</xpath>
                <value>
                  <comps>
                    <li/>
                  </comps>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/@MechGestatorBase/comps".to_string()])
        );
    }

    /// An `li`-tagged top-level `<value>` element (the common "add a list
    /// item" shape) still gets no *bare* `li` path segment — a list item
    /// isn't individually addressable by path — but a `Class`-keyed one
    /// (this test's own shape) does get the predicate-keyed path
    /// [`injected_li_predicate_paths_of`] adds alongside
    /// [`injected_paths_of`]'s own element-injection shape.
    #[test]
    fn injected_paths_element_injection_shape_drops_an_li_segment_but_keeps_a_class_keyed_one() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="Human"]/comps</xpath>
                <value>
                  <li Class="SomeComp"/>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(!ops[0].injected_paths.contains("ThingDef/Human/comps/li"));
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/Human/comps/li[@Class=\"SomeComp\"]".to_string()])
        );
    }

    /// `PatchOperationReplace`'s `<value>` is the replacement node itself,
    /// not a genuinely new child — it never contributes an element-injection
    /// entry, even though its top-level tag can otherwise look identical to a
    /// legitimate append.
    #[test]
    fn injected_paths_element_injection_shape_excludes_replace_ops() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="Human"]/alienRace</xpath>
                <value>
                  <alienRace>
                    <thingSettings/>
                  </alienRace>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].injected_paths.is_empty());
    }

    /// `PatchOperationInsert` never contributes an element-injection entry
    /// either — its value is positioned relative to an existing sibling, not
    /// owned by the target.
    #[test]
    fn injected_paths_element_injection_shape_excludes_insert_ops() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationInsert">
                <xpath>Defs/ThingDef[defName="Human"]/comps/li[1]</xpath>
                <order>Before</order>
                <value>
                  <someElement/>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].injected_paths.is_empty());
    }

    /// Whole-def-injection shape: `<xpath>Defs</xpath>` with top-level def
    /// elements in `<value>`, keyed by each one's own `<defName>`.
    #[test]
    fn injected_paths_for_the_whole_def_injection_shape() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs</xpath>
                <value>
                  <ThingDef>
                    <defName>NewThing</defName>
                  </ThingDef>
                  <RecipeDef>
                    <defName>NewRecipe</defName>
                  </RecipeDef>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from([
                "RecipeDef/NewRecipe".to_string(),
                "ThingDef/NewThing".to_string(),
            ])
        );
    }

    /// The leading-slash `/Defs` spelling is recognized too.
    #[test]
    fn injected_paths_recognizes_a_leading_slash_defs_root() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>/Defs</xpath>
                <value>
                  <ThingDef>
                    <defName>NewThing</defName>
                  </ThingDef>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/NewThing".to_string()])
        );
    }

    /// A top-level def element under a whole-`<Defs>` injection with no
    /// `<defName>` child contributes nothing — there's no def identity to
    /// key it by.
    #[test]
    fn injected_paths_skips_a_def_element_with_no_def_name() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs</xpath>
                <value>
                  <ThingDef>
                    <defName>NewThing</defName>
                  </ThingDef>
                  <SomeOtherElement>
                    <label>no defName here</label>
                  </SomeOtherElement>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].injected_paths,
            BTreeSet::from(["ThingDef/NewThing".to_string()])
        );
    }

    #[test]
    fn wrong_root_is_an_error() {
        assert!(matches!(
            walk(b"<NotPatch/>", &test_file()),
            Err(PatchesError::WrongRoot)
        ));
    }

    /// `XmlLocator` round trip on a nested op: re-parsing the same bytes
    /// and walking the recorded element path back down must land on the
    /// same `<li>` operation node.
    #[test]
    fn locator_round_trips_to_the_same_nested_operation_node() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationSequence">
                <operations>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="A"]</xpath>
                  </li>
                  <li Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="B"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let target_op = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "B"))
            .unwrap();

        let text = decode_lossy(xml);
        let doc = roxmltree::Document::parse(&text).unwrap();
        let mut node = doc.root_element();
        for &index in &target_op.locator.element_path {
            node = node
                .children()
                .filter(roxmltree::Node::is_element)
                .nth(index as usize)
                .unwrap();
        }
        assert_eq!(node.tag_name().name(), "li");
        assert_eq!(node.attribute("Class"), Some("PatchOperationReplace"));
    }

    /// `sequence_tail` is threaded straight through a `PatchOperationFindMod`
    /// wrapper nested inside a `PatchOperationSequence`'s own `<operations>`
    /// list: the FindMod's own `<li>` position in that list is what decides
    /// it for every op reached through the FindMod's branches, not the
    /// FindMod's own branch structure.
    #[test]
    fn sequence_tail_flag_is_computed_through_wrappers() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationSequence">
                <operations>
                  <li Class="PatchOperationFindMod">
                    <mods><li>Some Mod</li></mods>
                    <match Class="PatchOperationAdd">
                      <xpath>Defs/ThingDef[defName="A"]</xpath>
                    </match>
                  </li>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="B"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);

        let non_tail = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert!(
            !non_tail.sequence_tail,
            "the FindMod's own <li> is not the last item in the Sequence's operations list"
        );

        let tail = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "B"))
            .unwrap();
        assert!(
            tail.sequence_tail,
            "the last item in the list has no later sibling"
        );
    }

    /// A top-level op (no enclosing Sequence at all) reads `sequence_tail:
    /// true` vacuously — nothing gates it.
    #[test]
    fn sequence_tail_is_vacuously_true_with_no_enclosing_sequence() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A"]</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].sequence_tail);
    }

    /// A Sequence nested inside another Sequence's own non-last item stays
    /// non-tail all the way down, even though the *inner* Sequence's own
    /// single item is trivially last within its own list — every level up
    /// the ancestry must agree, not just the nearest one.
    #[test]
    fn sequence_tail_narrows_through_a_nested_non_tail_sequence() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationSequence">
                <operations>
                  <li Class="PatchOperationSequence">
                    <operations>
                      <li Class="PatchOperationAdd">
                        <xpath>Defs/ThingDef[defName="Inner"]</xpath>
                      </li>
                    </operations>
                  </li>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="Outer"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let inner = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "Inner"))
            .unwrap();
        assert!(
            !inner.sequence_tail,
            "the outer Sequence's own first item (the nested Sequence) is not its last"
        );
    }

    /// `conditional_branch` records which branch of the *nearest*
    /// enclosing Conditional an op is reached through, and
    /// `conditional_nomatch_creates` is stamped onto the `Match` branch's
    /// own ops from that same Conditional's `<nomatch>` content — `true`
    /// when it holds a creating op, `false` on the `NoMatch` branch's own
    /// ops (not meaningful there).
    #[test]
    fn conditional_branch_polarity_is_recorded() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <match Class="PatchOperationReplace">
                  <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                </match>
                <nomatch Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="A"]</xpath>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);

        let match_op = ops
            .iter()
            .find(|op| op.class == "PatchOperationReplace")
            .unwrap();
        assert_eq!(match_op.conditional_branch, Some(ConditionalBranch::Match));
        assert!(
            match_op.conditional_nomatch_creates,
            "the sibling <nomatch> branch holds a PatchOperationAdd"
        );

        let nomatch_op = ops
            .iter()
            .find(|op| op.class == "PatchOperationAdd")
            .unwrap();
        assert_eq!(
            nomatch_op.conditional_branch,
            Some(ConditionalBranch::NoMatch)
        );
    }

    /// A `<nomatch>` branch with no creating op (only a `Remove`, say)
    /// leaves `conditional_nomatch_creates` `false` on the `Match`
    /// branch's own ops.
    #[test]
    fn conditional_nomatch_creates_is_false_without_a_creating_op() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationConditional">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <match Class="PatchOperationReplace">
                  <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                </match>
                <nomatch Class="PatchOperationRemove">
                  <xpath>Defs/ThingDef[defName="A"]/otherThing</xpath>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let match_op = ops
            .iter()
            .find(|op| op.class == "PatchOperationReplace")
            .unwrap();
        assert!(!match_op.conditional_nomatch_creates);
    }

    /// A plain op naming exactly one def reads `names_single_def: true`;
    /// a disjunctive head naming two reads `false` on every op it
    /// produces.
    #[test]
    fn names_single_def_is_false_for_a_disjunctive_head() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A" or defName="B"]/comps</xpath>
                <value><li>x</li></value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(!ops[0].names_single_def);
    }

    #[test]
    fn names_single_def_is_true_for_a_plain_head() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value><li>x</li></value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].names_single_def);
    }

    /// A third-party `PatchOperationAddOrReplace` targeting the def root
    /// records its own `<value>`'s top-level element name — the real
    /// compat-mod shape `remover_recreates_region` reads.
    #[test]
    fn value_child_names_records_add_or_replace_targeting_the_def_root() {
        let xml = br#"<Patch>
              <Operation Class="RR.PatchOperationAddOrReplace">
                <xpath>Defs/ThingDef[defName="FloodLight"]</xpath>
                <value>
                  <researchPrerequisites Inherit="False">
                    <li>ColoredLights</li>
                  </researchPrerequisites>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(
            ops[0].value_child_names,
            BTreeSet::from(["researchPrerequisites".to_string()])
        );
    }

    /// `value_child_names` is empty for a class outside
    /// `VALUE_CHILD_SUFFIXES` — a plain `PatchOperationReplace`'s own
    /// `<value>` is the replaced node itself, not a genuinely new child,
    /// the same reasoning `injected_paths_of` already applies.
    #[test]
    fn value_child_names_is_empty_for_a_replace() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value><comps/></value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].value_child_names.is_empty());
    }

    // -- toggle_active / ToggleShape --------------------------------------

    /// A sequence-shaped custom toggle (`<operations>` child, no `<xpath>`
    /// of its own) with no declared `<enabled>`/`<defaultValue>`/`<default>`
    /// defaults to active, per [`toggle_default`] — every op inside stays
    /// `toggle_active: true`.
    #[test]
    fn toggle_shaped_sequence_defaults_active_with_no_declared_default() {
        let xml = br#"<Patch>
              <Operation Class="Example.ToggleMod.SequenceToggle">
                <operations>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="A"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let leaf = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert!(leaf.toggle_active);
    }

    /// The same sequence-shaped custom toggle, declared `<enabled>False</enabled>`
    /// — every op inside is marked unreachable.
    #[test]
    fn toggle_shaped_sequence_marks_children_inactive_when_declared_off() {
        let xml = br#"<Patch>
              <Operation Class="Example.ToggleMod.SequenceToggle">
                <enabled>False</enabled>
                <operations>
                  <li Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="A"]</xpath>
                  </li>
                </operations>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let leaf = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert!(!leaf.toggle_active);
    }

    /// A Conditional-shaped custom toggle (no `<xpath>` of its own, a
    /// `<match>`/`<nomatch>` pair instead) declared `<default>false</default>`
    /// marks its `match` branch inactive and its `nomatch` branch active —
    /// the toggle picks `nomatch` the way a real Conditional picks a
    /// branch by node existence.
    #[test]
    fn toggle_shaped_conditional_picks_nomatch_branch_when_declared_off() {
        let xml = br#"<Patch>
              <Operation Class="Example.ToggleMod.ConditionalToggle">
                <default>false</default>
                <match Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="A"]</xpath>
                </match>
                <nomatch Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="B"]</xpath>
                </nomatch>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let matched = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        let unmatched = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "B"))
            .unwrap();
        assert!(!matched.toggle_active);
        assert!(unmatched.toggle_active);
    }

    /// A Conditional-shaped custom toggle with a single `<operation>`
    /// child (rather than `<match>`/`<nomatch>`) runs that operation only
    /// when its own default resolves it on.
    #[test]
    fn toggle_shaped_conditional_single_operation_child_gated_by_default() {
        let xml = br#"<Patch>
              <Operation Class="Example.ToggleMod.ConditionalToggle">
                <defaultValue>true</defaultValue>
                <operation Class="PatchOperationAdd">
                  <xpath>Defs/ThingDef[defName="A"]</xpath>
                </operation>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let leaf = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        assert!(leaf.toggle_active);
    }

    /// A nested toggle (a sequence-shaped toggle default-on, itself inside
    /// a Conditional-shaped toggle default-off) narrows `toggle_active` at
    /// both levels, ANDed together.
    #[test]
    fn nested_toggles_and_their_toggle_active_narrowing() {
        let xml = br#"<Patch>
              <Operation Class="Example.ToggleMod.ConditionalToggle">
                <default>false</default>
                <match Class="Example.ToggleMod.SequenceToggle">
                  <operations>
                    <li Class="PatchOperationAdd">
                      <xpath>Defs/ThingDef[defName="A"]</xpath>
                    </li>
                  </operations>
                </match>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let leaf = ops
            .iter()
            .find(|op| op.target.as_ref().is_some_and(|t| t.def_name == "A"))
            .unwrap();
        // The outer Conditional-toggle defaults off, so its `match`
        // branch (and everything inside, including the inner
        // sequence-toggle's own default-on children) is unreachable.
        assert!(!leaf.toggle_active);
    }

    #[test]
    fn value_root_names_records_every_top_level_value_child_in_order() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value>
                  <comps><li Class="Example.CompFirst" /><li Class="Example.CompSecond" /></comps>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert_eq!(ops[0].value_root_names, vec!["comps".to_string()]);
    }

    #[test]
    fn value_root_names_is_empty_for_an_operation_with_no_value() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationRemove">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].value_root_names.is_empty());
    }

    #[test]
    fn value_digest_records_one_entry_per_element_with_a_stable_hash() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value>
                  <li Class="Example.CompOne"><amount>5</amount></li>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let digest = ops[0].value_digest.as_ref().unwrap();
        assert!(!digest.truncated);
        let paths: std::collections::BTreeSet<&str> = digest
            .entries
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        assert_eq!(paths, std::collections::BTreeSet::from(["li", "li/amount"]));
        // Re-walking the identical XML twice must hash identically —
        // required for the byte-identical-report invariant.
        let ops_again = walk_test(xml);
        assert_eq!(ops[0].value_digest, ops_again[0].value_digest);
    }

    #[test]
    fn value_digest_of_two_differently_valued_li_siblings_differ() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value>
                  <li Class="Example.CompOne" />
                  <li Class="Example.CompTwo" />
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        let digest = ops[0].value_digest.as_ref().unwrap();
        // Both entries share the path "li" (position is never part of the
        // path) but carry different content hashes, so both survive in
        // the set.
        let li_entries: Vec<u64> = digest
            .entries
            .iter()
            .filter(|(path, _)| path == "li")
            .map(|(_, hash)| *hash)
            .collect();
        assert_eq!(li_entries.len(), 2);
        assert_ne!(li_entries[0], li_entries[1]);
    }

    #[test]
    fn value_digest_is_none_for_an_operation_with_no_value() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationRemove">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(ops[0].value_digest.is_none());
    }

    /// A real-install shape (see
    /// `analysis::edges::patches::li_predicate_lookup_key`'s own doc
    /// comment): an `Add` injecting a `Class`-keyed `<li>` records a
    /// predicate-keyed path, not just the unaddressable bare `li`
    /// segment [`injected_paths_of`] itself drops.
    #[test]
    fn injected_paths_records_a_class_keyed_li_an_add_injects() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/PreceptDef[defName="A"]/comps</xpath>
                <value>
                  <li Class="Example.RoomRequirement_ThingAnyOfCount" />
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(
            ops[0].injected_paths.contains(
                "PreceptDef/A/comps/li[@Class=\"Example.RoomRequirement_ThingAnyOfCount\"]"
            )
        );
    }

    /// Another real-install shape: a `Replace`
    /// that keeps the replaced node's own tag (here, `researchPrerequisites`)
    /// records its own bare-text `<li>` children as predicate-keyed paths
    /// too — the "genuinely still injects content" case
    /// `replace_discards_addition`'s own "keeps the node" rule also relies
    /// on.
    #[test]
    fn injected_paths_records_a_text_keyed_li_a_node_keeping_replace_injects() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="A"]/researchPrerequisites</xpath>
                <value>
                  <researchPrerequisites><li>SomeResearch</li></researchPrerequisites>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(
            ops[0]
                .injected_paths
                .contains("ThingDef/A/researchPrerequisites/li[text()=\"SomeResearch\"]")
        );
    }

    /// A `Replace` whose value does **not** keep the replaced node's own
    /// tag (a rename) must record no predicate-keyed `<li>` path at all —
    /// the node it injects into isn't the one anything downstream would
    /// select by the old path.
    #[test]
    fn injected_paths_records_nothing_for_a_replace_that_renames_the_node() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="A"]/researchPrerequisites</xpath>
                <value>
                  <renamedList><li>SomeResearch</li></renamedList>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(
            !ops[0]
                .injected_paths
                .iter()
                .any(|path| path.contains("li["))
        );
    }

    /// A real-install shape: a `Replace` whose own target is itself a
    /// predicate-keyed `<li>` (not a plain container) swaps in a *new*
    /// `<li>` carrying a different identity at the same list position —
    /// recorded one level up, under the new identity, never the replaced
    /// one (a selector only ever reads the document after the replace
    /// runs).
    #[test]
    fn injected_paths_records_a_replaced_predicate_lis_new_identity() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/PreceptDef[defName="A"]/comps/li[@Class="Example.RoomRequirement_ThingCount"][thingDef="Column"]</xpath>
                <value>
                  <li Class="Example.RoomRequirement_ThingAnyOfCount">
                    <things><li>Column</li></things>
                  </li>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(
            ops[0].injected_paths.contains(
                "PreceptDef/A/comps/li[@Class=\"Example.RoomRequirement_ThingAnyOfCount\"]"
            )
        );
        assert!(
            !ops[0]
                .injected_paths
                .iter()
                .any(|path| path.contains("RoomRequirement_ThingCount\"]"))
        );
    }

    /// A bare `<li>` with no `Class` attribute and no text (or with
    /// element children of its own) has no identifiable predicate key,
    /// so it contributes no predicate-keyed path — silently skipped
    /// rather than guessed at.
    #[test]
    fn injected_paths_skips_an_li_with_no_identifiable_key() {
        let xml = br#"<Patch>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="A"]/comps</xpath>
                <value>
                  <li><nested>structured content</nested></li>
                </value>
              </Operation>
            </Patch>"#;
        let ops = walk_test(xml);
        assert!(
            !ops[0]
                .injected_paths
                .iter()
                .any(|path| path.contains("li["))
        );
    }

    /// Builds `<a><a>...leaf...</a></a>` nested `levels` deep.
    fn nested_element(levels: usize) -> String {
        let mut inner = "leaf".to_string();
        for _ in 0..levels {
            inner = format!("<a>{inner}</a>");
        }
        inner
    }

    /// A mod-provided `<value>` nested past [`MAX_CONTENT_HASH_DEPTH`] (64)
    /// but still well under [`MAX_RAW_ELEMENT_DEPTH`] (512, so the document
    /// still parses) must never overflow the stack while hashing it for
    /// [`PatchOp::value_digest`], and must come back `truncated: true`
    /// rather than silently hashing only part of the tree with no signal.
    /// Run on a 1 MiB stack (the CLI's own main thread size), the same
    /// proof shape as `xpath_expr_tests`'s
    /// `a_pathological_predicate_of_every_shape_fails_cleanly_on_a_small_stack`.
    #[test]
    fn a_deeply_nested_value_hashes_without_overflowing_a_small_stack() {
        let value = nested_element(200);
        let xml = format!(
            r#"<Patch>
                  <Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="X"]/comps</xpath>
                    <value>{value}</value>
                  </Operation>
                </Patch>"#
        );

        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let ops = walk_test(xml.as_bytes());
                let digest = ops[0].value_digest.as_ref().expect("op has a <value>");
                assert!(
                    digest.truncated,
                    "a value nested past MAX_CONTENT_HASH_DEPTH must report truncated"
                );
            })
            .expect("spawning the probe thread")
            .join()
            .expect("a deeply nested <value> overflowed a 1 MiB stack while hashing");
    }

    /// Builds `levels` levels of `<Operation Class="PatchOperationSequence">`
    /// each nesting the next inside its own `<operations><li>`.
    fn nested_sequences(levels: usize) -> String {
        let mut inner =
            r#"<li Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="X"]</xpath></li>"#
                .to_string();
        for _ in 0..levels {
            inner = format!(
                r#"<li Class="PatchOperationSequence"><operations>{inner}</operations></li>"#
            );
        }
        format!(
            r#"<Patch><Operation Class="PatchOperationSequence"><operations>{inner}</operations></Operation></Patch>"#
        )
    }

    /// 100 nested `PatchOperationSequence`s (raw element depth well under
    /// [`MAX_RAW_ELEMENT_DEPTH`], so the document parses) still passes
    /// [`MAX_PATCH_TREE_DEPTH`] (64) — [`walk_operation`]/[`walk_branch`]'s
    /// mutual recursion must stop there rather than keep descending, and
    /// report the tree was truncated rather than silently under-scanning
    /// with no signal. Run on a 1 MiB stack, same proof shape as the
    /// value-nesting test above.
    #[test]
    fn deeply_nested_sequences_are_capped_not_crashed() {
        let xml = nested_sequences(100);

        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let (ops, _ref_sites, tree_depth_truncated) =
                    walk_with_ref_sites(xml.as_bytes(), &test_file())
                        .expect("well under MAX_RAW_ELEMENT_DEPTH, so this must still parse");
                assert!(
                    !ops.is_empty(),
                    "at least the outermost operations must still be recorded"
                );
                assert!(
                    tree_depth_truncated,
                    "nesting past MAX_PATCH_TREE_DEPTH must report truncated"
                );
            })
            .expect("spawning the probe thread")
            .join()
            .expect("100 nested sequences overflowed a 1 MiB stack while walking");
    }

    /// The actual reported attack shape — a ~300 KB Workshop patch with
    /// 50,000 nested elements — must never even reach
    /// `roxmltree::Document::parse`, whose own recursive-descent parser
    /// overflows a 1 MiB stack well before either of this module's
    /// post-parse depth caps (`MAX_CONTENT_HASH_DEPTH`,
    /// `MAX_PATCH_TREE_DEPTH`) would ever get a chance to run — measured
    /// directly: bare element nesting this deep overflows the parse call
    /// itself. [`raw_element_nesting_exceeds`]'s pre-parse scan is what
    /// actually stands between this input and a crash; without it, this
    /// test aborts the whole test process rather than failing an
    /// assertion.
    #[test]
    fn pathologically_deep_patch_xml_is_rejected_before_parsing_not_crashed() {
        let value = nested_element(50_000);
        let xml = format!(
            r#"<Patch>
                  <Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="X"]/comps</xpath>
                    <value>{value}</value>
                  </Operation>
                </Patch>"#
        );

        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let Err(err) = walk_with_ref_sites(xml.as_bytes(), &test_file()) else {
                    panic!("50,000 levels of nesting must be refused, not parsed");
                };
                assert!(
                    matches!(err, PatchesError::TooDeep(depth) if depth == MAX_RAW_ELEMENT_DEPTH),
                    "expected TooDeep({MAX_RAW_ELEMENT_DEPTH}), got {err:?}"
                );
            })
            .expect("spawning the probe thread")
            .join()
            .expect("a pathologically deep document overflowed a 1 MiB stack instead of being rejected cleanly");
    }
}
