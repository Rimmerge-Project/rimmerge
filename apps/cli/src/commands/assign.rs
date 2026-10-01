//! `rimmerge assign`: the patch maker's CLI surface. Wraps `rim-session`'s
//! assignment use cases —
//! `propose`/`create`/`list`/`show`/`update`/`set-row`/`clear-row`/
//! `add-section`/`remove-section`/`delete`/`items`/`copy-from` — the same
//! thin, no-business-logic shape as `apps/cli/src/commands/patch.rs`. Row
//! editing here is deliberately minimal (there is no CLI row editor —
//! rows are meant to be edited in the UI or
//! by hand in `assignments/<id>.json`); `set-row`/`clear-row` exist only
//! so the end-to-end flow (propose -> create -> set-row -> show -> ...)
//! is exercisable without a GUI, not as a full editing surface.
//!
//! **`assign create` has no separate "confirm a wizard-shown schema"
//! step** — a CLI has no interactive table to show one in. It re-runs
//! [`CreateAssignment::propose`] itself (the same effective-refs/
//! candidate inference `assign propose` prints), then persists whichever
//! candidate's *unedited* inferred schema matches `--def-type` via
//! [`CreateAssignment::execute`](rim_session::use_cases::CreateAssignment::execute).
//! Reclassifying a field is not a CLI
//! feature (this crate's own convention: no domain `Serialize` derive, so
//! there is no natural way to round-trip a schema through a flag without
//! inventing one) — that stays a UI job (the wizard's schema table).
//!
//! **Multi-section projects**: a project can carry more
//! than one [`Section`], one per def type it assigns into. Every
//! subcommand that addresses one section (`set-row`/`clear-row`/
//! `coverage`/`copy-from`) takes an optional `--section <def-type>`,
//! resolved by [`resolve_section`] — required only once a project has
//! more than one section. `add-section`/`remove-section` add and remove
//! sections themselves. `set-row`/`clear-row` address a free-standing
//! row directly by omitting `--target`/`--key-field`.

use std::collections::BTreeMap;

use anyhow::bail;
use clap::Subcommand;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, Cardinality, DefKey, FieldPath, FieldRole, RowKey, RowValue,
    Section,
};

use coverage::run_coverage;
use export::run_export;
use items::run_items;
use project::{run_create, run_delete, run_list, run_show, run_update};
use propose::run_propose;
use rows::{run_clear_row, run_copy_from, run_set_row};
use sections::{run_add_section, run_remove_section};

mod coverage;
mod export;
mod items;
mod project;
mod propose;
mod rows;
mod sections;

pub use coverage::*;
pub use export::*;
pub use items::*;
pub use project::*;
pub use propose::*;
pub use rows::*;
pub use sections::*;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "assign/assign_tests.rs"]
mod tests;

/// Resolves which section a subcommand addresses: `requested` names a
/// def type explicitly (`--section`); `None` falls back to the project's
/// own single section, erroring rather than guessing once it has more
/// than one.
fn resolve_section<'a>(
    project: &'a AssignmentProject,
    requested: Option<&str>,
) -> anyhow::Result<&'a Section> {
    if let Some(def_type) = requested {
        return project.section(def_type).ok_or_else(|| {
            anyhow::anyhow!(
                "no section for def type {def_type:?}; this project has sections: {}",
                join_def_types(project.sections().keys())
            )
        });
    }
    let mut sections = project.sections().values();
    match (sections.next(), sections.next()) {
        (None, _) => bail!("this assignment project has no sections"),
        (Some(only), None) => Ok(only),
        (Some(_), Some(_)) => bail!(
            "this assignment project has {} sections ({}); pass --section <def-type>",
            project.sections().len(),
            join_def_types(project.sections().keys())
        ),
    }
}

/// [`resolve_section`], returning just the resolved section's own def
/// type — what every `--section`-taking use-case call needs.
fn resolve_section_type(
    project: &AssignmentProject,
    requested: Option<&str>,
) -> anyhow::Result<String> {
    resolve_section(project, requested).map(|section| section.schema.def_type.clone())
}

/// A [`RowKey`] the way a human reads it: a target-keyed row by its
/// target def (`ThingDef/Race0`), a free-standing row by its own
/// `defName` — never the domain `Debug`, the same convention
/// `format_role`/`format_row_intent` already follow.
fn format_row_key(key: &RowKey) -> String {
    match key {
        RowKey::Target(target) => target.def.to_string(),
        RowKey::Own(name) => name.clone(),
    }
}

/// `rimmerge assign`'s own subcommands.
#[derive(Debug, Subcommand)]
pub enum AssignCommand {
    /// Infers every assignment-def candidate for a reference/target
    /// selection, without persisting anything.
    Propose(ProposeArgs),
    /// Re-proposes candidates and persists the one matching `--def-type`.
    Create(CreateArgs),
    /// Lists every assignment project on this profile.
    List(ListArgs),
    /// Prints one assignment project's identity, scope, and schema.
    Show(ShowArgs),
    /// Applies a name/author/description/R/T/schema-refresh update.
    Update(UpdateArgs),
    /// Records one section's row — target-keyed (`--target`/
    /// `--key-field`) or free-standing (neither).
    SetRow(SetRowArgs),
    /// Removes one section's row.
    ClearRow(ClearRowArgs),
    /// Infers and adds a new section to an existing assignment project.
    AddSection(AddSectionArgs),
    /// Removes a section from an assignment project.
    RemoveSection(RemoveSectionArgs),
    /// Lists defs of one item type across the active list.
    Items(ItemsArgs),
    /// Copies an existing instance's fields into a fresh row.
    CopyFrom(CopyFromArgs),
    /// Deletes an assignment project.
    Delete(DeleteArgs),
    /// Prints the coverage work queue: every candidate target, what
    /// already references it, and (under a known rule) who would win.
    Coverage(CoverageArgs),
    /// Renders and writes the assignment into a user-chosen folder,
    /// optionally installing it into the game's `Mods/` folder.
    Export(ExportArgs),
}

/// Dispatches to the subcommand's own `run_*` function.
pub fn run(command: &AssignCommand) -> anyhow::Result<()> {
    match command {
        AssignCommand::Propose(args) => run_propose(args),
        AssignCommand::Create(args) => run_create(args),
        AssignCommand::List(args) => run_list(args),
        AssignCommand::Show(args) => run_show(args),
        AssignCommand::Update(args) => run_update(args),
        AssignCommand::SetRow(args) => run_set_row(args),
        AssignCommand::ClearRow(args) => run_clear_row(args),
        AssignCommand::AddSection(args) => run_add_section(args),
        AssignCommand::RemoveSection(args) => run_remove_section(args),
        AssignCommand::Items(args) => run_items(args),
        AssignCommand::CopyFrom(args) => run_copy_from(args),
        AssignCommand::Delete(args) => run_delete(args),
        AssignCommand::Coverage(args) => run_coverage(args),
        AssignCommand::Export(args) => run_export(args),
    }
}

fn parse_assignment_id(text: &str) -> anyhow::Result<AssignmentId> {
    text.parse()
        .map_err(|error| anyhow::anyhow!("--assignment {text:?}: {error}"))
}

fn parse_mod_set(csv: &str) -> std::collections::BTreeSet<ModId> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ModId::new)
        .collect()
}

fn join_ids<'a>(ids: impl IntoIterator<Item = &'a ModId>) -> String {
    ids.into_iter()
        .map(ModId::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

/// A project's section def types, comma-joined in `BTreeMap` order — used
/// by `resolve_section`'s own error messages and `assign list`.
fn join_def_types<'a>(types: impl IntoIterator<Item = &'a String>) -> String {
    types.into_iter().cloned().collect::<Vec<_>>().join(",")
}

/// `<def_type>/<def_name>`, the same shape [`DefKey`]'s own `Display`
/// prints — used by `--target`/`--source` flags.
fn parse_def_key(text: &str) -> anyhow::Result<DefKey> {
    let (def_type, def_name) = text
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("{text:?}: expected <def_type>/<def_name>"))?;
    Ok(DefKey {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
    })
}

fn parse_field_path(text: &str) -> anyhow::Result<FieldPath> {
    text.parse()
        .map_err(|error| anyhow::anyhow!("--key-field {text:?}: {error}"))
}

/// `names:a,b,c` | `numbers:1.0,2.0` | `text:...` | `omit` — the same
/// tagged-spec convention `patch decide`'s own `--choice` flag uses.
fn parse_row_value(spec: &str) -> anyhow::Result<RowValue> {
    if spec == "omit" {
        return Ok(RowValue::Omit);
    }
    if let Some(rest) = spec.strip_prefix("names:") {
        return Ok(RowValue::Names(
            rest.split(',').map(str::to_string).collect(),
        ));
    }
    if let Some(rest) = spec.strip_prefix("numbers:") {
        let numbers = rest
            .split(',')
            .map(|n| {
                n.parse::<f64>()
                    .map_err(|error| anyhow::anyhow!("--value {spec:?}: {error}"))
            })
            .collect::<anyhow::Result<Vec<f64>>>()?;
        return Ok(RowValue::Numbers(numbers));
    }
    if let Some(rest) = spec.strip_prefix("text:") {
        return Ok(RowValue::Text(rest.to_string()));
    }
    bail!(
        "--value {spec:?}: spec must be one of names:<a,b,...>, numbers:<n,...>, text:<...>, omit"
    );
}

fn parse_row_values(raw: &[String]) -> anyhow::Result<BTreeMap<FieldPath, RowValue>> {
    let mut values = BTreeMap::new();
    for entry in raw {
        let (path_text, spec) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--value {entry:?}: expected <path>=<spec>"))?;
        let path = parse_field_path(path_text)?;
        values.insert(path, parse_row_value(spec)?);
    }
    Ok(values)
}

fn format_role(role: &FieldRole) -> String {
    match role {
        FieldRole::TargetKey { def_type } => format!("TargetKey({def_type})"),
        FieldRole::ItemSlot { def_type } => format!("ItemSlot({def_type})"),
        FieldRole::Chances { for_slot } => format!("Chances(for {for_slot})"),
        FieldRole::Scalar { kind, default } => {
            let default = default.as_deref().unwrap_or("-");
            format!("Scalar({kind}, default={default})")
        }
        FieldRole::Opaque => "Opaque".to_string(),
    }
}

fn format_cardinality(cardinality: Cardinality) -> &'static str {
    match cardinality {
        Cardinality::Scalar => "scalar",
        Cardinality::List => "list",
    }
}

// ---------------------------------------------------------------------
// `assign propose`
// ---------------------------------------------------------------------
