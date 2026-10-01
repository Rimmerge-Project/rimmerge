//! The standard `PatchOperation*` classes: add, insert, remove, replace, attribute ops, `SetName`,
//! `AddModExtension`.

use rim_analyzer::domain::{ModId, Selector};
use roxmltree::Node;

use super::dispatch::unsupported;
use super::identity::{child_text, direct_child};
use super::select::{
    Selected, XPathResolution, matched_highest_index_first, matches_def_root, reject_root_target,
    resolve_element_selection, resolve_selection, resolve_xpath,
};
use super::tree::{ensure_children, insert_sibling, replace_at, value_children};
use super::{ReplayContext, ReplayError, ReplayLog};
use crate::plan::Caveat;
use crate::tree::{Content, FieldTree, get_mut, remove_at};

pub(super) fn apply_add(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    if try_recreate_def_from_document_root_add(node, tree, context, xpath_text)? {
        return Ok(true);
    }
    let matched = match resolve_element_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationAdd",
        log,
        mod_id,
    )? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let prepend =
        child_text(node, "order").is_some_and(|order| order.eq_ignore_ascii_case("prepend"));
    let values = value_children(node);
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            match &mut target.content {
                Content::Children(children) => {
                    if prepend {
                        for (offset, value) in values.iter().cloned().enumerate() {
                            children.insert(offset, value);
                        }
                    } else {
                        children.extend(values.iter().cloned());
                    }
                }
                Content::Empty => target.content = Content::Children(values.clone()),
                Content::Text(_) => {}
            }
        }
    }
    Ok(true)
}

/// A whole-`<Defs>`-root `PatchOperationAdd` whose own `<value>` injects a
/// fresh top-level def matching the def actually being replayed
/// (`context.def_type`/`def_name`/`selector`) genuinely recreates it —
/// RimWorld's own combined document gains a brand-new node under that
/// same identity, indistinguishable from any other def, once an earlier
/// contribution's own whole-def `Remove` left `tree.root` empty. The
/// ordinary document-root path (`Selected::Done(true)`, "succeeded
/// elsewhere") is correct only while the def is still genuinely absent —
/// or genuinely a *different* def's own injection, which this check
/// already excludes by matching identity first.
///
/// Gated on `tree.root.content` already being [`Content::Empty`]:
/// this replay only ever holds the *winning* owner's own tree
/// (`rim-merge/CLAUDE.md`'s own "losers' nodes are absent" assumption), so
/// a *present* tree can only mean this op's own value is a genuine
/// `Mod X has multiple <T>s named Y. Skipping.` duplicate of an
/// already-loaded def, never a real recreate — recreating over live
/// content would silently discard whatever ran between the `Remove` and
/// here. Real install shape this fixes: a mod removes a whole def, then
/// injects a brand-new one of the identical name later in the *same*
/// patch file — without this, a later mod's own op on that def
/// (predicted to succeed in the real game, since the def genuinely exists
/// again by the time it runs) was misreported as `DeadTarget`.
///
/// Returns `Ok(true)` (recreated, caller returns immediately) or
/// `Ok(false)` (not this shape — caller continues its own ordinary
/// dispatch).
fn try_recreate_def_from_document_root_add(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
) -> Result<bool, ReplayError> {
    let is_document_root = matches!(
        resolve_xpath(xpath_text, tree, context)
            .map_err(|reason| unsupported(xpath_text, reason))?,
        XPathResolution::DocumentRoot
    );
    if !is_document_root || !matches!(tree.root.content, Content::Empty) {
        return Ok(false);
    }
    let Some(value) = direct_child(node, "value") else {
        return Ok(false);
    };
    for injected in value.children().filter(Node::is_element) {
        if !injected
            .tag_name()
            .name()
            .eq_ignore_ascii_case(context.def_type)
        {
            continue;
        }
        let identity_matches = match context.selector {
            Selector::DefName => {
                child_text(injected, "defName").as_deref() == Some(context.def_name)
            }
            Selector::NameAttr => injected.attribute("Name") == Some(context.def_name),
        };
        if !identity_matches {
            continue;
        }
        let mut field_node = crate::xml::parse_element(injected).map_err(|_| {
            unsupported(
                xpath_text,
                "malformed <value> content for a recreated def".to_string(),
            )
        })?;
        // Mirror `xml::parse`'s own root handling — `Name`/`ParentName`
        // move onto the tree's own dedicated fields, `Abstract`/`Inherit`
        // are discarded — so a recreated def looks exactly like one
        // `xml::parse` loaded fresh, not like an ordinary `<value>`
        // fragment (which never needs this).
        tree.name = field_node.attrs.remove("Name");
        tree.parent_name = field_node.attrs.remove("ParentName");
        field_node.attrs.remove("Abstract");
        field_node.attrs.remove("Inherit");
        tree.root = field_node;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn apply_insert(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    let matched = match resolve_element_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationInsert",
        log,
        mod_id,
    )? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    reject_root_target(&matched, xpath_text, "PatchOperationInsert")?;
    let prepend =
        child_text(node, "order").is_some_and(|order| order.eq_ignore_ascii_case("prepend"));
    let values = value_children(node);
    for path in matched_highest_index_first(matched) {
        insert_sibling(&mut tree.root, &path, &values, prepend);
    }
    Ok(true)
}

pub(super) fn apply_remove(
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    match resolve_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationRemove",
        log,
        mod_id,
    )? {
        Selected::Done(result) => Ok(result),
        // Removing a `text()` node clears the element's text, leaving the
        // element itself in place.
        Selected::TextOf(matched) => {
            for path in matched {
                if let Some(target) = get_mut(&mut tree.root, path.segments()) {
                    target.content = Content::Empty;
                }
            }
            Ok(true)
        }
        Selected::Elements(matched) => {
            if matches_def_root(&matched) {
                // RimWorld really does delete the def node from the document —
                // unlike `Replace`/`Insert`/`SetName` on the def root
                // (still `Unsupported` below, via `reject_root_target`),
                // a replay can model exactly this and nothing more: empty
                // the tree and disclose it. `Caveat::DefRemoved` is
                // pushed unconditionally, before `<success>` is applied
                // by this op's caller — the underlying mutation isn't
                // undone by an enclosing `<success>Invert>`/`Never`, only
                // how the *result* is reported is (see
                // `apply_success_mode`'s own doc comment).
                tree.root.content = Content::Empty;
                log.caveats.push(Caveat::DefRemoved {
                    mod_id: mod_id.clone(),
                });
                return Ok(true);
            }
            reject_root_target(&matched, xpath_text, "PatchOperationRemove")?;
            for path in matched_highest_index_first(matched) {
                remove_at(&mut tree.root, path.segments());
            }
            Ok(true)
        }
    }
}

pub(super) fn apply_replace(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    match resolve_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationReplace",
        log,
        mod_id,
    )? {
        Selected::Done(result) => Ok(result),
        Selected::TextOf(matched) => {
            let text = replacement_text(node, xpath_text)?;
            for path in matched {
                if let Some(target) = get_mut(&mut tree.root, path.segments()) {
                    target.content = if text.is_empty() {
                        Content::Empty
                    } else {
                        Content::Text(text.clone())
                    };
                }
            }
            Ok(true)
        }
        Selected::Elements(matched) => {
            reject_root_target(&matched, xpath_text, "PatchOperationReplace")?;
            let values = value_children(node);
            for path in matched_highest_index_first(matched) {
                replace_at(&mut tree.root, &path, &values);
            }
            Ok(true)
        }
    }
}

/// The `<value>` of a `text()`-targeting `PatchOperationReplace`: its own
/// text, trimmed. A `<value>` carrying elements isn't a text replacement
/// at all, so it's `Unsupported` rather than guessed at.
fn replacement_text(node: Node, xpath_text: &str) -> Result<String, ReplayError> {
    let Some(value) = direct_child(node, "value") else {
        return Ok(String::new());
    };
    if value.children().any(|child| child.is_element()) {
        return Err(unsupported(
            xpath_text,
            "a text() Replace whose <value> carries elements is not modelled".to_string(),
        ));
    }
    Ok(value.text().map(str::trim).unwrap_or_default().to_string())
}

pub(super) fn apply_attribute_op(
    class: &str,
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    let matched = match resolve_element_selection(tree, context, xpath_text, class, log, mod_id)? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let attr_name = child_text(node, "attribute").unwrap_or_default();
    let attr_value = child_text(node, "value").unwrap_or_default();
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            if class.ends_with("PatchOperationAttributeAdd") {
                target
                    .attrs
                    .entry(attr_name.clone())
                    .or_insert_with(|| attr_value.clone());
            } else if class.ends_with("PatchOperationAttributeSet") {
                target.attrs.insert(attr_name.clone(), attr_value.clone());
            } else {
                target.attrs.remove(&attr_name);
            }
        }
    }
    Ok(true)
}

pub(super) fn apply_set_name(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    let matched = match resolve_element_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationSetName",
        log,
        mod_id,
    )? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let new_name = child_text(node, "name").unwrap_or_default();
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            target.tag = new_name.clone();
        }
    }
    Ok(true)
}

pub(super) fn apply_add_mod_extension(
    node: Node,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<bool, ReplayError> {
    let matched = match resolve_element_selection(
        tree,
        context,
        xpath_text,
        "PatchOperationAddModExtension",
        log,
        mod_id,
    )? {
        Selected::Done(result) => return Ok(result),
        Selected::Elements(matched) | Selected::TextOf(matched) => matched,
    };
    let values = value_children(node);
    for path in matched_highest_index_first(matched) {
        if let Some(target) = get_mut(&mut tree.root, path.segments()) {
            ensure_children(target, "modExtensions").extend(values.iter().cloned());
        }
    }
    Ok(true)
}
