//! Template-name, keyed-translation, and sound collisions.

use crate::domain::{
    Conflict, DuplicateTemplateName, KeyedTranslationCollision, LoadOrder, SoundOverride,
};

use super::sorted_by_load_order;
use crate::analysis::indices::Indices;

/// A `Name` attribute registered by more than one active mod: RimWorld's XML
/// inheritance resolves `ParentName`/`[@Name="X"]` references against
/// whichever template was registered last.
#[must_use]
pub fn duplicate_template_names(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .template_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(name, owners)| {
            Conflict::DuplicateTemplateName(DuplicateTemplateName {
                name: name.clone(),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}

/// The same `Languages/*/Keyed` key defined by more than one active mod —
/// last-loaded definition wins. Emitted one conflict per key; the ledger
/// groups these per mod pair for the inbox, since one busy pair of mods can
/// otherwise flood it with one row per colliding key.
#[must_use]
pub fn keyed_translation_collisions(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .translation_key_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(key, owners)| {
            Conflict::KeyedTranslationCollision(KeyedTranslationCollision {
                key: key.clone(),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}

/// The same normalized sound path shipped by more than one active mod —
/// mirrors
/// [`texture_overrides`](crate::analysis::conflicts::textures::texture_overrides)
/// for `Sounds/`.
#[must_use]
pub fn sound_overrides(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .sound_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(path, owners)| {
            Conflict::SoundOverride(SoundOverride {
                path: path.clone(),
                same_author: indices.shared_author(owners),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}
