//! `rimmerge fixture gen`: writes a complete, synthetic RimWorld install
//! from a committed spec —
//! `Version.txt`, `Data/Core`/DLC folders, a `Mods/` tree, a Steam
//! workshop content tree, and `ModsConfig.xml`. Deterministic (seeded,
//! no RNG state escapes this module), no network, no clock: every byte
//! written is a function of the spec alone.
//!
//! Dev/test tooling, not the shipped app's own composition root — see
//! this crate's own `CLAUDE.md` for why `fixture gen`/`fixture trim`
//! carry real generation logic directly here rather than in
//! `rim-session`/`rim-resolve` (the same precedent `fixture trim`
//! already set: `rim-resolve/examples/trim_fixture.rs` and this crate's
//! own `trim` duplicate the identical logic rather than share a port).
//!
//! # Structure
//!
//! [`generate`] validates the spec, then drives [`Population`] through
//! one `plant_*` method per construct in the synthetic install's
//! coverage checklist: this method list **is** the coverage table, so keep
//! the two from drifting. Each method owns exactly the mods/files one
//! construct needs; shared identifiers (framework ids, content-mod
//! folders) are read off `Population`'s own fields, populated by the
//! methods that ran before it in [`generate`]'s own call order.

use std::path::Path;

use anyhow::{Result, bail};
use mods::assemblies_dir;
use population::Population;
use spec::validate_spec;

mod mods;
mod population;
mod spec;

pub use spec::Spec;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "fixture_gen/fixture_gen_tests.rs"]
mod tests;

/// A summary of what was generated, printed by the CLI command and
/// asserted against by `the_synthetic_golden_is_large_and_nontrivial`
/// (via the analyzer's own report, not this struct — this is only a
/// human-readable count for the `fixture gen` command's own stdout).
#[derive(Debug)]
pub struct GenSummary {
    pub active_mods: usize,
    pub inactive_mods: usize,
    pub total_mod_folders: usize,
}

/// Generates the complete install described by `spec` into `out` (an
/// empty or nonexistent directory): `<out>/game/...` and
/// `<out>/workshop/content/294100/...`. `spec_path` locates the
/// committed synthetic-assembly blobs, which live beside the spec file.
pub fn generate(spec: &Spec, spec_path: &Path, out: &Path) -> Result<GenSummary> {
    validate_spec(spec)?;

    // `--out` must be empty or nonexistent -- bail
    // before writing anything rather than silently mixing generated
    // files into whatever was already there.
    if let Ok(mut entries) = std::fs::read_dir(out)
        && entries.next().is_some()
    {
        bail!(
            "--out {} must be empty or nonexistent (it already contains files)",
            out.display()
        );
    }

    let bin = assemblies_dir(spec_path)?;
    if !bin.is_dir() {
        bail!(
            "{} not found -- run scripts/build-synthetic-assemblies.ps1 first",
            bin.display()
        );
    }

    let mut population = Population::new(spec, bin, out.to_path_buf());
    population.plant_core_and_dlcs()?;
    population.plant_keep_list_infrastructure()?;
    population.plant_frameworks()?;
    population.plant_framework_dependents()?;
    population.plant_content_mods()?;
    population.plant_retextures()?;
    population.plant_missing_texture_paths()?;
    population.plant_sound_overrides()?;
    population.plant_translations()?;
    population.plant_likely_duplicate_mods()?;
    population.plant_def_overrides()?;
    population.plant_duplicate_template_names()?;
    population.plant_parent_template_cross_mod_child()?;
    population.plant_retexture_after_owner()?;
    population.plant_force_load_pair()?;
    population.plant_patch_collisions()?;
    population.plant_patch_only_zoo()?;
    population.plant_cycles()?;
    population.plant_missing_dependencies()?;
    population.plant_incompatible_pairs()?;
    population.plant_unsupported_versions()?;
    population.plant_top_bottom_pins()?;
    population.plant_inactive_mods()?;
    population.plant_shadowed_pair()?;
    population.finalize()
}
