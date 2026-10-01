//! `rimmerge` CLI: a thin `clap` binary over `rim-resolve`/`rim-session`/
//! `rim-io` — `sort`/`ledger`/`fixture trim` are JSON-in only (no live
//! project needed); `load`/`import`/`apply` work against a real RimWorld
//! install through `rim-io`'s adapters.

mod commands;
mod common;
#[cfg(test)]
mod test_fixtures;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rimmerge",
    version,
    about = "Rimmerge: load-order sorter and resolution ledger"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sorts a JSON report and prints the suggested order.
    Sort(commands::sort::SortArgs),
    /// Builds and prints the resolution ledger for a JSON report.
    Ledger(commands::ledger::LedgerArgs),
    /// Fixture-generation utilities.
    Fixture {
        #[command(subcommand)]
        command: commands::fixture::FixtureCommand,
    },
    /// Scans a RimWorld install and writes the report into the profile.
    Load(commands::load::LoadArgs),
    /// Imports a `Player.log`, attributing every failure/timer/etc.
    /// against the current install's active mods.
    Log {
        #[command(subcommand)]
        command: commands::log::LogCommand,
    },
    /// App-global path config (`<base>/config.json`): where this
    /// machine's RimWorld install, workshop folder and `ModsConfig.xml`
    /// are, so `--game-dir` and friends stop being needed.
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommand,
    },
    /// Rule-database cache maintenance: `status` reads the cache with no
    /// network, `refresh` fetches whichever enabled sources are due.
    Db {
        #[command(subcommand)]
        command: commands::db::DbCommand,
    },
    /// The app-global internet-access policy (`<base>/app-settings.json`):
    /// `status` reads it, `on`/`off` flip the master switch. Internet access
    /// means only the two GitHub hosts (update checks and rule-database
    /// refreshes), never your local network.
    Network {
        #[command(subcommand)]
        command: commands::network::NetworkCommand,
    },
    /// A manual, explicit check for a newer Rimmerge release. Never
    /// automatic — this is the only thing that makes this command run.
    CheckUpdate(commands::check_update::CheckUpdateArgs),
    /// Imports RimSort's rule databases into the current profile.
    Import(commands::import::ImportArgs),
    /// Writes the selected order to `ModsConfig.xml` (or previews it).
    Apply(commands::apply::ApplyArgs),
    /// Lists, activates, and deactivates mods directly in
    /// `ModsConfig.xml`'s own `<activeMods>` list.
    Mods {
        #[command(subcommand)]
        command: commands::mods::ModsCommand,
    },
    /// Merge editor diagnostics: `plan` prints one finding's diff and
    /// plan, `coverage` tallies xpath-grammar support over contested
    /// patch collisions.
    Merge {
        #[command(subcommand)]
        command: commands::merge::MergeCommand,
    },
    /// Compatibility patches: a user-chosen scope of two or more mods
    /// with its own decisions, exported as a publishable mod.
    Patch {
        #[command(subcommand)]
        command: commands::patch::PatchCommand,
    },
    /// Promotes an imported pair or placement rule to a user-owned copy
    /// that survives both import toggles and a re-import.
    Promote(commands::promote::PromoteArgs),
    /// Creates or replaces a user-owned pair or placement rule directly
    /// (`set-placement`/`set-pair`/`remove-pair`), for a mod or pair with
    /// no existing imported rule to `promote`.
    Rule {
        #[command(subcommand)]
        command: commands::rule::RuleCommand,
    },
    /// The def inspector: `changes` lists what one mod changes,
    /// `inspect` answers everything about one def/template, `search`
    /// finds a def/template by name.
    Defs {
        #[command(subcommand)]
        command: commands::defs::DefsCommand,
    },
    /// The patch maker: infers a generic reference-mod-to-target-mod
    /// assignment schema, then edits and exports one `Defs/`-only mod
    /// per project.
    Assign {
        #[command(subcommand)]
        command: commands::assign::AssignCommand,
    },
    /// Runs a real replay of every active cross-mod patcher against the
    /// chosen order and predicts which top-level operations RimWorld
    /// itself would log as failed. Explicit, on-demand only — never
    /// part of `sort`/`ledger`'s cached path.
    Verify(commands::verify::VerifyArgs),
    /// Prints the per-mod startup-cost table (patch ops, slow-shape
    /// xpaths, texture/DLL counts and bytes) from a JSON `Report` — no
    /// timings.
    Startup(commands::startup::StartupArgs),
}

/// Reports a failure the way `fn main() -> Result` would (`Error: ` plus the
/// error's `Debug` chain, exit code 1), except that the text goes through
/// [`common::TerminalSafe`]: an error message can quote a mod id, a path or
/// a name read from an install, a report or a log, and must not carry a
/// control sequence to the terminal.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "Error: {}",
                common::TerminalSafe::block(format!("{error:?}"))
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Sort(args) => commands::sort::run(args),
        Command::Ledger(args) => commands::ledger::run(args),
        Command::Fixture { command } => commands::fixture::run(command),
        Command::Load(args) => commands::load::run(args),
        Command::Log { command } => commands::log::run(command),
        Command::Config { command } => commands::config::run(command),
        Command::Db { command } => commands::db::run(command),
        Command::Network { command } => commands::network::run(command),
        Command::CheckUpdate(args) => commands::check_update::run(args),
        Command::Import(args) => commands::import::run(args),
        Command::Apply(args) => commands::apply::run(args),
        Command::Mods { command } => commands::mods::run(command),
        Command::Merge { command } => commands::merge::run(command),
        Command::Patch { command } => commands::patch::run(command),
        Command::Promote(args) => commands::promote::run(args),
        Command::Rule { command } => commands::rule::run(command),
        Command::Defs { command } => commands::defs::run(command),
        Command::Assign { command } => commands::assign::run(command),
        Command::Verify(args) => commands::verify::run(args),
        Command::Startup(args) => commands::startup::run(args),
    }
}
