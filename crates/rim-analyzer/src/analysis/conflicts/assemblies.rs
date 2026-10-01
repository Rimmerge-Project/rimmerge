//! Duplicate assemblies and colliding runtime patches and transpilers.

use crate::domain::{
    Conflict, DuplicateAssembly, LoadOrder, ModId, RuntimePatchCollision, TranspilerCollision,
};

use super::sorted_by_load_order;
use crate::analysis::indices::Indices;

/// The same assembly name shipped by more than one active mod (including
/// an inferred shared-library name, since the first-loaded copy still
/// wins at runtime regardless of whether ordering edges are inferred for
/// it).
#[must_use]
pub fn duplicate_assemblies(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    let vanilla_source = |id: &ModId| indices.mod_source.get(id).is_some_and(|s| s.is_vanilla());
    indices
        .assembly_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        // A vanilla assembly plus a mod's own copy of it isn't a
        // duplicate-shipping conflict between two *mods*.
        .filter(|(_, owners)| owners.iter().filter(|id| !vanilla_source(id)).count() > 1)
        .map(|(name, owners)| {
            let sorted_owners = sorted_by_load_order(owners, load_order);
            // Invariant: filtered to owners.len() > 1 above, so at least
            // one owner exists.
            #[allow(clippy::expect_used)]
            let first_loaded = sorted_owners
                .first()
                .cloned()
                .expect("owners is non-empty: filtered above");
            let versions = sorted_owners
                .iter()
                .map(|id| {
                    let version = indices
                        .assembly_versions
                        .get(&(id.clone(), name.clone()))
                        .copied()
                        .unwrap_or_default();
                    (id.clone(), version)
                })
                .collect();
            Conflict::DuplicateAssembly(DuplicateAssembly {
                assembly_name: name.clone(),
                owners: sorted_owners,
                versions,
                first_loaded,
            })
        })
        .collect()
}

/// Two or more active mods declare a runtime patch targeting the same type
/// and method — at equal runtime-patch priority, load order decides which patch
/// runs last. Owners are collapsed by *originating assembly name* before the
/// `> 1` test: two mods each bundling their own copy of the identically-named
/// assembly aren't independently-authored patches contesting the same target,
/// just one patch shipped twice, so they don't count as a collision on their
/// own — only a genuinely distinct second assembly (whether bundled by one of
/// those same mods or a third one entirely) makes this a real collision.
#[must_use]
pub fn runtime_patch_collisions(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .runtime_patch_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .filter(|(target, _)| {
            indices
                .runtime_patch_assembly_names
                .get(*target)
                .is_some_and(|names| names.len() > 1)
        })
        .map(|((target_type, target_method), owners)| {
            Conflict::RuntimePatchCollision(RuntimePatchCollision {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}

/// Two or more active mods each declare a `Transpiler` role targeting the
/// same `(type, method)` — the
/// narrow, order-sensitive subset of [`runtime_patch_collisions`]: a
/// `Prefix`/`Postfix` composes safely regardless of load order, but a
/// `Transpiler` rewrites the IL the *next* transpiler must still
/// pattern-match, so which one runs last can decide whether a later
/// patch's own match still succeeds. Disclosure only — this crate never
/// derives an ordering edge from it (see [`crate::domain::TranspilerCollision`]'s
/// own doc comment for why). The same bundled-copy collapse
/// `runtime_patch_collisions` uses applies here, against the transpiler-
/// scoped assembly-name index.
#[must_use]
pub fn transpiler_collisions(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .transpiler_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .filter(|(target, _)| {
            indices
                .transpiler_assembly_names
                .get(*target)
                .is_some_and(|names| names.len() > 1)
        })
        .map(|((target_type, target_method), owners)| {
            Conflict::TranspilerCollision(TranspilerCollision {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}
