//! `rimmerge fixture trim`: shrinks a full analyzer report into a
//! committable golden fixture — keeps mods, edges, constraints, and every
//! sanity list, but caps `conflicts` at the first 500 entries per kind.
//!
//! `rimmerge fixture gen`: writes a complete synthetic RimWorld install
//! from a committed spec (`crate::commands::fixture_gen`).

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};
use rim_analyzer::domain::Conflict;

use crate::commands::fixture_gen::{self, Spec};
use crate::common::read_report;

const MAX_PER_KIND: usize = 500;

#[derive(Debug, Subcommand)]
pub enum FixtureCommand {
    /// Trims a full report into a smaller golden fixture.
    Trim(TrimArgs),
    /// Generates a complete synthetic RimWorld install from a spec.
    Gen(GenArgs),
}

#[derive(Debug, Args)]
pub struct TrimArgs {
    /// The full report to trim.
    #[arg(long = "in")]
    input: PathBuf,
    /// Where to write the trimmed report.
    #[arg(long = "out")]
    output: PathBuf,
}

#[derive(Debug, Args)]
pub struct GenArgs {
    /// The generator spec (e.g.
    /// `crates/rim-resolve/tests/fixtures/synthetic-install.json`).
    #[arg(long = "spec")]
    spec: PathBuf,
    /// Directory to write the generated install into (created if
    /// missing; must be empty or nonexistent).
    #[arg(long = "out")]
    output: PathBuf,
}

fn conflict_kind(conflict: &Conflict) -> &'static str {
    match conflict {
        Conflict::DefOverride(_) => "def_override",
        Conflict::PatchCollision(_) => "patch_collision",
        Conflict::TextureOverride(_) => "texture_override",
        Conflict::DuplicateAssembly(_) => "duplicate_assembly",
        Conflict::LikelyDuplicateMod(_) => "likely_duplicate_mod",
        Conflict::DuplicateTemplateName(_) => "duplicate_template_name",
        Conflict::KeyedTranslationCollision(_) => "keyed_translation_collision",
        Conflict::SoundOverride(_) => "sound_override",
        Conflict::RuntimePatchCollision(_) => "runtime_patch_collision",
        Conflict::MissingTexturePath(_) => "missing_texture_path",
        Conflict::TranspilerCollision(_) => "transpiler_collision",
        Conflict::UndecodableTexture(_) => "undecodable_texture",
        Conflict::BrokenInheritance(_) => "broken_inheritance",
        Conflict::NearMissModReference(_) => "near_miss_mod_reference",
        Conflict::DiscardedAddition(_) => "discarded_addition",
        Conflict::DanglingDefReference(_) => "dangling_def_reference",
    }
}

pub fn run(command: &FixtureCommand) -> anyhow::Result<()> {
    match command {
        FixtureCommand::Trim(args) => trim(args),
        FixtureCommand::Gen(args) => run_gen(args),
    }
}

fn run_gen(args: &GenArgs) -> anyhow::Result<()> {
    let bytes =
        std::fs::read(&args.spec).with_context(|| format!("reading {}", args.spec.display()))?;
    let spec: Spec = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {}", args.spec.display()))?;
    let summary = fixture_gen::generate(&spec, &args.spec, &args.output)
        .with_context(|| format!("generating install into {}", args.output.display()))?;
    println!(
        "generated {} active mods, {} inactive mods ({} mod folders total) into {}",
        summary.active_mods,
        summary.inactive_mods,
        summary.total_mod_folders,
        args.output.display()
    );
    Ok(())
}

fn trim(args: &TrimArgs) -> anyhow::Result<()> {
    let mut report = read_report(&args.input)?;
    let original_conflicts = report.conflicts.len();

    let mut kept_per_kind: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    report.conflicts.retain(|conflict| {
        let kind = conflict_kind(conflict);
        let count = kept_per_kind.entry(kind).or_insert(0);
        if *count < MAX_PER_KIND {
            *count += 1;
            true
        } else {
            false
        }
    });

    let json = serde_json::to_string_pretty(&report).context("serializing trimmed report")?;
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&args.output, json)
        .with_context(|| format!("writing {}", args.output.display()))?;

    println!(
        "trimmed {} conflicts -> {} (kept {} mods, {} edges, {} constraints)",
        original_conflicts,
        report.conflicts.len(),
        report.mods.len(),
        report.edges.len(),
        report.constraints.len()
    );
    Ok(())
}
