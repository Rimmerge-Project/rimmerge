//! `assign set-row`/`clear-row`/`copy-from`: editing one row.

use anyhow::{Context, bail};
use clap::Args;
use rim_resolve::domain::{AssignmentRow, RowKey, TargetRef};
use rim_session::use_cases::{AssignmentInstances, ClearAssignmentRow, CopyFrom, SetAssignmentRow};
use serde::Serialize;

use super::project::{CopyFromRowJson, DroppedItemSlotValueJson, RowJson};
use super::{
    parse_assignment_id, parse_def_key, parse_field_path, parse_row_values, resolve_section,
    resolve_section_type,
};
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct SetRowArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// Which section (by def type) to address — optional when the
    /// project has exactly one section.
    #[arg(long)]
    section: Option<String>,
    /// The target def, `<def_type>/<def_name>` — a target-keyed row.
    /// Requires `--key-field`; omit both for a free-standing row,
    /// addressed by `--def-name` alone.
    #[arg(long, requires = "key_field")]
    target: Option<String>,
    /// The `FieldRole::TargetKey` field this target is matched through.
    #[arg(long = "key-field", requires = "target")]
    key_field: Option<String>,
    /// The emitted `defName`. Also this row's own address (there is no
    /// target) when `--target`/`--key-field` are omitted.
    #[arg(long = "def-name")]
    def_name: String,
    /// A free-text note.
    #[arg(long)]
    note: Option<String>,
    /// `<field path>=<spec>`, repeatable; `spec` is one of
    /// `names:a,b,c`, `numbers:1.0,2.0`, `text:...`, `omit`.
    #[arg(long = "value", value_name = "PATH=SPEC")]
    value: Vec<String>,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_set_row(args: &SetRowArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let key = match (&args.target, &args.key_field) {
        (Some(target), Some(key_field)) => RowKey::Target(TargetRef {
            key_field: parse_field_path(key_field)?,
            def: parse_def_key(target)?,
        }),
        _ => RowKey::Own(args.def_name.clone()),
    };
    let values = parse_row_values(&args.value)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let project = session
        .assignment(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))?;
    let def_type = resolve_section_type(&project, args.section.as_deref())?;

    let use_case = SetAssignmentRow::new(rim_io::JsonAssignmentProjectStore::new());
    let row = AssignmentRow {
        values,
        def_name: args.def_name.clone(),
        note: args.note.clone(),
    };
    let replaced = match use_case.execute(&mut session, &id, &def_type, key, row) {
        Ok(replaced) => replaced,
        Err(error @ rim_session::use_cases::SetAssignmentRowError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("setting the row"),
    };

    if args.json {
        let json = ReplacedJson {
            replaced_def_name: replaced.map(|row| row.def_name),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing the outcome")?;
        println!("{text}");
        return Ok(());
    }
    match replaced {
        Some(previous) => println!(
            "row set (replaced previous defName {:?})",
            TerminalSafe::line(&previous.def_name).to_string()
        ),
        None => println!("row set"),
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ReplacedJson {
    replaced_def_name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ClearRowArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// Which section (by def type) to address — optional when the
    /// project has exactly one section.
    #[arg(long)]
    section: Option<String>,
    /// The target def, `<def_type>/<def_name>` — a target-keyed row.
    /// Requires `--key-field`; mutually exclusive with `--def-name`.
    #[arg(long, requires = "key_field", conflicts_with = "def_name")]
    target: Option<String>,
    /// The `FieldRole::TargetKey` field this target is matched through.
    #[arg(long = "key-field", requires = "target", conflicts_with = "def_name")]
    key_field: Option<String>,
    /// A free-standing row's own `defName`. Mutually exclusive with
    /// `--target`/`--key-field`.
    #[arg(long = "def-name", conflicts_with_all = ["target", "key_field"])]
    def_name: Option<String>,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_clear_row(args: &ClearRowArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let key = match (&args.target, &args.key_field, &args.def_name) {
        (Some(target), Some(key_field), None) => RowKey::Target(TargetRef {
            key_field: parse_field_path(key_field)?,
            def: parse_def_key(target)?,
        }),
        (None, None, Some(def_name)) => RowKey::Own(def_name.clone()),
        _ => bail!(
            "specify either --target/--key-field (a target-keyed row) or --def-name (a free-standing row)"
        ),
    };
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let project = session
        .assignment(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))?;
    let def_type = resolve_section_type(&project, args.section.as_deref())?;

    let use_case = ClearAssignmentRow::new(rim_io::JsonAssignmentProjectStore::new());
    let cleared = match use_case.execute(&mut session, &id, &def_type, &key) {
        Ok(cleared) => cleared,
        Err(error @ rim_session::use_cases::ClearAssignmentRowError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("clearing the row"),
    };

    if args.json {
        let json = ReplacedJson {
            replaced_def_name: cleared.map(|row| row.def_name),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing the outcome")?;
        println!("{text}");
        return Ok(());
    }
    match cleared {
        Some(row) => println!(
            "cleared row (was defName {:?})",
            TerminalSafe::line(&row.def_name).to_string()
        ),
        None => println!("no row was set for this key"),
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `assign add-section` / `assign remove-section`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct CopyFromArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// Which section (by def type) to address — optional when the
    /// project has exactly one section.
    #[arg(long)]
    section: Option<String>,
    /// The target def a fresh row is built for, `<def_type>/<def_name>`.
    #[arg(long)]
    target: String,
    /// The `FieldRole::TargetKey` field this target is matched through.
    #[arg(long = "key-field")]
    key_field: String,
    /// An existing instance of this project's own assignment def type to
    /// copy fields from, by its own `defName`.
    #[arg(long = "source")]
    source_def_name: String,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_copy_from(args: &CopyFromArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let target = TargetRef {
        key_field: parse_field_path(&args.key_field)?,
        def: parse_def_key(&args.target)?,
    };
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let project = session
        .assignment(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))?;
    let section = resolve_section(&project, args.section.as_deref())?;
    if section.is_standalone() {
        bail!(
            "copy-from builds a target-keyed row, but section {:?} is free-standing (no TargetKey field to match {} through)",
            section.schema.def_type,
            args.target
        );
    }
    let def_type = section.schema.def_type.clone();
    // `find_by_name` (rim-session) is the one place that locates a named
    // instance out of `AssignmentInstances`' own defName-less flattened
    // result — see that method's own doc comment for why this CLI
    // has no business recomputing that ordering itself.
    let (_, source_instance) = AssignmentInstances::new(rim_io::FileDefSourceReader::new())
        .find_by_name(&mut session, &def_type, &args.source_def_name)
        .map_err(|error| anyhow::anyhow!("reading instances: {error}"))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no active instance named {:?} of type {def_type}",
                args.source_def_name
            )
        })?;

    let outcome = CopyFrom::execute(&project, &def_type, &target, &source_instance, &session)
        .map_err(|error| anyhow::anyhow!("building the copied row: {error}"))?;
    let row = outcome.row;

    if args.json {
        let json = CopyFromRowJson {
            row: RowJson {
                def_type: target.def.def_type.clone(),
                def_name: target.def.def_name.clone(),
                key_field: target.key_field.to_string(),
                row_def_name: row.def_name.clone(),
            },
            dropped: outcome
                .dropped
                .iter()
                .map(|d| DroppedItemSlotValueJson {
                    path: d.path.to_string(),
                    def_type: d.def_type.clone(),
                    name: d.name.clone(),
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing the row")?;
        println!("{text}");
        return Ok(());
    }
    println!(
        "built row: defName={} values={}",
        TerminalSafe::line(&row.def_name),
        row.values.len()
    );
    if !outcome.dropped.is_empty() {
        println!(
            "  {} borrowed value(s) skipped because those defs are not active:",
            outcome.dropped.len()
        );
        for dropped in &outcome.dropped {
            println!(
                "    {}: {} {:?}",
                TerminalSafe::line(&dropped.path),
                TerminalSafe::line(&dropped.def_type),
                TerminalSafe::line(&dropped.name).to_string()
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `assign delete`
// ---------------------------------------------------------------------
