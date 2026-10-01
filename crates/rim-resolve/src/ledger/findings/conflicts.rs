//! Turning the analyzer's conflicts into findings.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{
    Conflict, EdgeKind, EdgeStrength, LoadOrder, ModId, Report, last_loaded,
};

use super::provenance::sorted_pair;
use crate::domain::{DefKey, Finding, FindingKey};

/// `selected_order` is the order the ledger is being built for; a
/// finding whose evidence depends on it (a texture override's winner)
/// reads it from there.
pub(super) fn insert_conflict(
    findings: &mut BTreeMap<FindingKey, Finding>,
    conflict: &Conflict,
    selected_order: &LoadOrder,
) {
    match conflict {
        Conflict::DefOverride(c) => {
            let key = DefKey {
                def_type: c.def_type.clone(),
                def_name: c.def_name.clone(),
            };
            let Some(winner) = last_loaded(&c.owners, selected_order) else {
                return;
            };
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::DefOverride {
                    key: key.clone(),
                    owners,
                },
                Finding::DefOverride {
                    key,
                    owners: c.owners.clone(),
                    winner,
                },
            );
        }
        Conflict::PatchCollision(c) => {
            let key = DefKey {
                def_type: c.def_type.clone(),
                def_name: c.def_name.clone(),
            };
            let mods: Vec<ModId> = c.mods.iter().map(|entry| entry.mod_id.clone()).collect();
            let Some(winner) = last_loaded(&mods, selected_order) else {
                return;
            };
            findings.insert(
                FindingKey::PatchCollision {
                    key: key.clone(),
                    selector: c.selector,
                    sub_path: c.sub_path.clone(),
                    mods: mods.iter().cloned().collect(),
                },
                Finding::PatchCollision {
                    key,
                    selector: c.selector,
                    sub_path: c.sub_path.clone(),
                    mods,
                    winner,
                },
            );
        }
        Conflict::TextureOverride(c) => {
            // A conflict has at least two owners by construction; one with
            // none has no winner to name and no finding to show.
            let Some(winner) = last_loaded(&c.owners, selected_order) else {
                return;
            };
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::TextureOverride {
                    texture_path: c.texture_path.clone(),
                    owners,
                },
                Finding::TextureOverride {
                    texture_path: c.texture_path.clone(),
                    owners: c.owners.clone(),
                    winner,
                },
            );
        }
        Conflict::DuplicateAssembly(c) => {
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::DuplicateAssembly {
                    assembly_name: c.assembly_name.clone(),
                    owners,
                },
                Finding::DuplicateAssembly {
                    assembly_name: c.assembly_name.clone(),
                    owners: c.owners.clone(),
                },
            );
        }
        Conflict::DuplicateTemplateName(c) => {
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::DuplicateTemplateName {
                    name: c.name.clone(),
                    owners,
                },
                Finding::DuplicateTemplateName {
                    name: c.name.clone(),
                    owners: c.owners.clone(),
                },
            );
        }
        Conflict::SoundOverride(c) => {
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::SoundOverride {
                    path: c.path.clone(),
                    owners,
                },
                Finding::SoundOverride {
                    path: c.path.clone(),
                    owners: c.owners.clone(),
                },
            );
        }
        Conflict::LikelyDuplicateMod(c) => {
            let pair = sorted_pair(c.a.clone(), c.b.clone());
            findings.insert(
                FindingKey::LikelyDuplicateMod { pair: pair.clone() },
                Finding::LikelyDuplicateMod {
                    a: pair.0,
                    b: pair.1,
                    shared_defs: c.shared_defs,
                },
            );
        }
        // `KeyedTranslationCollision` is grouped per mod pair instead of
        // per key — a pair can share many keys, and each would otherwise
        // be its own row. Handled by `insert_keyed_translation_collisions`
        // below, which scans *every* conflict of that kind once (called
        // from `extract`) rather than one at a time like every other arm
        // here.
        Conflict::KeyedTranslationCollision(_) => {}
        // One `Conflict` to one `Finding` per target (see
        // `FindingKey::RuntimePatchCollision`'s own doc comment for why
        // not per pair).
        Conflict::RuntimePatchCollision(c) => {
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::RuntimePatchCollision {
                    target_type: c.target_type.clone(),
                    target_method: c.target_method.clone(),
                    owners,
                },
                Finding::RuntimePatchCollision {
                    target_type: c.target_type.clone(),
                    target_method: c.target_method.clone(),
                    owners: c.owners.clone(),
                },
            );
        }
        // One `Conflict` to one `Finding`, same
        // shape as `RuntimePatchCollision` above.
        Conflict::TranspilerCollision(c) => {
            let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
            findings.insert(
                FindingKey::TranspilerCollision {
                    target_type: c.target_type.clone(),
                    target_method: c.target_method.clone(),
                    owners,
                },
                Finding::TranspilerCollision {
                    target_type: c.target_type.clone(),
                    target_method: c.target_method.clone(),
                    owners: c.owners.clone(),
                },
            );
        }
        Conflict::MissingTexturePath(c) => {
            let def = DefKey {
                def_type: c.def_type.clone(),
                def_name: c.def_name.clone(),
            };
            findings.insert(
                FindingKey::MissingTexturePath {
                    referrer: c.referrer.clone(),
                    def: def.clone(),
                    field: c.field.clone(),
                    path: c.path.clone(),
                },
                Finding::MissingTexturePath {
                    referrer: c.referrer.clone(),
                    def,
                    field: c.field.clone(),
                    path: c.path.clone(),
                },
            );
        }
        Conflict::UndecodableTexture(c) => {
            findings.insert(
                FindingKey::UndecodableTexture {
                    mod_id: c.mod_id.clone(),
                    path: c.path.clone(),
                },
                Finding::UndecodableTexture {
                    mod_id: c.mod_id.clone(),
                    path: c.path.clone(),
                    width: c.width,
                    height: c.height,
                    fourcc: c.fourcc.clone(),
                    has_png_sibling: c.has_png_sibling,
                },
            );
        }
        Conflict::BrokenInheritance(c) => {
            findings.insert(
                FindingKey::BrokenInheritance {
                    mod_id: c.mod_id.clone(),
                    parent_name: c.parent_name.clone(),
                    problem_kind: c.problem.kind(),
                },
                Finding::BrokenInheritance {
                    mod_id: c.mod_id.clone(),
                    parent_name: c.parent_name.clone(),
                    child: c.child.clone(),
                    problem: c.problem.clone(),
                    affected: c.affected.clone(),
                    truncated: c.truncated,
                },
            );
        }
        Conflict::NearMissModReference(c) => {
            findings.insert(
                FindingKey::NearMissModReference {
                    referrer: c.referrer.clone(),
                    kind: c.reference_kind,
                    written: c.written.clone(),
                },
                Finding::NearMissModReference {
                    referrer: c.referrer.clone(),
                    kind: c.reference_kind,
                    written: c.written.clone(),
                    candidate: c.candidate.clone(),
                    candidate_name: c.candidate_name.clone(),
                    rule: c.rule,
                    locator: c.locator.clone(),
                },
            );
        }
        Conflict::DiscardedAddition(c) => {
            let def = DefKey {
                def_type: c.def_type.clone(),
                def_name: c.def_name.clone(),
            };
            findings.insert(
                FindingKey::DiscardedAddition {
                    replacer: c.replacer.clone(),
                    adder: c.adder.clone(),
                    def: Box::new(def.clone()),
                    path: c.path.clone(),
                },
                Finding::DiscardedAddition {
                    replacer: c.replacer.clone(),
                    adder: c.adder.clone(),
                    def,
                    path: c.path.clone(),
                    adder_path: c.adder_path.clone(),
                },
            );
        }
        Conflict::DanglingDefReference(c) => {
            findings.insert(
                FindingKey::DanglingDefReference {
                    name: c.name.clone(),
                },
                Finding::DanglingDefReference {
                    name: c.name.clone(),
                    referrers: c.referrers.clone(),
                    truncated_referrers: c.truncated_referrers,
                    cause: c.cause.clone(),
                    likely_sound: c.likely_sound,
                },
            );
        }
    }
}

/// Groups every [`Conflict::KeyedTranslationCollision`] by mod pair.
/// See [`FindingKey::KeyedTranslationCollision`]'s doc comment for why a
/// pair sharing several keys collapses into one row instead of several.
pub(super) fn insert_keyed_translation_collisions(
    findings: &mut BTreeMap<FindingKey, Finding>,
    conflicts: &[Conflict],
) {
    let mut by_pair: BTreeMap<(ModId, ModId), BTreeSet<String>> = BTreeMap::new();
    for conflict in conflicts {
        let Conflict::KeyedTranslationCollision(c) = conflict else {
            continue;
        };
        for (index, first) in c.owners.iter().enumerate() {
            for second in &c.owners[index + 1..] {
                let pair = sorted_pair(first.clone(), second.clone());
                by_pair.entry(pair).or_default().insert(c.key.clone());
            }
        }
    }
    for (pair, keys) in by_pair {
        findings.insert(
            FindingKey::KeyedTranslationCollision { pair: pair.clone() },
            Finding::KeyedTranslationCollision {
                a: pair.0,
                b: pair.1,
                keys: keys.into_iter().collect(),
            },
        );
    }
}

/// A def in one mod names a type from another mod's DLL
/// (`EdgeKind::UsesType`) with no already-declared relation onto the
/// providing mod (no `Hard`/`Declared`-strength edge in the same
/// direction between the same two mods). The type name comes straight off
/// `Edge::subject` (`Report` schema 4), the structured field, never a
/// parse of `Edge::detail`'s format string
/// (`rim_analyzer::analysis::edges::uses_type_edges`'s own wording), which
/// would tie this to that exact string.
pub(super) fn insert_undeclared_type_dependencies(
    findings: &mut BTreeMap<FindingKey, Finding>,
    report: &Report,
) {
    let declared_or_hard: BTreeSet<(ModId, ModId)> = report
        .edges
        .iter()
        .map(|entry| &entry.edge)
        .filter(|edge| matches!(edge.strength(), EdgeStrength::Hard | EdgeStrength::Declared))
        .map(|edge| (edge.after.clone(), edge.before.clone()))
        .collect();

    for entry in &report.edges {
        let edge = &entry.edge;
        if edge.kind != EdgeKind::UsesType {
            continue;
        }
        let Some(type_name) = edge.subject.as_deref() else {
            continue;
        };
        if declared_or_hard.contains(&(edge.after.clone(), edge.before.clone())) {
            continue;
        }
        findings.insert(
            FindingKey::UndeclaredTypeDependency {
                user: edge.after.clone(),
                provider: edge.before.clone(),
                type_name: type_name.to_string(),
            },
            Finding::UndeclaredTypeDependency {
                user: edge.after.clone(),
                provider: edge.before.clone(),
                type_name: type_name.to_string(),
            },
        );
    }
}
