//! `rimmerge load`: runs the real scanner against a RimWorld install and
//! writes the resulting report into the profile directory.

use std::time::Instant;

use anyhow::Context;
use clap::Args;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Args)]
pub struct LoadArgs {
    #[command(flatten)]
    paths: PathsArgs,
}

pub fn run(args: &LoadArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let profile_dir = paths.profile_dir.clone();

    let start = Instant::now();
    let session = build_session(paths)?;
    let elapsed = start.elapsed();

    std::fs::create_dir_all(&profile_dir)
        .with_context(|| format!("creating profile directory {}", profile_dir.display()))?;
    let report_path = profile_dir.join("report.json");
    let json = serde_json::to_string_pretty(session.report()).context("serializing report")?;
    std::fs::write(&report_path, json)
        .with_context(|| format!("writing {}", report_path.display()))?;

    println!(
        "scanned {} mod(s) in {:.2?}",
        session.report().mods.len(),
        elapsed
    );
    println!(
        "game version: {}",
        TerminalSafe::line(&session.report().metadata.game_version)
    );
    println!("warnings: {}", session.report().warnings.len());
    println!("profile directory: {}", profile_dir.display());
    println!("report written to: {}", report_path.display());
    Ok(())
}
