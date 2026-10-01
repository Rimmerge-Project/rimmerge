//! `rimmerge ledger`: sorts a bare report, builds the resolution ledger
//! for one order source, and prints the status breakdown plus the top
//! (lowest-confidence, needs-input-first) items.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use rim_analyzer::domain::{LoadOrder, Mod, ModId};
use rim_resolve::domain::{
    Confidence, DecisionSet, OrderSource, ResolutionStatus, SorterOverrides,
};
use rim_resolve::ledger::{BuildLedgerInput, build};
use rim_resolve::sort::{EnforcedLayers, SortInput, sort};
use rim_session::{FindingFilter, FindingIndex, filter_imported_rules};

use crate::common::{
    TerminalSafe, TieBreakArg, build_rules_and_tagging, format_order_source, read_report,
};

const TOP_ITEMS: usize = 20;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SourceArg {
    Current,
    Suggested,
}

impl From<SourceArg> for OrderSource {
    fn from(value: SourceArg) -> Self {
        match value {
            SourceArg::Current => OrderSource::Current,
            SourceArg::Suggested => OrderSource::Suggested,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StatusArg {
    Auto,
    NeedsInput,
    Overridden,
}

impl From<StatusArg> for ResolutionStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Auto => ResolutionStatus::Auto,
            StatusArg::NeedsInput => ResolutionStatus::NeedsInput,
            StatusArg::Overridden => ResolutionStatus::UserOverridden,
        }
    }
}

#[derive(Debug, Args)]
pub struct LedgerArgs {
    /// Path to a JSON `Report`.
    #[arg(long)]
    report: PathBuf,
    /// Which order to build the ledger for.
    #[arg(long, value_enum, default_value_t = SourceArg::Current)]
    source: SourceArg,
    /// Only list items with this status.
    #[arg(long, value_enum)]
    status: Option<StatusArg>,
    /// Which base key an unconstrained mod's emission starts from.
    #[arg(long, value_enum, default_value_t = TieBreakArg::Rebuild)]
    tie_break: TieBreakArg,
    /// Let imported (RimSort user/community/`SteamDb`) pair rules feed the
    /// sorter. Off by default: evidence first. No effect here today: `ledger` is JSON-in only and loads no
    /// rules file to filter — see `common::build_rules_and_tagging`.
    #[arg(long)]
    use_imported_pairs: bool,
    /// Exclude imported placement rules (`Top`/`Bottom` tier pins) from
    /// the sorter. On by default: a placement is a tier statement no
    /// file-level fact can reconstruct. No effect here today: `ledger` is
    /// JSON-in only and loads no rules file to filter — see
    /// `common::build_rules_and_tagging`.
    #[arg(long)]
    no_imported_placements: bool,
    /// Treat lazily-resolved `AssemblyRef`s as real ordering constraints.
    #[arg(long)]
    enforce_soft: bool,
    /// Treat awareness-only edges as real ordering constraints.
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
    /// The confidence threshold separating `Auto` from `NeedsInput`.
    #[arg(long, default_value_t = 80)]
    threshold: u8,
    /// Surface `DanglingDefReference` findings (a name written at a
    /// recognized reference site that no active def of any type
    /// actually has). Off by default — see
    /// `rim_session::Settings::show_dangling_def_references`'s own doc
    /// comment for the measured false-positive rate. Never changes sort
    /// output.
    #[arg(long)]
    show_dangling_def_references: bool,
}

pub fn run(args: &LedgerArgs) -> anyhow::Result<()> {
    let report = read_report(&args.report)?;
    let (rules, tagging) = build_rules_and_tagging(&report);
    let rules = filter_imported_rules(rules, args.use_imported_pairs, !args.no_imported_placements);
    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
    let overrides = SorterOverrides::default();

    let sort_outcome = sort(&SortInput {
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

    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let threshold =
        Confidence::new(args.threshold).map_err(|e| anyhow::anyhow!("--threshold: {e}"))?;
    let decisions = DecisionSet::new();
    let source: OrderSource = args.source.into();

    let ledger = build(&BuildLedgerInput {
        report: &report,
        sort_outcome: &sort_outcome,
        rules: &rules,
        tagging: &tagging,
        decisions: &decisions,
        threshold,
        source,
        current: &current,
        suggested: &sort_outcome.order,
        mods_by_id: &mods_by_id,
        show_dangling_def_references: args.show_dangling_def_references,
    });

    print_breakdown(&ledger);

    let index = FindingIndex::build(&ledger);
    let filter = FindingFilter {
        status: args.status.map(Into::into),
        limit: TOP_ITEMS,
        ..FindingFilter::default()
    };
    let page = index.page(&ledger, &filter);

    println!();
    println!(
        "Top {} of {} matching item(s) (needs-input first, lowest confidence first):",
        page.items.len(),
        page.total
    );
    for key in &page.items {
        let Some(entry_index) = index.index_of(key) else {
            continue;
        };
        let entry = &ledger.entries[entry_index];
        println!(
            "  [{}] conf={:<3} {}",
            format_status(entry.status),
            entry.suggestion.confidence.percent(),
            TerminalSafe::line(key)
        );
    }
    Ok(())
}

fn format_status(status: ResolutionStatus) -> &'static str {
    match status {
        ResolutionStatus::Auto => "Auto",
        ResolutionStatus::NeedsInput => "NeedsInput",
        ResolutionStatus::UserOverridden => "UserOverridden",
    }
}

fn print_breakdown(ledger: &rim_resolve::domain::Ledger) {
    println!(
        "Ledger for {} — {} finding(s):",
        format_order_source(ledger.source),
        ledger.entries.len()
    );
    println!("  auto               : {}", ledger.stats.auto);
    println!("  needs_input        : {}", ledger.stats.needs_input);
    println!("  overridden         : {}", ledger.stats.overridden);
    println!(
        "  resolved_by_suggested : {}",
        ledger.stats.resolved_by_suggested
    );
}
