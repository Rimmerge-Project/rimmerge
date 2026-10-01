//! `rimmerge sort`: sorts a bare report and prints the suggested order
//! with per-mod change markers plus the disturbance stats.

use std::path::PathBuf;

use clap::Args;
use rim_analyzer::domain::{LoadOrder, ModId};
use rim_resolve::domain::SorterOverrides;
use rim_resolve::sort::{DisturbanceStats, EnforcedLayers, SortInput, SortOutcome, sort};
use rim_session::filter_imported_rules;

use crate::common::{
    TerminalSafe, TieBreakArg, build_rules_and_tagging, position_marker, read_report,
};

mod dropped;

#[derive(Debug, Args)]
pub struct SortArgs {
    /// Path to a JSON `Report` (e.g. from `rimmerge load` or `rim-analyzer
    /// analyze --json`).
    #[arg(long)]
    report: PathBuf,
    /// Which base key an unconstrained mod's emission starts from.
    #[arg(long, value_enum, default_value_t = TieBreakArg::Rebuild)]
    tie_break: TieBreakArg,
    /// Let imported (RimSort user/community/`SteamDb`) pair rules feed the
    /// sorter. Off by default: evidence first. No effect here today: `sort` is JSON-in only and loads no
    /// rules file to filter — see `common::build_rules_and_tagging`.
    #[arg(long)]
    use_imported_pairs: bool,
    /// Exclude imported placement rules (`Top`/`Bottom` tier pins) from
    /// the sorter. On by default: a placement is a tier statement no
    /// file-level fact can reconstruct. No effect here today: `sort` is
    /// JSON-in only and loads no rules file to filter — see
    /// `common::build_rules_and_tagging`.
    #[arg(long)]
    no_imported_placements: bool,
    /// Treat lazily-resolved `AssemblyRef`s as real ordering constraints.
    #[arg(long)]
    enforce_soft: bool,
    /// Treat awareness-only edges (`FindMod`/`MayRequire`/...) as real
    /// ordering constraints.
    #[arg(long)]
    enforce_awareness: bool,
    /// Exclude `Inferred`-strength edges (`PatchRemovedNode`/
    /// `RetextureAfterOwner`/`DefOverrideAfterOrigin`/
    /// `PatchInvalidatesPredicate`/
    /// `PatchRemovedNodeCosmetic`) from real
    /// ordering constraints, treating them as advisory-only instead. On
    /// (enforced) by default, unlike `--enforce-soft`/`--enforce-awareness`
    /// — see `EnforcedLayers::inferred`'s own doc comment for why.
    #[arg(long)]
    no_inferred: bool,
    /// After the summary, list every dropped edge: the edge, the cycle it
    /// would have closed, and (for a direct two-mod contradiction) the edge
    /// that overruled it. Like the rest of `sort`, this uses no profile
    /// rules, so a dropped edge from one of your rules is not shown here.
    #[arg(long)]
    dropped: bool,
    /// With `--dropped`: keep only edges with this mod at either end.
    #[arg(long = "mod", value_name = "ID", requires = "dropped")]
    only_mod: Option<String>,
}

pub fn run(args: &SortArgs) -> anyhow::Result<()> {
    let report = read_report(&args.report)?;
    let (rules, tagging) = build_rules_and_tagging(&report);
    let rules = filter_imported_rules(rules, args.use_imported_pairs, !args.no_imported_placements);
    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
    let overrides = SorterOverrides::default();

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &tagging,
        overrides: &overrides,
        current: &current,
        enforce: EnforcedLayers {
            soft: args.enforce_soft,
            awareness: args.enforce_awareness,
            inferred: !args.no_inferred,
        },
        tie_break: args.tie_break.into(),
    });

    print_order(&outcome, &current);
    print_summary(&outcome);
    if args.dropped {
        let only_mod = args.only_mod.as_deref().map(ModId::new);
        println!();
        print!("{}", dropped::render(&outcome.dropped, only_mod.as_ref())?);
    }
    Ok(())
}

fn print_order(outcome: &SortOutcome, current: &LoadOrder) {
    println!(
        "Suggested order ({} mods) — marker: = unchanged, +N moved N slots earlier, -N moved N slots later, NEW not in the current order",
        outcome.order.as_slice().len()
    );
    for (new_position, id) in outcome.order.as_slice().iter().enumerate() {
        println!(
            "{:>4}. {:<45} {}",
            new_position + 1,
            TerminalSafe::line(id).to_string(),
            position_marker(new_position, current.position(id))
        );
    }
}

fn print_summary(outcome: &SortOutcome) {
    let stats: &DisturbanceStats = &outcome.stats;
    println!();
    println!("Disturbance vs current order:");
    println!(
        "  kendall_tau_inversions : {}",
        stats.kendall_tau_inversions
    );
    println!("  positions_changed      : {}", stats.positions_changed);
    println!(
        "  displaced >10 slots    : {}",
        stats.mods_displaced_over_10
    );
    println!(
        "  displaced >50 slots    : {}",
        stats.mods_displaced_over_50
    );
    println!("  max_displacement       : {}", stats.max_displacement);
    println!();
    println!(
        "dropped edges: {}, any-of choices: {}, warnings: {}",
        outcome.dropped.len(),
        outcome.any_of_choices.len(),
        outcome.warnings.len()
    );
}
