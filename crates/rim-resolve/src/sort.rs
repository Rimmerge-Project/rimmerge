//! The Rimmerge sorter: builds one [`LoadOrder`] from an analyzer
//! [`Report`], a [`RuleSet`], a [`Tagging`], and any user
//! [`SorterOverrides`].
//!
//! The algorithm (see `docs/concepts/sorting.md`): mod nodes plus five
//! tier sentinel chains (there is no `Framework` tier — see
//! `sort/tiers.rs`'s module doc), edges added in layers of decreasing
//! precedence ([`Layer::ALL`]) with layer-batched Tarjan cycle breaking
//! after each, any-of constraint resolution, and a final Kahn emission
//! that also records why every mod landed where it did.
//!
//! There is no cluster layer and no cluster-contiguity condensation step.
//! See `sort/tiers.rs`'s module doc for the `Body` tier's own (weakest)
//! membership layer.

mod any_of;
mod cycles;
mod direction;
mod disturbance;
mod emit;
mod explain;
mod graph;
mod layers;
mod tiers;

use std::collections::BTreeMap;

use rim_analyzer::domain::{EdgeKind, LoadOrder, ModId, Report};

use crate::domain::{RuleOrigin, RuleSet, SorterOverrides, Tagging};

pub use disturbance::DisturbanceStats;
pub use explain::{AdvisoryEdge, PlacementExplanation, PlacementTieBreak, TierReason};
pub use tiers::Tier;

/// Which base key [`sort`] gives every mod before propagation — not to be
/// confused with
/// [`PlacementTieBreak`], the per-mod *result* either mode produces.
///
/// Membership is the active list (`report.mods`) in both modes: a mod
/// absent from `SortInput::current` is never dropped in `PreserveCurrent`
/// (it simply sorts last, same as always), and `Rebuild` never adds a mod
/// beyond that same active set either — rebuilding the base key is not
/// the same as rebuilding *which* mods are nodes at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TieBreak {
    /// Every mod's base key is its own position in
    /// [`SortInput::current`] — for anyone who wants minimal disturbance
    /// from a list they already trust.
    PreserveCurrent,
    /// Ignore the current order entirely: every mod's base key is its
    /// rank when the active list is sorted by `sort::graph::normalized_rebuild_name`
    /// (case-insensitive, a leading bracketed/parenthesised tag or version
    /// prefix stripped first — then [`ModId`] as the tie-break for two mods
    /// sharing a normalized name). **Unlike
    /// `PreserveCurrent`, this mode does not propagate at all**
    /// (`sort::emit::non_propagated_keys`): pulling a prerequisite
    /// forward answers "how do I disturb the *current* order as little as
    /// possible", a question a from-scratch rebuild never asks. Combined
    /// with the same Kahn emission `PreserveCurrent` uses (just not its
    /// propagation), this yields RimSort's own "dependency layers,
    /// alphabetical inside each" shape. The default: a list built from the
    /// mods' own evidence is the sorter's whole point.
    #[default]
    Rebuild,
}

/// Everything [`sort`] needs: the analysis facts, every rule in effect,
/// tag membership, user overrides on top of the raw sort, and the
/// currently active order (used for tie-breaks and any-of preference).
pub struct SortInput<'a> {
    /// The analyzer's full report: mods, edges, and any-of constraints.
    pub report: &'a Report,
    /// Every pair/placement/incompatible rule in effect, already merged
    /// into one precedence-ordered set.
    pub rules: &'a RuleSet,
    /// Which mods carry which tags. Unused by the sorter itself (there is
    /// no tag-based scheduling); kept on `SortInput` as a stable seam for
    /// a future tag-driven ordering feature rather than churning every
    /// caller to build one less field.
    pub tagging: &'a Tagging,
    /// User decisions that change what the sorter builds.
    pub overrides: &'a SorterOverrides,
    /// The order currently active in `ModsConfig.xml`: always used for
    /// any-of candidate preference and for `DisturbanceStats`/
    /// `PlacementExplanation::previous_position`'s "how far from today"
    /// measurements, regardless of `tie_break`; used for the emission base
    /// key itself only in [`TieBreak::PreserveCurrent`].
    pub current: &'a LoadOrder,
    /// Which weak engine-edge layers are treated as real ordering
    /// constraints versus merely advisory.
    pub enforce: EnforcedLayers,
    /// Which base key every mod starts emission from before propagation.
    pub tie_break: TieBreak,
}

/// Which of the three weakest engine-edge strengths [`sort`] treats as a
/// real ordering constraint (added to the graph, able to force a move or
/// get dropped in a cycle) versus merely advisory (never added; only
/// reported per-mod via [`PlacementExplanation::advisory`]).
///
/// `Hard` and `Declared` engine edges and every rule-origin edge
/// (`RimSort`/`SteamDb`/user decisions) are never affected by this — only
/// `Inferred`/`Soft`/`Awareness` *engine* edges are ever advisory, and
/// only two of the three (`soft`/`awareness`) are ever advisory *by
/// default* — see `inferred`'s own default below. These are user-settable
/// strictness toggles, not a fixed policy: `soft`/`awareness` default off
/// because neither strength is an ordering promise RimWorld itself keeps
/// — a lazily-resolved `AssemblyRef` (`Soft`) resolves at JIT time, after
/// every mod's assemblies are already loaded, so which of the two mods
/// loads first genuinely doesn't matter in practice — but a user free to
/// judge their own mod list can turn either on. On a real install
/// (`PreserveCurrent`, no imported rules, `inferred: true`), enforcing
/// `Soft` on top adds thousands of Kendall-tau inversions for constraints
/// that don't actually affect the game; enforcing `Awareness` on top adds
/// roughly ten times as many and drops far more edges — `Awareness`
/// (`FindMod`/`MayRequire`/`PatchTargetsDef`/`IfModActive`/`UsesType`/
/// `PatchSelectsInjectedNode`; `ParentTemplate` is `Hard`, since the
/// decompiled `XmlInheritance.GetBestParentFor` shows an unresolvable
/// `ParentName` drops the def at load time) is evidence the mods
/// interact, not an ordering fact
/// (`ModDependency` is `Declared`, not `Awareness` — an author's declared
/// dependency is treated as an implicit `loadAfter`, always enforced
/// regardless of these toggles), and is far more numerous, so treating it
/// as a hard constraint is more disruptive than `Soft`. `Soft` violations
/// are never silently dropped, though: a `Soft` edge the suggested order
/// doesn't happen to satisfy still surfaces as a
/// `FindingKey::LazyReferenceViolated` finding (auto-resolved at high
/// confidence, but visible under "show all") so nothing is hidden by
/// turning enforcement off.
///
/// `soft`/`awareness` both default to `false` (advisory). `inferred`
/// defaults to **`true`** instead — a heuristic edge
/// (`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/`PatchInvalidatesPredicate`) is
/// the analyzer's own conclusion, not a mere presence signal the way
/// `Awareness` is, so leaving it advisory-only by default would silently
/// discard the very evidence `Layer::Inferred` exists to act on — see
/// that variant's own doc comment. This asymmetric default is why the
/// struct needs a manual `impl Default` rather than `#[derive(Default)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnforcedLayers {
    /// Whether `Soft`-strength engine edges (lazily-resolved
    /// `AssemblyRef`s) are added to the graph as real constraints.
    pub soft: bool,
    /// Whether `Awareness`-strength engine edges are added to the graph
    /// as real constraints.
    pub awareness: bool,
    /// Whether `Inferred`-strength engine edges
    /// (`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/
    /// `PatchInvalidatesPredicate`)
    /// are added to the graph as real constraints. Default `true` — see
    /// this struct's own doc comment.
    pub inferred: bool,
}

impl Default for EnforcedLayers {
    fn default() -> Self {
        Self {
            soft: false,
            awareness: false,
            inferred: true,
        }
    }
}

/// A load-order layer, in the precedence order [`sort`] adds them: an
/// earlier layer's edges are added — and cycles among them broken — before
/// the next layer's edges are even considered, so an edge from an earlier
/// layer is never the one dropped to satisfy a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Load-time `AssemblyRef`s and `forceLoad*` — breaking these breaks
    /// the game.
    Hard,
    /// A chosen candidate of an any-of constraint.
    AnyOf,
    /// The declared-edge override (deferred
    /// half of the pair-rule feature): a [`crate::domain::PairRule`] of
    /// [`crate::domain::RuleOrigin::UserDecision`] whose own
    /// `overrides_declared` is `true`. Sits directly ahead of `Declared`
    /// — never reordering any other layer's own precedence — so this
    /// edge is already committed by the time `Declared`'s own edges are
    /// added: a conflicting `loadAfter`/`modDependencies` edge is the one
    /// that loses the cycle, not this one, and this layer's own edges can
    /// in turn never be dropped by anything *weaker* than `Hard`/`AnyOf`
    /// (both added earlier still). Deliberately not `UserDecision` itself
    /// — an *ordinary* user decision or promoted import must keep losing
    /// to a `Declared` edge exactly as before; only a rule the user
    /// marked, per pair, as "this overrides the author's own declaration"
    /// reaches this layer at all. An unflagged `UserDecision`-origin pair
    /// rule, and every imported rule, still adds its edge at
    /// [`Self::UserDecision`]/its own layer, unaffected.
    DeclaredOverride,
    /// Author-declared `loadAfter`/`loadBefore`.
    Declared,
    /// The user's own reorder/prefer-winner decisions.
    UserDecision,
    /// Imported from RimSort's `userRules.json`.
    RimSortUser,
    /// Imported from RimSort's `communityRules.json`.
    RimSortCommunity,
    /// Imported from RimSort's `steamDB.json`.
    SteamDb,
    /// `PatchRemovedNode`/`RetextureAfterOwner`/
    /// `DefOverrideAfterOrigin` — a heuristic the analyzer infers from
    /// evidence rather than an author's own declaration. Sits after
    /// every imported rule (none of those are proven the way a `Hard`/
    /// `Declared` edge is either, but they're still someone's own
    /// authored word, imported or user-typed, which outranks a guess) and
    /// before `Soft`: strong enough to beat the bare name tie-break
    /// (`Layer::Soft` and weaker), otherwise the heuristic changes
    /// nothing, but weaker than anything with an actual author or user
    /// behind it. See `rim_analyzer::domain::EdgeStrength::Inferred`'s own
    /// doc comment for the full placement rationale.
    Inferred,
    /// A lazily-resolved `AssemblyRef`.
    Soft,
    /// `modDependencies`/`FindMod`/`IfModActive`/patch-target/`MayRequire`
    /// evidence, with no explicit ordering promise.
    Awareness,
}

impl Layer {
    /// Every variant, in the exact precedence order `sort::graph::run`
    /// adds them — the single source of truth `sort/graph.rs::LAYER_ORDER`
    /// is defined from, rather than a hand-maintained duplicate: the `match`
    /// driving the sort loop is exhaustive over `Layer`, so a *new* variant
    /// is a compile error there, but a variant simply missing from a
    /// duplicate array would compile fine and silently drop every edge of
    /// that layer, not even advisory (`sort::tiers::Tier::ALL` follows the
    /// same pattern). Keep this list and the enum's own variant
    /// list in sync by construction — `layer_all_contains_every_variant_exactly_once`
    /// (`sort/graph.rs` tests) fails loudly (a compile error in its own
    /// exhaustive match, or a length mismatch) if they ever drift.
    pub const ALL: [Layer; 11] = [
        Layer::Hard,
        Layer::AnyOf,
        Layer::DeclaredOverride,
        Layer::Declared,
        Layer::UserDecision,
        Layer::RimSortUser,
        Layer::RimSortCommunity,
        Layer::SteamDb,
        Layer::Inferred,
        Layer::Soft,
        Layer::Awareness,
    ];
}

/// Where an [`OrderingEdge`] came from. `Ord` is content-based (declaration
/// order between variants, then field order within one) — used by
/// `sort::cycles::break_cycles` as part of a permutation-invariant
/// cycle-drop tie-break, never to imply one provenance kind outranks
/// another.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeProvenance {
    /// A direct fact from the analyzer's report.
    Engine {
        /// The kind of engine edge.
        kind: EdgeKind,
        /// The engine's own description of the edge.
        detail: String,
    },
    /// The chosen candidate of an any-of constraint.
    AnyOf {
        /// The shared assembly name.
        assembly: String,
    },
    /// A pair or placement rule.
    Rule {
        /// Where the rule came from.
        origin: RuleOrigin,
        /// The rule's free-text note, if any.
        comment: Option<String>,
    },
    /// A tier sentinel boundary (see [`Tier`]).
    Tier {
        /// The tier this boundary belongs to.
        tier: Tier,
    },
}

/// One accepted load-order constraint between two mods: `after` must load
/// after `before`. `Ord` is content-based, field order — see
/// [`EdgeProvenance`]'s own doc comment for why it exists.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderingEdge {
    /// The dependent side: must load after `before`.
    pub after: ModId,
    /// The dependency side: must load before `after`.
    pub before: ModId,
    /// Which layer this edge was added in.
    pub layer: Layer,
    /// Where this edge came from.
    pub provenance: EdgeProvenance,
}

/// An edge the sorter could not honor: dropping it was the only way to
/// keep the graph acyclic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedEdge {
    /// The edge that had to be dropped.
    pub edge: OrderingEdge,
    /// A path from `edge.after` back around to `edge.before`, entirely
    /// through edges still accepted at the time of the drop — combined
    /// with `edge` itself (`before` -> `after`), proof of the cycle this
    /// edge would otherwise have closed.
    pub witness_cycle: Vec<ModId>,
    /// The edge that overruled this one, when `witness_cycle` is a direct
    /// two-mod contradiction (`witness_cycle.len() == 2`: the accepted
    /// graph already has a direct edge from `edge.after` straight back to
    /// `edge.before`, no intermediate mods involved), so the finding's
    /// rationale can name the higher-layer edge that overruled it. `None`
    /// for a
    /// longer cycle, where no single edge can be blamed for the
    /// contradiction.
    pub winner: Option<OrderingEdge>,
}

/// The outcome of resolving one any-of
/// [`rim_analyzer::domain::Constraint`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnyOfChoice {
    /// The mod requiring one of the candidates.
    pub after: ModId,
    /// The shared assembly name.
    pub assembly: String,
    /// The candidate chosen, or `None` if every candidate would have
    /// closed a cycle.
    pub chosen: Option<ModId>,
    /// Every candidate that could have satisfied the constraint.
    pub candidates: Vec<ModId>,
}

/// A non-fatal problem noticed while sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortWarning {
    /// A pair, placement, or incompatible rule names a mod that isn't
    /// active — the rule was skipped entirely.
    RuleNamesInactiveMod {
        /// The rule's origin.
        rule_origin: RuleOrigin,
        /// The inactive mod named by the rule.
        mod_id: ModId,
    },
    /// A `Rule { origin: UserDecision }` edge was dropped while breaking a
    /// cycle. `ledger::findings::extract` only ever surfaces a dropped
    /// *engine* edge as `EdgeDropped` (see that function's own doc
    /// comment), so without this warning a user's own decision losing a
    /// cycle would vanish with nothing but the why-panel knowing. The
    /// matching ledger finding is `FindingKey::RuleOverruled`.
    UserDecisionOverruled {
        /// The user's own decision that had to be dropped.
        edge: OrderingEdge,
        /// The edge that overruled it, when the witness cycle was a
        /// direct two-mod contradiction — `None` for a longer cycle,
        /// same as [`DroppedEdge::winner`].
        winner: Option<OrderingEdge>,
    },
}

/// The full result of one [`sort`] run.
///
/// `PartialEq` is implemented by hand (below): `rim_analyzer`'s
/// [`LoadOrder`] carries a `HashMap` position index and doesn't derive
/// `PartialEq` itself, so equality here compares `order.as_slice()`
/// instead.
#[derive(Debug, Clone)]
pub struct SortOutcome {
    /// The emitted load order.
    pub order: LoadOrder,
    /// Why every mod landed where it did.
    pub placements: BTreeMap<ModId, PlacementExplanation>,
    /// Every edge dropped to keep the graph acyclic.
    pub dropped: Vec<DroppedEdge>,
    /// Every any-of constraint's resolution.
    pub any_of_choices: Vec<AnyOfChoice>,
    /// Non-fatal problems noticed while sorting.
    pub warnings: Vec<SortWarning>,
    /// How far the suggested order strays from the current one.
    pub stats: DisturbanceStats,
}

impl PartialEq for SortOutcome {
    fn eq(&self, other: &Self) -> bool {
        self.order.as_slice() == other.order.as_slice()
            && self.placements == other.placements
            && self.dropped == other.dropped
            && self.any_of_choices == other.any_of_choices
            && self.warnings == other.warnings
            && self.stats == other.stats
    }
}

/// Builds a load order from `input`.
///
/// Total: malformed or inactive-mod rules become [`SortWarning`]s, never
/// errors. Deterministic: every internal collection is ordered, node
/// indices are assigned in sorted [`ModId`] order, and no hashing
/// influences the result.
#[must_use]
pub fn sort(input: &SortInput<'_>) -> SortOutcome {
    graph::run(input)
}

/// Computes [`DisturbanceStats`] for any two orders directly — the same
/// function [`sort`] itself uses for [`SortOutcome::stats`], exposed for
/// callers that want to measure disturbance over a sub-sequence (e.g. "how
/// much of this reordering traces to mods pulled forward by a dependent").
#[must_use]
pub fn compute_disturbance(order: &LoadOrder, current: &LoadOrder) -> DisturbanceStats {
    disturbance::compute(order, current)
}
