//! [`Session`]: the aggregate holding one loaded project's full state —
//! the report, evidence, rules, tagging, decisions, settings, the
//! sorter's outcome, both orders, the selected source, and lazily-built
//! per-source ledgers and finding indices.
//!
//! `Session` is single-writer: every mutation takes `&mut self` and
//! nothing here is `Sync`. A composition root sharing one session across
//! calls (a CLI reusing it across commands, a Tauri app across IPC
//! commands) must serialize access itself — e.g. behind a
//! `std::sync::Mutex`/`RwLock` — rather than cloning it or calling its
//! methods concurrently.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{Conflict, LoadOrder, Mod, ModId, Report};
use rim_resolve::domain::{
    Action, AssignmentId, AssignmentProject, BothOrders, Coverage, DecisionSet, DefRef,
    GeneratedModIdentity, Ledger, LedgerStats, ManualTag, MergeState, OrderSource, PatchId,
    PatchProject, Placement, PlacementRule, ResolutionStatus, Rule, RuleOrigin, RuleSet,
    TagEvidence, TagMode, Tagging, merge_status,
};
use rim_resolve::sort::{self, SortInput, SortOutcome};
use rim_resolve::tags::infer_tags;

use crate::ModKnowledge;
use crate::ProjectPaths;
use crate::active_set::ActiveSet;
use crate::finding_index::FindingIndex;
use crate::merge_workspace::{MergeWorkspace, PreviewSlot};
use crate::ports::{DefSourceReader, ModsConfigFile, RulesLoadWarning, StoredRules};
use crate::settings::{Settings, filter_imported_rules};

mod assignments;
mod findings;
mod merge;
mod mod_info;
mod order;
mod patches;
mod rules;

pub use assignments::UnknownAssignment;
pub use patches::{PatchDecideError, UnknownPatch};
pub use rules::RuleKey;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "session/session_tests.rs"]
mod tests;

fn mods_by_id(report: &Report) -> BTreeMap<ModId, &Mod> {
    report.mods.iter().map(|m| (m.id.clone(), m)).collect()
}

/// Nearest-at-or-before resolution, per child (ground-truthed against the
/// decompiled `Verse.XmlInheritance.GetBestParentFor`):
/// [`rim_resolve::domain::DecisionSet::sorter_overrides`]'s
/// own `template_children` input — every mod, keyed by the duplicated
/// template `Name` it belongs to, with a def or template whose own
/// `ParentName` names that `Name`, **excluding**:
/// - a mod that is itself one of the name's own registrants (per
///   `GetBestParentFor`, a mod's own child always resolves to that same
///   mod's own registration, at the mod's own `loadOrder` — no `Reorder`
///   this crate could add would ever change that, and including it in
///   this set would create a same-name reorder contradiction with the
///   registrant-side pairs `sorter_overrides` already adds);
/// - a vanilla-sourced mod (a *vanilla* child's own resolution inverts —
///   `GetBestParentFor` prefers a vanilla registration first, regardless
///   of load order, and only falls back to the lowest-`loadOrder` mod
///   when none exists — neither case is ever influenced by which mod
///   `PreferWinner` names).
///
/// Aggregated across every `(def_type, Name)` [`SourceIndex::children_by_template`]
/// entry sharing that `Name` — RimWorld's own `XmlInheritance` dictionary
/// ignores the enclosing tag entirely, the same "global dictionary"
/// aggregation `use_cases::inspect_def::name_only_children`'s own
/// cross-type merge already relies on for a name-only `DefRef`.
///
/// Computed fresh from `report`/`sources` alone — deliberately
/// independent of `decisions` — so [`Session::decide`]/
/// [`Session::revert_decision`]'s own before/after `sorter_overrides`
/// comparison can build this once and reuse the identical map for both
/// calls, rather than it silently drifting between them because the very
/// decision being recorded changed which names it would otherwise be
/// scoped to.
fn duplicate_template_children(
    report: &Report,
    sources: &SourceIndex,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> BTreeMap<String, BTreeSet<ModId>> {
    let mut by_name: BTreeMap<&str, BTreeSet<&ModId>> = BTreeMap::new();
    for ((_, name), kids) in &sources.children_by_template {
        let entry = by_name.entry(name.as_str()).or_default();
        for (child_owner, _) in kids {
            entry.insert(child_owner);
        }
    }
    let mut result = BTreeMap::new();
    for conflict in &report.conflicts {
        let Conflict::DuplicateTemplateName(duplicate) = conflict else {
            continue;
        };
        let owners: BTreeSet<&ModId> = duplicate.owners.iter().collect();
        let mut children = BTreeSet::new();
        if let Some(candidates) = by_name.get(duplicate.name.as_str()) {
            for &child in candidates {
                if owners.contains(child) {
                    continue;
                }
                if mods_by_id.get(child).is_some_and(|m| m.source.is_vanilla()) {
                    continue;
                }
                children.insert(child.clone());
            }
        }
        result.insert(duplicate.name.clone(), children);
    }
    result
}

/// Every active instance of one assignment def type, flattened —
/// [`Session::cached_assignment_instances`]/[`Session::cache_assignment_instances`]'s
/// own value shape, named so its `Arc<Vec<(ModId, InstanceValues)>>`
/// nesting is stated once rather than repeated at every call site.
type AssignmentInstanceList = Arc<Vec<(ModId, rim_resolve::domain::InstanceValues)>>;

fn adjust_stat(stats: &mut LedgerStats, status: ResolutionStatus, delta: isize) {
    let field = match status {
        ResolutionStatus::Auto => &mut stats.auto,
        ResolutionStatus::NeedsInput => &mut stats.needs_input,
        ResolutionStatus::UserOverridden => &mut stats.overridden,
    };
    *field = field.saturating_add_signed(delta);
}

/// Fills in [`rim_resolve::domain::Resolution::merge`] and re-derives
/// `status` for every `Merge` decision this ledger already cached a
/// preview for (see [`crate::merge_workspace::MergeWorkspace`]),
/// re-tallying [`LedgerStats`] to match.
///
/// A deliberate, documented gap: this
/// crate has no I/O port of its own to *compute* a missing preview here —
/// only [`crate::use_cases::PlanMerge`] (which holds a `DefSourceReader`)
/// can do that. In practice the cache stays warm because
/// [`crate::use_cases::DecideMerge`] always recomputes the preview for
/// the key it just decided, and the merge editor/CLI call `PlanMerge`
/// directly for any key it displays; a `Merge` decision this ledger has
/// never previewed simply keeps `ledger::build`'s own default
/// (`UserOverridden`, `merge: None`) until something previews it.
fn apply_merge_states(ledger: &mut Ledger, merges: &MergeWorkspace, slot: &PreviewSlot) {
    for entry in &mut ledger.entries {
        let Some(decision) = &entry.decision else {
            continue;
        };
        if !matches!(decision.action, Action::Merge { .. }) {
            continue;
        }
        let Some(preview) = merges.get(slot, &entry.key) else {
            continue;
        };
        let state = preview.state.clone();
        let new_status = merge_status(&state);
        if new_status != entry.status {
            adjust_stat(&mut ledger.stats, entry.status, -1);
            adjust_stat(&mut ledger.stats, new_status, 1);
            entry.status = new_status;
        }
        match &state {
            MergeState::Complete { .. } => ledger.stats.merged += 1,
            MergeState::NeedsFieldInput { .. } | MergeState::CannotMerge { .. } => {
                ledger.stats.merge_incomplete += 1;
            }
        }
        // A
        // pre-existing `Merge` decision on a def that has since become
        // guarded still reaches this branch (the guard forces `state` to
        // `NeedsFieldInput`, never `CannotMerge`) — carry the guard field
        // through alongside `merge` so the inbox card and merge-mod entry
        // list can both say "confirm the winner" instead of a field count.
        entry.structural_guard_field = preview.structural_guard_field();
        entry.merge = Some(state);
    }
}

/// Whether any decision on file is a `Merge`/`ShipAsset` action — the
/// generated merge mod exists (or is about to) whenever this is true, so
/// [`compute`] gives it a placement even before the first apply.
fn has_merge_decision(decisions: &DecisionSet) -> bool {
    decisions.iter().any(|decision| {
        matches!(
            decision.action,
            Action::Merge { .. } | Action::ShipAsset { .. }
        )
    })
}

/// The synthetic Bottom [`PlacementRule`] for the generated merge mod,
/// when it should exist at all: the id is already in the current order
/// (a previous apply wrote it), or a `Merge`/`ShipAsset` decision exists
/// (the next apply will write it). `None` otherwise, so a project with no merge activity at
/// all never carries a placement rule for a mod that doesn't exist.
fn merge_mod_placement(
    profile_hash: &str,
    current: &LoadOrder,
    decisions: &DecisionSet,
) -> Option<PlacementRule> {
    let identity = GeneratedModIdentity::for_profile(profile_hash);
    let already_present = current.position(&identity.package_id).is_some();
    if !already_present && !has_merge_decision(decisions) {
        return None;
    }
    Some(PlacementRule {
        mod_id: identity.package_id,
        placement: Placement::Bottom,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })
}

/// Builds the tagging, sort outcome, and effective (filtered, merged, plus
/// the synthetic merge-mod placement) rule set that follow from the given
/// facts — a free function (not a method) so [`Session::new`] can call it
/// before `self` exists. The rule set is returned (not just consumed
/// internally) so [`Session::recompute_sort`] can cache it as
/// [`Session::effective_rules`] — the placement-origin lookup needs the
/// exact set that produced `sort_outcome` to name a placement's own
/// origin back out of it in `ledger::extract_findings`.
fn compute(
    report: &Report,
    evidence: &[TagEvidence],
    rules: &StoredRules,
    decisions: &DecisionSet,
    current: &LoadOrder,
    profile_hash: &str,
    sources: &SourceIndex,
) -> (Tagging, SortOutcome, RuleSet) {
    let template_children = duplicate_template_children(report, sources, &mods_by_id(report));
    let overrides = decisions.sorter_overrides(&template_children);

    // `AddTag`/`RemoveTag` decisions are additional manual overrides on
    // top of the rules page's own `manual_tags` — concatenating them
    // works regardless of order because `infer_tags` processes every
    // `Remove` first, then every `Add`, across the whole slice.
    let mut manual: Vec<ManualTag> = rules.manual_tags.clone();
    manual.extend(
        overrides
            .added_tags
            .iter()
            .cloned()
            .map(|(mod_id, tag)| ManualTag {
                mod_id,
                tag,
                mode: TagMode::Add,
            }),
    );
    manual.extend(
        overrides
            .removed_tags
            .iter()
            .cloned()
            .map(|(mod_id, tag)| ManualTag {
                mod_id,
                tag,
                mode: TagMode::Remove,
            }),
    );

    let tagging = infer_tags(evidence, &rules.tag_rules, &manual);
    // Cluster membership is a scheduling decision, not just a display
    // label (see `Tagging::accepted`'s own doc comment): only tags the
    // ledger would already auto-accept at the current threshold — a
    // manual tag, or an inferred one confident enough — may move a mod
    // into a contiguous cluster. `self.tagging` (returned below) stays
    // the full, unfiltered view so the rules page can still show every
    // inferred tag alongside its confidence.
    let accepted = tagging.accepted(rules.settings.threshold);
    // The two import toggles
    // are applied before the synthetic merge-mod placement is merged in
    // — that placement is always `UserDecision`-origin (never filtered)
    // and isn't part of what the user imported, so filtering after would
    // change nothing; filtering first just keeps the two concerns apart.
    let filtered_rules = filter_imported_rules(
        rules.rule_set(),
        rules.settings.use_imported_pairs,
        rules.settings.use_imported_placements,
    );
    let rule_set = match merge_mod_placement(profile_hash, current, decisions) {
        Some(placement) => RuleSet::merged([
            filtered_rules,
            RuleSet::new(vec![Rule::Placement(placement)]),
        ]),
        None => filtered_rules,
    };
    let sort_outcome = sort::sort(&SortInput {
        report,
        rules: &rule_set,
        tagging: &accepted,
        overrides: &overrides,
        current,
        enforce: rules.settings.enforce,
        tie_break: rules.settings.tie_break,
    });
    (tagging, sort_outcome, rule_set)
}

/// One loaded project's full state.
pub struct Session {
    paths: ProjectPaths,
    report: Report,
    evidence: Vec<TagEvidence>,
    /// Where every def, template, and mutating patch op lives on disk —
    /// what [`crate::use_cases::PlanMerge`] reads back XML through.
    sources: SourceIndex,
    rules: StoredRules,
    tagging: Tagging,
    decisions: DecisionSet,
    sort: SortOutcome,
    /// The exact (filtered, merged, plus the synthetic merge-mod
    /// placement) [`RuleSet`] that produced `sort` — kept alongside it
    /// (not re-derived) so [`Session::ensure_ledger`]/
    /// [`Session::ensure_scoped_ledger`] can pass the same set
    /// `ledger::extract_findings` needs for its placement-origin lookup.
    /// Always in sync
    /// with `sort`: both are written together, only by [`compute`].
    effective_rules: RuleSet,
    orders: BothOrders,
    /// `ModsConfig.xml`'s own fields, preserved verbatim for
    /// [`Session::mods_config_file_for`] to round-trip on apply.
    mods_config_version: String,
    known_expansions: Vec<ModId>,
    /// The working (in-memory, unsaved) active-mod list. Initialised from
    /// `orders.current` at construction (see [`ActiveSet::trusted`]'s own
    /// doc comment for why that's always valid); mutated only by
    /// [`crate::use_cases::ActivateMods`]/[`crate::use_cases::DeactivateMods`],
    /// which never touch `orders`, the ledgers, or the sorter — the report
    /// doesn't know about a pending change yet, so re-sorting would be a
    /// lie. This is the *only* in-memory-only state a mutating use case in
    /// this crate carries; see this file's own top-of-file doc comment.
    working: ActiveSet,
    /// The active-mod list `ModsConfig.xml` actually held at load time —
    /// updated by [`Session::set_current_order`] (whose own contract is
    /// "the file now *is* this") and, for a session built by
    /// [`crate::use_cases::Rescan`], by [`Session::set_file_active_mods`]
    /// right after construction (the composition-root-sets-it-after-
    /// construction pattern [`Session::set_rule_load_warnings`] already
    /// uses) — a rescan's own `orders.current` is the *working* set being
    /// previewed, not the file's, so the two must be tracked separately
    /// for [`Session::pending_changes`]'s `unapplied` diff to mean
    /// anything.
    file_active_mods: Vec<ModId>,
    /// Indexed `[OrderSource::Current as usize, OrderSource::Suggested as usize]`.
    ledgers: [Option<Ledger>; 2],
    /// A copy of `ledgers`, taken the moment each is built, before
    /// [`Session::redecide_clean_merge_at`] (the lazy clean-merge
    /// promotion) can mutate `ledgers`' own entries in place. A patch's
    /// scoped ledger (`Session::ensure_scoped_ledger`) always derives from
    /// this copy, never from `ledgers` directly: `ledgers` goes stale
    /// out from under a *cached* scoped ledger the moment a promotion
    /// runs (nothing invalidates `self.scoped` when that happens), and a
    /// *freshly built* scoped ledger would otherwise bake in whichever of
    /// the two states — promoted or not — some unrelated earlier call
    /// happened to have already forced. Cleared alongside `ledgers` by
    /// [`Session::invalidate_ledgers`].
    pristine_ledgers: [Option<Ledger>; 2],
    finding_index: [Option<FindingIndex>; 2],
    /// Per-finding merge previews, cached per order source; invalidated
    /// with the ledgers.
    merges: MergeWorkspace,
    /// Every compat patch project loaded with this profile, keyed by id.
    patches: BTreeMap<PatchId, PatchProject>,
    /// Every patch maker project loaded with this profile, keyed by id
    /// — the assignment twin of
    /// [`Self::patches`]. Independent of it: an assignment project has no
    /// decisions/findings of its own, so it needs none of `patches`' own
    /// scoped-ledger machinery.
    assignments: BTreeMap<AssignmentId, AssignmentProject>,
    /// Lazily built per `(patch, source)`: that patch's own ledger and
    /// finding index, derived from the profile's via
    /// [`rim_resolve::ledger::scoped`]. Cleared for a given patch by
    /// [`Session::invalidate_patch_caches`] on that patch's own changes,
    /// and wholesale by [`Session::invalidate_ledgers`] on any profile
    /// change (a patch's scoped ledger is derived from the profile's, so
    /// it goes stale whenever the profile's own does). A `HashMap`, not a
    /// `BTreeMap` — like [`FindingIndex`]'s own `by_key`, this is a
    /// point-lookup cache never iterated for output, and `OrderSource`
    /// carries no `Ord` of its own to key a `BTreeMap` with (see
    /// `crate::merge_workspace::PreviewSlot`'s own doc comment on the same
    /// point).
    scoped: HashMap<(PatchId, OrderSource), (Ledger, FindingIndex)>,
    /// Non-fatal warnings [`crate::ports::RuleStore::load`] noticed while
    /// reading the rules file (currently only the dropped-cluster-rules
    /// migration) — set once by the
    /// composition root right after [`Session::new`]/
    /// [`crate::use_cases::LoadProject::execute`], the same way
    /// [`Session::set_def_source_reader`] threads its own capability.
    /// Empty for a freshly-built `Session` most tests use directly.
    rule_load_warnings: Vec<RulesLoadWarning>,
    /// The mod-specific knowledge that lives in data, set once by
    /// [`crate::use_cases::LoadProject`] from its
    /// [`crate::ports::ModKnowledgeStore`] — the same
    /// set-right-after-construction threading
    /// [`Session::set_rule_load_warnings`] uses. Default (knows nothing)
    /// for a freshly-built `Session`, which is what most tests want; see
    /// [`ModKnowledge`]'s own doc comment for what that degrades.
    mod_knowledge: ModKnowledge,
    selected: OrderSource,
    /// Set by the composition root ([`Session::set_def_source_reader`])
    /// once real XML I/O is available. `None` in most tests (and in a
    /// freshly-built `Session` before the composition root wires it up),
    /// in which case [`Session::redecide_clean_merge_at`]'s clean-merge
    /// pass is simply skipped — every finding keeps the ledger's own
    /// suggestion, exactly like `suggestMergeWhenClean: false`. Threaded
    /// this way (rather than a generic `Reader` type parameter on
    /// `Session` itself) as the smallest way to wire it: a type parameter
    /// would spread through
    /// every method signature and every test fixture in this crate, for a
    /// capability only that one lazy pass uses.
    def_source_reader: Option<Arc<dyn DefSourceReader + Send + Sync>>,
    /// Per-`(OrderSource, DefRef)` [`crate::use_cases::DefInspection`]
    /// cache — a point lookup,
    /// never iterated for output, so a `HashMap` like [`Self::scoped`]'s
    /// own per-slot map (`OrderSource` carries no `Ord`). Deliberately
    /// **not** cleared by [`Session::invalidate_ledgers`]: an inspection
    /// reads only the selected order, which mods are active, and the raw
    /// XML on disk — none of which a plain decision/rule/tag/settings/
    /// import change touches when it doesn't also change `orders` (see
    /// `crate::changes`' own "pure and order-independent" framing for the
    /// same reasoning applied to `Session::changes`). Cleared only where
    /// `orders` itself gets a new value — [`Session::recompute_sort`] (any
    /// resort) and [`Session::set_current_order`] — and, trivially, by a
    /// rescan building a brand new `Session`. Also bounded at
    /// [`Session::MAX_CACHED_INSPECTIONS`] entries: unlike `merges`
    /// (naturally capped by however many findings are currently in the
    /// ledger), an inspection can be built for *any* def or template in
    /// the install, so a long session that inspects many different ones
    /// would otherwise grow this map without limit — see
    /// [`Session::cache_inspection`]'s own doc comment for the (blunt,
    /// whole-cache-clear) eviction policy.
    inspections: HashMap<(OrderSource, DefRef), crate::use_cases::DefInspection>,
    /// Per-`(OrderSource, def_type)` cache of every active instance of one
    /// assignment def type, already flattened
    /// (`rim_merge::assign::read_instance`) —
    /// "read once per session and reused by inference and
    /// coverage". `Arc` so `CreateAssignment`/`UpdateAssignment`/a later
    /// `AssignmentCoverage` can all hold their own reference without
    /// cloning every instance's flattened field map. Same invalidation
    /// reasoning as [`Self::inspections`] (deliberately **not** cleared by
    /// [`Session::invalidate_ledgers`]): reading an instance's raw XML
    /// depends only on the selected order and which mods are active,
    /// neither of which a plain decision/rule/tag/settings/import change
    /// touches — cleared only where `orders` itself gets a new value.
    assignment_instances: HashMap<(OrderSource, String), AssignmentInstanceList>,
    /// Per-`(OrderSource, AssignmentId, def_type)`
    /// [`crate::use_cases::AssignmentCoverage`] cache
    /// (coverage is per target-keyed section, so the cache key
    /// includes the section's own def type). Cleared alongside
    /// [`Self::assignment_instances`] wherever `orders` gets a new value
    /// ([`Session::recompute_sort`]/[`Session::set_current_order`] — the
    /// same "which instance wins depends on the selected order"
    /// reasoning), **and** per id (every section's own entry) whenever
    /// that project's own state changes
    /// ([`Session::upsert_assignment`]/[`Session::remove_assignment`] —
    /// this also covers a section add/remove, since both go through
    /// `upsert_assignment`): unlike `assignment_instances` (which reads
    /// only the active install's raw XML), a coverage result also
    /// reflects the project's own rows (`has_row`, and `winner` computed
    /// "as if already exported"), so a row/section edit must invalidate it
    /// even though nothing about the selected order changed.
    assignment_coverage: HashMap<(OrderSource, AssignmentId, String), Coverage>,
    /// Per-`(OrderSource, DefRef)` cache of what a def's texture resolves
    /// to ([`crate::use_cases::ResolveDefGraphic`]). Selected-order family,
    /// like [`Self::inspections`]: it reads the effective def, the texture
    /// index and the order, so it is cleared only where `orders` gets a new
    /// value. Bounded at [`Session::MAX_CACHED_DEF_GRAPHICS`] (cleared
    /// whole, like `inspections`): a refetch stays cheap even after the
    /// smaller inspection cache has been thrashed by a long queue.
    def_graphics: HashMap<(OrderSource, DefRef), crate::use_cases::DefGraphic>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("paths", &self.paths)
            .field("report", &self.report)
            .field("evidence", &self.evidence)
            .field("sources", &self.sources)
            .field("rules", &self.rules)
            .field("tagging", &self.tagging)
            .field("decisions", &self.decisions)
            .field("sort", &self.sort)
            .field("effective_rules", &self.effective_rules)
            .field("orders", &self.orders)
            .field("mods_config_version", &self.mods_config_version)
            .field("known_expansions", &self.known_expansions)
            .field("working", &self.working)
            .field("file_active_mods", &self.file_active_mods)
            .field("ledgers", &self.ledgers)
            .field("pristine_ledgers", &self.pristine_ledgers)
            .field("finding_index", &self.finding_index)
            .field("merges", &self.merges)
            .field("patches", &self.patches)
            .field("assignments", &self.assignments)
            .field("scoped", &self.scoped)
            .field("rule_load_warnings", &self.rule_load_warnings)
            .field("selected", &self.selected)
            .field(
                "def_source_reader",
                &self
                    .def_source_reader
                    .as_ref()
                    .map(|_| "<dyn DefSourceReader>"),
            )
            .field("inspections", &self.inspections)
            .field("assignment_instances", &self.assignment_instances)
            .field("assignment_coverage", &self.assignment_coverage)
            .field("def_graphics", &self.def_graphics.len())
            .finish()
    }
}

impl Session {
    /// See [`Self::cache_inspection`]'s own doc comment.
    const MAX_CACHED_INSPECTIONS: usize = 256;

    /// See [`Self::def_graphics`]' own doc comment.
    const MAX_CACHED_DEF_GRAPHICS: usize = 1024;

    fn slot(source: OrderSource) -> usize {
        match source {
            OrderSource::Current => 0,
            OrderSource::Suggested => 1,
        }
    }

    /// Builds a new session from everything a fresh scan/load produced,
    /// plus every compat patch and patch maker project
    /// [`crate::use_cases::LoadProject`] loaded alongside it.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "every field is a distinct piece of a freshly loaded project; grouping them into a struct would only rename this same list one level down, and every existing caller already passes them positionally"
    )]
    pub fn new(
        paths: ProjectPaths,
        report: Report,
        evidence: Vec<TagEvidence>,
        sources: SourceIndex,
        rules: StoredRules,
        decisions: DecisionSet,
        mods_config: ModsConfigFile,
        patches: Vec<PatchProject>,
        assignments: Vec<AssignmentProject>,
    ) -> Self {
        let current_order = LoadOrder::new(mods_config.active_mods);
        // Captured before `current_order` moves into `orders` below —
        // the ordinary-load default for both `working` and
        // `file_active_mods` is "exactly what the file said", which a
        // rescan's own `LoadProject::execute_with_active_set` corrects
        // afterward via `Self::set_file_active_mods` (see that method's
        // own doc comment; `working` needs no equivalent correction,
        // since a rescan's whole point is to preview the working set
        // that already existed before it ran).
        let file_active_mods = current_order.as_slice().to_vec();
        let working = ActiveSet::trusted(file_active_mods.clone());
        let profile_hash = paths.profile_hash().to_string();
        let (tagging, sort_outcome, effective_rules) = compute(
            &report,
            &evidence,
            &rules,
            &decisions,
            &current_order,
            &profile_hash,
            &sources,
        );
        let orders = BothOrders {
            current: current_order,
            suggested: sort_outcome.order.clone(),
        };
        Self {
            paths,
            report,
            evidence,
            sources,
            rules,
            tagging,
            decisions,
            sort: sort_outcome,
            effective_rules,
            orders,
            mods_config_version: mods_config.version,
            known_expansions: mods_config.known_expansions,
            working,
            file_active_mods,
            ledgers: [None, None],
            pristine_ledgers: [None, None],
            finding_index: [None, None],
            merges: MergeWorkspace::default(),
            patches: patches.into_iter().map(|p| (p.id().clone(), p)).collect(),
            assignments: assignments
                .into_iter()
                .map(|p| (p.id().clone(), p))
                .collect(),
            scoped: HashMap::new(),
            rule_load_warnings: Vec::new(),
            mod_knowledge: ModKnowledge::default(),
            selected: OrderSource::Current,
            def_source_reader: None,
            inspections: HashMap::new(),
            assignment_instances: HashMap::new(),
            assignment_coverage: HashMap::new(),
            def_graphics: HashMap::new(),
        }
    }

    /// Records the warnings [`crate::ports::RuleStore::load`] returned
    /// alongside the rules it read. Set once by the composition root right
    /// after construction (see [`Session::set_def_source_reader`] for the
    /// same threading pattern) — never mutated afterward, since a session
    /// only ever loads its rules file once.
    pub fn set_rule_load_warnings(&mut self, warnings: Vec<RulesLoadWarning>) {
        self.rule_load_warnings = warnings;
    }

    /// Records the mod-specific knowledge
    /// [`crate::ports::ModKnowledgeStore::load`] returned. Set once by
    /// [`crate::use_cases::LoadProject`] right after construction, the
    /// same way [`Session::set_rule_load_warnings`] is.
    pub fn set_mod_knowledge(&mut self, knowledge: ModKnowledge) {
        self.mod_knowledge = knowledge;
    }

    /// This session's own mod-specific knowledge — precedence rules,
    /// patch-operation behaviours, def-cache carriers. See
    /// [`ModKnowledge`].
    #[must_use]
    pub fn mod_knowledge(&self) -> &ModKnowledge {
        &self.mod_knowledge
    }

    /// The active-mod list `ModsConfig.xml` held when this session was
    /// loaded (or last written), by exact ids in file order: what an
    /// import is diffed against.
    #[must_use]
    pub fn file_active_mods(&self) -> &[ModId] {
        &self.file_active_mods
    }

    /// Overrides `file_active_mods` with `ids` — [`crate::use_cases::Rescan`]'s
    /// own correction, right after construction, for exactly the case
    /// [`Session::new`]'s own default (`ModsConfig.xml`'s own
    /// `<activeMods>`) is wrong: a session built from
    /// [`crate::use_cases::LoadProject::execute_with_active_set`] has
    /// `orders().current` set to the *working* set being previewed, not
    /// the file's, so `file_active_mods` must be told the file's real
    /// list separately or [`Session::pending_changes`]'s `unapplied` diff
    /// would compare the working set against itself. `pub(crate)`: only
    /// [`crate::use_cases::LoadProject`] (in the same crate) should ever
    /// call this.
    pub(crate) fn set_file_active_mods(&mut self, ids: Vec<ModId>) {
        self.file_active_mods = ids;
    }

    /// Non-fatal warnings noticed while loading this session's rules file.
    #[must_use]
    pub fn rule_load_warnings(&self) -> &[RulesLoadWarning] {
        &self.rule_load_warnings
    }

    /// Gives this session a [`DefSourceReader`], so
    /// [`Session::redecide_clean_merge_at`]'s clean-merge pass (see
    /// [`Settings::suggest_merge_when_clean`]) can compute a missing
    /// preview instead of only reusing whatever's already cached. Set
    /// once by the composition root right after [`Session::new`]/
    /// [`crate::use_cases::LoadProject::execute`] — never required: a
    /// session with none simply skips the pass.
    pub fn set_def_source_reader(&mut self, reader: Arc<dyn DefSourceReader + Send + Sync>) {
        self.def_source_reader = Some(reader);
    }

    /// The package id of this profile's own generated merge mod, derived
    /// from the profile hash (never stored).
    #[must_use]
    pub fn own_merge_mod_id(&self) -> ModId {
        GeneratedModIdentity::for_profile(self.paths.profile_hash()).package_id
    }

    /// The installed game's version text, or `None` when the scan found
    /// none.
    #[must_use]
    pub fn game_version(&self) -> Option<String> {
        Some(self.report.metadata.game_version.clone()).filter(|version| !version.is_empty())
    }

    /// This session's filesystem locations.
    #[must_use]
    pub fn paths(&self) -> &ProjectPaths {
        &self.paths
    }

    /// Where every def, template, and mutating patch op lives on disk.
    #[must_use]
    pub fn sources(&self) -> &SourceIndex {
        &self.sources
    }

    /// The analyzer report this session was built from.
    #[must_use]
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// The tag evidence collected alongside the report.
    #[must_use]
    pub fn evidence(&self) -> &[TagEvidence] {
        &self.evidence
    }

    /// Every rule and setting currently in effect.
    #[must_use]
    pub fn rules(&self) -> &StoredRules {
        &self.rules
    }

    /// The current tag assignments (inference plus manual overrides).
    #[must_use]
    pub fn tagging(&self) -> &Tagging {
        &self.tagging
    }

    /// Every decision on file.
    #[must_use]
    pub fn decisions(&self) -> &DecisionSet {
        &self.decisions
    }

    /// The current sorter/ledger settings.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.rules.settings
    }
}
