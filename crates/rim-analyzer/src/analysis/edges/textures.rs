//! Retexture-after-owner edges.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{Edge, EdgeKind, ModId, ScannedMod};

use crate::analysis::indices::Indices;

/// A texture-only active mod (no `defs`, `templates`, mutating `patch_ops`,
/// or `assemblies` of its own) ships the same texture path as a
/// content-shipping active mod — the texture-only mod must load after every
/// content owner it overrides, so its replacement texture is the one that
/// actually wins (RimWorld loads textures in mod order; the last-loaded copy
/// of a path wins). `EdgeStrength::Inferred` — a heuristic, not an author
/// declaration.
///
/// A path with **exactly one** texture-only owner and at least one content
/// owner emits one edge per (texture-only, content) pair; a path with two
/// content owners, or two-or-more texture-only owners, emits nothing here —
/// the ledger's existing `TextureOverride` conflict still contests those,
/// since this producer can't tell which one should win. Duplicate per-pair
/// edges (the same two mods sharing many texture paths) collapse into one,
/// the count recorded in `detail` — unlike most of this module's producers,
/// which let `push_edge_once` silently drop every occurrence after the first,
/// this is counted explicitly so the retexture numbers are legible from the
/// report alone. `Edge.subject` is the alphabetically-first shared path
/// (`indices.texture_owners` is already a `BTreeMap`, so this is
/// deterministic) even when `detail` names a count of several.
#[must_use]
pub fn retexture_after_owner_edges(scanned: &[ScannedMod], indices: &Indices) -> Vec<Edge> {
    let texture_only: BTreeSet<ModId> = scanned
        .iter()
        .filter(|m| {
            m.defs.is_empty()
                && m.templates.is_empty()
                && m.assemblies.is_empty()
                && !m.patch_ops.iter().any(|op| op.is_mutating)
        })
        .map(|m| m.info.id.clone())
        .collect();

    let mut pairs: BTreeMap<(ModId, ModId), (usize, String)> = BTreeMap::new();
    for (path, owners) in &indices.texture_owners {
        if owners.len() < 2 {
            continue;
        }
        let (texture_only_owners, content_owners): (Vec<&ModId>, Vec<&ModId>) =
            owners.iter().partition(|id| texture_only.contains(*id));
        let [sole] = texture_only_owners.as_slice() else {
            continue;
        };
        for content in content_owners {
            let entry = pairs
                .entry(((*sole).clone(), content.clone()))
                .or_insert_with(|| (0, path.clone()));
            entry.0 += 1;
        }
    }

    let mut edges = Vec::new();
    for ((after, before), (count, first_path)) in pairs {
        let detail = if count == 1 {
            format!("retextures '{first_path}', shipped by {before}")
        } else {
            format!("retextures {count} texture paths shipped by {before} (e.g. '{first_path}')")
        };
        edges.push(Edge {
            after,
            before,
            kind: EdgeKind::RetextureAfterOwner,
            detail,
            load_time: true,
            subject: Some(first_path),
        });
    }
    edges
}
