//! Building one mod's rows: defs it owns, templates it registers, foreign defs it patches, and
//! assets.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{Conflict, ModId, Report, Selector};
use rim_resolve::domain::{DefKey, DefRef, FindingKey, GeneratedMods};

use super::conflicts::{ConflictIndex, contested_finding_keys, owner_set_hidden, pair_hidden};
use super::rows::{AssetKind, ChangeKind, ChangeRow};

/// `ids`, deduped to base ids and stripped of `target` — the "how many
/// *other* mods" building block every [`ChangeRow::other_touchers`]
/// count is a union of.
fn other_base_ids<'a>(ids: impl IntoIterator<Item = &'a ModId>, target: &ModId) -> BTreeSet<ModId> {
    ids.into_iter()
        .map(ModId::base)
        .filter(|id| id != target)
        .collect()
}

fn distinct_patchers(sources: &SourceIndex, key: &(String, String, Selector)) -> BTreeSet<ModId> {
    sources
        .patch_ops_by_def
        .get(key)
        .map(|ops| ops.iter().map(|op| op.mod_id.clone()).collect())
        .unwrap_or_default()
}

fn op_count_of(sources: &SourceIndex, mod_id: &ModId, key: &(String, String, Selector)) -> usize {
    sources
        .ops_by_mod
        .get(mod_id)
        .and_then(|ops| ops.get(key))
        .copied()
        .unwrap_or(0)
}

/// The raw `ModsConfig.xml` id(s) active for `base` — almost always
/// exactly one, but [`ModId::base`]'s own doc comment describes a local
/// copy and a `_steam`-suffixed workshop copy of the same `packageId`
/// both being present, so more than one raw id can share a base. Every
/// `SourceIndex` map this module reads (`defs`, `ops_by_mod`) is keyed by
/// the raw scan-time id, never the base one, so a caller resolving `base`
/// down to "the" mod needs every one of these to find its own
/// contributions — a query keyed on the bare base id would silently miss
/// a Steam-installed mod's own rows. Falls back to `[base]` when `report.mods` doesn't name
/// it at all (a synthetic/test report, or a base id that already has no
/// suffix and needs no resolution).
pub(super) fn active_raw_ids(report: &Report, base: &ModId) -> Vec<ModId> {
    let raw: Vec<ModId> = report
        .mods
        .iter()
        .map(|m| &m.id)
        .filter(|id| id.base() == *base)
        .cloned()
        .collect();
    if raw.is_empty() {
        vec![base.clone()]
    } else {
        raw
    }
}

/// Whether `target` owns `(def_type, def_name)` under `selector` — a
/// concrete def ([`sources.defs`]) or a registered template
/// ([`sources.templates`]). A [`ChangeKind::PatchesDef`] row is only
/// built for a target this mod does *not* own this way — an owner's own
/// patch on its own def/template folds into the
/// [`ChangeKind::OwnsDef`]/[`ChangeKind::OwnsTemplate`] row instead (its
/// op count shown there). `raw_ids` (every raw id sharing `target`'s
/// base, see [`active_raw_ids`]) is needed for the `DefName` arm since
/// [`SourceIndex::defs`] is keyed by the raw id, not the base one; the
/// `NameAttr` arm already compares owners through [`ModId::base`], so it
/// needs `target` alone.
fn owns(
    sources: &SourceIndex,
    target: &ModId,
    raw_ids: &[ModId],
    def_type: &str,
    name: &str,
    selector: Selector,
) -> bool {
    match selector {
        Selector::DefName => raw_ids.iter().any(|raw| {
            sources
                .defs
                .contains_key(&(raw.clone(), (def_type.to_string(), name.to_string())))
        }),
        Selector::NameAttr => sources
            .templates
            .get(&(def_type.to_string(), name.to_string()))
            .is_some_and(|owners| owners.iter().any(|(owner, _)| owner.base() == *target)),
    }
}

pub(super) fn owns_def_rows(
    sources: &SourceIndex,
    conflicts: &ConflictIndex<'_>,
    generated: &GeneratedMods,
    target: &ModId,
    raw_ids: &[ModId],
) -> Vec<ChangeRow> {
    // A def can only ever appear under the raw id that actually owns it,
    // but more than one raw id can share `target`'s base
    // (`active_raw_ids`), so every one of them needs its own range scan
    // over `sources.defs` (keyed by the raw id) — a `BTreeSet` dedups the
    // (vanishingly unlikely, but not impossible) case of the same
    // `(def_type, def_name)` appearing under two raw variants at once.
    let mut owned: BTreeSet<(String, String)> = BTreeSet::new();
    for raw in raw_ids {
        let start = (raw.clone(), (String::new(), String::new()));
        owned.extend(
            sources
                .defs
                .range((std::ops::Bound::Included(start), std::ops::Bound::Unbounded))
                .take_while(|((owner, _), _)| owner == raw)
                .map(|((_, (def_type, def_name)), _)| (def_type.clone(), def_name.clone())),
        );
    }
    owned
        .into_iter()
        .map(|(def_type, def_name)| {
            let key = (def_type.clone(), def_name.clone(), Selector::DefName);
            let owners = other_base_ids(
                sources
                    .owners_by_def
                    .get(&(def_type.clone(), def_name.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                target,
            );
            let patchers = other_base_ids(distinct_patchers(sources, &key).iter(), target);
            let mut touchers = owners;
            touchers.extend(patchers);
            let op_count: usize = raw_ids
                .iter()
                .map(|raw| op_count_of(sources, raw, &key))
                .sum();
            ChangeRow {
                kind: ChangeKind::OwnsDef,
                def_ref: Some(DefRef::new(
                    DefKey {
                        def_type: def_type.clone(),
                        def_name: def_name.clone(),
                    },
                    Selector::DefName,
                )),
                asset_path: None,
                other_touchers: touchers.len(),
                op_count,
                finding_keys: contested_finding_keys(
                    conflicts,
                    generated,
                    &def_type,
                    &def_name,
                    Selector::DefName,
                    target,
                ),
            }
        })
        .collect()
}

pub(super) fn owns_template_rows(
    sources: &SourceIndex,
    conflicts: &ConflictIndex<'_>,
    generated: &GeneratedMods,
    target: &ModId,
    raw_ids: &[ModId],
) -> Vec<ChangeRow> {
    sources
        .templates
        .iter()
        .filter(|(_, owners)| owners.iter().any(|(owner, _)| owner.base() == *target))
        .map(|((def_type, name), owners)| {
            let key = (def_type.clone(), name.clone(), Selector::NameAttr);
            let other_owners = other_base_ids(owners.iter().map(|(owner, _)| owner), target);
            let other_patchers = other_base_ids(distinct_patchers(sources, &key).iter(), target);
            let other_children = other_base_ids(
                sources
                    .children_by_template
                    .get(&(def_type.clone(), name.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .map(|(child, _)| child),
                target,
            );
            let mut touchers = other_owners;
            touchers.extend(other_patchers);
            touchers.extend(other_children);
            let op_count: usize = raw_ids
                .iter()
                .map(|raw| op_count_of(sources, raw, &key))
                .sum();
            ChangeRow {
                kind: ChangeKind::OwnsTemplate,
                def_ref: Some(DefRef::new(
                    DefKey {
                        def_type: def_type.clone(),
                        def_name: name.clone(),
                    },
                    Selector::NameAttr,
                )),
                asset_path: None,
                other_touchers: touchers.len(),
                op_count,
                finding_keys: contested_finding_keys(
                    conflicts,
                    generated,
                    def_type,
                    name,
                    Selector::NameAttr,
                    target,
                ),
            }
        })
        .collect()
}

pub(super) fn patches_def_rows(
    sources: &SourceIndex,
    conflicts: &ConflictIndex<'_>,
    generated: &GeneratedMods,
    target: &ModId,
    raw_ids: &[ModId],
) -> Vec<ChangeRow> {
    // Merge every raw id's own op counts before filtering: a mod's own
    // patch ops on a foreign target could in principle be indexed under
    // any raw variant sharing its base (`active_raw_ids`), same reasoning
    // as `owns_def_rows`'s own multi-raw-id scan.
    let mut merged: BTreeMap<(String, String, Selector), usize> = BTreeMap::new();
    for raw in raw_ids {
        if let Some(ops) = sources.ops_by_mod.get(raw) {
            for (key, &count) in ops {
                *merged.entry(key.clone()).or_default() += count;
            }
        }
    }
    merged
        .into_iter()
        .filter(|((def_type, name, selector), _)| {
            !owns(sources, target, raw_ids, def_type, name, *selector)
        })
        .map(|((def_type, name, selector), count)| {
            let owners_all: Vec<ModId> = match selector {
                Selector::DefName => sources
                    .owners_by_def
                    .get(&(def_type.clone(), name.clone()))
                    .cloned()
                    .unwrap_or_default(),
                Selector::NameAttr => sources
                    .templates
                    .get(&(def_type.clone(), name.clone()))
                    .map(|owners| owners.iter().map(|(owner, _)| owner.clone()).collect())
                    .unwrap_or_default(),
            };
            let key = (def_type.clone(), name.clone(), selector);
            let mut touchers = other_base_ids(owners_all.iter(), target);
            touchers.extend(other_base_ids(
                distinct_patchers(sources, &key).iter(),
                target,
            ));
            ChangeRow {
                kind: ChangeKind::PatchesDef,
                def_ref: Some(DefRef::new(
                    DefKey {
                        def_type: def_type.clone(),
                        def_name: name.clone(),
                    },
                    selector,
                )),
                asset_path: None,
                other_touchers: touchers.len(),
                op_count: count,
                finding_keys: contested_finding_keys(
                    conflicts, generated, &def_type, &name, selector, target,
                ),
            }
        })
        .collect()
}

/// The two mods sorted into a fixed order — the same convention
/// `FindingKey::KeyedTranslationCollision::pair` uses.
fn sorted_pair(a: ModId, b: ModId) -> (ModId, ModId) {
    if a <= b { (a, b) } else { (b, a) }
}

pub(super) fn asset_rows(
    report: &Report,
    generated: &GeneratedMods,
    target: &ModId,
) -> Vec<ChangeRow> {
    let mut rows = Vec::new();
    for conflict in &report.conflicts {
        match conflict {
            Conflict::TextureOverride(c) if c.owners.iter().any(|o| o.base() == *target) => {
                let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
                let finding_keys = if owner_set_hidden(generated, &owners) {
                    Vec::new()
                } else {
                    vec![FindingKey::TextureOverride {
                        texture_path: c.texture_path.clone(),
                        owners,
                    }]
                };
                rows.push(ChangeRow {
                    kind: ChangeKind::OverridesAsset(AssetKind::Texture),
                    def_ref: None,
                    asset_path: Some(c.texture_path.clone()),
                    other_touchers: other_base_ids(c.owners.iter(), target).len(),
                    op_count: 0,
                    finding_keys,
                });
            }
            // A generated mod ships no `Sounds/` of its own
            // (`rim_resolve::ledger::findings::hidden_by_generated`'s own
            // doc comment), so — unlike `TextureOverride` — this arm never
            // needs an [`owner_set_hidden`] check.
            Conflict::SoundOverride(c) if c.owners.iter().any(|o| o.base() == *target) => {
                rows.push(ChangeRow {
                    kind: ChangeKind::OverridesAsset(AssetKind::Sound),
                    def_ref: None,
                    asset_path: Some(c.path.clone()),
                    other_touchers: other_base_ids(c.owners.iter(), target).len(),
                    op_count: 0,
                    finding_keys: vec![FindingKey::SoundOverride {
                        path: c.path.clone(),
                        owners: c.owners.iter().cloned().collect(),
                    }],
                });
            }
            // `FindingKey::KeyedTranslationCollision` groups every
            // colliding key by mod *pair*
            // (`rim_resolve::ledger::findings::insert_keyed_translation_collisions`),
            // so a key with more than two owners backs several different
            // pair-shaped findings at once — one per other owner naming
            // `target`, not the single arbitrarily-chosen pair an
            // `Option<FindingKey>` shape could represent.
            Conflict::KeyedTranslationCollision(c)
                if c.owners.iter().any(|o| o.base() == *target) =>
            {
                let finding_keys: Vec<FindingKey> = c
                    .owners
                    .iter()
                    .filter(|owner| owner.base() == *target)
                    .flat_map(|target_raw| {
                        c.owners
                            .iter()
                            .filter(move |other| other.base() != *target)
                            .filter(move |other| !pair_hidden(generated, target_raw, other))
                            .map(move |other| FindingKey::KeyedTranslationCollision {
                                pair: sorted_pair(target_raw.clone(), other.clone()),
                            })
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                rows.push(ChangeRow {
                    kind: ChangeKind::OverridesAsset(AssetKind::KeyedTranslation),
                    def_ref: None,
                    asset_path: Some(c.key.clone()),
                    other_touchers: other_base_ids(c.owners.iter(), target).len(),
                    op_count: 0,
                    finding_keys,
                });
            }
            _ => {}
        }
    }
    rows
}

pub(super) fn search_text(row: &ChangeRow) -> String {
    match (&row.def_ref, &row.asset_path) {
        (Some(def_ref), _) => def_ref.to_string(),
        (None, Some(path)) => path.clone(),
        (None, None) => String::new(),
    }
}
