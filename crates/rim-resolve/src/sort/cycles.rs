//! Layer-batched cycle breaking: after one [`super::Layer`]'s edges have
//! all been added, repeatedly find a non-trivial strongly connected
//! component and drop one of *that layer's own* edges from it until the
//! graph is acyclic again.
//!
//! Every earlier layer was left acyclic by construction, so any cycle
//! found now must include at least one edge from the layer just added —
//! dropping one such edge is always enough to make progress.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use petgraph::stable_graph::{EdgeIndex, NodeIndex};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use petgraph::{Direction, algo::tarjan_scc};
use rim_analyzer::domain::{EdgeKind, LoadOrder, Mod, ModId};

use super::explain::TierReason;
use super::graph::{GraphEdge, GraphEdgeKind, Node, SortGraph, hard_dependents_of};
use super::tiers::Tier;
use super::{DroppedEdge, Layer, SortWarning};
use crate::domain::RuleOrigin;

/// What one [`break_cycles`] call produced.
#[must_use]
#[derive(Debug, Default)]
pub(super) struct CycleBreakResult {
    pub dropped: Vec<DroppedEdge>,
    /// A tier membership edge was dropped for this mod: the other
    /// same-layer edge (if any could be identified) that displaced it.
    pub promotions: BTreeMap<ModId, super::OrderingEdge>,
    /// One [`SortWarning::UserDecisionOverruled`]
    /// per dropped `Rule { origin: UserDecision }` edge.
    pub warnings: Vec<SortWarning>,
}

fn is_kept(weight: &GraphEdge, kept: &BTreeSet<(ModId, ModId, EdgeKind)>) -> bool {
    match &weight.kind {
        GraphEdgeKind::Real(edge) => match &edge.provenance {
            super::EdgeProvenance::Engine { kind, .. } => {
                kept.contains(&(edge.after.clone(), edge.before.clone(), *kind))
            }
            _ => false,
        },
        GraphEdgeKind::Membership { .. } | GraphEdgeKind::Boundary => false,
    }
}

/// `modDependencies` is only ever
/// a "needs" statement — RimWorld itself never reorders by it, it only
/// warns when the named mod is missing (see [`EdgeKind::ModDependency`]'s
/// own doc comment) — while an explicit `loadAfter`/`loadBefore` is the
/// author's own direct order statement, so it must be the one that
/// survives when the two contradict **within the `Declared` layer**,
/// where both kinds actually coexist (`ModDependency`'s own
/// `EdgeKind::strength()` is `Declared`; `LoadAfter`/`LoadBefore` are
/// `Declared` too).
///
/// **The whole function is gated on `edge.layer` being `Declared` or
/// `Inferred`** — the only two layers with a real, kind-specific tie-break
/// rule below; every other layer falls through to the `1` neutral rank.
/// `ForceLoadAfter`/`ForceLoadBefore` (and a load-time `AssemblyRef`,
/// and `PatchInjectedNode`) are *always* `EdgeStrength::Hard` →
/// `Layer::Hard` (`EdgeKind::strength()`), never `Declared`. Ranking by
/// `EdgeKind` alone would put `ForceLoadAfter`/`ForceLoadBefore` at `2`
/// (protected) with every *other* kind — including a `Hard`-strength
/// `AssemblyRef`, which carries no `EdgeKind` variant of its own to rank
/// specially — in the `1` middle bucket. Since `kind_rank` is
/// `tie_break_key`'s own leading component, that would make a load-time
/// `AssemblyRef` the unconditional drop victim in *any* `Layer::Hard`
/// cycle contradicting a `forceLoad*`, regardless of `hard_dependents` —
/// directly inverting `Layer::Hard`'s own "breaking these breaks the
/// game" contract (a wrong `forceLoadBefore` would win over a genuine
/// load-time DLL reference, producing an order that throws
/// `TypeLoadException` at startup while reporting the cycle resolved).
/// Gating the layer up front, rather than editing the kind list, leaves
/// every other layer's tie-break untouched by construction, including
/// any future layer this function's kind list hasn't been taught about;
/// it also keeps `ModDependency`'s own rank `0` from reaching any layer
/// it might appear in later. `ForceLoadAfter`/`ForceLoadBefore` stay in
/// the kind list below (never actually reached through this path, since
/// they're `Hard`-only). `Layer::Inferred`'s own rule is separate (see the
/// `edge.layer == Layer::Inferred` branch below) — `PatchRemovedNodeCosmetic`
/// is the one `Inferred` kind dropped first — a deliberate design choice:
/// enforced when free, dropped first when a cycle forces a choice.
///
/// **Direction note**: [`tie_break_key`]'s own candidates are chosen for
/// *dropping* by `pool.iter().min_by_key(tie_break_key)`
/// (`break_cycles`) — confirmed against
/// `breaks_a_two_cycle_dropping_the_edge_pointing_at_the_least_depended_on_mod`,
/// whose `before.hard_dependents` component only makes sense
/// read that way. So "`loadAfter` survives" means `LoadAfter`/
/// `LoadBefore` must get the *highest* rank here (never the minimum,
/// i.e. never preferentially dropped) and `ModDependency` the *lowest*
/// (preferentially dropped first); the reverse (read against
/// `min_by_key`-drops) would drop `LoadAfter`/`LoadBefore` first and
/// *increase* declared-order violations. `0` for `ModDependency`, `1` for
/// everything else
/// (every other `EdgeKind` that could in principle reach this function —
/// none do, since `Declared`'s only real members are `LoadAfter`/
/// `LoadBefore`/`ModDependency`/`AssemblyVersionPrecedence` — a
/// rule-origin edge, or a tier sentinel edge; none of those are
/// "declared order vs. a mere need", so none should out-rank the other
/// two on this axis), `2` for `LoadAfter`/`LoadBefore`/`ForceLoadAfter`/
/// `ForceLoadBefore`. Content-based, like every other component of
/// `tie_break_key`, so determinism is unaffected.
fn kind_rank(edge: &GraphEdge) -> u8 {
    if edge.layer != Layer::Declared && edge.layer != Layer::Inferred {
        return 1;
    }
    let GraphEdgeKind::Real(ordering_edge) = &edge.kind else {
        return 1;
    };
    let super::EdgeProvenance::Engine { kind, .. } = &ordering_edge.provenance else {
        return 1;
    };
    if edge.layer == Layer::Inferred {
        // A deliberate design choice: a cosmetic `PatchRemovedNode` pair
        // changes nothing in the final defs either order, so it's the one
        // `Inferred` kind that should give way first when the layer's own
        // edges have to be broken — `0` (preferentially dropped) here,
        // versus `1` for every other `Inferred` kind. Never reaches `2`:
        // `Declared`'s own `LoadAfter`/`LoadBefore`/`ForceLoadAfter`/
        // `ForceLoadBefore` protection has no `Inferred`-layer analogue.
        return match kind {
            EdgeKind::PatchRemovedNodeCosmetic => 0,
            _ => 1,
        };
    }
    match kind {
        EdgeKind::ModDependency => 0,
        EdgeKind::LoadAfter
        | EdgeKind::LoadBefore
        | EdgeKind::ForceLoadAfter
        | EdgeKind::ForceLoadBefore => 2,
        _ => 1,
    }
}

/// Whether `edge` (a genuine mod-to-mod [`super::OrderingEdge`], never a
/// tier `Membership`/`Boundary` edge) agrees with, contradicts, or says
/// nothing about `current`'s own relative order of its two mods — the
/// unused signal "cycle-break direction for a
/// mutual `patch_removed_node` pair" entry identified: real, game-verified
/// evidence that a mutual same-kind, same-layer 2-cycle
/// (`example.progression.temperature`/`example.hygienepatches`, both
/// `Inferred`-layer `PatchRemovedNode` edges pointing at each other, zero
/// `hard_dependents` either side) was broken in the wrong direction:
/// `kind_rank` ties (same kind), the `hard_dependents` pair ties (both
/// zero), so the drop was decided by the mods' own ids in alphabetical
/// order — deterministic, but carrying no information about which
/// direction is correct.
///
/// `edge.before` must load before `edge.after` for `edge` to hold — so it
/// **agrees** with `current` when `current` already has `before` at an
/// earlier position than `after` (`2`, "prefer to keep": [`tie_break_key`]
/// feeds this into `pool.iter().min_by_key`, which drops the *lowest*
/// key, so agreement must rank high to protect the edge from being
/// dropped), **contradicts** it when the positions are reversed (`0`,
/// "prefer to drop" — the lowest value, so a genuine contradiction always
/// outranks every mod pair with no signal at all), and returns **`1`**
/// (`"no signal"`) when either mod is absent from `current` — a mod the
/// user's own `ModsConfig.xml` never listed carries no order evidence to
/// disbelieve either edge with, so both candidates tie here and fall
/// through to [`tie_break_key`]'s own next component exactly as they did
/// before this signal existed, rather than one arbitrarily outranking the
/// other. Two mods that are both present can never tie at this level:
/// [`LoadOrder::position`] is a total order over distinct positions, so
/// `before_position == after_position` is impossible for two different
/// mods.
///
/// **Applies under both [`super::TieBreak::Rebuild`] and
/// [`super::TieBreak::PreserveCurrent`], not gated on `tie_break` at all**
/// — a deliberate decision (the user's own
/// call), not an oversight that only `PreserveCurrent` should have this.
/// Cycle-breaking and emission are different questions: emission asks
/// *where* mods go once every edge is settled, and `Rebuild` legitimately
/// ignores `current` there (`sort::emit::non_propagated_keys`).
/// Cycle-breaking asks *which of two contradictory edges to disbelieve*,
/// and "disbelieve the one contradicting a configuration the user is
/// actually running" is sound regardless of how the result then gets
/// emitted — `current` is unconditionally available on `SortInput` for
/// exactly this reason (used for any-of preference and disturbance
/// measurement in every mode already), so reading it here needs no new
/// plumbing beyond threading it into this module.
fn current_order_rank(edge: &super::OrderingEdge, current: &LoadOrder) -> u8 {
    match (
        current.position(&edge.before),
        current.position(&edge.after),
    ) {
        (Some(before_position), Some(after_position)) if before_position < after_position => 2,
        (Some(_), Some(_)) => 0,
        _ => 1,
    }
}

/// `current_order_rank` for a [`GraphEdge`] — `1` ("no signal") for a tier
/// `Membership`/`Boundary` edge, which names no real
/// [`super::OrderingEdge`] to look up in `current` at all.
fn graph_current_order_rank(edge: &GraphEdge, current: &LoadOrder) -> u8 {
    match &edge.kind {
        GraphEdgeKind::Real(ordering_edge) => current_order_rank(ordering_edge, current),
        GraphEdgeKind::Membership { .. } | GraphEdgeKind::Boundary => 1,
    }
}

/// `(kind_rank, before.hard_dependents asc, after.hard_dependents desc,
/// current_order_rank, after, before, edge content)` — `kind_rank` is
/// the leading component (see [`kind_rank`]'s own doc comment), followed
/// by the `hard_dependents` pair. `current_order_rank` (see its
/// own doc comment) sits below both — real evidence (`kind_rank`,
/// `hard_dependents`) always decides first when it can — but *above* the
/// trailing `after`/`before` alphabetical fallback, since the mods' own
/// ids carry no information about which of two evidence-tied edges is
/// correct while the current order, when it has an opinion, does. The
/// trailing `edge content` component only ever breaks a tie between two
/// *different* edges connecting the exact same `(source, target)` pair
/// (e.g. two distinct engine edges between the same mod pair): without
/// it, such a tie would fall back to `graph.edge_references()`'s own
/// iteration order, which depends on insertion order and hence on the
/// *input* edges' own order — making `SortOutcome::dropped` (though never
/// the final emitted order, since dropping either parallel edge leaves
/// the other enforcing the identical constraint) depend on something
/// other than the edges' own content. See
/// `sort_proptest::sort_is_invariant_under_input_edge_shuffling`, which
/// compares the whole `SortOutcome` (not just the emitted order) for
/// exactly this reason — `current_order_rank` is itself content-based
/// (a pure function of `edge`'s own `after`/`before` and `current`, which
/// is identical across every shuffle of the *input* edges), so it
/// preserves that same invariance.
fn tie_break_key(
    source: NodeIndex,
    target: NodeIndex,
    graph: &SortGraph,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    edge: &GraphEdge,
    current: &LoadOrder,
) -> (u8, usize, Reverse<usize>, u8, Node, Node, GraphEdge) {
    let source_node = graph[source].clone();
    let target_node = graph[target].clone();
    let before_hard_dependents = hard_dependents_of(&source_node, mods_by_id);
    let after_hard_dependents = hard_dependents_of(&target_node, mods_by_id);
    (
        kind_rank(edge),
        before_hard_dependents,
        Reverse(after_hard_dependents),
        graph_current_order_rank(edge, current),
        target_node,
        source_node,
        edge.clone(),
    )
}

/// Reconstructs a path (as mod ids only — sentinel hops are omitted, since
/// [`DroppedEdge::witness_cycle`] is `Vec<ModId>`) from `from` to `to`
/// using only edges currently in `graph` and restricted to `within`, via a
/// BFS that visits a node's candidate neighbors in sorted (`Node`, hence
/// `ModId`) order rather than `graph.edges_directed`'s own iteration
/// order: when more than one shortest path exists, which one BFS finds
/// first would otherwise depend on edge insertion order, making this
/// witness (though display-only) depend on the *input* edges' own order —
/// sorting first picks the same, lexicographically-smallest witness
/// regardless. Returns an empty vector if the still-connected mods can't
/// otherwise be found (should not happen for a genuine SCC member, but
/// this is display-only, so it degrades gracefully rather than panicking).
fn witness_path(
    graph: &SortGraph,
    within: &BTreeSet<NodeIndex>,
    from: NodeIndex,
    to: NodeIndex,
) -> Vec<ModId> {
    let mut previous: BTreeMap<NodeIndex, NodeIndex> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(from);
    let mut visited: BTreeSet<NodeIndex> = BTreeSet::from([from]);
    while let Some(node) = queue.pop_front() {
        if node == to {
            break;
        }
        let mut candidates: Vec<NodeIndex> = graph
            .edges_directed(node, Direction::Outgoing)
            .map(|edge| edge.target())
            .filter(|next| within.contains(next) && !visited.contains(next))
            .collect();
        candidates.sort_by_key(|&next| graph[next].clone());
        for next in candidates {
            if visited.insert(next) {
                previous.insert(next, node);
                queue.push_back(next);
            }
        }
    }
    if !visited.contains(&to) {
        return Vec::new();
    }
    let mut path = vec![to];
    let mut current = to;
    while current != from {
        let Some(&prev) = previous.get(&current) else {
            break;
        };
        path.push(prev);
        current = prev;
    }
    path.reverse();
    path.into_iter()
        .filter_map(|idx| match &graph[idx] {
            Node::Mod(id) => Some(id.clone()),
            Node::TierStart(_) | Node::TierEnd(_) => None,
        })
        .collect()
}

/// When the accepted graph
/// already has a direct edge from `dropped_target` straight back to
/// `dropped_source` — the two-mod contradiction a length-2 witness cycle
/// proves — that edge is unambiguously "the one that won"; this returns
/// it. `None` when no such direct edge exists (a longer cycle, where the
/// contradiction runs through other mods and no single edge can be
/// blamed).
fn direct_winner(
    graph: &SortGraph,
    dropped_target: NodeIndex,
    dropped_source: NodeIndex,
) -> Option<super::OrderingEdge> {
    // `.find(...)` would return the *first* matching edge in
    // `graph.edges_directed`'s own iteration order — when `dropped_target`
    // has *two* parallel surviving edges to `dropped_source` (a `LoadAfter`
    // and a `ModDependency` between the same pair, both accepted), which
    // one gets named `winner` would depend on petgraph's iteration order,
    // itself dependent on the *input* edges' own insertion order — exactly
    // what `sort_is_invariant_under_input_edge_shuffling` exists to catch
    // (display-only, so the final emitted order is never affected, but
    // `DroppedEdge::winner` — a `Finding`'s own displayed rationale — is).
    // `.min()` over every match, not `.find()` over the first: `OrderingEdge`'s
    // own content-based `Ord` (the same convention `witness_path`/
    // `scc_real_neighbors` already use) picks the same edge regardless of
    // iteration order.
    graph
        .edges_directed(dropped_target, Direction::Outgoing)
        .filter(|e| e.target() == dropped_source)
        .filter_map(|e| match &e.weight().kind {
            GraphEdgeKind::Real(edge) => Some(edge.clone()),
            GraphEdgeKind::Membership { .. } | GraphEdgeKind::Boundary => None,
        })
        .min()
}

/// Whether `id`'s own *nominal* tier assignment (`sort::tiers::assign`,
/// unaffected by any promotion — see `sort::emit::placement_bias`'s own
/// doc comment) is an explicit `Top`/`Bottom` [`crate::domain::PlacementRule`]
/// pin.
fn is_placement_pinned(tier_of: &BTreeMap<ModId, (Tier, TierReason)>, id: &ModId) -> bool {
    matches!(
        tier_of.get(id),
        Some((Tier::Top | Tier::Bottom, TierReason::Placement(_)))
    )
}

/// Every still-present `Real` edge in `scc_set` directly connecting `node`
/// to another SCC member, both directions, as `(neighbor, edge)` pairs —
/// sorted by `(neighbor node, edge content)` for the same
/// permutation-invariant determinism [`witness_path`] relies on when more
/// than one edge could be walked next.
fn scc_real_neighbors(
    graph: &SortGraph,
    scc_set: &BTreeSet<NodeIndex>,
    removed: EdgeIndex,
    node: NodeIndex,
) -> Vec<(NodeIndex, super::OrderingEdge)> {
    let mut neighbors: Vec<(NodeIndex, super::OrderingEdge)> = graph
        .edges_directed(node, Direction::Outgoing)
        .filter(|e| e.id() != removed && scc_set.contains(&e.target()))
        .filter_map(|e| match &e.weight().kind {
            GraphEdgeKind::Real(edge) => Some((e.target(), edge.clone())),
            GraphEdgeKind::Membership { .. } | GraphEdgeKind::Boundary => None,
        })
        .collect();
    neighbors.extend(
        graph
            .edges_directed(node, Direction::Incoming)
            .filter(|e| e.id() != removed && scc_set.contains(&e.source()))
            .filter_map(|e| match &e.weight().kind {
                GraphEdgeKind::Real(edge) => Some((e.source(), edge.clone())),
                GraphEdgeKind::Membership { .. } | GraphEdgeKind::Boundary => None,
            }),
    );
    neighbors.sort_by(|(a_idx, a_edge), (b_idx, b_edge)| {
        (graph[*a_idx].clone(), a_edge.clone()).cmp(&(graph[*b_idx].clone(), b_edge.clone()))
    });
    neighbors
}

/// The mod actually responsible for a dropped tier `Membership` edge: a
/// BFS outward from
/// `mod_id`'s own node, through the SCC's still-present `Real` edges
/// ([`scc_real_neighbors`], both directions — an *undirected* walk to the
/// nearest connected placement, not a walk along the specific chain of
/// edges that forced the promotion), until it reaches a mod whose
/// *nominal* tier is an explicit `Top`/`Bottom` pin
/// ([`is_placement_pinned`]) — the nearest placed mod in the cycle,
/// returning the specific edge that closed the walk. A low practical risk,
/// disclosed rather than guarded: naming it "the node that anchors the
/// cycle's contradiction" would overclaim a directional/causal reading
/// this undirected BFS doesn't actually establish — a real-install
/// closure-math check (attributed = direct + transitive) reconciles
/// exactly, so this has not been observed to pick a misleading witness in
/// practice. Falls back to the
/// "smallest-keyed `Real` edge touching `mod_id`" rule when no placed
/// node is reachable within the SCC at all (a promotion with no placement
/// involved, e.g. two ordinary `Body` mods deadlocked over an unrelated
/// declared pair) — that rule's own tie-break note still applies verbatim
/// to the fallback: any layer may name the cause, since the mod's tier
/// membership is typically much weaker than whatever forced it out.
fn find_promotion_cause(
    graph: &SortGraph,
    scc_set: &BTreeSet<NodeIndex>,
    removed: EdgeIndex,
    mod_id: &ModId,
    start: NodeIndex,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
) -> Option<super::OrderingEdge> {
    let mut visited: BTreeSet<NodeIndex> = BTreeSet::from([start]);
    let mut queue: VecDeque<(NodeIndex, super::OrderingEdge)> = VecDeque::new();
    for (next, edge) in scc_real_neighbors(graph, scc_set, removed, start) {
        if visited.insert(next) {
            queue.push_back((next, edge));
        }
    }
    while let Some((node, edge_in)) = queue.pop_front() {
        if let Node::Mod(id) = &graph[node]
            && is_placement_pinned(tier_of, id)
        {
            return Some(edge_in);
        }
        for (next, edge) in scc_real_neighbors(graph, scc_set, removed, node) {
            if visited.insert(next) {
                queue.push_back((next, edge));
            }
        }
    }

    graph
        .edge_references()
        .filter(|e| e.id() != removed)
        .filter(|e| scc_set.contains(&e.source()) && scc_set.contains(&e.target()))
        .filter_map(|e| match &e.weight().kind {
            GraphEdgeKind::Real(edge) if edge.after == *mod_id || edge.before == *mod_id => {
                Some(edge.clone())
            }
            _ => None,
        })
        .min_by(|a, b| (&a.after, &a.before).cmp(&(&b.after, &b.before)))
}

/// Breaks every cycle formed by the edges of `layer` (which the caller has
/// just finished adding), dropping one edge of that layer per cycle using
/// the `(kind_rank, before.hard_dependents asc, after.hard_dependents
/// desc, current_order_rank, after, before, edge content)` tie-break (see
/// [`tie_break_key`]), preferring edges the caller hasn't
/// `KeepEdge`-pinned. `current` is [`SortInput::current`][super::SortInput::current]
/// unconditionally — see [`current_order_rank`]'s own doc comment for why
/// this runs the same way under both [`super::TieBreak`] modes.
pub(super) fn break_cycles(
    graph: &mut SortGraph,
    layer: Layer,
    kept: &BTreeSet<(ModId, ModId, EdgeKind)>,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    current: &LoadOrder,
) -> CycleBreakResult {
    let mut result = CycleBreakResult::default();

    // Bounded by construction (each iteration removes one edge), but
    // capped defensively so a bug here can never hang the sorter.
    let max_iterations = graph.edge_count() + 1;
    for _ in 0..max_iterations {
        // `tarjan_scc` emits SCCs in DFS-completion order, and petgraph's
        // DFS walks `edges_directed`, whose neighbor order follows the
        // (reverse) edge-insertion order of the adjacency list — which
        // traces straight back to the *input* edges' own order
        // (`report.edges`, via `layers::engine_edges_of_strength` and
        // `graph::add_real_edges`). When two or more vertex-disjoint
        // non-trivial SCCs are alive in the same pass, this loop drops one
        // edge from each in `sccs`'s own order, pushing onto
        // `result.dropped` — never re-sorted — so the *sequence* of
        // `result.dropped` leaked input edge order without this sort. Same
        // content-based normalization every other petgraph-order consumer
        // in this module already applies ([`witness_path`],
        // [`scc_real_neighbors`], `direct_winner`'s `.min()`,
        // `tie_break_key`'s trailing edge-content component): SCCs are
        // vertex-disjoint, so each one's smallest [`Node`] is a unique
        // total key (`Node` derives `Ord` for exactly this purpose). Keyed
        // on the graph-node's own `Node` weight, not the petgraph
        // `NodeIndex` — a `NodeIndex` ordering only reflects node
        // *insertion* order, which is no more content-determined than the
        // edge order this normalizes. See
        // `two_disjoint_cycles_drop_in_the_same_order_regardless_of_edge_insertion_order`.
        let mut sccs = tarjan_scc(&*graph);
        sccs.sort_by_cached_key(|scc| scc.iter().map(|&n| graph[n].clone()).min());
        let mut broke_any = false;

        for scc in sccs {
            let is_self_loop = scc.len() == 1
                && graph
                    .edges_directed(scc[0], Direction::Outgoing)
                    .any(|e| e.target() == scc[0]);
            if scc.len() < 2 && !is_self_loop {
                continue;
            }
            let scc_set: BTreeSet<NodeIndex> = scc.iter().copied().collect();

            let candidates: Vec<(EdgeIndex, NodeIndex, NodeIndex, GraphEdge)> = graph
                .edge_references()
                .filter(|e| e.weight().layer == layer)
                .filter(|e| scc_set.contains(&e.source()) && scc_set.contains(&e.target()))
                .map(|e| (e.id(), e.source(), e.target(), e.weight().clone()))
                .collect();
            if candidates.is_empty() {
                // No edge of this layer participates — a pre-existing
                // cycle from an earlier layer, which the invariant says
                // can't happen. Skip rather than loop forever on it.
                continue;
            }

            // A `Boundary` edge (the tier backbone itself) is an
            // absolute-last-resort drop — sacrificing one collapses every
            // tier after it, per `GraphEdgeKind::Boundary`'s own doc
            // comment. Exclude it from the pool whenever any
            // `Real`/`Membership` candidate is available in this same SCC,
            // so the backbone only ever yields when nothing else in the
            // cycle could instead.
            let non_boundary: Vec<_> = candidates
                .iter()
                .filter(|(_, _, _, w)| !matches!(w.kind, GraphEdgeKind::Boundary))
                .cloned()
                .collect();
            let candidates: &[(EdgeIndex, NodeIndex, NodeIndex, GraphEdge)] =
                if non_boundary.is_empty() {
                    &candidates
                } else {
                    &non_boundary
                };

            let non_kept: Vec<_> = candidates
                .iter()
                .filter(|(_, _, _, w)| !is_kept(w, kept))
                .cloned()
                .collect();
            let pool: &[(EdgeIndex, NodeIndex, NodeIndex, GraphEdge)] = if non_kept.is_empty() {
                candidates
            } else {
                &non_kept
            };

            // Prefer dropping a tier
            // `Membership` edge over a `Real` enforced one when both are
            // candidates in the same cycle — losing a mod's tier
            // placement (disclosed as `TierReason::PromotedBy`) is a
            // smaller sacrifice than dropping a genuine cross-mod
            // ordering fact, which is why tier boundaries are the layer's
            // weakest edges. Mirrors the `Boundary`-exclusion step above:
            // restrict to the `Membership` subset only when one exists.
            let membership_preferred: Vec<_> = pool
                .iter()
                .filter(|(_, _, _, w)| matches!(w.kind, GraphEdgeKind::Membership { .. }))
                .cloned()
                .collect();
            let pool: &[(EdgeIndex, NodeIndex, NodeIndex, GraphEdge)] =
                if membership_preferred.is_empty() {
                    pool
                } else {
                    &membership_preferred
                };

            // `pool` is always non-empty: `candidates` was checked above,
            // and `pool` falls back to it when `non_kept`/
            // `membership_preferred` are empty.
            let Some((edge_id, source, target, weight)) = pool
                .iter()
                .min_by_key(|(_, s, t, w)| tie_break_key(*s, *t, graph, mods_by_id, w, current))
                .cloned()
            else {
                continue;
            };
            let witness = witness_path(graph, &scc_set, target, source);

            match &weight.kind {
                GraphEdgeKind::Real(ordering_edge) => {
                    let winner = direct_winner(graph, target, source);
                    // A dropped `UserDecision` edge must never vanish
                    // silently — see `SortWarning::UserDecisionOverruled`'s
                    // own doc comment.
                    if matches!(
                        &ordering_edge.provenance,
                        super::EdgeProvenance::Rule {
                            origin: RuleOrigin::UserDecision,
                            ..
                        }
                    ) {
                        result.warnings.push(SortWarning::UserDecisionOverruled {
                            edge: ordering_edge.clone(),
                            winner: winner.clone(),
                        });
                    }
                    result.dropped.push(DroppedEdge {
                        edge: ordering_edge.clone(),
                        witness_cycle: witness,
                        winner,
                    });
                }
                GraphEdgeKind::Membership { mod_id, .. } => {
                    // Exactly one of the dropped `Membership` edge's own
                    // endpoints is the mod's own node (`Node::Mod`), by
                    // `sort::tiers::add_sentinel_edges_for_layer`'s own
                    // construction (`TierStart -> mod` or `mod -> TierEnd`,
                    // never both sentinels) — the other is a tier sentinel,
                    // never the walk's own starting point.
                    let mod_node = match (&graph[source], &graph[target]) {
                        (Node::Mod(_), _) => source,
                        _ => target,
                    };
                    if let Some(cause) =
                        find_promotion_cause(graph, &scc_set, edge_id, mod_id, mod_node, tier_of)
                    {
                        result.promotions.insert(mod_id.clone(), cause);
                    }
                }
                GraphEdgeKind::Boundary => {
                    // The five-tier backbone itself had to yield — extremely
                    // pathological; nothing mod-shaped to report.
                }
            }

            graph.remove_edge(edge_id);
            broke_any = true;
        }

        if !broke_any {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use petgraph::stable_graph::StableDiGraph;

    use super::*;
    use crate::sort::{EdgeProvenance, OrderingEdge};

    /// `current` for every test that isn't exercising [`current_order_rank`]
    /// itself: an empty [`LoadOrder`] carries no position for any mod, so
    /// `current_order_rank` returns `1` ("no signal") for every candidate
    /// and every one of these pre-existing tests keeps deciding its
    /// tie-break exactly the way it did before this component existed.
    fn no_current_order() -> LoadOrder {
        LoadOrder::new(Vec::new())
    }

    fn real_edge(after: &str, before: &str, layer: Layer) -> GraphEdge {
        GraphEdge {
            layer,
            kind: GraphEdgeKind::Real(OrderingEdge {
                after: ModId::new(after),
                before: ModId::new(before),
                layer,
                provenance: EdgeProvenance::Engine {
                    kind: EdgeKind::LoadAfter,
                    detail: String::new(),
                },
            }),
        }
    }

    #[test]
    fn breaks_a_two_cycle_dropping_the_edge_pointing_at_the_least_depended_on_mod() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a")));
        let b = graph.add_node(Node::Mod(ModId::new("b")));
        // a -> b (b must load before a) and b -> a (a must load before b): a genuine cycle.
        graph.add_edge(b, a, real_edge("a", "b", Layer::Declared));
        graph.add_edge(a, b, real_edge("b", "a", Layer::Declared));

        let mut mods_by_id = BTreeMap::new();
        let mod_a = test_mod("a", 0);
        let mod_b = test_mod("b", 5);
        mods_by_id.insert(ModId::new("a"), &mod_a);
        mods_by_id.insert(ModId::new("b"), &mod_b);

        let result = break_cycles(
            &mut graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        assert_eq!(graph.edge_count(), 1, "exactly one edge must survive");
        // A direct two-mod
        // contradiction must name the surviving edge as the winner — the
        // edge pointing at "a" (least depended-on) is dropped, so the
        // survivor is the other one, `{after: a, before: b}`.
        assert_eq!(result.dropped[0].edge.before, ModId::new("a"));
        let winner = result.dropped[0]
            .winner
            .as_ref()
            .expect("a direct 2-cycle must name a winner");
        assert_eq!(winner.after, ModId::new("a"));
        assert_eq!(winner.before, ModId::new("b"));
        assert!(
            result.warnings.is_empty(),
            "a dropped engine edge must never emit UserDecisionOverruled: {:?}",
            result.warnings
        );
    }

    /// A `ModDependency` edge
    /// directly contradicting a `LoadAfter` edge, both `Declared`, no
    /// `hard_dependents` difference to fall back on (`mods_by_id` is
    /// empty, so both mods report `0`) — isolating `kind_rank` as the
    /// only thing that can decide it.
    ///
    /// **Mod naming matters**: naming the `ModDependency` edge's own `after`
    /// mod `"a"` and the `LoadAfter` edge's `after` mod `"b"` would let
    /// `tie_break_key`'s own `target_node` component (the tuple's *fourth*
    /// element, right after `kind_rank`/`hard_dependents` asc/desc) favor
    /// dropping the `ModDependency` edge on its own, since `"a" < "b"`,
    /// with or without `kind_rank` doing anything. The mods are named
    /// `"zzz"` (the `ModDependency` edge's own `after`) and `"aaa"` (the
    /// `LoadAfter` edge's own `after`) — `target_node` alone
    /// (`"aaa" < "zzz"`) favors dropping the *`LoadAfter`* edge instead,
    /// the wrong answer, so only `kind_rank` correctly overriding that bias
    /// back toward `ModDependency` proves this test isolates what it claims
    /// to.
    ///
    /// What would make this fail: removing `kind_rank` from `tie_break_key`
    /// (or reversing its direction) flips this fixture's own result to
    /// dropping `LoadAfter` instead.
    #[test]
    fn a_mod_dependency_edge_is_dropped_before_a_contradicting_load_after_edge() {
        let mut graph: SortGraph = StableDiGraph::new();
        let aaa = graph.add_node(Node::Mod(ModId::new("aaa")));
        let zzz = graph.add_node(Node::Mod(ModId::new("zzz")));

        // "zzz" declares a modDependency on "aaa" (zzz after aaa): graph
        // edge aaa -> zzz, per `add_real_edges`'s before->after convention.
        graph.add_edge(
            aaa,
            zzz,
            GraphEdge {
                layer: Layer::Declared,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new("zzz"),
                    before: ModId::new("aaa"),
                    layer: Layer::Declared,
                    provenance: EdgeProvenance::Engine {
                        kind: EdgeKind::ModDependency,
                        detail: String::new(),
                    },
                }),
            },
        );
        // "aaa" declares loadAfter="zzz" (aaa after zzz), directly
        // contradicting: graph edge zzz -> aaa.
        graph.add_edge(zzz, aaa, real_edge("aaa", "zzz", Layer::Declared));

        let mods_by_id = BTreeMap::new();
        let result = break_cycles(
            &mut graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        let EdgeProvenance::Engine { kind, .. } = &result.dropped[0].edge.provenance else {
            panic!("expected an engine edge");
        };
        assert_eq!(
            *kind,
            EdgeKind::ModDependency,
            "the ModDependency edge must be the one dropped, not the LoadAfter edge"
        );
        let winner = result.dropped[0]
            .winner
            .as_ref()
            .expect("a direct 2-cycle must name a winner");
        let EdgeProvenance::Engine {
            kind: winner_kind, ..
        } = &winner.provenance
        else {
            panic!("expected an engine edge");
        };
        assert_eq!(
            *winner_kind,
            EdgeKind::LoadAfter,
            "the LoadAfter edge must survive as the winner"
        );
    }

    /// The cycle-break direction for a mutual
    /// `patch_removed_node` pair — a real, game-verified failure shape
    /// reproduced at unit scale: a mutual same-*kind*, same-*layer* 2-cycle
    /// (both edges `Inferred`/`PatchRemovedNode`, mirroring
    /// `example.progression.temperature`/`example.hygienepatches` exactly)
    /// with zero `hard_dependents` on both sides, so `kind_rank` and the
    /// `hard_dependents` pair both tie and can decide nothing. `current`
    /// has `zzz_loads_first` loading before `aaa_loads_second` — the
    /// *opposite* of alphabetical order, chosen deliberately so a pass
    /// only because `current_order_rank` falls through to the
    /// alphabetical `after`/`before` fallback cannot masquerade as a real
    /// one: `"aaa_loads_second" < "zzz_loads_first"`, so the
    /// tie-break's own `target_node` component alone would favor dropping
    /// the edge that *targets* `"zzz_loads_first"` — the edge that
    /// **agrees** with `current` here — which is exactly the wrong
    /// direction this test exists to catch.
    ///
    /// What would make this fail: deleting `current_order_rank` from
    /// `tie_break_key` (or reversing its direction) — the remaining five
    /// components drop the edge naming `"aaa_loads_second"` as `before`
    /// instead, the edge that *agrees* with `current`, reproducing the
    /// real-install failure.
    #[test]
    fn a_mutual_same_kind_same_layer_two_cycle_breaks_toward_the_current_order() {
        fn inferred_patch_removed_node_edge(after: &str, before: &str) -> GraphEdge {
            GraphEdge {
                layer: Layer::Inferred,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new(after),
                    before: ModId::new(before),
                    layer: Layer::Inferred,
                    provenance: EdgeProvenance::Engine {
                        kind: EdgeKind::PatchRemovedNode,
                        detail: String::new(),
                    },
                }),
            }
        }

        let mut graph: SortGraph = StableDiGraph::new();
        let zzz = graph.add_node(Node::Mod(ModId::new("zzz_loads_first")));
        let aaa = graph.add_node(Node::Mod(ModId::new("aaa_loads_second")));

        // Edge A: "zzz_loads_first" after "aaa_loads_second" — wants
        // aaa_loads_second before zzz_loads_first, contradicting `current`
        // (below). Graph edge aaa -> zzz, per `add_real_edges`'s
        // before->after convention.
        graph.add_edge(
            aaa,
            zzz,
            inferred_patch_removed_node_edge("zzz_loads_first", "aaa_loads_second"),
        );
        // Edge B: "aaa_loads_second" after "zzz_loads_first" — wants
        // zzz_loads_first before aaa_loads_second, agreeing with `current`.
        // Graph edge zzz -> aaa.
        graph.add_edge(
            zzz,
            aaa,
            inferred_patch_removed_node_edge("aaa_loads_second", "zzz_loads_first"),
        );

        let mods_by_id = BTreeMap::new();
        let current = LoadOrder::new(vec![
            ModId::new("zzz_loads_first"),
            ModId::new("aaa_loads_second"),
        ]);

        let result = break_cycles(
            &mut graph,
            Layer::Inferred,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &current,
        );

        assert_eq!(result.dropped.len(), 1);
        assert_eq!(
            result.dropped[0].edge.after,
            ModId::new("zzz_loads_first"),
            "the edge contradicting `current` (Edge A) must be the one dropped"
        );
        assert_eq!(
            result.dropped[0].edge.before,
            ModId::new("aaa_loads_second")
        );
        let winner = result.dropped[0]
            .winner
            .as_ref()
            .expect("a direct 2-cycle must name a winner");
        assert_eq!(
            winner.after,
            ModId::new("aaa_loads_second"),
            "the edge agreeing with `current` (Edge B) must survive as the winner"
        );
        assert_eq!(winner.before, ModId::new("zzz_loads_first"));
    }

    /// `kind_rank` must never rank `EdgeKind`s in a `Layer::Hard` cycle:
    /// ranking with no layer check would make a cycle between a load-time
    /// `AssemblyRef` (no special-cased `EdgeKind`, in the `1` bucket) and a
    /// `ForceLoadAfter` (ranked `2`, protected) always drop the
    /// `AssemblyRef` — regardless of `hard_dependents` — since `kind_rank`
    /// is `tie_break_key`'s own leading, dominating component. That would
    /// directly invert `Layer::Hard`'s own contract ("breaking these breaks
    /// the game"): a wrong author-written `forceLoadAfter` would always
    /// beat a genuine load-time DLL reference.
    ///
    /// `provider`/`consumer` form a direct two-mod `Layer::Hard`
    /// contradiction: `consumer`'s DLL references `provider`'s (a load-time
    /// `AssemblyRef`, `consumer` after `provider`) while `provider`'s own
    /// (wrong) `forceLoadAfter` demands the opposite (`provider` after
    /// `consumer`). Run twice, swapping which mod carries more
    /// `hard_dependents` each time ("both hard_dependents orientations"):
    /// `kind_rank` returns a constant for this `Layer::Hard` cycle (never
    /// `Declared`), so the `before.hard_dependents asc` component is what
    /// decides, and the drop victim flips with the orientation exactly like
    /// `breaks_a_two_cycle_dropping_the_edge_pointing_at_the_least_depended_on_mod`
    /// establishes for an ordinary pair with no `kind_rank` involved at
    /// all. **The orientation flip is the actual proof**: a layer-blind
    /// `kind_rank` would drop the `AssemblyRef` in *both* orientations
    /// unconditionally; asserting a single fixed kind always survives would
    /// not distinguish the two, since one orientation's correct answer
    /// coincides with the layer-blind one — only the orientation-flip
    /// itself proves `kind_rank` does not dominate a `Hard` cycle.
    ///
    /// What would make this fail: removing the `edge.layer != Layer::Declared`
    /// gate from `kind_rank` — both
    /// orientations below would then drop the `AssemblyRef` instead of
    /// following `hard_dependents`.
    #[test]
    fn kind_rank_never_influences_a_hard_layer_cycle_in_either_hard_dependents_orientation() {
        fn hard_edge(after: &str, before: &str, kind: EdgeKind) -> GraphEdge {
            GraphEdge {
                layer: Layer::Hard,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new(after),
                    before: ModId::new(before),
                    layer: Layer::Hard,
                    provenance: EdgeProvenance::Engine {
                        kind,
                        detail: String::new(),
                    },
                }),
            }
        }

        // `provider_dependents == 0`: the `AssemblyRef` edge's own
        // `before.hard_dependents` (`provider`) is the smaller of the two,
        // so — with `kind_rank` neutralized — it must be the one dropped;
        // `ForceLoadAfter` survives.
        {
            let mut graph: SortGraph = StableDiGraph::new();
            let consumer = graph.add_node(Node::Mod(ModId::new("consumer")));
            let provider = graph.add_node(Node::Mod(ModId::new("provider")));
            // consumer after provider (AssemblyRef): graph provider -> consumer.
            graph.add_edge(
                provider,
                consumer,
                hard_edge("consumer", "provider", EdgeKind::AssemblyRef),
            );
            // provider after consumer (wrong ForceLoadAfter): graph consumer -> provider.
            graph.add_edge(
                consumer,
                provider,
                hard_edge("provider", "consumer", EdgeKind::ForceLoadAfter),
            );
            let mut mods_by_id = BTreeMap::new();
            let provider_mod = test_mod("provider", 0);
            let consumer_mod = test_mod("consumer", 5);
            mods_by_id.insert(ModId::new("provider"), &provider_mod);
            mods_by_id.insert(ModId::new("consumer"), &consumer_mod);

            let result = break_cycles(
                &mut graph,
                Layer::Hard,
                &BTreeSet::new(),
                &mods_by_id,
                &BTreeMap::new(),
                &no_current_order(),
            );

            assert_eq!(result.dropped.len(), 1);
            let EdgeProvenance::Engine { kind, .. } = &result.dropped[0].edge.provenance else {
                panic!("expected an engine edge");
            };
            assert_eq!(
                *kind,
                EdgeKind::AssemblyRef,
                "with provider (before of AssemblyRef) at fewer hard_dependents, \
                 AssemblyRef must be the one dropped, not unconditionally protected"
            );
        }

        // Reversed: `consumer_dependents == 0`, so `ForceLoadAfter`'s own
        // `before.hard_dependents` (`consumer`) is now the smaller one —
        // `ForceLoadAfter` must be dropped instead, `AssemblyRef` survives.
        // A layer-blind `kind_rank` could never produce this orientation.
        {
            let mut graph: SortGraph = StableDiGraph::new();
            let consumer = graph.add_node(Node::Mod(ModId::new("consumer")));
            let provider = graph.add_node(Node::Mod(ModId::new("provider")));
            graph.add_edge(
                provider,
                consumer,
                hard_edge("consumer", "provider", EdgeKind::AssemblyRef),
            );
            graph.add_edge(
                consumer,
                provider,
                hard_edge("provider", "consumer", EdgeKind::ForceLoadAfter),
            );
            let mut mods_by_id = BTreeMap::new();
            let provider_mod = test_mod("provider", 5);
            let consumer_mod = test_mod("consumer", 0);
            mods_by_id.insert(ModId::new("provider"), &provider_mod);
            mods_by_id.insert(ModId::new("consumer"), &consumer_mod);

            let result = break_cycles(
                &mut graph,
                Layer::Hard,
                &BTreeSet::new(),
                &mods_by_id,
                &BTreeMap::new(),
                &no_current_order(),
            );

            assert_eq!(result.dropped.len(), 1);
            let EdgeProvenance::Engine { kind, .. } = &result.dropped[0].edge.provenance else {
                panic!("expected an engine edge");
            };
            assert_eq!(
                *kind,
                EdgeKind::ForceLoadAfter,
                "with consumer (before of ForceLoadAfter) at fewer hard_dependents, \
                 ForceLoadAfter must be the one dropped — proving AssemblyRef is not \
                 unconditionally protected by kind_rank in a Layer::Hard cycle"
            );
        }
    }

    /// A dropped
    /// `Rule { origin: UserDecision }` edge must never vanish silently —
    /// it always produces a `SortWarning::UserDecisionOverruled`, naming
    /// the winner when the witness cycle is a direct two-mod
    /// contradiction, exactly like `DroppedEdge::winner` does.
    #[test]
    fn a_dropped_user_decision_edge_emits_a_warning_naming_the_winner() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a")));
        let b = graph.add_node(Node::Mod(ModId::new("b")));
        // Already accepted (an earlier layer): b must load after a.
        graph.add_edge(a, b, real_edge("b", "a", Layer::Declared));
        // The user's own decision contradicts it directly: a must load
        // after b — added at the UserDecision layer, so it's the only
        // drop candidate.
        graph.add_edge(
            b,
            a,
            GraphEdge {
                layer: Layer::UserDecision,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                    layer: Layer::UserDecision,
                    provenance: EdgeProvenance::Rule {
                        origin: RuleOrigin::UserDecision,
                        comment: None,
                    },
                }),
            },
        );

        let mods_by_id = BTreeMap::new();
        let result = break_cycles(
            &mut graph,
            Layer::UserDecision,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        let SortWarning::UserDecisionOverruled { edge, winner } = &result.warnings[0] else {
            panic!(
                "expected UserDecisionOverruled, got {:?}",
                result.warnings[0]
            );
        };
        assert_eq!(edge.after, ModId::new("a"));
        assert_eq!(edge.before, ModId::new("b"));
        let winner = winner
            .as_ref()
            .expect("a direct 2-cycle must name a winner");
        assert_eq!(winner.after, ModId::new("b"));
        assert_eq!(winner.before, ModId::new("a"));
    }

    /// A genuine 3-mod cycle (a -> b -> c -> a, no direct edge between any
    /// two of them) has no single edge to blame, so it names no winner
    /// (`winner: None`).
    #[test]
    fn a_three_cycle_names_no_winner() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a")));
        let b = graph.add_node(Node::Mod(ModId::new("b")));
        let c = graph.add_node(Node::Mod(ModId::new("c")));
        // a must load after b, b after c, c after a: a -> b -> c -> a.
        graph.add_edge(b, a, real_edge("a", "b", Layer::Declared));
        graph.add_edge(c, b, real_edge("b", "c", Layer::Declared));
        graph.add_edge(a, c, real_edge("c", "a", Layer::Declared));

        let mods_by_id = BTreeMap::new();
        let result = break_cycles(
            &mut graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        assert_eq!(
            result.dropped[0].winner, None,
            "a 3-cycle has no single edge to blame"
        );
    }

    /// `find_promotion_cause` must walk to the nearest placement-pinned
    /// mod, not settle for the lexicographically smallest `Real` edge
    /// touching the promoted mod directly. `a_promoted` has two direct
    /// edges — a decoy to `b_decoy` (alphabetically smaller, and leading
    /// nowhere near a placement) and one continuing on to `m_middle`,
    /// which itself connects to `z_pinned`, an explicit `Bottom` pin. A
    /// smallest-edge rule can never even see the `m_middle -> z_pinned`
    /// edge (it doesn't name `a_promoted` at all) and would report the
    /// decoy instead.
    #[test]
    fn find_promotion_cause_walks_to_the_nearest_placed_mod_not_the_smallest_edge() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a_promoted")));
        let b = graph.add_node(Node::Mod(ModId::new("b_decoy")));
        let m = graph.add_node(Node::Mod(ModId::new("m_middle")));
        let z = graph.add_node(Node::Mod(ModId::new("z_pinned")));

        graph.add_edge(a, b, real_edge("b_decoy", "a_promoted", Layer::Declared));
        graph.add_edge(a, m, real_edge("m_middle", "a_promoted", Layer::Declared));
        let placed_edge = real_edge("z_pinned", "m_middle", Layer::Declared);
        graph.add_edge(m, z, placed_edge.clone());
        // A throwaway edge purely to mint a distinct `EdgeIndex` for
        // `removed` (the already-dropped membership edge, irrelevant to
        // this function's own `Real`-only neighbor search).
        let removed = graph.add_edge(
            z,
            a,
            GraphEdge {
                layer: Layer::Hard,
                kind: GraphEdgeKind::Boundary,
            },
        );

        let scc_set: BTreeSet<NodeIndex> = [a, b, m, z].into_iter().collect();
        let tier_of = BTreeMap::from([(
            ModId::new("z_pinned"),
            (
                Tier::Bottom,
                TierReason::Placement(RuleOrigin::UserDecision),
            ),
        )]);

        let cause = find_promotion_cause(
            &graph,
            &scc_set,
            removed,
            &ModId::new("a_promoted"),
            a,
            &tier_of,
        );

        let GraphEdgeKind::Real(expected) = &placed_edge.kind else {
            unreachable!("placed_edge was built by this test's own real_edge helper")
        };
        assert_eq!(
            cause.as_ref(),
            Some(expected),
            "expected the edge adjacent to the placed mod (z_pinned), not \
             the lexicographically smallest edge (b_decoy)"
        );
    }

    /// A `Boundary` edge (the tier backbone itself) must never be the one
    /// dropped when a `Real`/`Membership` alternative exists in the same
    /// SCC — even though the tie-break's own `(hard_dependents, node,
    /// edge)` ordering has no opinion either way between them.
    #[test]
    fn boundary_edge_is_never_dropped_when_a_real_or_membership_alternative_exists() {
        use super::super::tiers::Tier;

        let mut graph: SortGraph = StableDiGraph::new();
        let tier_a_end = graph.add_node(Node::TierEnd(Tier::Core));
        let tier_b_start = graph.add_node(Node::TierStart(Tier::Dlc));
        let mod1 = graph.add_node(Node::Mod(ModId::new("mod1")));
        let mod2 = graph.add_node(Node::Mod(ModId::new("mod2")));

        let boundary_edge_id = graph.add_edge(
            tier_a_end,
            tier_b_start,
            GraphEdge {
                layer: Layer::Hard,
                kind: GraphEdgeKind::Boundary,
            },
        );
        graph.add_edge(
            tier_b_start,
            mod2,
            GraphEdge {
                layer: Layer::Hard,
                kind: GraphEdgeKind::Membership {
                    mod_id: ModId::new("mod2"),
                    tier: Tier::Dlc,
                },
            },
        );
        graph.add_edge(mod2, mod1, real_edge("mod1", "mod2", Layer::Hard));
        graph.add_edge(
            mod1,
            tier_a_end,
            GraphEdge {
                layer: Layer::Hard,
                kind: GraphEdgeKind::Membership {
                    mod_id: ModId::new("mod1"),
                    tier: Tier::Core,
                },
            },
        );
        // The four edges above form one cycle: tier_a_end -> tier_b_start
        // -> mod2 -> mod1 -> tier_a_end — mixing a Boundary edge with two
        // Membership edges and one Real edge, all in the same SCC.

        let mods_by_id = BTreeMap::new();
        let _ = break_cycles(
            &mut graph,
            Layer::Hard,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert!(
            graph.edge_weight(boundary_edge_id).is_some(),
            "the Boundary edge must survive while a Real/Membership alternative exists"
        );
    }

    #[test]
    fn kept_edge_is_never_the_one_dropped_when_an_alternative_exists() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a")));
        let b = graph.add_node(Node::Mod(ModId::new("b")));
        graph.add_edge(b, a, real_edge("a", "b", Layer::Declared));
        graph.add_edge(a, b, real_edge("b", "a", Layer::Declared));

        let kept: BTreeSet<(ModId, ModId, EdgeKind)> =
            [(ModId::new("a"), ModId::new("b"), EdgeKind::LoadAfter)]
                .into_iter()
                .collect();
        let mods_by_id = BTreeMap::new();

        let result = break_cycles(
            &mut graph,
            Layer::Declared,
            &kept,
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        assert_eq!(result.dropped[0].edge.after, ModId::new("b"));
    }

    /// When *every* edge in the cycle is `KeepEdge`-pinned, the graph
    /// still must not stay cyclic — a real cycle among Hard edges the
    /// mods are genuinely broken over. `pool` falls back to the full
    /// (still-pinned) candidate set rather than getting stuck with an
    /// empty `non_kept`, and the sorter still drops one edge (confidence
    /// 20 downstream in `ledger::suggest`, since a fully-pinned Hard cycle
    /// means the mods are genuinely in conflict).
    #[test]
    fn a_fully_pinned_scc_still_gets_broken() {
        let mut graph: SortGraph = StableDiGraph::new();
        let a = graph.add_node(Node::Mod(ModId::new("a")));
        let b = graph.add_node(Node::Mod(ModId::new("b")));
        graph.add_edge(b, a, real_edge("a", "b", Layer::Hard));
        graph.add_edge(a, b, real_edge("b", "a", Layer::Hard));

        let kept: BTreeSet<(ModId, ModId, EdgeKind)> = [
            (ModId::new("a"), ModId::new("b"), EdgeKind::LoadAfter),
            (ModId::new("b"), ModId::new("a"), EdgeKind::LoadAfter),
        ]
        .into_iter()
        .collect();
        let mods_by_id = BTreeMap::new();

        let result = break_cycles(
            &mut graph,
            Layer::Hard,
            &kept,
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(
            result.dropped.len(),
            1,
            "a fully-pinned cycle must still be broken, not left cyclic"
        );
        assert_eq!(graph.edge_count(), 1, "exactly one edge must survive");
    }

    /// Pins the `sccs` ordering in [`break_cycles`] (see its
    /// own doc comment): two vertex-disjoint non-trivial SCCs (`a<->b` and
    /// `y<->z`) are both broken in the same outer pass, so without that
    /// ordering the *order* the two drops land in `result.dropped` would
    /// track `tarjan_scc`'s own DFS-completion order rather than either
    /// SCC's content — exactly what
    /// `sort_is_invariant_under_input_edge_shuffling` exists to catch.
    /// Compares the two `dropped` vectors element-for-element (not as
    /// sets, which pass trivially — the drop *set* is never in question,
    /// only its order).
    ///
    /// **Deviation from a literal two-bare-2-cycles graph, disclosed**:
    /// four edges forming exactly `a<->b` and `y<->z` with no other
    /// connectivity can *never* exhibit this bug, for any node or edge
    /// order — provable directly from `tarjan_scc`'s own
    /// algorithm (`petgraph::algo::scc::tarjan_scc::TarjanScc::run`): its
    /// outer loop starts a fresh DFS from each unvisited node in ascending
    /// `NodeIndex` order, and every node here has out-degree exactly 1
    /// (one edge per cycle direction), so with no edge at all connecting
    /// the two components, *which* SCC is discovered and completed first
    /// is decided purely by which component contains the lower-indexed
    /// node — never by either component's own internal edge order, since
    /// there is nothing for that order to change (a single-neighbor DFS
    /// step has no alternative to reorder). Reproducing the real bug needs
    /// a node whose DFS visit reaches into *both* otherwise-disjoint
    /// components while both are still unvisited, so which one gets
    /// visited (and thus completed) first is a genuine, order-sensitive
    /// choice — here, `p`, a `Layer::Hard` node with two outgoing edges
    /// `p->a` and `p->y` (never a `Layer::Declared` candidate itself, so
    /// it never affects which edges `break_cycles` can choose to drop,
    /// only the traversal that decides `sccs`' own order). The whole
    /// six-edge list (the two bridge edges plus the four cycle edges) is
    /// reversed together, not just the four cycle edges — reversing only
    /// the cycle edges while holding `p`'s own two edges at a fixed
    /// position can't diverge either, by the same "nothing to reorder at
    /// a single-neighbor step" argument, since `p`'s own branch is what
    /// has to flip.
    #[test]
    fn two_disjoint_cycles_drop_in_the_same_order_regardless_of_edge_insertion_order() {
        fn build_graph(forward: bool) -> SortGraph {
            let mut graph: SortGraph = StableDiGraph::new();
            let p = graph.add_node(Node::Mod(ModId::new("p")));
            let a = graph.add_node(Node::Mod(ModId::new("a")));
            let b = graph.add_node(Node::Mod(ModId::new("b")));
            let y = graph.add_node(Node::Mod(ModId::new("y")));
            let z = graph.add_node(Node::Mod(ModId::new("z")));

            let edges = [
                (p, a, real_edge("a", "p", Layer::Hard)),
                (p, y, real_edge("y", "p", Layer::Hard)),
                (b, a, real_edge("a", "b", Layer::Declared)),
                (a, b, real_edge("b", "a", Layer::Declared)),
                (z, y, real_edge("y", "z", Layer::Declared)),
                (y, z, real_edge("z", "y", Layer::Declared)),
            ];
            if forward {
                for (source, target, weight) in edges {
                    graph.add_edge(source, target, weight);
                }
            } else {
                for (source, target, weight) in edges.into_iter().rev() {
                    graph.add_edge(source, target, weight);
                }
            }
            graph
        }

        let mods_by_id = BTreeMap::new();

        let mut forward_graph = build_graph(true);
        let forward_result = break_cycles(
            &mut forward_graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        let mut reversed_graph = build_graph(false);
        let reversed_result = break_cycles(
            &mut reversed_graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(
            forward_result.dropped.len(),
            2,
            "both disjoint 2-cycles must each drop exactly one edge"
        );
        assert_eq!(
            forward_result.dropped, reversed_result.dropped,
            "SortOutcome::dropped must not depend on the input edges' own \
             insertion order when two vertex-disjoint SCCs are broken in \
             the same pass"
        );
    }

    /// A deliberate design choice: within `Layer::Inferred`, a
    /// `PatchRemovedNodeCosmetic` edge is dropped before a contradicting
    /// `PatchRemovedNode` (content) edge, with `hard_dependents` tied on
    /// both sides so only `kind_rank` can decide it.
    ///
    /// What would make this fail: removing the `Layer::Inferred` branch
    /// from `kind_rank` (or ranking `PatchRemovedNodeCosmetic` the same
    /// as every other `Inferred` kind) — the drop would then fall back
    /// to the mods' own ids, which this fixture is named to get wrong.
    #[test]
    fn cosmetic_edge_is_dropped_before_a_content_inferred_edge_in_a_cycle() {
        fn inferred_edge(after: &str, before: &str, kind: EdgeKind) -> GraphEdge {
            GraphEdge {
                layer: Layer::Inferred,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new(after),
                    before: ModId::new(before),
                    layer: Layer::Inferred,
                    provenance: EdgeProvenance::Engine {
                        kind,
                        detail: String::new(),
                    },
                }),
            }
        }

        let mut graph: SortGraph = StableDiGraph::new();
        // Named so the `target_node` tie-break component (the one right
        // after `kind_rank`/`hard_dependents`) would favor dropping the
        // *content* edge if `kind_rank` didn't override it first —
        // `"aaa" < "zzz"`, so a layer-blind tie-break would drop the edge
        // pointing at "aaa", which is the cosmetic one; naming it this way
        // proves `kind_rank` is what actually decides this, not a
        // downstream tie-break agreeing by coincidence. The cosmetic
        // edge's own `before` is "zzz", the content edge's own `before` is
        // "aaa" — `kind_rank` must drop the "zzz" one despite that.
        let zzz = graph.add_node(Node::Mod(ModId::new("zzz")));
        let aaa = graph.add_node(Node::Mod(ModId::new("aaa")));
        // "aaa" after "zzz" (cosmetic): graph zzz -> aaa.
        graph.add_edge(
            zzz,
            aaa,
            inferred_edge("aaa", "zzz", EdgeKind::PatchRemovedNodeCosmetic),
        );
        // "zzz" after "aaa" (content), directly contradicting: graph aaa -> zzz.
        graph.add_edge(
            aaa,
            zzz,
            inferred_edge("zzz", "aaa", EdgeKind::PatchRemovedNode),
        );

        let mods_by_id = BTreeMap::new();
        let result = break_cycles(
            &mut graph,
            Layer::Inferred,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        let EdgeProvenance::Engine { kind, .. } = &result.dropped[0].edge.provenance else {
            panic!("expected an engine edge");
        };
        assert_eq!(
            *kind,
            EdgeKind::PatchRemovedNodeCosmetic,
            "the cosmetic edge must be the one dropped, not the content edge"
        );
    }

    /// `Layer::Declared`'s own kind-rank rule (`ModDependency` dropped
    /// first, `LoadAfter`/`LoadBefore`/`ForceLoadAfter`/`ForceLoadBefore`
    /// protected) is unaffected by the `Layer::Inferred` branch existing
    /// alongside it — re-runs
    /// `a_mod_dependency_edge_is_dropped_before_a_contradicting_load_after_edge`'s
    /// own fixture shape as a direct regression guard for that
    /// independence.
    #[test]
    fn declared_layer_tie_break_is_unchanged_by_the_inferred_layer_rule() {
        let mut graph: SortGraph = StableDiGraph::new();
        let aaa = graph.add_node(Node::Mod(ModId::new("aaa")));
        let zzz = graph.add_node(Node::Mod(ModId::new("zzz")));
        graph.add_edge(
            aaa,
            zzz,
            GraphEdge {
                layer: Layer::Declared,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new("zzz"),
                    before: ModId::new("aaa"),
                    layer: Layer::Declared,
                    provenance: EdgeProvenance::Engine {
                        kind: EdgeKind::ModDependency,
                        detail: String::new(),
                    },
                }),
            },
        );
        graph.add_edge(zzz, aaa, real_edge("aaa", "zzz", Layer::Declared));

        let mods_by_id = BTreeMap::new();
        let result = break_cycles(
            &mut graph,
            Layer::Declared,
            &BTreeSet::new(),
            &mods_by_id,
            &BTreeMap::new(),
            &no_current_order(),
        );

        assert_eq!(result.dropped.len(), 1);
        let EdgeProvenance::Engine { kind, .. } = &result.dropped[0].edge.provenance else {
            panic!("expected an engine edge");
        };
        assert_eq!(*kind, EdgeKind::ModDependency);
    }

    fn test_mod(id: &str, hard_dependents: usize) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            url: None,
            path: std::path::PathBuf::new(),
            source: rim_analyzer::domain::Source::Local,
            supported_versions: Vec::new(),
            declared: rim_analyzer::domain::DeclaredOrder::default(),
            loaded_folders: Vec::new(),
            hard_dependents,
            soft_dependents: 0,
            awareness_dependents: 0,
            is_framework_candidate: false,
            generated: None,
            workshop_id: None,
            load_folders_version_matched: None,
        }
    }
}
