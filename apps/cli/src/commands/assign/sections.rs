//! `assign add-section`/`remove-section`.

use std::fmt::Write as _;

use anyhow::{Context, bail};
use clap::Args;
use rim_resolve::domain::SectionError;
use rim_session::use_cases::{AddAssignmentSection, RemoveAssignmentSection};
use serde::Serialize;

use super::propose::{print_candidates_json, print_candidates_text};
use super::{format_row_key, parse_assignment_id};
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct AddSectionArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// The def type to infer and add as a new section.
    #[arg(long = "def-type")]
    def_type: String,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_add_section(args: &AddSectionArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let use_case = AddAssignmentSection::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    let candidate = match use_case.execute(&mut session, &id, &args.def_type) {
        Ok(candidate) => candidate,
        Err(error @ rim_session::use_cases::AddAssignmentSectionError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("adding the section"),
    };

    if args.json {
        return print_candidates_json(std::slice::from_ref(&candidate));
    }
    let kind = if candidate.schema.has_target_key() {
        "target-keyed"
    } else {
        "free-standing"
    };
    println!(
        "added section {} ({kind})",
        TerminalSafe::line(&candidate.def_type)
    );
    print_candidates_text(std::slice::from_ref(&candidate));
    Ok(())
}

#[derive(Debug, Args)]
pub struct RemoveSectionArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// The section's def type to remove.
    #[arg(long = "def-type")]
    def_type: String,
    /// Remove the section even if another section's row still references
    /// one of its own free-standing rows, leaving that reference
    /// dangling (a later `export` reports it as a skip).
    #[arg(long)]
    force: bool,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct RemoveSectionJson {
    removed: bool,
    def_type: String,
    row_count: usize,
}

pub(super) fn run_remove_section(args: &RemoveSectionArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let use_case = RemoveAssignmentSection::new(rim_io::JsonAssignmentProjectStore::new());
    let removed = match use_case.execute(&mut session, &id, &args.def_type, args.force) {
        Ok(removed) => removed,
        Err(error @ rim_session::use_cases::RemoveAssignmentSectionError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(rim_session::use_cases::RemoveAssignmentSectionError::Section(
            SectionError::SectionInUse { referenced_by },
        )) => {
            let mut message = String::new();
            let _ = writeln!(
                message,
                "section {:?} is still referenced by {} row(s) in other sections:",
                args.def_type,
                referenced_by.len()
            );
            for (def_type, key, path) in &referenced_by {
                let _ = writeln!(message, "  {def_type} {} {path}", format_row_key(key));
            }
            message.push_str(
                "pass --force to remove it anyway (references are left dangling; \
                 `export` reports them as skips)",
            );
            bail!(message);
        }
        Err(error) => return Err(error).context("removing the section"),
    };

    if args.json {
        let json = RemoveSectionJson {
            removed: removed.is_some(),
            def_type: args.def_type.clone(),
            row_count: removed
                .as_ref()
                .map(|section| section.rows.len())
                .unwrap_or(0),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing the outcome")?;
        println!("{text}");
        return Ok(());
    }
    match removed {
        Some(section) => println!(
            "removed section {} ({} row(s))",
            args.def_type,
            section.rows.len()
        ),
        None => println!("no section for def type {}", args.def_type),
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `assign items`
// ---------------------------------------------------------------------
