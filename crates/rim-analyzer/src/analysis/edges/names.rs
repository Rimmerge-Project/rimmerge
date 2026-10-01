//! Display-name resolution and the `FindMod`/`IfModActive` awareness edges.

use std::collections::HashSet;

use crate::domain::{Edge, EdgeKind, ModId, ScannedMod, UnresolvedFindMod, Warning};

use crate::analysis::indices::{ActiveMods, DisplayNameIndex};

/// Maps every active mod's display name (lowercased) to its id, for
/// resolving `PatchOperationFindMod` mod names. Two active mods sharing a
/// display name is a genuine ambiguity — the first one seen (scan order)
/// wins the slot and the rest are reported as [`Warning`]s rather than
/// silently overwritten.
///
/// **Keep in sync with
/// `rim_session::use_cases::verify_order::build_gate_name_map`** — that
/// function reimplements this same "lowercased name -> id, first wins"
/// resolution over `rim_analyzer::domain::Mod` instead of `ScannedMod` (the
/// session layer only ever has the already-built `Mod` list, never a live
/// scan's `ScannedMod`s — shared by `VerifyOrder`, `ContributesNothing`, and
/// `ImportGameLog`, all three of which need it), so a change to the
/// resolution rule here (e.g. the tie-break, or what counts as "empty")
/// silently drifts out of step with that copy unless both are updated
/// together.
#[must_use]
pub fn build_name_map(scanned: &[ScannedMod]) -> (DisplayNameIndex, Vec<Warning>) {
    let mut map = DisplayNameIndex::new();
    let mut warnings = Vec::new();

    for scanned_mod in scanned {
        let name = &scanned_mod.info.name;
        if name.is_empty() {
            continue;
        }
        match map.get(name) {
            Some(existing) if *existing != scanned_mod.info.id => {
                warnings.push(Warning::new(Some(scanned_mod.info.id.clone()),
                    format!("duplicate display name '{name}': also used by {existing} (FindMod resolution will use {existing})"
                    )));
            }
            Some(_) => {}
            None => {
                map.insert(name, scanned_mod.info.id.clone());
            }
        }
    }
    (map, warnings)
}

/// The result of resolving every `PatchOperationFindMod` name across
/// every scanned mod.
#[derive(Debug, Default)]
pub struct FindModResolution {
    pub edges: Vec<Edge>,
    /// Names that resolved to no active mod's display name at all.
    pub unresolved: Vec<UnresolvedFindMod>,
    /// Names that resolved to no active mod's *display name*, but
    /// happen to equal an active mod's packageId — RimWorld's `FindMod`
    /// matches on display name only, so these stay unresolved as an
    /// edge, but are worth surfacing separately as a likely authoring
    /// mistake.
    pub using_package_id: Vec<UnresolvedFindMod>,
}

/// `PatchOperationFindMod` names resolved against active mod display
/// names; names that don't resolve to any active mod are returned
/// separately rather than silently dropped.
///
/// Reads each `PatchOperationFindMod` op's own [`PatchOp::find_mod_names`]
/// (not the `find_mod_context` gates it hands its `<match>`/`<nomatch>`
/// descendants) — so a `FindMod` with only a `<nomatch>` branch, or no
/// branch at all, still yields the awareness edges its named mods imply.
#[must_use]
pub fn find_mod_edges(
    scanned: &[ScannedMod],
    name_map: &DisplayNameIndex,
    active: &ActiveMods,
) -> FindModResolution {
    let mut result = FindModResolution::default();

    for scanned_mod in scanned {
        let mut seen_names = HashSet::new();
        let mut seen_targets = HashSet::new();
        // Kept as `(name, op)` pairs, not flattened away, so each name
        // still has its own op's locator to attribute
        // `UnresolvedFindMod`/`using_package_id` back to a file and line —
        // a `FindMod` op usually names one mod, but the op-level locator
        // is the closest available site even for one that lists several.
        let names = scanned_mod
            .patch_ops
            .iter()
            .flat_map(|op| op.find_mod_names.iter().map(move |name| (name, op)));
        for (name, op) in names {
            if !seen_names.insert(name.clone()) {
                continue;
            }
            match name_map.get(name) {
                Some(target) if *target == scanned_mod.info.id => {}
                Some(target) => {
                    if seen_targets.insert(target.clone()) {
                        result.edges.push(Edge {
                            after: scanned_mod.info.id.clone(),
                            before: target.clone(),
                            kind: EdgeKind::FindMod,
                            detail: format!("patch conditioned on mod '{name}' being present"),
                            load_time: true,
                            subject: None,
                        });
                    }
                }
                None if active.contains(&ModId::new(name)) => {
                    result.using_package_id.push(UnresolvedFindMod {
                        mod_id: scanned_mod.info.id.clone(),
                        display_name: name.clone(),
                        locator: Some(op.locator.clone()),
                    });
                }
                None => result.unresolved.push(UnresolvedFindMod {
                    mod_id: scanned_mod.info.id.clone(),
                    display_name: name.clone(),
                    locator: Some(op.locator.clone()),
                }),
            }
        }
    }
    result
}

/// `LoadFolders.xml` `IfModActive`/`IfModActiveAll` gates: the gated mod is
/// aware of, and assumed to load after, the mod(s) it's gated on. Both
/// attributes feed `ScannedMod::if_mod_active_targets` identically — an
/// all-of gate is exactly as much this "aware of, assumed to load after"
/// relation as an any-of one. `IfModNotActive` stays uncollected: "must be
/// absent" is a different relation, out of scope here.
#[must_use]
pub fn if_mod_active_edges(scanned: &[ScannedMod], active: &ActiveMods) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let mut seen = HashSet::new();
        for target in &scanned_mod.if_mod_active_targets {
            let Some(resolved) = active.resolve(target) else {
                continue;
            };
            if *resolved == scanned_mod.info.id || !seen.insert(resolved.clone()) {
                continue;
            }
            edges.push(Edge {
                after: scanned_mod.info.id.clone(),
                before: resolved.clone(),
                kind: EdgeKind::IfModActive,
                detail: format!("LoadFolders.xml gates a folder on '{target}' being active"),
                load_time: true,
                subject: None,
            });
        }
    }
    edges
}
