//! `rimmerge order export|import`: share a load order as a RimWorld mod
//! list (`.rml`) or as text, and apply a shared list to `ModsConfig.xml`.
//!
//! **Zero business logic here.** The list, its text codec, the diff
//! (`plan_import`) and the consent rule (`ImportLoss`) all live in
//! `rim-session`; the `.rml` reader/writer is `rim-io`'s `RmlFileStore`.
//! Like `mods list|activate|deactivate`, both subcommands use
//! `rim_io::discover_inventory` (discovery only, seconds, no full scan),
//! and `import` is a real writer: an explicit `<activeMods>` write through
//! `ModsConfigStore::write_with_backup`, with the running-game probe and
//! `--force`. It deliberately does not go through `apply`, which writes
//! Current or Suggested from a full scan and cannot name an external list.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use rim_analyzer::domain::ModId;
use rim_io::GameProcessProbe;
use rim_resolve::domain::GeneratedModIdentity;
use rim_session::mod_info::workshop_url;
use rim_session::mod_list::{
    Activation, CorePlacement, ImportContext, ImportPlan, ImportedEntry, MissingKind,
    ModListLimits, Rejection, SkippedEntry, VersionCheck, render_text,
};
use rim_session::ports::ModListRead;
use rim_session::use_cases::{
    ExportOrder, ExportSource, ImportLoss, ImportOrder, ImportOrderError, ImportPreview,
    ImportTarget, PreviewOrderImport, ReadyImport,
};
use rim_session::{ActiveSet, ModInventory, ProjectPaths};

use crate::commands::mods::{discover, refuse_while_running, write_active_set};
use crate::common::{PathsArgs, TerminalSafe, resolve_paths};

#[derive(Debug, Subcommand)]
pub enum OrderCommand {
    /// Exports the order in `ModsConfig.xml`: as text on stdout, or as a
    /// RimWorld mod list (`.rml`) with `--out`. Never touches
    /// `ModsConfig.xml`.
    Export(ExportArgs),
    /// Previews a shared list (a `.rml`, a `ModsConfig.xml`-shaped file or
    /// text; `-` reads text from stdin) and writes it as the active order.
    Import(ImportArgs),
}

pub fn run(command: &OrderCommand) -> anyhow::Result<()> {
    match command {
        OrderCommand::Export(args) => run_export(args),
        OrderCommand::Import(args) => run_import(args, &rim_io::SysinfoGameProcessProbe::new()),
    }
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Write a RimWorld mod list (`.rml`) here instead of printing text.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Replace `--out`'s file if it already exists (without this, an
    /// existing file is a refusal).
    #[arg(long, requires = "out")]
    overwrite: bool,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The list to import, or `-` to read it from stdin (the text format or
    /// a `.rml`, detected by content).
    #[arg(value_name = "FILE")]
    source: PathBuf,
    /// Print the plan without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Import even though it deactivates active mods or names mods that
    /// are not installed (without this, that's a refusal).
    #[arg(long)]
    yes: bool,
    /// Write `ModsConfig.xml` even if `RimWorldWin64.exe` looks like it's
    /// running.
    #[arg(long)]
    force: bool,
}

/// This profile's own generated merge mod: never exported, never deactivated.
fn merge_mod_for(paths: &ProjectPaths) -> ModId {
    GeneratedModIdentity::for_profile(paths.profile_hash()).package_id
}

/// The installed game's `major.minor`, the same text a scan records in a
/// session's report. `None` when `Version.txt` cannot be read: the list
/// then simply carries no version, as the domain types allow.
fn game_version(paths: &ProjectPaths) -> Option<String> {
    rim_analyzer::infra::paths::default_game_version(&paths.game_dir)
        .ok()
        .map(|version| version.to_string())
}

// -- export ---------------------------------------------------------------

fn run_export(args: &ExportArgs) -> anyhow::Result<()> {
    if let Some(out) = &args.out
        && !args.overwrite
        && out.exists()
    {
        bail!(
            "{} already exists; pass --overwrite to replace it",
            out.display()
        );
    }
    let paths = resolve_paths(&args.paths)?;
    let (inventory, _active) = rim_io::discover_inventory(&paths, None)
        .context("discovering the mod inventory (no full scan)")?;
    let source = ExportSource {
        mods_config: paths.mods_config.clone(),
        inventory,
        own_merge_mod: merge_mod_for(&paths),
        game_version: game_version(&paths),
    };
    let use_case = ExportOrder::new(
        rim_io::ModsConfigFileStore::new(),
        rim_io::RmlFileStore::new(),
    );
    match &args.out {
        Some(out) => {
            let exported = use_case.write_file(&source, out)?;
            report_unrepresentable(&exported.unrepresentable);
            println!(
                "wrote {} mods to {}",
                exported.list.entries().len(),
                TerminalSafe::line(out.display())
            );
        }
        None => {
            let exported = use_case.build(&source)?;
            report_unrepresentable(&exported.unrepresentable);
            // Raw `print!`: the text is the product (pasted elsewhere), and
            // `render_text` already strips control characters from names.
            print!("{}", render_text(&exported.list));
        }
    }
    Ok(())
}

/// Active ids that cannot be written into a shared list go to stderr, so
/// the text on stdout stays exactly what gets pasted.
fn report_unrepresentable(ids: &[ModId]) {
    for id in ids {
        eprintln!(
            "warning: {} is not a valid package id and was left out of the list",
            TerminalSafe::line(id)
        );
    }
}

// -- import ---------------------------------------------------------------

fn import_target(
    paths: &ProjectPaths,
    file_ids: &[ModId],
    inventory: ModInventory,
) -> anyhow::Result<ImportTarget> {
    Ok(ImportTarget {
        inventory,
        file_ids: file_ids.to_vec(),
        context: ImportContext {
            own_merge_mod: merge_mod_for(paths),
            game_version: game_version(paths),
            // The CLI has no working set, so there is nothing to replace.
            has_pending_changes: false,
        },
    })
}

fn preview(source: &Path, target: &ImportTarget) -> anyhow::Result<ImportPreview> {
    if source.as_os_str() != "-" {
        return PreviewOrderImport::new(rim_io::RmlFileStore::new())
            .from_file(source, target)
            .map_err(Into::into);
    }
    let bytes = rim_io::read_mod_list_bounded(io::stdin().lock()).context("reading stdin")?;
    let read = match bytes {
        Some(bytes) => rim_io::parse_mod_list_bytes(&bytes),
        None => ModListRead::Rejected(Rejection::TooLarge {
            limit_bytes: ModListLimits::MAX_INPUT_BYTES,
        }),
    };
    Ok(ImportPreview::from_read(read, target))
}

fn describe_rejection(rejection: Rejection) -> String {
    match rejection {
        Rejection::TooLarge { limit_bytes } => {
            format!("the input is larger than {limit_bytes} bytes")
        }
        Rejection::TooManyEntries { limit } => format!("the list has more than {limit} entries"),
        Rejection::MalformedXml => "the file is not valid XML".to_string(),
        Rejection::DtdNotAllowed => "the file declares a DTD, which a mod list never does".into(),
        Rejection::TooDeep => "the file is nested too deeply to be a mod list".to_string(),
        Rejection::UnrecognizedFormat => {
            "the input is not a RimWorld mod list (.rml), a ModsConfig.xml, or a text list".into()
        }
        Rejection::MissingModList => "the file has no mod list in it".to_string(),
        Rejection::NoEntries => "no mod ids were found".to_string(),
    }
}

fn describe_invalid_order(error: ImportOrderError) -> anyhow::Error {
    match error {
        ImportOrderError::CoreMissing => anyhow::anyhow!(
            "refusing: the resulting order would have no Core (no installed copy of \
             ludeon.rimworld), so nothing was written"
        ),
        ImportOrderError::Unknown(id) => anyhow::anyhow!(
            "refusing: {} is not an installed mod, so nothing was written",
            TerminalSafe::line(id)
        ),
        ImportOrderError::Duplicate(id) => anyhow::anyhow!(
            "refusing: {} appears more than once in the planned order, so nothing was written",
            TerminalSafe::line(id)
        ),
        ImportOrderError::NothingInstalled => anyhow::anyhow!(
            "refusing: none of the listed mods is installed, so the import would only \
             deactivate your mods; nothing was written"
        ),
        ImportOrderError::TooMany { limit } => anyhow::anyhow!(
            "refusing: the planned order holds more than {limit} mods, so nothing was written"
        ),
        ImportOrderError::ScanDidNotMatch => {
            anyhow::anyhow!("refusing: the planned order does not match what was validated")
        }
    }
}

fn run_import(args: &ImportArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let (project_paths, file, inventory) = discover(&args.paths)?;
    let target = import_target(&project_paths, &file.active_mods, inventory)?;
    let ready = match preview(&args.source, &target)? {
        ImportPreview::Ready(ready) => ready,
        ImportPreview::Rejected(rejection) => {
            bail!("cannot import: {}", describe_rejection(rejection))
        }
    };
    print_plan(&ready, &target);
    // The same rule the desktop applies (every id installed, none repeated,
    // Core present), checked before the dry-run exit, the consent rule and
    // the probe so a bad order is never reported as anything else.
    let validated = ImportOrder::validate_order(&target.inventory, ready.plan.order.clone())
        .map_err(describe_invalid_order)?;

    if args.dry_run {
        println!("(dry run — nothing written)");
        return Ok(());
    }
    if let Some(loss) = ImportLoss::of(&ready.plan)
        && !args.yes
    {
        println!("(blocked — nothing written; pass --yes to import anyway)");
        bail!(
            "this import deactivates {} active mod(s) and names {} mod(s) that are not \
             installed — pass --yes to import anyway",
            loss.deactivated,
            loss.not_installed
        );
    }
    if ready.plan.order == file.active_mods {
        println!("(ModsConfig.xml already has this order — nothing written)");
        return Ok(());
    }

    refuse_while_running(args.force, probe)?;
    let active = ActiveSet::new(validated.order().to_vec(), &target.inventory)
        .context("the planned order is invalid")?;
    let backup_path = write_active_set(&project_paths, &file, &active)?;
    println!("wrote ModsConfig.xml");
    println!("backup: {}", backup_path.display());
    println!("next: run 'rimmerge apply --dry-run --source current' to check it");
    Ok(())
}

// -- plan output ----------------------------------------------------------

fn print_plan(ready: &ReadyImport, target: &ImportTarget) {
    let plan = &ready.plan;
    let activated = activated_ids(plan);
    println!(
        "import plan: {} mods in the new order, {} activated, {} deactivated, {} moved",
        plan.order.len(),
        activated.len(),
        plan.deactivated.len(),
        plan.moved
    );
    print_notes(plan);
    print_section("activate", activated.iter().map(|id| id.to_string()));
    print_section(
        "deactivate",
        plan.deactivated
            .iter()
            .map(|id| named(id, &target.inventory)),
    );
    print_section("not installed", plan.entries.iter().filter_map(missing_row));
    print_section(
        "matched another copy",
        plan.entries.iter().filter_map(other_copy_row),
    );
    print_section(
        "listed more than once",
        plan.entries.iter().filter_map(duplicate_row),
    );
    print_section("skipped", skipped_rows(ready).into_iter());
}

fn print_notes(plan: &ImportPlan) {
    match plan.core {
        CorePlacement::Listed => {}
        CorePlacement::AddedFirst => println!("core: not in the list; kept first"),
        CorePlacement::Missing => println!("core: no installed Core to place"),
    }
    match &plan.version {
        VersionCheck::Unknown | VersionCheck::Same => {}
        VersionCheck::Differs { listed, game } => println!(
            "version: made with RimWorld {}; this install is {}",
            TerminalSafe::line(listed),
            TerminalSafe::line(game)
        ),
    }
}

fn print_section(title: &str, rows: impl Iterator<Item = String>) {
    let rows: Vec<String> = rows.collect();
    if rows.is_empty() {
        return;
    }
    println!("{title} ({}):", rows.len());
    let mut stdout = io::stdout().lock();
    for row in rows {
        // A failed write to stdout (a closed pipe) is not worth a panic.
        let _ = writeln!(stdout, "  {row}");
    }
}

fn activated_ids(plan: &ImportPlan) -> Vec<&ModId> {
    plan.entries
        .iter()
        .filter_map(|entry| match entry {
            ImportedEntry::Activated { id } => Some(id),
            ImportedEntry::MatchedOtherCopy {
                installed,
                activation: Activation::Activated,
                ..
            } => Some(installed),
            ImportedEntry::AlreadyActive { .. }
            | ImportedEntry::MatchedOtherCopy {
                activation: Activation::AlreadyActive,
                ..
            }
            | ImportedEntry::NotInstalled { .. }
            | ImportedEntry::Duplicate { .. } => None,
        })
        .collect()
}

/// `id` with the inventory's display name, terminal-safe.
fn named(id: &ModId, inventory: &ModInventory) -> String {
    match inventory.entry(id) {
        Some(entry) if entry.name != id.as_str() => format!(
            "{} ({})",
            TerminalSafe::line(id),
            TerminalSafe::line(&entry.name)
        ),
        Some(_) | None => TerminalSafe::line(id).to_string(),
    }
}

fn missing_row(entry: &ImportedEntry) -> Option<String> {
    let ImportedEntry::NotInstalled { listed, name, kind } = entry else {
        return None;
    };
    let label = match name {
        Some(name) => format!(
            "{} ({})",
            TerminalSafe::line(listed),
            TerminalSafe::line(name)
        ),
        None => TerminalSafe::line(listed).to_string(),
    };
    let detail = match kind {
        MissingKind::Workshop(id) => workshop_url(id.get()).as_str().to_string(),
        MissingKind::Dlc => "a DLC you do not have".to_string(),
        MissingKind::RimmergeMergeMod => {
            "made by Rimmerge on the sender's computer; not needed".to_string()
        }
        MissingKind::NoLink => "no Workshop link in the list".to_string(),
    };
    Some(format!("{label} — {detail}"))
}

fn other_copy_row(entry: &ImportedEntry) -> Option<String> {
    let ImportedEntry::MatchedOtherCopy {
        listed, installed, ..
    } = entry
    else {
        return None;
    };
    Some(format!(
        "{} -> {}",
        TerminalSafe::line(listed),
        TerminalSafe::line(installed)
    ))
}

fn duplicate_row(entry: &ImportedEntry) -> Option<String> {
    let ImportedEntry::Duplicate { id, first_position } = entry else {
        return None;
    };
    Some(format!(
        "{} (first listed at #{first_position}; that position is used)",
        TerminalSafe::line(id)
    ))
}

fn skipped_rows(ready: &ReadyImport) -> Vec<String> {
    let mut rows: Vec<String> = ready
        .skipped
        .iter()
        .map(|skipped| match skipped {
            SkippedEntry::NotAnEntry { line } => format!("line {line}: not a mod entry"),
            SkippedEntry::MalformedId { position, text } => format!(
                "entry {position}: \"{}\" is not a valid package id",
                TerminalSafe::line(text.as_str())
            ),
        })
        .collect();
    if ready.omitted_skipped > 0 && !rows.is_empty() {
        rows.push(format!("... and {} more", ready.omitted_skipped));
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::test_fixtures::copy_dir_recursive;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    /// A scratch copy of `rim-analyzer`'s checked-in fixture game tree
    /// (`sample.mod` active; `aaa.mod`/`zzz.mod` inactive) plus a text list
    /// that swaps `sample.mod` for `aaa.mod`.
    fn scratch(temp_dir: &Path) -> (PathsArgs, PathBuf, PathBuf) {
        let game_dir = temp_dir.join("game");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("crates")
            .join("rim-analyzer")
            .join("tests")
            .join("fixtures")
            .join("sample_game");
        copy_dir_recursive(&fixture, &game_dir).expect("copy fixture game tree");
        fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");
        let core_about = game_dir.join("Data").join("Core").join("About");
        fs::create_dir_all(&core_about).expect("create Core");
        fs::write(
            core_about.join("About.xml"),
            "<ModMetaData><packageId>Ludeon.RimWorld</packageId></ModMetaData>",
        )
        .expect("write Core About.xml");
        let list = temp_dir.join("list.txt");
        fs::write(&list, "1. Aaa [aaa.mod]\n").expect("write list");
        let paths = PathsArgs {
            game_dir: Some(game_dir.clone()),
            workshop_dir: Some(temp_dir.join("workshop_does_not_exist")),
            mods_config: Some(game_dir.join("ModsConfig.xml")),
            profile_dir: Some(temp_dir.join("profile")),
        };
        (paths, game_dir, list)
    }

    fn import_args(paths: PathsArgs, list: PathBuf, force: bool) -> ImportArgs {
        ImportArgs {
            paths,
            source: list,
            dry_run: false,
            yes: true,
            force,
        }
    }

    fn has_backup(game_dir: &Path) -> bool {
        fs::read_dir(game_dir)
            .expect("read game dir")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
    }

    #[test]
    fn import_refuses_to_write_when_the_probe_reports_the_game_running_and_not_forced() {
        let temp_dir = tempdir().expect("tempdir");
        let (paths, game_dir, list) = scratch(temp_dir.path());
        let before = fs::read(game_dir.join("ModsConfig.xml")).expect("read");

        let error = run_import(&import_args(paths, list, false), &FixedProbe(true))
            .expect_err("must refuse while the game looks like it's running");

        assert!(error.to_string().contains("--force"));
        assert!(!has_backup(&game_dir), "a refusal must never take a backup");
        assert_eq!(
            fs::read(game_dir.join("ModsConfig.xml")).expect("read"),
            before
        );
    }

    #[test]
    fn import_writes_when_forced_despite_the_game_running() {
        let temp_dir = tempdir().expect("tempdir");
        let (paths, game_dir, list) = scratch(temp_dir.path());

        run_import(&import_args(paths, list, true), &FixedProbe(true))
            .expect("a forced import must succeed");

        assert!(has_backup(&game_dir), "a forced import takes a backup");
        let written = fs::read_to_string(game_dir.join("ModsConfig.xml")).expect("read");
        assert!(written.contains("<li>aaa.mod</li>"));
        assert!(!written.contains("<li>sample.mod</li>"));
    }
}
