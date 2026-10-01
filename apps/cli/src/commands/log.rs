//! `rimmerge log import`: reads and parses one or more game logs (a
//! `Player.log` or an in-game console snapshot, told apart by content),
//! attributes every failure/timer/etc. against the current install's active
//! mods, and prints each file's result with what it covers.
//! Needs a live session (unlike `sort`/`ledger`/`startup`'s JSON-in
//! shape) because attribution reads the scan's own active-mod list —
//! the same shape `verify`/`defs inspect` already are.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, Subcommand, ValueEnum};
use rim_analyzer::domain::{Mod, ModId};
use rim_io::game_log::MAX_LINE_BYTES;
use rim_session::ports::{
    KindChoice, LoadEventKind, LogKind, LogReadStats, RawCrossReference, ReadLoss,
};
use rim_session::use_cases::{
    DdsFailureSummary, DependencyWarningSummary, EnclosingOp, GameLogSummary, ImportGameLog,
    LoadEvent, LogAttribution, PatchFailureSummary, StackTraceDetail, TimerSummary,
    UnpairedStackTrace,
};
use serde::Serialize;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

mod coverage;
mod entries;

use self::coverage::{
    CoverageJson, LoggingGapJson, LowerBoundFamilyJson, format_kind, logging_gaps_json,
    lower_bound_families_json, print_coverage, print_gap_warning, print_lower_bound_note,
    snapshot_scope,
};
use self::entries::{
    ClassJson, SentinelsJson, TotalsJson, classes_json, print_classes, print_entry_warnings,
    print_totals,
};

#[derive(Debug, Subcommand)]
pub enum LogCommand {
    /// Imports one or more game logs (a `Player.log` or an in-game console
    /// snapshot), attributing every failure/timer/etc. against the current
    /// install's active mods, and says what each file covers.
    Import(LogImportArgs),
}

/// How `log import` decides a file's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum KindArg {
    /// By content, from the first lines: a console-copy trace start
    /// anywhere there means a console snapshot; otherwise the exact
    /// `RimWorld <version> rev<n>` banner means a `Player.log`; neither
    /// means a console snapshot.
    Auto,
    /// Read every file as a `Player.log`.
    PlayerLog,
    /// Read every file as a console snapshot.
    ConsoleSnapshot,
}

impl From<KindArg> for KindChoice {
    fn from(kind: KindArg) -> Self {
        match kind {
            KindArg::Auto => Self::Detect,
            KindArg::PlayerLog => Self::Force(LogKind::PlayerLog),
            KindArg::ConsoleSnapshot => Self::Force(LogKind::ConsoleSnapshot),
        }
    }
}

#[derive(Debug, Args)]
pub struct LogImportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// One or more logs to import (a `Player.log` or a console snapshot).
    /// Read-only: never modified, moved, or copied. Each file is imported
    /// on its own; they are not merged.
    #[arg(required = true, num_args = 1..)]
    log_paths: Vec<PathBuf>,
    /// How to decide each file's kind. `auto` reads it from the content;
    /// the file name is never used.
    #[arg(long, value_enum, default_value_t = KindArg::Auto)]
    kind: KindArg,
    /// Print machine-readable JSON instead of text: an object for one
    /// path, an array of objects (each with its `path`) for several.
    #[arg(long)]
    json: bool,
}

pub fn run(command: &LogCommand) -> anyhow::Result<()> {
    match command {
        LogCommand::Import(args) => run_import(args),
    }
}

fn run_import(args: &LogImportArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let session = build_session(paths)?;
    let mods_by_id: BTreeMap<ModId, &Mod> = session
        .report()
        .mods
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();

    let use_case = ImportGameLog::new(rim_io::FileGameLogReader::new());
    let choice = KindChoice::from(args.kind);
    let summaries = args
        .log_paths
        .iter()
        .map(|path| {
            use_case
                .execute(&session, path, choice)
                .with_context(|| format!("importing {}", path.display()))
        })
        .collect::<anyhow::Result<Vec<GameLogSummary>>>()?;

    if args.json {
        return print_json(&args.log_paths, &summaries, &mods_by_id);
    }
    let is_kind_forced = args.kind != KindArg::Auto;
    for (index, (path, summary)) in args.log_paths.iter().zip(&summaries).enumerate() {
        if args.log_paths.len() > 1 {
            if index > 0 {
                println!();
            }
            println!("==> {} <==", TerminalSafe::line(path.display()));
        }
        print_text(summary, &mods_by_id, is_kind_forced);
    }
    Ok(())
}

/// A mod's own display name, falling back to its raw id when the id somehow
/// isn't in this scan's own `mods_by_id` (should not happen for an active
/// mod, but this command never panics on a report inconsistency).
fn mod_name(mods_by_id: &BTreeMap<ModId, &Mod>, id: &ModId) -> String {
    mods_by_id
        .get(id)
        .map_or_else(|| id.to_string(), |m| m.name.clone())
}

/// A mod's display name for an [`LogAttribution::Mod`], or the raw logged
/// text for [`LogAttribution::Unattributed`].
fn format_attribution(mods_by_id: &BTreeMap<ModId, &Mod>, attribution: &LogAttribution) -> String {
    match attribution {
        LogAttribution::Mod(id) => TerminalSafe::line(mod_name(mods_by_id, id)).to_string(),
        LogAttribution::Unattributed(raw) => {
            format!("(unattributed: {})", TerminalSafe::line(raw))
        }
    }
}

fn print_stack_trace_detail(detail: &StackTraceDetail) {
    println!(
        "    leaf: {} {} — {}",
        TerminalSafe::line(&detail.leaf_class),
        TerminalSafe::line(detail.leaf_xpath.as_deref().unwrap_or("-")),
        TerminalSafe::line(&detail.leaf_reason)
    );
    for op in &detail.enclosing_chain {
        let branch = op
            .branch
            .as_deref()
            .map_or_else(String::new, |b| format!(" <{}>", TerminalSafe::line(b)));
        println!(
            "    enclosing: {}{branch} — {}",
            TerminalSafe::line(&op.class),
            TerminalSafe::line(&op.reason)
        );
    }
}

/// One line per load event (kind, save name if any, line, mod count),
/// then a note when the events' mod lists disagree.
fn print_load_events(summary: &GameLogSummary) {
    for event in &summary.load_events {
        let kind = match &event.kind {
            LoadEventKind::NewGame => "new game".to_string(),
            LoadEventKind::SaveLoad { save_name } => {
                format!("save load \"{}\"", TerminalSafe::line(save_name))
            }
        };
        let mod_count = event.mods.len();
        let noun = if mod_count == 1 { "mod" } else { "mods" };
        println!(
            "  load event: {kind}, line {}, {mod_count} {noun}",
            event.line
        );
    }
    if summary.load_events_disagree() {
        println!("  the load events' mod lists differ from each other");
    }
}

/// `count` with the singular or plural of a noun.
fn counted(count: u64, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

/// How the text output names one kind of loss.
fn describe_loss(kind: ReadLoss, count: u64) -> String {
    match kind {
        ReadLoss::LineTruncated => format!(
            "{} truncated to the first {} KiB",
            counted(count, "line", "lines"),
            MAX_LINE_BYTES / 1024
        ),
        ReadLoss::LineWithInvalidUtf8 => format!(
            "{} with invalid UTF-8 (decoded lossily)",
            counted(count, "line", "lines")
        ),
        ReadLoss::StackBlockLineDropped => format!(
            "{} not examined",
            counted(count, "stack-trace block line", "stack-trace block lines")
        ),
        ReadLoss::StackJoinAbandoned => format!(
            "{} abandoned",
            counted(count, "multi-line join", "multi-line joins")
        ),
        ReadLoss::PassFolded => format!(
            "{} folded into the last tracked startup pass",
            counted(count, "entry", "entries")
        ),
        ReadLoss::StackRefDropped => format!(
            "{} not indexed",
            counted(count, "stack back-reference", "stack back-references")
        ),
        ReadLoss::LoggingGapDropped => format!(
            "{} not listed",
            counted(count, "logging gap", "logging gaps")
        ),
        ReadLoss::CrashReportPathTruncated => format!(
            "{} cut to its bound",
            counted(count, "crash-report location", "crash-report locations")
        ),
    }
}

/// One line naming every bound the reader had to apply, and only when one
/// applied: an ordinary log prints nothing here.
fn print_read_losses(stats: &LogReadStats) {
    let parts: Vec<String> = stats
        .losses()
        .into_iter()
        .map(|(kind, count)| describe_loss(kind, count))
        .collect();
    if parts.is_empty() {
        return;
    }
    println!(
        "  read {} lines; bounded: {}",
        stats.lines_read,
        parts.join(", ")
    );
}

/// The title of a file's text output, by kind. A snapshot's title states
/// its scope: it is what one console held, never a whole session.
fn import_title(summary: &GameLogSummary) -> String {
    match summary.kind() {
        LogKind::PlayerLog => "Player.log import".to_string(),
        LogKind::ConsoleSnapshot => {
            format!("Console snapshot import ({})", snapshot_scope(summary))
        }
    }
}

fn print_text(summary: &GameLogSummary, mods_by_id: &BTreeMap<ModId, &Mod>, is_kind_forced: bool) {
    let with_stack_trace = summary
        .patch_failures
        .iter()
        .filter(|f| f.stack_trace.is_some())
        .count();
    print_gap_warning(summary);
    println!(
        "{}: {} patch failures ({with_stack_trace} with stack-trace detail), \
         {} extra stack traces, {} cross-references, {} DDS failures, \
         {} dependency warnings, {} timers, {} load events",
        import_title(summary),
        summary.patch_failures.len(),
        summary.extra_stack_traces.len(),
        summary.cross_references.len(),
        summary.dds_failures.len(),
        summary.dependency_warnings.len(),
        summary.timers.len(),
        summary.load_events.len()
    );
    print_coverage(summary, is_kind_forced);
    print_entry_warnings(summary);
    print_load_events(summary);
    print_totals(summary);
    print_read_losses(&summary.read_stats);

    println!();
    for failure in &summary.patch_failures {
        let file = failure
            .source_file
            .as_deref()
            .map_or_else(String::new, |f| {
                format!(" (file: {})", TerminalSafe::line(f))
            });
        println!(
            "[{}] Patch operation {} failed{file}",
            format_attribution(mods_by_id, &failure.attribution),
            TerminalSafe::line(&failure.operation)
        );
        if let Some(detail) = &failure.stack_trace {
            print_stack_trace_detail(detail);
        }
    }

    if !summary.extra_stack_traces.is_empty() {
        println!();
        println!("stack traces with no terse failure line (kept, not dropped):");
        for extra in &summary.extra_stack_traces {
            println!("[{}]", format_attribution(mods_by_id, &extra.attribution));
            match &extra.detail {
                Some(detail) => print_stack_trace_detail(detail),
                None => println!("    (no line in this block could be parsed as an operation)"),
            }
        }
    }

    if !summary.dds_failures.is_empty() {
        println!();
        println!("DDS failures:");
        for dds in &summary.dds_failures {
            println!(
                "  [{}] {} ({}x{} {})",
                format_attribution(mods_by_id, &dds.attribution),
                TerminalSafe::line(&dds.path),
                dds.width,
                dds.height,
                TerminalSafe::line(&dds.format)
            );
        }
    }

    if !summary.dependency_warnings.is_empty() {
        println!();
        println!("dependencies without a download URL:");
        for warning in &summary.dependency_warnings {
            println!(
                "  [{}] {}",
                format_attribution(mods_by_id, &warning.attribution),
                TerminalSafe::line(&warning.dependency_id)
            );
        }
    }

    if !summary.timers.is_empty() {
        println!();
        println!("timers:");
        for timer in &summary.timers {
            println!(
                "  {} — {} ms ({})",
                TerminalSafe::line(&timer.label),
                timer.milliseconds,
                format_attribution(mods_by_id, &timer.attribution)
            );
        }
    }

    if !summary.cross_references.is_empty() {
        println!();
        println!("cross-reference errors:");
        for cross_reference in &summary.cross_references {
            println!("  {}", format_cross_reference(cross_reference));
        }
    }

    if !summary.def_cache_lines.is_empty() {
        println!();
        println!("def-cache plugin lines (for the apply-dialog note):");
        for line in &summary.def_cache_lines {
            println!("  {}", TerminalSafe::line(line));
        }
    }

    print_classes(summary, mods_by_id);
    print_lower_bound_note(summary);
}

fn format_cross_reference(cross_reference: &RawCrossReference) -> String {
    match cross_reference {
        RawCrossReference::Wanter {
            missing_type,
            missing_name,
            wanter_field,
        } => format!(
            "no {} named {} (wanter={})",
            TerminalSafe::line(missing_type),
            TerminalSafe::line(missing_name),
            TerminalSafe::line(wanter_field)
        ),
        RawCrossReference::WantingDef {
            missing_type,
            missing_name,
            wanting_def,
            note,
        } => {
            let note = note
                .as_deref()
                .map_or_else(String::new, |n| format!(" ({})", TerminalSafe::line(n)));
            format!(
                "no {} named {}, wanted by {}{note}",
                TerminalSafe::line(missing_type),
                TerminalSafe::line(missing_name),
                TerminalSafe::line(wanting_def)
            )
        }
    }
}

#[derive(Debug, Serialize)]
struct AttributionJson {
    mod_id: Option<String>,
    mod_name: Option<String>,
    unattributed_raw: Option<String>,
}

fn attribution_json(
    mods_by_id: &BTreeMap<ModId, &Mod>,
    attribution: &LogAttribution,
) -> AttributionJson {
    match attribution {
        LogAttribution::Mod(id) => AttributionJson {
            mod_id: Some(id.to_string()),
            mod_name: Some(mod_name(mods_by_id, id)),
            unattributed_raw: None,
        },
        LogAttribution::Unattributed(raw) => AttributionJson {
            mod_id: None,
            mod_name: None,
            unattributed_raw: Some(raw.clone()),
        },
    }
}

#[derive(Debug, Serialize)]
struct EnclosingOpJson {
    class: String,
    detail: Option<String>,
    reason: String,
    branch: Option<String>,
}

fn enclosing_op_json(op: &EnclosingOp) -> EnclosingOpJson {
    EnclosingOpJson {
        class: op.class.clone(),
        detail: op.detail.clone(),
        reason: op.reason.clone(),
        branch: op.branch.clone(),
    }
}

#[derive(Debug, Serialize)]
struct StackTraceDetailJson {
    leaf_class: String,
    leaf_xpath: Option<String>,
    leaf_reason: String,
    enclosing_chain: Vec<EnclosingOpJson>,
}

fn stack_trace_detail_json(detail: &StackTraceDetail) -> StackTraceDetailJson {
    StackTraceDetailJson {
        leaf_class: detail.leaf_class.clone(),
        leaf_xpath: detail.leaf_xpath.clone(),
        leaf_reason: detail.leaf_reason.clone(),
        enclosing_chain: detail
            .enclosing_chain
            .iter()
            .map(enclosing_op_json)
            .collect(),
    }
}

#[derive(Debug, Serialize)]
struct PatchFailureJson {
    attribution: AttributionJson,
    operation: String,
    source_file: Option<String>,
    stack_trace: Option<StackTraceDetailJson>,
}

fn patch_failure_json(
    failure: &PatchFailureSummary,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> PatchFailureJson {
    PatchFailureJson {
        attribution: attribution_json(mods_by_id, &failure.attribution),
        operation: failure.operation.clone(),
        source_file: failure.source_file.clone(),
        stack_trace: failure.stack_trace.as_ref().map(stack_trace_detail_json),
    }
}

#[derive(Debug, Serialize)]
struct UnpairedStackTraceJson {
    attribution: AttributionJson,
    source_file: Option<String>,
    detail: Option<StackTraceDetailJson>,
}

fn unpaired_stack_trace_json(
    extra: &UnpairedStackTrace,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> UnpairedStackTraceJson {
    UnpairedStackTraceJson {
        attribution: attribution_json(mods_by_id, &extra.attribution),
        source_file: extra.source_file.clone(),
        detail: extra.detail.as_ref().map(stack_trace_detail_json),
    }
}

#[derive(Debug, Serialize)]
struct DdsFailureJson {
    attribution: AttributionJson,
    path: String,
    width: u32,
    height: u32,
    format: String,
}

fn dds_failure_json(dds: &DdsFailureSummary, mods_by_id: &BTreeMap<ModId, &Mod>) -> DdsFailureJson {
    DdsFailureJson {
        attribution: attribution_json(mods_by_id, &dds.attribution),
        path: dds.path.clone(),
        width: dds.width,
        height: dds.height,
        format: dds.format.clone(),
    }
}

#[derive(Debug, Serialize)]
struct DependencyWarningJson {
    attribution: AttributionJson,
    dependency_id: String,
}

fn dependency_warning_json(
    warning: &DependencyWarningSummary,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> DependencyWarningJson {
    DependencyWarningJson {
        attribution: attribution_json(mods_by_id, &warning.attribution),
        dependency_id: warning.dependency_id.clone(),
    }
}

#[derive(Debug, Serialize)]
struct TimerJson {
    attribution: AttributionJson,
    label: String,
    milliseconds: u64,
    pass: u32,
}

fn timer_json(timer: &TimerSummary, mods_by_id: &BTreeMap<ModId, &Mod>) -> TimerJson {
    TimerJson {
        attribution: attribution_json(mods_by_id, &timer.attribution),
        label: timer.label.clone(),
        milliseconds: timer.milliseconds,
        pass: timer.pass,
    }
}

#[derive(Debug, Serialize)]
struct CrossReferenceJson {
    kind: &'static str,
    missing_type: String,
    missing_name: String,
    wanter_field: Option<String>,
    wanting_def: Option<String>,
    note: Option<String>,
}

fn cross_reference_json(cross_reference: &RawCrossReference) -> CrossReferenceJson {
    match cross_reference {
        RawCrossReference::Wanter {
            missing_type,
            missing_name,
            wanter_field,
        } => CrossReferenceJson {
            kind: "wanter",
            missing_type: missing_type.clone(),
            missing_name: missing_name.clone(),
            wanter_field: Some(wanter_field.clone()),
            wanting_def: None,
            note: None,
        },
        RawCrossReference::WantingDef {
            missing_type,
            missing_name,
            wanting_def,
            note,
        } => CrossReferenceJson {
            kind: "wanting_def",
            missing_type: missing_type.clone(),
            missing_name: missing_name.clone(),
            wanter_field: None,
            wanting_def: Some(wanting_def.clone()),
            note: note.clone(),
        },
    }
}

#[derive(Debug, Serialize)]
struct LoadEventJson {
    line: usize,
    /// `"new_game"` or `"save_load"`.
    kind: &'static str,
    /// The save's name for `"save_load"`; always emitted, `null` for `"new_game"`.
    save_name: Option<String>,
    mods: Vec<String>,
}

fn load_event_json(event: &LoadEvent) -> LoadEventJson {
    let (kind, save_name) = match &event.kind {
        LoadEventKind::NewGame => ("new_game", None),
        LoadEventKind::SaveLoad { save_name } => ("save_load", Some(save_name.clone())),
    };
    LoadEventJson {
        line: event.line,
        kind,
        save_name,
        mods: event.mods.iter().map(ToString::to_string).collect(),
    }
}

#[derive(Debug, Serialize)]
struct ReadStatsJson {
    lines_read: u64,
    lines_truncated: u64,
    lines_with_invalid_utf8: u64,
    stack_block_lines_dropped: u64,
    stack_joins_abandoned: u64,
    passes_folded: u64,
    stack_refs_dropped: u64,
    logging_gaps_dropped: u64,
    crash_report_paths_truncated: u64,
}

impl From<&LogReadStats> for ReadStatsJson {
    fn from(stats: &LogReadStats) -> Self {
        // Exhaustive on purpose: a new counter must be added here or this
        // stops compiling.
        let LogReadStats {
            lines_read,
            lines_truncated,
            lines_with_invalid_utf8,
            stack_block_lines_dropped,
            stack_joins_abandoned,
            passes_folded,
            stack_refs_dropped,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        } = *stats;
        Self {
            lines_read,
            lines_truncated,
            lines_with_invalid_utf8,
            stack_block_lines_dropped,
            stack_joins_abandoned,
            passes_folded,
            stack_refs_dropped,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        }
    }
}

#[derive(Debug, Serialize)]
struct GameLogSummaryJson {
    /// Present only when several files were imported (one path keeps the
    /// single-file shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    patch_failures: Vec<PatchFailureJson>,
    extra_stack_traces: Vec<UnpairedStackTraceJson>,
    cross_references: Vec<CrossReferenceJson>,
    dds_failures: Vec<DdsFailureJson>,
    dependency_warnings: Vec<DependencyWarningJson>,
    timers: Vec<TimerJson>,
    def_cache_lines: Vec<String>,
    load_events: Vec<LoadEventJson>,
    load_events_disagree: bool,
    read_stats: ReadStatsJson,
    totals: TotalsJson,
    classes: Vec<ClassJson>,
    sentinels: SentinelsJson,
    kind: &'static str,
    coverage: CoverageJson,
    logging_gaps: Vec<LoggingGapJson>,
    /// Families whose count may be short because they span a logging gap.
    lower_bound_families: Vec<LowerBoundFamilyJson>,
}

fn summary_json(
    summary: &GameLogSummary,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    path: Option<&Path>,
) -> GameLogSummaryJson {
    GameLogSummaryJson {
        path: path.map(|path| path.display().to_string()),
        patch_failures: summary
            .patch_failures
            .iter()
            .map(|f| patch_failure_json(f, mods_by_id))
            .collect(),
        extra_stack_traces: summary
            .extra_stack_traces
            .iter()
            .map(|e| unpaired_stack_trace_json(e, mods_by_id))
            .collect(),
        cross_references: summary
            .cross_references
            .iter()
            .map(cross_reference_json)
            .collect(),
        dds_failures: summary
            .dds_failures
            .iter()
            .map(|d| dds_failure_json(d, mods_by_id))
            .collect(),
        dependency_warnings: summary
            .dependency_warnings
            .iter()
            .map(|w| dependency_warning_json(w, mods_by_id))
            .collect(),
        timers: summary
            .timers
            .iter()
            .map(|t| timer_json(t, mods_by_id))
            .collect(),
        def_cache_lines: summary.def_cache_lines.clone(),
        load_events: summary.load_events.iter().map(load_event_json).collect(),
        load_events_disagree: summary.load_events_disagree(),
        read_stats: (&summary.read_stats).into(),
        totals: summary.into(),
        classes: classes_json(summary, mods_by_id),
        sentinels: (&summary.sentinels).into(),
        kind: format_kind(summary.kind()),
        coverage: (&summary.coverage).into(),
        logging_gaps: logging_gaps_json(summary),
        lower_bound_families: lower_bound_families_json(summary),
    }
}

/// One object for a single path (the single-file shape), an array of objects
/// each carrying its `path` for several.
fn print_json(
    paths: &[PathBuf],
    summaries: &[GameLogSummary],
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> anyhow::Result<()> {
    let text = match (paths, summaries) {
        ([_], [summary]) => serde_json::to_string_pretty(&summary_json(summary, mods_by_id, None)),
        _ => {
            let objects: Vec<GameLogSummaryJson> = paths
                .iter()
                .zip(summaries)
                .map(|(path, summary)| summary_json(summary, mods_by_id, Some(path)))
                .collect();
            serde_json::to_string_pretty(&objects)
        }
    }
    .context("serializing the game log summary as JSON")?;
    println!("{text}");
    Ok(())
}
