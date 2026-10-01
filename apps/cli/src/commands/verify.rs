//! `rimmerge verify`: runs the `VerifyOrder` pass — a real replay of every
//! active patch operation against a chosen order, predicting which
//! operations RimWorld itself would log as failed — and prints the result.
//! Explicit and on-demand only: this is the *only* place in `apps/cli`
//! that ever calls `VerifyOrder::execute`, never through `sort`/`ledger`'s
//! cached ledger path. It is not cheap — on a large install, a single
//! order source takes on the order of a minute.
//!
//! **No `--enforce-soft`/`--enforce-awareness`/`--no-inferred` flags,
//! deliberately** (unlike `sort`/`ledger`, which are bare-JSON-in and
//! have no persisted session to read settings from): `verify` is a
//! live-session command, the same shape `apply`/`defs inspect` already
//! are — `--source suggested` already reflects whichever `EnforcedLayers`
//! the *session's own persisted* `rules.json` settings produce, the
//! exact order `apply` would actually write. A separate override flag
//! here would let this command's own printed "suggested" order silently
//! diverge from what `apply --source suggested` writes — a correctness
//! hazard, not a feature.
//!
//! **Grouped by `(mod, operation identity)`, not printed one row per
//! `Finding`**: `VerifyOrder`'s own architecture is per-def — a top-level
//! operation whose head matches several defNames (a common OR-list idiom:
//! `ThingDef[defName="A" or defName="B" or ...]`) is checked once per
//! matched def, producing one `Finding::PatchWillFail` each, even though
//! RimWorld's own log reports that operation's failure exactly once.
//! Printing raw `Finding`s one-per-line would reproduce that duplication
//! verbatim, rendering one real operation many times over. Grouping in
//! this presentation layer — never in `VerifyOrder` itself, which checks
//! per def and keeps that data, genuinely useful for diagnosis — collapses
//! each real operation back to one header, with every affected def target
//! listed beneath it.

use std::collections::BTreeMap;

use anyhow::Context;
use clap::{Args, ValueEnum};
use rim_analyzer::domain::{EdgeKind, EdgeStatus, Mod, ModId, Selector};
use rim_resolve::domain::{
    Action, DefKey, Finding, FindingKey, OrderSource, PatchFailureCause, ReorderKind, RuleOrigin,
};
use rim_resolve::ledger::{SuggestContext, suggest};
use rim_session::use_cases::{
    CounterfactualStats, ReorderConflictDirection, SelectOrder, VerifyOptions, VerifyOrder,
};
use serde::Serialize;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

/// The load-order change that would make one predicted failure succeed,
/// read off [`rim_resolve::ledger::suggest`]'s own `Reorder` alternative.
///
/// The direction is never re-derived here — `patch_will_fail`'s
/// `RemovedBy` arm asks for the *remover* to load after the failing mod,
/// its `NotYetInjected` arm for the failing mod to load after the
/// *injector*, and getting that backwards would print a
/// copy-pasteable command that makes the failure worse. Same reasoning,
/// and same one-`find_map`-over-`alternatives` shape, as
/// `apps/desktop/src-tauri/src/dto/verify.rs`'s own `reorder_of`: the two
/// interface layers each read `suggest` directly rather than sharing a
/// helper, because the only shared home for one would be a new
/// `crates/**` API for a one-line lookup.
///
/// **Rule of Three**: two copies is the tolerated duplication. A *third*
/// consumer of "the `Reorder` alternative for a `PatchWillFail`" is the
/// trigger to lift this into `rim_resolve::ledger` beside `suggest`
/// itself.
///
/// Whether the reorder itself *contradicts* something in the report
/// (`conflicts`, below) is computed by
/// `rim_session::use_cases::reorder_conflicts` — a `rim-session` function
/// both this file and `apps/desktop/src-tauri/src/dto/verify.rs` call,
/// not an independent copy of the same edge-direction lookup. Only the
/// `Reorder`-alternative lookup above is duplicated.
///
/// `ctx` is required by `suggest`'s own signature, not by this finding
/// kind: `patch_will_fail` reads none of the report, sort outcome,
/// current order or `mods_by_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reorder {
    after: ModId,
    before: ModId,
    rationale: String,
    /// A `UserDecision` pair rule already stored at this exact
    /// `(after, before)` key, if there is one — read off the session's
    /// own `rules.json`, not guessed. `upsert_rule` replaces a rule
    /// wholesale, so re-running the printed command would silently
    /// rewrite that rule's `comment` and clear its `overrides_declared`
    /// unless both are carried forward (see [`set_pair_command`]).
    existing: Option<ExistingUserPair>,
    /// Every edge in the session's own report this reorder would
    /// contradict — either it reverses an author declaration/already-
    /// satisfied fact, or it's the same direction as an edge the sorter's
    /// own `outcome.dropped` says it actually gave up to break a real cycle
    /// (never merely "violated" — see
    /// `rim_session::use_cases::reorder_conflicts`'s own doc comment).
    /// Empty when the reorder contradicts nothing.
    conflicts: Vec<rim_session::use_cases::ReorderConflict>,
}

/// See [`Reorder::existing`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExistingUserPair {
    overrides_declared: bool,
    has_comment: bool,
}

fn reorder_of(
    finding: &Finding,
    ctx: &SuggestContext<'_>,
    user_pairs: &BTreeMap<(ModId, ModId), ExistingUserPair>,
) -> Option<Reorder> {
    suggest(finding, ctx)
        .alternatives
        .into_iter()
        .find_map(|alternative| match alternative.action {
            Action::Reorder { after, before } => {
                let existing = user_pairs.get(&(after.clone(), before.clone())).cloned();
                let conflicts = rim_session::use_cases::reorder_conflicts(
                    ctx.report,
                    ctx.sort_outcome,
                    &after,
                    &before,
                );
                Some(Reorder {
                    after,
                    before,
                    rationale: alternative.rationale.to_string(),
                    existing,
                    conflicts,
                })
            }
            _ => None,
        })
}

/// **The shell contract for every printed `rule set-pair` line: single
/// quotes, which are literal in both POSIX `sh` and PowerShell.**
///
/// Mod ids come from an untrusted `About.xml` `<packageId>` and the
/// rationale interpolates them, so neither can be pasted into a command
/// line bare — a space, a `$`, a `` ` ``, a `;` or a `&` would all change
/// what the shell runs. A single-quoted string is inert in both shells,
/// so one quoted form covers both and no second PowerShell variant is
/// needed.
///
/// The one value this cannot cover is a string containing a single quote
/// itself: POSIX escapes it `'\''`, PowerShell doubles it `''`, and no
/// single spelling is correct in both. Rather than print a line that is
/// safe in one shell and broken (or, worse, *executable* as something
/// else) in the other, this returns `None` and the caller prints the
/// relation in prose instead. Control characters are refused for the same
/// reason.
fn shell_quote(value: &str) -> Option<String> {
    if value.contains('\'') || value.chars().any(char::is_control) {
        return None;
    }
    Some(format!("'{value}'"))
}

/// The exact, copy-pasteable invocation that turns one predicted
/// order-caused failure into a `user_decision` pair rule. `None` when a
/// value cannot be quoted safely (see [`shell_quote`]).
///
/// Deliberately carries no `--profile-dir`/`--game-dir`: those are
/// whatever the reader already passed to `verify`, and echoing a possibly
/// long scratch path into every row would bury the part that matters.
///
/// `--override-declared` is re-emitted when a user rule at this key
/// already carries it, because `rule set-pair`'s own doc comment is
/// explicit that omitting the flag *clears* it — a printed command that
/// silently demoted a rule out of `Layer::DeclaredOverride` would be
/// worse than no command at all.
fn set_pair_command(reorder: &Reorder) -> Option<String> {
    let after = shell_quote(reorder.after.as_str())?;
    let before = shell_quote(reorder.before.as_str())?;
    let comment = shell_quote(&reorder.rationale)?;
    let mut command = format!("rimmerge rule set-pair --after {after} --before {before}");
    if reorder
        .existing
        .as_ref()
        .is_some_and(|existing| existing.overrides_declared)
    {
        command.push_str(" --override-declared");
    }
    command.push_str(&format!(" --comment {comment}"));
    Some(command)
}

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

/// The spelling `verify` prints and puts in `--json` for an edge kind
/// (`LoadAfter`, `AssemblyRef`, ...): a contract of its own, independent of
/// the type's derived `Debug`. The match is exhaustive, so a new kind is a
/// compile error here rather than a silently different word.
fn format_edge_kind(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::AssemblyRef => "AssemblyRef",
        EdgeKind::ForceLoadAfter => "ForceLoadAfter",
        EdgeKind::ForceLoadBefore => "ForceLoadBefore",
        EdgeKind::LoadAfter => "LoadAfter",
        EdgeKind::LoadBefore => "LoadBefore",
        EdgeKind::ModDependency => "ModDependency",
        EdgeKind::FindMod => "FindMod",
        EdgeKind::IfModActive => "IfModActive",
        EdgeKind::PatchTargetsDef => "PatchTargetsDef",
        EdgeKind::MayRequire => "MayRequire",
        EdgeKind::PatchInjectedNode => "PatchInjectedNode",
        EdgeKind::AssemblyVersionPrecedence => "AssemblyVersionPrecedence",
        EdgeKind::UsesType => "UsesType",
        EdgeKind::ParentTemplate => "ParentTemplate",
        EdgeKind::PatchRemovedNode => "PatchRemovedNode",
        EdgeKind::RetextureAfterOwner => "RetextureAfterOwner",
        EdgeKind::DefOverrideAfterOrigin => "DefOverrideAfterOrigin",
        EdgeKind::PatchSelectsInjectedNode => "PatchSelectsInjectedNode",
        EdgeKind::PatchInvalidatesPredicate => "PatchInvalidatesPredicate",
        EdgeKind::PatchRemovedNodeCosmetic => "PatchRemovedNodeCosmetic",
        EdgeKind::ReplaceDiscardsAddition => "ReplaceDiscardsAddition",
    }
}

/// See [`format_edge_kind`].
fn format_edge_status(status: EdgeStatus) -> &'static str {
    match status {
        EdgeStatus::Satisfied => "Satisfied",
        EdgeStatus::Violated => "Violated",
        EdgeStatus::Unevaluated => "Unevaluated",
    }
}

/// See [`format_edge_kind`].
fn format_conflict_direction(direction: ReorderConflictDirection) -> &'static str {
    match direction {
        ReorderConflictDirection::Reverses => "Reverses",
        ReorderConflictDirection::ReAsserts => "ReAsserts",
    }
}

fn format_source(source: OrderSource) -> &'static str {
    match source {
        OrderSource::Current => "current",
        OrderSource::Suggested => "suggested",
    }
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Which order to check every active patcher's own operations
    /// against — `suggested` (the default) is the order `apply` would
    /// actually write without an explicit `--source current`.
    #[arg(long, value_enum, default_value_t = SourceArg::Suggested)]
    source: SourceArg,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
    /// Skip the counterfactual phase — the extra replay that asks
    /// whether a different load order would have made each
    /// otherwise-unexplained failure succeed. On by default; this flag
    /// exists so before/after numbers come from one binary, not so the
    /// pass is routinely run without it.
    #[arg(long)]
    no_counterfactual: bool,
}

pub fn run(args: &VerifyArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    let source: OrderSource = args.source.into();
    SelectOrder::new().execute(&mut session, source);

    let verify = VerifyOrder::new(rim_io::FileDefSourceReader::new());
    let report = verify.execute_with_options(
        &session,
        source,
        VerifyOptions {
            counterfactual: !args.no_counterfactual,
        },
    );

    let mods_by_id: BTreeMap<ModId, &Mod> = session
        .report()
        .mods
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    // The same `SuggestContext` the ledger builds,
    // so each predicted failure's own `Reorder` alternative (and the
    // `rule set-pair` line printed from it) is read off
    // `rim_resolve::ledger::suggest` rather than restated here.
    // `mods_by_id` is required by the signature, not by this finding
    // kind — `suggest`'s `PatchWillFail` arm reads none of the context.
    let ctx = SuggestContext {
        report: session.report(),
        sort_outcome: session.sort_outcome(),
        current: &session.orders().current,
        mods_by_id: &mods_by_id,
    };
    // `upsert_rule` replaces a rule
    // wholesale, so the printed command has to know whether a user rule
    // already sits at this key before telling anyone to run it.
    let user_pairs: BTreeMap<(ModId, ModId), ExistingUserPair> = session
        .rules()
        .pairs
        .iter()
        .filter(|pair| pair.origin == RuleOrigin::UserDecision)
        .map(|pair| {
            (
                (pair.after.clone(), pair.before.clone()),
                ExistingUserPair {
                    overrides_declared: pair.overrides_declared,
                    has_comment: pair
                        .comment
                        .as_ref()
                        .is_some_and(|comment| !comment.trim().is_empty()),
                },
            )
        })
        .collect();

    if args.json {
        return print_json(&report, &mods_by_id, &ctx, &user_pairs);
    }
    print_text(&report, source, &mods_by_id, &ctx, &user_pairs);
    Ok(())
}

/// A mod's own display name, falling back to its raw id when the scan
/// somehow doesn't have one (should not happen for an active mod, but
/// this command never panics on a report inconsistency).
fn mod_name<'a>(mods_by_id: &'a BTreeMap<ModId, &'a Mod>, mod_id: &'a ModId) -> &'a str {
    mods_by_id
        .get(mod_id)
        .map(|m| m.name.as_str())
        .unwrap_or_else(|| mod_id.as_str())
}

fn format_cause(cause: &PatchFailureCause) -> String {
    match cause {
        PatchFailureCause::RemovedBy(mod_id) => format!("RemovedBy({mod_id})"),
        PatchFailureCause::NotYetInjected(mod_id) => format!("NotYetInjected({mod_id})"),
        PatchFailureCause::DeadTarget => "DeadTarget".to_string(),
        PatchFailureCause::Unknown => "Unknown".to_string(),
    }
}

/// One def target a grouped operation's own failure was predicted
/// against — [`group_by_operation`]'s own per-def data, kept in full for
/// diagnosis.
#[derive(Debug)]
struct GroupedDef<'a> {
    def_key: &'a DefKey,
    selector: Selector,
    leaf_xpath: &'a Option<String>,
    cause: &'a PatchFailureCause,
    /// Whether this row's own reorder-bearing cause would change the
    /// final resolved def — `None` for `DeadTarget`/`Unknown`, which
    /// carry no reorder at all. See [`ReorderKind`].
    reorder_kind: &'a Option<ReorderKind>,
    /// The `Reorder` alternative this row's own suggestion carries, if
    /// any — `None` for `DeadTarget`/`Unknown`, and **also** `None` for
    /// a [`ReorderKind::Cosmetic`] row: never offers `set-pair` for a
    /// fix that wouldn't change anything RimWorld actually loads.
    reorder: Option<Reorder>,
}

/// Every `Finding::PatchWillFail` sharing one real `(mod, operation
/// identity)` pair, collapsed to a single entry with its affected def
/// targets kept underneath — never discarded, just not repeated as the
/// identical headline once per def.
#[derive(Debug)]
struct GroupedOperation<'a> {
    mod_id: &'a ModId,
    operation: &'a str,
    defs: Vec<GroupedDef<'a>>,
}

/// Groups `findings` by `(mod_id, operation)` — see this module's own
/// doc comment for why. Key order (`BTreeMap<(ModId, &str), _>`) is
/// deterministic and load-order-then-alphabetical, matching this
/// codebase's own determinism convention (no `HashMap` reaching output).
///
/// Grouping depends on [`rim_merge::patch_eval::operation_identity`]
/// telling operations apart: it recurses into a custom sequence-like
/// class's own children, not only the literal `PatchOperationSequence`
/// class, so a sequence-wrapper mod's distinct failing operations keep
/// distinct identities here instead of sharing one headline. Both halves
/// are needed — grouping alone would merge genuinely distinct failures
/// into what looks like one.
fn group_by_operation<'a>(
    findings: &'a [Finding],
    ctx: &SuggestContext<'_>,
    user_pairs: &BTreeMap<(ModId, ModId), ExistingUserPair>,
) -> Vec<GroupedOperation<'a>> {
    let mut groups: BTreeMap<(ModId, &str), GroupedOperation<'_>> = BTreeMap::new();
    for finding in findings {
        let Finding::PatchWillFail {
            mod_id,
            def_key,
            selector,
            operation,
            leaf_xpath,
            cause,
            reorder_kind,
        } = finding
        else {
            continue;
        };
        // A cosmetic row never offers `set-pair`: the fix wouldn't
        // change anything RimWorld actually loads, only which op's own
        // failure it would log.
        let reorder = match reorder_kind {
            Some(ReorderKind::Cosmetic { .. }) => None,
            _ => reorder_of(finding, ctx, user_pairs),
        };
        groups
            .entry((mod_id.clone(), operation.as_str()))
            .or_insert_with(|| GroupedOperation {
                mod_id,
                operation,
                defs: Vec::new(),
            })
            .defs
            .push(GroupedDef {
                def_key,
                selector: *selector,
                leaf_xpath,
                cause,
                reorder_kind,
                reorder,
            });
    }
    groups.into_values().collect()
}

fn print_text(
    report: &rim_session::use_cases::VerifyOrderReport,
    source: OrderSource,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    ctx: &SuggestContext<'_>,
    user_pairs: &BTreeMap<(ModId, ModId), ExistingUserPair>,
) {
    let mut removed_by = 0usize;
    let mut not_yet_injected = 0usize;
    let mut dead_target = 0usize;
    let mut unknown = 0usize;
    for finding in &report.findings {
        let Finding::PatchWillFail { cause, .. } = finding else {
            continue;
        };
        match cause {
            PatchFailureCause::RemovedBy(_) => removed_by += 1,
            PatchFailureCause::NotYetInjected(_) => not_yet_injected += 1,
            PatchFailureCause::DeadTarget => dead_target += 1,
            PatchFailureCause::Unknown => unknown += 1,
        }
    }

    let operations = group_by_operation(&report.findings, ctx, user_pairs);

    println!(
        "VerifyOrder ({}): {} defs checked, {} skipped",
        format_source(source),
        report.defs_checked,
        report.skipped.len()
    );
    println!(
        "  {} operations predicted to fail across {} def targets",
        operations.len(),
        report.findings.len()
    );
    println!(
        "  RemovedBy: {removed_by}  NotYetInjected: {not_yet_injected}  DeadTarget: {dead_target}  Unknown: {unknown}"
    );
    print_counterfactual_text(&report.counterfactual);
    if report.suppressed_filter_head_ops > 0 {
        println!(
            "  {} operation replays matched no node under a filter-predicate xpath head and were not predicted",
            report.suppressed_filter_head_ops
        );
    }
    println!();

    // Leads with the exact text RimWorld's own `Player.log` shows for a
    // failed top-level operation, so a line here can be grepped straight
    // against a real log — the whole point of `Finding::PatchWillFail::operation`.
    // One header per real operation (see this module's own doc comment),
    // every affected def target listed beneath it.
    for group in &operations {
        println!(
            "[{}] Patch operation {} failed ({} def target{})",
            TerminalSafe::line(mod_name(mods_by_id, group.mod_id)),
            TerminalSafe::line(group.operation),
            group.defs.len(),
            if group.defs.len() == 1 { "" } else { "s" }
        );
        for def in &group.defs {
            print!(
                "    -> {}, cause: {}",
                TerminalSafe::line(def.def_key),
                TerminalSafe::line(format_cause(def.cause))
            );
            if let Some(leaf) = def.leaf_xpath {
                print!(", field: {}", TerminalSafe::line(leaf));
            }
            println!();
        }
        // The copy-pasteable command that states this relation as a
        // `user_decision` pair rule. Printed **once per
        // distinct reorder** under the group, not once per def target —
        // an `OR`-list head matching 19 defs blocked by the same mod is
        // one relation to state, and printing the identical command 19
        // times is the same duplication this command's own grouping
        // exists to remove.
        let mut printed: Vec<&Reorder> = Vec::new();
        for reorder in group.defs.iter().filter_map(|def| def.reorder.as_ref()) {
            if printed.iter().any(|seen| **seen == *reorder) {
                continue;
            }
            printed.push(reorder);
            match set_pair_command(reorder) {
                Some(command) => println!("    fix: {command}"),
                // See `shell_quote`: a value carrying a single quote has
                // no spelling that is correct in both POSIX sh and
                // PowerShell, so the relation is stated in prose rather
                // than as a line that is safe in only one of them.
                None => println!(
                    "    fix: load {} before {} — not printed as a command: a mod id or its \
                     rationale contains a quote or control character, so quote the arguments by \
                     hand",
                    TerminalSafe::line(&reorder.before),
                    TerminalSafe::line(&reorder.after)
                ),
            }
            for conflict in &reorder.conflicts {
                let direction = match conflict.direction {
                    ReorderConflictDirection::Reverses => "reverses it",
                    ReorderConflictDirection::ReAsserts => {
                        "re-asserts a direction the sorter already dropped"
                    }
                };
                println!(
                    "    conflicts with {} ({direction}): {} ({})",
                    format_edge_kind(conflict.kind),
                    TerminalSafe::line(&conflict.detail),
                    format_edge_status(conflict.status)
                );
            }
            if let Some(existing) = &reorder.existing {
                // `upsert_rule` replaces a rule wholesale. The
                // command above carries `--override-declared` forward on
                // its own; the comment cannot be carried forward without
                // reprinting the user's own text, so it is disclosed.
                if existing.has_comment {
                    println!(
                        "    note: a user pair rule already exists for this pair — running the \
                         line above replaces its comment with the one shown"
                    );
                } else {
                    println!("    note: a user pair rule already exists for this pair");
                }
            }
        }
        // The cosmetic fallback: printed once per distinct
        // existing-merge target under the group, same dedup shape as the
        // `fix:` loop above — a cosmetic row never gets a `set-pair`
        // line, so this is its own, separate notice.
        let mut printed_cosmetic: Vec<&Option<FindingKey>> = Vec::new();
        for reorder_kind in group
            .defs
            .iter()
            .filter_map(|def| def.reorder_kind.as_ref())
        {
            let ReorderKind::Cosmetic { existing_merge } = reorder_kind else {
                continue;
            };
            if printed_cosmetic
                .iter()
                .any(|seen| **seen == *existing_merge)
            {
                continue;
            }
            printed_cosmetic.push(existing_merge);
            println!("    cosmetic: the same final def either order — no set-pair rule offered");
            match existing_merge {
                None => println!("    merge: no PatchCollision finding covers this target"),
                Some(key) => match shell_quote(&key.to_string()) {
                    Some(quoted) => println!("    merge: rimmerge merge plan --key {quoted}"),
                    None => println!(
                        "    merge: an existing PatchCollision finding covers this target — not \
                         printed as a command: its key contains a quote or control character"
                    ),
                },
            }
        }
    }

    if !report.skipped.is_empty() {
        println!();
        println!("skipped (could not check):");
        for (def_key, selector, reason) in &report.skipped {
            println!(
                "  {} ({}): {}",
                TerminalSafe::line(def_key),
                selector_str(*selector),
                TerminalSafe::line(reason)
            );
        }
    }
}

/// The machine-readable half of the `fix:` line the
/// text output prints — `after`/`before` straight off
/// `rim_resolve::ledger::suggest`'s own `Reorder` alternative, plus the
/// exact command, so a measurement run can grep one line rather than
/// reassembling the invocation from three fields.
#[derive(Debug, Serialize)]
struct VerifyReorderJson {
    after: String,
    before: String,
    rationale: String,
    /// `null` when a value could not be quoted safely for both POSIX sh
    /// and PowerShell — see [`shell_quote`].
    set_pair_command: Option<String>,
    /// Whether a `user_decision` pair rule already sits at this key, and
    /// what running the command would overwrite.
    existing_user_rule: bool,
    existing_overrides_declared: bool,
    existing_has_comment: bool,
    /// Every report edge this reorder would contradict. Empty when there
    /// are none.
    conflicts: Vec<VerifyReorderConflictJson>,
}

/// One [`rim_session::use_cases::ReorderConflict`], transcribed —
/// mirrors [`VerifyReorderJson`]'s own "no domain `Serialize`" rule.
#[derive(Debug, Serialize)]
struct VerifyReorderConflictJson {
    kind: String,
    detail: String,
    status: String,
    direction: String,
}

impl From<&rim_session::use_cases::ReorderConflict> for VerifyReorderConflictJson {
    fn from(value: &rim_session::use_cases::ReorderConflict) -> Self {
        Self {
            kind: format_edge_kind(value.kind).to_string(),
            detail: value.detail.clone(),
            status: format_edge_status(value.status).to_string(),
            direction: format_conflict_direction(value.direction).to_string(),
        }
    }
}

impl From<&Reorder> for VerifyReorderJson {
    fn from(value: &Reorder) -> Self {
        Self {
            after: value.after.to_string(),
            before: value.before.to_string(),
            rationale: value.rationale.clone(),
            set_pair_command: set_pair_command(value),
            existing_user_rule: value.existing.is_some(),
            existing_overrides_declared: value
                .existing
                .as_ref()
                .is_some_and(|existing| existing.overrides_declared),
            existing_has_comment: value
                .existing
                .as_ref()
                .is_some_and(|existing| existing.has_comment),
            conflicts: value.conflicts.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct VerifyDefJson {
    def_type: String,
    def_name: String,
    selector: String,
    leaf_xpath: Option<String>,
    cause: String,
    reorder: Option<VerifyReorderJson>,
    /// `"content"` or `"cosmetic"`, `null` when `cause` offers no
    /// reorder at all (`DeadTarget`/`Unknown`). See
    /// `rim_resolve::domain::ReorderKind`.
    reorder_kind: Option<&'static str>,
    /// A `"cosmetic"` row's own existing `PatchCollision` finding key
    /// (canonical text form, `rimmerge merge plan --key`-ready), if the
    /// ledger has one — always `null` for a `"content"` row.
    cosmetic_merge_key: Option<String>,
}

fn reorder_kind_str(reorder_kind: &Option<ReorderKind>) -> Option<&'static str> {
    match reorder_kind {
        Some(ReorderKind::Content) => Some("content"),
        Some(ReorderKind::Cosmetic { .. }) => Some("cosmetic"),
        None => None,
    }
}

fn cosmetic_merge_key(reorder_kind: &Option<ReorderKind>) -> Option<String> {
    match reorder_kind {
        Some(ReorderKind::Cosmetic { existing_merge }) => {
            existing_merge.as_ref().map(ToString::to_string)
        }
        _ => None,
    }
}

#[derive(Debug, Serialize)]
struct VerifyOperationJson {
    mod_id: String,
    mod_name: String,
    operation: String,
    defs: Vec<VerifyDefJson>,
}

#[derive(Debug, Serialize)]
struct VerifySkippedJson {
    def_type: String,
    def_name: String,
    selector: String,
    reason: String,
}

/// The counterfactual phase's summary line. Printed whenever
/// the phase has anything at all to report — **not** only when it ran a
/// job: a run where every candidate was refused (co-owner, cap, not
/// replayable) has `jobs: 0` and is exactly the run whose reason for
/// doing nothing a reader most needs to see. A `--no-counterfactual`
/// run leaves every counter zero and so prints nothing, keeping its
/// output byte-identical to a run without the phase.
fn print_counterfactual_text(stats: &CounterfactualStats) {
    let skipped = stats.skipped_too_many_mods
        + stats.skipped_too_many_subjects
        + stats.skipped_job_ceiling
        + stats.skipped_not_replayable
        + stats.skipped_co_owner
        + stats.skipped_subject_not_found;
    if stats.jobs == 0 && skipped == 0 && stats.mods_per_def.is_empty() {
        return;
    }
    println!(
        "  counterfactual: {} jobs, {} Unknown resolved, {} DeadTarget demoted          ({} alternatives replayed, {} rejected for regression, {} for truncation,          {} ties, {} non-contiguous refusals)",
        stats.jobs,
        stats.resolved,
        stats.demoted_dead_targets,
        stats.attempts,
        stats.rejected_for_regression,
        stats.rejected_for_truncation,
        stats.ties,
        stats.refused_non_contiguous
    );
    if skipped > 0 {
        println!(
            "  counterfactual skipped: {} too many mods, {} too many subjects per def,              {} over the job ceiling, {} not replayable, {} co-owner of the def,              {} subject not found",
            stats.skipped_too_many_mods,
            stats.skipped_too_many_subjects,
            stats.skipped_job_ceiling,
            stats.skipped_not_replayable,
            stats.skipped_co_owner,
            stats.skipped_subject_not_found
        );
    }
    if !stats.mods_per_def.is_empty() {
        let distribution = stats
            .mods_per_def
            .iter()
            .map(|(mods, defs)| format!("K={mods}: {defs}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  counterfactual K distribution: {distribution}");
    }
}

/// Mirrors [`CounterfactualStats`] field for field — `jobs`, `resolved`,
/// `demoted_dead_targets`, and the skip and rejection counters are the
/// headline numbers; every other counter is here too because each one is
/// a stated claim (the caps, the refusals, the `K` distribution the caps
/// assume) that a measurement run has to read somewhere.
#[derive(Debug, Serialize)]
struct VerifyCounterfactualJson {
    jobs: usize,
    resolved: usize,
    demoted_dead_targets: usize,
    skipped_too_many_mods: usize,
    skipped_too_many_subjects: usize,
    skipped_job_ceiling: usize,
    skipped_not_replayable: usize,
    skipped_co_owner: usize,
    skipped_subject_not_found: usize,
    attempts: usize,
    rejected_for_regression: usize,
    rejected_for_truncation: usize,
    ties: usize,
    refused_non_contiguous: usize,
    /// `K` (distinct patcher mods on a candidate def) -> how many defs.
    mods_per_def: BTreeMap<usize, usize>,
}

impl From<&CounterfactualStats> for VerifyCounterfactualJson {
    fn from(value: &CounterfactualStats) -> Self {
        Self {
            jobs: value.jobs,
            resolved: value.resolved,
            demoted_dead_targets: value.demoted_dead_targets,
            skipped_too_many_mods: value.skipped_too_many_mods,
            skipped_too_many_subjects: value.skipped_too_many_subjects,
            skipped_job_ceiling: value.skipped_job_ceiling,
            skipped_not_replayable: value.skipped_not_replayable,
            skipped_co_owner: value.skipped_co_owner,
            skipped_subject_not_found: value.skipped_subject_not_found,
            attempts: value.attempts,
            rejected_for_regression: value.rejected_for_regression,
            rejected_for_truncation: value.rejected_for_truncation,
            ties: value.ties,
            refused_non_contiguous: value.refused_non_contiguous,
            mods_per_def: value.mods_per_def.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct VerifyReportJson {
    source: String,
    defs_checked: usize,
    operations_total: usize,
    def_targets_total: usize,
    operations: Vec<VerifyOperationJson>,
    skipped: Vec<VerifySkippedJson>,
    counterfactual: VerifyCounterfactualJson,
    /// See `VerifyOrderReport::suppressed_filter_head_ops` — reported so
    /// the decision to lift that conservatism rests on a measured number.
    suppressed_filter_head_ops: usize,
}

fn selector_str(selector: rim_analyzer::domain::Selector) -> &'static str {
    match selector {
        rim_analyzer::domain::Selector::DefName => "defName",
        rim_analyzer::domain::Selector::NameAttr => "nameAttr",
    }
}

fn print_json(
    report: &rim_session::use_cases::VerifyOrderReport,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    ctx: &SuggestContext<'_>,
    user_pairs: &BTreeMap<(ModId, ModId), ExistingUserPair>,
) -> anyhow::Result<()> {
    let grouped = group_by_operation(&report.findings, ctx, user_pairs);
    let operations = grouped
        .iter()
        .map(|group| VerifyOperationJson {
            mod_id: group.mod_id.to_string(),
            mod_name: mod_name(mods_by_id, group.mod_id).to_string(),
            operation: group.operation.to_string(),
            defs: group
                .defs
                .iter()
                .map(|def| VerifyDefJson {
                    def_type: def.def_key.def_type.clone(),
                    def_name: def.def_key.def_name.clone(),
                    selector: selector_str(def.selector).to_string(),
                    leaf_xpath: def.leaf_xpath.clone(),
                    cause: format_cause(def.cause),
                    reorder: def.reorder.as_ref().map(VerifyReorderJson::from),
                    reorder_kind: reorder_kind_str(def.reorder_kind),
                    cosmetic_merge_key: cosmetic_merge_key(def.reorder_kind),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let skipped = report
        .skipped
        .iter()
        .map(|(def_key, selector, reason)| VerifySkippedJson {
            def_type: def_key.def_type.clone(),
            def_name: def_key.def_name.clone(),
            selector: selector_str(*selector).to_string(),
            reason: reason.clone(),
        })
        .collect();
    let json = VerifyReportJson {
        source: format_source(report.source).to_string(),
        defs_checked: report.defs_checked,
        operations_total: operations.len(),
        def_targets_total: report.findings.len(),
        operations,
        skipped,
        counterfactual: (&report.counterfactual).into(),
        suppressed_filter_head_ops: report.suppressed_filter_head_ops,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json).context("serializing verify report")?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reorder(after: &str, before: &str, rationale: &str) -> Reorder {
        Reorder {
            after: ModId::new(after),
            before: ModId::new(before),
            rationale: rationale.to_string(),
            existing: None,
            conflicts: Vec::new(),
        }
    }

    #[test]
    fn set_pair_command_single_quotes_every_interpolated_value() {
        let command = set_pair_command(&reorder("a.mod", "b.mod", "because")).expect("quotable");
        assert_eq!(
            command,
            "rimmerge rule set-pair --after 'a.mod' --before 'b.mod' --comment 'because'"
        );
    }

    /// A `packageId` is untrusted
    /// `About.xml` text. Pasted bare, `$(...)`/`` ` ``/`;`/`&`/a space
    /// would each change what the shell runs; wrapped in single quotes
    /// they are inert in both POSIX `sh` and PowerShell.
    #[test]
    fn set_pair_command_neutralizes_a_hostile_mod_id() {
        // `ModId::new` lowercases (package ids are case-insensitive), so
        // the expectation below is the lowercased form — the point is the
        // metacharacters, every one of which survives into the id.
        let hostile = "evil mod; rm -rf /$HOME `whoami` $(id) & echo";
        let command =
            set_pair_command(&reorder(hostile, "b.mod", "why")).expect("no quote in the value");
        assert_eq!(
            command,
            "rimmerge rule set-pair --after 'evil mod; rm -rf /$home `whoami` $(id) & echo' \
             --before 'b.mod' --comment 'why'"
        );
        // Everything hostile sits strictly between the single quotes —
        // the only characters outside them are the literal flag text.
        let outside: String = command.split('\'').step_by(2).collect();
        assert!(
            !outside.contains('$')
                && !outside.contains('`')
                && !outside.contains(';')
                && !outside.contains('&'),
            "shell metacharacters escaped the quoting: {outside}"
        );
    }

    /// The one value the single-quote contract cannot cover: POSIX
    /// escapes an embedded `'` as `'\''`, PowerShell doubles it, and no
    /// single spelling is right in both — so no command is printed at all.
    #[test]
    fn set_pair_command_refuses_a_value_carrying_a_single_quote_or_a_control_character() {
        assert_eq!(
            set_pair_command(&reorder("it's.a.mod", "b.mod", "why")),
            None
        );
        assert_eq!(set_pair_command(&reorder("a.mod", "b'mod", "why")), None);
        assert_eq!(
            set_pair_command(&reorder("a.mod", "b.mod", "why\nrm -rf /")),
            None
        );
    }

    /// `rule set-pair` *clears* `--override-declared` when the flag is
    /// omitted, so a printed command that dropped it would silently demote
    /// an existing rule out of `Layer::DeclaredOverride`.
    #[test]
    fn set_pair_command_carries_an_existing_override_declared_flag_forward() {
        let mut with_override = reorder("a.mod", "b.mod", "because");
        with_override.existing = Some(ExistingUserPair {
            overrides_declared: true,
            has_comment: true,
        });
        assert_eq!(
            set_pair_command(&with_override).expect("quotable"),
            "rimmerge rule set-pair --after 'a.mod' --before 'b.mod' --override-declared \
             --comment 'because'"
        );

        let mut without_override = reorder("a.mod", "b.mod", "because");
        without_override.existing = Some(ExistingUserPair {
            overrides_declared: false,
            has_comment: false,
        });
        assert!(
            !set_pair_command(&without_override)
                .expect("quotable")
                .contains("--override-declared"),
            "an existing rule without the flag must not gain one"
        );
    }
}
