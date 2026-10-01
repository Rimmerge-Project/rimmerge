//! Def overrides: inline and patch-injected owners of the same def.

use std::collections::BTreeMap;

use crate::domain::{Conflict, DefOverride, LoadOrder, ModId};

use super::sorted_by_load_order;
use crate::analysis::indices::{DefKey, Indices};

/// `indices.def_owners` (`Defs/`-inline ownership) merged with
/// `injected_owners` (whole-def patch injections, from
/// [`injected_def_owners`](crate::analysis::edges::injected_def_owners)) —
/// the same normalized key can be claimed by either mechanism, and a
/// collision between the two (an inline-authored def colliding with a
/// patch-injected one of the same name, or two mods each patch-injecting the
/// identical whole def) is exactly as real a [`DefOverride`] as two inline
/// owners colliding. Deduplicates a mod that somehow appears in both sources
/// for the same key (kept once, at its inline position — inline owners are
/// added first, injected ones appended after).
fn merged_def_owners(
    indices: &Indices,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
) -> BTreeMap<DefKey, Vec<ModId>> {
    let mut merged = indices.def_owners.clone();
    for (key, injected) in injected_owners {
        let entry = merged.entry(key.clone()).or_default();
        for id in injected {
            if !entry.contains(id) {
                entry.push(id.clone());
            }
        }
    }
    merged
}

/// The same `(def_type, def_name)` provided by more than one active mod —
/// either `Defs/`-inline, patch-injected as a whole def, or one of each (see
/// [`merged_def_owners`]). Records the owners in `load_order` and only
/// order-free facts besides; which owner wins and what that winner declares
/// are asked per order via [`DefOverride::winner_declares_relation`] and
/// [`DefOverride::shadows_framework`].
#[must_use]
pub fn def_overrides(
    indices: &Indices,
    load_order: &LoadOrder,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
) -> Vec<Conflict> {
    merged_def_owners(indices, injected_owners)
        .into_iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|((def_type, def_name), owners)| {
            let overrides_vanilla = owners
                .iter()
                .filter_map(|id| indices.mod_source.get(id))
                .any(|s| s.is_vanilla());
            Conflict::DefOverride(DefOverride {
                def_type,
                def_name,
                same_author: indices.shared_author(&owners),
                owners: sorted_by_load_order(&owners, load_order),
                overrides_vanilla,
            })
        })
        .collect()
}
