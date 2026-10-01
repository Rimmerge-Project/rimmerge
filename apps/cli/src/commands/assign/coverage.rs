//! `assign coverage`: which rows the export will honour and which it will skip.

use anyhow::{Context, bail};
use clap::Args;
use rim_resolve::domain::Winner;
use rim_session::use_cases::AssignmentCoverage;
use serde::Serialize;

use super::{parse_assignment_id, resolve_section_type};
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct CoverageArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// Which section (by def type) to build coverage for — optional when
    /// the project has exactly one section.
    #[arg(long)]
    section: Option<String>,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

fn format_winner(winner: &Winner) -> String {
    match winner {
        Winner::Existing(existing) => {
            format!("{} ({})", existing.owner, existing.instance_def_name)
        }
        Winner::ThisProject => "this project".to_string(),
    }
}

/// `RowIntent` has no `Display` of its own (a plain, field-less enum, so
/// its derived `Debug` is already stable and bounded — `"Cover"`/
/// `"Override"`, never raw struct syntax); this local match is still
/// preferred over `{:?}` for the same reason `format_role`/
/// `format_cardinality` are local matches rather than relying on any
/// domain type's `Debug`: this module never trusts a domain `Debug` impl
/// to stay stable enough for a table column or a `--json` field.
fn format_row_intent(intent: rim_resolve::domain::RowIntent) -> &'static str {
    match intent {
        rim_resolve::domain::RowIntent::Cover => "Cover",
        rim_resolve::domain::RowIntent::Override => "Override",
    }
}

pub(super) fn run_coverage(args: &CoverageArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let project = session
        .assignment(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))?;
    let def_type = resolve_section_type(&project, args.section.as_deref())?;

    let use_case = AssignmentCoverage::new(rim_io::FileDefSourceReader::new());
    let coverage = match use_case.execute(&mut session, &id, &def_type) {
        Ok(coverage) => coverage,
        Err(error @ rim_session::use_cases::AssignmentCoverageError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("building the coverage work queue"),
    };

    if args.json {
        return print_coverage_json(&coverage);
    }
    if !coverage.applicable {
        println!(
            "coverage not applicable — section {def_type:?} is free-standing (\"new def\"), with no TargetKey field to match a target through"
        );
        return Ok(());
    }
    println!(
        "{:<40} {:<20} {:<10} {:<7} winner",
        "target", "owner", "intent", "matches"
    );
    for row in &coverage.rows {
        println!(
            "{:<40} {:<20} {:<10} {:<7} {}",
            TerminalSafe::line(&row.target.def).to_string(),
            TerminalSafe::line(&row.owner).to_string(),
            format_row_intent(row.intent),
            row.matches.len(),
            TerminalSafe::line(
                row.winner
                    .as_ref()
                    .map(format_winner)
                    .unwrap_or_else(|| "-".to_string())
            )
        );
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct CoverageRowJson {
    def_type: String,
    def_name: String,
    owner: String,
    intent: String,
    matches: usize,
    has_row: bool,
    winner: Option<String>,
}

#[derive(Debug, Serialize)]
struct CoverageJson {
    /// `false` for a standalone ("new def") project — see
    /// [`rim_resolve::domain::Coverage::applicable`]'s own doc comment.
    applicable: bool,
    rows: Vec<CoverageRowJson>,
}

fn print_coverage_json(coverage: &rim_resolve::domain::Coverage) -> anyhow::Result<()> {
    let json = CoverageJson {
        applicable: coverage.applicable,
        rows: coverage
            .rows
            .iter()
            .map(|row| CoverageRowJson {
                def_type: row.target.def.def_type.clone(),
                def_name: row.target.def.def_name.clone(),
                owner: row.owner.to_string(),
                intent: format_row_intent(row.intent).to_string(),
                matches: row.matches.len(),
                has_row: row.has_row,
                winner: row.winner.as_ref().map(format_winner),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).context("serializing coverage as JSON")?;
    println!("{text}");
    Ok(())
}

// ---------------------------------------------------------------------
// `assign export`
// ---------------------------------------------------------------------
