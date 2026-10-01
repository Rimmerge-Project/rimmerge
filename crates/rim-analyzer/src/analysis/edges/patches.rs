//! Patch-target edges: injected and removed nodes, and predicates a patch can invalidate.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::domain::{
    ConditionalBranch, Conflict, DefTarget, DiscardedAddition, Edge, EdgeKind, ModId, PatchOp,
    ScannedMod, Selector,
};
use crate::extract::patches::NODE_CREATING_SUFFIXES;
use crate::extract::{xpath_expr, xpath_target};

use crate::analysis::indices::{ActiveMods, DefKey, DisplayNameIndex, Indices, patch_op_active};

/// A mutating patch operation targeting a def owned by exactly one other,
/// non-vanilla active mod. Operations gated off by `MayRequire`/
/// `MayRequireAnyOf` or an unsatisfied `PatchOperationFindMod` context are
/// skipped — they never run — as are `[@Name="X"]` targets, since `Name`
/// is a separate namespace from `defName` and can't be matched against
/// def ownership (keyed by `defName`).
///
/// `injected_owners` (built by
/// [`injected_def_owners`](super::defs::injected_def_owners)) is merged in as
/// a second ownership source alongside `indices.def_owners` — a whole-def
/// patch injection owns its def just as much as a `Defs/`-inline one does,
/// for this question's purposes. Its keys are whole-def injections only, and
/// it's already gated by the real `patch_op_active` (not just
/// `may_require_satisfied`), so no re-gating is needed here. An op patching
/// *inside* a patch-injected def (a `sub_path`, not the whole def) finds an
/// owner too — unlike the path-shape injected-node pass
/// (`emit_path_injected_node_edge`), which only matches an *exact* path, this
/// still fires for any depth under the injected def, at `Awareness` strength
/// rather than `Hard`, since only an exact-path match is provably
/// order-required.
#[must_use]
pub fn patch_target_edges(
    scanned: &[ScannedMod],
    indices: &Indices,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let mut seen = HashSet::new();
        for op in &scanned_mod.patch_ops {
            if !op.is_mutating || !patch_op_active(op, active, name_map) {
                continue;
            }
            let Some(target) = &op.target else { continue };
            if target.selector != Selector::DefName {
                continue;
            }
            let key = (target.def_type.clone(), target.def_name.clone());
            let combined_owners: BTreeSet<&ModId> = indices
                .def_owners
                .get(&key)
                .into_iter()
                .flatten()
                .chain(injected_owners.get(&key).into_iter().flatten())
                .collect();
            if combined_owners.is_empty() {
                continue;
            }

            let non_vanilla_owners: Vec<&ModId> = combined_owners
                .into_iter()
                .filter(|id| **id != scanned_mod.info.id)
                .filter(|id| !indices.mod_source.get(*id).is_some_and(|s| s.is_vanilla()))
                .collect();
            if non_vanilla_owners.len() != 1 {
                continue;
            }
            let owner = non_vanilla_owners[0];

            if seen.insert(owner.clone()) {
                edges.push(Edge {
                    after: scanned_mod.info.id.clone(),
                    before: owner.clone(),
                    kind: EdgeKind::PatchTargetsDef,
                    detail: format!(
                        "patches {} '{}' owned by {owner}",
                        target.def_type, target.def_name
                    ),
                    load_time: true,
                    subject: None,
                });
            }
        }
    }
    edges
}

/// `after` -> `before` -> `kind` triples already emitted, so a mod naming
/// the same injector/provider through several ops or types produces one
/// edge, not several.
pub(super) type SeenEdges = HashSet<(ModId, ModId, EdgeKind)>;

pub(super) fn push_edge_once(
    edges: &mut Vec<Edge>,
    seen: &mut SeenEdges,
    after: ModId,
    before: ModId,
    kind: EdgeKind,
    detail: String,
    subject: Option<String>,
) {
    if seen.insert((after.clone(), before.clone(), kind)) {
        edges.push(Edge {
            after,
            before,
            kind,
            detail,
            load_time: true,
            subject,
        });
    }
}

/// Whether a selecting op's own `DefTarget` and an injecting op's own
/// `DefTarget` are compatible evidence that the same node is at stake (rule
/// 2): true whenever either side carries no target — nothing to contradict —
/// or when both target the same `(def_type, def_name)`. `selector`/`sub_path`
/// aren't compared: the two ops' xpaths differ by construction (one selects a
/// class the other injected, so they can't be the identical xpath), only
/// which *def* each is scoped to matters. False only when both carry a target
/// and those targets disagree — a class string injected into an unrelated def
/// is no evidence the selector's own def depends on that injection.
fn def_targets_compatible(selecting: Option<&DefTarget>, injecting: Option<&DefTarget>) -> bool {
    match (selecting, injecting) {
        (Some(a), Some(b)) => a.def_type == b.def_type && a.def_name == b.def_name,
        _ => true,
    }
}

/// A mutating patch operation's xpath selects a namespace-qualified type
/// (`[@Class="T"]`/`[name="T"]`, `T` containing a `.`) that another active
/// mod's own patch injects via `<value>`. When that exact type string is
/// written inline nowhere in any active mod's `Defs/`, the selected node can
/// only exist because the injector's patch created it — a genuine, `Hard`
/// load-order requirement (`PatchInjectedNode`). When it *does* appear inline
/// somewhere, or more than one active mod injects the same string (no single
/// injector can be named with confidence), or the selecting mod injects the
/// string itself (its own patch could have created the node regardless of any
/// other mod's — rule 1's "ambiguous injector" demotion, generalized to a mod
/// being ambiguous with itself), or the selecting op and the sole injector's
/// op both carry a `DefTarget` naming a different def (rule 2: the injected
/// node lives under an unrelated def, so selecting the same class string
/// elsewhere isn't evidence of a dependency on *this* injection) — the node
/// may pre-exist independently of the injecting patch: demoted to advisory
/// `UsesType` naming the injector as the interacting mod.
#[must_use]
pub fn patch_injected_node_edges(
    scanned: &[ScannedMod],
    indices: &Indices,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> Vec<Edge> {
    // For each injected class string, every (injecting mod, that
    // injecting op's own `DefTarget`) occurrence — one entry per
    // mutating, active op that injects it. Kept at op granularity (not
    // collapsed to "which mods inject this") because rule 2 needs each
    // occurrence's own target to compare against the selecting op's.
    let mut injected_type_owners: BTreeMap<String, Vec<(ModId, Option<DefTarget>)>> =
        BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            for injected in &op.injected_types {
                injected_type_owners
                    .entry(injected.clone())
                    .or_default()
                    .push((scanned_mod.info.id.clone(), op.target.clone()));
            }
        }
    }

    let mut edges = Vec::new();
    let mut seen: SeenEdges = HashSet::new();
    for scanned_mod in scanned {
        let selector_id = &scanned_mod.info.id;
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            let Some(xpath) = op.xpath.as_deref() else {
                continue;
            };
            // Already a `BTreeSet` (`xpath_expr::selected_classes`'s own
            // return type) and `patch_ops` is scanned in a fixed order,
            // so iterating per op here stays fully deterministic.
            for class in xpath_expr::selected_classes(xpath) {
                emit_patch_injected_node_edge(
                    &mut edges,
                    &mut seen,
                    indices,
                    &injected_type_owners,
                    selector_id,
                    op,
                    &class,
                );
            }
        }
    }

    // Second pass (generalizes the class-string pass above to node paths and
    // injected defs). Every active mutating op's own `injected_paths`
    // entries, owner mod per path — parallels `injected_type_owners` above,
    // but keyed by the exact path text ([`PatchOp::injected_paths`]'s own
    // format) instead of a bare class string, so no per-occurrence target is
    // needed here: the path itself already fully encodes the target location,
    // which is what lets this pass skip rule 2 entirely (see
    // [`emit_path_injected_node_edge`]'s own doc comment).
    let mut injected_path_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            for path in &op.injected_paths {
                injected_path_owners
                    .entry(path.clone())
                    .or_default()
                    .push(scanned_mod.info.id.clone());
            }
        }
    }
    for scanned_mod in scanned {
        let selector_id = &scanned_mod.info.id;
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            let Some(target) = &op.target else {
                continue;
            };
            // An op nested under a Conditional whose own xpath equals its
            // own (the "if it exists, then ..." idiom) tolerates its own
            // target's absence — the identical `tolerates_absence` shape
            // `patch_invalidates_predicate_edges` already guards against,
            // and for the same reason: without it, this whole loop would
            // credit such an op as *requiring* whatever it selects to
            // already exist, when the Conditional makes it a graceful
            // no-op instead. This was unreachable before the
            // predicate-keyed `<li>` shape below existed — nothing in
            // `injected_path_owners` ever carried a predicate-bracketed
            // key for the plain `target.match_key()` lookup to coincide
            // with — but a real-install compat patch commonly wraps a
            // predicate-li `Replace` in exactly this idiom
            // (`PatchOperationConditional[xpath = X]/PatchOperationReplace[xpath
            // = X]`), and two such ops on the same pair of mods — one for
            // each side's own gene/comp — would otherwise fabricate a
            // mutual `Hard` cycle neither op's own author ever required.
            let tolerates_absence = op.conditional_xpath.as_deref() == op.xpath.as_deref()
                && op.conditional_xpath.is_some();
            if tolerates_absence {
                continue;
            }
            let path = target.match_key();
            emit_path_injected_node_edge(
                &mut edges,
                &mut seen,
                indices,
                &injected_path_owners,
                selector_id,
                &path,
                InlineCheck::Applicable,
            );
            // The predicate-keyed `<li>` shape: a selecting op whose own
            // sub_path reads through one or more `li[@Attr="v"]`/
            // `li[text()="v"]` steps reaches the identical normalized key
            // text `injected_path_owners` may carry for each (see
            // `li_predicate_lookup_keys`'s own doc comment) — tried in
            // addition to, never instead of, the ordinary
            // `target.match_key()` lookup above.
            for predicate_key in li_predicate_lookup_keys(target) {
                emit_path_injected_node_edge(
                    &mut edges,
                    &mut seen,
                    indices,
                    &injected_path_owners,
                    selector_id,
                    &predicate_key.key,
                    predicate_key.inline_check,
                );
            }
        }
    }

    edges
}

/// One selecting op's own class -> edge decision, factored out of
/// [`patch_injected_node_edges`]: emits a `Hard` `PatchInjectedNode` edge to
/// the class's sole injector when it can be named with confidence (see that
/// function's own doc comment for the full rule), otherwise an advisory
/// `UsesType` edge to every distinct injector (excluding the selector
/// itself).
fn emit_patch_injected_node_edge(
    edges: &mut Vec<Edge>,
    seen: &mut SeenEdges,
    indices: &Indices,
    injected_type_owners: &BTreeMap<String, Vec<(ModId, Option<DefTarget>)>>,
    selector_id: &ModId,
    selecting_op: &PatchOp,
    class: &str,
) {
    let Some(occurrences) = injected_type_owners.get(class) else {
        return;
    };
    let self_injects = occurrences.iter().any(|(id, _)| id == selector_id);
    let mut other_injectors: Vec<&ModId> = occurrences
        .iter()
        .map(|(id, _)| id)
        .filter(|id| *id != selector_id)
        .collect();
    other_injectors.sort();
    other_injectors.dedup();
    if other_injectors.is_empty() {
        return;
    }

    let pre_exists_inline = indices.inline_type_names.contains(class);
    let sole_injector = match other_injectors.as_slice() {
        [sole] => Some(*sole),
        _ => None,
    };
    let hard_injector = sole_injector.filter(|injector| {
        !pre_exists_inline
            && !self_injects
            && occurrences
                .iter()
                .filter(|(id, _)| id == *injector)
                .any(|(_, injecting_target)| {
                    def_targets_compatible(selecting_op.target.as_ref(), injecting_target.as_ref())
                })
    });

    if let Some(injector) = hard_injector {
        push_edge_once(
            edges,
            seen,
            selector_id.clone(),
            injector.clone(),
            EdgeKind::PatchInjectedNode,
            format!("patch selects '{class}', injected by {injector}"),
            Some(class.to_string()),
        );
        return;
    }

    for injector in other_injectors {
        push_edge_once(
            edges,
            seen,
            selector_id.clone(),
            injector.clone(),
            EdgeKind::UsesType,
            format!(
                "patch selects '{class}', injected by {injector} but also written inline, injected by more than one mod, injected by the selector itself, or injected into an unrelated def, so the order isn't provably required"
            ),
            Some(class.to_string()),
        );
    }
}

/// Splits `path` into `(def_type, name_segment)` when it's the
/// whole-`<Defs>`-root injected-def shape — exactly two `/`-delimited
/// segments, no `sub_path` — `None` for the element-injection shape
/// (three or more segments). `path` is always built from
/// [`DefTarget::match_key`], so `name_segment` carries the selector-aware
/// `@`-prefix for a [`Selector::NameAttr`] target — see
/// [`emit_path_injected_node_edge`]'s own doc comment for how the caller
/// tells the two apart.
fn whole_def_path_parts(path: &str) -> Option<(&str, &str)> {
    let mut segments = path.split('/');
    let def_type = segments.next()?;
    let name_segment = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    Some((def_type, name_segment))
}

/// The element-injection shape's own `sub_path` depth (number of
/// `/`-segments after `"{def_type}/{name_segment}/"`) — meaningless on
/// the whole-def shape (see [`whole_def_path_parts`]), whose caller never
/// calls this on one.
fn element_injection_depth(path: &str) -> usize {
    path.split('/').count().saturating_sub(2)
}

/// One selecting op's own path -> edge decision (the second pass,
/// generalizing [`emit_patch_injected_node_edge`]'s own class-string ladder
/// to a node path): emits a `Hard` `PatchInjectedNode` edge to the path's
/// sole injector when it can be named with confidence, otherwise an advisory
/// [`EdgeKind::PatchSelectsInjectedNode`] edge to every distinct injector
/// (excluding the selector itself) — a separate kind from `UsesType`, which
/// the class-string pass above uses for its own demotion (that shape's
/// `subject` genuinely names a type, this one names an XML path) — same shape
/// as the class-string pass, with one deliberate difference: **no rule-2
/// equivalent** (comparing the selecting and injecting op's own `DefTarget`s)
/// — a matching *path* (unlike a bare class string) already fully encodes
/// which def and sub-location it names, so there's no "same class name,
/// different def" ambiguity left to disambiguate.
///
/// The "pre-exists inline" check is shape-scoped, and both shapes demote
/// (many element-injection subjects, e.g. `ThingDef/MealNutrientPaste/
/// ingestible`, already ship inline in Core, so treating them as `Hard` would
/// assert the patch created something that already existed):
/// - Whole-`<Defs>`-root shape (`path` is exactly `"{def_type}/{name_segment}"`,
///   see [`whole_def_path_parts`]): a [`Selector::DefName`] target (bare
///   `name_segment`) reuses `indices.def_owners` directly — the existing
///   `pre_exists_inline`/`def_owners` logic, by path instead of by class —
///   asking whether that exact def name is *also* authored inline by some
///   active mod (a def-override-style collision `conflicts::def_overrides`
///   reports separately).
///   A [`Selector::NameAttr`] target (`@`-prefixed `name_segment`, from
///   [`DefTarget::match_key`]) instead consults `indices.template_owners`
///   by the unprefixed name (`def_owners` only ever holds `defName`-keyed
///   concrete defs, so it would read "absent" for every `[@Name="…"]`
///   target and assert a false `Hard` edge, which can even beat a correct
///   remover-last edge in a cross-layer 2-cycle).
/// - Element-injection shape (a `sub_path` under an existing target
///   def): consults `indices.inline_node_paths`
///   ([`crate::domain::hash_node_path`] of `path`) — the per-node index
///   built in `extract::defs` specifically because `indices.def_owners`
///   only proves the *outer* def has an inline owner, not that this
///   specific descendant node does. **Bounded, deliberate conservatism**:
///   a `path` deeper than [`crate::extract::defs::MAX_INLINE_NODE_PATH_DEPTH`]
///   was never indexed at all, so it's treated as unknown and demoted —
///   the safe direction (a missed `Hard` edge, never a wrongly-asserted
///   one), not a false "not found".
///
/// `inline_check` says whether the element-injection lookup may consult the
/// inline index at all: a predicate-keyed `li[...]` path only proves "a Def
/// ships this `<li>`" when it is the selecting op's last step and was
/// captured whole (see [`InlineCheck`]); otherwise the inline index is
/// skipped and the edge stays `Hard` unless another rule demotes it.
pub(super) fn emit_path_injected_node_edge(
    edges: &mut Vec<Edge>,
    seen: &mut SeenEdges,
    indices: &Indices,
    injected_path_owners: &BTreeMap<String, Vec<ModId>>,
    selector_id: &ModId,
    path: &str,
    inline_check: InlineCheck,
) {
    let Some(occurrences) = injected_path_owners.get(path) else {
        return;
    };
    let self_injects = occurrences.iter().any(|id| id == selector_id);
    let mut other_injectors: Vec<&ModId> =
        occurrences.iter().filter(|id| *id != selector_id).collect();
    other_injectors.sort();
    other_injectors.dedup();
    if other_injectors.is_empty() {
        return;
    }

    let pre_exists_inline = match whole_def_path_parts(path) {
        Some((def_type, name_segment)) => match name_segment.strip_prefix('@') {
            Some(template_name) => indices.template_owners.contains_key(template_name),
            None => indices
                .def_owners
                .contains_key(&(def_type.to_string(), name_segment.to_string())),
        },
        None if element_injection_depth(path)
            <= crate::extract::defs::MAX_INLINE_NODE_PATH_DEPTH =>
        {
            match inline_check {
                InlineCheck::Applicable => indices
                    .inline_node_paths
                    .contains(&crate::domain::hash_node_path(path)),
                InlineCheck::NotApplicable => false,
            }
        }
        // Deeper than the index goes: unknown, so demote (safe direction).
        None => true,
    };
    let sole_injector = match other_injectors.as_slice() {
        [sole] => Some(*sole),
        _ => None,
    };
    let hard_injector = sole_injector.filter(|_| !pre_exists_inline && !self_injects);

    if let Some(injector) = hard_injector {
        push_edge_once(
            edges,
            seen,
            selector_id.clone(),
            injector.clone(),
            EdgeKind::PatchInjectedNode,
            format!("patch selects '{path}', injected by {injector}"),
            Some(path.to_string()),
        );
        return;
    }

    for injector in other_injectors {
        push_edge_once(
            edges,
            seen,
            selector_id.clone(),
            injector.clone(),
            // This is the path-shape demotion, so it gets its own `EdgeKind`
            // rather than `UsesType` — `subject` here is an XML node path,
            // never a type name, and conflating the two would report false
            // `UndeclaredTypeDependency` findings (see
            // `EdgeKind::PatchSelectsInjectedNode`'s own doc comment).
            EdgeKind::PatchSelectsInjectedNode,
            format!(
                "patch selects '{path}', injected by {injector} but also written inline, injected by more than one mod, or injected by the selector itself, so the order isn't provably required"
            ),
            Some(path.to_string()),
        );
    }
}

/// One active, mutating toucher op's own contribution to
/// [`patch_removed_node_edges`]'s touchers index — the op itself (for
/// `sequence_tail`/`conditional_branch`/`conditional_nomatch_creates`/
/// `names_single_def`/`load_folder_gate`), plus the specific `sub_path`
/// it resolved to for the particular `(def_type, def_name, selector)` key
/// it's indexed under (a disjunctive head can resolve to several targets
/// per op, normally sharing one trailing `sub_path`, but kept per-target
/// rather than assumed).
struct ToucherOp<'a> {
    mod_id: ModId,
    op: &'a PatchOp,
    sub_path: Option<String>,
}

/// [`patch_removed_node_edges`]'s own touchers index: every active,
/// mutating op, keyed the same way both its Remove-remover and
/// Replace-remover passes look candidates up — hoisted to module scope so
/// both passes (and the shared per-target emission helper,
/// [`emit_removed_node_edges_for_target`]) can share the one type.
type Touchers<'a> = BTreeMap<(String, String, Selector), Vec<ToucherOp<'a>>>;

/// Op classes whose own correctness depends on reading or writing content
/// that already exists at their own target — [`patch_removed_node_edges`]'s
/// own Replace-remover pass restricts candidate touchers to these three,
/// never the full "any mutating op" set its Remove-remover pass uses.
/// Ground-truthed against the decompiled engine
/// (`Verse.PatchOperationRemove`/`Replace`/`Insert.ApplyWorker`, all three
/// `xml.SelectNodes(xpath)` over the node(s) already there — a `Remove`/
/// `Replace` whose own xpath resolves to nothing simply does nothing, and
/// `PatchOperation.Complete` then logs `Log.Error` because the op "never
/// succeeded"; an `Insert` is identical, its own xpath naming the sibling
/// **anchor** it inserts before/after, which must already exist). Deliberately
/// excludes `Add`/`AddModExtension`/`AttributeAdd`/`AttributeSet`
/// ([`ADDITIVE_CLASS_SUFFIXES`], the discarded-addition pass's own domain) even though their own
/// `ApplyWorker`s share the identical `SelectNodes`-must-resolve shape:
/// that pass's real-install evidence is overwhelmingly the *same-node* case (`A`'s
/// own xpath **is** `P`, the node the `Replace` keeps), where `P` always
/// still exists post-Replace by [`replace_keeps_node`]'s own construction,
/// so ordering `A` after `B` never puts `A`'s own required node at risk —
/// only the silently-discarded-content question that pass already covers. An
/// `Insert`'s own xpath, by contrast, is never `P` itself (you insert
/// relative to an existing **sibling** inside `P`, not the container), so
/// it is inherently a strict descendant of `P` and is exactly as exposed to
/// `P`'s old subtree being wiped as a `Remove`/`Replace` toucher is — moved
/// here entirely, out of [`ADDITIVE_CLASS_SUFFIXES`], so the same op can
/// never source both a discarded-addition edge and one of this pass's edges in opposite
/// directions for the same pair.
const READING_CLASS_SUFFIXES: [&str; 3] = [
    "PatchOperationRemove",
    "PatchOperationReplace",
    "PatchOperationInsert",
];

fn is_reading_toucher_class(class: &str) -> bool {
    READING_CLASS_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix))
}

/// "Remover loads last": from the same per-op data
/// `super::conflicts::patch_collisions` groups — [`super::patch_op_targets`],
/// not the bare [`PatchOp::target`], since a multi-def head (`[defName="A" or
/// defName="B"]`) removes from every def it names, not just the first. For
/// each active, mutating op `R` whose class ends `PatchOperationRemove`, on
/// target `T` (`sub_path` `P`) in mod `B`, and each *other* active mod `A`
/// with an active mutating op touching the same `T` whose own `sub_path` is
/// `P` itself or lies strictly beneath it (see [`sub_path_covers`] for the
/// exact rule — a real `/`-delimited path-segment test, never a bare
/// `str::starts_with`: `statBases` must not match `statBasesExtra`), emits
/// `after: B, before: A` — the remover must load after everyone else who also
/// touches the node it's about to delete, so their own patch runs while the
/// node still exists.
///
/// Two exclusions, both matching `patch_collisions`'s own gating:
/// `A == B` (never orders a mod against itself), and an op whose
/// `find_mod_context` is closed ([`patch_op_active`]).
///
/// A third: `R` nested under a `PatchOperationConditional` whose own xpath
/// *differs* from `R`'s ([`is_genuinely_conditional_remove`]) is a genuinely
/// conditional remove — **this function emits no `PatchRemovedNode`/
/// `PatchRemovedNodeCosmetic` edge at all for it**, so such a remove stays
/// advisory. `EdgeStrength` is a
/// property of the whole `EdgeKind` (see `EdgeKind::strength`), not something
/// one edge can override in isolation, so there is no way to emit "this one
/// `PatchRemovedNode` edge, but at `Awareness` strength instead of
/// `Inferred`". Rather than synthesizing a *different* edge kind just for
/// this case, it emits nothing and lets whatever `PatchTargetsDef`/`UsesType`
/// edge the pair already has stand as the (weaker, but real) evidence. An "if
/// exists, then remove" Conditional (its own xpath *equals* `R`'s) is
/// unconditional in effect and *does* emit.
///
/// Two further exclusions, checked per candidate toucher op *A* before
/// it counts towards anything below — a toucher this excludes is treated as
/// if it didn't exist for this remover, both for whether an edge is emitted
/// at all and for the cosmetic classification:
/// - `is_tolerant_toucher`: `A` sits in the `Match` branch of a Conditional
///   testing `A`'s own site (or its parent), whose `NoMatch` branch recreates
///   it — `A`'s author already planned for `R` running first.
/// - `toucher_folder_gated_on_remover`: `A`'s own loaded folder is gated
///   `IfModActive`/`IfModActiveAll` on `R`'s mod (by base id) — that folder
///   was written for `R`'s own end state.
///
/// **Cosmetic classification** (`PatchRemovedNodeCosmetic` instead of
/// `PatchRemovedNode`): first, a toucher mod `A`'s own (exclusion-surviving) ops on
/// this key are filtered down to the ones that actually *cover* `R`'s own
/// removed region ([`sub_path_covers`], rule a) — an op of `A` elsewhere on
/// the same def is unaffected by `R`'s removal at all, so it neither
/// justifies the edge nor counts toward classification (a real-install false
/// positive this rule specifically fixes: a mod's own unrelated `Add`/
/// `Conditional` on the *same def* but a different sub-tree used to force
/// `PatchRemovedNode` on an otherwise genuinely cosmetic pair). At least one
/// covering op is still required for an edge to exist at all. The pair is
/// then cosmetic exactly when **every** one of those *covering* ops also
/// satisfies all of `toucher_op_is_cosmetic`'s three remaining conditions —
/// one covering op having a later Sequence sibling, sitting in an unrelated
/// `NoMatch` branch, or naming more than one def, is enough to keep the
/// whole pair `PatchRemovedNode` (content), even if `A`'s *other* covering
/// ops on the same key would individually have qualified. Still strict where
/// it matters: a wrongly-cosmetic edge would hide real loss, so every
/// covering op still has to individually clear b/c/d (the "final
/// document identical either way" proof only holds when *nothing* of `A`'s
/// own writes *inside* `R`'s region survives).
///
/// **Known gaps** (measured on a real install):
/// - An attribute-predicate remover (e.g.
///   `ThingDef[@EX_Legacy_Hook="RemoveMe"]`) has no `DefTarget` —
///   [`crate::extract::xpath_target`] can't resolve an attribute
///   predicate — so it never appears in `patch_op_targets` and produces
///   no edge of this kind; the patch replay is what catches that case.
/// - `PatchOp::conditional_xpath` carries no `<match>`/`<nomatch>`
///   polarity, so a same-xpath `PatchOperationRemove` nested under a
///   Conditional's `<nomatch>` branch ("if it does *not* exist, remove
///   it" — dead code, since a nonexistent node can't be removed) reads
///   as unconditional here and *would* emit. Measured negligible: 9,615
///   ops sit under a Conditional's `<nomatch>` branch, only 6 have an
///   xpath equal to their own Conditional's, and none of the 6 is a
///   `PatchOperationRemove`.
/// - Raw string equality (never trimmed/normalized) is the right
///   comparison for "same xpath as the enclosing Conditional": of 750
///   `PatchOperationRemove`s nested under a Conditional on the real
///   install, 371 match their Conditional's xpath by raw string and 0
///   match only after trimming.
///
/// Cycles are expected and fine (e.g. two mods each removing a node the other
/// also touches): `Inferred`-strength edges are droppable by the existing
/// `break_cycles`, and each drop surfaces as an `EdgeDropped` finding, same
/// as any other droppable layer.
///
/// **A `PatchOperationReplace` is a remover too**, of everything strictly
/// *below* the node it keeps (`replace_keeps_node` — a renaming/splitting
/// `Replace` is the discarded-addition pass's own excluded case, never a remover here either): once
/// the old subtree is gone, a later `Remove`/`Replace`/`Insert` reading or
/// rewriting something that only existed in it fails exactly the way a
/// `PatchOperationRemove` remover already makes one fail, so it runs this
/// same emission logic a second time, over the same touchers index, with
/// three differences from the `Remove`-remover pass above: (1) candidate
/// touchers are restricted to `READING_CLASS_SUFFIXES`, never the full
/// "any mutating op" set — see that const's own doc comment for why this is
/// also what keeps this pass and [`replace_discards_addition`]
/// class-disjoint, so the same op can never source edges in both directions
/// for the same pair; (2) a toucher whose own predicate-keyed `<li>` need is
/// recreated by the replacer's own new value is excluded outright
/// (`replace_recreates_touchers_target`) — reusing the identical
/// predicate-`<li>` identity `li_predicate_lookup_keys` and
/// `PatchOp::injected_paths` already carry for the *opposite* direction
/// (the "Replace creates a node a selector needs" shape), since "the
/// replacer's own new value already contains the node the toucher needs" is
/// the same fact read from the other side; (3) the emitted detail text says
/// "replaces", not "removes". A same-pair, opposite-direction edge from a
/// *different* op of the same two mods (say, `A`'s own `Add` triggering the discarded-addition edge
/// in one direction and `A`'s own separate `Remove` triggering this pass in
/// the other) is not eliminated by any of this — that is two independent,
/// both-true facts about the pair, and `Layer::Inferred`'s own cycle-breaking
/// already exists to resolve exactly that class of situation, the same as
/// it does for any other two same-layer edges that disagree.
#[must_use]
pub fn patch_removed_node_edges(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> Vec<Edge> {
    // Every active, mutating op's own (def_type, def_name, selector) ->
    // [ToucherOp], expanded per `patch_op_targets` (not the bare
    // `op.target`) so a multi-def head is indexed under each def it names —
    // the same expansion `patch_collisions` itself performs. `Selector` is
    // part of the key: a `[@Name="X"]` template and a `[defName="X"]` def are
    // separate XML namespaces, the same reason `conflicts::patch_collisions`
    // keys on `(def_type, def_name, selector, sub_path)`, and real installs
    // target some keys (`ThingDef/Human` among them) under both selectors.
    // `BTreeMap`, for the same determinism reason as every other index in
    // this module. Shared by both the `Remove`-remover pass below and the
    // `Replace`-remover pass that follows it.
    let mut touchers: Touchers = BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            for target in super::patch_op_targets(op) {
                touchers
                    .entry((target.def_type, target.def_name, target.selector))
                    .or_default()
                    .push(ToucherOp {
                        mod_id: scanned_mod.info.id.clone(),
                        op,
                        sub_path: target.sub_path,
                    });
            }
        }
    }

    // Each mod's own creating ops, indexed once for both passes below —
    // `remover_recreates_region` asks it about every remover target.
    let own_creators: Vec<OwnCreatingOps<'_>> = scanned
        .iter()
        .map(|scanned_mod| OwnCreatingOps::build(&scanned_mod.patch_ops, active, name_map))
        .collect();

    let mut edges = Vec::new();
    let mut seen: SeenEdges = HashSet::new();
    for (scanned_mod, creators) in scanned.iter().zip(&own_creators) {
        let remover_id = &scanned_mod.info.id;
        for (op_index, op) in scanned_mod.patch_ops.iter().enumerate() {
            if !(op.is_mutating
                && op.class.ends_with("PatchOperationRemove")
                && patch_op_active(op, active, name_map))
            {
                continue;
            }
            if is_genuinely_conditional_remove(op) {
                continue;
            }
            for remover_target in super::patch_op_targets(op) {
                if remover_recreates_region(&remover_target, creators, op_index) {
                    // `R`'s own mod never leaves the region permanently
                    // absent — see `remover_recreates_region`'s own doc
                    // comment. No edge at all, cosmetic or not: this was
                    // never really a remover for this target.
                    continue;
                }
                emit_removed_node_edges_for_target(
                    &mut edges,
                    &mut seen,
                    &touchers,
                    remover_id,
                    op,
                    &remover_target,
                    |_class| true,
                    sub_path_covers,
                    false,
                    "removes",
                );
            }
        }
    }

    // The `Replace`-remover pass — see this function's own doc comment for
    // why it exists and how it differs from the `Remove`-remover pass above.
    for (scanned_mod, creators) in scanned.iter().zip(&own_creators) {
        let remover_id = &scanned_mod.info.id;
        for (op_index, op) in scanned_mod.patch_ops.iter().enumerate() {
            if !(op.is_mutating
                && op.class.ends_with("PatchOperationReplace")
                && patch_op_active(op, active, name_map))
            {
                continue;
            }
            if is_genuinely_conditional_remove(op) {
                continue;
            }
            for remover_target in super::patch_op_targets(op) {
                if !replace_keeps_node(op, &remover_target) {
                    // A rename/split never counts as a remover here either
                    // — same exclusion the discarded-addition pass applies on the additive side.
                    continue;
                }
                if remover_recreates_region(&remover_target, creators, op_index) {
                    continue;
                }
                emit_removed_node_edges_for_target(
                    &mut edges,
                    &mut seen,
                    &touchers,
                    remover_id,
                    op,
                    &remover_target,
                    is_reading_toucher_class,
                    sub_path_strictly_below,
                    true,
                    "replaces",
                );
            }
        }
    }

    edges
}

/// One remover target's own by-mod grouping, cosmetic classification, and
/// edge emission — shared by [`patch_removed_node_edges`]'s `Remove`- and
/// `Replace`-remover passes. `toucher_class_allowed` is the class
/// restriction only the `Replace`-remover pass applies
/// ([`READING_CLASS_SUFFIXES`]; the `Remove`-remover pass passes `|_| true`,
/// its own original, unrestricted behavior). `covers` is the coverage test:
/// [`sub_path_covers`] (`P` itself or a descendant) for the `Remove`-remover
/// pass, since an outright `Remove` destroys the node itself too, so any
/// toucher *at* that exact node genuinely needs to run first; but
/// [`sub_path_strictly_below`] for the `Replace`-remover pass — a
/// node-*keeping* `Replace`'s own target node is guaranteed to still exist
/// afterward by construction (`replace_keeps_node`), so a toucher whose own
/// `sub_path` names that *exact* node (a plain tag lookup, not a
/// content-based predicate) resolves identically regardless of order; only
/// content genuinely *below* that node is what the replacement's own new
/// value decides. `exclude_recreated` gates the predicate-`<li>` recreate
/// check ([`replace_recreates_touchers_target`]), meaningless for a plain
/// `Remove` (which has no `<value>` to recreate anything with) and
/// therefore `false` there. `verb` only changes the emitted detail text
/// ("removes"/"replaces").
#[allow(clippy::too_many_arguments)]
fn emit_removed_node_edges_for_target(
    edges: &mut Vec<Edge>,
    seen: &mut SeenEdges,
    touchers: &Touchers,
    remover_id: &ModId,
    remover_op: &PatchOp,
    remover_target: &DefTarget,
    toucher_class_allowed: impl Fn(&str) -> bool,
    covers: impl Fn(Option<&str>, Option<&str>) -> bool,
    exclude_recreated: bool,
    verb: &str,
) {
    let key = (
        remover_target.def_type.clone(),
        remover_target.def_name.clone(),
        remover_target.selector,
    );
    let Some(entries) = touchers.get(&key) else {
        return;
    };

    // Group this key's touchers by mod, skipping the remover's own entries
    // and every exclusion-excluded op outright — a `BTreeMap` keeps the grouping
    // deterministic regardless of scan order.
    let mut by_mod: BTreeMap<&ModId, Vec<&ToucherOp>> = BTreeMap::new();
    for entry in entries {
        if entry.mod_id == *remover_id {
            continue;
        }
        if !toucher_class_allowed(&entry.op.class) {
            continue;
        }
        if is_tolerant_toucher(entry.op, remover_target)
            || toucher_folder_gated_on_remover(entry.op, remover_id)
        {
            continue;
        }
        if exclude_recreated
            && replace_recreates_touchers_target(
                remover_op,
                remover_target,
                entry.sub_path.as_deref(),
            )
        {
            continue;
        }
        by_mod.entry(&entry.mod_id).or_default().push(entry);
    }

    for (other_id, other_ops) in by_mod {
        // Rule (a) is a *filter*, not one of the "every op must satisfy"
        // conditions: an op of `A` that never selects into `R`'s own removed
        // region is unaffected by `R` running at all, so it can neither
        // justify the edge nor veto a cosmetic classification — see
        // `toucher_op_is_cosmetic`'s own doc comment for why this is a
        // coordinator-reviewed correction to the original per-op rule (real
        // installs ship a mod whose *other*, out-of-region ops on the same
        // def were wrongly forcing `PatchRemovedNode` for a pair that's
        // actually cosmetic).
        let covering_ops: Vec<&&ToucherOp> = other_ops
            .iter()
            .filter(|entry| {
                covers(
                    remover_target.sub_path.as_deref(),
                    entry.sub_path.as_deref(),
                )
            })
            .collect();
        if covering_ops.is_empty() {
            continue;
        }
        let all_cosmetic = covering_ops
            .iter()
            .all(|entry| toucher_op_is_cosmetic(entry.op, remover_target));

        // Deliberately `display_path`, not `match_key`: this `Edge.subject`
        // is display text only (never matched against anything else), so it
        // stays selector-agnostic — see `DefTarget::display_path`'s own doc
        // comment.
        let subject = remover_target.display_path();
        let kind = if all_cosmetic {
            EdgeKind::PatchRemovedNodeCosmetic
        } else {
            EdgeKind::PatchRemovedNode
        };
        push_edge_once(
            edges,
            seen,
            remover_id.clone(),
            other_id.clone(),
            kind,
            format!("{verb} '{subject}', patched by {other_id}"),
            Some(subject),
        );
    }
}

/// Whether the `Replace`-remover pass should skip `toucher_sub_path` as a
/// candidate: `true` when `remover_op`'s own replacement value recreates a
/// predicate-keyed `<li>` at exactly the identity the toucher would need —
/// checked by reusing [`li_predicate_lookup_keys`] against
/// `remover_op.injected_paths` directly (not the cross-mod
/// `injected_path_owners` index [`patch_injected_node_edges`] builds, since
/// this only ever asks about *this one* replacer's own new value), the same
/// normalized predicate-`<li>` identity that lets the predicate-`<li>` producer
/// (`extract::patches::injected_li_predicate_paths_of`) match across a
/// quote-style difference. Scoped deliberately narrow, matching that producer's own
/// scope: a plain (non-`<li>`) child name recreated by coincidence isn't
/// checked, since there is no normalized identity to compare it against —
/// the safe direction here is to keep the edge (a missed exclusion only
/// costs one extra, still-mostly-true edge), never to assume recreation
/// without positive evidence.
fn replace_recreates_touchers_target(
    remover_op: &PatchOp,
    remover_target: &DefTarget,
    toucher_sub_path: Option<&str>,
) -> bool {
    let toucher_target = DefTarget {
        def_type: remover_target.def_type.clone(),
        def_name: remover_target.def_name.clone(),
        selector: remover_target.selector,
        sub_path: toucher_sub_path.map(str::to_string),
    };
    li_predicate_lookup_keys(&toucher_target)
        .iter()
        .any(|predicate_key| remover_op.injected_paths.contains(&predicate_key.key))
}

/// The three conditions (b-d of the cosmetic classification — (a) is the caller's own
/// coverage *filter*, checked before this function is ever called, never
/// repeated here) an individual, already-covering toucher op `A` must *all*
/// satisfy for its own contribution to a remover/toucher pair to be
/// cosmetic — [`patch_removed_node_edges`] requires this of **every** one of
/// a mod's own *covering* ops on the key before calling the whole pair
/// cosmetic; an op of `A` that never selects into `R`'s own removed region
/// is filtered out by the caller first and never reaches here at all, so it
/// can neither justify the edge nor veto a cosmetic classification (a
/// coordinator-reviewed correction: the original rule required *every* op on
/// the key, in-region or not, to individually re-prove coverage, which made
/// an unrelated, out-of-region op on the same real-install mod wrongly force
/// `PatchRemovedNode` for a pair that was actually cosmetic):
/// (b) [`PatchOp::sequence_tail`]: `A` has no later sibling in any enclosing
/// `PatchOperationSequence`, so a Sequence abort in one order can never skip
/// real, later work the other order would have kept;
/// (c) [`nomatch_conditional_targets_outside_removed_region`] is `false`:
/// `A` isn't reached through a `NoMatch` branch whose own Conditional tests a
/// location genuinely outside `R`'s removed region — such a toucher's own
/// participation isn't provably tied to `R`'s own removal at all;
/// (d) [`PatchOp::names_single_def`]: `A`'s own xpath names exactly one def,
/// since a disjunctive head can write outside `R`'s region under a different
/// name even when *this* target's own path is covered.
fn toucher_op_is_cosmetic(op: &PatchOp, remover_target: &DefTarget) -> bool {
    op.sequence_tail
        && !nomatch_conditional_targets_outside_removed_region(op, remover_target)
        && op.names_single_def
}

/// Whether `remover_target`'s own removed region is recreated by a *later*
/// op of `R`'s own mod (an op of `own_creators` positioned after
/// `remover_index`, `R`'s own position in its mod's `patch_ops`, which are
/// already in engine order) — a coordinator-reviewed
/// exclusion: when `R`'s own mod re-adds the region it just
/// removed, `R` never actually leaves it permanently absent, so there is no
/// genuine "must load after everyone who touches it" fact to assert
/// regardless of any other mod's own load position — [`patch_removed_node_edges`]
/// skips this remover target entirely rather than trying to classify it
/// cosmetic; it was never really a remover here. Two real-install shapes:
///
/// - **Exact recreate** (`remover_target.sub_path` is `Some`): a later,
///   active, mutating, creating-class op ([`NODE_CREATING_SUFFIXES`]) whose
///   own resolved target names the identical `(def_type, def_name,
///   selector, sub_path)`.
/// - **Parent recreate** (`remover_target.sub_path` is `Some`): a later,
///   active, mutating, creating-class op whose own resolved target is the
///   removed `sub_path`'s own *parent* (the def root itself, when the
///   removed `sub_path` was a single segment) and whose own
///   [`PatchOp::value_child_names`] names the removed `sub_path`'s own
///   trailing segment — the real-install case this rule exists for: a
///   compat mod removes `FloodLight/researchPrerequisites`, then its own
///   later `RR.PatchOperationAddOrReplace` targets `ThingDef[defName=
///   "FloodLight"]` itself (no `sub_path`) with a `<value>` naming
///   `<researchPrerequisites>` — "add or replace this child", which is
///   exactly an `Add` once the sibling `Remove` has already deleted it.
/// - **Whole-def recreate** (`remover_target.sub_path` is `None`): a later,
///   active, mutating op whose own `injected_paths` names the identical
///   whole def (the whole-`<Defs>`-injection shape `injected_paths_of`
///   already tracks for any mutating class) — a real content mod removes
///   `Table_RoyalDresser` outright, then injects a brand-new `ThingDef`
///   under that same `defName`.
///
/// Conservative on purpose: only an exact site/def match, or an exact
/// parent-plus-declared-child-name match, counts — never a same-named
/// sibling or a guess at what a parent-targeting op's own `<value>`
/// contains beyond its own top-level element names.
fn remover_recreates_region(
    remover_target: &DefTarget,
    own_creators: &OwnCreatingOps<'_>,
    remover_index: usize,
) -> bool {
    let Some(removed_sub_path) = remover_target.sub_path.as_deref() else {
        // Only the whole-`<Defs>`-injection shape's own key format — plain
        // "{def_type}/{def_name}", never `@`-prefixed — ever appears in
        // `injected_paths` for a fresh top-level def, so a `NameAttr`
        // remover target has no whole-def recreate shape to match against
        // at all.
        if remover_target.selector != Selector::DefName {
            return false;
        }
        let whole_def_key = format!("{}/{}", remover_target.def_type, remover_target.def_name);
        return own_creators
            .last_injecting
            .get(whole_def_key.as_str())
            .is_some_and(|last| *last > remover_index);
    };
    let (parent, last_segment) = sub_path_parent_and_last_segment(removed_sub_path);
    let key = (
        remover_target.def_type.clone(),
        remover_target.def_name.clone(),
        remover_target.selector,
    );
    own_creators.by_target.get(&key).is_some_and(|creators| {
        creators.iter().any(|creator| {
            creator.op_index > remover_index
                && (creator.sub_path.as_deref() == Some(removed_sub_path)
                    || (creator.sub_path.as_deref() == parent.as_deref()
                        && creator.op.value_child_names.contains(last_segment)))
        })
    })
}

/// One mod's own active, mutating, creating-class ops
/// ([`NODE_CREATING_SUFFIXES`]) — everything [`remover_recreates_region`]
/// can match a later recreate against, indexed once per mod rather than
/// re-read (and every xpath re-parsed) from the remover onward for each
/// remover target, which was quadratic in a mod's own op count.
struct OwnCreatingOps<'a> {
    /// Every resolved target of a creating op, by the def it names.
    by_target: BTreeMap<(String, String, Selector), Vec<CreatingTarget<'a>>>,
    /// Each whole-def `injected_paths` entry, mapped to the position of the
    /// last creating op injecting it.
    last_injecting: BTreeMap<&'a str, usize>,
}

/// One resolved target of a creating op, with that op's own position in
/// its mod's `patch_ops`.
struct CreatingTarget<'a> {
    op_index: usize,
    sub_path: Option<String>,
    op: &'a PatchOp,
}

impl<'a> OwnCreatingOps<'a> {
    fn build(patch_ops: &'a [PatchOp], active: &ActiveMods, name_map: &DisplayNameIndex) -> Self {
        let mut by_target: BTreeMap<_, Vec<CreatingTarget<'a>>> = BTreeMap::new();
        let mut last_injecting = BTreeMap::new();
        let creating_ops = patch_ops.iter().enumerate().filter(|(_, op)| {
            op.is_mutating
                && NODE_CREATING_SUFFIXES
                    .iter()
                    .any(|suffix| op.class.ends_with(suffix))
                && patch_op_active(op, active, name_map)
        });
        for (op_index, op) in creating_ops {
            for target in super::patch_op_targets(op) {
                by_target
                    .entry((target.def_type, target.def_name, target.selector))
                    .or_default()
                    .push(CreatingTarget {
                        op_index,
                        sub_path: target.sub_path,
                        op,
                    });
            }
            for path in &op.injected_paths {
                last_injecting.insert(path.as_str(), op_index);
            }
        }
        Self {
            by_target,
            last_injecting,
        }
    }
}

/// `sub_path`'s own parent (every `/`-delimited segment except the last,
/// bracket/quote-aware via [`split_sub_path_segments`]) and its own
/// trailing segment — `None` for the parent when `sub_path` is a single
/// segment (the parent is then the def root itself, which
/// [`DefTarget::sub_path`] represents as `None`, not `Some("")`).
fn sub_path_parent_and_last_segment(sub_path: &str) -> (Option<String>, &str) {
    let segments = split_sub_path_segments(sub_path);
    let last = *segments.last().unwrap_or(&sub_path);
    if segments.len() <= 1 {
        (None, last)
    } else {
        (Some(segments[..segments.len() - 1].join("/")), last)
    }
}

/// Additive op classes the discarded-addition pass considers — [`crate::domain::EdgeKind::ReplaceDiscardsAddition`]'s
/// own `A` side. `AttributeAdd`/`AttributeSet` write a scalar attribute
/// value, never a `<value>` subtree the same way `Add`/`AddModExtension`
/// do — [`addition_is_duplicate`] handles that shape difference (an empty
/// digest, from a textual `<value>`, never counts as a duplicate).
/// `PatchOperationInsert` is deliberately **not** here — see
/// [`READING_CLASS_SUFFIXES`]'s own doc comment for why it belongs
/// entirely to [`patch_removed_node_edges`]'s `Replace`-remover pass
/// instead.
const ADDITIVE_CLASS_SUFFIXES: [&str; 4] = [
    "PatchOperationAdd",
    "PatchOperationAddModExtension",
    "PatchOperationAttributeAdd",
    "PatchOperationAttributeSet",
];

fn is_additive_class(class: &str) -> bool {
    ADDITIVE_CLASS_SUFFIXES
        .iter()
        .any(|suffix| class.ends_with(suffix))
}

/// A later active `PatchOperationReplace` (`B`) discards an earlier
/// active mod's own addition (`A`) inside the replaced node, silently —
/// both operations report success and nothing is logged, unlike
/// [`patch_removed_node_edges`]'s own remove/touch shape, where the
/// toucher's own op at least fails visibly. RimWorld applies every active
/// mod's patch operations in one flat load-order sequence over one
/// combined document (`PatchOperationReplace.ApplyWorker`: inserts each
/// child of `<value>` before the matched node, then removes the node,
/// subtree and all — anything an earlier op wrote into it goes with it).
/// So whichever of `A`/`B` runs *second* decides the outcome: `B` after
/// `A` wipes `A`'s own addition out; `A` after `B` lands `A`'s addition
/// inside `B`'s own fresh replacement, keeping it. This function's own
/// edge therefore orders `A` **after** `B` (`Edge.after` is `A`'s own
/// mod) whenever four conditions all hold:
///
/// 1. **`B`'s value keeps the node** (`replace_keeps_node`): its
///    `<value>` has exactly one direct element child, named `P`'s own
///    last path segment. A value that renames or splits the node is
///    excluded outright — in that case `A`-after-`B` order makes `A`
///    itself *fail* (a different, visibly-logged problem), so no edge or
///    finding of this kind applies at all.
/// 2. **Not a duplicate** (`addition_is_duplicate`, negated): `A`'s own
///    added content isn't already present in `B`'s own replacement at the
///    matching relative path — otherwise `A`-after-`B` would add it
///    twice.
/// 3. **Owner policy**: when `B`'s own `About.xml` declares `loadAfter`,
///    `forceLoadAfter`, or `modDependencies` naming `A` (base-id aware,
///    [`crate::domain::DeclaredOrder::declares_after_or_dependency`]), the
///    replacer's own author already chose this outcome deliberately — no
///    edge, only a [`crate::domain::Conflict::DiscardedAddition`] finding
///    so the user can still see what was chosen away.
/// 4. `A` and `B` are different mods.
///
/// `A` is expanded the same way [`patch_removed_node_edges`]'s own
/// touchers are: every active op whose class is additive
/// (`is_additive_class`), indexed per `super::patch_op_targets` target
/// (a disjunctive head covers every def it names), and kept as a
/// candidate for `P` exactly when `sub_path_covers` says its own
/// `sub_path` is `P` itself or a real descendant of it — the same
/// same-node/ancestor-case test `patch_removed_node_edges` already uses
/// for its own remover/toucher region. `B` itself is excluded from being
/// its own `A` via a genuinely-conditional gate mirroring
/// [`patch_removed_node_edges`]'s own remover-side rule
/// (`is_genuinely_conditional_remove`, reused unchanged — it already
/// gates a `Replace` the same way it gates a `Remove`).
#[must_use]
pub fn replace_discards_addition(
    scanned: &[ScannedMod],
    indices: &Indices,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> (Vec<Edge>, Vec<Conflict>) {
    type Adders<'a> = BTreeMap<(String, String, Selector), Vec<ToucherOp<'a>>>;
    let mut adders: Adders = BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && is_additive_class(&op.class))
            .filter(|op| patch_op_active(op, active, name_map))
        {
            for target in super::patch_op_targets(op) {
                adders
                    .entry((target.def_type, target.def_name, target.selector))
                    .or_default()
                    .push(ToucherOp {
                        mod_id: scanned_mod.info.id.clone(),
                        op,
                        sub_path: target.sub_path,
                    });
            }
        }
    }

    let mut edges = Vec::new();
    let mut seen_edges: SeenEdges = HashSet::new();
    let mut conflicts = Vec::new();
    let mut seen_conflicts: HashSet<(ModId, ModId, String, String, Option<String>)> =
        HashSet::new();
    for scanned_mod in scanned {
        let replacer_id = &scanned_mod.info.id;
        for op in scanned_mod.patch_ops.iter().filter(|op| {
            op.is_mutating
                && op.class.ends_with("PatchOperationReplace")
                && patch_op_active(op, active, name_map)
        }) {
            if is_genuinely_conditional_remove(op) {
                continue;
            }
            for p in super::patch_op_targets(op) {
                if !replace_keeps_node(op, &p) {
                    continue;
                }
                let key = (p.def_type.clone(), p.def_name.clone(), p.selector);
                let Some(entries) = adders.get(&key) else {
                    continue;
                };
                let p_display = p.display_path();
                for entry in entries {
                    if entry.mod_id == *replacer_id {
                        continue;
                    }
                    if !sub_path_covers(p.sub_path.as_deref(), entry.sub_path.as_deref()) {
                        continue;
                    }
                    if addition_is_duplicate(
                        entry.op,
                        op,
                        p.sub_path.as_deref(),
                        entry.sub_path.as_deref(),
                    ) {
                        continue;
                    }
                    let adder_display = DefTarget {
                        def_type: p.def_type.clone(),
                        def_name: p.def_name.clone(),
                        selector: p.selector,
                        sub_path: entry.sub_path.clone(),
                    }
                    .display_path();
                    let declares_override =
                        indices
                            .mod_declared
                            .get(replacer_id)
                            .is_some_and(|declared| {
                                declared.declares_after_or_dependency(&entry.mod_id)
                            });
                    if declares_override {
                        let conflict_key = (
                            replacer_id.clone(),
                            entry.mod_id.clone(),
                            p.def_type.clone(),
                            p.def_name.clone(),
                            Some(p_display.clone()),
                        );
                        if seen_conflicts.insert(conflict_key) {
                            conflicts.push(Conflict::DiscardedAddition(DiscardedAddition {
                                replacer: replacer_id.clone(),
                                adder: entry.mod_id.clone(),
                                def_type: p.def_type.clone(),
                                def_name: p.def_name.clone(),
                                path: p_display.clone(),
                                adder_path: adder_display,
                            }));
                        }
                        continue;
                    }
                    push_edge_once(
                        &mut edges,
                        &mut seen_edges,
                        entry.mod_id.clone(),
                        replacer_id.clone(),
                        EdgeKind::ReplaceDiscardsAddition,
                        format!(
                            "replaces '{p_display}', discarding {}'s own addition at '{adder_display}' unless it loads after",
                            entry.mod_id
                        ),
                        Some(p_display.clone()),
                    );
                }
            }
        }
    }
    (edges, conflicts)
}

/// Rule 1 of [`replace_discards_addition`]: `op`'s own `<value>` has
/// exactly one direct element child, named `target`'s own last path
/// segment — the replacement genuinely keeps the replaced node's own
/// identity rather than renaming or splitting it.
fn replace_keeps_node(op: &PatchOp, target: &DefTarget) -> bool {
    matches!(
        op.value_root_names.as_slice(),
        [only] if only == crate::extract::patches::target_last_step_tag(target)
    )
}

/// Rule 2 of [`replace_discards_addition`]: whether `adder_op`'s own added
/// content is already present in `replacer_op`'s own replacement value, at
/// the matching relative path. `false` (never a duplicate, so the edge is
/// kept) whenever either side's digest is missing, empty, or truncated —
/// an empty digest means the adder's own value has no element content to
/// compare at all (the `AttributeAdd`/`AttributeSet` shape, or a plain
/// scalar `<value>`), and a truncated one means this function cannot
/// *prove* a duplicate, which must never read as "assume one" (the safe
/// direction: a missed duplicate only costs one extra, still-true edge, a
/// wrongly-assumed one would hide a real content loss).
fn addition_is_duplicate(
    adder_op: &PatchOp,
    replacer_op: &PatchOp,
    p_sub_path: Option<&str>,
    a_sub_path: Option<&str>,
) -> bool {
    let Some(adder_digest) = &adder_op.value_digest else {
        return false;
    };
    let Some(replacer_digest) = &replacer_op.value_digest else {
        return false;
    };
    if adder_digest.entries.is_empty() || adder_digest.truncated || replacer_digest.truncated {
        return false;
    }
    let Some(relative) = relative_sub_path(a_sub_path, p_sub_path) else {
        return false;
    };
    // The replacer's digest keys start at its `<value>` root, which is the
    // replaced node itself; the adder's start at the children it adds to
    // `a_sub_path`. Prefix the replaced node's tag (and the path down from
    // it) onto the adder's keys so both name the same node. `Replace`
    // rule 1 already guarantees exactly one root.
    let [replacer_root] = replacer_op.value_root_names.as_slice() else {
        return false;
    };
    adder_digest.entries.iter().all(|(path, hash)| {
        let aligned = if relative.is_empty() {
            format!("{replacer_root}/{path}")
        } else {
            format!("{replacer_root}/{relative}/{path}")
        };
        replacer_digest.entries.contains(&(aligned, *hash))
    })
}

/// `a_sub_path`'s own segments with `p_sub_path`'s own leading segments
/// stripped — `None` when `a_sub_path` doesn't actually start with
/// `p_sub_path` (shouldn't happen once [`sub_path_covers`] already
/// confirmed coverage, but checked directly here rather than assumed).
/// `Some(String::new())` for the same-node case (`a_sub_path ==
/// p_sub_path`, segment-for-segment).
fn relative_sub_path(a_sub_path: Option<&str>, p_sub_path: Option<&str>) -> Option<String> {
    match (a_sub_path, p_sub_path) {
        (None, None) => Some(String::new()),
        (Some(a), None) => Some(a.to_string()),
        (None, Some(_)) => None,
        (Some(a), Some(p)) => {
            let a_segments = split_sub_path_segments(a);
            let p_segments = split_sub_path_segments(p);
            if a_segments.len() < p_segments.len()
                || a_segments[..p_segments.len()] != p_segments[..]
            {
                return None;
            }
            Some(a_segments[p_segments.len()..].join("/"))
        }
    }
}

/// Rule 6(c): `true` when toucher op `op` is reached through a Conditional's
/// own `NoMatch` branch *and* that Conditional's own tested xpath lies
/// outside the remover's own removed region — parses `op`'s own
/// `conditional_xpath` text (never re-derived from `op`'s own target, since
/// the Conditional's own tested node is a different location from `op`'s
/// own write) and compares it against `remover_target` the same way
/// [`sub_path_covers`] compares any other toucher. `false` (not excluded)
/// whenever `op` isn't reached through a `NoMatch` branch at all, or the
/// Conditional's own xpath is unparseable — the strict, lean-towards-content
/// direction only ever narrows *which* touchers are exempt from this one
/// check, not whether an edge exists at all.
fn nomatch_conditional_targets_outside_removed_region(
    op: &PatchOp,
    remover_target: &DefTarget,
) -> bool {
    if op.conditional_branch != Some(ConditionalBranch::NoMatch) {
        return false;
    }
    let Some(conditional_xpath) = op.conditional_xpath.as_deref() else {
        return false;
    };
    match xpath_target::parse(conditional_xpath) {
        Some(_) => !conditional_xpath_within_removed_region(conditional_xpath, remover_target),
        // Unparseable: can't prove it lies inside `R`'s region, so treat it
        // as outside (the strict, content-leaning direction).
        None => true,
    }
}

/// Whether `conditional_xpath` (the nearest enclosing Conditional's own
/// tested xpath) resolves to a site at or beneath `remover_target`'s own
/// removed region — shared by both
/// [`nomatch_conditional_targets_outside_removed_region`] (negated) and
/// [`is_tolerant_toucher`]. Unparseable xpaths are treated as *not*
/// covered, the same strict, content-leaning direction both callers already
/// use for their own "can't prove it" case.
fn conditional_xpath_within_removed_region(
    conditional_xpath: &str,
    remover_target: &DefTarget,
) -> bool {
    let Some(conditional_target) = xpath_target::parse(conditional_xpath) else {
        return false;
    };
    conditional_target.def_type == remover_target.def_type
        && conditional_target.def_name == remover_target.def_name
        && conditional_target.selector == remover_target.selector
        && sub_path_covers(
            remover_target.sub_path.as_deref(),
            conditional_target.sub_path.as_deref(),
        )
}

/// Tolerant-toucher exclusion: a toucher this tolerant of the remover's own removal is
/// skipped outright (never counted towards emitting an edge, cosmetic or
/// not). `op` sits in the `Match` branch of a Conditional, and either:
///
/// - the Conditional's own tested xpath equals `op`'s own xpath, or
///   resolves to a site at or beneath the remover's own removed region
///   ([`conditional_xpath_within_removed_region`]) — ground-truthed against
///   the decompiled engine: `PatchOperationConditional.ApplyWorker` returns
///   success (no error logged) for a match-only Conditional whenever the
///   tested node is simply absent, with or without a `<nomatch>` branch at
///   all. Once the remover has run, the tested node (or the region it sits
///   in) is gone, so the Match branch's own op silently never runs — `op`'s
///   own author already tolerated that outcome by writing a Conditional
///   around it, whether or not `<nomatch>` recreates anything; or
/// - the Conditional's own tested xpath is `op`'s own xpath's parent
///   ([`parent_xpath`]) *and* that Conditional's `NoMatch` branch holds a
///   creating op ([`PatchOp::conditional_nomatch_creates`]): here the
///   remover's absence doesn't make the Conditional itself silently no-op
///   (the parent may still exist), so tolerance only holds when the
///   `NoMatch` branch actively recreates the site `op` expects — the same
///   tolerance idea `is_genuinely_conditional_remove` already applies to the
///   remover side, extended here with branch polarity. Kept gated on
///   `conditional_nomatch_creates`, unlike the two cases above.
fn is_tolerant_toucher(op: &PatchOp, remover_target: &DefTarget) -> bool {
    if op.conditional_branch != Some(ConditionalBranch::Match) {
        return false;
    }
    let Some(conditional) = op.conditional_xpath.as_deref() else {
        return false;
    };
    let same_site = op.xpath.as_deref() == Some(conditional);
    if same_site || conditional_xpath_within_removed_region(conditional, remover_target) {
        return true;
    }
    op.conditional_nomatch_creates
        && op.xpath.as_deref().and_then(parent_xpath).as_deref() == Some(conditional)
}

/// `xpath`'s own parent path: every `/`-delimited segment except the last
/// (bracket/quote-aware, via [`split_sub_path_segments`] — the same rule
/// applied to a `sub_path` here applied to a whole xpath, since neither
/// splitter cares what precedes the string it's given). `None` when `xpath`
/// is a single segment with no parent to name.
fn parent_xpath(xpath: &str) -> Option<String> {
    let segments = split_sub_path_segments(xpath);
    if segments.len() <= 1 {
        return None;
    }
    Some(segments[..segments.len() - 1].join("/"))
}

/// A toucher whose own loaded folder is gated
/// `IfModActive`/`IfModActiveAll` on the remover's own mod (compared by
/// [`ModId::base`], the same `_steam`-suffix-insensitive comparison used
/// throughout this module) is skipped outright — that folder was written
/// against the remover's own end state, so "R after A" is never emitted
/// for it, cosmetic or not.
fn toucher_folder_gated_on_remover(op: &PatchOp, remover_id: &ModId) -> bool {
    op.load_folder_gate
        .iter()
        .any(|gated| gated.base() == remover_id.base())
}

/// A mutating op nested under a `PatchOperationConditional` whose own
/// xpath *differs* from the op's own is genuinely conditional — it only
/// runs when the Conditional's own (different) xpath resolves. A
/// same-xpath Conditional ("if exists, then remove/replace") is
/// unconditional in effect and returns `false` here — see
/// [`patch_removed_node_edges`]'s own doc comment for the reading this
/// feeds and its known polarity gap. Despite the name (kept from this
/// function's original, `PatchOperationRemove`-only caller), nothing
/// here reads `op.class` — [`patch_invalidates_predicate_edges`] reuses
/// it unchanged for a `Replace` gate too.
pub(crate) fn is_genuinely_conditional_remove(op: &PatchOp) -> bool {
    op.conditional_xpath
        .as_deref()
        .is_some_and(|conditional| Some(conditional) != op.xpath.as_deref())
}

/// Whether an "other" mod's own `sub_path` sits inside the region a remover
/// targeting `remover_sub_path` deletes: `remover_sub_path = None` is a
/// whole-def remove and covers *everything*, including another `None`; a
/// `Some(p)` remover only covers an "other" `Some` sub_path that is `p`
/// itself or a real path-segment descendant of it — an "other" op touching
/// the *whole* def (`None`) is **not** covered by a partial remove, since the
/// whole def is an ancestor of `p`, not something at or beneath it ("P or a
/// descendant of P" names only those two cases).
fn sub_path_covers(remover_sub_path: Option<&str>, other_sub_path: Option<&str>) -> bool {
    match (remover_sub_path, other_sub_path) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(p), Some(other)) => is_path_segment_descendant_or_equal(p, other),
    }
}

/// A real `/`-delimited path-segment prefix test: `candidate` is
/// `ancestor` itself or lies strictly beneath it. Never a bare
/// `str::starts_with` — `statBases` must not match `statBasesExtra`.
fn is_path_segment_descendant_or_equal(ancestor: &str, candidate: &str) -> bool {
    let ancestor_segments: Vec<&str> = ancestor.split('/').collect();
    let candidate_segments: Vec<&str> = candidate.split('/').collect();
    candidate_segments.len() >= ancestor_segments.len()
        && candidate_segments[..ancestor_segments.len()] == ancestor_segments[..]
}

/// The `Replace`-remover pass's own coverage test — unlike [`sub_path_covers`]
/// (which a `Remove` remover correctly uses, since it destroys the node
/// itself too), a node-*keeping* `Replace`'s own target node is guaranteed
/// to still exist afterward (`replace_keeps_node`'s own construction: the
/// new value's root element shares the old node's own tag), so a toucher
/// whose own `sub_path` names that *exact* node — a plain tag-name lookup,
/// never a content-based predicate — resolves identically regardless of
/// order and needs no edge at all. Only `other_sub_path` genuinely *below*
/// `remover_sub_path` is covered here; `remover_sub_path = None` (a
/// whole-def replace) still covers any `Some` sub_path, since every field
/// below the def root is exactly what the replacement's own new value
/// decides. A `None` `other_sub_path`, or one equal to `remover_sub_path`,
/// is never covered.
fn sub_path_strictly_below(remover_sub_path: Option<&str>, other_sub_path: Option<&str>) -> bool {
    match (remover_sub_path, other_sub_path) {
        (None, Some(_)) => true,
        (None, None) | (Some(_), None) => false,
        (Some(p), Some(other)) => {
            let p_segments: Vec<&str> = p.split('/').collect();
            let other_segments: Vec<&str> = other.split('/').collect();
            other_segments.len() > p_segments.len()
                && other_segments[..p_segments.len()] == p_segments[..]
        }
    }
}

/// From the same per-op data [`patch_removed_node_edges`] reads — every
/// active mutating op `B` whose class ends `PatchOperationReplace` or
/// `PatchOperationRemove` and whose own `sub_path` is exactly a predicate
/// step `P/S[K="v"]` (`Replace` only; a same-shape `Remove` is already
/// [`patch_removed_node_edges`]'s job — see [`predicate_invalidation_shape`])
/// or exactly that step's own predicate key child, `P/S[K="v"]/K` (either
/// class) — against every other active mutating op `A`, in a different mod,
/// on the same `(def_type, def_name, selector)`, whose own `sub_path` starts
/// with the same `P/S[K="v"]` segments — emits `after: B's mod, before: A's
/// mod`: RimWorld combines every active mod's Defs into one document and runs
/// every mod's patch operations over it in load order, each xpath evaluated
/// against the document's *current* state, so once `B` rewrites or deletes
/// the node `A`'s own predicate reads, `A`'s op must already have run.
///
/// Real case:
/// a real install's replacer mod replaces
/// `Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label`
/// (the predicate key child shape); `example.vecontent.predicatereader`
/// adds to `.../degreeDatas/li[label="iron-willed"]/statOffsets` (the
/// same `degreeDatas/li[label="iron-willed"]` prefix) — both share the
/// predicate step, and the replacer's replace invalidates it, so the
/// predicate reader must load first.
///
/// A step's bracket may carry several predicates
/// (`li[label="x"][foo="y"]`); `K` matches any of them
/// ([`parse_predicate_step`]). A predicate this module cannot resolve to
/// a plain `key="value"` equality (`@attr=`, `text()`, `contains(`,
/// `not(`, an `and`/`or` composition — anything
/// [`parse_simple_equality_predicate`] doesn't recognize) is out of
/// scope for that predicate: no candidate `K`, no edge from it. Same-mod
/// pairs, `A == B`, and a genuinely conditional `B`
/// ([`is_genuinely_conditional_remove`] — despite the name, gates a
/// `Replace` here too) are skipped, mirroring [`patch_removed_node_edges`].
///
/// **`A`'s own tolerance for the predicate's absence is checked too**
/// (without it, real installs produce false mutual 2-cycles). Real case:
/// Example Core and Example Portal Kit both carry the *identical* op —
/// `PatchOperationConditional[xpath = X]/PatchOperationReplace[xpath = X]` on
/// `RoyalTitleDef[defName="Knight"]/throneRoomRequirements//
/// li[@Class="RoomRequirement_ThingCount"][thingDef="Column"]`, an "if it
/// exists, then replace it" idiom. Each mod's own op is a valid `B` (an
/// unconditional-in-effect Replace of the predicated node itself, shape 2)
/// *and* a toucher whose own `sub_path` matches the other's `B` as a
/// candidate `A` — but as `A`, each one's own Conditional guards exactly the
/// case where the predicate is gone: if the *other* mod's `B` already
/// invalidated it, this op's own Conditional simply doesn't fire, no error,
/// nothing tolerated wrongly. That is not "`A` depends on this predicate
/// resolving", it is the opposite — `A` is written to tolerate it not
/// resolving — so it must never be a `before` candidate.
/// [`is_genuinely_conditional_remove`]'s own equality test, read as "does
/// `A`'s own Conditional exactly guard `A`'s own xpath", is exactly this
/// tolerance check; the touchers index records it per entry so the emission
/// loop can skip a tolerant `A`.
#[must_use]
pub fn patch_invalidates_predicate_edges(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> Vec<Edge> {
    // Same touchers-index shape as `patch_removed_node_edges`, widened
    // with one more field this producer alone needs — local to this
    // function for the same reason: the two producers are siblings, not
    // variants of one shared index some third caller also needs.
    type Touchers = BTreeMap<(String, String, Selector), Vec<(ModId, Option<String>, bool)>>;
    let mut touchers: Touchers = BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            // See this function's own doc comment: an op nested under a
            // Conditional whose own xpath exactly equals this op's own
            // xpath ("if it exists, then ...") tolerates the predicate it
            // reads being absent — it must never source an edge as `A`.
            let tolerates_absence = op.conditional_xpath.as_deref() == op.xpath.as_deref()
                && op.conditional_xpath.is_some();
            for target in super::patch_op_targets(op) {
                touchers
                    .entry((target.def_type, target.def_name, target.selector))
                    .or_default()
                    .push((
                        scanned_mod.info.id.clone(),
                        target.sub_path,
                        tolerates_absence,
                    ));
            }
        }
    }

    let mut edges = Vec::new();
    let mut seen: SeenEdges = HashSet::new();
    for scanned_mod in scanned {
        let after_id = &scanned_mod.info.id;
        for op in scanned_mod.patch_ops.iter().filter(|op| {
            op.is_mutating
                && (op.class.ends_with("PatchOperationReplace")
                    || op.class.ends_with("PatchOperationRemove"))
                && patch_op_active(op, active, name_map)
        }) {
            if is_genuinely_conditional_remove(op) {
                continue;
            }
            let is_remove = op.class.ends_with("PatchOperationRemove");
            for target in super::patch_op_targets(op) {
                let Some(sub_path) = target.sub_path.as_deref() else {
                    continue;
                };
                let Some((prefix_segments, key, shape)) =
                    predicate_invalidation_shape(sub_path, is_remove)
                else {
                    continue;
                };
                // The invalidated subject and the verb both depend on which
                // shape matched — shape 2 replaces the predicated node itself
                // (never the `Add`ed key-child path, which names a node this
                // op never actually touched), and only shape 1 can ever be a
                // `Remove`.
                let subject = match shape {
                    PredicateShape::KeyChild => DefTarget {
                        def_type: target.def_type.clone(),
                        def_name: target.def_name.clone(),
                        selector: target.selector,
                        sub_path: Some(format!("{}/{key}", prefix_segments.join("/"))),
                    }
                    .display_path(),
                    PredicateShape::PredicatedNode => DefTarget {
                        def_type: target.def_type.clone(),
                        def_name: target.def_name.clone(),
                        selector: target.selector,
                        sub_path: Some(prefix_segments.join("/")),
                    }
                    .display_path(),
                };
                let verb = if is_remove { "removes" } else { "replaces" };

                let map_key = (
                    target.def_type.clone(),
                    target.def_name.clone(),
                    target.selector,
                );
                let Some(entries) = touchers.get(&map_key) else {
                    continue;
                };
                for (before_id, before_sub_path, tolerates_absence) in entries {
                    if before_id == after_id {
                        continue;
                    }
                    if *tolerates_absence {
                        continue;
                    }
                    // A whole-def `A` (`sub_path: None`, e.g. a bare
                    // `PatchOperationAdd` under `Defs/ThingDef[defName="X"]`
                    // with nothing after the head) reads no predicate at all
                    // — it targets the def root, not a nested node — so it
                    // cannot depend on this one. Mirrors `sub_path_covers`'s
                    // own reading of a `None` sub_path as "not a specific
                    // node", just on the *toucher* side instead of the
                    // remover's.
                    let Some(before_sub_path) = before_sub_path else {
                        continue;
                    };
                    if !sub_path_starts_with_segments(before_sub_path, &prefix_segments) {
                        continue;
                    }
                    push_edge_once(
                        &mut edges,
                        &mut seen,
                        after_id.clone(),
                        before_id.clone(),
                        EdgeKind::PatchInvalidatesPredicate,
                        format!("{verb} '{subject}' that {before_id}'s patch selects by"),
                        Some(subject.clone()),
                    );
                }
            }
        }
    }
    edges
}

/// Splits a raw patch `sub_path` on `/`, treating a `/` inside a
/// bracketed predicate (including one inside a quoted value, e.g.
/// `li[label="a/b"]`) as part of that predicate rather than a segment
/// boundary — the same quote/bracket-depth rule
/// [`crate::extract::xpath_target::find_matching_bracket`] applies, just
/// scanning the whole string instead of one leading bracket.
fn split_sub_path_segments(sub_path: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (index, ch) in sub_path.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' => depth += 1,
            ']' => depth -= 1,
            '/' if depth == 0 => {
                segments.push(&sub_path[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    segments.push(&sub_path[start..]);
    segments
}

/// A bare XML child-element name: `HEAD_RE`'s own `def_type` char class
/// (`[A-Za-z_][\w.:-]*`) — never `@attr`, `text()`, `contains(...)`, or
/// anything else carrying a character outside that set.
fn is_plain_child_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '-')
}

/// Parses one path segment as a predicate step, `Tag[pred1][pred2]...` —
/// `None` when the segment has no bracket, its tag isn't a plain child
/// name, or a bracket is unbalanced (the same tolerant-but-strict
/// contract [`crate::extract::xpath_target::locate_head`] applies to a
/// def head). The returned predicate texts are each bracket's raw inner
/// text, unparsed — [`parse_simple_equality_predicate`] is what decides
/// whether any given one is a usable `key="value"` equality.
fn parse_predicate_step(segment: &str) -> Option<(&str, Vec<&str>)> {
    let bracket_start = segment.find('[')?;
    let tag = &segment[..bracket_start];
    if !is_plain_child_name(tag) {
        return None;
    }
    let mut rest = &segment[bracket_start..];
    let mut predicates = Vec::new();
    while rest.starts_with('[') {
        let close = crate::extract::xpath_target::find_matching_bracket(rest)?;
        predicates.push(&rest[1..close]);
        rest = &rest[close + 1..];
    }
    (rest.is_empty()).then_some((tag, predicates))
}

/// Whether `predicate_text` (one bracket's raw inner text) is exactly a
/// plain `key="value"` or `key='value'` equality — `Some((key, value))`
/// if so. Rejects an `@attr=` predicate (its key would start with `@`,
/// which [`is_plain_child_name`] refuses), `text()`/`contains(`/`not(`
/// (a `(` character, likewise refused), an `and`/`or` composition, and
/// any other shape this module doesn't model — deliberately narrower
/// than [`crate::extract::xpath_target::HEAD_NAME_RE`], which only ever
/// looks for `defName=`/`@Name=` specifically and would be the wrong
/// tool for an arbitrary child predicate like this one.
pub(super) fn parse_simple_equality_predicate(predicate_text: &str) -> Option<(&str, &str)> {
    let trimmed = predicate_text.trim();
    let (key, value_part) = trimmed.split_once('=')?;
    let key = key.trim();
    if !is_plain_child_name(key) {
        return None;
    }
    Some((key, parse_quoted_value(value_part)?))
}

/// The quote-parsing half [`parse_simple_equality_predicate`],
/// [`parse_attribute_equality_predicate`], and
/// [`parse_text_equality_predicate`] all share: given the raw text after
/// a predicate's own `=` sign, extract a plain double- or single-quoted
/// value. `None` for anything else (no quote, unterminated, or trailing
/// content after the closing quote — see
/// [`parse_simple_equality_predicate`]'s own doc comment for why the
/// *first* closing quote is the right one, never `rfind`).
fn parse_quoted_value(value_part: &str) -> Option<&str> {
    let value_part = value_part.trim();
    let mut chars = value_part.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &value_part[quote.len_utf8()..];
    let closing = quote.len_utf8() + rest.find(quote)?;
    if closing + quote.len_utf8() != value_part.len() {
        return None;
    }
    Some(&value_part[quote.len_utf8()..closing])
}

/// The sibling of [`parse_simple_equality_predicate`] for an *attribute*
/// equality (`[@Attr="value"]`) rather than a child-text equality
/// (`[key="value"]`) — feeds [`li_predicate_lookup_keys`], the consumer
/// side of the predicate-keyed `<li>` shape
/// `extract::patches::injected_li_predicate_paths_of` produces on the
/// producer side. Rejects everything [`parse_simple_equality_predicate`]
/// does, plus a bare (non-`@`-prefixed) key.
fn parse_attribute_equality_predicate(predicate_text: &str) -> Option<(&str, &str)> {
    let trimmed = predicate_text.trim();
    let rest = trimmed.strip_prefix('@')?;
    let (key, value_part) = rest.split_once('=')?;
    let key = key.trim();
    if !is_plain_child_name(key) {
        return None;
    }
    Some((key, parse_quoted_value(value_part)?))
}

/// A `text()="value"` predicate — the third shape
/// [`li_predicate_lookup_keys`] recognizes, for a bare-text `<li>` (no
/// `Class` attribute, no element children — the "list of plain defName
/// references" idiom).
fn parse_text_equality_predicate(predicate_text: &str) -> Option<&str> {
    let trimmed = predicate_text.trim();
    let value_part = trimmed.strip_prefix("text()")?.trim_start();
    let value_part = value_part.strip_prefix('=')?;
    parse_quoted_value(value_part)
}

/// Whether a predicate-keyed lookup key may be checked against the inline
/// node index (`Indices::inline_node_paths`).
///
/// The inline index records each inline `<li>` under its *one* recognized
/// identity. A key therefore only proves "a Def already ships this step"
/// when it names the whole step: the **last** segment of the selecting op's
/// path, with exactly one predicate bracket. A multi-predicate step
/// (`li[@Class="X"][thingDef="Column"]` collapses to `li[@Class="X"]`) or a
/// read-through step (`li[@Class="X"]/things/li[...]`, where an inline
/// `<li>` proves only that the container exists) must not demote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InlineCheck {
    /// The key is the whole path, or a whole, last, single-predicate step.
    Applicable,
    /// The key under-describes the selecting op's step; skip the inline index.
    NotApplicable,
}

/// One normalized predicate-`<li>` lookup key and whether it may consult the
/// inline index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiPredicateKey {
    key: String,
    inline_check: InlineCheck,
}

/// The consumer-side half of the predicate-keyed `<li>` shape (see
/// `extract::patches::injected_li_predicate_paths_of`'s own doc comment for
/// the producer side): scans **every** segment of `target`'s own
/// `sub_path` for an `li[@Attr="v"]`/`li[text()="v"]` step — not only the
/// last one, since a real-install op can read *through* a predicate-keyed
/// `<li>` on its way to a deeper child (`li[@Class="X"]/things/li[text()="Y"]`)
/// — and returns one normalized lookup key per match, in the identical
/// text the producer side builds (`extract::patches::li_predicate_suffix`),
/// each prefixed by every segment before its own `li` step. Empty when
/// `target` carries no `sub_path`, or when no segment resolves to a
/// predicate this module can recognize as a plain equality. Tries the
/// attribute shape first, then the text shape, on each step's own
/// predicates in order — a step could in principle carry both
/// (`li[@Class="X"][text()="Y"]`), and either being recognized is enough
/// to build that step's own candidate key.
///
/// Each key carries an [`InlineCheck`]: `Applicable` only for the last
/// segment, and only when its bracket set holds exactly one predicate.
fn li_predicate_lookup_keys(target: &DefTarget) -> Vec<LiPredicateKey> {
    let Some(sub_path) = target.sub_path.as_deref() else {
        return Vec::new();
    };
    let segments = split_sub_path_segments(sub_path);
    let mut keys = Vec::new();
    // Every segment, not just the last: a selecting op's own path can
    // read *through* a predicate-keyed `<li>` on its way to a deeper
    // child (e.g. `li[@Class="X"]/things/li[text()="Y"]`, a real-install
    // shape) — the dependency is on that intermediate `<li>` existing,
    // wherever in the path it sits, not only on a trailing one.
    for (index, segment) in segments.iter().enumerate() {
        let Some((tag, predicates)) = parse_predicate_step(segment) else {
            continue;
        };
        if tag != "li" {
            continue;
        }
        let Some(identity) = predicates.iter().find_map(|predicate| {
            parse_attribute_equality_predicate(predicate)
                .map(|(attr, value)| (Some(attr), value.to_string()))
                .or_else(|| {
                    parse_text_equality_predicate(predicate).map(|value| (None, value.to_string()))
                })
        }) else {
            continue;
        };
        let prefix = DefTarget {
            def_type: target.def_type.clone(),
            def_name: target.def_name.clone(),
            selector: target.selector,
            sub_path: (index > 0).then(|| segments[..index].join("/")),
        }
        .match_key();
        let is_whole_last_step = index + 1 == segments.len() && predicates.len() == 1;
        keys.push(LiPredicateKey {
            key: format!(
                "{prefix}/{}",
                crate::extract::patches::li_predicate_suffix(&identity)
            ),
            inline_check: if is_whole_last_step {
                InlineCheck::Applicable
            } else {
                InlineCheck::NotApplicable
            },
        });
    }
    keys
}

/// Which of the two predicate-invalidation shapes
/// [`predicate_invalidation_shape`] matched — see
/// [`patch_invalidates_predicate_edges`]'s own doc comment for both. Threaded
/// through to the emission loop so the edge's own subject/verb can name what
/// `B` actually did: shape 1's subject is the key child itself and can be
/// either `Replace`d or `Remove`d; shape 2's subject is the predicated node
/// itself and is always `Replace`d (a same-shape `Remove` is
/// `PatchRemovedNode`'s own job, never this producer's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PredicateShape {
    /// `P/S[K="v"]/K` — the predicate's own key child.
    KeyChild,
    /// `P/S[K="v"]` — the predicated node itself.
    PredicatedNode,
}

/// The `(prefix segments, predicate key, shape)` a mutating op's own
/// `sub_path` must match for [`patch_invalidates_predicate_edges`] to
/// consider it — see that function's own doc comment for the two
/// shapes. `None` when `sub_path` matches neither.
fn predicate_invalidation_shape(
    sub_path: &str,
    is_remove: bool,
) -> Option<(Vec<&str>, &str, PredicateShape)> {
    let segments = split_sub_path_segments(sub_path);
    let last = *segments.last()?;

    // Shape 2: `P/S[K="v"]` itself — a `Replace` of the predicated node
    // (a same-shape `Remove` is `PatchRemovedNode`'s own job already).
    if !is_remove && let Some((_, predicates)) = parse_predicate_step(last) {
        for predicate in predicates {
            if let Some((key, _value)) = parse_simple_equality_predicate(predicate) {
                return Some((segments.clone(), key, PredicateShape::PredicatedNode));
            }
        }
    }

    // Shape 1: `P/S[K="v"]/K` — the predicate's own key child, named as
    // the sub_path's trailing plain segment.
    if segments.len() >= 2 && is_plain_child_name(last) {
        let step = segments[segments.len() - 2];
        if let Some((_, predicates)) = parse_predicate_step(step) {
            for predicate in predicates {
                if let Some((key, _value)) = parse_simple_equality_predicate(predicate)
                    && key == last
                {
                    return Some((
                        segments[..segments.len() - 1].to_vec(),
                        key,
                        PredicateShape::KeyChild,
                    ));
                }
            }
        }
    }

    None
}

/// Whether `sub_path`'s own segments start with `prefix` — the same
/// "op `A`'s xpath depends on this predicate step" test
/// [`patch_invalidates_predicate_edges`] applies to every other toucher.
fn sub_path_starts_with_segments(sub_path: &str, prefix: &[&str]) -> bool {
    let segments = split_sub_path_segments(sub_path);
    segments.len() >= prefix.len() && segments[..prefix.len()] == prefix[..]
}
