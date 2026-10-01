//! Author-declared ordering (`loadAfter`/`loadBefore`/`forceLoad*`) and `mayRequire` edges.

use std::collections::HashSet;

use crate::domain::{Edge, EdgeKind, ModId, ScannedMod};

use crate::analysis::indices::ActiveMods;

/// `loadAfter`/`loadBefore`/`forceLoadAfter`/`forceLoadBefore`/`modDependencies`,
/// resolved only when the target is itself active. The target `ModId` in
/// `About.xml` never carries a `_steam` suffix, so it's resolved through
/// `active` to the exact id the current [`LoadOrder`](crate::domain::LoadOrder)
/// (and the target's own [`Mod::id`](crate::domain::Mod::id)) actually uses.
#[must_use]
pub fn declared_edges(scanned: &[ScannedMod], active: &ActiveMods) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let id = &scanned_mod.info.id;
        let declared = &scanned_mod.info.declared;

        // `id` must load after each of these targets.
        let after_targets = declared
            .load_after
            .iter()
            .map(|t| (t, EdgeKind::LoadAfter, "loadAfter"))
            .chain(
                declared
                    .force_load_after
                    .iter()
                    .map(|t| (t, EdgeKind::ForceLoadAfter, "forceLoadAfter")),
            )
            .chain(
                declared
                    .dependencies
                    .iter()
                    .map(|d| (&d.id, EdgeKind::ModDependency, "a dependency on")),
            );
        for (target, kind, verb) in after_targets {
            if let Some(resolved) = active.resolve(target) {
                edges.push(Edge {
                    after: id.clone(),
                    before: resolved.clone(),
                    kind,
                    detail: format!("{id} declares {verb} {target}"),
                    load_time: true,
                    subject: None,
                });
            }
        }

        // `id` must load before each of these targets.
        let before_targets = declared
            .load_before
            .iter()
            .map(|t| (t, EdgeKind::LoadBefore, "loadBefore"))
            .chain(
                declared
                    .force_load_before
                    .iter()
                    .map(|t| (t, EdgeKind::ForceLoadBefore, "forceLoadBefore")),
            );
        for (target, kind, verb) in before_targets {
            if let Some(resolved) = active.resolve(target) {
                edges.push(Edge {
                    after: resolved.clone(),
                    before: id.clone(),
                    kind,
                    detail: format!("{id} declares {verb} {target}"),
                    load_time: true,
                    subject: None,
                });
            }
        }
    }
    edges
}

/// `MayRequire`/`MayRequireAnyOf` on a def or patch operation, naming
/// another active mod — evidence the mod is aware of it, even though the
/// condition doesn't imply an ordering promise.
#[must_use]
pub fn may_require_edges(scanned: &[ScannedMod], active: &ActiveMods) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let mut seen = HashSet::new();
        let names = scanned_mod
            .defs
            .iter()
            .flat_map(|d| d.may_require.iter().chain(d.may_require_any_of.iter()))
            .chain(
                scanned_mod
                    .patch_ops
                    .iter()
                    .flat_map(|op| op.may_require.iter().chain(op.may_require_any_of.iter())),
            );
        for name in names {
            let target = ModId::new(name);
            let Some(resolved) = active.resolve(&target) else {
                continue;
            };
            if *resolved == scanned_mod.info.id || !seen.insert(resolved.clone()) {
                continue;
            }
            edges.push(Edge {
                after: scanned_mod.info.id.clone(),
                before: resolved.clone(),
                kind: EdgeKind::MayRequire,
                detail: format!("gated on '{resolved}' via MayRequire/MayRequireAnyOf"),
                load_time: true,
                subject: None,
            });
        }
    }
    edges
}
