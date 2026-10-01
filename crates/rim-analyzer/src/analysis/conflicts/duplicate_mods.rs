//! Likely duplicate or forked mods, by shared defs.

use std::collections::{HashMap, HashSet};

use crate::domain::{Conflict, DeclaredOrder, LikelyDuplicateMod, ModId, ScannedMod};

use crate::analysis::indices::{ActiveMods, DefKey, Indices, may_require_satisfied};

/// The minimum number of identical `(def_type, def_name)` keys two mods
/// must share before they're even considered as a possible duplicate/fork
/// pair — below this, an overlap is just as likely coincidental.
const MIN_SHARED_DEFS: usize = 10;

/// The minimum fraction of the *smaller* mod's total def count that must
/// be covered by the shared set.
const MIN_SHARE_OF_SMALLER: f32 = 0.5;

/// Two active, non-vanilla mods that look like duplicates or forks of the
/// same content: a large overlap of identical `(def_type, def_name)`
/// keys, neither declaring a relation to the other, and no shared author
/// to explain the overlap deliberately (e.g. an author's split-out addon).
///
/// Vanilla (Core/DLC) mods are excluded: comparing their (large) def sets
/// against every other mod's is both meaningless — RimWorld's own content
/// isn't a "duplicate" of anything — and, at `O(n^2)` mod pairs, the most
/// expensive comparisons in the set by far.
#[must_use]
pub fn likely_duplicate_mods(scanned: &[ScannedMod], indices: &Indices) -> Vec<Conflict> {
    let mut def_sets: HashMap<&ModId, HashSet<&DefKey>> = HashMap::new();
    for (key, owners) in &indices.def_owners {
        for owner in owners {
            def_sets.entry(owner).or_default().insert(key);
        }
    }

    let declared_by_id: HashMap<&ModId, &DeclaredOrder> = scanned
        .iter()
        .filter(|sm| !sm.info.source.is_vanilla())
        .map(|sm| (&sm.info.id, &sm.info.declared))
        .collect();

    // Only a mod with at least `MIN_SHARED_DEFS` defs of its own can ever
    // reach the shared-key threshold — pruning here keeps the `O(n^2)`
    // pairwise comparison below to only the mods that could plausibly
    // qualify, which is the overwhelming majority of a real install.
    let mut candidates: Vec<&ModId> = def_sets
        .iter()
        .filter(|(id, defs)| declared_by_id.contains_key(**id) && defs.len() >= MIN_SHARED_DEFS)
        .map(|(id, _)| *id)
        .collect();
    candidates.sort();

    let mut conflicts = Vec::new();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            let (a, b) = (candidates[i], candidates[j]);
            let set_a = &def_sets[a];
            let set_b = &def_sets[b];
            let shared = set_a.intersection(set_b).count();
            if shared < MIN_SHARED_DEFS {
                continue;
            }
            let smaller = set_a.len().min(set_b.len());
            #[allow(
                clippy::cast_precision_loss,
                reason = "def counts are small (well under f32's 24-bit exact-integer range)"
            )]
            let share_of_smaller = shared as f32 / smaller as f32;
            if share_of_smaller < MIN_SHARE_OF_SMALLER {
                continue;
            }
            if declared_by_id[a].declares_any_relation(b)
                || declared_by_id[b].declares_any_relation(a)
            {
                continue;
            }
            if indices.shared_author(&[a.clone(), b.clone()]) {
                continue;
            }
            conflicts.push(Conflict::LikelyDuplicateMod(LikelyDuplicateMod {
                a: a.clone(),
                b: b.clone(),
                shared_defs: shared,
                share_of_smaller,
            }));
        }
    }
    conflicts
}

/// Every `(def_type, def_name)` key `scanned_mod` itself owns whose own
/// `MayRequire`/`MayRequireAnyOf` gate is satisfied — mirrors
/// [`Indices::build`]'s own `def_owners` gating, applied per mod instead of
/// merged across every mod: a texture-path candidate collected from a def
/// RimWorld never actually loads (the def's own gate failed) is not a real
/// fact about this install and must be skipped before
/// [`missing_texture_paths`](crate::analysis::conflicts::textures::missing_texture_paths)
/// even asks whether its value resolves.
pub(super) fn active_def_keys_of<'a>(
    scanned_mod: &'a ScannedMod,
    active: &ActiveMods,
) -> HashSet<(&'a str, &'a str)> {
    scanned_mod
        .defs
        .iter()
        .filter(|d| may_require_satisfied(&d.may_require, &d.may_require_any_of, active))
        .map(|d| (d.def_type.as_str(), d.def_name.as_str()))
        .collect()
}
