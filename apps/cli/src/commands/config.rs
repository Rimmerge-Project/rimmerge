//! `rimmerge config`: reads and writes `<base>/config.json`, the
//! app-global record of where this machine's RimWorld install, workshop
//! folder and `ModsConfig.xml` are (`rim_io::AppConfig`).
//!
//! Rung 3 of the resolution ladder: below `--game-dir` and the
//! `RIMMERGE_*` environment variables, above detection. `show` prints
//! both what is pinned here *and* what the whole ladder currently
//! resolves to, because "what did you actually decide" is the question a
//! user asking about paths has — including, when nothing resolves, the
//! full list of places that were looked at.
//!
//! `--base` exists so these three subcommands are testable against a
//! scratch directory (`apps/cli/tests/config_cli.rs`) without touching
//! the real `%LOCALAPPDATA%\rimmerge`. It is not a general-purpose knob:
//! every other command derives the same base from `%LOCALAPPDATA%`, and a
//! `config.json` written under some other base would never be read back.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};
use rim_io::AppConfig;

use crate::common::default_profile_base;

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Prints the pinned paths and what the resolution ladder currently
    /// resolves to.
    Show(BaseArgs),
    /// Pins one or more paths so `--game-dir` and friends stop being
    /// needed. Only the flags you pass are changed.
    Set(SetArgs),
    /// Unpins every path, leaving detection to do the work again.
    Clear(BaseArgs),
}

#[derive(Debug, Args)]
pub struct BaseArgs {
    /// The directory `config.json`, `profiles/` and `databases/` live
    /// under. Defaults to `%LOCALAPPDATA%\rimmerge`.
    #[arg(long, value_name = "DIR")]
    base: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    #[command(flatten)]
    base: BaseArgs,
    /// The RimWorld install directory (contains `Data/` and `Mods/`).
    #[arg(long)]
    game_dir: Option<PathBuf>,
    /// The Steam workshop content folder for RimWorld (app id `294100`).
    #[arg(long)]
    workshop_dir: Option<PathBuf>,
    /// The active `ModsConfig.xml`.
    #[arg(long)]
    mods_config: Option<PathBuf>,
}

impl BaseArgs {
    fn resolve(&self) -> anyhow::Result<PathBuf> {
        match &self.base {
            Some(base) => Ok(base.clone()),
            None => default_profile_base(),
        }
    }
}

pub fn run(command: &ConfigCommand) -> anyhow::Result<()> {
    match command {
        ConfigCommand::Show(args) => show(args),
        ConfigCommand::Set(args) => set(args),
        ConfigCommand::Clear(args) => clear(args),
    }
}

fn show(args: &BaseArgs) -> anyhow::Result<()> {
    let base = args.resolve()?;
    let config = AppConfig::load(&base);

    println!("config file: {}", rim_io::app_config_path(&base).display());
    if config.is_empty() {
        println!("  (nothing pinned)");
    } else {
        print_pinned("game-dir", config.game_dir.as_deref());
        print_pinned("workshop-dir", config.workshop_dir.as_deref());
        print_pinned("mods-config", config.mods_config.as_deref());
    }

    println!();
    match rim_io::resolve_project_paths(rim_io::PathOverrides::new(base)) {
        Ok(resolved) => {
            println!("resolved:");
            println!("  game-dir:     {}", resolved.paths.game_dir.display());
            println!("  workshop-dir: {}", resolved.paths.workshop_dir.display());
            println!("  mods-config:  {}", resolved.paths.mods_config.display());
            println!("  profile-dir:  {}", resolved.paths.profile_dir.display());
            if let Some(warning) = &resolved.warning {
                println!("warning: {warning}");
            }
        }
        // Not an error exit: `config show` is the command a user runs
        // *because* resolution is failing, so printing why is the whole
        // point of running it.
        Err(error) => println!("not resolved: {error}"),
    }
    Ok(())
}

fn print_pinned(label: &str, value: Option<&std::path::Path>) {
    match value {
        Some(path) => println!("  {label}: {}", path.display()),
        None => println!("  {label}: (not set)"),
    }
}

fn set(args: &SetArgs) -> anyhow::Result<()> {
    if args.game_dir.is_none() && args.workshop_dir.is_none() && args.mods_config.is_none() {
        anyhow::bail!(
            "nothing to set: pass at least one of --game-dir, --workshop-dir, --mods-config"
        );
    }
    let base = args.base.resolve()?;
    let mut config = AppConfig::load(&base);
    // Every pin is absolutized first (`rim_io::pinned_path`): `config.json`
    // is read back by a process whose working directory is wherever it
    // was launched, so a stored relative path would resolve somewhere
    // different every run — see that function's own doc comment.
    if let Some(game_dir) = &args.game_dir {
        config.game_dir = Some(rim_io::pinned_path(game_dir));
    }
    if let Some(workshop_dir) = &args.workshop_dir {
        config.workshop_dir = Some(rim_io::pinned_path(workshop_dir));
    }
    if let Some(mods_config) = &args.mods_config {
        config.mods_config = Some(rim_io::pinned_path(mods_config));
    }
    config
        .save(&base)
        .with_context(|| format!("writing {}", rim_io::app_config_path(&base).display()))?;

    println!("wrote {}", rim_io::app_config_path(&base).display());
    // A path that isn't an install today is still worth pinning (an
    // external drive that isn't mounted yet, a typo the user is about to
    // fix), so this warns rather than refusing the write — in the same
    // wording `resolve_project_paths` and the desktop Setup page use.
    if let Some(game_dir) = &config.game_dir
        && !rim_analyzer::infra::paths::is_game_dir(game_dir)
    {
        println!("warning: {}", rim_io::not_an_install_warning(game_dir));
    }
    Ok(())
}

fn clear(args: &BaseArgs) -> anyhow::Result<()> {
    let base = args.resolve()?;
    AppConfig::default()
        .save(&base)
        .with_context(|| format!("writing {}", rim_io::app_config_path(&base).display()))?;
    println!("cleared {}", rim_io::app_config_path(&base).display());
    Ok(())
}
