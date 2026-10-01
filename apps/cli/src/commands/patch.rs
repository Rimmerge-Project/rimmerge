//! `rimmerge patch`: create, inspect, decide, and export compatibility
//! patches — a user-chosen scope of two or more mods with its own
//! decisions, independent of the profile's.
//! Every subcommand is a thin composition of
//! `rim-session::use_cases` and `rim-io` adapters; no business logic here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use rim_analyzer::domain::ModId;
use rim_io::GameProcessProbe;
use rim_merge::diff::{DiffClass, Value};
use rim_merge::plan::PlanOp;
use rim_resolve::domain::{
    Action, Decision, FieldPath, FindingKey, LedgerStats, MergeChoice, MergeState, PatchId,
    PatchProject,
};
use rim_session::PreviewSlot;
use rim_session::use_cases::{
    CreatePatch, CreatePatchInput, DecidePatch, DecidePatchMerge, DeletePatch, ExportOptions,
    ExportPatch, ImportProfileDecisions, PlanMerge, PrunePatchDecisions, RevertPatchDecision,
};

use crate::common::{PathsArgs, TerminalSafe, build_session, format_order_source, resolve_paths};

/// `rimmerge patch`'s own subcommands.
#[derive(Debug, Subcommand)]
pub enum PatchCommand {
    /// Creates a new compat patch scoped to two or more active mods.
    New(NewArgs),
    /// Lists every patch project on this profile.
    List(ListArgs),
    /// Prints one patch's identity, scope, and scoped ledger stats.
    Show(ShowArgs),
    /// Prints one finding's merge plan, restricted to the patch's scope.
    Plan(PlanArgs),
    /// Records a decision on one of a patch's own findings.
    Decide(DecideArgs),
    /// Removes a decision from one of a patch's own findings.
    Revert(RevertArgs),
    /// Copies the profile's own `Merge`/`ShipAsset` decisions into a patch.
    Import(ImportArgs),
    /// Removes every decision the patch's scoped ledger considers orphaned.
    Prune(PruneArgs),
    /// Renders and writes the patch into a user-chosen folder, optionally
    /// installing it into the game's `Mods/` folder.
    Export(ExportArgs),
    /// Deletes a patch project (never touches a previously exported folder).
    Delete(DeleteArgs),
}

/// Dispatches to the subcommand's own `run_*` function.
pub fn run(command: &PatchCommand) -> anyhow::Result<()> {
    match command {
        PatchCommand::New(args) => run_new(args),
        PatchCommand::List(args) => run_list(args),
        PatchCommand::Show(args) => run_show(args),
        PatchCommand::Plan(args) => run_plan(args),
        PatchCommand::Decide(args) => run_decide(args),
        PatchCommand::Revert(args) => run_revert(args),
        PatchCommand::Import(args) => run_import(args),
        PatchCommand::Prune(args) => run_prune(args),
        PatchCommand::Export(args) => run_export(args),
        PatchCommand::Delete(args) => run_delete(args),
    }
}

fn parse_patch_id(text: &str) -> anyhow::Result<PatchId> {
    text.parse()
        .map_err(|error| anyhow::anyhow!("--patch {text:?}: {error}"))
}

fn parse_finding_key(text: &str) -> anyhow::Result<FindingKey> {
    text.parse()
        .map_err(|error| anyhow::anyhow!("--key {text:?}: {error}"))
}

fn join_ids<'a>(ids: impl IntoIterator<Item = &'a ModId>) -> String {
    ids.into_iter()
        .map(ModId::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Debug, Args)]
pub struct NewArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The project's label in lists (distinct from the published mod's
    /// own display name).
    #[arg(long)]
    name: String,
    /// The published mod's package id, e.g. `author.abcompat`.
    #[arg(long = "package-id")]
    package_id: String,
    /// The published mod's display name. Defaults to `--name` when
    /// omitted.
    #[arg(long = "display-name")]
    display_name: Option<String>,
    /// Two or more scope member ids, comma-separated.
    #[arg(long, value_delimiter = ',')]
    scope: Vec<String>,
}

fn run_new(args: &NewArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let input = CreatePatchInput {
        name: args.name.clone(),
        package_id: args.package_id.clone(),
        display_name: args
            .display_name
            .clone()
            .unwrap_or_else(|| args.name.clone()),
        scope: args.scope.iter().map(ModId::new).collect(),
    };
    let id = CreatePatch::new(rim_io::JsonPatchProjectStore::new())
        .execute(&mut session, input)
        .context("creating the patch")?;
    println!("created patch {id}");
    Ok(())
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    paths: PathsArgs,
}

fn run_list(args: &ListArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    let source = session.selected();

    // Snapshot every project before calling `patch_ledger` (`&mut self`)
    // so no borrow of `session.patches()` is held across the mutable
    // calls in the loop below.
    let projects: Vec<PatchProject> = session.patches().cloned().collect();
    if projects.is_empty() {
        println!("no patches on this profile");
        return Ok(());
    }

    for project in &projects {
        let stats = session
            .patch_ledger(project.id(), source)
            .context("building the patch ledger")?
            .stats;
        println!(
            "{:<14} {:<20} {:<28} scope=[{}] needs_input={} merged={} export={}",
            project.id(),
            TerminalSafe::line(project.name()).to_string(),
            TerminalSafe::line(project.identity().package_id()).to_string(),
            TerminalSafe::line(join_ids(project.scope().members())),
            stats.needs_input,
            stats.merged,
            project
                .export_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string())
        );
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
}

fn print_ledger_stats(stats: &LedgerStats) {
    println!(
        "needs_input={}  auto={}  overridden={}  merged={}  merge_incomplete={}",
        stats.needs_input, stats.auto, stats.overridden, stats.merged, stats.merge_incomplete
    );
}

fn run_show(args: &ShowArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    let source = session.selected();

    let project = session
        .patch(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no such patch: {id}"))?;
    let ledger = session
        .patch_ledger(&id, source)
        .context("building the patch ledger")?;

    println!("id:          {}", project.id());
    println!("name:        {}", TerminalSafe::line(project.name()));
    println!(
        "package id:  {}",
        TerminalSafe::line(project.identity().package_id())
    );
    println!(
        "display:     {}",
        TerminalSafe::line(project.identity().display_name())
    );
    println!("author:      {}", TerminalSafe::line(project.author()));
    println!(
        "scope:       {}",
        TerminalSafe::line(join_ids(project.scope().members()))
    );
    println!(
        "export dir:  {}",
        project
            .export_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!();
    println!(
        "scoped ledger ({}) — {} finding(s):",
        format_order_source(ledger.source),
        ledger.entries.len()
    );
    print_ledger_stats(&ledger.stats);

    let orphaned = session
        .patch_orphaned(&id, source)
        .context("computing orphaned decisions")?;
    if !orphaned.is_empty() {
        println!();
        println!("orphaned decisions:");
        for key in orphaned {
            println!("  {}", TerminalSafe::line(key));
        }
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
    /// A finding key's canonical text form (see
    /// `rim_resolve::domain::FindingKey`'s `Display`).
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
    let id = parse_patch_id(&args.patch)?;
    let key = parse_finding_key(&args.key)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let slot = PreviewSlot::patch(session.selected(), id.clone());
    let ctx = session
        .merge_context(slot, &key)
        .context("building the patch's merge context")?;
    let planner = PlanMerge::new(rim_io::FileDefSourceReader::new());
    let preview = planner
        .execute_in(&mut session, ctx, &key)
        .context("building the merge preview")?;

    println!("key:    {}", TerminalSafe::line(&preview.key));
    println!(
        "owners: {}  base={}  winner={}",
        TerminalSafe::line(join_ids(&preview.owners)),
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

    println!();
    println!("{:<40} {:<22} {:<24} result", "field", "class", "base");
    for field in &preview.diff.fields {
        if matches!(field.class, DiffClass::Unchanged) {
            continue;
        }
        println!(
            "{:<40} {:<22} {:<24} {}",
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
pub struct DecideArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
    /// A finding key's canonical text form.
    #[arg(long)]
    key: String,
    /// Suppress this finding — not addressed by this patch.
    #[arg(long, conflicts_with_all = ["ship_asset", "choice"])]
    ignore: bool,
    /// Ships one owner's texture file for a `TextureOverride` finding.
    #[arg(long = "ship-asset",
        value_name = "MOD",
        conflicts_with_all = ["ignore", "choice"]
    )]
    ship_asset: Option<String>,
    /// `<field path>=<from:mod|value:text|drop>`, repeatable. Replaces
    /// the whole choices map; with none given (and neither `--ignore` nor
    /// `--ship-asset`), records an empty `Merge` decision (every field
    /// automatic).
    #[arg(long = "choice", value_name = "PATH=SPEC")]
    choice: Vec<String>,
}

fn parse_choices(raw: &[String]) -> anyhow::Result<BTreeMap<FieldPath, MergeChoice>> {
    let mut choices = BTreeMap::new();
    for entry in raw {
        let (path_text, spec) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--choice {entry:?}: expected <path>=<spec>"))?;
        let path: FieldPath = path_text
            .parse()
            .map_err(|error| anyhow::anyhow!("--choice {entry:?}: invalid field path: {error}"))?;
        let choice = if spec == "drop" {
            MergeChoice::Drop
        } else if let Some(mod_id) = spec.strip_prefix("from:") {
            MergeChoice::From {
                mod_id: ModId::new(mod_id),
            }
        } else if let Some(text) = spec.strip_prefix("value:") {
            MergeChoice::Value {
                text: text.to_string(),
            }
        } else {
            bail!("--choice {entry:?}: spec must be one of from:<mod>, value:<text>, drop");
        };
        choices.insert(path, choice);
    }
    Ok(choices)
}

fn print_merge_state(state: &MergeState) {
    match state {
        MergeState::Complete { op_count } => println!("state: Complete ({op_count} op(s))"),
        MergeState::NeedsFieldInput { unresolved, total } => {
            println!("state: NeedsFieldInput ({unresolved}/{total} field(s) unresolved)");
        }
        MergeState::CannotMerge { reason } => {
            println!("state: CannotMerge — {}", TerminalSafe::line(reason));
        }
    }
}

fn run_decide(args: &DecideArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let key = parse_finding_key(&args.key)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    if args.ignore {
        let decision = Decision {
            key,
            action: Action::Ignore,
            note: None,
            decided_at: jiff::Timestamp::now(),
        };
        let stats = DecidePatch::new(rim_io::JsonPatchProjectStore::new())
            .execute(&mut session, &id, decision)
            .context("recording the decision")?;
        print_ledger_stats(&stats);
        return Ok(());
    }

    if let Some(mod_id) = &args.ship_asset {
        let FindingKey::TextureOverride { texture_path, .. } = &key else {
            bail!("--ship-asset only applies to a TextureOverride finding key");
        };
        let decision = Decision {
            key: key.clone(),
            action: Action::ShipAsset {
                texture_path: texture_path.clone(),
                from: ModId::new(mod_id),
            },
            note: None,
            decided_at: jiff::Timestamp::now(),
        };
        let stats = DecidePatch::new(rim_io::JsonPatchProjectStore::new())
            .execute(&mut session, &id, decision)
            .context("recording the decision")?;
        print_ledger_stats(&stats);
        return Ok(());
    }

    let choices = parse_choices(&args.choice)?;
    let state = DecidePatchMerge::new(
        rim_io::JsonPatchProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    )
    .execute(&mut session, &id, &key, choices)
    .context("recording the merge decision")?;
    print_merge_state(&state);
    Ok(())
}

#[derive(Debug, Args)]
pub struct RevertArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
    /// A finding key's canonical text form.
    #[arg(long)]
    key: String,
}

fn run_revert(args: &RevertArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let key = parse_finding_key(&args.key)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let removed = RevertPatchDecision::new(rim_io::JsonPatchProjectStore::new())
        .execute(&mut session, &id, &key)
        .context("reverting the decision")?;
    match removed {
        Some(_) => println!("reverted the decision on {}", TerminalSafe::line(&key)),
        None => println!("no decision was recorded on {}", TerminalSafe::line(&key)),
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
    /// A finding key to import; repeatable. Omit to import every key the
    /// patch's own scoped ledger admits.
    #[arg(long = "key")]
    keys: Vec<String>,
}

fn run_import(args: &ImportArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let keys = if args.keys.is_empty() {
        None
    } else {
        Some(
            args.keys
                .iter()
                .map(|text| parse_finding_key(text))
                .collect::<anyhow::Result<Vec<_>>>()?,
        )
    };
    let report = ImportProfileDecisions::new(rim_io::JsonPatchProjectStore::new())
        .execute(&mut session, &id, keys.as_deref())
        .context("importing profile decisions")?;

    println!("imported {} decision(s):", report.imported.len());
    for key in &report.imported {
        println!("  {}", TerminalSafe::line(key));
    }
    if !report.skipped.is_empty() {
        println!("skipped {} decision(s):", report.skipped.len());
        for (key, reason) in &report.skipped {
            println!(
                "  {}: {}",
                TerminalSafe::line(key),
                TerminalSafe::line(reason)
            );
        }
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct PruneArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
}

fn run_prune(args: &PruneArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    let pruned = PrunePatchDecisions::new(rim_io::JsonPatchProjectStore::new())
        .execute(&mut session, &id)
        .context("pruning orphaned decisions")?;
    println!("pruned {} decision(s):", pruned.len());
    for decision in &pruned {
        println!("  {}", TerminalSafe::line(&decision.key));
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
    /// The folder to render the patch into — never `<game>/Mods` or a
    /// subfolder of it (use `--install` instead).
    #[arg(long = "out")]
    out_dir: PathBuf,
    /// Also copy the rendered folder into `<game>/Mods` and activate it.
    #[arg(long)]
    install: bool,
    /// Install even if `RimWorldWin64.exe` looks like it's running.
    #[arg(long)]
    force: bool,
}

fn run_export(args: &ExportArgs) -> anyhow::Result<()> {
    run_export_with_probe(args, &rim_io::SysinfoGameProcessProbe::new())
}

/// Shares the same process-probe seam `apps/cli`'s own `apply` command
/// uses, so `--install`'s running-game refusal can be exercised with a
/// fake probe in a unit test rather than needing a real
/// `RimWorldWin64.exe` process on the test machine.
fn run_export_with_probe(args: &ExportArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    if args.install && !args.force && probe.is_running() {
        bail!(
            "RimWorldWin64.exe is running; close the game or retry with --force (installing a patch writes ModsConfig.xml, and RimWorld overwrites it on exit)"
        );
    }

    // Absolutized (`std::path::absolute`) and, when it already exists,
    // canonicalized (resolving `.`/`..`, symlinks, and junctions, with
    // the `\\?\` prefix stripped back off) — `ExportPatch` itself has no
    // filesystem access of its own and can only compare `out_dir`
    // lexically (see that use case's doc comment on `is_inside`), so a
    // relative path or a junction pointing into the game's `Mods` folder
    // must be resolved here, before it ever reaches the use case.
    let out_dir = rim_io::resolve_user_dir(&args.out_dir)
        .with_context(|| format!("resolving --out {}", args.out_dir.display()))?;

    let use_case = ExportPatch::new(
        rim_io::MergeModFolderWriter::new(),
        rim_io::FileDefSourceReader::new(),
        rim_io::FileAssetLocator::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonPatchProjectStore::new(),
    );
    // `{error:?}` (the derived `Debug`, not `Display`) so a refusal names
    // its own `ExportPatchError` variant verbatim in the printed message
    // (e.g. `ForeignFolder { .. }`, `OutDirIsModsFolder(..)`) rather than
    // only the friendlier prose `#[error("...")]` renders, so a failed
    // export names exactly which refusal fired.
    let outcome = use_case
        .execute(
            &mut session,
            &id,
            ExportOptions {
                out_dir,
                install: args.install,
            },
        )
        .map_err(|error| anyhow::anyhow!("exporting the patch: {error:?} ({error})"))?;

    println!("exported to:      {}", outcome.export_path.display());
    if let Some(installed) = &outcome.installed_path {
        println!("installed to:     {}", installed.display());
    }
    if let Some(backup) = &outcome.mods_config_backup {
        println!("ModsConfig.xml backup: {}", backup.display());
    }
    println!("files written:    {}", outcome.files.len());
    println!("decisions hash:   {}", outcome.decisions_sha256);
    if !outcome.skipped.is_empty() {
        println!(
            "skipped {} decision(s) (not yet a complete plan):",
            outcome.skipped.len()
        );
        for key in &outcome.skipped {
            println!("  {}", TerminalSafe::line(key));
        }
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The patch's id, from `patch list`.
    #[arg(long)]
    patch: String,
}

fn run_delete(args: &DeleteArgs) -> anyhow::Result<()> {
    let id = parse_patch_id(&args.patch)?;
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;

    DeletePatch::new(rim_io::JsonPatchProjectStore::new())
        .execute(&mut session, &id)
        .context("deleting the patch")?;
    println!("deleted patch {id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::test_fixtures::{copy_dir_recursive, merge_game_fixture};

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    /// A scratch copy of `rim-io`'s `merge_game` fixture, with a patch
    /// already created and decided (`ModA`'s `MaxHitPoints`) — everything
    /// `run_export_with_probe`'s own tests need.
    fn decided_patch_args(temp_dir: &Path) -> ExportArgs {
        let game_dir = temp_dir.join("game");
        copy_dir_recursive(&merge_game_fixture(), &game_dir).expect("copy fixture game tree");
        let paths = PathsArgs {
            game_dir: Some(game_dir.clone()),
            workshop_dir: Some(temp_dir.join("workshop_does_not_exist")),
            mods_config: Some(game_dir.join("ModsConfig.xml")),
            profile_dir: Some(temp_dir.join("profile")),
        };
        let resolved = resolve_paths(&paths).expect("resolve scratch paths");
        let mut session = build_session(resolved).expect("build session from scratch fixture");

        let id = CreatePatch::new(rim_io::JsonPatchProjectStore::new())
            .execute(
                &mut session,
                CreatePatchInput {
                    name: "AB compat".to_string(),
                    package_id: "sample.abcompat".to_string(),
                    display_name: "A + B Compatibility".to_string(),
                    scope: [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                        .into_iter()
                        .collect(),
                },
            )
            .expect("creating the patch must succeed");
        let key = FindingKey::DefOverride {
            key: rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Fixture_Wall".to_string(),
            },
            owners: [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                .into_iter()
                .collect(),
        };
        let mut choices = BTreeMap::new();
        choices.insert(
            "statBases/MaxHitPoints".parse().expect("valid field path"),
            MergeChoice::From {
                mod_id: ModId::new("fixture.moda"),
            },
        );
        DecidePatchMerge::new(
            rim_io::JsonPatchProjectStore::new(),
            rim_io::FileDefSourceReader::new(),
        )
        .execute(&mut session, &id, &key, choices)
        .expect("deciding the patch merge must succeed");

        ExportArgs {
            paths,
            patch: id.to_string(),
            out_dir: temp_dir.join("out"),
            install: true,
            force: false,
        }
    }

    #[test]
    fn export_install_refuses_when_the_probe_reports_the_game_running_and_not_forced() {
        let temp_dir = tempdir().expect("tempdir");
        let args = decided_patch_args(temp_dir.path());

        let result = run_export_with_probe(&args, &FixedProbe(true));

        let error = result.expect_err("must refuse when the game looks like it's running");
        assert!(error.to_string().contains("--force"));
        assert!(
            !args.out_dir.exists(),
            "a refused install must never write the export folder"
        );
    }

    #[test]
    fn export_install_proceeds_when_forced_despite_the_game_running() {
        let temp_dir = tempdir().expect("tempdir");
        let mut forced = decided_patch_args(temp_dir.path());
        forced.force = true;

        run_export_with_probe(&forced, &FixedProbe(true)).expect("forced export must succeed");

        assert!(forced.out_dir.join("sample_abcompat").is_dir());
    }
}
