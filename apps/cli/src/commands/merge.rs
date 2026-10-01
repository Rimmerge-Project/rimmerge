//! `rimmerge merge`: `plan` prints one finding's field-level diff and
//! merge plan; `coverage` tallies two things — how many contested patch
//! collisions the replay's xpath grammar can (and can't) replay (a dev
//! diagnostic for the "XPath coverage" risk), and def overrides by owner
//! count, structural-guard trigger, preview state, and current `Merge`-85
//! promotion rate. `run_coverage` is a thin renderer over
//! `rim_session::use_cases::MergeCoverage`, which owns both tallies —
//! this file holds no business logic of its own, per `CLAUDE.md`'s
//! layering rule (`apps/cli` is a composition root).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};
use rim_merge::diff::{DiffClass, StructuralField, Value};
use rim_merge::plan::PlanOp;
use rim_resolve::domain::{FindingKey, MergeState};
use rim_session::use_cases::{MergeCoverage, MergeCoverageReport, PlanMerge};
use serde::Serialize;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Subcommand)]
pub enum MergeCommand {
    /// Prints one finding's field-level diff and the merge plan it
    /// currently folds to.
    Plan(PlanArgs),
    /// Tallies contested patch collisions by whether their xpaths parse
    /// under Rimmerge's own xpath grammar (`Supported`) or not
    /// (`Unsupported`, grouped by reason), and — for the supported ones —
    /// by what an actual replay makes of them (`Agreeing` when every
    /// candidate order lands on the same value, `Conflict` when they
    /// differ).
    Coverage(CoverageArgs),
}

pub fn run(command: &MergeCommand) -> anyhow::Result<()> {
    match command {
        MergeCommand::Plan(args) => run_plan(args),
        MergeCommand::Coverage(args) => run_coverage(args),
    }
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// A finding key's canonical text form (see
    /// `rim_resolve::domain::FindingKey`'s `Display`), e.g.
    /// `def_override:ThingDef/Wall:[a.mod,b.mod]`.
    #[arg(long)]
    key: String,
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Absent => "<absent>".to_string(),
        Value::Leaf(text) => text.clone(),
        Value::Item(node) => rim_merge::xml::render_node(node, 0).trim_end().to_string(),
    }
}

fn join_ids(ids: &std::collections::BTreeSet<rim_analyzer::domain::ModId>) -> String {
    ids.iter()
        .map(rim_analyzer::domain::ModId::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_class(class: &DiffClass) -> String {
    match class {
        DiffClass::Unchanged => "unchanged".to_string(),
        DiffClass::OneSided { by } => format!("one_sided({by})"),
        DiffClass::Agreeing { by } => format!("agreeing({})", join_ids(by)),
        DiffClass::Conflict { by } => format!("CONFLICT({})", join_ids(by)),
    }
}

fn format_op(op: &PlanOp) -> String {
    let render =
        |node: &rim_merge::tree::FieldNode| rim_merge::xml::render_node(node, 0).trim().to_string();
    match op {
        PlanOp::Replace { path, node } => format!("Replace   {path}  <-  {}", render(node)),
        PlanOp::Add { parent, node } => format!("Add       under '{parent}'  <-  {}", render(node)),
        PlanOp::Remove { path } => format!("Remove    {path}"),
        PlanOp::ReplaceInheritFalse { path, node } => {
            format!("ReplaceInheritFalse  {path}  <-  {}", render(node))
        }
        PlanOp::SetAttribute { path, name, value } => {
            format!("SetAttribute  {path}.{name} = {value}")
        }
    }
}

fn run_plan(args: &PlanArgs) -> anyhow::Result<()> {
    let key: FindingKey = args
        .key
        .parse()
        .map_err(|e| anyhow::anyhow!("--key {:?}: {e}", args.key))?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let planner = PlanMerge::new(rim_io::FileDefSourceReader::new());
    let preview = planner
        .execute(&mut session, &key)
        .context("building the merge preview")?;

    println!("key:    {}", TerminalSafe::line(&preview.key));
    println!(
        "owners: {}  base={}  winner={}",
        TerminalSafe::line(
            preview
                .owners
                .iter()
                .map(rim_analyzer::domain::ModId::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TerminalSafe::line(&preview.base),
        TerminalSafe::line(&preview.winner)
    );
    match &preview.state {
        MergeState::Complete { op_count } => println!("state:  Complete ({op_count} op(s))"),
        MergeState::NeedsFieldInput { unresolved, total } => {
            println!("state:  NeedsFieldInput ({unresolved}/{total} field(s) unresolved)");
        }
        MergeState::CannotMerge { reason } => {
            println!("state:  CannotMerge — {}", TerminalSafe::line(reason));
        }
    }
    if let Some(change) = &preview.structural_change {
        println!(
            "guard:  {} differs ({} vs base) — confirm the winner; no field choice clears this",
            change.field,
            TerminalSafe::line(&change.by)
        );
    }

    println!();
    // The fourth column is
    // `winner candidate`, not `result` — it's `field.candidates`' entry
    // for `preview.winner` alone, which for a `PatchCollision` is always
    // `def_owner` (never one of the collision's own contributing mods —
    // see `rim_merge::plan::plan_patch_collision`'s own doc comment), so
    // this column reads `?` for almost every field of a collision. The
    // fifth, `final`, is the actual outcome: `preview.final_values`, the
    // full-order replay's own value for a `PatchCollision`
    // (`rim_merge::plan::PatchCollisionOutcome::final_values`) or the
    // merge's own resolved value for a `DefOverride`
    // (`rim_merge::plan::resolved_field_values`) — see
    // `MergePreview::final_values`'s own doc comment for why the two
    // kinds mean genuinely different things. A field absent from that map
    // has no meaningful final value at all (an unresolved conflict, an
    // invalid choice) and prints the marker below rather than silently
    // repeating `winner candidate`.
    println!(
        "{:<40} {:<22} {:<24} {:<24} final",
        "field", "class", "base", "winner candidate"
    );
    for field in &preview.diff.fields {
        if matches!(field.class, DiffClass::Unchanged) {
            continue;
        }
        // `.to_string()` each column before applying the outer `{:<N}`
        // width: `FieldPath`'s own `Display` impl (and `format_value`'s
        // plain `String` output) don't call `f.pad`, so a custom-Display
        // value passed directly to a width-specified `{}` ignores that
        // width entirely — only `str`/`String`'s own `Display` honors it.
        println!(
            "{:<40} {:<22} {:<24} {:<24} {}",
            TerminalSafe::line(&field.path).to_string(),
            TerminalSafe::line(format_class(&field.class)).to_string(),
            TerminalSafe::block(format_value(&field.base)).to_string(),
            TerminalSafe::block(
                field
                    .candidates
                    .get(&preview.winner)
                    .map(format_value)
                    .unwrap_or_else(|| "?".to_string())
            )
            .to_string(),
            TerminalSafe::block(
                preview
                    .final_values
                    .get(&field.path)
                    .map(format_value)
                    .unwrap_or_else(|| "<no final value>".to_string())
            )
        );
    }

    println!();
    println!(
        "plan ({} op(s), {} unresolved):",
        preview.plan.ops.len(),
        preview.plan.unresolved.len()
    );
    for planned in &preview.plan.ops {
        let gate = if planned.depends_on.is_empty() {
            String::new()
        } else {
            format!(
                "  [MayRequire={}]",
                TerminalSafe::line(join_ids(&planned.depends_on))
            )
        };
        println!("  {}{gate}", TerminalSafe::block(format_op(&planned.op)));
    }
    for path in &preview.plan.unresolved {
        println!("  UNRESOLVED  {}", TerminalSafe::line(path));
    }
    if !preview.plan.caveats.is_empty() {
        println!();
        println!("caveats:");
        for caveat in &preview.plan.caveats {
            println!("  {caveat:?}");
        }
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct CoverageArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Also write the coverage breakdown as JSON to this path.
    #[arg(long)]
    report: Option<PathBuf>,
}

/// The `--report` JSON shape. The patch-collision fields keep their
/// top-level names and meaning — `def_overrides` is a separate, nested
/// section, so a consumer reading only the top-level fields is unaffected
/// by it.
#[derive(Debug, Default, Serialize)]
struct CoverageReportJson {
    contested_collisions: usize,
    supported: usize,
    unsupported: usize,
    /// Of the `supported` ones, how many actually replay to an
    /// order-independent outcome (every candidate agrees, so the
    /// collision needs no decision) ...
    agreeing: usize,
    /// ... and how many to a real conflict the user has to resolve.
    conflict: usize,
    unsupported_reasons: BTreeMap<String, usize>,
    /// The post-redecision suggestion outcome for every
    /// contested collision that reached a real preview. `guarded` is
    /// always `0` here — the structural guard never applies to a `PatchCollision`.
    suggestion_outcomes: SuggestionOutcomesJson,
    def_overrides: DefOverrideReportJson,
}

/// [`rim_session::use_cases::SuggestionOutcomeTally`], transcribed —
/// `apps/cli` holds no `serde` dependency on `rim-session`'s own domain
/// types (this file's own established convention, e.g.
/// `DefOverrideReportJson` next to it).
#[derive(Debug, Default, Serialize)]
struct SuggestionOutcomesJson {
    /// What the redecision did — see
    /// `rim_session::use_cases::Redecision`'s own doc comment for why this
    /// is split from `inbox` below.
    accept_95: usize,
    accept_80: usize,
    guarded: usize,
    merge_85: usize,
    /// `redecide_for_clean_merge` returned the suggestion byte-for-byte
    /// unchanged (its own `offers_merge` gate never fired, or the state
    /// was `CannotMerge`).
    unchanged: usize,
    /// A real rewrite that isn't one of the three named outcomes above
    /// (today: the structural guard's guarded-rationale rewrite, and the ordinary
    /// lead-with-`Merge` `NeedsFieldInput` rewrite).
    other_rewrite: usize,
    /// Whether the *result*'s confidence clears the profile's own
    /// threshold — independent of which redecision bucket above a finding
    /// landed in.
    auto: usize,
    needs_input: usize,
}

impl From<&rim_session::use_cases::SuggestionOutcomeTally> for SuggestionOutcomesJson {
    fn from(tally: &rim_session::use_cases::SuggestionOutcomeTally) -> Self {
        Self {
            accept_95: tally.accept_95,
            accept_80: tally.accept_80,
            guarded: tally.guarded,
            merge_85: tally.merge_85,
            unchanged: tally.unchanged,
            other_rewrite: tally.other_rewrite,
            auto: tally.auto,
            needs_input: tally.needs_input,
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct DefOverrideReportJson {
    total: usize,
    by_owner_count: BTreeMap<usize, usize>,
    mod_versus_mod: usize,
    /// A finding whose own source couldn't be read at all — no preview
    /// exists for it, so it's absent from every field below.
    planning_failures: usize,
    planning_failure_reasons: BTreeMap<String, usize>,
    guard_fires: BTreeMap<String, usize>,
    /// Evaluated and genuinely clean — **not** the complement of
    /// `guard_fires`: see `guard_reread_failed`.
    guard_does_not_fire: usize,
    /// A previewed finding whose owner data couldn't be re-read for the
    /// guard check itself — neither a fire nor a does-not-fire.
    guard_reread_failed: usize,
    preview_complete_zero_ops: usize,
    preview_complete_with_ops: usize,
    preview_needs_field_input: usize,
    preview_cannot_merge: usize,
    preview_cannot_merge_reasons: BTreeMap<String, usize>,
    /// A ceiling, not a live inbox count — see
    /// `rim_session::use_cases::DefOverrideCoverage::promotes_to_merge_85`'s
    /// own doc comment for the real preconditions this tally doesn't
    /// check (no stored decision yet, `suggest_merge_when_clean`, 85
    /// meeting the profile's own threshold).
    promotes_to_merge_85: usize,
    /// The post-redecision suggestion outcome for every def
    /// override that reached a real preview. `suggestion_outcomes.merge_85`
    /// is the same count as `promotes_to_merge_85` above.
    suggestion_outcomes: SuggestionOutcomesJson,
}

/// A stable, short identifier for a [`StructuralField`] — used as both
/// the JSON key and the text table's own row label. Deliberately not
/// that type's own `Display` (worded as a full sentence for an editor
/// banner) or its derived `Debug` (never trusted for a
/// persisted/parseable value in this codebase — see `format_role`/
/// `format_row_intent`'s own convention in `commands/assign.rs`).
fn structural_field_key(field: StructuralField) -> &'static str {
    match field {
        StructuralField::ThingClass => "thing_class",
        StructuralField::ParentName => "parent_name",
        StructuralField::RootClass => "root_class",
        StructuralField::CompClass => "comp_class",
    }
}

fn to_json(report: &MergeCoverageReport) -> CoverageReportJson {
    let patch = &report.patch_collisions;
    let overrides = &report.def_overrides;
    CoverageReportJson {
        contested_collisions: patch.contested_collisions,
        supported: patch.supported,
        unsupported: patch.unsupported,
        agreeing: patch.agreeing,
        conflict: patch.conflict,
        unsupported_reasons: patch.unsupported_reasons.clone(),
        suggestion_outcomes: (&patch.suggestion_outcomes).into(),
        def_overrides: DefOverrideReportJson {
            total: overrides.total,
            by_owner_count: overrides.by_owner_count.clone(),
            mod_versus_mod: overrides.mod_versus_mod,
            planning_failures: overrides.planning_failures,
            planning_failure_reasons: overrides.planning_failure_reasons.clone(),
            guard_fires: overrides
                .guard_fires
                .iter()
                .map(|(field, count)| (structural_field_key(*field).to_string(), *count))
                .collect(),
            guard_does_not_fire: overrides.guard_does_not_fire,
            guard_reread_failed: overrides.guard_reread_failed,
            preview_complete_zero_ops: overrides.preview.complete_zero_ops,
            preview_complete_with_ops: overrides.preview.complete_with_ops,
            preview_needs_field_input: overrides.preview.needs_field_input,
            preview_cannot_merge: overrides.preview.cannot_merge,
            preview_cannot_merge_reasons: overrides.preview.cannot_merge_reasons.clone(),
            promotes_to_merge_85: overrides.promotes_to_merge_85,
            suggestion_outcomes: (&overrides.suggestion_outcomes).into(),
        },
    }
}

fn print_patch_collision_table(report: &MergeCoverageReport) {
    let patch = &report.patch_collisions;
    println!("contested patch collisions: {}", patch.contested_collisions);
    println!(
        "  supported:   {} ({:.1}%)",
        patch.supported,
        percent(patch.supported, patch.contested_collisions)
    );
    println!(
        "    agreeing:  {} ({:.1}% of supported)",
        patch.agreeing,
        percent(patch.agreeing, patch.supported)
    );
    println!(
        "    conflict:  {} ({:.1}% of supported)",
        patch.conflict,
        percent(patch.conflict, patch.supported)
    );
    println!(
        "  unsupported: {} ({:.1}%)",
        patch.unsupported,
        percent(patch.unsupported, patch.contested_collisions)
    );
    if !patch.unsupported_reasons.is_empty() {
        println!("  by reason:");
        let mut reasons: Vec<(&String, &usize)> = patch.unsupported_reasons.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons {
            println!("    {count:>4}  {reason}");
        }
    }
    println!("  post-redecision suggestion outcome (of every collision with a real preview):");
    print_suggestion_outcomes(&patch.suggestion_outcomes);
}

/// The suggestion-outcome table, shared by both `print_patch_collision_table`/
/// `print_def_override_table`. Two independent axes (see
/// `rim_session::use_cases::SuggestionOutcome`'s own doc comment for why
/// they are separate): what the redecision *did* (the first five lines,
/// percentaged against their own sum — the finding count that actually
/// reached both a preview and a ledger suggestion, since a planning
/// failure, or a finding the ledger never built an entry for, has neither
/// and is absent from every bucket) and whether the *result* clears the
/// inbox threshold (the last two lines, percentaged the same way — both
/// totals agree, since every bucketed finding lands in exactly one
/// redecision bucket and exactly one inbox bucket).
fn print_suggestion_outcomes(tally: &rim_session::use_cases::SuggestionOutcomeTally) {
    let redecision_total = tally.accept_95
        + tally.accept_80
        + tally.guarded
        + tally.merge_85
        + tally.unchanged
        + tally.other_rewrite;
    println!(
        "    accept @ 95:    {} ({:.1}%)",
        tally.accept_95,
        percent(tally.accept_95, redecision_total)
    );
    println!(
        "    accept @ 80:    {} ({:.1}%)",
        tally.accept_80,
        percent(tally.accept_80, redecision_total)
    );
    println!(
        "    guarded:        {} ({:.1}%)",
        tally.guarded,
        percent(tally.guarded, redecision_total)
    );
    println!(
        "    merge @ 85:     {} ({:.1}%)",
        tally.merge_85,
        percent(tally.merge_85, redecision_total)
    );
    println!(
        "    unchanged:      {} ({:.1}%)",
        tally.unchanged,
        percent(tally.unchanged, redecision_total)
    );
    println!(
        "    other rewrite:  {} ({:.1}%)",
        tally.other_rewrite,
        percent(tally.other_rewrite, redecision_total)
    );
    let inbox_total = tally.auto + tally.needs_input;
    println!(
        "    auto:           {} ({:.1}%)",
        tally.auto,
        percent(tally.auto, inbox_total)
    );
    println!(
        "    needs input:    {} ({:.1}%)",
        tally.needs_input,
        percent(tally.needs_input, inbox_total)
    );
}

fn print_def_override_table(report: &MergeCoverageReport) {
    let overrides = &report.def_overrides;
    println!();
    println!("def overrides: {}", overrides.total);
    println!(
        "  mod-versus-mod: {} ({:.1}%)",
        overrides.mod_versus_mod,
        percent(overrides.mod_versus_mod, overrides.total)
    );
    if !overrides.by_owner_count.is_empty() {
        println!("  by owner count:");
        for (owners, count) in &overrides.by_owner_count {
            println!("    {count:>4}  {owners} owner(s)");
        }
    }
    println!(
        "  planning failed (no preview built at all): {} ({:.1}%)",
        overrides.planning_failures,
        percent(overrides.planning_failures, overrides.total)
    );
    if !overrides.planning_failure_reasons.is_empty() {
        println!("    by reason:");
        let mut reasons: Vec<(&String, &usize)> =
            overrides.planning_failure_reasons.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons {
            println!("      {count:>4}  {reason}");
        }
    }
    // The guard buckets are denominated by `previewable` (`total` minus
    // `planning_failures`), never `total`: a finding with no preview has
    // no `ThreeWayDiff` for `structural_change` to read at all, so it can
    // never be a fire or a does-not-fire — only `previewable` findings
    // are ever guard-evaluated in the first place. `guard_fires`'s own
    // sum, plus `guard_does_not_fire`, plus `guard_reread_failed`, always
    // equals `previewable` exactly.
    let previewable = overrides.total - overrides.planning_failures;
    let guard_total: usize = overrides.guard_fires.values().sum();
    println!(
        "  structural guard fires: {guard_total} ({:.1}% of previewable)",
        percent(guard_total, previewable)
    );
    for (field, count) in &overrides.guard_fires {
        println!("    {count:>4}  {}", structural_field_key(*field));
    }
    println!(
        "  structural guard does not fire: {} ({:.1}% of previewable)",
        overrides.guard_does_not_fire,
        percent(overrides.guard_does_not_fire, previewable)
    );
    if overrides.guard_reread_failed > 0 {
        println!(
            "  structural guard re-read failed: {} ({:.1}% of previewable)",
            overrides.guard_reread_failed,
            percent(overrides.guard_reread_failed, previewable)
        );
    }
    println!("  preview state (of previewable):");
    println!(
        "    complete (0 ops):    {} ({:.1}%)",
        overrides.preview.complete_zero_ops,
        percent(overrides.preview.complete_zero_ops, previewable)
    );
    println!(
        "    complete (>0 ops):   {} ({:.1}%)",
        overrides.preview.complete_with_ops,
        percent(overrides.preview.complete_with_ops, previewable)
    );
    println!(
        "    needs field input:   {} ({:.1}%)",
        overrides.preview.needs_field_input,
        percent(overrides.preview.needs_field_input, previewable)
    );
    println!(
        "    cannot merge:        {} ({:.1}%)",
        overrides.preview.cannot_merge,
        percent(overrides.preview.cannot_merge, previewable)
    );
    if !overrides.preview.cannot_merge_reasons.is_empty() {
        println!("      by reason:");
        let mut reasons: Vec<(&String, &usize)> =
            overrides.preview.cannot_merge_reasons.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons {
            println!("        {count:>4}  {reason}");
        }
    }
    println!(
        "  currently promote to Merge at confidence 85: {} ({:.1}% of total)",
        overrides.promotes_to_merge_85,
        percent(overrides.promotes_to_merge_85, overrides.total)
    );
    println!(
        "    (a ceiling: also needs no stored decision yet, \
         suggest_merge_when_clean on, and 85 meeting this profile's own \
         threshold — none of which this count checks)"
    );
    println!("  post-redecision suggestion outcome (of every override with a real preview):");
    print_suggestion_outcomes(&overrides.suggestion_outcomes);
}

fn run_coverage(args: &CoverageArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let use_case = MergeCoverage::new(rim_io::FileDefSourceReader::new());
    let report = use_case.execute(&mut session);

    print_patch_collision_table(&report);
    print_def_override_table(&report);

    if let Some(report_path) = &args.report {
        let json = serde_json::to_string_pretty(&to_json(&report))
            .context("serializing coverage report")?;
        std::fs::write(report_path, json)
            .with_context(|| format!("writing {}", report_path.display()))?;
        println!("coverage report written to: {}", report_path.display());
    }
    Ok(())
}

fn percent(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (count as f64 / total as f64) * 100.0
    }
}
