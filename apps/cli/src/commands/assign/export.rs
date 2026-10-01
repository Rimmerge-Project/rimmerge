//! `assign export`: writing the assignment as a patch mod.

use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Args;
use rim_io::GameProcessProbe;
use rim_resolve::domain::RowKey;
use rim_session::use_cases::{AssignmentExportOptions, ExportAssignment};

use super::parse_assignment_id;
use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The assignment's id, from `assign list`.
    #[arg(long)]
    assignment: String,
    /// The folder to render the assignment into — never `<game>/Mods` or
    /// a subfolder of it (use `--install` instead).
    #[arg(long = "out")]
    out_dir: PathBuf,
    /// Also copy the rendered folder into `<game>/Mods` and activate it.
    #[arg(long)]
    install: bool,
    /// Install even if `RimWorldWin64.exe` looks like it's running.
    #[arg(long)]
    force: bool,
}

pub(super) fn run_export(args: &ExportArgs) -> anyhow::Result<()> {
    run_export_with_probe(args, &rim_io::SysinfoGameProcessProbe::new())
}

/// Shares the same process-probe seam `patch export`'s own
/// `run_export_with_probe` uses, so `--install`'s running-game refusal can
/// be exercised with a fake probe in a unit test.
fn run_export_with_probe(args: &ExportArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let id = parse_assignment_id(&args.assignment)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    if args.install && !args.force && probe.is_running() {
        bail!(
            "RimWorldWin64.exe is running; close the game or retry with --force (installing an assignment writes ModsConfig.xml, and RimWorld overwrites it on exit)"
        );
    }

    let out_dir = rim_io::resolve_user_dir(&args.out_dir)
        .with_context(|| format!("resolving --out {}", args.out_dir.display()))?;

    let use_case = ExportAssignment::new(
        rim_io::MergeModFolderWriter::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
    );
    let outcome = match use_case.execute(
        &mut session,
        &id,
        AssignmentExportOptions {
            out_dir,
            install: args.install,
        },
    ) {
        Ok(outcome) => outcome,
        Err(error @ rim_session::use_cases::ExportAssignmentError::Unknown(_)) => {
            bail!("assignment {id} not found: {error}")
        }
        // `{error:?}` (the derived `Debug`) so a refusal names its own
        // `ExportAssignmentError` variant verbatim, mirroring `patch
        // export`'s own convention.
        Err(error) => {
            return Err(anyhow::anyhow!(
                "exporting the assignment: {error:?} ({error})"
            ));
        }
    };

    println!("exported to:      {}", outcome.export_path.display());
    if let Some(installed) = &outcome.installed_path {
        println!("installed to:     {}", installed.display());
    }
    if let Some(backup) = &outcome.mods_config_backup {
        println!("ModsConfig.xml backup: {}", backup.display());
    }
    println!("files written:    {}", outcome.files.len());
    println!("content hash:     {}", outcome.content_sha256);
    // `outcome.skipped` is one unified list across
    // every section (`rim_merge::assign::SkippedField`'s own `def_type`/
    // `row: RowKey` shape) — split by `RowKey` variant purely for this
    // command's own two-heading text output, not because the two shapes
    // are different types.
    let (own_skips, target_skips): (Vec<_>, Vec<_>) = outcome
        .skipped
        .iter()
        .partition(|skip| matches!(skip.row, RowKey::Own(_)));
    if !own_skips.is_empty() {
        println!("skipped {} free-standing field(s):", own_skips.len());
        for skip in &own_skips {
            let RowKey::Own(def_name) = &skip.row else {
                unreachable!("partitioned above")
            };
            println!(
                "  {} ({}) {}: {}",
                TerminalSafe::line(def_name),
                TerminalSafe::line(&skip.def_type),
                TerminalSafe::line(&skip.path),
                TerminalSafe::line(&skip.reason)
            );
        }
    }
    if !target_skips.is_empty() {
        println!("skipped {} field(s):", target_skips.len());
        for skip in &target_skips {
            let RowKey::Target(target) = &skip.row else {
                unreachable!("partitioned above")
            };
            println!(
                "  {} ({}) {}: {}",
                TerminalSafe::line(&target.def),
                TerminalSafe::line(&skip.def_type),
                TerminalSafe::line(&skip.path),
                TerminalSafe::line(&skip.reason)
            );
        }
    }
    Ok(())
}
