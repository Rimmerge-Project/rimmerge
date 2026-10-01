//! `rimmerge rule`: creates or replaces a user-owned rule directly.
//!
//! `set-placement`: pins a mod to the top or bottom of the load order as
//! your own (`UserDecision`-origin) decision. `promote.rs`'s own
//! `--placement` only ever *copies* an already-*imported* placement rule
//! (`RimSortUser`/`RimSortCommunity`/`SteamDb`); this command creates a
//! brand-new placement pin from scratch, headlessly. The desktop's own
//! `upsert_rule` Tauri command exposes the identical
//! `rim_session::use_cases::UpsertRule` use case this wraps, but its
//! `RulesPage`/`RuleTable.vue` only list, delete, and promote existing
//! rows — there is no "add a new rule" form there.
//!
//! `set-pair`/`remove-pair`: the pair-rule sibling of `set-placement`, a
//! supported way for a user to state "A before B" for a pair no rule
//! database mentions (for example, to settle a mutual `patch_removed_node`
//! 2-cycle the sorter would otherwise break alphabetically).
//! `RuleSet`/`Session::upsert_rule`/`delete_rule` store and evaluate a
//! `UserDecision`-origin `PairRule` exactly like a promoted import — this
//! only adds a headless way to *author* one from scratch, as
//! `set-placement` does for placements. Both mod ids are validated against
//! the session's own scan before the rule is saved, mirroring
//! `set-placement`'s own `--mod-id` check.

use anyhow::{Context, bail};
use clap::{Args, Subcommand, ValueEnum};
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{PairRule, Placement, PlacementRule, Rule, RuleOrigin};
use rim_session::use_cases::{DeleteRule, UpsertRule};
use rim_session::{RuleKey, Session};

use crate::common::{PathsArgs, build_session, resolve_paths};

#[derive(Debug, Subcommand)]
pub enum RuleCommand {
    /// Pins a mod to the top or bottom of the load order as your own
    /// decision, persisted to `rules.json` so it survives re-sorts and
    /// import toggles.
    SetPlacement(SetPlacementArgs),
    /// States that `--after` must load after `--before`, as your own
    /// decision, persisted to `rules.json`.
    SetPair(SetPairArgs),
    /// Removes a pair rule you (or an earlier `set-pair`) created.
    RemovePair(RemovePairArgs),
}

/// Mirrors [`Placement`] for `clap`'s `ValueEnum` derive (which needs
/// `clap`, a dependency `rim-resolve` deliberately never takes) —
/// `common.rs`'s own `TieBreakArg` does the identical thing for
/// [`rim_resolve::sort::TieBreak`].
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PlacementArg {
    /// See [`Placement::Top`].
    Top,
    /// See [`Placement::Bottom`].
    Bottom,
}

impl From<PlacementArg> for Placement {
    fn from(value: PlacementArg) -> Self {
        match value {
            PlacementArg::Top => Self::Top,
            PlacementArg::Bottom => Self::Bottom,
        }
    }
}

#[derive(Debug, Args)]
pub struct SetPlacementArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The mod to pin.
    #[arg(long, value_name = "MOD_ID")]
    mod_id: String,
    /// Which tier to pin it to.
    #[arg(long, value_enum)]
    placement: PlacementArg,
    /// A free-text note (e.g. why this mod is pinned).
    #[arg(long)]
    comment: Option<String>,
}

#[derive(Debug, Args)]
pub struct SetPairArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The mod that must load after `--before`.
    #[arg(long, value_name = "MOD_ID")]
    after: String,
    /// The mod that must load before `--after`.
    #[arg(long, value_name = "MOD_ID")]
    before: String,
    /// A free-text note (e.g. why this order is required).
    #[arg(long)]
    comment: Option<String>,
    /// The declared-edge override: mark this rule as deliberately
    /// overriding a `Declared`-strength author edge (`loadAfter`/
    /// `loadBefore`/`modDependencies`) between the same two mods, should
    /// one exist. An ordinary pair rule (this flag omitted) still loses
    /// to a declaration — this is loud, explicit,
    /// per-pair opt-in, never a default, and it still loses to any
    /// `Hard`/any-of requirement regardless. Re-run `set-pair` for the
    /// same `--after`/`--before` without this flag to clear it (a second
    /// call replaces the first, the same upsert-by-key semantics every
    /// other field here already has).
    #[arg(long)]
    override_declared: bool,
}

#[derive(Debug, Args)]
pub struct RemovePairArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The rule's dependent side, as originally given to `set-pair`.
    #[arg(long, value_name = "MOD_ID")]
    after: String,
    /// The rule's dependency side, as originally given to `set-pair`.
    #[arg(long, value_name = "MOD_ID")]
    before: String,
}

/// Whether `mod_id` names an active mod in `session`'s own scan, matched
/// through the `_steam` suffix (`ModId::base()`) exactly the way
/// `sort::tiers::assign_one` later matches a stored `PlacementRule`
/// against a mod's tier — so this check agrees with what the rule will
/// actually do once applied, not a stricter or looser notion of
/// "active".
fn mod_is_active(session: &Session, mod_id: &ModId) -> bool {
    let base = mod_id.base();
    session.report().mods.iter().any(|m| m.id.base() == base)
}

pub fn run(command: &RuleCommand) -> anyhow::Result<()> {
    match command {
        RuleCommand::SetPlacement(args) => run_set_placement(args),
        RuleCommand::SetPair(args) => run_set_pair(args),
        RuleCommand::RemovePair(args) => run_remove_pair(args),
    }
}

fn run_set_placement(args: &SetPlacementArgs) -> anyhow::Result<()> {
    let project_paths = resolve_paths(&args.paths)?;
    let mut session = build_session(project_paths)?;

    let mod_id = ModId::new(&args.mod_id);
    if !mod_is_active(&session, &mod_id) {
        bail!(
            "no active mod named {mod_id} — a typo here would persist a placement rule that \
             matches nothing and can't be diagnosed from the rules page; pass an id from `rimmerge \
             sort`/`rimmerge ledger`'s own mod list"
        );
    }

    let use_case = UpsertRule::new(rim_io::JsonRuleStore::new());
    use_case
        .execute(
            &mut session,
            Rule::Placement(PlacementRule {
                mod_id,
                placement: args.placement.into(),
                origin: RuleOrigin::UserDecision,
                comment: args.comment.clone(),
            }),
        )
        .context("saving the placement rule")?;

    let placement_word = match args.placement {
        PlacementArg::Top => "top",
        PlacementArg::Bottom => "bottom",
    };
    println!(
        "pinned {} to the {placement_word} as your own decision — this survives re-sorts and \
         import toggles",
        args.mod_id
    );
    Ok(())
}

fn run_set_pair(args: &SetPairArgs) -> anyhow::Result<()> {
    let project_paths = resolve_paths(&args.paths)?;
    let mut session = build_session(project_paths)?;

    let after = ModId::new(&args.after);
    let before = ModId::new(&args.before);
    for (flag, mod_id) in [("--after", &after), ("--before", &before)] {
        if !mod_is_active(&session, mod_id) {
            bail!(
                "no active mod named {mod_id} ({flag}) — a typo here would persist a pair rule \
                 that matches nothing and can't be diagnosed from the rules page; pass an id from \
                 `rimmerge sort`/`rimmerge ledger`'s own mod list"
            );
        }
    }
    if after.base() == before.base() {
        bail!("--after and --before name the same mod ({after}) — a pair rule needs two");
    }

    let use_case = UpsertRule::new(rim_io::JsonRuleStore::new());
    use_case
        .execute(
            &mut session,
            Rule::Pair(PairRule {
                after,
                before,
                origin: RuleOrigin::UserDecision,
                comment: args.comment.clone(),
                overrides_declared: args.override_declared,
            }),
        )
        .context("saving the pair rule")?;

    if args.override_declared {
        println!(
            "{} now loads after {} as your own decision, marked to override a declared edge \
             between them if one exists — this beats the author's own loadAfter/modDependencies \
             for this pair specifically, but never a hard dependency; survives re-sorts and \
             import toggles",
            args.after, args.before
        );
        return Ok(());
    }
    println!(
        "{} now loads after {} as your own decision — this survives re-sorts and import toggles",
        args.after, args.before
    );
    Ok(())
}

fn run_remove_pair(args: &RemovePairArgs) -> anyhow::Result<()> {
    let project_paths = resolve_paths(&args.paths)?;
    let mut session = build_session(project_paths)?;

    let key = RuleKey::Pair {
        after: ModId::new(&args.after),
        before: ModId::new(&args.before),
    };
    let use_case = DeleteRule::new(rim_io::JsonRuleStore::new());
    use_case
        .execute(&mut session, &key, None)
        .context("removing the pair rule")?;

    println!(
        "removed every rule stating {} after {} (any origin)",
        args.after, args.before
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_arg_top_maps_to_placement_top() {
        assert_eq!(Placement::from(PlacementArg::Top), Placement::Top);
    }

    #[test]
    fn placement_arg_bottom_maps_to_placement_bottom() {
        assert_eq!(Placement::from(PlacementArg::Bottom), Placement::Bottom);
    }
}
