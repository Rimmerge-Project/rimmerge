//! `assign create`/`list`/`show`/`update`/`delete`: the project lifecycle and its text/JSON renderings.

use anyhow::{Context, bail};
use clap::Args;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{AssignmentProject, AssignmentRow, RowKey, Section, TargetRef};
use rim_session::use_cases::{
    CreateAssignment, CreateAssignmentInput, DeleteAssignment, UpdateAssignment,
    UpdateAssignmentInput,
};
use serde::Serialize;

use super::propose::FieldSpecJson;
use super::{
    format_cardinality, format_role, join_def_types, join_ids, parse_assignment_id, parse_mod_set,
};
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct CreateArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The project's label in lists.
    #[arg(long)]
    name: String,
    /// The published mod's package id.
    #[arg(long = "package-id")]
    package_id: String,
    /// The published mod's display name. Defaults to `--name` when
    /// omitted.
    #[arg(long = "display-name")]
    display_name: Option<String>,
    /// The selected reference set R, comma-separated package ids.
    #[arg(long)]
    refs: String,
    /// Closure members of R to opt out of, comma-separated.
    #[arg(long = "exclude-refs", default_value = "")]
    exclude_refs: String,
    /// The target set T, comma-separated package ids. May be empty only
    /// for a "new def" candidate — one with no `TargetKey` field.
    #[arg(long, default_value = "")]
    targets: String,
    /// Which candidate def type (from `assign propose`'s own phase-1
    /// listing) to infer and persist.
    #[arg(long = "def-type")]
    def_type: String,
}

pub(super) fn run_create(args: &CreateArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    let refs = parse_mod_set(&args.refs);
    let excluded_refs = parse_mod_set(&args.exclude_refs);
    let targets = parse_mod_set(&args.targets);

    let use_case = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    let candidate = use_case
        .infer_candidate(
            &mut session,
            &refs,
            &excluded_refs,
            &targets,
            &args.def_type,
        )
        .map_err(|error| anyhow::anyhow!("inferring candidate {:?}: {error}", args.def_type))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no candidate def type {:?} for this reference/target selection",
                args.def_type
            )
        })?;

    let id = use_case
        .execute(
            &mut session,
            CreateAssignmentInput {
                name: args.name.clone(),
                package_id: args.package_id.clone(),
                display_name: args
                    .display_name
                    .clone()
                    .unwrap_or_else(|| args.name.clone()),
                refs,
                excluded_refs,
                targets,
                schema: candidate.schema,
            },
        )
        .context("creating the assignment")?;
    println!("created assignment {id}");
    Ok(())
}

// ---------------------------------------------------------------------
// `assign list`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    paths: PathsArgs,
}

pub(super) fn run_list(args: &ListArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let session = build_session(paths)?;

    let projects: Vec<_> = session.assignments().collect();
    if projects.is_empty() {
        println!("no assignments on this profile");
        return Ok(());
    }
    for project in projects {
        let total_rows: usize = project.sections().values().map(|s| s.rows.len()).sum();
        println!(
            "{:<14} {:<20} {:<28} sections=[{}] refs=[{}] targets=[{}] rows={}",
            project.id(),
            TerminalSafe::line(project.name()).to_string(),
            TerminalSafe::line(project.identity().package_id()).to_string(),
            TerminalSafe::line(join_def_types(project.sections().keys())),
            TerminalSafe::line(join_ids(project.refs())),
            TerminalSafe::line(join_ids(project.targets())),
            total_rows
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `assign show`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_show(args: &ShowArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let session = build_session(paths)?;

    let project = session
        .assignment(&id)
        .ok_or_else(|| anyhow::anyhow!("assignment {id} not found"))?;

    if args.json {
        return print_show_json(project);
    }

    println!("id:          {}", project.id());
    println!("name:        {}", TerminalSafe::line(project.name()));
    println!(
        "package id:  {}",
        TerminalSafe::line(project.identity().package_id())
    );
    println!(
        "display:     {}",
        TerminalSafe::line(project.identity().display_name())
    );
    println!("author:      {}", TerminalSafe::line(project.author()));
    println!(
        "refs:        {}",
        TerminalSafe::line(join_ids(project.refs()))
    );
    println!(
        "excl. refs:  {}",
        TerminalSafe::line(join_ids(project.excluded_refs()))
    );
    println!(
        "targets:     {}",
        TerminalSafe::line(join_ids(project.targets()))
    );
    println!(
        "export dir:  {}",
        project
            .export_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!();
    println!(
        "sections ({}): {}",
        project.sections().len(),
        TerminalSafe::line(join_def_types(project.sections().keys()))
    );
    for (def_type, section) in project.sections() {
        println!();
        print_section_text(def_type, section);
    }
    Ok(())
}

/// `run_show`'s per-section block: the def type and target-keyed/
/// free-standing marker, the field table, then either its standalone
/// rows or its target-keyed rows — split out so `run_show` itself stays
/// one abstraction level (loop-over-sections) rather than mixing that
/// with a section's own field/row formatting.
fn print_section_text(def_type: &str, section: &Section) {
    println!(
        "section: {} ({})",
        TerminalSafe::line(def_type),
        if section.is_standalone() {
            "free-standing"
        } else {
            "target-keyed"
        }
    );
    println!(
        "{:<28} {:<10} {:>7} role",
        "field", "cardinality", "observed"
    );
    for (path, spec) in &section.schema.fields {
        println!(
            "{:<28} {:<10} {:>3}/{:<3} {}",
            TerminalSafe::line(path).to_string(),
            format_cardinality(spec.cardinality),
            spec.observed.0,
            spec.observed.1,
            TerminalSafe::line(format_role(&spec.role))
        );
    }
    println!();
    if section.is_standalone() {
        let own_names = section.own_instance_names();
        println!("standalone rows ({}):", own_names.len());
        for def_name in &own_names {
            println!("  defName={}", TerminalSafe::line(def_name));
        }
    } else {
        let target_rows: Vec<(&TargetRef, &AssignmentRow)> = section
            .rows
            .iter()
            .filter_map(|(key, row)| match key {
                RowKey::Target(target) => Some((target, row)),
                RowKey::Own(_) => None,
            })
            .collect();
        println!("rows ({}):", target_rows.len());
        for (target, row) in target_rows {
            println!(
                "  {} (key={})  defName={}",
                TerminalSafe::line(&target.def),
                TerminalSafe::line(&target.key_field),
                TerminalSafe::line(&row.def_name)
            );
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct RowJson {
    pub(super) def_type: String,
    pub(super) def_name: String,
    pub(super) key_field: String,
    pub(super) row_def_name: String,
}

/// `copy-from --json`'s own row shape: [`RowJson`] plus the values it had
/// to drop — kept separate from [`RowJson`] itself, which `assign show`
/// also builds for a row that was never copied and so never has anything
/// to report dropping.
#[derive(Debug, Serialize)]
pub(super) struct CopyFromRowJson {
    #[serde(flatten)]
    pub(super) row: RowJson,
    pub(super) dropped: Vec<DroppedItemSlotValueJson>,
}

/// One [`rim_session::use_cases::DroppedItemSlotValue`], transcribed for
/// `--json` — never derive `Serialize` on a `rim-session`/`rim-resolve`
/// domain type directly, per this file's own established convention.
#[derive(Debug, Serialize)]
pub(super) struct DroppedItemSlotValueJson {
    pub(super) path: String,
    pub(super) def_type: String,
    pub(super) name: String,
}

#[derive(Debug, Serialize)]
struct SectionJson {
    def_type: String,
    is_standalone: bool,
    fields: Vec<FieldSpecJson>,
    rows: Vec<RowJson>,
    standalone_rows: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ShowJson {
    id: String,
    name: String,
    package_id: String,
    display_name: String,
    author: String,
    refs: Vec<String>,
    excluded_refs: Vec<String>,
    targets: Vec<String>,
    export_dir: Option<String>,
    sections: Vec<SectionJson>,
}

fn section_json(def_type: &str, section: &Section) -> SectionJson {
    SectionJson {
        def_type: def_type.to_string(),
        is_standalone: section.is_standalone(),
        fields: section
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
                required_children: section
                    .schema
                    .target_shapes
                    .get(path)
                    .map(|shape| shape.required_children.iter().cloned().collect())
                    .unwrap_or_default(),
            })
            .collect(),
        rows: section
            .rows
            .iter()
            .filter_map(|(key, row)| match key {
                RowKey::Target(target) => Some(RowJson {
                    def_type: target.def.def_type.clone(),
                    def_name: target.def.def_name.clone(),
                    key_field: target.key_field.to_string(),
                    row_def_name: row.def_name.clone(),
                }),
                RowKey::Own(_) => None,
            })
            .collect(),
        standalone_rows: section.own_instance_names().into_iter().collect(),
    }
}

fn print_show_json(project: &AssignmentProject) -> anyhow::Result<()> {
    let json = ShowJson {
        id: project.id().to_string(),
        name: project.name().to_string(),
        package_id: project.identity().package_id().to_string(),
        display_name: project.identity().display_name().to_string(),
        author: project.author().to_string(),
        refs: project.refs().iter().map(ModId::to_string).collect(),
        excluded_refs: project
            .excluded_refs()
            .iter()
            .map(ModId::to_string)
            .collect(),
        targets: project.targets().iter().map(ModId::to_string).collect(),
        export_dir: project.export_dir().map(|path| path.display().to_string()),
        sections: project
            .sections()
            .iter()
            .map(|(def_type, section)| section_json(def_type, section))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).context("serializing the assignment as JSON")?;
    println!("{text}");
    Ok(())
}

// ---------------------------------------------------------------------
// `assign update`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct UpdateArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// A new list label.
    #[arg(long)]
    name: Option<String>,
    /// A new `About.xml` author.
    #[arg(long)]
    author: Option<String>,
    /// A new `About.xml` description.
    #[arg(long)]
    description: Option<String>,
    /// A new selected reference set, comma-separated. Re-infers the
    /// schema against the new effective set when given (alone or with
    /// `--exclude-refs`).
    #[arg(long)]
    refs: Option<String>,
    /// A new set of closure members opted out of, comma-separated.
    #[arg(long = "exclude-refs")]
    exclude_refs: Option<String>,
    /// A new target set, comma-separated. Drops any row whose target left
    /// it.
    #[arg(long)]
    targets: Option<String>,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(super) fn run_update(args: &UpdateArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let input = UpdateAssignmentInput {
        name: args.name.clone(),
        author: args.author.clone(),
        description: args.description.clone(),
        refs: args.refs.as_deref().map(parse_mod_set),
        excluded_refs: args.exclude_refs.as_deref().map(parse_mod_set),
        targets: args.targets.as_deref().map(parse_mod_set),
    };
    let use_case = UpdateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    let outcome = match use_case.execute(&mut session, &id, input) {
        Ok(outcome) => outcome,
        Err(error @ rim_session::use_cases::UpdateAssignmentError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("updating the assignment"),
    };

    if args.json {
        let json = UpdateOutcomeJson {
            sections: outcome
                .schema_changes
                .iter()
                .map(|(def_type, change)| SectionSchemaChangeJson {
                    def_type: def_type.clone(),
                    added: change.added.iter().map(ToString::to_string).collect(),
                    removed: change.removed.iter().map(ToString::to_string).collect(),
                    reclassified: change
                        .reclassified
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                })
                .collect(),
            dropped_rows: outcome
                .dropped_rows
                .iter()
                .map(|(def_type, target)| format!("{def_type}: {}", target.def))
                .collect(),
        };
        let text = serde_json::to_string_pretty(&json).context("serializing the outcome")?;
        println!("{text}");
        return Ok(());
    }

    for (def_type, change) in &outcome.schema_changes {
        println!(
            "schema change ({}): added={} removed={} reclassified={}",
            TerminalSafe::line(def_type),
            change.added.len(),
            change.removed.len(),
            change.reclassified.len()
        );
        for path in &change.reclassified {
            println!("  reclassified: {}", TerminalSafe::line(path));
        }
    }
    if !outcome.dropped_rows.is_empty() {
        println!("dropped {} row(s):", outcome.dropped_rows.len());
        for (def_type, target) in &outcome.dropped_rows {
            println!(
                "  {}: {}",
                TerminalSafe::line(def_type),
                TerminalSafe::line(&target.def)
            );
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct UpdateOutcomeJson {
    sections: Vec<SectionSchemaChangeJson>,
    dropped_rows: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SectionSchemaChangeJson {
    def_type: String,
    added: Vec<String>,
    removed: Vec<String>,
    reclassified: Vec<String>,
}

// ---------------------------------------------------------------------
// `assign set-row` / `assign clear-row`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct DeleteArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
}

pub(super) fn run_delete(args: &DeleteArgs) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let use_case = DeleteAssignment::new(rim_io::JsonAssignmentProjectStore::new());
    match use_case.execute(&mut session, &id) {
        Ok(_) => {}
        Err(error @ rim_session::use_cases::DeleteAssignmentError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        Err(error) => return Err(error).context("deleting the assignment"),
    }
    println!("deleted assignment {id}");
    Ok(())
}

// ---------------------------------------------------------------------
// `assign coverage`
// ---------------------------------------------------------------------
