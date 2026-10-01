//! Indexing conflicts by what they touch, and the finding keys a change row cites.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{
    Conflict, DuplicateTemplateName, ModId, PatchCollision, Report, Selector,
};
use rim_resolve::domain::{DefKey, FindingKey, GeneratedMods};

/// Every [`Conflict`] this module cares about, indexed once per
/// [`query`](crate::changes::query) call so every row's [`ChangeRow::finding_keys`](crate::changes::rows::ChangeRow::finding_keys) is an
/// O(log n) lookup instead of a fresh scan of [`Report::conflicts`] per
/// row.
pub(super) struct ConflictIndex<'a> {
    def_overrides: BTreeMap<(&'a str, &'a str), &'a rim_analyzer::domain::DefOverride>,
    patch_collisions: BTreeMap<(&'a str, &'a str, Selector), Vec<&'a PatchCollision>>,
    duplicate_templates: BTreeMap<&'a str, &'a DuplicateTemplateName>,
}

pub(super) fn index_conflicts(report: &Report) -> ConflictIndex<'_> {
    let mut def_overrides = BTreeMap::new();
    let mut patch_collisions: BTreeMap<(&str, &str, Selector), Vec<&PatchCollision>> =
        BTreeMap::new();
    let mut duplicate_templates = BTreeMap::new();
    for conflict in &report.conflicts {
        match conflict {
            Conflict::DefOverride(c) => {
                def_overrides.insert((c.def_type.as_str(), c.def_name.as_str()), c);
            }
            Conflict::PatchCollision(c) => {
                patch_collisions
                    .entry((c.def_type.as_str(), c.def_name.as_str(), c.selector))
                    .or_default()
                    .push(c);
            }
            Conflict::DuplicateTemplateName(c) => {
                duplicate_templates.insert(c.name.as_str(), c);
            }
            _ => {}
        }
    }
    ConflictIndex {
        def_overrides,
        patch_collisions,
        duplicate_templates,
    }
}

/// Mirrors `rim_resolve::ledger::findings::owner_set_hidden`, which is
/// private to that crate: hidden the moment any one of `owners` is a
/// generated mod whose own declared scope covers the rest of the set —
/// see [`GeneratedMods::hides`]. Re-derived here on the same public
/// primitive rather than exposing the original, narrower than it sounds:
/// this crate only ever needs it for the handful of `FindingKey` shapes
/// [`contested_finding_keys`]/[`asset_rows`](crate::changes::inventory::asset_rows) build.
pub(super) fn owner_set_hidden(generated: &GeneratedMods, owners: &BTreeSet<ModId>) -> bool {
    owners.iter().any(|owner| {
        generated.contains(owner) && generated.hides(owner, owners.iter().filter(|o| *o != owner))
    })
}

/// [`owner_set_hidden`]'s pair-shaped sibling, mirroring
/// `rim_resolve::ledger::findings::pair_hidden`.
pub(super) fn pair_hidden(generated: &GeneratedMods, a: &ModId, b: &ModId) -> bool {
    (generated.contains(a) && generated.hides(a, std::iter::once(b)))
        || (generated.contains(b) && generated.hides(b, std::iter::once(a)))
}

fn def_override_finding_key(c: &rim_analyzer::domain::DefOverride) -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: c.def_type.clone(),
            def_name: c.def_name.clone(),
        },
        owners: c.owners.iter().cloned().collect(),
    }
}

fn patch_collision_finding_key(c: &PatchCollision) -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: c.def_type.clone(),
            def_name: c.def_name.clone(),
        },
        selector: c.selector,
        sub_path: c.sub_path.clone(),
        mods: c.mods.iter().map(|entry| entry.mod_id.clone()).collect(),
    }
}

fn duplicate_template_finding_key(c: &DuplicateTemplateName) -> FindingKey {
    FindingKey::DuplicateTemplateName {
        name: c.name.clone(),
        owners: c.owners.iter().cloned().collect(),
    }
}

/// Every [`FindingKey`] already backing `(def_type, name)` under
/// `selector` that `target` (base-compared) is a genuine party to — the
/// target's own `DefOverride`/`DuplicateTemplateName` (when contested and
/// `target` is one of its owners), plus one `PatchCollision` per
/// contested `sub_path` `target` is a patcher in. Used for both
/// [`ChangeKind::OwnsDef`](crate::changes::rows::ChangeKind::OwnsDef)/[`ChangeKind::OwnsTemplate`](crate::changes::rows::ChangeKind::OwnsTemplate) rows (target owns
/// this) and [`ChangeKind::PatchesDef`](crate::changes::rows::ChangeKind::PatchesDef) rows (target patches a foreign
/// owner's def/template) alike — the same target can be a member of
/// either shape of finding regardless of which kind of row it's attached
/// to. BTree-ordered and deduplicated; never a finding [`GeneratedMods`]
/// says the ledger would hide.
///
/// `DuplicateTemplateName`'s own conflict index is keyed by template
/// `Name` alone (see [`ChangeRow::finding_keys`](crate::changes::rows::ChangeRow::finding_keys)'s own doc comment for
/// why), so its arm additionally requires a second, *different*-base
/// owner in the conflict — otherwise a mod that merely owns a
/// same-named template under its own unrelated `def_type` would borrow a
/// Name collision that isn't actually about its own registration.
pub(super) fn contested_finding_keys(
    index: &ConflictIndex<'_>,
    generated: &GeneratedMods,
    def_type: &str,
    name: &str,
    selector: Selector,
    target: &ModId,
) -> Vec<FindingKey> {
    let mut keys: BTreeSet<FindingKey> = BTreeSet::new();

    match selector {
        Selector::DefName => {
            if let Some(c) = index.def_overrides.get(&(def_type, name)) {
                let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
                if owners.iter().any(|o| o.base() == *target)
                    && !owner_set_hidden(generated, &owners)
                {
                    keys.insert(def_override_finding_key(c));
                }
            }
        }
        Selector::NameAttr => {
            if let Some(c) = index.duplicate_templates.get(name) {
                let owners: BTreeSet<ModId> = c.owners.iter().cloned().collect();
                let is_member = owners.iter().any(|o| o.base() == *target);
                let has_other_owner = owners.iter().any(|o| o.base() != *target);
                if is_member && has_other_owner && !owner_set_hidden(generated, &owners) {
                    keys.insert(duplicate_template_finding_key(c));
                }
            }
        }
    }

    if let Some(collisions) = index.patch_collisions.get(&(def_type, name, selector)) {
        for c in collisions {
            let mods: BTreeSet<ModId> = c.mods.iter().map(|entry| entry.mod_id.clone()).collect();
            if mods.iter().any(|m| m.base() == *target) && !owner_set_hidden(generated, &mods) {
                keys.insert(patch_collision_finding_key(c));
            }
        }
    }

    keys.into_iter().collect()
}
