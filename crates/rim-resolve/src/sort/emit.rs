//! Topological emission:
//! Kahn's algorithm over the final, acyclic graph, breaking ties by
//! *propagated* minimal-disturbance keys rather than raw current
//! positions — with tier sentinels always outranking the mods of their own
//! tier.
//!
//! **Why propagate.** A raw-position tie-break moves the *dependent* mod
//! to wherever its prerequisite ends up, even when the prerequisite is the
//! one out of place: a mod the user keeps early that turns out to need a
//! prerequisite sitting hundreds of positions later gets pushed down past
//! everything between the two, disturbing every mod in between's relative
//! order along the way. Pulling the prerequisite forward instead — to just
//! before its earliest dependent — moves one mod instead of dozens. For
//! every mod `m`, [`propagate_keys`] computes `key(m) = min(base(m), min
//! over m's real dependents d of key(d))` in reverse topological order (a
//! dependent's key is only smaller than its own base position when
//! *something else, even later,* pulled at it in turn — the recursion
//! chains through as many hops as the real dependency graph has). Sentinel
//! nodes and non-`Real` edges never participate: tier structure is not "a
//! dependent asking to be pulled forward", and letting it propagate would
//! let one early mod collapse an entire tier's worth of unrelated later
//! mods toward it.
//!
//! **Which `m`'s *dependents* propagation actually follows.** Not every
//! `Real` edge — only one [`propagates`] admits: an edge carrying an
//! author's or the user's own order statement
//! (`Hard`/`AnyOf`/`Declared`/`UserDecision`/any imported rule layer), or
//! `Inferred` (a heuristic the analyzer concluded itself, still evidence
//! worth acting on). `Soft`/`Awareness` are real, can still be enforced,
//! and still hold a dependent back in the Kahn loop below exactly like
//! any other accepted edge — they just never pull *or* push, since
//! neither direction's cost was ever measured for a signal with no
//! author or user behind it at all. See [`propagates`]'s own doc comment
//! for the full rationale, and `sort::direction` for how a violated,
//! propagating edge decides *which* side actually moves.
//!
//! There is no cluster scheduling mode. Every mod's
//! base key is simply its `base_positions` entry (see [`run`]): the
//! caller (`sort/graph.rs`) precomputes this once per [`super::TieBreak`]
//! mode — the mod's own current position in `PreserveCurrent`, or its rank
//! in display-name order in `Rebuild` — so this module stays
//! tie-break-agnostic.
//!
//! **Placement extremes.** [`placement_bias`] alone only orders nodes the Kahn
//! ready-set heap sees in the *same round* — see [`HeapKey`]'s own doc
//! comment for why that is not the same thing as "an explicit `Top`/
//! `Bottom` pin always reaches its tier's extreme edge". [`run`] calls
//! [`apply_placement_extremes`] once its own Kahn loop finishes, which is
//! what actually makes that guarantee hold, subject to accepted edges:
//! it moves each pin's own *closure* — the pin plus everything in its
//! region transitively forced to follow (`Bottom`) or precede (`Top`) it
//! — to the region's extreme edge as one block, largest closure first.
//! See [`extremize_region`] for why a block move rather than a per-slot
//! peel, which would give that guarantee to *leaf* pins alone.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rim_analyzer::domain::ModId;

use super::direction::EmissionDirection;
use super::explain::TierReason;
use super::graph::{GraphEdgeKind, Indices, Node, SortGraph};
use super::tiers::Tier;
use super::{Layer, OrderingEdge};

/// The final scheduling key [`run`] used for one mod, and — when a real
/// dependent is what set it below the mod's own base position — the edge
/// naming that dependent.
#[derive(Debug, Clone)]
pub(super) struct EffectiveKey {
    pub key: usize,
    pub pulled_forward_by: Option<OrderingEdge>,
}

/// The emitted mod order, plus everything [`super::explain`] needs to
/// describe how each mod got there.
pub(super) struct Emission {
    pub order: Vec<ModId>,
    /// Every mod's final scheduling key, for [`super::PlacementTieBreak`].
    pub effective_keys: BTreeMap<ModId, EffectiveKey>,
}

/// The total order [`run`]'s ready-set is kept sorted by: sentinels before
/// mods, then [`placement_bias`], then ascending effective key (mods
/// absent from the current
/// order sort after every present one, before propagation can pull them
/// forward), then a node *pulled* to this key by a real dependent ahead of
/// one that merely started here on its own, then the node itself — so two
/// mods both absent from the current order fall back to id order — as the
/// final deterministic tie-break.
///
/// Preferring a pulled node at a tie keeps two same-keyed mods
/// deterministic without depending on iteration order: a node explicitly
/// pulled here by propagation wins a tie against one that merely happens
/// to share the key on its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct HeapKey {
    is_mod: u8,
    placement_bias: i8,
    effective_key: usize,
    was_pulled: std::cmp::Reverse<bool>,
    node: Node,
}

/// Whether `node` is an *explicitly* [`crate::domain::PlacementRule`]-pinned
/// mod, and which direction its region's extreme edge lies in — `-1` (sorts
/// first among its region's ready mods) for a `Top` pin, `+1` (sorts last)
/// for a `Bottom` pin, `0` for everything else, *including* a mod merely
/// dependency-promoted alongside a pin (`TierReason::PromotedBy` never
/// changes a mod's own entry in `tier_of`, which still names its nominal,
/// pre-promotion tier — see `sort::tiers::assign`).
///
/// Placed in [`HeapKey`] ahead of `effective_key` so it partitions
/// *within one Kahn round*: whenever a `0`-bias mod and an explicit pin
/// become ready at the same time, the `0`-bias mod wins. **This alone
/// does not make a pin sort to its region's extreme edge overall** — a
/// pin ready in an *earlier* round than some other mod's own prerequisite
/// chain is still emitted immediately, since nothing here can look ahead
/// to a mod that isn't ready yet. The
/// actual tier-edge guarantee comes from [`extremize_region`], a second
/// pass run after this module's own Kahn loop finishes — which is also
/// where two pins of the same placement are finally ordered against each
/// other (by closure size, then by the base order this heap emitted them
/// in; see [`pin_closures_in_processing_order`]). Within the heap itself
/// they still tie-break on `effective_key`: this narrows that tie-break,
/// it never replaces it.
fn placement_bias(node: &Node, tier_of: &BTreeMap<ModId, (Tier, TierReason)>) -> i8 {
    let Node::Mod(id) = node else {
        return 0;
    };
    bias_of(id, tier_of)
}

/// [`placement_bias`]'s own per-mod logic, factored out so
/// [`extremize_region`] (which only ever deals in [`ModId`]s, never raw
/// graph [`Node`]s) can share it without allocating a throwaway
/// [`Node::Mod`] just to ask.
fn bias_of(id: &ModId, tier_of: &BTreeMap<ModId, (Tier, TierReason)>) -> i8 {
    match tier_of.get(id) {
        Some((Tier::Top, TierReason::Placement(_))) => -1,
        Some((Tier::Bottom, TierReason::Placement(_))) => 1,
        _ => 0,
    }
}

/// Which edge of its own region [`extremize_region`] is pushing pins
/// toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Extreme {
    /// The very front of the whole order (`Top`'s own leading edge).
    Head,
    /// The very end of the whole order (`Bottom`'s own trailing edge).
    Tail,
}

/// Reorders one `Top`/`Bottom` region
/// so every explicit pin of that placement sits as close to the region's
/// own extreme edge as its accepted edges permit, by moving each pin's
/// **closure** — the pin plus every region member transitively reachable
/// from it through `Real` edges in the extremized direction — to that
/// edge as one block.
///
/// `region` is a contiguous slice of [`run`]'s already-valid base order;
/// see [`apply_placement_extremes`] for exactly how each side computes
/// its own slice, and why the two sides are bounded *asymmetrically*.
///
/// **Why a block move, and not a back-to-front peel.** A peel fills the
/// extreme slot first and works inward, preferring an explicit pin
/// whenever more than one candidate is eligible — but a pin with
/// region-internal successors is eligible only once every one of them is
/// already assigned, and those successors are ordinary `0`-bias nodes,
/// picked only after every non-pin that merely sits later in the base
/// order. So a peel's guarantee holds for *leaf* pins alone: on a real
/// install, a `Bottom` pin with a small closure can land at the very
/// *first* slot of the `Bottom` region with over a hundred mods after it.
/// The block move is not a proven optimum on real data:
/// [`pin_closures_in_processing_order`]'s exchange argument establishes
/// minimality only for *pairwise disjoint* closures, and real closures can
/// overlap (a mod in two closures), leaving no order-independent "block
/// size" to exchange.
///
/// **Why the block move is correct.** Read "edge" throughout as
/// "accepted `Real` edge" — those are the only ones [`closure_of`] walks,
/// and the known limit that creates is stated below. A pin's closure is
/// successor-closed (`Tail`) or predecessor-closed (`Head`) *among region
/// members* by construction, which splits the argument in two:
///
/// - **Both endpoints inside the region.** For any edge `a -> b`, either
///   `b` is in the moved block and `a` is not (satisfied by the move), or
///   `a` is in it — which forces `b` into it too, closure being closed
///   under successors, and the block's own internal order is untouched —
///   or neither is, and their relative order never changed.
/// - **One endpoint outside.** `region` is a *contiguous slice* of
///   [`run`]'s base order, which already satisfies every accepted edge,
///   and this pass only ever permutes *within* that slice. So an
///   out-of-region endpoint keeps whichever side of the whole slice it
///   started on, and the edge stays satisfied however the inside endpoint
///   moves. That is also why [`closure_of`] may stop at the region
///   boundary rather than follow such an edge: a `Tail`-side successor
///   outside the region can only sit *after* the slice — one before it
///   would already have been a violated edge in the base order, so no
///   such edge exists — and nothing this pass does can overtake it.
///   `Head` is the mirror.
///
/// **Known limit, disclosed rather than guarded.** An ordering implied
/// *purely* through tier sentinels — `m -> TierEnd(t) -> TierStart(t')
/// -> m'`, with no direct `Real` edge between `m` and `m'` — is not
/// honoured, because [`closure_of`] filters sentinel edges out and this
/// pass never consults them. The region bounds ([`top_region_start`];
/// `Bottom` running to the true end) are what keep tiers apart instead,
/// and they are coarser than a per-edge check. A case is constructible on
/// the `Head` side: a `Body` mod promoted into some larger pin's closure
/// can be carried ahead of a `Top` pin whose own membership sentinel
/// survived cycle-breaking, and `sort_golden.rs`'s `assert_invariants`
/// would not notice, since its tier-monotonicity check deliberately
/// excludes `TierReason::PromotedBy` mods. No known install or fixture
/// produces one, so this stays a known limit rather than a guarded case.
///
/// No edge is added, dropped, or reinterpreted, so `ledger::findings`'
/// `PlacementOrderingOverridden`/`PlacementPromotesDependents` — computed
/// from `PlacementExplanation::upper_bounds`/`lower_bounds`, never from
/// final positions — are unaffected.
///
/// **The resulting invariant**, which the `sort_core.rs`/`sort_golden.rs`
/// tests assert directly: within a region, every mod positioned beyond
/// (`Tail`: after; `Head`: before) any explicit pin of that placement is
/// itself an explicit pin of that placement or lies in some such pin's
/// closure. It holds by construction — every pin is in its own closure,
/// so every pin ends up inside some moved block, and the untouched
/// remainder is always further from the extreme than every block.
///
/// A region member absent from the graph (defensive only — every id in
/// `region` came from [`run`]'s own emitted order, always present in
/// `indices`) contributes no edges, so it is *either* an explicit pin,
/// which still gets a singleton closure and is moved like any other, *or*
/// in no closure at all and left in the untouched remainder. Either way
/// [`move_closure_to_extreme`]'s partition keeps it, so the result is
/// still a total permutation of `region`.
fn extremize_region(
    region: &[ModId],
    graph: &SortGraph,
    indices: &Indices,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    extreme: Extreme,
) -> Vec<ModId> {
    let members: BTreeSet<ModId> = region.iter().cloned().collect();
    let mut order = region.to_vec();
    for closure in
        pin_closures_in_processing_order(region, graph, indices, tier_of, &members, extreme)
    {
        order = move_closure_to_extreme(&order, &closure, extreme);
    }
    order
}

/// Which way [`extremize_region`] walks `Real` edges to build a pin's own
/// closure: a `Tail` pin claims its successors (they must follow it, so
/// they are exactly what it has to drag along to the end), a `Head` pin
/// its predecessors.
fn closure_direction(extreme: Extreme) -> Direction {
    match extreme {
        Extreme::Head => Direction::Incoming,
        Extreme::Tail => Direction::Outgoing,
    }
}

/// One pin's closure: itself plus every `members` mod transitively
/// reachable from it through `Real` edges in [`closure_direction`].
///
/// Two exclusions, for two different reasons. Sentinel
/// (`Membership`/`Boundary`) edges are skipped *by design* — tier
/// structure is not a relation a pin has to drag along — at the cost
/// [`extremize_region`]'s own "known limit" paragraph states. An edge
/// whose other endpoint lies outside `region` is skipped because it is
/// already satisfied and cannot stop being: `region` is a contiguous
/// slice of an already-valid order and this pass permutes only within it,
/// so such an endpoint keeps whichever side of the whole slice it started
/// on.
fn closure_of(
    pin: &ModId,
    graph: &SortGraph,
    indices: &Indices,
    members: &BTreeSet<ModId>,
    extreme: Extreme,
) -> BTreeSet<ModId> {
    let direction = closure_direction(extreme);
    let mut closure: BTreeSet<ModId> = BTreeSet::new();
    closure.insert(pin.clone());
    let mut frontier: Vec<ModId> = vec![pin.clone()];
    while let Some(current) = frontier.pop() {
        let Some(&node_idx) = indices.mod_index.get(&current) else {
            continue;
        };
        for edge in graph.edges_directed(node_idx, direction) {
            let GraphEdgeKind::Real(ordering_edge) = &edge.weight().kind else {
                continue;
            };
            let other = match direction {
                Direction::Outgoing => &ordering_edge.after,
                Direction::Incoming => &ordering_edge.before,
            };
            if members.contains(other) && closure.insert(other.clone()) {
                frontier.push(other.clone());
            }
        }
    }
    closure
}

/// Every explicit pin's closure, in the order [`extremize_region`] moves
/// them — **largest closure first**, ties broken by the pin's own
/// position in `region` (i.e. in [`run`]'s base order): ascending for
/// `Tail`, descending for `Head`. Since each move puts its block at the
/// extreme, a pin processed *later* ends up *closer* to it.
///
/// **Why largest first.** Every mod sitting past a pin is a mod that pin
/// did not get as close to the edge as it asked for, so the quantity to
/// minimise is the total number of (pin, mod beyond that pin) pairs.
/// **For pairwise disjoint closures this is exactly an exchange
/// argument**: two adjacent blocks of sizes `s_i` and `s_j` contribute
/// `s_j` such pairs with the `s_i` block nearer the extreme and `s_i` the
/// other way round, so no adjacent swap can improve a descending-size
/// order, and largest-first is optimal.
///
/// **Overlapping closures fall outside that proof, and real installs
/// have them**: a mod in two closures is carried by both moves and ends up
/// counted wherever the *later* one puts it, so "block size" is not even
/// well defined independently of the processing order being chosen.
/// Largest-first is kept there because it is optimal in the disjoint case
/// and measurably good in the overlapping one (it roughly halves the
/// pairs a per-slot peel leaves on a real install), not because it is
/// proven optimal.
///
/// The tie rule then keeps two same-sized pins in `run`'s own base order
/// relative to each other: for `Tail` the pin earlier in `region` is
/// moved first and so ends up earlier; for `Head` the later one is moved
/// first and so ends up later. `base_rank` below is the only thing that
/// decides — pinned by `sort_core.rs`'s
/// `two_equal_closure_bottom_pins_keep_their_base_order` and its `Head`
/// mirror, which are the only tests in the suite that reach it at all.
///
/// Two pins of the same placement tie-break on `effective_key` only
/// within one closure size: `effective_key` order *is* `region` order
/// among the pins (it is what [`run`]'s heap emitted them by), so that
/// rule is the *tie* rule — closure size outranks it, because a pin's
/// closure size is the thing that decides how far from the edge it can
/// possibly get.
fn pin_closures_in_processing_order(
    region: &[ModId],
    graph: &SortGraph,
    indices: &Indices,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    members: &BTreeSet<ModId>,
    extreme: Extreme,
) -> Vec<BTreeSet<ModId>> {
    let target_bias = match extreme {
        Extreme::Head => -1,
        Extreme::Tail => 1,
    };
    let mut pins: Vec<(usize, BTreeSet<ModId>)> = region
        .iter()
        .enumerate()
        .filter(|(_, id)| bias_of(id, tier_of) == target_bias)
        .map(|(index, id)| (index, closure_of(id, graph, indices, members, extreme)))
        .collect();
    pins.sort_by_key(|(index, closure)| {
        let base_rank = match extreme {
            Extreme::Tail => *index,
            // `index` is always a valid index into `region`, so this
            // never underflows.
            Extreme::Head => region.len().saturating_sub(1) - *index,
        };
        (std::cmp::Reverse(closure.len()), base_rank)
    });
    pins.into_iter().map(|(_, closure)| closure).collect()
}

/// Stable-partitions `order` into `closure`'s own members and everything
/// else, then reassembles it with the closure block at `extreme`. Both
/// parts keep their relative order, which is what the correctness
/// argument in [`extremize_region`] rests on.
fn move_closure_to_extreme(
    order: &[ModId],
    closure: &BTreeSet<ModId>,
    extreme: Extreme,
) -> Vec<ModId> {
    let (block, rest): (Vec<ModId>, Vec<ModId>) =
        order.iter().cloned().partition(|id| closure.contains(id));
    match extreme {
        Extreme::Tail => rest.into_iter().chain(block).collect(),
        Extreme::Head => block.into_iter().chain(rest).collect(),
    }
}

/// The first index in `order` a `Top` region may safely start at: one
/// past the last `Core`/`Dlc`-tagged mod's own position, or `0` when
/// neither tier has a member at all (a fixture with no `Core`/`Dlc` —
/// nothing precedes `Top` structurally in that case).
///
/// **This is deliberately not "the first `Top`-tagged mod's own
/// position"** (the symmetric counterpart of
/// [`apply_placement_extremes`]'s own `Bottom` side): with a single `Top`
/// pin, as on a real install, "first `Top`-tagged" and "last
/// `Top`-tagged" name the exact same position — a single-element span
/// that excludes the pin's own attributed dependents entirely
/// (nominally `Body`-tagged interlopers a
/// real edge forces to precede it; `TierReason::PromotedBy` never rewrites
/// their own tier). Bounding by `Core`/`Dlc`'s own last position instead
/// has no such degenerate case: `Core`/`Dlc` bind at `Layer::Hard`
/// (`sort::tiers::membership_layer`), so nothing legitimately needing to
/// reach the `Top` region can be excluded by starting right after them,
/// regardless of how many `Top` pins exist or how deep their own
/// dependent chains run.
fn top_region_start(order: &[ModId], tier_of: &BTreeMap<ModId, (Tier, TierReason)>) -> usize {
    order
        .iter()
        .rposition(|id| matches!(tier_of.get(id), Some((Tier::Core | Tier::Dlc, _))))
        .map_or(0, |position| position + 1)
}

/// Runs [`extremize_region`] once per side that actually has an explicit
/// pin — a report with no `Top`/`Bottom` placements at all (the common
/// case for most fixtures) leaves `order` completely untouched, doing no
/// work.
///
/// **`Top` and `Bottom` are bounded asymmetrically, deliberately** — see
/// [`top_region_start`]'s own doc comment for the real-install case that
/// needs this shape. `Top`'s region runs from [`top_region_start`] to
/// the last `Top`-tagged mod's own position: nothing can be promoted
/// *past* that last member either (a mod forced to precede some `Top` pin
/// necessarily lands at or before that pin's own position, which is at
/// most the last `Top`-tagged member's), so the right side needs no
/// analogous widening. `Bottom`'s region runs from the first
/// `Bottom`-tagged mod's own position to the *true end* of `order` — not
/// to the last `Bottom`-tagged position, the mirror image of the same
/// mistake: `Bottom` is the terminal tier, so there is no adjacent tier's
/// worth of immovable mods to sweep in from the right, and a mod forced
/// to *follow* a `Bottom` pin can legitimately land past the last
/// `Bottom`-tagged member. Reproduced directly: `sort_core.rs`'s
/// `an_unrelated_bottom_pin_ready_in_an_earlier_round_still_ends_up_after_a_blocked_pins_dependent`
/// (a `Body` mod forced to follow a `Bottom` pin, landing after every
/// `Bottom`-tagged member) fails if `Bottom`'s own right bound is
/// tightened the same way.
fn apply_placement_extremes(
    mut order: Vec<ModId>,
    graph: &SortGraph,
    indices: &Indices,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
) -> Vec<ModId> {
    if let Some(start) = order.iter().position(|id| bias_of(id, tier_of) == 1) {
        let region = order[start..].to_vec();
        let reordered = extremize_region(&region, graph, indices, tier_of, Extreme::Tail);
        order[start..].clone_from_slice(&reordered);
    }
    if let Some(end) = order.iter().rposition(|id| bias_of(id, tier_of) == -1) {
        let start = top_region_start(&order, tier_of);
        // `start` can only exceed `end` if a `Core`/`Dlc` mod was itself
        // promoted past every `Top` pin — a pathological, two-Hard-edges
        // scenario nothing in this crate's own test suite produces;
        // skip rather than build an invalid range if that invariant ever
        // breaks elsewhere.
        if start <= end {
            let region = order[start..=end].to_vec();
            let reordered = extremize_region(&region, graph, indices, tier_of, Extreme::Head);
            order[start..=end].clone_from_slice(&reordered);
        }
    }
    order
}

/// This node's *base* key before propagation: its entry in
/// `base_positions` (every active mod has one — see [`run`]'s own doc
/// comment), else [`usize::MAX`] for a tier sentinel (never a real
/// scheduling target).
fn base_key(node: &Node, base_positions: &BTreeMap<ModId, usize>) -> usize {
    match node {
        Node::Mod(id) => base_positions.get(id).copied().unwrap_or(usize::MAX),
        Node::TierStart(_) | Node::TierEnd(_) => usize::MAX,
    }
}

/// Every node's key with no propagation at all: each node keeps its own
/// `base_key` unchanged, never pulled forward by a dependent. This is
/// [`run`]'s direct choice
/// for [`super::TieBreak::Rebuild`], not just [`propagate_keys`]'s own
/// defensive fallback below — `propagate_keys`'s minimal-disturbance
/// pulling answers "how do I disturb the *current* order as little as
/// possible", a question `Rebuild` never asks (there is no current order
/// to disturb; the point is a list built from scratch, ranked by name).
/// Feeding these keys straight to [`run`]'s own Kahn loop yields "plain
/// Kahn with a name-ranked heap": the lexicographically smallest
/// topological order, where a prerequisite is placed the moment its own
/// name comes up and a dependent simply waits its turn.
fn non_propagated_keys(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
) -> BTreeMap<NodeIndex, EffectiveKey> {
    graph
        .node_indices()
        .map(|n| {
            (
                n,
                EffectiveKey {
                    key: base_key(&graph[n], base_positions),
                    pulled_forward_by: None,
                },
            )
        })
        .collect()
}

/// `PreserveCurrent`'s own counterpart to `Rebuild` never propagating:
/// whether a `Real` edge at `layer` participates in [`propagate_keys`]'s
/// own pull-or-push walk at all. Propagation follows an author's or the
/// user's own order statement — `Hard`, `AnyOf` (a resolved any-of is
/// still the analyzer reporting a load-time fact), `DeclaredOverride` (a
/// user's own explicit, per-pair statement that this edge outranks a
/// declaration — still an order statement someone wrote down, the same
/// reasoning as `Declared`/`UserDecision` below), `Declared`,
/// `UserDecision`, every imported rule layer
/// (`RimSortUser`/`RimSortCommunity`/`SteamDb`, someone's own curated
/// pairing), and `Inferred` (a heuristic the analyzer concluded itself —
/// still evidence worth acting on, not a mere presence signal the way
/// `Awareness` is). `Soft`/`Awareness` stay excluded: evidence the mods
/// merely interact, with no author or user behind it at all, and
/// RimWorld doesn't actually enforce a lazily-resolved `Soft` reference's
/// own order at load time in the first place.
///
/// This function only decides *whether* a violated edge at `layer` can
/// move either side at all — *which* side actually moves (pull the
/// prerequisite forward, or push the dependent back instead) is
/// `sort::direction::edge_directions`'s own position-aware cost
/// comparison, precomputed once per sort and looked up by
/// [`pull_keys`]/[`push_keys`] below. `Layer::Inferred` being
/// [`propagates`]-eligible only changes *how* a violated `Inferred` edge
/// is satisfied, never *whether* — [`super::EnforcedLayers::inferred`]
/// alone still decides that.
///
/// [`super::TieBreak::Rebuild`] asks no version of this question at all —
/// that mode never propagates ([`non_propagated_keys`]).
pub(super) fn propagates(layer: Layer) -> bool {
    match layer {
        Layer::Hard
        | Layer::AnyOf
        | Layer::DeclaredOverride
        | Layer::Declared
        | Layer::UserDecision
        | Layer::RimSortUser
        | Layer::RimSortCommunity
        | Layer::SteamDb
        | Layer::Inferred => true,
        Layer::Soft | Layer::Awareness => false,
    }
}

/// The prerequisite-pull half of [`propagate_keys`]'s own two-direction
/// fold, gated per edge by `directions` (`sort::direction::edge_directions`,
/// precomputed once for the whole sort): `base_key`, pulled down to the
/// smallest key among its dependents connected by an edge [`propagates`]
/// admits *and* `directions` decided [`EmissionDirection::Pull`],
/// computed in reverse topological order so a dependent's own (possibly
/// already-pulled) key is known before its prerequisite's is computed.
/// Chains through as many pull-decided propagating-edge hops as the graph
/// has, so a dependent three hops down pulling hard still reaches all the
/// way back to the mod at the head of the chain — *unless* a hop along
/// the way was decided [`EmissionDirection::Push`] instead, in which case
/// the chain stops there for this direction (see [`push_keys`] for what
/// continues it the other way).
///
/// A non-[`propagates`] `Real` edge (`Soft`/`Awareness`) is skipped here
/// exactly like a sentinel `Membership`/`Boundary` edge: neither is a
/// dependent's own ask to be scheduled earlier. The edge itself is
/// untouched either way — still a real graph edge, still enforced, still
/// exactly as able to hold this node back in the Kahn loop below; only
/// whether it can *pull* the other node forward changes.
fn pull_keys(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
    topo_order: &[NodeIndex],
    directions: &BTreeMap<(NodeIndex, NodeIndex), EmissionDirection>,
) -> BTreeMap<NodeIndex, EffectiveKey> {
    let mut keys: BTreeMap<NodeIndex, EffectiveKey> = BTreeMap::new();
    for &node in topo_order.iter().rev() {
        let base = base_key(&graph[node], base_positions);
        let mut best: Option<(usize, OrderingEdge)> = None;
        for edge in graph.edges_directed(node, Direction::Outgoing) {
            let GraphEdgeKind::Real(ordering_edge) = &edge.weight().kind else {
                continue;
            };
            if !propagates(edge.weight().layer) {
                continue;
            }
            let direction = directions
                .get(&(edge.source(), edge.target()))
                .copied()
                .unwrap_or(EmissionDirection::Pull);
            if direction != EmissionDirection::Pull {
                continue;
            }
            let Some(successor_key) = keys.get(&edge.target()) else {
                continue;
            };
            // Compare the whole `ordering_edge`, not just
            // `(successor_key.key, &ordering_edge.after)`: that pair ties
            // whenever two *parallel* edges from `node` reach the *same*
            // successor (a `LoadAfter` and a `ModDependency` between the
            // same pair, both accepted) — `ordering_edge.after` is
            // `edge.target()`'s own id either way, so it can't tell them
            // apart, and `best` would keep whichever parallel edge
            // `graph.edges_directed`'s own iteration order (input-
            // edge-insertion-order-dependent) happened to visit first,
            // making `PlacementExplanation.tie_break.pulled_forward_by`
            // (though never the final emitted order, which every
            // successor-tied parallel edge enforces identically) depend
            // on something other than the edges' own content — exactly
            // what `sort_is_invariant_under_input_edge_shuffling` exists
            // to catch. The whole `ordering_edge` (content-ordered:
            // `after`, then `before`, `layer`, `provenance`) gives the same
            // result whenever `after` already differs, and a deterministic
            // one when it doesn't.
            let is_better = best
                .as_ref()
                .is_none_or(|(k, e)| (successor_key.key, ordering_edge) < (*k, e));
            if is_better {
                best = Some((successor_key.key, ordering_edge.clone()));
            }
        }
        let (key, pulled_forward_by) = match best {
            Some((successor_key, edge)) if successor_key < base => (successor_key, Some(edge)),
            _ => (base, None),
        };
        keys.insert(
            node,
            EffectiveKey {
                key,
                pulled_forward_by,
            },
        );
    }

    keys
}

/// The push-the-dependent-back half of [`propagate_keys`]'s own
/// two-direction fold — [`pull_keys`]'s mirror image. For every node,
/// `key(m) = max(base(m),
/// max over m's own real prerequisites `p` connected by an edge
/// [`propagates`] admits *and* `directions` decided
/// [`EmissionDirection::Push`] of `key(p)`)`, computed in **forward**
/// topological order — the opposite of [`pull_keys`] — so a prerequisite
/// three hops up pushing hard still reaches all the way forward to the
/// dependent at the end of the chain. No `pulled_forward_by`-style
/// attribution is tracked here: unlike a pull, which names the one edge
/// responsible via [`EffectiveKey::pulled_forward_by`], a maximum has no
/// single "which predecessor caused this" answer worth threading through
/// [`super::explain`] the same way — a disclosed, deliberate gap, not an
/// oversight: `super::explain`'s why-panel currently has no wording for
/// "this mod moved later to avoid dragging a large prerequisite chain
/// forward", so a pushed mod's own [`PlacementTieBreak`](super::explain::PlacementTieBreak)
/// still reads `pulled_forward_by: None` even though its `effective_key`
/// differs from its base position — see that struct's own doc comment,
/// which covers exactly this shape.
fn push_keys(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
    topo_order: &[NodeIndex],
    directions: &BTreeMap<(NodeIndex, NodeIndex), EmissionDirection>,
) -> BTreeMap<NodeIndex, usize> {
    let mut keys: BTreeMap<NodeIndex, usize> = BTreeMap::new();
    for &node in topo_order {
        let mut key = base_key(&graph[node], base_positions);
        for edge in graph.edges_directed(node, Direction::Incoming) {
            let GraphEdgeKind::Real(_) = &edge.weight().kind else {
                continue;
            };
            if !propagates(edge.weight().layer) {
                continue;
            }
            let prerequisite = edge.source();
            let direction = directions
                .get(&(prerequisite, node))
                .copied()
                .unwrap_or(EmissionDirection::Pull);
            if direction != EmissionDirection::Push {
                continue;
            }
            if let Some(&prerequisite_key) = keys.get(&prerequisite) {
                key = key.max(prerequisite_key);
            }
        }
        keys.insert(node, key);
    }
    keys
}

/// Computes every node's effective key by combining [`pull_keys`] and
/// [`push_keys`] — each
/// violated edge is resolved by exactly one of the two, per
/// `sort::direction::edge_directions`, so at most one of a node's own
/// pull/push deltas from its base key is ever non-zero in practice (both
/// can be
/// non-zero only when a node is simultaneously the pulled prerequisite of
/// one edge and the pushed dependent of another — rare, and still safe:
/// Kahn's algorithm in [`run`] enforces every accepted edge strictly
/// through in-degree tracking regardless of key value, so no combination
/// this function could produce can ever violate the DAG, only affect how
/// well it minimizes disturbance). Used for
/// [`super::TieBreak::PreserveCurrent`] only — see [`non_propagated_keys`]
/// for [`super::TieBreak::Rebuild`]'s own, simpler choice.
fn propagate_keys(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
) -> BTreeMap<NodeIndex, EffectiveKey> {
    // The graph is acyclic by the time `run` is called (every layer's
    // cycles were already broken); a `toposort` failure here would mean
    // that invariant broke elsewhere. Rather than panic on someone else's
    // bug, degrade to no propagation at all (every node keeps its own base
    // key) — still a valid, if less minimal-disturbance, schedule.
    let Ok(topo_order) = toposort(graph, None) else {
        return non_propagated_keys(graph, base_positions);
    };

    let directions = super::direction::edge_directions(graph, base_positions);

    let pulled = pull_keys(graph, base_positions, &topo_order, &directions);
    let pushed = push_keys(graph, base_positions, &topo_order, &directions);

    graph
        .node_indices()
        .map(|node| {
            let base = base_key(&graph[node], base_positions);
            let pull = pulled.get(&node).cloned().unwrap_or(EffectiveKey {
                key: base,
                pulled_forward_by: None,
            });
            let push_key = pushed.get(&node).copied().unwrap_or(base);
            // Combined via deltas from `base`, not a direct sum of the
            // two raw keys — `base` is `usize::MAX` for a tier sentinel
            // (never pulled or pushed, so both deltas are always `0`
            // there), and `pull.key + push_key` would overflow long
            // before this line ever saw it. Whichever direction actually
            // moved this node (usually at most one, per this function's
            // own doc comment) contributes its own delta; an unaffected
            // node's key is unchanged.
            let pull_delta = base.saturating_sub(pull.key);
            let push_delta = push_key.saturating_sub(base);
            let key = base.saturating_sub(pull_delta).saturating_add(push_delta);
            (
                node,
                EffectiveKey {
                    key,
                    pulled_forward_by: pull.pulled_forward_by,
                },
            )
        })
        .collect()
}

/// Runs Kahn's algorithm over `graph`, returning the emitted
/// [`rim_analyzer::domain::LoadOrder`] (mods only) plus every mod's final
/// scheduling key.
///
/// `base_positions` is every active mod's pre-propagation base key,
/// already resolved for whichever [`super::TieBreak`] mode is in effect
/// (`sort/graph.rs::run` builds it) — every mod in the graph has an entry,
/// so [`base_key`] never needs a "mod absent" fallback of its own.
/// `tier_of` is the same per-mod nominal tier assignment `sort/graph.rs::run`
/// already computed via `sort::tiers::assign`, threaded through so
/// [`placement_bias`] can tell an explicit pin from a merely-promoted mod.
/// `tie_break` picks which
/// of [`propagate_keys`]/[`non_propagated_keys`] computes the schedule —
/// see their own doc comments for why `Rebuild` gets the simpler one.
pub(super) fn run(
    graph: &SortGraph,
    indices: &Indices,
    base_positions: &BTreeMap<ModId, usize>,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    tie_break: super::TieBreak,
) -> Emission {
    let effective_keys = match tie_break {
        super::TieBreak::PreserveCurrent => propagate_keys(graph, base_positions),
        super::TieBreak::Rebuild => non_propagated_keys(graph, base_positions),
    };

    let mut in_degree: BTreeMap<NodeIndex, usize> = graph
        .node_indices()
        .map(|n| (n, graph.edges_directed(n, Direction::Incoming).count()))
        .collect();

    let heap_key_of = |node: &Node, idx: NodeIndex| -> HeapKey {
        let (effective_key, was_pulled) =
            effective_keys.get(&idx).map_or((usize::MAX, false), |k| {
                (k.key, k.pulled_forward_by.is_some())
            });
        HeapKey {
            is_mod: match node {
                Node::Mod(_) => 1,
                Node::TierStart(_) | Node::TierEnd(_) => 0,
            },
            placement_bias: placement_bias(node, tier_of),
            effective_key,
            was_pulled: std::cmp::Reverse(was_pulled),
            node: node.clone(),
        }
    };

    let mut ready: BTreeSet<HeapKey> = BTreeSet::new();
    for (&node, &degree) in &in_degree {
        if degree == 0 {
            ready.insert(heap_key_of(&graph[node], node));
        }
    }

    let mut order = Vec::with_capacity(in_degree.len());

    while !ready.is_empty() {
        // `ready` is non-empty per the loop condition, so this always
        // finds something; `continue` (never actually reached) keeps this
        // total without an `unwrap`/`expect`.
        let Some(picked) = ready.iter().next().cloned() else {
            continue;
        };
        ready.remove(&picked);

        let node_idx = indices.node_index_of(&picked.node);
        if let Node::Mod(id) = &picked.node {
            order.push(id.clone());
        }

        let mut newly_ready = Vec::new();
        for successor in graph.neighbors_directed(node_idx, Direction::Outgoing) {
            // Every node's in-degree was seeded up front and the graph
            // never gains or loses nodes during emission, so this always
            // hits; skip rather than panic if that invariant ever breaks.
            let Some(degree) = in_degree.get_mut(&successor) else {
                continue;
            };
            *degree -= 1;
            if *degree == 0 {
                newly_ready.push(successor);
            }
        }
        for successor in newly_ready {
            ready.insert(heap_key_of(&graph[successor], successor));
        }
    }

    let effective_keys_by_id: BTreeMap<ModId, EffectiveKey> = indices
        .mod_index
        .iter()
        .filter_map(|(id, &idx)| effective_keys.get(&idx).map(|k| (id.clone(), k.clone())))
        .collect();

    // The Kahn loop above already produced a valid order,
    // but `placement_bias` alone can't guarantee a pin actually reaches
    // its tier's extreme edge (see `HeapKey`'s own doc comment) — this
    // second pass does. `effective_keys_by_id` is deliberately computed
    // from the *pre*-partition `order` above (it describes the heap's own
    // scheduling input, not this later step).
    let order = apply_placement_extremes(order, graph, indices, tier_of);

    Emission {
        order,
        effective_keys: effective_keys_by_id,
    }
}
