//! CLI entry point: parses arguments, runs the scan and analysis, writes
//! the optional JSON report, and prints the text summary. All rendering
//! logic lives in [`rim_analyzer::interface::text`] — this is a thin shell.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use clap::{Args, Parser, Subcommand};

use rim_analyzer::domain::{FolderPolicy, GameVersion, Report};
use rim_analyzer::{analysis, infra, interface};

#[derive(Parser)]
#[command(
    name = "rim-analyzer",
    version,
    about = "Read-only analyzer for RimWorld mod load orders"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a RimWorld install and report load-order-relevant facts.
    Analyze(AnalyzeArgs),
}

#[derive(Args)]
struct AnalyzeArgs {
    /// RimWorld install directory (contains `Data/` and `Mods/`).
    #[arg(long)]
    game_dir: Option<PathBuf>,
    /// Steam workshop content folder for RimWorld (app id 294100).
    #[arg(long)]
    workshop_dir: Option<PathBuf>,
    /// Path to `ModsConfig.xml`.
    #[arg(long)]
    mods_config: Option<PathBuf>,
    /// Game version to resolve `ByVersion`/`LoadFolders.xml` against, e.g. `1.6`.
    #[arg(long)]
    game_version: Option<String>,
    /// Write the full JSON report to this path.
    #[arg(long)]
    json: Option<PathBuf>,
    /// Diagnostic mode: ignore `LoadFolders.xml` and scan every folder a mod ships.
    #[arg(long)]
    all_folders: bool,
    /// List every item in the text summary instead of the top 20.
    #[arg(long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Analyze(args) => run_analyze(&args),
    }
}

/// The error `--game-dir` was omitted on a machine with no detectable
/// install: every place that was looked at, one per line, so the user can see
/// why detection failed rather than being told a path they've never heard of.
/// The analyzer binary has no config file of its own (unlike `rimmerge`,
/// which adds `config.json` and `RIMMERGE_GAME_DIR` rungs above this one).
fn no_install_found() -> anyhow::Error {
    let candidates: Vec<String> = infra::paths::default_game_dir_candidates()
        .iter()
        .map(|candidate| format!("  {}", candidate.display()))
        .collect();
    anyhow::anyhow!(
        "no RimWorld install found. Pass --game-dir <path>. Looked in:\n{}",
        candidates.join("\n")
    )
}

fn run_analyze(args: &AnalyzeArgs) -> anyhow::Result<()> {
    let game_dir = match args.game_dir.clone() {
        Some(path) => path,
        None => infra::paths::detect_game_dir().ok_or_else(no_install_found)?,
    };
    let workshop_dir = args
        .workshop_dir
        .clone()
        .unwrap_or_else(|| infra::paths::default_workshop_dir(&game_dir));
    let mods_config_path = match &args.mods_config {
        Some(path) => path.clone(),
        None => infra::paths::default_mods_config_path()?,
    };
    let game_version = resolve_game_version(args.game_version.as_deref(), &game_dir)?;
    let folder_policy = if args.all_folders {
        FolderPolicy::Everything
    } else {
        FolderPolicy::LoadFolders
    };

    let scan_config = infra::ScanConfig {
        game_dir: game_dir.clone(),
        workshop_dir: workshop_dir.clone(),
        mods_config_path: mods_config_path.clone(),
        game_version,
        folder_policy,
        active_mods: None,
    };

    let start = Instant::now();
    let scan_output = infra::scan(&scan_config)?;
    let context = analysis::RunContext {
        game_dir,
        workshop_dir,
        mods_config_path,
        game_version,
    };
    let report = infra::build_explained_report(&scan_output, &context);
    let elapsed = start.elapsed();

    if let Some(json_path) = &args.json {
        write_json_report(&report, json_path)?;
    }

    interface::text::print_summary(std::io::stdout(), &report, args.verbose, elapsed);
    Ok(())
}

fn resolve_game_version(cli_value: Option<&str>, game_dir: &Path) -> anyhow::Result<GameVersion> {
    match cli_value {
        Some(v) => v
            .parse()
            .with_context(|| format!("parsing --game-version {v:?}")),
        None => infra::paths::default_game_version(game_dir),
    }
}

fn write_json_report(report: &Report, path: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report).context("serializing report to JSON")?;
    std::fs::write(path, json).with_context(|| format!("writing JSON report to {}", path.display()))
}
