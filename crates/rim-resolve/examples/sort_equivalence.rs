//! Asserts that renaming (via [`crate::examples::anonymize_report`],
//! run first) is behaviour-neutral: the sorted order of the anonymized
//! report is the *image under the mapping* of the sorted order of the
//! real report, `dropped`/`any_of_choices`/`warnings`/
//! `kendall_tau_inversions`/`positions_changed` are identical, and (since
//! the order/summary checks alone pass even when an edge is deleted or
//! flipped in the anonymized report; see the kill test below) so are
//! `Report.edges`/`Report.constraints` themselves, structurally: every
//! real `(after, before, kind, load_time, status)` edge tuple and
//! `(after, assembly, candidates, load_time)` constraint tuple, mapped
//! through the same id/assembly substitution `anonymize_report` applied,
//! must appear in the anonymized report's own set, and vice versa — plus
//! per-conflict-kind counts. Ships no data of its own; safe to publish.
//!
//! ```text
//! cargo run -p rim-resolve --example anonymize_report -- \
//!     .journal/local/report.json .journal/local/report.anon.json .journal/local/mapping.json
//! cargo run -p rim-resolve --example sort_equivalence -- \
//!     .journal/local/report.json .journal/local/report.anon.json .journal/local/mapping.json
//! ```
//!
//! # Kill test
//!
//! The structural check catches a mutation rather than passing
//! vacuously: deleting one entry from
//! `.journal/local/report.anon.json`'s own `edges` array, or swapping one edge's
//! `after`/`before`, makes this binary exit 1 with a specific
//! "missing from anon" / "extra in anon" line naming the exact mapped
//! tuple; restoring the file makes it pass again. The order/summary
//! checks alone do **not** catch either mutation (a single edge rarely
//! changes the sorted order or the aggregate counts) — which is exactly
//! why this structural pass exists, not a restatement of the same
//! guarantee.

use std::collections::{BTreeMap, BTreeSet};
use std::env;

use rim_analyzer::domain::{Conflict, Constraint, LoadOrder, ModId, Report};
use rim_resolve::domain::{RuleSet, SorterOverrides, Tagging};
use rim_resolve::sort::{EnforcedLayers, SortOutcome, TieBreak, sort};

fn load_report(path: &str) -> Report {
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        eprintln!("failed to parse {path}: {e}");
        std::process::exit(1);
    })
}

/// Reads one top-level `original -> token` section (`"mod_id"` or
/// `"assembly"`) out of `anonymize_report`'s own `mapping.json`.
fn load_mapping_section(path: &str, section: &str) -> BTreeMap<String, String> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        eprintln!("failed to parse {path}: {e}");
        std::process::exit(1);
    });
    let Some(map) = value.get(section).and_then(serde_json::Value::as_object) else {
        eprintln!("{path} has no {section:?} section");
        std::process::exit(1);
    };
    map.iter()
        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
        .collect()
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

/// One `(after, before, kind, load_time, status)` edge tuple, with
/// `Ord`/`Debug` from its own field types so a `BTreeSet<EdgeTuple>` can
/// be diffed and printed directly.
type EdgeTuple = (ModId, ModId, String, bool, String);
/// One `(after, assembly, candidates, load_time)` constraint tuple.
type ConstraintTuple = (ModId, String, Vec<ModId>, bool);

fn mod_id_or_exit(mapping: &BTreeMap<String, String>, id: &ModId) -> ModId {
    mapping
        .get(id.as_str())
        .map(|s| ModId::new(s.clone()))
        .unwrap_or_else(|| {
            eprintln!("no mod_id mapping entry for {id}");
            std::process::exit(1);
        })
}

fn mapped_edge_tuples(
    report: &Report,
    mod_id_map: &BTreeMap<String, String>,
) -> BTreeSet<EdgeTuple> {
    report
        .edges
        .iter()
        .map(|e| {
            (
                mod_id_or_exit(mod_id_map, &e.edge.after),
                mod_id_or_exit(mod_id_map, &e.edge.before),
                format!("{:?}", e.edge.kind),
                e.edge.load_time,
                format!("{:?}", e.status),
            )
        })
        .collect()
}

fn mapped_constraint_tuples(
    report: &Report,
    mod_id_map: &BTreeMap<String, String>,
    assembly_map: &BTreeMap<String, String>,
) -> BTreeSet<ConstraintTuple> {
    report
        .constraints
        .iter()
        .map(|c| {
            let Constraint::AnyOf {
                after,
                assembly,
                candidates,
                load_time,
                ..
            } = c;
            (
                mod_id_or_exit(mod_id_map, after),
                assembly_map
                    .get(assembly)
                    .cloned()
                    .unwrap_or_else(|| assembly.clone()),
                candidates
                    .iter()
                    .map(|id| mod_id_or_exit(mod_id_map, id))
                    .collect(),
                *load_time,
            )
        })
        .collect()
}

fn raw_edge_tuples(report: &Report) -> BTreeSet<EdgeTuple> {
    report
        .edges
        .iter()
        .map(|e| {
            (
                e.edge.after.clone(),
                e.edge.before.clone(),
                format!("{:?}", e.edge.kind),
                e.edge.load_time,
                format!("{:?}", e.status),
            )
        })
        .collect()
}

fn raw_constraint_tuples(report: &Report) -> BTreeSet<ConstraintTuple> {
    report
        .constraints
        .iter()
        .map(|c| {
            let Constraint::AnyOf {
                after,
                assembly,
                candidates,
                load_time,
                ..
            } = c;
            (
                after.clone(),
                assembly.clone(),
                candidates.clone(),
                *load_time,
            )
        })
        .collect()
}

/// Every element of `mapped_real` missing from `anon`, and vice versa --
/// empty in both directions iff the two sets are equal.
fn set_diff<T: Ord + std::fmt::Debug + Clone>(
    mapped_real: &BTreeSet<T>,
    anon: &BTreeSet<T>,
) -> (Vec<T>, Vec<T>) {
    (
        mapped_real.difference(anon).cloned().collect(),
        anon.difference(mapped_real).cloned().collect(),
    )
}

fn run_sort(report: &Report) -> SortOutcome {
    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
    sort(&rim_resolve::sort::SortInput {
        report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: TieBreak::PreserveCurrent,
    })
}

fn main() {
    let mut args = env::args().skip(1);
    let (Some(real_path), Some(anon_path), Some(mapping_path)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: sort_equivalence <real report.json> <anon report.json> <mapping.json>");
        std::process::exit(2);
    };

    let real_report = load_report(&real_path);
    let anon_report = load_report(&anon_path);
    let mod_id_map = load_mapping_section(&mapping_path, "mod_id");
    let assembly_map = load_mapping_section(&mapping_path, "assembly");

    let real_outcome = run_sort(&real_report);
    let anon_outcome = run_sort(&anon_report);

    // The real order, mapped through the same id substitution
    // `anonymize_report` applied -- this must equal the anonymized
    // report's own sorted order, element for element.
    let mapped_real_order: Vec<ModId> = real_outcome
        .order
        .as_slice()
        .iter()
        .map(|id| mod_id_or_exit(&mod_id_map, id))
        .collect();
    let anon_order: Vec<ModId> = anon_outcome.order.as_slice().to_vec();

    let mut failures = Vec::new();
    if mapped_real_order != anon_order {
        let mismatches = mapped_real_order
            .iter()
            .zip(anon_order.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(5)
            .map(|(i, (a, b))| format!("  #{i}: mapped-real {a} != anon {b}"))
            .collect::<Vec<_>>()
            .join("\n");
        failures.push(format!(
            "sorted order diverges after mapping ({} of {} positions differ):\n{mismatches}",
            mapped_real_order
                .iter()
                .zip(anon_order.iter())
                .filter(|(a, b)| a != b)
                .count(),
            mapped_real_order.len()
        ));
    }
    if real_outcome.dropped.len() != anon_outcome.dropped.len() {
        failures.push(format!(
            "dropped edge count differs: real {} vs anon {}",
            real_outcome.dropped.len(),
            anon_outcome.dropped.len()
        ));
    }
    if real_outcome.any_of_choices.len() != anon_outcome.any_of_choices.len() {
        failures.push(format!(
            "any_of_choices count differs: real {} vs anon {}",
            real_outcome.any_of_choices.len(),
            anon_outcome.any_of_choices.len()
        ));
    }
    if real_outcome.warnings.len() != anon_outcome.warnings.len() {
        failures.push(format!(
            "warnings count differs: real {} vs anon {}",
            real_outcome.warnings.len(),
            anon_outcome.warnings.len()
        ));
    }
    if real_outcome.stats.kendall_tau_inversions != anon_outcome.stats.kendall_tau_inversions {
        failures.push(format!(
            "kendall_tau_inversions differs: real {} vs anon {}",
            real_outcome.stats.kendall_tau_inversions, anon_outcome.stats.kendall_tau_inversions
        ));
    }
    if real_outcome.stats.positions_changed != anon_outcome.stats.positions_changed {
        failures.push(format!(
            "positions_changed differs: real {} vs anon {}",
            real_outcome.stats.positions_changed, anon_outcome.stats.positions_changed
        ));
    }

    // --- structural comparison -----------------------
    let mapped_real_edges = mapped_edge_tuples(&real_report, &mod_id_map);
    let anon_edges = raw_edge_tuples(&anon_report);
    let (missing_edges, extra_edges) = set_diff(&mapped_real_edges, &anon_edges);
    if !missing_edges.is_empty() || !extra_edges.is_empty() {
        failures.push(format!(
            "edges differ after mapping: {} missing from anon, {} extra in anon\n\
             {}{}",
            missing_edges.len(),
            extra_edges.len(),
            missing_edges
                .iter()
                .take(5)
                .map(|t| format!("  missing from anon: {t:?}\n"))
                .collect::<String>(),
            extra_edges
                .iter()
                .take(5)
                .map(|t| format!("  extra in anon: {t:?}\n"))
                .collect::<String>(),
        ));
    }

    let mapped_real_constraints =
        mapped_constraint_tuples(&real_report, &mod_id_map, &assembly_map);
    let anon_constraints = raw_constraint_tuples(&anon_report);
    let (missing_constraints, extra_constraints) =
        set_diff(&mapped_real_constraints, &anon_constraints);
    if !missing_constraints.is_empty() || !extra_constraints.is_empty() {
        failures.push(format!(
            "constraints differ after mapping: {} missing from anon, {} extra in anon\n\
             {}{}",
            missing_constraints.len(),
            extra_constraints.len(),
            missing_constraints
                .iter()
                .take(5)
                .map(|t| format!("  missing from anon: {t:?}\n"))
                .collect::<String>(),
            extra_constraints
                .iter()
                .take(5)
                .map(|t| format!("  extra in anon: {t:?}\n"))
                .collect::<String>(),
        ));
    }

    // --- per-conflict-kind counts -----------------------------------------
    let mut real_kind_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for conflict in &real_report.conflicts {
        *real_kind_counts.entry(conflict_kind(conflict)).or_insert(0) += 1;
    }
    let mut anon_kind_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for conflict in &anon_report.conflicts {
        *anon_kind_counts.entry(conflict_kind(conflict)).or_insert(0) += 1;
    }
    if real_kind_counts != anon_kind_counts {
        failures.push(format!(
            "per-kind conflict counts differ: real {real_kind_counts:?} vs anon \
             {anon_kind_counts:?}"
        ));
    }

    if failures.is_empty() {
        println!(
            "sort_equivalence: PASS -- {} mods, {} dropped, {} any_of_choices, {} warnings, \
             kendall_tau_inversions={}, positions_changed={} (identical, mapped order matches), \
             {} edges / {} constraints structurally identical after mapping, per-kind conflict \
             counts identical ({} kinds)",
            real_report.mods.len(),
            real_outcome.dropped.len(),
            real_outcome.any_of_choices.len(),
            real_outcome.warnings.len(),
            real_outcome.stats.kendall_tau_inversions,
            real_outcome.stats.positions_changed,
            mapped_real_edges.len(),
            mapped_real_constraints.len(),
            real_kind_counts.len(),
        );
    } else {
        println!("sort_equivalence: FAIL");
        for failure in &failures {
            println!("  - {failure}");
        }
        std::process::exit(1);
    }
}
