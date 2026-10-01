//! Trims a full-size analyzer report down to a small report: every mod,
//! edge, and constraint (needed for the sorter to run at all), every
//! sanity list, and the first 500 conflicts per kind.
//!
//! **Not how the committed golden fixture is produced**:
//! `tests/golden/report.synthetic.json` is produced
//! by the synthetic install generator (`rimmerge fixture gen`) plus
//! `rimmerge fixture trim` (the CLI subcommand, `apps/cli/src/commands/
//! fixture.rs` — this file's own logic, duplicated there deliberately),
//! per `sort_golden.rs`'s own `# regenerate` doc comment. This example is
//! generic trimming
//! utility, still useful for shrinking any full-size report (a real
//! `.journal/local/report.json`, or a synthetic one) by hand; it is not tied to
//! either pipeline.
//!
//! Run from the workspace root:
//!
//! ```text
//! cargo run -p rim-resolve --example trim_fixture -- \
//!     <input report.json> <output trimmed.json>
//! ```
//!
//! Defaults to `.journal/local/report.json` -> `tests/golden/report.trimmed.json`
//! (relative to this crate, and **not** a path that exists in the repo)
//! when no arguments are given — pass both explicitly.

use std::collections::HashMap;
use std::path::PathBuf;

use rim_analyzer::domain::{Conflict, Report};

const CONFLICTS_PER_KIND: usize = 500;

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

fn trim(report: Report) -> Report {
    let mut kept_per_kind: HashMap<&'static str, usize> = HashMap::new();
    let conflicts = report
        .conflicts
        .into_iter()
        .filter(|conflict| {
            let kind = conflict_kind(conflict);
            let count = kept_per_kind.entry(kind).or_insert(0);
            *count += 1;
            *count <= CONFLICTS_PER_KIND
        })
        .collect();

    Report {
        conflicts,
        ..report
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("../../.journal/local/report.json"));
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("tests/golden/report.trimmed.json"));

    let Ok(bytes) = std::fs::read(&input) else {
        eprintln!("skipping: {} not found ", input.display());
        return;
    };
    let report: Report = match serde_json::from_slice(&bytes) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("failed to parse {}: {error}", input.display());
            std::process::exit(1);
        }
    };

    let before = report.mods.len();
    let trimmed = trim(report);
    let after_conflicts = trimmed.conflicts.len();

    match serde_json::to_vec_pretty(&trimmed) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&output, json) {
                eprintln!("failed to write {}: {error}", output.display());
                std::process::exit(1);
            }
            println!(
                "wrote {} ({} mods, {} trimmed conflicts) to {}",
                input.display(),
                before,
                after_conflicts,
                output.display()
            );
        }
        Err(error) => {
            eprintln!("failed to serialize trimmed report: {error}");
            std::process::exit(1);
        }
    }
}
