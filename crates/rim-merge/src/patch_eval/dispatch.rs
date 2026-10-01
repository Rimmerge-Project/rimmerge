//! Operation dispatch: success modes, sequences, conditionals, `FindMod`, and whether an op can
//! affect this def at all.

use rim_analyzer::domain::ModId;
use roxmltree::Node;

use super::custom_ops::{apply_custom_behaviour, apply_mutation};
use super::identity::{child_text, direct_child, gates_open, has_operations_child, toggle_default};
use super::select::{DOCUMENT_ROOT_ADDITIONS, XPathResolution, condition_matches, resolve_xpath};
use super::{ReplayContext, ReplayError, ReplayLog};
use crate::plan::Caveat;
use crate::tree::FieldTree;

/// Applies one operation node (already gate-checked by its caller, except
/// for the top-level call, whose own gate is checked here). Returns
/// `Ok(true)` when the operation ran and (if applicable) matched at least
/// one node, `Ok(false)` when a `Replace`/`Remove`/`Add`/`Insert`/
/// `Attribute*`/`AddModExtension` matched zero nodes or a
/// `PatchOperationTest`'s xpath didn't match (RimWorld logs an error, or
/// silently stops, and moves on — a `PatchOperationSequence` stops
/// running the rest of its own children, but see [`apply_operations_list`]
/// for how that itself is reported to *its* parent), and `Err` when the
/// operation's xpath (or class) falls outside the replay's xpath grammar —
/// this aborts the whole replay.
pub(super) fn apply_operation<'a, 'input>(
    node: Node<'a, 'input>,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    let Some(class) = node.attribute("Class") else {
        return Ok(true);
    };
    // A `MayRequire` gate RimWorld doesn't satisfy means the operation
    // is never constructed at all, so `<success>` never applies to it.
    if !gates_open(node, context) {
        return Ok(true);
    }
    let outcome = apply_operation_body(class, node, tree, context, mod_id, log, failed_leaf)?;
    let outcome = apply_success_mode(node, outcome);
    // If this node's own final outcome is a
    // failure and nothing *deeper* already claimed responsibility (every
    // recursive call below sets `failed_leaf` before returning up), this
    // node itself is the real leaf to blame — either it's genuinely a
    // leaf (a mutation matching nothing, or a `PatchOperationTest` whose
    // condition is false — neither recurses any further), or its own
    // `<success>Invert>` flipped an otherwise-clean result into a
    // failure that no deeper node caused. First-write-wins by construction:
    // replay stops at the first `false` it meets, so at most one node per
    // top-level contribution can ever reach this still finding `None`.
    if !outcome && failed_leaf.is_none() {
        *failed_leaf = Some(node);
    }
    Ok(outcome)
}

/// RimWorld's `PatchOperation.Apply` wrapper: `<success>` decides what
/// the operation's caller (an enclosing `PatchOperationSequence`, or the
/// game's own patch loop) sees, whatever the operation itself did.
///
/// A `Success.Always` operation that matched nothing still leaves its
/// [`Caveat::FailedOp`] behind: the caveat records "this op matched no
/// node", which stays true and worth surfacing, while the boolean here
/// governs control flow only.
fn apply_success_mode(node: Node, outcome: bool) -> bool {
    match child_text(node, "success") {
        Some(mode) if mode.eq_ignore_ascii_case("Always") => true,
        Some(mode) if mode.eq_ignore_ascii_case("Invert") => !outcome,
        Some(mode) if mode.eq_ignore_ascii_case("Never") => false,
        // `Normal`, absent, or a value RimWorld's own enum parse would
        // reject (which leaves it at the default).
        _ => outcome,
    }
}

/// [`apply_operation`]'s dispatch, before `<success>` is applied.
fn apply_operation_body<'a, 'input>(
    class: &str,
    node: Node<'a, 'input>,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    if class.ends_with("PatchOperationTest") {
        return condition_matches(node, tree, context, "PatchOperationTest");
    }
    if class.ends_with("PatchOperationSequence") {
        return apply_operations_list(node, tree, context, mod_id, log, failed_leaf);
    }
    if class.ends_with("PatchOperationConditional") {
        return apply_conditional(node, tree, context, mod_id, log, failed_leaf);
    }
    if class.ends_with("PatchOperationFindMod") {
        return apply_find_mod(node, tree, context, mod_id, log, failed_leaf);
    }
    if let Some(behaviour) = context.behaviours.behaviour_for(class) {
        return apply_custom_behaviour(behaviour, class, node, tree, context, mod_id, log);
    }
    if has_operations_child(node) {
        // An unrecognized sequence-like custom operation class,
        // replayed at its declared default toggle.
        log.caveats.push(Caveat::ModSettingDefault {
            mod_id: mod_id.clone(),
            class: class.to_string(),
        });
        if toggle_default(node) {
            return apply_operations_list(node, tree, context, mod_id, log, failed_leaf);
        }
        return Ok(true);
    }
    // A custom class with no `<xpath>` of its own but with
    // `<match>`/`<nomatch>` branches (or a single `<operation>`) is a
    // mod-setting toggle wearing a `PatchOperationConditional`'s clothes
    // — its "condition" is the class's own declared default.
    if direct_child(node, "xpath").is_none() {
        let has_branches =
            direct_child(node, "match").is_some() || direct_child(node, "nomatch").is_some();
        let has_operation = direct_child(node, "operation").is_some();
        if has_branches || has_operation {
            log.caveats.push(Caveat::ModSettingDefault {
                mod_id: mod_id.clone(),
                class: class.to_string(),
            });
            let enabled = toggle_default(node);
            if has_branches {
                let branch = if enabled { "match" } else { "nomatch" };
                return apply_branch(node, branch, tree, context, mod_id, log, failed_leaf);
            }
            if enabled {
                return apply_branch(node, "operation", tree, context, mod_id, log, failed_leaf);
            }
            return Ok(true);
        }
    }

    apply_mutation(class, node, tree, context, mod_id, log)
}

/// Runs `<operations>`'s children in order, stopping at the first one
/// that returns `Ok(false)`. Returns that same `Ok(false)` (or `Ok(true)`
/// if every child ran clean) rather than always `Ok(true)` — a failure
/// inside a *nested* `PatchOperationSequence`/`PatchOperationConditional`/
/// `PatchOperationFindMod` must propagate to whichever construct wraps
/// it, exactly like a leaf mutation's own failure would.
fn apply_operations_list<'a, 'input>(
    node: Node<'a, 'input>,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    let Some(operations) = direct_child(node, "operations") else {
        return Ok(true);
    };
    let mut succeeded = true;
    for child in operations.children().filter(Node::is_element) {
        succeeded = apply_operation(child, tree, context, mod_id, log, failed_leaf)?;
        if !succeeded {
            break;
        }
    }
    Ok(succeeded)
}

fn apply_branch<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    let Some(wrapper) = direct_child(node, tag) else {
        return Ok(true);
    };
    if wrapper.attribute("Class").is_some() {
        return apply_operation(wrapper, tree, context, mod_id, log, failed_leaf);
    }
    let mut succeeded = true;
    for child in wrapper.children().filter(Node::is_element) {
        succeeded = apply_operation(child, tree, context, mod_id, log, failed_leaf)?;
        if !succeeded {
            break;
        }
    }
    Ok(succeeded)
}

pub(super) fn unsupported(xpath: &str, reason: String) -> ReplayError {
    ReplayError::Unsupported {
        xpath: xpath.to_string(),
        reason,
    }
}

fn apply_conditional<'a, 'input>(
    node: Node<'a, 'input>,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    let matched = match condition_matches(node, tree, context, "PatchOperationConditional") {
        Ok(matched) => matched,
        // A test this replay can't answer (another def with steps, a
        // root predicate, `text()`, an `@Name` head, or an unparseable
        // xpath) is moot to this replay when neither branch could touch
        // this def anyway — whatever the real answer is, running the
        // matching branch here would be a no-op. See
        // `branches_can_affect_this_def`'s own doc comment for how
        // conservative that walk is.
        Err(error) => {
            if branches_can_affect_this_def(node, tree, context)? {
                return Err(error);
            }
            return Ok(true);
        }
    };
    let branch = if matched { "match" } else { "nomatch" };
    apply_branch(node, branch, tree, context, mod_id, log, failed_leaf)
}

/// Whether either of a Conditional's branches contains an operation
/// that could affect the def being replayed. Walked only after
/// `condition_matches` itself fails to answer the test — a conservative
/// scan, not a replay: it never mutates `tree`, only asks "could this"
/// (hence `&FieldTree`, not `&mut` — needed only so a nested content
/// predicate can be evaluated the same way an ordinary replay would,
/// Group B).
fn branches_can_affect_this_def(
    node: Node,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
) -> Result<bool, ReplayError> {
    for tag in ["match", "nomatch"] {
        if let Some(wrapper) = direct_child(node, tag)
            && operation_can_affect_this_def(wrapper, tree, context)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every operation class this module's grammar recognizes by name —
/// mirrors `apply_operation_body`/`apply_mutation`'s own `ends_with`
/// checks. Used only by [`class_is_modelled`]; keep in sync with those
/// two functions rather than trying to derive one from the other (they
/// serve different callers: dispatch vs. "do we understand this class at
/// all").
const STANDARD_CLASS_SUFFIXES: [&str; 13] = [
    "PatchOperationTest",
    "PatchOperationSequence",
    "PatchOperationConditional",
    "PatchOperationFindMod",
    "PatchOperationAdd",
    "PatchOperationInsert",
    "PatchOperationRemove",
    "PatchOperationReplace",
    "PatchOperationAttributeAdd",
    "PatchOperationAttributeSet",
    "PatchOperationAttributeRemove",
    "PatchOperationSetName",
    "PatchOperationAddModExtension",
];

/// Whether this module knows what `node`'s own `Class` does at all — a
/// standard class, a class [`ReplayContext::behaviours`] maps onto a
/// modelled [`CustomBehaviour`], an `<operations>`
/// sequence, or a mod-setting-toggle wearing a Conditional's clothes (no
/// `<xpath>` of its own, but `<match>`/`<nomatch>`/
/// `<operation>` branches). No `Class` attribute at all means `node` is a
/// bare `<match>`/`<nomatch>`/`<operations>` wrapper, not an operation —
/// vacuously "modelled" since [`operation_can_affect_this_def`] recurses
/// into its children instead of dispatching on it directly.
fn class_is_modelled(node: Node, context: &ReplayContext<'_>) -> bool {
    let Some(class) = node.attribute("Class") else {
        return true;
    };
    if STANDARD_CLASS_SUFFIXES.iter().any(|s| class.ends_with(s))
        || context.behaviours.behaviour_for(class).is_some()
        || has_operations_child(node)
    {
        return true;
    }
    direct_child(node, "xpath").is_none()
        && (direct_child(node, "match").is_some()
            || direct_child(node, "nomatch").is_some()
            || direct_child(node, "operation").is_some())
}

/// The conservative branch walk: `true` means "assume it could", not
/// "it does". An unmodelled class, an xpath this module can't resolve at
/// all, an xpath naming this def, or a non-additive document-root op
/// (see [`DOCUMENT_ROOT_ADDITIONS`]) all count as "could affect" — only
/// an xpath strictly or loosely resolved to *other* defs, or a class this
/// module knows only ever adds new document-root defs, rules a node out.
/// Recurses into every child but `value`/`xpath`/`mods` (a value fragment
/// can itself contain arbitrary element children that are not sub-ops).
fn operation_can_affect_this_def(
    node: Node,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
) -> Result<bool, ReplayError> {
    if !class_is_modelled(node, context) {
        return Ok(true);
    }
    if let Some(xpath_text) = child_text(node, "xpath") {
        match resolve_xpath(&xpath_text, tree, context) {
            Ok(XPathResolution::Def(resolved)) => {
                if resolved.is_this_def {
                    return Ok(true);
                }
            }
            Ok(XPathResolution::DocumentRoot) => {
                let class = node.attribute("Class").unwrap_or("");
                if !DOCUMENT_ROOT_ADDITIONS.iter().any(|s| class.ends_with(s)) {
                    return Ok(true);
                }
            }
            Ok(XPathResolution::Elsewhere) => {}
            Err(reason) => return Err(unsupported(&xpath_text, reason)),
        }
    }
    for child in node.children().filter(Node::is_element) {
        if matches!(child.tag_name().name(), "value" | "xpath" | "mods") {
            continue;
        }
        if operation_can_affect_this_def(child, tree, context)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn apply_find_mod<'a, 'input>(
    node: Node<'a, 'input>,
    tree: &mut FieldTree,
    context: &ReplayContext<'_>,
    mod_id: &ModId,
    log: &mut ReplayLog,
    failed_leaf: &mut Option<Node<'a, 'input>>,
) -> Result<bool, ReplayError> {
    let names: Vec<String> = direct_child(node, "mods")
        .map(|mods| {
            mods.children()
                .filter(Node::is_element)
                .filter_map(|li| li.text())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let any_active = names.iter().any(|name| {
        context
            .mod_names_by_display
            .get(name)
            .is_some_and(|id| context.active_mods.contains(id))
    });
    let branch = if any_active { "match" } else { "nomatch" };
    apply_branch(node, branch, tree, context, mod_id, log, failed_leaf)
}
