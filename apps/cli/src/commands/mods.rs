//! `rimmerge mods list|activate|deactivate`: lists the active/inactive/
//! missing mods and edits `ModsConfig.xml`'s own `<activeMods>` list
//! directly — the same "an explicit write a human asked for" rule
//! `apply` already follows (backup taken, refused while the game runs
//! unless `--force`).
//!
//! **Zero business logic here**: `list`/`activate`/`deactivate` all
//! go through `rim_io::discover_inventory` (discovery only — seconds, not
//! the full scan `apply`/`load` need) and `rim_session::{ActiveSet::new,
//! plan_activate, plan_deactivate}`, the exact same pure functions
//! the session's own activation use cases call. Because discovery never
//! reads `Report::edges`, a deactivation's own "dependents" warning here
//! only ever comes from declared `modDependencies` — never the `Hard`-
//! strength report edges a full scan would also see (see
//! `print_deactivate_plan_text`'s own `dependents (declared)` label).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Args, Subcommand, ValueEnum};
use rim_analyzer::domain::{ModId, Source};
use rim_io::GameProcessProbe;
use rim_resolve::sort::Tier;
use rim_session::mod_info::{HomepageLink, ModInfo, PendingChange};
use rim_session::ports::{ModsConfigFile, ModsConfigStore};
use rim_session::use_cases::{AboutOutcome, ReadModAbout, SelectOrder};
use rim_session::{
    ActivatePlan, ActiveSet, DeactivatePlan, ModInventory, ProjectPaths, plan_activate,
    plan_deactivate,
};
use serde::Serialize;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Subcommand)]
pub enum ModsCommand {
    /// Lists active, inactive, or every discovered mod.
    List(ListArgs),
    /// Shows one mod's full information: identity, `About.xml` details,
    /// and (for an active mod) its placement, cost, dependents, and
    /// live findings.
    Show(ShowArgs),
    /// Activates one or more inactive mods, appended at the end of the
    /// active list (dependencies first when `--with-dependencies` is
    /// given).
    Activate(ActivateArgs),
    /// Deactivates one or more active mods.
    Deactivate(DeactivateArgs),
}

pub fn run(command: &ModsCommand) -> anyhow::Result<()> {
    match command {
        ModsCommand::List(args) => run_list(args),
        ModsCommand::Show(args) => run_show(args),
        ModsCommand::Activate(args) => run_activate(args, &rim_io::SysinfoGameProcessProbe::new()),
        ModsCommand::Deactivate(args) => {
            run_deactivate(args, &rim_io::SysinfoGameProcessProbe::new())
        }
    }
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// List inactive mods instead of the active list.
    #[arg(long, conflicts_with = "all")]
    inactive: bool,
    /// List active, inactive, and missing mods together.
    #[arg(long, conflicts_with = "inactive")]
    all: bool,
    /// Emit the inventory as JSON instead of a text table.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub struct ActivateArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Mod ids to activate.
    #[arg(required = true)]
    ids: Vec<String>,
    /// Also activate each requested mod's own inactive dependency
    /// closure (dependencies first, RimWorld's own append-at-the-end
    /// placement for every id).
    #[arg(long)]
    with_dependencies: bool,
    /// Print the plan without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Write `ModsConfig.xml` even if `RimWorldWin64.exe` looks like it's
    /// running.
    #[arg(long)]
    force: bool,
    /// Print the plan as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub struct DeactivateArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Mod ids to deactivate.
    #[arg(required = true)]
    ids: Vec<String>,
    /// Deactivate even when a still-active mod declares one of the
    /// requested ids a dependency (without this, that's a refusal).
    #[arg(long)]
    yes: bool,
    /// Print the plan without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Write `ModsConfig.xml` even if `RimWorldWin64.exe` looks like it's
    /// running.
    #[arg(long)]
    force: bool,
    /// Print the plan as JSON.
    #[arg(long)]
    json: bool,
}

/// Reads `ModsConfig.xml` **once** (`version`/`knownExpansions`/
/// `active_mods`, via [`ModsConfigStore::read`]) and builds the
/// discovery-only inventory from that same snapshot's own active list —
/// shared by all three subcommands. Reading once matters: a second read
/// right before writing would leave a window for the running game to
/// rewrite `ModsConfig.xml` between the two — planning against one
/// snapshot and then writing `version`/`knownExpansions` from a
/// *different* one. [`rim_io::discover_inventory`]'s `active_override`
/// takes this same read's own `active_mods` instead of re-reading.
fn discover(
    paths_args: &PathsArgs,
) -> anyhow::Result<(ProjectPaths, ModsConfigFile, ModInventory)> {
    let project_paths = resolve_paths(paths_args)?;
    let file = rim_io::ModsConfigFileStore::new()
        .read(&project_paths.mods_config)
        .context("reading ModsConfig.xml")?;
    let (inventory, _active) = rim_io::discover_inventory(&project_paths, Some(&file.active_mods))
        .context(
            "discovering the mod inventory (no full scan — identity and declared dependencies \
             only)",
        )?;
    Ok((project_paths, file, inventory))
}

/// Writes `active`'s own list into `ModsConfig.xml`, preserving `file`'s
/// own `version`/`knownExpansions` — `file` is the exact read
/// [`discover`] already did, never a fresh one. Returns the backup path
/// `write_with_backup` created.
fn write_active_set(
    project_paths: &ProjectPaths,
    file: &ModsConfigFile,
    active: &ActiveSet,
) -> anyhow::Result<PathBuf> {
    let updated = ModsConfigFile {
        version: file.version.clone(),
        active_mods: active.ids().to_vec(),
        known_expansions: file.known_expansions.clone(),
    };
    rim_io::ModsConfigFileStore::new()
        .write_with_backup(&project_paths.mods_config, &updated)
        .context("writing ModsConfig.xml")
}

fn refuse_while_running(force: bool, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    if !force && probe.is_running() {
        bail!(
            "RimWorldWin64.exe is running; close the game or retry with --force (RimWorld \
             overwrites ModsConfig.xml on exit, so a concurrent write is likely to be lost or \
             corrupt the running game's own copy)"
        );
    }
    Ok(())
}

// -- list -----------------------------------------------------------------

struct ModRow {
    id: ModId,
    name: String,
    source: Option<Source>,
}

fn format_source(source: Option<Source>) -> String {
    source.map_or_else(|| "-".to_string(), |s| s.to_string())
}

fn print_row(row: &ModRow) {
    println!(
        "  {:<40} {:<10} {}",
        TerminalSafe::line(&row.id).to_string(),
        format_source(row.source),
        TerminalSafe::line(&row.name)
    );
}

fn print_numbered_row(index: usize, row: &ModRow) {
    println!(
        "{:>4}. {:<40} {:<10} {}",
        index + 1,
        TerminalSafe::line(&row.id).to_string(),
        format_source(row.source),
        TerminalSafe::line(&row.name)
    );
}

#[derive(Serialize)]
struct ModRowJson {
    id: String,
    name: String,
    source: Option<String>,
}

impl From<&ModRow> for ModRowJson {
    fn from(row: &ModRow) -> Self {
        Self {
            id: row.id.to_string(),
            name: row.name.clone(),
            source: row.source.map(|s| s.to_string()),
        }
    }
}

#[derive(Serialize)]
struct InventoryJson {
    active: Vec<ModRowJson>,
    inactive: Vec<ModRowJson>,
    missing: Vec<String>,
}

fn run_list(args: &ListArgs) -> anyhow::Result<()> {
    let (_paths, file, inventory) = discover(&args.paths)?;
    let active_ids = &file.active_mods;
    let active_set: BTreeSet<ModId> = active_ids.iter().cloned().collect();

    let active_rows: Vec<ModRow> = active_ids
        .iter()
        .map(|id| match inventory.entry(id) {
            Some(entry) => ModRow {
                id: id.clone(),
                name: entry.name.clone(),
                source: entry.source,
            },
            None => ModRow {
                id: id.clone(),
                name: id.to_string(),
                source: None,
            },
        })
        .collect();
    let inactive_rows: Vec<ModRow> = inventory
        .entries()
        .filter(|(id, entry)| entry.present_on_disk && !active_set.contains(id))
        .map(|(id, entry)| ModRow {
            id: id.clone(),
            name: entry.name.clone(),
            source: entry.source,
        })
        .collect();
    // Read straight off the inventory's own missing set, not
    // recomputed by walking `active_ids`: the two disagree the moment
    // `active_ids` repeats an id, since `missing_ids()` is backed by a
    // map that's already deduplicated.
    let missing_ids: Vec<ModId> = inventory.missing_ids().cloned().collect();

    if args.json {
        let json = InventoryJson {
            active: active_rows.iter().map(ModRowJson::from).collect(),
            inactive: inactive_rows.iter().map(ModRowJson::from).collect(),
            missing: missing_ids.iter().map(ToString::to_string).collect(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&json).context("serializing the inventory")?
        );
        return Ok(());
    }

    if args.all {
        println!("active ({}):", active_rows.len());
        for (index, row) in active_rows.iter().enumerate() {
            print_numbered_row(index, row);
        }
        println!("inactive ({}):", inactive_rows.len());
        for row in &inactive_rows {
            print_row(row);
        }
        println!("missing ({}):", missing_ids.len());
        for id in &missing_ids {
            println!("  {}", TerminalSafe::line(id));
        }
    } else if args.inactive {
        println!("inactive ({}):", inactive_rows.len());
        for row in &inactive_rows {
            print_row(row);
        }
    } else {
        println!("active ({}):", active_rows.len());
        for (index, row) in active_rows.iter().enumerate() {
            print_numbered_row(index, row);
        }
    }
    Ok(())
}

// -- show -------------------------------------------------------------------

/// Mirrors [`rim_resolve::domain::OrderSource`] for `clap`'s `ValueEnum`
/// derive, the same shape `defs inspect`'s own private `SourceArg` uses.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ShowSourceArg {
    Current,
    Suggested,
}

impl From<ShowSourceArg> for rim_resolve::domain::OrderSource {
    fn from(value: ShowSourceArg) -> Self {
        match value {
            ShowSourceArg::Current => Self::Current,
            ShowSourceArg::Suggested => Self::Suggested,
        }
    }
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The mod id to show.
    id: String,
    /// Which order to report an active mod's position under.
    #[arg(long, value_enum, default_value_t = ShowSourceArg::Current)]
    source: ShowSourceArg,
    /// Emit the mod's info as JSON instead of text.
    #[arg(long)]
    json: bool,
}

fn format_tier(tier: Tier) -> &'static str {
    match tier {
        Tier::Core => "core",
        Tier::Dlc => "dlc",
        Tier::Top => "top",
        Tier::Body => "body",
        Tier::Bottom => "bottom",
    }
}

fn format_pending(pending: Option<PendingChange>) -> Option<&'static str> {
    match pending {
        Some(PendingChange::ActivationPending) => Some("activation pending"),
        Some(PendingChange::DeactivationPending) => Some("deactivation pending"),
        None => None,
    }
}

/// Renders a description's sanitized runs as plain concatenated text —
/// there is no terminal rich-text rendering here, only Unity's bold/
/// italic markup stripped back to the text it wrapped (the same text a
/// `<b>`/`<i>`-blind reader already sees today).
fn format_description(outcome: &AboutOutcome) -> Option<String> {
    let text = match outcome {
        AboutOutcome::Read { description, .. } => {
            let description = description.as_ref()?;
            let mut text: String = description
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect();
            if description.truncated {
                text.push_str(" [truncated]");
            }
            text
        }
        // Disclosed, not silently dropped — a mod that changed folders
        // or has a broken `About.xml` still shows a reason, rather than
        // a description line that just isn't there with nothing to
        // explain why.
        AboutOutcome::Changed => {
            "description unavailable: the mod's folder changed since the last scan".to_string()
        }
        AboutOutcome::Unreadable(reason) => format!("description unavailable: {reason}"),
        AboutOutcome::NotOnDisk => return None,
    };
    Some(TerminalSafe::block(&text).to_string())
}

fn format_homepage(homepage: Option<&HomepageLink>) -> Option<String> {
    match homepage {
        Some(HomepageLink::Openable(url)) => Some(url.as_str().to_string()),
        Some(HomepageLink::Text(text)) => Some(text.clone()),
        None => None,
    }
}

/// An active mod's own scan-time `homepage` is preferred; an inactive
/// mod carries none of its own, so its homepage only ever comes from the
/// lazily re-read `about.homepage` — the same precedence
/// `ModInfoWithAbout::link_url` uses internally, mirrored here because
/// that method only ever returns the *openable* half (it exists to feed
/// the opener use case), and `show`'s own text/JSON output also wants
/// the non-openable [`HomepageLink::Text`] case, which `link_url` throws
/// away by design.
fn resolve_homepage(with_about: &rim_session::use_cases::ModInfoWithAbout) -> Option<HomepageLink> {
    match &with_about.info {
        ModInfo::Active(active) => active.homepage.clone(),
        ModInfo::Inactive(_) => match &with_about.about {
            AboutOutcome::Read { homepage, .. } => homepage.clone(),
            AboutOutcome::Changed | AboutOutcome::Unreadable(_) | AboutOutcome::NotOnDisk => None,
        },
        ModInfo::Missing(_) => None,
    }
}

#[derive(Serialize)]
struct ShowJson {
    #[serde(rename = "kind")]
    kind: &'static str,
    #[serde(rename = "modId")]
    mod_id: String,
    name: Option<String>,
    authors: Vec<String>,
    source: Option<String>,
    homepage: Option<String>,
    #[serde(rename = "modVersion")]
    mod_version: Option<String>,
    description: Option<String>,
    root: Option<String>,
    tags: Vec<String>,
    position: Option<usize>,
    tier: Option<&'static str>,
    #[serde(rename = "hardDependents")]
    hard_dependents: Option<usize>,
    #[serde(rename = "softDependents")]
    soft_dependents: Option<usize>,
    #[serde(rename = "awarenessDependents")]
    awareness_dependents: Option<usize>,
    #[serde(rename = "findingsTotal")]
    findings_total: Option<usize>,
    #[serde(rename = "needsInputCount")]
    needs_input_count: Option<usize>,
    pending: Option<&'static str>,
    #[serde(rename = "requiredBy")]
    required_by: Vec<String>,
}

fn run_show(args: &ShowArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    SelectOrder::new().execute(&mut session, args.source.into());

    let use_case = ReadModAbout::new(rim_io::FileModAboutReader::new());
    let id = ModId::new(&args.id);
    let with_about = use_case
        .execute(&mut session, &id, args.source.into())
        .map_err(|_unknown| anyhow::anyhow!("{} not found: not a known mod", args.id))?;

    let homepage = resolve_homepage(&with_about);
    let description = format_description(&with_about.about);
    let mod_version = match &with_about.about {
        AboutOutcome::Read { mod_version, .. } => mod_version.clone(),
        AboutOutcome::Changed | AboutOutcome::Unreadable(_) | AboutOutcome::NotOnDisk => None,
    };

    if args.json {
        let json = show_json(
            &with_about.info,
            homepage.as_ref(),
            mod_version,
            description,
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&json).context("serializing the mod info")?
        );
        return Ok(());
    }
    print_show_text(
        &with_about.info,
        homepage.as_ref(),
        mod_version.as_deref(),
        description.as_deref(),
    );
    Ok(())
}

fn show_json(
    info: &ModInfo,
    homepage: Option<&HomepageLink>,
    mod_version: Option<String>,
    description: Option<String>,
) -> ShowJson {
    match info {
        ModInfo::Active(active) => ShowJson {
            kind: "active",
            mod_id: active.mod_id.to_string(),
            name: Some(active.name.clone()),
            authors: active.authors.clone(),
            source: Some(active.source.to_string()),
            homepage: format_homepage(homepage),
            mod_version,
            description,
            root: Some(active.root.display().to_string()),
            tags: active.tags.iter().map(ToString::to_string).collect(),
            position: Some(active.position),
            tier: Some(format_tier(active.tier)),
            hard_dependents: Some(active.hard_dependents),
            soft_dependents: Some(active.soft_dependents),
            awareness_dependents: Some(active.awareness_dependents),
            findings_total: Some(active.findings_total),
            needs_input_count: Some(active.needs_input_count),
            pending: format_pending(active.pending),
            required_by: Vec::new(),
        },
        ModInfo::Inactive(inactive) => ShowJson {
            kind: "inactive",
            mod_id: inactive.mod_id.to_string(),
            name: Some(inactive.name.clone()),
            authors: inactive.authors.clone(),
            source: Some(inactive.source.to_string()),
            homepage: format_homepage(homepage),
            mod_version,
            description,
            root: Some(inactive.root.display().to_string()),
            tags: Vec::new(),
            position: None,
            tier: None,
            hard_dependents: None,
            soft_dependents: None,
            awareness_dependents: None,
            findings_total: None,
            needs_input_count: None,
            pending: format_pending(inactive.pending),
            required_by: Vec::new(),
        },
        ModInfo::Missing(missing) => ShowJson {
            kind: "missing",
            mod_id: missing.mod_id.to_string(),
            name: None,
            authors: Vec::new(),
            source: None,
            homepage: None,
            mod_version: None,
            description: None,
            root: None,
            tags: Vec::new(),
            position: None,
            tier: None,
            hard_dependents: None,
            soft_dependents: None,
            awareness_dependents: None,
            findings_total: None,
            needs_input_count: None,
            pending: None,
            required_by: missing
                .required_by
                .iter()
                .map(ToString::to_string)
                .collect(),
        },
    }
}

fn print_show_text(
    info: &ModInfo,
    homepage: Option<&HomepageLink>,
    mod_version: Option<&str>,
    description: Option<&str>,
) {
    match info {
        ModInfo::Active(active) => {
            println!(
                "{} ({})",
                TerminalSafe::line(&active.name),
                TerminalSafe::line(&active.mod_id)
            );
            println!("  status: active");
            println!("  source: {}", active.source);
            if !active.authors.is_empty() {
                println!(
                    "  authors: {}",
                    TerminalSafe::line(active.authors.join(", "))
                );
            }
            if let Some(version) = mod_version {
                println!("  version: {}", TerminalSafe::line(version));
            }
            if let Some(homepage) = format_homepage(homepage) {
                println!("  homepage: {}", TerminalSafe::line(homepage));
            }
            println!("  root: {}", TerminalSafe::line(active.root.display()));
            println!(
                "  position: #{} ({})",
                active.position + 1,
                format_tier(active.tier)
            );
            if let Some(pending) = format_pending(active.pending) {
                println!("  pending: {pending}");
            }
            if !active.tags.is_empty() {
                let tags = active
                    .tags
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                println!("  tags: {}", TerminalSafe::line(tags.join(", ")));
            }
            println!(
                "  dependents: hard {}, soft {}, awareness {}",
                active.hard_dependents, active.soft_dependents, active.awareness_dependents
            );
            println!(
                "  findings: {} ({} need input)",
                active.findings_total, active.needs_input_count
            );
            if let Some(description) = description {
                println!("  description: {description}");
            }
        }
        ModInfo::Inactive(inactive) => {
            println!(
                "{} ({})",
                TerminalSafe::line(&inactive.name),
                TerminalSafe::line(&inactive.mod_id)
            );
            println!("  status: inactive");
            println!("  source: {}", inactive.source);
            if !inactive.authors.is_empty() {
                println!(
                    "  authors: {}",
                    TerminalSafe::line(inactive.authors.join(", "))
                );
            }
            if let Some(version) = mod_version {
                println!("  version: {}", TerminalSafe::line(version));
            }
            if let Some(homepage) = format_homepage(homepage) {
                println!("  homepage: {}", TerminalSafe::line(homepage));
            }
            println!("  root: {}", TerminalSafe::line(inactive.root.display()));
            if let Some(pending) = format_pending(inactive.pending) {
                println!("  pending: {pending}");
            }
            if let Some(description) = description {
                println!("  description: {description}");
            }
        }
        ModInfo::Missing(missing) => {
            println!("{}", TerminalSafe::line(&missing.mod_id));
            println!("  status: missing (active in ModsConfig.xml, not found on disk)");
            if !missing.required_by.is_empty() {
                let required_by = missing
                    .required_by
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                println!(
                    "  required by: {}",
                    TerminalSafe::line(required_by.join(", "))
                );
            }
        }
    }
}

// -- activate ---------------------------------------------------------------

#[derive(Serialize)]
struct ActivatePlanJson {
    will_activate: Vec<String>,
    dependencies_also_activated: Vec<String>,
    unresolvable_dependencies: BTreeMap<String, Vec<String>>,
    already_active: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backup_path: Option<String>,
}

/// Splits `plan.to_add` into (a) the ids actually named in `requested`
/// (base-compared, matching [`rim_session::ActiveSet::activate`]'s own
/// "already active" check) and (b) everything else — closure-pulled-in
/// dependencies. Shared by the text and JSON renderers so the two can
/// never disagree about which id landed in which bucket.
fn split_by_requested<'a>(
    plan: &'a ActivatePlan,
    requested: &[ModId],
) -> (Vec<&'a ModId>, Vec<&'a ModId>) {
    let requested_bases: BTreeSet<ModId> = requested.iter().map(ModId::base).collect();
    plan.to_add
        .iter()
        .partition(|id| requested_bases.contains(&id.base()))
}

fn activate_plan_json(
    plan: &ActivatePlan,
    requested: &[ModId],
    backup_path: Option<&Path>,
) -> ActivatePlanJson {
    let (will_activate, dependencies_also_activated) = split_by_requested(plan, requested);
    ActivatePlanJson {
        will_activate: will_activate.iter().map(ToString::to_string).collect(),
        dependencies_also_activated: dependencies_also_activated
            .iter()
            .map(ToString::to_string)
            .collect(),
        unresolvable_dependencies: plan
            .unresolvable_dependencies
            .iter()
            .map(|(id, deps)| {
                (
                    id.to_string(),
                    deps.iter().map(ToString::to_string).collect(),
                )
            })
            .collect(),
        already_active: plan
            .already_active
            .iter()
            .map(ToString::to_string)
            .collect(),
        backup_path: backup_path.map(|p| p.display().to_string()),
    }
}

fn print_activate_plan_text(plan: &ActivatePlan, requested: &[ModId]) {
    let (will_activate, dependency_adds) = split_by_requested(plan, requested);

    println!("will activate:");
    for id in &will_activate {
        println!("  {}", TerminalSafe::line(id));
    }
    if !dependency_adds.is_empty() {
        println!("dependencies also activated:");
        for id in &dependency_adds {
            println!("  {}", TerminalSafe::line(id));
        }
    }
    if !plan.unresolvable_dependencies.is_empty() {
        println!("unresolvable dependencies (not on disk):");
        for (id, deps) in &plan.unresolvable_dependencies {
            let deps_text = deps
                .iter()
                .map(ModId::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  {}: {}",
                TerminalSafe::line(id),
                TerminalSafe::line(deps_text)
            );
        }
    }
    if !plan.already_active.is_empty() {
        println!("already active:");
        for id in &plan.already_active {
            println!("  {}", TerminalSafe::line(id));
        }
    }
}

fn print_activate_result(
    plan: &ActivatePlan,
    requested: &[ModId],
    backup_path: Option<&Path>,
    json: bool,
) -> anyhow::Result<()> {
    if json {
        let payload = activate_plan_json(plan, requested, backup_path);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("serializing the plan")?
        );
        return Ok(());
    }
    print_activate_plan_text(plan, requested);
    match backup_path {
        Some(path) => {
            println!("wrote ModsConfig.xml");
            println!("backup: {}", path.display());
            println!("next: run 'rimmerge sort' then 'rimmerge apply' to place them");
        }
        None => println!("(dry run — nothing written)"),
    }
    Ok(())
}

fn run_activate(args: &ActivateArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let (project_paths, file, inventory) = discover(&args.paths)?;
    let active = ActiveSet::new(file.active_mods.clone(), &inventory)
        .context("the active list in ModsConfig.xml is invalid")?;

    let ids: Vec<ModId> = args.ids.iter().map(ModId::new).collect();
    for id in &ids {
        if !inventory.contains(id) {
            bail!(
                "{id} is not a known mod (no directory found on disk, and ModsConfig.xml names \
                 no missing entry for it either)"
            );
        }
    }

    let plan = plan_activate(&active, &inventory, &ids, args.with_dependencies);

    if args.dry_run {
        print_activate_result(&plan, &ids, None, args.json)?;
        return Ok(());
    }

    refuse_while_running(args.force, probe)?;

    let mut new_active = active.clone();
    new_active
        .activate(plan.to_add.clone(), &inventory)
        .context("activating the planned mods")?;

    let backup_path = write_active_set(&project_paths, &file, &new_active)?;
    print_activate_result(&plan, &ids, Some(&backup_path), args.json)?;
    Ok(())
}

// -- deactivate ---------------------------------------------------------------

#[derive(Serialize)]
struct DeactivatePlanJson {
    will_deactivate: Vec<String>,
    dependents_declared: BTreeMap<String, Vec<String>>,
    refused: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backup_path: Option<String>,
}

fn deactivate_plan_json(plan: &DeactivatePlan, backup_path: Option<&Path>) -> DeactivatePlanJson {
    DeactivatePlanJson {
        will_deactivate: plan.to_remove.iter().map(ToString::to_string).collect(),
        dependents_declared: plan
            .dependents_still_active
            .iter()
            .map(|(id, deps)| {
                (
                    id.to_string(),
                    deps.iter().map(ToString::to_string).collect(),
                )
            })
            .collect(),
        refused: plan.refused.iter().map(ToString::to_string).collect(),
        backup_path: backup_path.map(|p| p.display().to_string()),
    }
}

fn print_deactivate_plan_text(plan: &DeactivatePlan) {
    println!("will deactivate:");
    for id in &plan.to_remove {
        println!("  {}", TerminalSafe::line(id));
    }
    if !plan.dependents_still_active.is_empty() {
        // Discovery-only inventory never sees `Report::edges`, so this is
        // declared `modDependencies` only — never a `Hard`-strength
        // report edge a full scan would also catch (this module's own
        // doc comment).
        println!("dependents (declared) still active:");
        for (id, deps) in &plan.dependents_still_active {
            let deps_text = deps
                .iter()
                .map(ModId::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  {}: {}",
                TerminalSafe::line(id),
                TerminalSafe::line(deps_text)
            );
        }
    }
    if !plan.refused.is_empty() {
        println!("refused (Core, never removable):");
        for id in &plan.refused {
            println!("  {}", TerminalSafe::line(id));
        }
    }
}

/// Why `run_deactivate` did or didn't write — distinct from a bare
/// `Option<&Path>` specifically so a refusal never prints the same
/// "(dry run — nothing written)" text a genuine `--dry-run` does (a plain,
/// non-dry-run refusal reported as "dry run" would misdescribe what
/// happened).
enum WriteOutcome<'a> {
    Wrote { backup_path: &'a Path },
    DryRun,
    RefusedCore,
    BlockedByDependents,
}

fn print_deactivate_result(
    plan: &DeactivatePlan,
    outcome: &WriteOutcome<'_>,
    json: bool,
) -> anyhow::Result<()> {
    let backup_path = match outcome {
        WriteOutcome::Wrote { backup_path } => Some(*backup_path),
        WriteOutcome::DryRun | WriteOutcome::RefusedCore | WriteOutcome::BlockedByDependents => {
            None
        }
    };
    if json {
        let payload = deactivate_plan_json(plan, backup_path);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("serializing the plan")?
        );
        return Ok(());
    }
    print_deactivate_plan_text(plan);
    match outcome {
        WriteOutcome::Wrote { backup_path } => {
            println!("wrote ModsConfig.xml");
            println!("backup: {}", backup_path.display());
            println!("next: run 'rimmerge sort' then 'rimmerge apply' to place them");
        }
        WriteOutcome::DryRun => println!("(dry run — nothing written)"),
        WriteOutcome::RefusedCore => println!("(refused — nothing written)"),
        WriteOutcome::BlockedByDependents => println!(
            "(blocked by active dependents — nothing written; pass --yes to deactivate anyway)"
        ),
    }
    Ok(())
}

fn run_deactivate(args: &DeactivateArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let (project_paths, file, inventory) = discover(&args.paths)?;
    let active = ActiveSet::new(file.active_mods.clone(), &inventory)
        .context("the active list in ModsConfig.xml is invalid")?;

    let ids: Vec<ModId> = args.ids.iter().map(ModId::new).collect();
    let plan = plan_deactivate(&active, &inventory, &ids);

    if args.dry_run {
        print_deactivate_result(&plan, &WriteOutcome::DryRun, args.json)?;
        return Ok(());
    }

    if !plan.refused.is_empty() {
        print_deactivate_result(&plan, &WriteOutcome::RefusedCore, args.json)?;
        let refused = plan
            .refused
            .iter()
            .map(ModId::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        bail!("refusing: Core ({refused}) can never be deactivated");
    }
    if !plan.dependents_still_active.is_empty() && !args.yes {
        print_deactivate_result(&plan, &WriteOutcome::BlockedByDependents, args.json)?;
        bail!(
            "one or more mods to deactivate still have active dependents (see \
             \"dependents\" above) — pass --yes to deactivate anyway, or deactivate the \
             dependents first"
        );
    }

    refuse_while_running(args.force, probe)?;

    let mut new_active = active.clone();
    new_active.deactivate(&plan.to_remove);

    let backup_path = write_active_set(&project_paths, &file, &new_active)?;
    print_deactivate_result(
        &plan,
        &WriteOutcome::Wrote {
            backup_path: &backup_path,
        },
        args.json,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use rim_analyzer::extract::rich_text::{RichRun, RunStyle};
    use rim_session::use_cases::{AboutUnreadable, DescriptionText};

    use super::*;
    use crate::test_fixtures::copy_dir_recursive;

    fn read_outcome(text: &str) -> AboutOutcome {
        AboutOutcome::Read {
            mod_version: None,
            mod_icon_path: None,
            description: Some(DescriptionText {
                runs: vec![RichRun {
                    text: text.to_string(),
                    style: RunStyle::default(),
                }],
                truncated: false,
            }),
            homepage: None,
        }
    }

    #[test]
    fn format_description_strips_control_characters_but_keeps_newline_and_tab() {
        let text = format_description(&read_outcome("line one\n\tindented\u{0007}bell\u{009B}csi"))
            .expect("must be Some");

        assert_eq!(text, "line one\n\tindentedbellcsi");
    }

    #[test]
    fn format_description_discloses_a_reason_for_changed_and_unreadable() {
        let changed =
            format_description(&AboutOutcome::Changed).expect("Changed must disclose a reason");
        assert!(changed.starts_with("description unavailable:"), "{changed}");

        let unreadable = format_description(&AboutOutcome::Unreadable(AboutUnreadable::Xml {
            detail: "XML parse error".to_string(),
        }))
        .expect("Unreadable must disclose its reason");
        assert_eq!(
            unreadable,
            "description unavailable: invalid About.xml: XML parse error"
        );
    }

    #[test]
    fn format_description_is_none_for_not_on_disk() {
        assert_eq!(format_description(&AboutOutcome::NotOnDisk), None);
    }

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    fn sample_game_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("crates")
            .join("rim-analyzer")
            .join("tests")
            .join("fixtures")
            .join("sample_game")
    }

    /// A scratch copy of `rim-analyzer`'s checked-in fixture game tree —
    /// never the real game install or the real `ModsConfig.xml`.
    fn scratch_paths(temp_dir: &Path) -> PathsArgs {
        let game_dir = temp_dir.join("game");
        copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
        fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");

        PathsArgs {
            game_dir: Some(game_dir.clone()),
            workshop_dir: Some(temp_dir.join("workshop_does_not_exist")),
            mods_config: Some(game_dir.join("ModsConfig.xml")),
            profile_dir: Some(temp_dir.join("profile")),
        }
    }

    fn find_backup_file(game_dir: &Path) -> Option<PathBuf> {
        fs::read_dir(game_dir)
            .expect("read game dir")
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .map(|entry| entry.path())
    }

    #[test]
    fn activate_refuses_to_write_when_the_probe_reports_the_game_running_and_not_forced() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());
        let game_dir = paths.game_dir.clone().expect("game_dir was set");
        let args = ActivateArgs {
            paths,
            ids: vec!["aaa.mod".to_string()],
            with_dependencies: false,
            dry_run: false,
            force: false,
            json: false,
        };

        let result = run_activate(&args, &FixedProbe(true));

        let error = result.expect_err("must refuse when the game looks like it's running");
        assert!(error.to_string().contains("--force"));
        assert!(
            find_backup_file(&game_dir).is_none(),
            "a refused activate must never write a backup"
        );
    }

    #[test]
    fn activate_writes_when_forced_despite_the_game_running() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());
        let game_dir = paths.game_dir.clone().expect("game_dir was set");
        let args = ActivateArgs {
            paths,
            ids: vec!["aaa.mod".to_string()],
            with_dependencies: false,
            dry_run: false,
            force: true,
            json: false,
        };

        run_activate(&args, &FixedProbe(true)).expect("forced activate must succeed");

        assert!(
            find_backup_file(&game_dir).is_some(),
            "a forced, non-dry-run activate must leave a ModsConfig.xml.bak-* file"
        );
    }

    #[test]
    fn deactivate_refuses_to_write_when_the_probe_reports_the_game_running_and_not_forced() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());
        let game_dir = paths.game_dir.clone().expect("game_dir was set");
        let args = DeactivateArgs {
            paths,
            ids: vec!["sample.mod".to_string()],
            yes: true,
            dry_run: false,
            force: false,
            json: false,
        };

        let result = run_deactivate(&args, &FixedProbe(true));

        let error = result.expect_err("must refuse when the game looks like it's running");
        assert!(error.to_string().contains("--force"));
        assert!(
            find_backup_file(&game_dir).is_none(),
            "a refused deactivate must never write a backup"
        );
    }

    fn with_about_for(paths: &PathsArgs, id: &str) -> rim_session::use_cases::ModInfoWithAbout {
        let mut session =
            build_session(resolve_paths(paths).expect("resolve paths")).expect("build session");
        SelectOrder::new().execute(&mut session, rim_resolve::domain::OrderSource::Current);
        ReadModAbout::new(rim_io::FileModAboutReader::new())
            .execute(
                &mut session,
                &ModId::new(id),
                rim_resolve::domain::OrderSource::Current,
            )
            .unwrap_or_else(|error| panic!("{id} must resolve in the fixture: {error}"))
    }

    #[test]
    fn show_json_reports_an_active_mod_s_position_tier_and_dependents() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());

        // `sample.mod` is the fixture's own active mod (`ModsConfig.xml`)
        // — building a real session (not just discovery) is what makes
        // `position`/`tier`/`findingsTotal` available at all.
        let with_about = with_about_for(&paths, "sample.mod");
        let homepage = resolve_homepage(&with_about);
        let json = show_json(&with_about.info, homepage.as_ref(), None, None);

        assert_eq!(json.kind, "active");
        assert_eq!(json.mod_id, "sample.mod");
        assert_eq!(json.source.as_deref(), Some("local"));
        assert_eq!(json.position, Some(0));
        assert_eq!(json.tier, Some("body"));
        assert!(json.findings_total.is_some());
    }

    #[test]
    fn show_json_reports_an_inactive_mod_with_no_placement_fields() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());

        // `aaa.mod` is on disk but not listed in the fixture's
        // `ModsConfig.xml` — present, inactive.
        let with_about = with_about_for(&paths, "aaa.mod");
        let homepage = resolve_homepage(&with_about);
        let json = show_json(&with_about.info, homepage.as_ref(), None, None);

        assert_eq!(json.kind, "inactive");
        assert_eq!(json.mod_id, "aaa.mod");
        assert_eq!(json.position, None);
        assert_eq!(json.tier, None);
        assert_eq!(json.hard_dependents, None);
    }

    #[test]
    fn show_fails_with_a_grep_able_not_found_message_for_an_unknown_id() {
        let temp_dir = tempdir().expect("tempdir");
        let paths = scratch_paths(temp_dir.path());
        let args = ShowArgs {
            paths,
            id: "totally.unknown.mod".to_string(),
            source: ShowSourceArg::Current,
            json: false,
        };

        let error = run_show(&args).expect_err("an unknown id must fail");

        assert!(error.to_string().contains("not found"));
    }
}
