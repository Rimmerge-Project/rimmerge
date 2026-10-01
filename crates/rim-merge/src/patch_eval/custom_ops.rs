//! Data-driven custom behaviours: set/add-or-replace mod extensions, gate skips,
//! research-coordinate replacement.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use roxmltree::Node;

use super::dispatch::unsupported;
use super::identity::child_text;
use super::select::{Selected, matched_highest_index_first, resolve_element_selection};
use super::standard_ops::{
    apply_add, apply_add_mod_extension, apply_attribute_op, apply_insert, apply_remove,
    apply_replace, apply_set_name,
};
use super::tree::{ensure_children, value_children};
use super::{ReplayContext, ReplayError, ReplayLog};
use crate::patch_behaviours::{ClassGate, ConditionalKind, CustomBehaviour, GateBehaviour};
use crate::tree::{Content, FieldNode, FieldTree, get_mut};

/// Runs the [`CustomBehaviour`] `class` maps onto — the dispatch half of
/// [`ReplayContext::behaviours`]. Every behaviour is a closed enum
/// variant with its own handler below, so a data file can only ever
/// select something this binary already implements.
pub(super) fn apply_custom_behaviour(
    behaviour: CustomBehaviour,
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
) -> Result<bool, ReplayError> {
    match behaviour {
        CustomBehaviour::SetModExtension => {
            apply_set_mod_extension(class, node, tree, context, mod_id, log)
        }
        CustomBehaviour::AddOrReplace => {
            apply_add_or_replace(class, node, tree, context, mod_id, log)
        }
        CustomBehaviour::ReplaceResearchCoords => {
            apply_replace_research_coords(class, node, tree, context, mod_id, log)
        }
    }
}

/// [`CustomBehaviour::SetModExtension`]: each `<value>` `li`
/// replaces the `modExtensions` item carrying the same `Class`, or is
/// appended when there is none. `modExtensions` is created if absent —
/// the same container handling `PatchOperationAddModExtension` does.
fn apply_set_mod_extension(
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
) -> Result<bool, ReplayError> {
    let xpath_text = child_text(node, "xpath")
        .ok_or_else(|| unsupported("", format!("{class} missing <xpath>")))?;
    let matched = match resolve_element_selection(tree, context, &xpath_text, class, log, mod_id)? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let values = value_children(node);
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            let items = ensure_children(target, "modExtensions");
            for value in &values {
                let same_class = value.attrs.get("Class").and_then(|class| {
                    items
                        .iter()
                        .position(|item| item.attrs.get("Class") == Some(class))
                });
                match same_class {
                    Some(index) => items[index] = value.clone(),
                    None => items.push(value.clone()),
                }
            }
        }
    }
    Ok(true)
}

/// [`CustomBehaviour::AddOrReplace`]: the xpath names a *container* (very
/// often the def node itself), and each `<value>` child replaces the
/// container's existing child of the same tag, or is appended when there
/// is none.
///
/// That reading — rather than "replace the node the xpath names" — is
/// what every real use of this behaviour on the reference install looks
/// like: `<xpath>Defs/ResearchProjectDef[defName="X"]</xpath>` with
/// `<value><prerequisites>…</prerequisites></value>` can only mean "set
/// this def's `prerequisites`", never "replace the whole def with a
/// `prerequisites` element". `<ignoreAttributesWhenMatching>`, which some
/// callers set, is a no-op here: children are matched by tag alone.
///
/// **Deliberately not gated**: this handler never calls [`gate_skips`];
/// only [`apply_replace_research_coords`] runs the gate. The data file's
/// gate row matches a whole class-name *prefix*, and its own evidence line
/// claims every class under that prefix runs the gate, so if that evidence
/// is right this behaviour is under-gated. Closing the gap is a
/// **measured** change — it would alter real replay outcomes on an
/// unbounded set of real operations — not something to fold into a data
/// change.
fn apply_add_or_replace(
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
) -> Result<bool, ReplayError> {
    let xpath_text = child_text(node, "xpath")
        .ok_or_else(|| unsupported("", format!("{class} missing <xpath>")))?;
    let matched = match resolve_element_selection(tree, context, &xpath_text, class, log, mod_id)? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let values = value_children(node);
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            add_or_replace_children(target, &values);
        }
    }
    Ok(true)
}

/// Each of `values` replaces `target`'s existing child of the same tag,
/// or is appended when `target` has none — see [`apply_add_or_replace`].
fn add_or_replace_children(target: &mut FieldNode, values: &[FieldNode]) {
    if values.is_empty() {
        return;
    }
    if !matches!(target.content, Content::Children(_)) {
        target.content = Content::Children(Vec::new());
    }
    let Content::Children(children) = &mut target.content else {
        unreachable!("just ensured Content::Children above")
    };
    for value in values {
        match children.iter().position(|child| child.tag == value.tag) {
            Some(index) => children[index] = value.clone(),
            None => children.push(value.clone()),
        }
    }
}

/// The [`GateBehaviour::ModsLoadedGate`] a framework runs before its
/// own operation's worker — IL-verified against the one real framework
/// this behaviour was modelled from, whose class-name pattern and element
/// vocabulary are now data ([`ClassGate`]). `true` means the operation is
/// skipped outright (RimWorld never constructs it, the same "no-op, not a
/// failure" outcome as a `MayRequire` gate that isn't met).
///
/// [`GateFields::requires_all`](crate::patch_behaviours::GateFields::requires_all)
/// must name only active mods; the
/// [`GateFields::conditional_type`](crate::patch_behaviours::GateFields::conditional_type)
/// element selects one of the gate's own declared
/// [`ConditionalKind`]s against the active set (matched by suffix, so a
/// namespace-qualified value still resolves). A `conditionalType` the
/// gate's data does *not* declare stays `Unsupported` rather than
/// guessing: such a value can read the user's own mod settings, which
/// this replay has no access to.
fn mods_loaded_gate_skips(
    node: Node,
    context: &ReplayContext<'_>,
    gate: &ClassGate,
    class: &str,
    xpath_text: &str,
) -> Result<bool, ReplayError> {
    let ids = |text: String| -> Vec<ModId> {
        text.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ModId::new)
            .collect()
    };
    if let Some(required) = child_text(node, &gate.fields.requires_all)
        && !ids(required)
            .iter()
            .all(|id| context.active_mods.contains(id))
    {
        return Ok(true);
    }
    let Some(conditional_type) = child_text(node, &gate.fields.conditional_type) else {
        return Ok(false);
    };
    let named = ids(child_text(node, &gate.fields.conditional_param).unwrap_or_default());
    let kind = gate
        .conditional_types
        .iter()
        .find(|(declared, _)| conditional_type.ends_with(declared.as_str()))
        .map(|(_, kind)| *kind);
    match kind {
        Some(ConditionalKind::AllLoaded) => {
            Ok(!named.iter().all(|id| context.active_mods.contains(id)))
        }
        Some(ConditionalKind::NoneLoaded) => {
            Ok(named.iter().any(|id| context.active_mods.contains(id)))
        }
        None => Err(unsupported(
            xpath_text,
            format!(
                "{class} with an unmodelled <{}> {conditional_type}",
                gate.fields.conditional_type
            ),
        )),
    }
}

/// Whether `class`'s own gate (if it has one) says this operation is
/// skipped outright. No gate declared for `class` means "not skipped" —
/// the same answer a gate whose conditions are all met gives.
fn gate_skips(
    node: Node,
    context: &ReplayContext<'_>,
    class: &str,
    xpath_text: &str,
) -> Result<bool, ReplayError> {
    let Some(gate) = context.behaviours.gate_for(class) else {
        return Ok(false);
    };
    match gate.behaviour {
        GateBehaviour::ModsLoadedGate => {
            mods_loaded_gate_skips(node, context, gate, class, xpath_text)
        }
    }
}

/// Sets `target`'s `tag` child's text, appending a new child when absent
/// — [`apply_replace_research_coords`]'s own field-level primitive.
fn set_child_text(target: &mut FieldNode, tag: &str, text: &str) {
    if !matches!(target.content, Content::Children(_)) {
        target.content = Content::Children(Vec::new());
    }
    let Content::Children(children) = &mut target.content else {
        unreachable!("just ensured Content::Children above")
    };
    match children.iter_mut().find(|child| child.tag == tag) {
        Some(child) => child.content = Content::Text(text.to_string()),
        None => children.push(FieldNode {
            tag: tag.to_string(),
            attrs: BTreeMap::new(),
            content: Content::Text(text.to_string()),
        }),
    }
}

/// [`CustomBehaviour::ReplaceResearchCoords`] (IL-verified against
/// the framework it was modelled from): past its class's own gate
/// ([`gate_skips`]), sets the text of `researchViewX`/`researchViewY` on
/// every node the xpath matches, appending either child when the matched
/// node doesn't already have it. Every other attribute of the matched
/// node is left untouched.
fn apply_replace_research_coords(
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
) -> Result<bool, ReplayError> {
    let xpath_text = child_text(node, "xpath")
        .ok_or_else(|| unsupported("", format!("{class} missing <xpath>")))?;
    if gate_skips(node, context, class, &xpath_text)? {
        return Ok(true);
    }
    let matched = match resolve_element_selection(tree, context, &xpath_text, class, log, mod_id)? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let x = child_text(node, "researchViewX")
        .ok_or_else(|| unsupported(&xpath_text, format!("{class} without <researchViewX>")))?;
    let y = child_text(node, "researchViewY")
        .ok_or_else(|| unsupported(&xpath_text, format!("{class} without <researchViewY>")))?;
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            set_child_text(target, "researchViewX", &x);
            set_child_text(target, "researchViewY", &y);
        }
    }
    Ok(true)
}

pub(super) fn apply_mutation(
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
) -> Result<bool, ReplayError> {
    let xpath_text = child_text(node, "xpath")
        .ok_or_else(|| unsupported("", format!("{class} missing <xpath>")))?;

    if class.ends_with("PatchOperationAdd") {
        return apply_add(node, tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationInsert") {
        return apply_insert(node, tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationRemove") {
        return apply_remove(tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationReplace") {
        return apply_replace(node, tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationAttributeAdd")
        || class.ends_with("PatchOperationAttributeSet")
        || class.ends_with("PatchOperationAttributeRemove")
    {
        return apply_attribute_op(class, node, tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationSetName") {
        return apply_set_name(node, tree, context, &xpath_text, log, mod_id);
    }
    if class.ends_with("PatchOperationAddModExtension") {
        return apply_add_mod_extension(node, tree, context, &xpath_text, log, mod_id);
    }

    Err(unsupported(
        &xpath_text,
        format!("unsupported operation class: {class}"),
    ))
}
