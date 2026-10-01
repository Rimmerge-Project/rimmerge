//! Operation identity (`qualified_class`, `operation_identity`) and the small XML-reading helpers
//! the evaluator shares.

use rim_analyzer::domain::ModId;
use roxmltree::Node;

use super::ReplayContext;

/// RimWorld's own class name for a patch operation node's `Class`
/// attribute, fully namespace-qualified the way RimWorld's own
/// `GetType().ToString()` shows it in a log line: a mod almost always
/// writes a bare, unqualified name for a *built-in* class
/// (`Class="PatchOperationAdd"`), which RimWorld resolves against its own
/// `Verse` namespace by default — confirmed against real Core/DLC patch
/// files (every `Class="PatchOperation..."` attribute is bare, never
/// `Verse.`-prefixed) and real log lines (every distinct class name is
/// logged with a `Verse.` prefix). A class name a mod *does* qualify
/// (contains a `.` — a custom, non-vanilla class) is left exactly as
/// written; RimWorld never rewrites those.
fn qualified_class(class: &str) -> String {
    if class.contains('.') {
        class.to_string()
    } else {
        format!("Verse.{class}")
    }
}

/// RimWorld's own log identity for one operation node — close to (not a
/// verified decompile of) `PatchOperation.ToStringShort()`, built
/// empirically from real "Patch operation ... failed" log lines — an
/// empirical reconstruction, not a verified one.
///
/// **Confirmed against a real log line**: a leaf mutation
/// (`Add`/`Replace`/`Remove`/anything not specially handled below) is
/// `"{class}({xpath})"`; `PatchOperationConditional` is
/// `"{class}({its own <xpath>})"` (the condition's own xpath, never the
/// branch actually taken); `PatchOperationSequence` on failure is
/// `"{class}(count={total children}, lastFailedOperation={the failed
/// child's own identity, recursively})"`, confirmed exactly against a
/// real xenotype-patches mod's own log line.
///
/// **Inferred, not confirmed against a real multi-mod example**:
/// `PatchOperationFindMod` is `"{class}({mod names, comma-joined})"` —
/// every real log line checked so far names exactly one
/// mod, so the join separator itself is a guess.
///
/// **Sequence-shaped detection is structural, not by class name**: the
/// `PatchOperationSequence` treatment above applies to *any* class with a
/// direct `<operations>` child, matching [`has_operations_child`] — the
/// exact same structural test `apply_operation_body`'s own sequence-like
/// fallback uses to run an unrecognized sequence-like custom class. A real
/// install's own toggle-in-mod-options sequence wrapper (a custom class
/// shaped exactly like `PatchOperationSequence`) has no `<xpath>` of its
/// own, so falling through to the bare-`<xpath>` leaf branch below would
/// render `"<that class>()"` for *every* one of its failures regardless of
/// which child actually failed, collapsing many genuinely distinct
/// predicted failures into one indistinguishable line. `last_failed_leaf`
/// itself is tracked structurally through the real control flow (see
/// [`TopLevelOutcome::failed_leaf_xpath`]'s own doc comment), never
/// recovered after the fact by re-searching the tree for a caveat's own
/// xpath text, so only the *decision to recurse at all* depends on shape,
/// using the same shape check replay itself uses.
pub(super) fn operation_identity(node: Node, last_failed_leaf: Option<Node>) -> String {
    let class = node.attribute("Class").unwrap_or("PatchOperation");
    let qualified = qualified_class(class);
    if class.ends_with("PatchOperationFindMod") {
        let mods = direct_child(node, "mods")
            .map(|mods_node| {
                mods_node
                    .children()
                    .filter(Node::is_element)
                    .filter_map(|li| li.text())
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        return format!("{qualified}({mods})");
    }
    if has_operations_child(node) {
        let count = direct_child(node, "operations")
            .map(|operations| operations.children().filter(Node::is_element).count())
            .unwrap_or(0);
        return match last_failed_leaf {
            Some(leaf) => format!(
                "{qualified}(count={count}, lastFailedOperation={})",
                operation_identity(leaf, None)
            ),
            None => format!("{qualified}(count={count})"),
        };
    }
    // A `PatchOperationConditional`, an ordinary leaf mutation, or
    // anything else with a bare `<xpath>` of its own.
    let xpath = child_text(node, "xpath").unwrap_or_default();
    if xpath.is_empty() {
        // Genuinely nothing to distinguish this operation by — no
        // `<operations>` list (handled above) and no `<xpath>` of its
        // own either. Real, but rare: every shape this crate's own
        // replay recognizes has one or the other. Must never render as
        // bare `()`, which would be indistinguishable from a different
        // failing operation of the same class — say so explicitly
        // instead of pretending there's nothing to say.
        return format!("{qualified}(no distinguishing detail available)");
    }
    format!("{qualified}({xpath})")
}

/// Exact tag-name match — RimWorld's own XML is case-sensitive (a mod
/// author writing `<XPath>` would not actually work in the real game),
/// so matching control elements case-insensitively could accept a shape
/// RimWorld itself would reject.
pub(super) fn direct_child<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == tag)
}

pub(super) fn child_text(node: Node, tag: &str) -> Option<String> {
    direct_child(node, tag)
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn csv_attr(node: Node, name: &str) -> Vec<String> {
    node.attribute(name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn gates_open(node: Node, context: &ReplayContext<'_>) -> bool {
    let required = csv_attr(node, "MayRequire");
    let any_of = csv_attr(node, "MayRequireAnyOf");
    let required_ok = required
        .iter()
        .all(|id| context.active_mods.contains(&ModId::new(id)));
    let any_of_ok = any_of.is_empty()
        || any_of
            .iter()
            .any(|id| context.active_mods.contains(&ModId::new(id)));
    required_ok && any_of_ok
}

pub(super) fn has_operations_child(node: Node) -> bool {
    direct_child(node, "operations").is_some()
}

/// The declared default of a custom, mod-setting-gated operation class —
/// re-exported from `rim-analyzer` rather than kept as a second copy here,
/// so the replay and the analyzer's own load-order edges never silently
/// disagree about which mods a default-off toggle actually runs (`rim-merge`
/// already depends on `rim-analyzer` directly; see that function's own doc
/// comment for the full rule).
pub(super) use rim_analyzer::extract::patches::toggle_default;
