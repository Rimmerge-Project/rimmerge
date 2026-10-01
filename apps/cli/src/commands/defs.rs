//! `rimmerge defs`: the def inspector's CLI surface
//! `changes` lists what one mod
//! changes (`Session::changes`); `inspect` answers everything about one
//! def/template under a selected order (`InspectDef`); `search` finds a
//! `DefRef` by name (`Session::search_defs`). Thin, like every other
//! command here: parse args, call the session/use case, print a table or
//! (`--json`) a machine-readable document built from plain, locally
//! defined `Serialize` structs — no domain/session type here derives
//! `Serialize` itself, so this module transcribes fields rather than
//! reusing one, the same shape `merge coverage`'s own `CoverageReport`
//! already uses.

use std::collections::BTreeMap;

use anyhow::Context;
use clap::{Args, Subcommand, ValueEnum};
use rim_analyzer::domain::{FindModGate, ModId};
use rim_merge::effective::{Completeness, Provenance, Stopper};
use rim_resolve::domain::{DefRef, FindingKey, OrderSource, ResolutionStatus};
use rim_session::effective_fields;
use rim_session::use_cases::{DefInspection, InspectDef, InspectDefError, SelectOrder};
use rim_session::{AssetKind, ChangeFilter, ChangeKind, ChangePage, ChangeRow, MAX_PAGE_SIZE};
use serde::Serialize;

use crate::common::{PathsArgs, TerminalSafe, build_session, resolve_paths};

#[derive(Debug, Subcommand)]
pub enum DefsCommand {
    /// Lists what one mod changes: defs/templates it owns, foreign defs
    /// it patches, and assets it overrides.
    Changes(ChangesArgs),
    /// Everything about one def/template under a selected order: owners,
    /// patchers, template chain, and the effective (in-game) def.
    Inspect(InspectArgs),
    /// Finds a def or template by name.
    Search(SearchArgs),
}

pub fn run(command: &DefsCommand) -> anyhow::Result<()> {
    match command {
        DefsCommand::Changes(args) => run_changes(args),
        DefsCommand::Inspect(args) => run_inspect(args),
        DefsCommand::Search(args) => run_search(args),
    }
}

// ---------------------------------------------------------------------
// `defs changes`
// ---------------------------------------------------------------------

/// Mirrors [`ChangeKind`] for `clap`'s `ValueEnum` derive (which needs
/// `clap`, a dependency `rim-session` deliberately never takes) —
/// `overrides-asset` expands to every [`AssetKind`] variant via
/// [`expand_kind`], since [`ChangeFilter::kinds`] filters by exact
/// [`ChangeKind`] equality and a CLI user typing the coarse form has no
/// reason to pick a single asset kind. The three fine-grained variants
/// (`overrides-texture`/`overrides-sound`/`overrides-keyed`) are the
/// strings [`format_kind`] itself prints for a single row, so a caller
/// can pipe one printed kind back in as a `--kind` filter (e.g.
/// `defs changes --kind overrides-texture` after seeing that exact text in
/// a `--json` row).
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ChangeKindArg {
    OwnsDef,
    OwnsTemplate,
    PatchesDef,
    OverridesAsset,
    OverridesTexture,
    OverridesSound,
    OverridesKeyed,
}

fn expand_kind(arg: ChangeKindArg) -> Vec<ChangeKind> {
    match arg {
        ChangeKindArg::OwnsDef => vec![ChangeKind::OwnsDef],
        ChangeKindArg::OwnsTemplate => vec![ChangeKind::OwnsTemplate],
        ChangeKindArg::PatchesDef => vec![ChangeKind::PatchesDef],
        ChangeKindArg::OverridesAsset => vec![
            ChangeKind::OverridesAsset(AssetKind::Texture),
            ChangeKind::OverridesAsset(AssetKind::Sound),
            ChangeKind::OverridesAsset(AssetKind::KeyedTranslation),
        ],
        ChangeKindArg::OverridesTexture => vec![ChangeKind::OverridesAsset(AssetKind::Texture)],
        ChangeKindArg::OverridesSound => vec![ChangeKind::OverridesAsset(AssetKind::Sound)],
        ChangeKindArg::OverridesKeyed => {
            vec![ChangeKind::OverridesAsset(AssetKind::KeyedTranslation)]
        }
    }
}

fn format_kind(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::OwnsDef => "owns-def",
        ChangeKind::OwnsTemplate => "owns-template",
        ChangeKind::PatchesDef => "patches-def",
        ChangeKind::OverridesAsset(AssetKind::Texture) => "overrides-asset(texture)",
        ChangeKind::OverridesAsset(AssetKind::Sound) => "overrides-asset(sound)",
        ChangeKind::OverridesAsset(AssetKind::KeyedTranslation) => {
            "overrides-asset(keyed-translation)"
        }
    }
}

#[derive(Debug, Args)]
pub struct ChangesArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The mod to list changes for (its declared package id).
    mod_id: String,
    /// Only rows of these kinds (repeatable; every kind when omitted).
    #[arg(long = "kind", value_enum)]
    kinds: Vec<ChangeKindArg>,
    /// Only rows whose def/template name, type, or asset path contains
    /// this substring (case-insensitive).
    #[arg(long)]
    search: Option<String>,
    /// How many matching rows to skip before collecting the page.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// How many rows to print, capped at `MAX_PAGE_SIZE`.
    #[arg(long, default_value_t = MAX_PAGE_SIZE)]
    limit: usize,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

fn run_changes(args: &ChangesArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let session = build_session(paths)?;
    let mod_id = ModId::new(&args.mod_id);

    let kinds: Vec<ChangeKind> = args.kinds.iter().copied().flat_map(expand_kind).collect();
    let filter = ChangeFilter {
        kinds: (!kinds.is_empty()).then(|| kinds.into_iter().collect()),
        search: args.search.clone(),
        offset: args.offset,
        limit: args.limit,
    };

    let page = session.changes(&mod_id, &filter);

    if args.json {
        return print_changes_json(&mod_id, args.offset, &page);
    }
    print_changes_table(&mod_id, &page);
    Ok(())
}

fn target_text(row: &ChangeRow) -> String {
    match (&row.def_ref, &row.asset_path) {
        (Some(def_ref), _) => TerminalSafe::line(def_ref).to_string(),
        (None, Some(path)) => TerminalSafe::line(path).to_string(),
        (None, None) => "?".to_string(),
    }
}

fn finding_keys_text(keys: &[FindingKey]) -> String {
    if keys.is_empty() {
        return "-".to_string();
    }
    keys.iter()
        .map(|key| TerminalSafe::line(key).to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

fn print_changes_table(mod_id: &ModId, page: &ChangePage) {
    println!(
        "changes for {} — {} total",
        TerminalSafe::line(mod_id),
        page.total
    );
    if !page.kind_counts.is_empty() {
        let counts: Vec<String> = page
            .kind_counts
            .iter()
            .map(|(kind, count)| format!("{}={count}", format_kind(*kind)))
            .collect();
        println!("  by kind: {}", counts.join(", "));
    }
    println!();
    println!(
        "{:<24} {:<40} {:>8} {:>6}  findings",
        "kind", "target", "others", "ops"
    );
    for row in &page.items {
        println!(
            "{:<24} {:<40} {:>8} {:>6}  {}",
            format_kind(row.kind),
            target_text(row),
            row.other_touchers,
            row.op_count,
            finding_keys_text(&row.finding_keys)
        );
    }
    let shown = page.items.len();
    if shown < page.total {
        println!(
            "... {} more row(s) not shown (see --offset/--limit)",
            page.total - shown
        );
    }
}

#[derive(Debug, Serialize)]
struct ChangeRowJson {
    kind: String,
    def_ref: Option<String>,
    asset_path: Option<String>,
    other_touchers: usize,
    op_count: usize,
    finding_keys: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChangesJson {
    mod_id: String,
    total: usize,
    offset: usize,
    kind_counts: BTreeMap<String, usize>,
    items: Vec<ChangeRowJson>,
}

fn print_changes_json(mod_id: &ModId, offset: usize, page: &ChangePage) -> anyhow::Result<()> {
    let json = ChangesJson {
        mod_id: mod_id.to_string(),
        total: page.total,
        offset,
        kind_counts: page
            .kind_counts
            .iter()
            .map(|(kind, count)| (format_kind(*kind).to_string(), *count))
            .collect(),
        items: page
            .items
            .iter()
            .map(|row| ChangeRowJson {
                kind: format_kind(row.kind).to_string(),
                def_ref: row.def_ref.as_ref().map(ToString::to_string),
                asset_path: row.asset_path.clone(),
                other_touchers: row.other_touchers,
                op_count: row.op_count,
                finding_keys: row.finding_keys.iter().map(ToString::to_string).collect(),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).context("serializing changes as JSON")?;
    println!("{text}");
    Ok(())
}

// ---------------------------------------------------------------------
// `defs inspect`
// ---------------------------------------------------------------------

/// Mirrors [`OrderSource`] for `clap`'s `ValueEnum` derive, same shape as
/// `ledger`'s own private `SourceArg`.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum SourceArg {
    Current,
    Suggested,
}

impl From<SourceArg> for OrderSource {
    fn from(value: SourceArg) -> Self {
        match value {
            SourceArg::Current => Self::Current,
            SourceArg::Suggested => Self::Suggested,
        }
    }
}

fn format_source(source: OrderSource) -> &'static str {
    match source {
        OrderSource::Current => "current",
        OrderSource::Suggested => "suggested",
    }
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// The def or template to inspect, e.g. `ThingDef/Fixture_Wall` or
    /// `ThingDef/@Fixture_WallBase` (`@name` alone for a name-only
    /// template ref). Quote it if the name contains spaces.
    def_ref: DefRef,
    /// Which order to inspect under.
    #[arg(long, value_enum, default_value_t = SourceArg::Current)]
    source: SourceArg,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

fn run_inspect(args: &InspectArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let mut session = build_session(paths)?;
    SelectOrder::new().execute(&mut session, args.source.into());

    let inspector = InspectDef::new(rim_io::FileDefSourceReader::new());
    let inspection = match inspector.execute(&mut session, &args.def_ref) {
        Ok(inspection) => inspection,
        Err(InspectDefError::NotFound(def_ref)) => {
            anyhow::bail!("{def_ref} not found: not indexed as a def or template by this scan");
        }
        Err(error) => return Err(error).context("inspecting the def"),
    };

    if args.json {
        return print_inspection_json(inspection);
    }
    print_inspection_text(inspection);
    Ok(())
}

fn format_stopper(stopper: &Stopper) -> String {
    match stopper {
        Stopper::Replay {
            mod_id,
            op_index,
            error,
        } => format!("{mod_id}'s operation #{op_index} — {error}"),
        Stopper::Inherit(error) => format!("inheritance — {error}"),
    }
}

/// A short, human-readable rendering of a [`FindModGate`] — used in both
/// the text and `--json` paths so a caller never has
/// to read `{gate:?}`'s `Debug` shape to know which mods a gate names.
fn format_gate(gate: &FindModGate) -> String {
    match gate {
        FindModGate::AnyActive(names) => format!("any active: {}", names.join(", ")),
        FindModGate::NoneActive(names) => format!("none active: {}", names.join(", ")),
    }
}

fn format_provenance(provenance: &Provenance) -> String {
    match provenance {
        Provenance::Owner(mod_id) => format!("owner ({mod_id})"),
        Provenance::Patch { mod_id, op_index } => {
            format!("patched by {mod_id} (op #{op_index})")
        }
        Provenance::Inherited { template, owner } => {
            format!("inherited from {template} (registered by {owner})")
        }
        Provenance::UnattributedTemplate { template } => {
            format!("inherited from {template} (owner unknown)")
        }
    }
}

fn print_inspection_text(inspection: &DefInspection) {
    println!("def:    {}", TerminalSafe::line(&inspection.def_ref));
    println!("source: {}", format_source(inspection.source));
    if inspection.template_ambiguity.is_some() {
        println!(
            "winner: {}  (representative only — no single answer exists, see \"ambiguous template\" below)",
            TerminalSafe::line(&inspection.winner)
        );
    } else {
        println!("winner: {}", TerminalSafe::line(&inspection.winner));
    }

    println!();
    println!("owners:");
    for owner in &inspection.owners {
        let marker = if owner.mod_id != inspection.winner {
            ""
        } else if inspection.template_ambiguity.is_some() {
            " (last-loaded representative)"
        } else {
            " (winner)"
        };
        let generated = if owner.is_generated {
            " [generated]"
        } else {
            ""
        };
        println!(
            "  [{:>3}] {}{marker}{generated}",
            owner.position,
            TerminalSafe::line(&owner.mod_id)
        );
    }

    if let Some(ambiguity) = &inspection.template_ambiguity {
        println!();
        println!(
            "ambiguous template: this Name has {} registrations. RimWorld resolves \
             each child independently to whichever registration loads nearest at or \
             before it — never a single winner. \"winner\"/\"(last-loaded \
             representative)\" above is only this crate's own disclosed stand-in for \
             when no specific child is being asked, not an answer.",
            ambiguity.registrants.len()
        );
        let mut by_resolution: BTreeMap<ModId, Vec<ModId>> = BTreeMap::new();
        for (child, resolved_to) in &ambiguity.resolutions {
            by_resolution
                .entry(resolved_to.clone())
                .or_default()
                .push(child.clone());
        }
        for registrant in &ambiguity.registrants {
            let empty = Vec::new();
            let children = by_resolution.get(registrant).unwrap_or(&empty);
            println!(
                "  {} — {} known child(ren) resolve here:",
                TerminalSafe::line(registrant),
                children.len()
            );
            for child in children {
                println!("    {}", TerminalSafe::line(child));
            }
        }
    }

    if !inspection.patchers.is_empty() {
        println!();
        println!("patchers:");
        for patcher in &inspection.patchers {
            let generated = if patcher.is_generated {
                " [generated]"
            } else {
                ""
            };
            // `Ok(())` alone conflates "verified clean" with "never
            // reached" — `reached` is `false` only when an *earlier*
            // mod's own op was the fold's stopper, so this mod's ops were
            // never attempted at all; that case gets its own wording,
            // never a bare "ok" implying a verified pass.
            let replay = match (&patcher.replay, patcher.reached) {
                (Ok(()), true) => "replay: ok".to_string(),
                (Ok(()), false) => {
                    "replay: not reached (an earlier mod's op stopped the fold)".to_string()
                }
                (Err(reason), _) => format!("replay: FAILED — {reason}"),
            };
            println!(
                "  [{:>3}] {}{generated} — {}",
                patcher.position,
                TerminalSafe::line(&patcher.mod_id),
                TerminalSafe::line(replay)
            );
            for op in &patcher.ops {
                let sub_path = op
                    .sub_path
                    .as_deref()
                    .map(|s| format!(" [{}]", TerminalSafe::line(s)))
                    .unwrap_or_default();
                let gates = if op.find_mod_context.is_empty() {
                    String::new()
                } else {
                    let gates = op
                        .find_mod_context
                        .iter()
                        .map(format_gate)
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("  gates=[{}]", TerminalSafe::line(gates))
                };
                let may_require = if op.may_require.is_empty() {
                    String::new()
                } else {
                    format!(
                        "  may_require=[{}]",
                        TerminalSafe::line(op.may_require.join(", "))
                    )
                };
                let wrapped = if op.is_wrapped {
                    "  (inside a wrapper op)"
                } else {
                    ""
                };
                println!(
                    "      {} {}{sub_path}{gates}{may_require}{wrapped}",
                    TerminalSafe::line(&op.class),
                    TerminalSafe::line(op.xpath.as_deref().unwrap_or("<no xpath>"))
                );
            }
            for caveat in &patcher.caveats {
                println!("      caveat: {}", TerminalSafe::line(caveat));
            }
        }
    }

    if !inspection.parents.is_empty() {
        println!();
        println!("parents (nearest first):");
        for (key, owner) in &inspection.parents {
            println!(
                "  {}  (registered by {})",
                TerminalSafe::line(key),
                TerminalSafe::line(owner)
            );
        }
    }

    if !inspection.children.is_empty() {
        println!();
        println!("children:");
        for (owner, key) in &inspection.children {
            println!(
                "  {}  (owned by {})",
                TerminalSafe::line(key),
                TerminalSafe::line(owner)
            );
        }
    }

    println!();
    match &inspection.effective.completeness {
        Completeness::Complete => println!("completeness: complete"),
        Completeness::Partial { stopped_at } => {
            println!(
                "completeness: PARTIAL — stopped at {}",
                TerminalSafe::line(format_stopper(stopped_at))
            );
        }
    }
    if !inspection.effective.caveats.is_empty() {
        println!("caveats:");
        for caveat in &inspection.effective.caveats {
            println!("  {}", TerminalSafe::line(caveat));
        }
    }

    println!();
    println!("effective fields:");
    println!("{:<44} provenance", "path");
    for field in effective_fields::fields(&inspection.effective) {
        println!(
            "  {:<42} {}",
            TerminalSafe::line(&field.path).to_string(),
            TerminalSafe::line(format_provenance(&field.provenance))
        );
    }

    println!();
    println!("resolved XML:");
    println!("{}", TerminalSafe::block(&inspection.resolved_xml));

    if !inspection.findings.is_empty() {
        println!();
        println!("findings:");
        for (key, status) in &inspection.findings {
            println!("  [{}] {}", format_status(*status), TerminalSafe::line(key));
        }
    }
}

#[derive(Debug, Serialize)]
struct ToucherJson {
    mod_id: String,
    position: usize,
    is_generated: bool,
}

#[derive(Debug, Serialize)]
struct PatchOpJson {
    class: String,
    xpath: Option<String>,
    sub_path: Option<String>,
    find_mod_context: Vec<String>,
    may_require: Vec<String>,
    may_require_any_of: Vec<String>,
    locator_file: String,
    /// See `PatchOpSummary::is_wrapped`'s own doc comment: `true` when
    /// this summary describes a stand-in mutating descendant rather than
    /// the top-level `<Operation>` node itself.
    is_wrapped: bool,
}

#[derive(Debug, Serialize)]
struct PatcherJson {
    mod_id: String,
    position: usize,
    is_generated: bool,
    ops: Vec<PatchOpJson>,
    replay_ok: bool,
    replay_error: Option<String>,
    /// See `Patcher::reached`'s own doc comment: `false` only when an
    /// earlier mod's own op was the fold's stopper, so this mod's ops
    /// were never attempted — `replay_ok: true` alongside `reached:
    /// false` means "not disproven", not "verified".
    reached: bool,
    /// Every caveat the fold recorded while replaying this mod's own
    /// contributions.
    caveats: Vec<String>,
}

#[derive(Debug, Serialize)]
struct KeyOwnerJson {
    def_key: String,
    mod_id: String,
}

#[derive(Debug, Serialize)]
struct FieldJson {
    path: String,
    provenance: String,
}

#[derive(Debug, Serialize)]
struct FindingJson {
    key: String,
    status: String,
}

/// See [`DefInspection::template_ambiguity`]'s own doc comment: `Some`
/// only for a template with more than one registrant, where `winner` on
/// [`InspectionJson`] is a disclosed representative (the last-loaded
/// registrant), never a real single answer.
#[derive(Debug, Serialize)]
struct TemplateAmbiguityJson {
    /// Every mod registering this `Name`, in load order.
    registrants: Vec<String>,
    /// One entry per known child, keyed by the child's own mod id,
    /// naming which registrant it actually resolves to under the
    /// selected order.
    resolutions: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct InspectionJson {
    def_ref: String,
    source: String,
    owners: Vec<ToucherJson>,
    winner: String,
    /// `Some` when `winner` above is a disclosed representative rather
    /// than a real answer — see [`TemplateAmbiguityJson`]'s own doc
    /// comment.
    template_ambiguity: Option<TemplateAmbiguityJson>,
    patchers: Vec<PatcherJson>,
    parents: Vec<KeyOwnerJson>,
    children: Vec<KeyOwnerJson>,
    completeness: String,
    stopped_at: Option<String>,
    caveats: Vec<String>,
    fields: Vec<FieldJson>,
    resolved_xml: String,
    findings: Vec<FindingJson>,
}

fn format_status(status: ResolutionStatus) -> &'static str {
    match status {
        ResolutionStatus::Auto => "auto",
        ResolutionStatus::NeedsInput => "needs_input",
        ResolutionStatus::UserOverridden => "user_overridden",
    }
}

fn print_inspection_json(inspection: &DefInspection) -> anyhow::Result<()> {
    let (completeness, stopped_at) = match &inspection.effective.completeness {
        Completeness::Complete => ("complete".to_string(), None),
        Completeness::Partial { stopped_at } => {
            ("partial".to_string(), Some(format_stopper(stopped_at)))
        }
    };

    let json = InspectionJson {
        def_ref: inspection.def_ref.to_string(),
        source: format_source(inspection.source).to_string(),
        owners: inspection
            .owners
            .iter()
            .map(|owner| ToucherJson {
                mod_id: owner.mod_id.to_string(),
                position: owner.position,
                is_generated: owner.is_generated,
            })
            .collect(),
        winner: inspection.winner.to_string(),
        template_ambiguity: inspection.template_ambiguity.as_ref().map(|ambiguity| {
            TemplateAmbiguityJson {
                registrants: ambiguity
                    .registrants
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                resolutions: ambiguity
                    .resolutions
                    .iter()
                    .map(|(child, resolved_to)| (child.to_string(), resolved_to.to_string()))
                    .collect(),
            }
        }),
        patchers: inspection
            .patchers
            .iter()
            .map(|patcher| PatcherJson {
                mod_id: patcher.mod_id.to_string(),
                position: patcher.position,
                is_generated: patcher.is_generated,
                ops: patcher
                    .ops
                    .iter()
                    .map(|op| PatchOpJson {
                        class: op.class.clone(),
                        xpath: op.xpath.clone(),
                        sub_path: op.sub_path.clone(),
                        find_mod_context: op.find_mod_context.iter().map(format_gate).collect(),
                        may_require: op.may_require.clone(),
                        may_require_any_of: op.may_require_any_of.clone(),
                        locator_file: op.locator_file.display().to_string(),
                        is_wrapped: op.is_wrapped,
                    })
                    .collect(),
                replay_ok: patcher.replay.is_ok(),
                replay_error: patcher.replay.as_ref().err().cloned(),
                reached: patcher.reached,
                caveats: patcher.caveats.iter().map(ToString::to_string).collect(),
            })
            .collect(),
        parents: inspection
            .parents
            .iter()
            .map(|(key, owner)| KeyOwnerJson {
                def_key: key.to_string(),
                mod_id: owner.to_string(),
            })
            .collect(),
        children: inspection
            .children
            .iter()
            .map(|(owner, key)| KeyOwnerJson {
                def_key: key.to_string(),
                mod_id: owner.to_string(),
            })
            .collect(),
        completeness,
        stopped_at,
        caveats: inspection
            .effective
            .caveats
            .iter()
            .map(ToString::to_string)
            .collect(),
        fields: effective_fields::fields(&inspection.effective)
            .map(|field| FieldJson {
                path: field.path.to_string(),
                provenance: format_provenance(&field.provenance),
            })
            .collect(),
        resolved_xml: inspection.resolved_xml.clone(),
        findings: inspection
            .findings
            .iter()
            .map(|(key, status)| FindingJson {
                key: key.to_string(),
                status: format_status(*status).to_string(),
            })
            .collect(),
    };

    let text = serde_json::to_string_pretty(&json).context("serializing the inspection as JSON")?;
    println!("{text}");
    Ok(())
}

// ---------------------------------------------------------------------
// `defs search`
// ---------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Text to match against a def/template's name, then its type
    /// (case-insensitive substring, active mods only).
    query: String,
    /// How many hits to print, capped at `MAX_PAGE_SIZE` (also this
    /// flag's own default — the CLI's own default, larger than the
    /// desktop's own smaller 10-hit page).
    #[arg(long, default_value_t = MAX_PAGE_SIZE)]
    limit: usize,
    /// Print machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct SearchHitJson {
    def_ref: String,
    owners: usize,
}

fn run_search(args: &SearchArgs) -> anyhow::Result<()> {
    let paths = resolve_paths(&args.paths)?;
    let session = build_session(paths)?;
    let hits = session.search_defs(&args.query, args.limit);

    if args.json {
        let json: Vec<SearchHitJson> = hits
            .iter()
            .map(|(def_ref, owners)| SearchHitJson {
                def_ref: def_ref.to_string(),
                owners: *owners,
            })
            .collect();
        let text =
            serde_json::to_string_pretty(&json).context("serializing search results as JSON")?;
        println!("{text}");
        return Ok(());
    }

    if hits.is_empty() {
        println!("no defs or templates match {:?}", args.query);
        return Ok(());
    }
    println!("{:<48} owners", "def_ref");
    for (def_ref, owners) in &hits {
        println!("{:<48} {owners}", TerminalSafe::line(def_ref).to_string());
    }
    Ok(())
}
