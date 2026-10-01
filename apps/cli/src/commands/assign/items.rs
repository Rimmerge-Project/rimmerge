//! `assign items`: listing the target items a section can fill.

use anyhow::Context;
use clap::Args;
use rim_resolve::domain::{AssignmentProject, Section};
use rim_session::use_cases::{ListItems, ListItemsFilter};
use serde::Serialize;

use super::parse_assignment_id;
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct ItemsArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The item def type to list, e.g. `example.PartDef`.
    def_type: String,
    /// Only defs whose `defName` contains this substring
    /// (case-insensitive).
    #[arg(long)]
    search: Option<String>,
    /// How many matching defs to skip before collecting the page.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// How many defs to collect, capped at `MAX_PAGE_SIZE`.
    #[arg(long, default_value_t = rim_session::MAX_PAGE_SIZE)]
    limit: usize,
    /// An assignment project whose own free-standing rows of this type
    /// (if any) are prepended, labelled as owned by "this project".
    #[arg(long)]
    assignment: Option<String>,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_items(args: &ItemsArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let project = args
        .assignment
        .as_deref()
        .map(|raw| -> anyhow::Result<AssignmentProject> {
            let id = parse_assignment_id(raw)?;
            session
                .assignment(&id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))
        })
        .transpose()?;

    let filter = ListItemsFilter {
        search: args.search.clone(),
        offset: args.offset,
        limit: args.limit,
    };
    let use_case = ListItems::new(rim_io::FileDefSourceReader::new());
    let page = use_case
        .execute(&mut session, project.as_ref(), &args.def_type, &filter)
        .map_err(|error| anyhow::anyhow!("listing items: {error}"))?;

    // Own-ness is read off the project's own section, never guessed from
    // `item.owner` — the owner
    // column of an active def happens to be a real `ModId`, never this
    // project's own package id, but that's incidental, not the contract.
    let own_names: std::collections::BTreeSet<String> = project
        .as_ref()
        .and_then(|p| p.section(&args.def_type))
        .map(Section::own_instance_names)
        .unwrap_or_default();

    if args.json {
        let json = ItemsJson {
            total: page.total,
            items: page
                .items
                .iter()
                .map(|item| ItemJson {
                    def_name: item.def.def_name.clone(),
                    owner: item.owner.to_string(),
                    own: own_names.contains(&item.def.def_name),
                    hint: item.hint.clone(),
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing items as JSON")?;
        println!("{text}");
        return Ok(());
    }

    println!(
        "{} item(s) of type {} — {} total",
        page.items.len(),
        TerminalSafe::line(&args.def_type),
        page.total
    );
    println!("{:<32} {:<20} hint", "defName", "owner");
    for item in &page.items {
        let owner = if own_names.contains(&item.def.def_name) {
            "this project".to_string()
        } else {
            TerminalSafe::line(&item.owner).to_string()
        };
        println!(
            "{:<32} {:<20} {}",
            TerminalSafe::line(&item.def.def_name).to_string(),
            owner,
            TerminalSafe::line(item.hint.as_deref().unwrap_or("-"))
        );
    }
    let shown = page.items.len();
    if shown < page.total {
        println!(
            "... {} more item(s) not shown (see --offset/--limit)",
            page.total - shown
        );
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ItemJson {
    def_name: String,
    owner: String,
    own: bool,
    hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct ItemsJson {
    total: usize,
    items: Vec<ItemJson>,
}

// ---------------------------------------------------------------------
// `assign copy-from`
// ---------------------------------------------------------------------
