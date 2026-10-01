//! The generator spec (`fixture gen` input) and its validation.

use anyhow::{Result, bail};
use serde::Deserialize;

/// The committed spec's own top-level shape
/// (`crates/rim-resolve/tests/fixtures/synthetic-install.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct Spec {
    pub schema: u32,
    pub seed: u64,
    pub game_version: String,
    pub mod_count: usize,
    pub archetypes: Archetypes,
    pub planted: Planted,
    pub inactive: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Archetypes {
    pub frameworks: usize,
    pub framework_dependents: usize,
    pub content_mods: usize,
    pub retextures: usize,
    pub translations: usize,
    pub patch_only: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Planted {
    pub def_overrides: usize,
    pub patch_collisions: usize,
    pub texture_overrides: usize,
    pub sound_overrides: usize,
    pub duplicate_assemblies: usize,
    pub likely_duplicate_mods: usize,
    pub duplicate_template_names: usize,
    pub keyed_translation_collisions: usize,
    pub missing_texture_paths: usize,
    pub runtime_patch_collisions: usize,
    pub transpiler_collisions: usize,
    pub cycles_forcing_dropped_edges: usize,
    pub any_of_constraints: usize,
    pub missing_dependencies: usize,
    pub incompatible_pairs: usize,
    pub unsupported_versions: usize,
    pub top_pinned: usize,
    pub bottom_pinned: usize,
}

/// Every up-front check `generate` runs before writing a single byte, so
/// no check fires only after archetype mods are already on disk.
pub(super) fn validate_spec(spec: &Spec) -> Result<()> {
    if spec.schema != 1 {
        bail!("unsupported spec schema {} (expected 1)", spec.schema);
    }
    // This generator's every hardcoded version-folder literal ("1.6")
    // below is pinned to this one supported value -- a spec asking for a
    // different game version would need every one of those updated too,
    // so it's rejected up front rather than silently producing a
    // half-versioned install.
    if spec.game_version != "1.6" {
        bail!(
            "this generator only supports game_version \"1.6\" today, got {:?}",
            spec.game_version
        );
    }
    let archetype_sum = spec.archetypes.frameworks
        + spec.archetypes.framework_dependents
        + spec.archetypes.content_mods
        + spec.archetypes.retextures
        + spec.archetypes.translations
        + spec.archetypes.patch_only;
    if archetype_sum != spec.mod_count {
        bail!(
            "archetypes sum to {archetype_sum} but mod_count is {} -- keep them in sync",
            spec.mod_count
        );
    }
    // Assembly-derived constructs are fixed by the committed blob set
    // (assemblies/README.md), not chosen at generation time -- assert the
    // spec agrees rather than silently drifting from what was actually
    // planted.
    if spec.planted.duplicate_assemblies != 3 {
        bail!(
            "planted.duplicate_assemblies must be 3 (exports x2 [1.0.0.0 + 1.1.0.0], \
             extendshard x4, callssoft x2 -- fixed by the committed assembly blobs), got {}",
            spec.planted.duplicate_assemblies
        );
    }
    if spec.planted.any_of_constraints != 6 {
        bail!(
            "planted.any_of_constraints must be 6 (one per ExtendsHard/CallsSoft-shipping \
             dependent against the two ambiguous Exports owners -- 4 load-time, 2 lazy; \
             ExportsSolo/ExtendsSolo are deliberately unambiguous and contribute none), got {}",
            spec.planted.any_of_constraints
        );
    }
    if spec.planted.runtime_patch_collisions != 6 {
        bail!(
            "planted.runtime_patch_collisions must be 6 (six target pairs), got {}",
            spec.planted.runtime_patch_collisions
        );
    }
    if spec.planted.transpiler_collisions != 2 {
        bail!(
            "planted.transpiler_collisions must be 2 (TargetAlpha/TargetBeta), got {}",
            spec.planted.transpiler_collisions
        );
    }
    // The retexture/translation pairing schemes (`plant_retextures`/
    // `plant_translations`) need `2 * planted` archetype mods to give
    // every pair its own distinct path/key: reusing one literal path/key
    // across every pair would collapse N planted conflicts into one big
    // N-owner one.
    if spec.archetypes.retextures < spec.planted.texture_overrides * 2 {
        bail!(
            "archetypes.retextures ({}) must be at least 2x planted.texture_overrides ({}) to \
             fit one pair per override",
            spec.archetypes.retextures,
            spec.planted.texture_overrides
        );
    }
    if spec.archetypes.translations < spec.planted.keyed_translation_collisions * 2 {
        bail!(
            "archetypes.translations ({}) must be at least 2x \
             planted.keyed_translation_collisions ({}) to fit one pair per collision",
            spec.archetypes.translations,
            spec.planted.keyed_translation_collisions
        );
    }
    // `plant_patch_only_zoo` uses indices 1..=13 for one-off constructs.
    if spec.archetypes.patch_only < 13 {
        bail!(
            "archetypes.patch_only ({}) must be at least 13 (the xpath/grammar zoo's own \
             one-off constructs, one per index)",
            spec.archetypes.patch_only
        );
    }
    Ok(())
}
