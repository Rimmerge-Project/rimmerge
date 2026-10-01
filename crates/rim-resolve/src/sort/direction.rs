//! Which side of one violated, [`propagates`]-eligible pair actually
//! moves under `PreserveCurrent` emission — pulling the prerequisite
//! forward, or pushing the dependent back.
//!
//! **The cost.** For a violated pair `(p, d)` — `p` must load before `d`,
//! but `p` currently sits after it — a pull moves every mod in `{p} ∪
//! Anc(p)` that currently sits after `d`, each by however far it has to
//! travel to reach `d`; a push moves every mod in `{d} ∪ Desc(d)` that
//! currently sits before `p`, each by however far it has to travel to
//! reach `p`. Summing those per-mod distances, rather than merely
//! counting how many mods move, is what tells "one mod moving 700 slots"
//! apart from "a dozen mods each moving 5" — the former is far cheaper.
//! Only mods on the *wrong* side of the target count: an ancestor of `p`
//! already before `d` doesn't move when `p` is pulled, and a descendant
//! of `d` already after `p` doesn't move when `d` is pushed — counting
//! either would bias against pulling any mod with framework dependencies,
//! or pushing any mod whose dependents already sit late. Whichever total
//! is strictly smaller wins; an exact tie falls to [`pull_wins_ties`]
//! over every layer any accepted edge between `p` and `d` carries, so a
//! pair joined by both a weaker and a stronger layer always resolves the
//! way the stronger one would alone.
//!
//! **Why ranks, not raw base positions.** Under `PreserveCurrent`, every
//! mod missing from the current order shares the same base position
//! (`active.len()`), so base positions alone aren't a strict order.
//! [`ranks_of`] breaks that tie by `ModId`, giving every mod a distinct
//! rank the cost comparison can subtract and compare directly.
//!
//! **Correctness is unaffected either way.** [`edge_directions`] only
//! feeds [`super::emit::pull_keys`]/[`super::emit::push_keys`], which
//! only ever influence Kahn's own tie-break key — the Kahn loop in
//! [`super::emit::run`] still enforces every accepted edge strictly
//! through in-degree tracking, regardless of which direction a pair was
//! decided. A wrong or stale cost only ever makes the result disturb the
//! current order more than necessary, never invalid.

use std::collections::BTreeMap;

use fixedbitset::FixedBitSet;
use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rim_analyzer::domain::ModId;

use super::Layer;
use super::emit::propagates;
use super::graph::{GraphEdgeKind, Node, SortGraph};

/// Which side of one violated pair [`edge_directions`] decided should
/// move. See this module's own doc comment for how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmissionDirection {
    /// Pull the prerequisite forward to the dependent's key.
    Pull,
    /// Push the dependent back to the prerequisite's key instead.
    Push,
}

/// Whether an exact pull/push tie resolves to [`EmissionDirection::Pull`]
/// for an edge at `layer` — `true` for every layer carrying an author's
/// or the user's own order statement (the same set [`propagates`]
/// admits, minus `Inferred`), so the overwhelmingly common shape (neither
/// side has any further chain, both costs zero) keeps behaving exactly as
/// a pull-only rule always has for those layers. `false` for `Inferred`:
/// a tie there pushes the dependent instead, so a bare heuristic edge
/// whose two options genuinely cost the same never gets to drag a
/// prerequisite forward on a coin flip. `Soft`/`Awareness` are
/// unreachable here in practice (neither ever [`propagates`]); `false`
/// only for symmetry with `Inferred`.
fn pull_wins_ties(layer: Layer) -> bool {
    match layer {
        Layer::Hard
        | Layer::AnyOf
        | Layer::DeclaredOverride
        | Layer::Declared
        | Layer::UserDecision
        | Layer::RimSortUser
        | Layer::RimSortCommunity
        | Layer::SteamDb => true,
        Layer::Inferred | Layer::Soft | Layer::Awareness => false,
    }
}

/// Every mod node's rank when mods are ordered by `(base_positions[id],
/// id)` — unlike `base_positions` alone, this is a strict total order
/// (every mod `PreserveCurrent` finds missing from `current` shares the
/// same base position, `active.len()`), which is what lets [`decide`]
/// subtract and compare ranks directly. Tier sentinels get no entry: a
/// [`GraphEdgeKind::Real`] edge never touches one.
fn ranks_of(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
) -> BTreeMap<NodeIndex, usize> {
    let mut ordered: Vec<(NodeIndex, &ModId)> = graph
        .node_indices()
        .filter_map(|idx| match &graph[idx] {
            Node::Mod(id) => Some((idx, id)),
            Node::TierStart(_) | Node::TierEnd(_) => None,
        })
        .collect();
    ordered.sort_by_key(|(_, id)| {
        (
            base_positions.get(*id).copied().unwrap_or(usize::MAX),
            (*id).clone(),
        )
    });
    ordered
        .into_iter()
        .enumerate()
        .map(|(rank, (idx, _))| (idx, rank))
        .collect()
}

/// Every mod node's ancestor (`Direction::Incoming`) or descendant
/// (`Direction::Outgoing`) closure within the [`propagates`]-eligible
/// subgraph, as a [`FixedBitSet`] indexed by *rank* rather than
/// [`NodeIndex`] — [`displacement_above`]/[`displacement_below`] need a
/// contiguous, orderable range to sum over, which a rank gives and a raw
/// node index doesn't. `topo_order` must already be the order this
/// closure walks *against*: forward topological order (sources first)
/// for `Direction::Incoming`'s ancestor walk, reversed (sinks first) for
/// `Direction::Outgoing`'s descendant walk — either way, a node's own
/// closure is only ever asked for once every neighbour it unions in has
/// already computed its own. A node's closure is the union of its
/// immediate propagating neighbours' own ranks and *their* already-known
/// closures, not a plain sum: a diamond shape (two paths to the same
/// ancestor) would otherwise double-count. Mirrors what
/// `sort::emit`'s retired `propagating_closure_sizes` computed, but keeps
/// the member set itself rather than just its size.
fn closures(
    graph: &SortGraph,
    topo_order: &[NodeIndex],
    direction: Direction,
    ranks: &BTreeMap<NodeIndex, usize>,
) -> BTreeMap<NodeIndex, FixedBitSet> {
    let capacity = ranks.len();
    let mut closures: BTreeMap<NodeIndex, FixedBitSet> = BTreeMap::new();
    for &node in topo_order {
        if !ranks.contains_key(&node) {
            continue; // a tier sentinel: never a propagating edge's endpoint.
        }
        let mut closure = FixedBitSet::with_capacity(capacity);
        for edge in graph.edges_directed(node, direction) {
            let GraphEdgeKind::Real(_) = &edge.weight().kind else {
                continue;
            };
            if !propagates(edge.weight().layer) {
                continue;
            }
            let neighbour = match direction {
                Direction::Incoming => edge.source(),
                Direction::Outgoing => edge.target(),
            };
            let Some(&neighbour_rank) = ranks.get(&neighbour) else {
                continue;
            };
            closure.insert(neighbour_rank);
            if let Some(neighbour_closure) = closures.get(&neighbour) {
                closure.union_with(neighbour_closure);
            }
        }
        closures.insert(node, closure);
    }
    closures
}

/// The total displacement every set bit strictly *past* `threshold`
/// contributes to a pull's cost: each set bit is itself a rank, and
/// `rank - threshold` is how far that mod would have to move to reach
/// `threshold`. A bit at or before `threshold` already sits on the
/// correct side and contributes nothing.
fn displacement_above(closure: &FixedBitSet, threshold: usize) -> u64 {
    closure
        .ones()
        .filter(|&rank| rank > threshold)
        .map(|rank| (rank - threshold) as u64)
        .fold(0, u64::saturating_add)
}

/// [`displacement_above`]'s mirror for a push's cost: every set bit
/// strictly *before* `threshold` contributes `threshold - rank`.
fn displacement_below(closure: &FixedBitSet, threshold: usize) -> u64 {
    closure
        .ones()
        .filter(|&rank| rank < threshold)
        .map(|rank| (threshold - rank) as u64)
        .fold(0, u64::saturating_add)
}

/// Decides one violated pair `(p, d)`: `p` must load before `d` per every
/// edge in `layers`. `p`/`d` and their own closures always contribute
/// `rank(p) - rank(d)` to whichever side is scored — see this module's
/// own doc comment for the full derivation — on top of whatever else in
/// `{p} ∪ Anc(p)`/`{d} ∪ Desc(d)` sits on the wrong side of the target.
fn decide(
    p: NodeIndex,
    d: NodeIndex,
    ranks: &BTreeMap<NodeIndex, usize>,
    ancestors: &BTreeMap<NodeIndex, FixedBitSet>,
    descendants: &BTreeMap<NodeIndex, FixedBitSet>,
    layers: &[Layer],
) -> EmissionDirection {
    let (Some(&rank_p), Some(&rank_d)) = (ranks.get(&p), ranks.get(&d)) else {
        // Every `Real` edge connects two mod nodes by construction
        // (`sort::graph::add_real_edges` only ever resolves
        // `indices.mod_index`), so both endpoints always have a rank —
        // unreachable, not a real input to guard against. `Push` matches
        // this module's own tie default for every non-author layer, the
        // more conservative of the two.
        return EmissionDirection::Push;
    };
    if rank_p < rank_d {
        // Already satisfied in base order: chain continuation, a no-op
        // unless something else pulls `d` in turn — that pull's own cost
        // already counts `p` through `Anc(d)`.
        return EmissionDirection::Pull;
    }
    let empty = FixedBitSet::with_capacity(0);
    let ancestors_of_p = ancestors.get(&p).unwrap_or(&empty);
    let descendants_of_d = descendants.get(&d).unwrap_or(&empty);
    let base = (rank_p - rank_d) as u64;
    let pull_cost = base.saturating_add(displacement_above(ancestors_of_p, rank_d));
    let push_cost = base.saturating_add(displacement_below(descendants_of_d, rank_p));
    match pull_cost.cmp(&push_cost) {
        std::cmp::Ordering::Less => EmissionDirection::Pull,
        std::cmp::Ordering::Greater => EmissionDirection::Push,
        std::cmp::Ordering::Equal if layers.iter().copied().any(pull_wins_ties) => {
            EmissionDirection::Pull
        }
        std::cmp::Ordering::Equal => EmissionDirection::Push,
    }
}

/// Every violated, [`propagates`]-eligible pair's direction, decided once
/// up front for the whole graph — [`super::emit::pull_keys`]/
/// [`super::emit::push_keys`] both read this same map rather than each
/// recomputing its own per edge. Keyed by the pair (`(before, after)`
/// node indices), not by [`petgraph::stable_graph::EdgeIndex`]: two
/// parallel edges between the same two mods (say a `Declared` `LoadAfter`
/// and an `Inferred` `ReplaceDiscardsAddition`) must resolve in one
/// direction, never opposite ones, and keying by the pair is what
/// guarantees that.
pub(super) fn edge_directions(
    graph: &SortGraph,
    base_positions: &BTreeMap<ModId, usize>,
) -> BTreeMap<(NodeIndex, NodeIndex), EmissionDirection> {
    // The graph is acyclic by the time this runs (every layer's cycles
    // were already broken before `emit::run` is ever called) — the same
    // invariant `propagate_keys`'s own `toposort` call relies on. An
    // empty map here just means every pair falls back to
    // `pull_keys`/`push_keys`'s own `EmissionDirection::Pull` default,
    // never a panic.
    let Ok(topo_order) = toposort(graph, None) else {
        return BTreeMap::new();
    };

    let ranks = ranks_of(graph, base_positions);
    let ancestors = closures(graph, &topo_order, Direction::Incoming, &ranks);
    let mut reverse_topo_order = topo_order;
    reverse_topo_order.reverse();
    let descendants = closures(graph, &reverse_topo_order, Direction::Outgoing, &ranks);

    let mut pairs: BTreeMap<(NodeIndex, NodeIndex), Vec<Layer>> = BTreeMap::new();
    for node in graph.node_indices() {
        for edge in graph.edges_directed(node, Direction::Outgoing) {
            let GraphEdgeKind::Real(_) = &edge.weight().kind else {
                continue;
            };
            let layer = edge.weight().layer;
            if !propagates(layer) {
                continue;
            }
            pairs
                .entry((edge.source(), edge.target()))
                .or_default()
                .push(layer);
        }
    }

    pairs
        .into_iter()
        .map(|((p, d), layers)| {
            let direction = decide(p, d, &ranks, &ancestors, &descendants, &layers);
            ((p, d), direction)
        })
        .collect()
}
