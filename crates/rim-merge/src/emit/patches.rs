//! Rendering `Patches/rimmerge_<DefType>.xml`: xpaths, operations, and plan blocks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rim_analyzer::domain::Selector;

use super::about::join_ids;
use crate::plan::{DefKey, MergePlan, PlanOp};
use crate::tree::{Content, FieldNode, FieldPath, ItemId, PathSegment};
use crate::xml::{escape_text, render_node};

/// Picks `"` unless `value` itself contains one, in which case `'` —
/// [`crate::plan::plan_def_override`]/[`crate::plan::plan_patch_collision`]
/// already refuse (unresolved, [`crate::plan::Caveat::UnsafeXpathValue`])
/// any field whose identity value contains *both*, so exactly one of the
/// two always works here.
fn quote(value: &str) -> String {
    let q = if value.contains('"') { '\'' } else { '"' };
    format!("{q}{value}{q}")
}

/// Renders one xpath segment's own text, **unescaped** — this is XML
/// *content* (it goes inside an `<xpath>` element, not an attribute), so it
/// must be escaped exactly once, by the generic `Content::Text` renderer in
/// [`crate::xml::render_node`] that ultimately serializes it. Escaping here
/// too (with `escape_attr`) would double-escape every `&`/`<`/`>`/`"` a
/// value happens to contain (`Tom & Jerry` -> `&amp;amp; Jerry` instead of
/// `&amp; Jerry`).
fn render_xpath_segment(segment: &PathSegment) -> String {
    match segment {
        PathSegment::Child(tag) => tag.clone(),
        PathSegment::Item(item_id) => match item_id {
            ItemId::Class(class) => format!("li[@Class={}]", quote(class)),
            ItemId::Key { child, value } => format!("li[{child}={}]", quote(value)),
            ItemId::Text(text) => format!("li[text()={}]", quote(text)),
            ItemId::Position(position) => format!("li[{}]", position + 1),
        },
    }
}

/// Renders `Defs/<def_type>[defName="<def_name>"]` (or `[@Name=...]` for
/// [`Selector::NameAttr`]) followed by every segment of `path` — the only
/// shapes the replay's xpath grammar can parse back (the closure test
/// depends on this). See [`render_xpath_segment`] for why this text is
/// left unescaped.
pub(super) fn render_xpath(key: &DefKey, selector: Selector, path: &FieldPath) -> String {
    let predicate = match selector {
        Selector::DefName => "defName",
        Selector::NameAttr => "@Name",
    };
    let mut xpath = format!(
        "Defs/{}[{predicate}={}]",
        key.def_type,
        quote(&key.def_name)
    );
    for segment in path.segments() {
        xpath.push('/');
        xpath.push_str(&render_xpath_segment(segment));
    }
    xpath
}

fn xpath_node(xpath: String) -> FieldNode {
    FieldNode {
        tag: "xpath".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text(xpath),
    }
}

fn value_node(inner: FieldNode) -> FieldNode {
    FieldNode {
        tag: "value".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(vec![inner]),
    }
}

fn text_child(tag: &str, text: String) -> FieldNode {
    FieldNode {
        tag: tag.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text(text),
    }
}

fn build_li(class: &str, children: Vec<FieldNode>) -> FieldNode {
    let mut attrs = BTreeMap::new();
    attrs.insert("Class".to_string(), class.to_string());
    FieldNode {
        tag: "li".to_string(),
        attrs,
        content: Content::Children(children),
    }
}

/// Renders one [`PlanOp`] as its `<li Class="...">` element. `ReplaceInheritFalse`
/// and `Replace` both emit `PatchOperationReplace`; `Add` emits
/// `PatchOperationAdd` regardless of whether the added node carries
/// `Inherit="False"` (that attribute already lives on the node itself —
/// see [`crate::plan::PlanOp::Add`]'s doc comment for why no separate
/// rendering path is needed for the drop-an-inherited-item case).
fn render_op(key: &DefKey, selector: Selector, op: &PlanOp) -> FieldNode {
    match op {
        PlanOp::Replace { path, node } | PlanOp::ReplaceInheritFalse { path, node } => build_li(
            "PatchOperationReplace",
            vec![
                xpath_node(render_xpath(key, selector, path)),
                value_node(node.clone()),
            ],
        ),
        PlanOp::Add { parent, node } => build_li(
            "PatchOperationAdd",
            vec![
                xpath_node(render_xpath(key, selector, parent)),
                value_node(node.clone()),
            ],
        ),
        PlanOp::Remove { path } => build_li(
            "PatchOperationRemove",
            vec![xpath_node(render_xpath(key, selector, path))],
        ),
        PlanOp::SetAttribute { path, name, value } => build_li(
            "PatchOperationAttributeSet",
            vec![
                xpath_node(render_xpath(key, selector, path)),
                text_child("attribute", name.clone()),
                text_child("value", value.clone()),
            ],
        ),
    }
}

/// One def's plan rendered as one `<!-- ... --> <Operation
/// Class="PatchOperationSequence">` block, or `None` for a plan with no
/// ops (an `Agreeing` patch collision, or a def-override merge that
/// turned out to need no changes) — nothing is emitted for it at all.
///
/// **Every `<li>` in `<operations>` carries its own op's full
/// `depends_on` as its own `MayRequire`, never the wrapping `Operation`.**
/// Ground-truthed against the decompiled engine: `ModContentPack
/// .LoadPatches` builds a top-level `<Operation>` via `DirectXmlToObject
/// .ObjectFromXml<PatchOperation>`, which never reads `MayRequire` at
/// all — only `DirectXmlToObject.ListFromXml`, the reader for a
/// `PatchOperationSequence`'s own `<operations>` list, actually honours
/// the attribute on each item. A gate placed on the wrapper instead (the
/// shape this function used before) is silently never read by the game,
/// so every generated patch's dependency gate would be inert — the whole
/// sequence would run even when none of its dependencies are active. An
/// op whose `depends_on` is empty (winner is Core, no other contributor —
/// `PlannedOp::depends_on`'s own contract) needs no gate at all, so its
/// `<li>` carries none.
fn render_plan_block(plan: &MergePlan) -> Option<String> {
    if plan.ops.is_empty() {
        return None;
    }

    let operations: Vec<FieldNode> = plan
        .ops
        .iter()
        .map(|planned| {
            let mut li = render_op(&plan.key, plan.selector, &planned.op);
            if !planned.depends_on.is_empty() {
                li.attrs
                    .insert("MayRequire".to_string(), join_ids(&planned.depends_on));
            }
            li
        })
        .collect();

    let operations_node = FieldNode {
        tag: "operations".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(operations),
    };

    let mut sequence_attrs = BTreeMap::new();
    sequence_attrs.insert("Class".to_string(), "PatchOperationSequence".to_string());
    let operation_node = FieldNode {
        tag: "Operation".to_string(),
        attrs: sequence_attrs,
        content: Content::Children(vec![operations_node]),
    };

    let owners = join_ids(&plan.owners.iter().cloned().collect());
    let mut block = String::new();
    block.push_str(&format!(
        "  <!-- {} — owners: {} -->\n",
        escape_text(&plan.key.to_string()),
        escape_text(&owners)
    ));
    block.push_str(&render_node(&operation_node, 1));
    Some(block)
}

/// The relative path a `<def_type>`'s patch operations render into —
/// `Patches/rimmerge_<DefType>.xml`, per def type. Exposed publicly so a
/// caller that needs
/// to *predict* which file a plan's ops will land in (e.g. `rim-session`'s
/// merge-mod entry listing) reads it from here rather than re-deriving
/// the same naming scheme independently, which could silently drift from
/// what [`render`](crate::emit::render) itself actually writes.
#[must_use]
pub fn patch_file_path(def_type: &str) -> PathBuf {
    PathBuf::from(format!("Patches/rimmerge_{def_type}.xml"))
}

pub(super) fn render_patches_file(plans: &[&MergePlan]) -> Option<String> {
    // Deterministic, readable output regardless of the caller's own
    // iteration order.
    let mut sorted = plans.to_vec();
    sorted.sort_by(|a, b| a.key.cmp(&b.key));

    let blocks: Vec<String> = sorted
        .iter()
        .filter_map(|plan| render_plan_block(plan))
        .collect();
    if blocks.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<Patch>\n");
    for block in blocks {
        out.push_str(&block);
    }
    out.push_str("</Patch>\n");
    Some(out)
}
