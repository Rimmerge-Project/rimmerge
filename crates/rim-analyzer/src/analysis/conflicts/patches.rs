//! Patch collisions: grouping patch operations by the node they touch, and grading their severity.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::domain::{
    Conflict, DefTarget, LoadOrder, ModId, PatchCollision, PatchCollisionEntry,
    PatchCollisionSeverity, PatchOp, ScannedMod, Selector,
};
use crate::extract::patches::has_append_semantics;

use crate::analysis::indices::{ActiveMods, DisplayNameIndex, patch_op_active};

/// `BTreeMap`, not `HashMap`: iterated below to build the `Conflict`
/// list, and `HashMap`'s per-process random iteration order would make
/// two runs over the same install produce a differently-ordered report.
type CollisionKey = (String, String, Selector, Option<String>);

/// The top-level element names an **append-semantics** op
/// (`Add`/`AddModExtension` — see [`has_append_semantics`]) injects at
/// `target`, derived from [`PatchOp::injected_paths`] rather than re-walking
/// the op's own `<value>` a second time. Empty for any other class (`Insert`
/// included — see this function's own doc note below) and for an append op
/// whose injection has no addressable top-level shape for this specific
/// target (every entry dropped for a bracketed `sub_path` or an `li` segment
/// — [`PatchOp::injected_paths`]'s own doc comment).
///
/// **`Insert` is deliberately excluded** — `injected_paths` covers
/// append-semantics classes only (`PatchOp::injected_paths`'s doc comment: "a
/// sibling `PatchOperationInsert` creates next to an existing node ... never
/// captured — Insert has no append-to-target semantics this shape can
/// honor"), so `injected_paths` never carries an `Insert`'s own entries in
/// the first place; `has_append_semantics` is checked explicitly here too, as
/// defense in depth documenting that narrowing for this call site directly,
/// rather than relying solely on the upstream invariant. Do not "fix" this to
/// match Insert: keying an `Insert` by injected element names would assert a
/// parent/child relationship an `Insert`'s sibling placement doesn't have.
fn split_injected_elements(op: &PatchOp, target: &DefTarget) -> Vec<String> {
    if !has_append_semantics(&op.class) {
        return Vec::new();
    }
    let prefix = format!("{}/", target.match_key());
    op.injected_paths
        .iter()
        .filter_map(|path| path.strip_prefix(prefix.as_str()))
        .filter(|element| !element.is_empty() && !element.contains('/'))
        .map(str::to_string)
        .collect()
}

/// An op class that "acts on the node itself" — anything but an
/// append-semantics `Add`/`AddModExtension` (which appends new children
/// rather than touching the node's own content, [`split_injected_elements`]'s own
/// concern) and `Insert` (which places its `<value>` as a *sibling* at the
/// xpath location, never touching the target node itself either). A
/// `Replace`/`Remove`/attribute-set op, `SetName`, or any unrecognized custom
/// class all qualify — conservatively: an op this crate can't otherwise
/// reason about is assumed capable of rewriting the whole node, the same
/// "over-triggering costs one confirmation, under-triggering hides a real
/// collision" judgment call `collision_severity`'s own `is_unknown_class`
/// already makes.
///
/// **Casing**: this function mixes two different casing conventions where
/// they meet. Its own `Insert` check lower-cases first (matching
/// `is_unknown_class`/`class_contains_any`), but [`has_append_semantics`] —
/// the other half of this function's own `!has_append_semantics(class) &&
/// ...` — is a case-sensitive `ends_with`. Safe only in the conservative
/// direction: a mis-cased `...patchoperationadd` fails
/// `has_append_semantics`, so this function returns `true` (node-acting) for
/// what is really an `Add` — it blocks a sibling's split rather than the
/// mis-cased `Add` itself being (wrongly) split, over-blocking rather than
/// under-blocking.
fn acts_on_the_node_itself(class: &str) -> bool {
    !has_append_semantics(class) && !class.to_lowercase().ends_with("patchoperationinsert")
}

/// The def+selector triple a [`CollisionKey`]'s first three fields share —
/// [`patch_collisions`]'s own blocking sets are keyed one level coarser
/// than a full `CollisionKey`, since a node-acting op's presence at a
/// given `sub_path` gates splitting for *every* `Add` sharing that
/// `(def_type, def_name, selector)`, not one specific candidate.
type DefSelector = (String, String, Selector);

/// The same normalized `(def_type, def_name, selector, sub_path)` receives
/// mutating patch operations from more than one distinct mod. `selector`
/// is part of the key: a `[@Name="X"]` template and a `[defName="X"]` def
/// are different targets even when `X` matches both. Operations gated off
/// by `MayRequire`/`MayRequireAnyOf` or an unsatisfied
/// `PatchOperationFindMod` context are excluded — they never run.
///
/// An append-semantics `Add`/`AddModExtension` op is keyed by `sub_path`
/// **plus** each top-level element name its own `<value>` injects
/// ([`split_injected_elements`]) — two mods adding *different* children to the same
/// def root don't collide, since their derived keys differ — **but only when
/// no op that [`acts_on_the_node_itself`] already targets that `Add`'s own
/// exact, un-split `(def_type, def_name, selector, sub_path)` key-space**
/// (`root_blocked`/`exact_blocked` below, computed in a first pass over every
/// contribution before any split key is built). When such an op is present,
/// the `Add` is **not** split at all — it keeps its own plain, un-split key.
/// That is the bucket the blocking op itself occupies exactly when the two
/// target the same path (the whole-def "Replace at root vs Add at root" case,
/// where both sit at `sub_path: None` and land in one bucket together); when
/// the blocker sits at an ancestor path instead (`root_blocked` set, `Add` at
/// a deeper `sub_path`), the `Add` simply stays un-split — its own key, not
/// unioned with the blocker's. Either way, a `Replace` at the def root **must
/// not** be unioned into every one of that def's split element buckets one at
/// a time, since that reports one real disagreement as N — it must instead
/// suppress the split outright.
///
/// **Blocking is computed before any split key exists**, scoped exactly to
/// the blocked `Add`'s own key-space — it can never reach a sibling field's
/// unrelated bucket, only the bucket its own un-split target would already
/// occupy. The reverse order — split first, then retroactively *union* a
/// node-acting op's own entries into every existing key found to be its
/// descendant — looks equivalent but isn't: `by_target.keys()` at that point
/// holds *every* key for the def, including genuinely separate per-field
/// collisions on unrelated fields, and a root-level (`sub_path: None`)
/// node-acting op would match every one of them. On a real install that
/// roughly triples contested patch collisions, converting many clean,
/// purely-additive per-field collisions on heavily-patched defs into falsely
/// contested ones.
#[must_use]
pub fn patch_collisions(
    scanned: &[ScannedMod],
    load_order: &LoadOrder,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> Vec<Conflict> {
    // Pass 1: every mutating, active (mod, op, target) contribution, plus
    // the blocking sets a node-acting op populates — computed up front so
    // pass 2 can decide each `Add`'s split independent of iteration order
    // (a blocking op may be scanned before or after the `Add` it blocks).
    let mut contributions: Vec<(&ModId, &PatchOp, DefTarget)> = Vec::new();
    // A node-acting op whose own target has no `sub_path` at all — it
    // targets the def node itself, so it blocks splitting for *every*
    // `Add` on this `(def_type, def_name, selector)`, whatever depth that
    // `Add`'s own target sits at.
    let mut root_blocked: HashSet<DefSelector> = HashSet::new();
    // A node-acting op whose own target's `sub_path` matches an `Add`'s
    // own un-split `sub_path` exactly — it blocks splitting for that one
    // key-space only, never a sibling field's.
    let mut exact_blocked: HashSet<(DefSelector, String)> = HashSet::new();
    // Every active `PatchOperationRemove` target, for `PatchCollision::removed_by`.
    let mut removals: Vec<Removal> = Vec::new();

    for scanned_mod in scanned {
        for op in &scanned_mod.patch_ops {
            if !op.is_mutating || !patch_op_active(op, active, name_map) {
                continue;
            }
            // A head naming several defs (`[defName="A" or defName="B"]`)
            // patches every one of them, so it collides under each — the
            // single `op.target` the scan resolved is only the first.
            for target in super::patch_op_targets(op) {
                if acts_on_the_node_itself(&op.class) {
                    let def_selector = (
                        target.def_type.clone(),
                        target.def_name.clone(),
                        target.selector,
                    );
                    match &target.sub_path {
                        None => {
                            root_blocked.insert(def_selector);
                        }
                        Some(sub_path) => {
                            exact_blocked.insert((def_selector, sub_path.clone()));
                        }
                    }
                }
                if is_remove_class(&op.class) {
                    removals.push(Removal {
                        def_selector: (
                            target.def_type.clone(),
                            target.def_name.clone(),
                            target.selector,
                        ),
                        sub_path: target.sub_path.clone(),
                        remover: scanned_mod.info.id.clone(),
                    });
                }
                contributions.push((&scanned_mod.info.id, op, target));
            }
        }
    }

    let mut by_target: BTreeMap<CollisionKey, Vec<(ModId, String)>> = BTreeMap::new();
    for (mod_id, op, target) in contributions {
        let def_selector = (
            target.def_type.clone(),
            target.def_name.clone(),
            target.selector,
        );
        let blocked = root_blocked.contains(&def_selector)
            || target
                .sub_path
                .as_ref()
                .is_some_and(|s| exact_blocked.contains(&(def_selector, s.clone())));
        let entry = (mod_id.clone(), op.class.clone());
        let elements = if blocked {
            Vec::new()
        } else {
            split_injected_elements(op, &target)
        };
        if elements.is_empty() {
            let key = (
                target.def_type.clone(),
                target.def_name.clone(),
                target.selector,
                target.sub_path.clone(),
            );
            by_target.entry(key).or_default().push(entry);
        } else {
            for element in elements {
                let sub_path = Some(match &target.sub_path {
                    Some(existing) => format!("{existing}/{element}"),
                    None => element,
                });
                let key = (
                    target.def_type.clone(),
                    target.def_name.clone(),
                    target.selector,
                    sub_path,
                );
                by_target.entry(key).or_default().push(entry.clone());
            }
        }
    }

    by_target
        .into_iter()
        .filter(|(_, entries)| {
            entries
                .iter()
                .map(|(id, _)| id)
                .collect::<HashSet<_>>()
                .len()
                > 1
        })
        .map(|((def_type, def_name, selector, sub_path), mut entries)| {
            entries.sort_by_key(|(id, _)| load_order.position(id).unwrap_or(usize::MAX));
            let mods: Vec<PatchCollisionEntry> = entries
                .into_iter()
                .map(|(mod_id, op_class)| PatchCollisionEntry { mod_id, op_class })
                .collect();
            let severity = collision_severity(&mods);
            let removed_by = removed_by(
                &removals,
                &(def_type.clone(), def_name.clone(), selector),
                sub_path.as_deref(),
            );
            Conflict::PatchCollision(PatchCollision {
                def_type,
                def_name,
                selector,
                sub_path,
                mods,
                severity,
                removed_by,
            })
        })
        .collect()
}

/// One active `PatchOperationRemove` target: which mod removes which node.
struct Removal {
    def_selector: DefSelector,
    sub_path: Option<String>,
    remover: ModId,
}

fn is_remove_class(class: &str) -> bool {
    class.to_lowercase().ends_with("patchoperationremove")
}

/// Whether `ancestor` is `path` itself or one of its `/`-separated
/// ancestors; a `None` ancestor is the def root, above every path.
fn covers(ancestor: Option<&str>, path: Option<&str>) -> bool {
    match (ancestor, path) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(ancestor), Some(path)) => {
            path == ancestor
                || path
                    .strip_prefix(ancestor)
                    .is_some_and(|rest| rest.starts_with('/'))
        }
    }
}

/// The mods that remove `sub_path` on `def_selector`, or an ancestor of
/// it (a whole-def removal included), sorted and deduplicated. A removal
/// makes any other mod's operation on that field fail to find its node
/// when it runs first, whichever order is later judged.
fn removed_by(
    removals: &[Removal],
    def_selector: &DefSelector,
    sub_path: Option<&str>,
) -> Vec<ModId> {
    let removers: BTreeSet<&ModId> = removals
        .iter()
        .filter(|removal| {
            removal.def_selector == *def_selector && covers(removal.sub_path.as_deref(), sub_path)
        })
        .map(|removal| &removal.remover)
        .collect();
    removers.into_iter().cloned().collect()
}

/// Op class substrings (matched case-insensitively) whose presence means
/// load order between the contributing mods can change the outcome:
/// replacing, removing, inserting relative to a sibling, renaming, or
/// setting an attribute are all order-sensitive in a way a plain add
/// isn't.
const CONTESTED_CLASS_SUBSTRINGS: [&str; 5] =
    ["replace", "remove", "insert", "setname", "attributeset"];

/// Suffixes (case-insensitive) of every vanilla mutating `PatchOperation*`
/// class RimWorld ships. Matched by suffix, like [`crate::extract::patches`]'s
/// own `NON_MUTATING_SUFFIXES`, since mods sometimes fully qualify `Class`.
const VANILLA_PATCH_OPERATION_SUFFIXES: [&str; 8] = [
    "patchoperationadd",
    "patchoperationinsert",
    "patchoperationremove",
    "patchoperationreplace",
    "patchoperationattributeadd",
    "patchoperationattributeset",
    "patchoperationattributeremove",
    "patchoperationsetname",
];

fn class_contains_any(class: &str, needles: &[&str]) -> bool {
    let lower = class.to_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

/// A class RimWorld doesn't ship and that isn't an `Add`/`AddModExtension`
/// variant either — an unrecognized custom class, which (per the same
/// reasoning as the contested-substring list) could do anything to the
/// target, including something order-sensitive.
fn is_unknown_class(class: &str) -> bool {
    let lower = class.to_lowercase();
    let is_vanilla = VANILLA_PATCH_OPERATION_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    !is_vanilla && !class_contains_any(class, &["add", "addmodextension"])
}

/// Classifies a [`PatchCollision`]'s severity: [`PatchCollisionSeverity::Contested`]
/// when more than one mod contributes to the target *and* at least one
/// contributing operation is order-sensitive (matches a contested class
/// substring, or is an unrecognized custom class) — [`PatchCollisionSeverity::Additive`]
/// otherwise, including whenever every contributing entry is from the
/// same single mod (there's no other mod's ordering to contest against).
pub(super) fn collision_severity(entries: &[PatchCollisionEntry]) -> PatchCollisionSeverity {
    let distinct_mods: HashSet<&ModId> = entries.iter().map(|e| &e.mod_id).collect();
    let contested = distinct_mods.len() > 1
        && entries.iter().any(|e| {
            class_contains_any(&e.op_class, &CONTESTED_CLASS_SUBSTRINGS)
                || is_unknown_class(&e.op_class)
        });
    if contested {
        PatchCollisionSeverity::Contested
    } else {
        PatchCollisionSeverity::Additive
    }
}
