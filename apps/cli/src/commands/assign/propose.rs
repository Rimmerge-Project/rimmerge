//! `assign propose`: inferring candidate assignments and printing them.

use anyhow::Context;
use clap::Args;
use rim_analyzer::domain::ModId;
use rim_session::use_cases::{AssignmentCandidate, CreateAssignment};
use serde::Serialize;

use super::{format_cardinality, format_role, join_ids, parse_mod_set};
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct ProposeArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The selected reference set R, comma-separated package ids.
    #[arg(long)]
    refs: String,
    /// Closure members of R to opt out of, comma-separated.
    #[arg(long = "exclude-refs", default_value = "")]
    exclude_refs: String,
    /// The target set T, comma-separated package ids (Core/DLC ids
    /// included). May be empty — a candidate with no `TargetKey` field
    /// (a "new def" candidate) is only offered when this is empty.
    #[arg(long, default_value = "")]
    targets: String,
    /// Runs the real (expensive) per-type inference for exactly one
    /// candidate and prints its schema table, instead of the cheap phase-1
    /// listing (type, instance count, owners) every other invocation
    /// prints. The wizard/CLI two-phase flow: list first, then inspect one
    /// type by name.
    #[arg(long = "def-type")]
    def_type: Option<String>,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_propose(args: &ProposeArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    let refs = parse_mod_set(&args.refs);
    let excluded_refs = parse_mod_set(&args.exclude_refs);
    let targets = parse_mod_set(&args.targets);

    let use_case = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );

    if let Some(def_type) = &args.def_type {
        let candidate = use_case
            .infer_candidate(&mut session, &refs, &excluded_refs, &targets, def_type)
            .map_err(|error| anyhow::anyhow!("inferring candidate {def_type:?}: {error}"))?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{def_type:?} is not a valid candidate for this reference/target selection"
                )
            })?;
        if args.json {
            return print_candidates_json(std::slice::from_ref(&candidate));
        }
        print_candidates_text(std::slice::from_ref(&candidate));
        return Ok(());
    }

    let summaries = use_case.list_candidates(&session, &refs, &excluded_refs, &targets);
    if args.json {
        return print_candidate_summaries_json(&summaries);
    }
    print_candidate_summaries_text(&summaries);
    Ok(())
}

fn print_candidate_summaries_text(summaries: &[rim_session::use_cases::CandidateSummary]) {
    if summaries.is_empty() {
        println!("no assignment-def candidates for this reference set");
        return;
    }
    println!(
        "{:<32} {:>9} owners",
        "def type (--def-type to inspect)", "instances"
    );
    for summary in summaries {
        println!(
            "{:<32} {:>9} {}",
            TerminalSafe::line(&summary.def_type).to_string(),
            summary.instance_count,
            TerminalSafe::line(join_ids(&summary.owners))
        );
    }
}

#[derive(Debug, Serialize)]
struct CandidateSummaryJson {
    def_type: String,
    instance_count: usize,
    owners: Vec<String>,
}

fn print_candidate_summaries_json(
    summaries: &[rim_session::use_cases::CandidateSummary],
) -> anyhow::Result<()> {
    let json: Vec<CandidateSummaryJson> = summaries
        .iter()
        .map(|summary| CandidateSummaryJson {
            def_type: summary.def_type.clone(),
            instance_count: summary.instance_count,
            owners: summary.owners.iter().map(ModId::to_string).collect(),
        })
        .collect();
    let text = serde_json::to_string_pretty(&json).context("serializing candidates as JSON")?;
    println!("{text}");
    Ok(())
}

pub(super) fn print_candidates_text(candidates: &[AssignmentCandidate]) {
    if candidates.is_empty() {
        println!("no assignment-def candidates for this reference set");
        return;
    }
    for candidate in candidates {
        println!("{}", TerminalSafe::line(&candidate.def_type));
        println!(
            "  {:<28} {:<10} {:>7} {:<32} shape",
            "field", "cardinality", "observed", "role"
        );
        for (path, spec) in &candidate.schema.fields {
            let shape = candidate
                .schema
                .target_shapes
                .get(path)
                .map(|shape| join_children(&shape.required_children))
                .unwrap_or_default();
            println!(
                "  {:<28} {:<10} {:>3}/{:<3} {:<32} {}",
                TerminalSafe::line(path).to_string(),
                format_cardinality(spec.cardinality),
                spec.observed.0,
                spec.observed.1,
                TerminalSafe::line(format_role(&spec.role)).to_string(),
                TerminalSafe::line(shape)
            );
        }
        println!();
    }
}

fn join_children(children: &std::collections::BTreeSet<String>) -> String {
    if children.is_empty() {
        return "-".to_string();
    }
    children.iter().cloned().collect::<Vec<_>>().join(",")
}

#[derive(Debug, Serialize)]
pub(super) struct FieldSpecJson {
    pub(super) path: String,
    pub(super) role: String,
    pub(super) cardinality: &'static str,
    pub(super) observed_with: usize,
    pub(super) observed_of: usize,
    pub(super) user_confirmed: bool,
    pub(super) required_children: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CandidateJson {
    def_type: String,
    fields: Vec<FieldSpecJson>,
}

fn candidate_json(candidate: &AssignmentCandidate) -> CandidateJson {
    CandidateJson {
        def_type: candidate.def_type.clone(),
        fields: candidate
            .schema
            .fields
            .iter()
            .map(|(path, spec)| FieldSpecJson {
                path: path.to_string(),
                role: format_role(&spec.role),
                cardinality: format_cardinality(spec.cardinality),
                observed_with: spec.observed.0,
                observed_of: spec.observed.1,
                user_confirmed: spec.inferred_role.is_some(),
                required_children: candidate
                    .schema
                    .target_shapes
                    .get(path)
                    .map(|shape| shape.required_children.iter().cloned().collect())
                    .unwrap_or_default(),
            })
            .collect(),
    }
}

pub(super) fn print_candidates_json(candidates: &[AssignmentCandidate]) -> anyhow::Result<()> {
    let json: Vec<CandidateJson> = candidates.iter().map(candidate_json).collect();
    let text = serde_json::to_string_pretty(&json).context("serializing candidates as JSON")?;
    println!("{text}");
    Ok(())
}

// ---------------------------------------------------------------------
// `assign create`
// ---------------------------------------------------------------------
