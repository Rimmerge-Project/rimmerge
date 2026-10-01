//! Placement explanations:
//! for every mod, why it ended up in its tier, what made it ready, and
//! which edges bounded it from above and below.

use std::collections::BTreeMap;

use petgraph::Direction;
use rim_analyzer::domain::{EdgeStrength, LoadOrder, ModId, Source};

use super::emit::Emission;
use super::graph::{GraphEdgeKind, Indices, SortGraph};
use super::tiers::Tier;
use super::{DroppedEdge, Layer, OrderingEdge, RuleOrigin};

/// Why a mod sits in its assigned [`Tier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierReason {
    /// `Source::Core`/`Source::Dlc`.
    Source(Source),
    /// A [`crate::domain::PlacementRule`] pinned it, from this origin.
    Placement(RuleOrigin),
    /// A stronger edge beat this mod's own tier sentinel, pulling it out
    /// of its nominal tier — the edge named here is the one that won.
    PromotedBy(OrderingEdge),
    /// No source or placement signal applied — the default. Covers every
    /// mod that isn't `Core`/`Dlc`/pinned, including one flagged
    /// `Mod::is_framework_candidate` — that heuristic never influences
    /// tier assignment (see `sort/tiers.rs`'s module doc), only
    /// `sort/emit.rs`'s propagated key can still earn it an early
    /// position, on the strength of real edges.
    Body,
}

/// The tie-break inputs that decided this mod's exact position among
/// everything else ready at the same time. Not to be confused with
/// [`super::TieBreak`], the *mode* (`PreserveCurrent`/`Rebuild`) that
/// decides what `effective_key`'s own base value means in the first
/// place — this struct is the per-mod diagnostic output either mode
/// produces, named separately to keep the two distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementTieBreak {
    /// This mod's position in the *current* order, if it has one.
    pub current_position: Option<usize>,
    /// The key actually used to schedule this mod in the emission heap:
    /// `current_position` (or, absent one, a value past every present
    /// mod), propagated through real accepted edges so a violated edge is
    /// resolved by moving whichever side moves the smaller total distance
    /// — a late prerequisite pulled forward to just before its earliest
    /// dependent, or an early dependent pushed back past its prerequisite
    /// — see `sort/direction.rs` for the cost comparison and
    /// `sort/emit.rs`'s
    /// `pull_keys`/`push_keys` for how it's applied. Equal to
    /// `current_position` whenever nothing pulled or pushed this mod.
    pub effective_key: usize,
    /// The accepted outgoing edge whose target's own key is what set
    /// `effective_key` below this mod's own current position — `None`
    /// when `effective_key` came from the mod's own base key unchanged,
    /// even if that still differs from `current_position` (this is
    /// exactly what happens to a *pushed* mod — its `effective_key` moves
    /// later than `current_position`, but this field stays `None`, since
    /// a push has no single "which accepted edge is responsible" answer
    /// the way a pull's minimum does — see `sort/emit.rs`'s `push_keys`
    /// for why).
    pub pulled_forward_by: Option<OrderingEdge>,
    /// How many other mods were emitted between `became_ready_after`'s
    /// source and this mod.
    pub mods_preferred_ahead: usize,
}

/// One advisory engine edge touching a mod: a `Soft`/`Awareness`-strength
/// edge [`super::EnforcedLayers`] excluded from the graph entirely, kept
/// here purely so the why-panel can still say "would prefer after X
/// (FindMod)" — and whether the emitted order happens to satisfy it
/// anyway, even though nothing forced it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryEdge {
    /// The excluded edge.
    pub edge: OrderingEdge,
    /// Whether `edge.before` happens to load before `edge.after` in the
    /// emitted order regardless.
    pub satisfied: bool,
    /// `Soft` (lazily-resolved `AssemblyRef`) or `Awareness` — which
    /// strength excluded this edge, so the UI can show lazy references and
    /// awareness hints as separate categories instead of one flat list.
    pub strength: EdgeStrength,
}

/// Why one mod ended up exactly where it did in the emitted order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementExplanation {
    /// The explained mod.
    pub mod_id: ModId,
    /// Its zero-based position in the emitted order.
    pub position: usize,
    /// Its position in the current order, if it has one.
    pub previous_position: Option<usize>,
    /// The tier it was assigned to.
    pub tier: Tier,
    /// Why it's in that tier.
    pub tier_reason: TierReason,
    /// The accepted incoming edge whose source was emitted most recently
    /// before this mod — the edge that actually made it ready.
    pub became_ready_after: Option<OrderingEdge>,
    /// Every accepted incoming edge, sorted by layer (`Hard` first).
    pub lower_bounds: Vec<OrderingEdge>,
    /// Every accepted outgoing edge, sorted by layer (`Hard` first).
    pub upper_bounds: Vec<OrderingEdge>,
    /// Every dropped edge touching this mod.
    pub dropped: Vec<DroppedEdge>,
    /// Every advisory (not-enforced) engine edge touching this mod — see
    /// [`super::EnforcedLayers`].
    pub advisory: Vec<AdvisoryEdge>,
    /// The tie-break inputs behind its exact position.
    pub tie_break: PlacementTieBreak,
}

#[allow(clippy::too_many_arguments)] // internal orchestration function; splitting further would just move the same facts into a struct nobody else needs
pub(super) fn build(
    graph: &SortGraph,
    indices: &Indices,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    promotions: &BTreeMap<ModId, OrderingEdge>,
    dropped: &[DroppedEdge],
    advisory_edges: &[OrderingEdge],
    emission: &Emission,
    current: &LoadOrder,
    final_order: &LoadOrder,
) -> BTreeMap<ModId, PlacementExplanation> {
    // Graph-relative positions: used only to work out relative emission
    // order among graph nodes (`became_ready_after`, `mods_preferred_ahead`,
    // whether an advisory edge happens to be satisfied) — a missing mod
    // reinserted into `final_order` was never a graph node, so it can't
    // shift any of that. The publicly reported `position` field uses
    // `final_order` instead (see below), since that's the order the
    // reinsertion actually produced.
    let position_in_order: BTreeMap<ModId, usize> = emission
        .order
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index))
        .collect();

    let mut placements = BTreeMap::new();

    for id in &emission.order {
        let Some(&node_idx) = indices.mod_index.get(id) else {
            continue;
        };
        let (tier, base_reason) = tier_of
            .get(id)
            .cloned()
            .unwrap_or((Tier::Body, TierReason::Body));
        let tier_reason = promotions
            .get(id)
            .map(|edge| TierReason::PromotedBy(edge.clone()))
            .unwrap_or(base_reason);

        // Sorted by `(layer, edge)` — layer first ("Hard first"
        // ordering), then the edge's own full content as a
        // deterministic tie-break for two same-layer edges: sorting by
        // `layer` alone leaves same-layer edges in whatever order
        // `graph.edges_directed` happens to iterate them, which depends on
        // insertion order and hence on the *input* edges' own order —
        // exactly the kind of permutation-dependence
        // `sort_proptest::sort_is_invariant_under_input_edge_shuffling`
        // checks for on the whole `SortOutcome`.
        let mut lower_bounds: Vec<OrderingEdge> = graph
            .edges_directed(node_idx, Direction::Incoming)
            .filter_map(|edge| match &edge.weight().kind {
                GraphEdgeKind::Real(ordering_edge) => Some(ordering_edge.clone()),
                _ => None,
            })
            .collect();
        lower_bounds.sort_by(|a, b| (a.layer, a).cmp(&(b.layer, b)));

        let mut upper_bounds: Vec<OrderingEdge> = graph
            .edges_directed(node_idx, Direction::Outgoing)
            .filter_map(|edge| match &edge.weight().kind {
                GraphEdgeKind::Real(ordering_edge) => Some(ordering_edge.clone()),
                _ => None,
            })
            .collect();
        upper_bounds.sort_by(|a, b| (a.layer, a).cmp(&(b.layer, b)));

        let became_ready_after = lower_bounds
            .iter()
            .filter_map(|edge| {
                position_in_order
                    .get(&edge.before)
                    .map(|&pos| (pos, edge.clone()))
            })
            .max_by_key(|(pos, _)| *pos)
            .map(|(_, edge)| edge);

        let graph_position = position_in_order[id];
        let mods_preferred_ahead = became_ready_after
            .as_ref()
            .and_then(|edge| position_in_order.get(&edge.before))
            .map_or(0, |&source_pos| {
                graph_position.saturating_sub(source_pos + 1)
            });
        // The publicly reported position: `final_order`'s, so it always
        // agrees with `outcome.order.position(id)` even after missing-mod
        // reinsertion shifted every later index. `id` came from
        // `emission.order`, a subset of `final_order`'s own mods, so this
        // is always `Some` — `graph_position` is the only defensive
        // fallback that could ever apply.
        let position = final_order.position(id).unwrap_or(graph_position);

        let mod_dropped: Vec<DroppedEdge> = dropped
            .iter()
            .filter(|d| d.edge.after == *id || d.edge.before == *id)
            .cloned()
            .collect();

        let mod_advisory: Vec<AdvisoryEdge> = advisory_edges
            .iter()
            .filter(|edge| edge.after == *id || edge.before == *id)
            .map(|edge| {
                let satisfied = match (
                    position_in_order.get(&edge.before),
                    position_in_order.get(&edge.after),
                ) {
                    (Some(before_pos), Some(after_pos)) => before_pos < after_pos,
                    _ => false,
                };
                // `edge.layer` is `Layer::Soft`, `Layer::Inferred`, or
                // `Layer::Awareness` by construction (see
                // `sort/graph.rs::run`: `advisory_edges` is only ever
                // extended from one of those three layers, when its own
                // `EnforcedLayers` toggle is off) — read the strength
                // straight off it rather than
                // re-deriving from `EdgeKind::strength()`, which can't
                // tell a lazy `AssemblyRef` (`Soft`) from a load-time one
                // (`Hard`) on its own (that distinction lives in `Edge`'s
                // own `load_time` flag, not carried by `OrderingEdge`).
                let strength = match edge.layer {
                    Layer::Soft => EdgeStrength::Soft,
                    // Never read every non-`Soft` advisory edge back as
                    // `Awareness` — a `Layer::Inferred`
                    // edge (`PatchRemovedNode`/`RetextureAfterOwner`/
                    // `DefOverrideAfterOrigin`) must report its own real
                    // strength, not borrow `Awareness`'s.
                    Layer::Inferred => EdgeStrength::Inferred,
                    // Any other layer here would mean `advisory_edges` grew
                    // a new source without updating this match — not a
                    // real input to guard against today.
                    _ => EdgeStrength::Awareness,
                };
                AdvisoryEdge {
                    edge: edge.clone(),
                    satisfied,
                    strength,
                }
            })
            .collect();

        let effective_key = emission
            .effective_keys
            .get(id)
            .map_or_else(|| current.position(id).unwrap_or(0), |k| k.key);
        let pulled_forward_by = emission
            .effective_keys
            .get(id)
            .and_then(|k| k.pulled_forward_by.clone());

        placements.insert(
            id.clone(),
            PlacementExplanation {
                mod_id: id.clone(),
                position,
                previous_position: current.position(id),
                tier,
                tier_reason,
                became_ready_after,
                lower_bounds,
                upper_bounds,
                dropped: mod_dropped,
                advisory: mod_advisory,
                tie_break: PlacementTieBreak {
                    current_position: current.position(id),
                    effective_key,
                    pulled_forward_by,
                    mods_preferred_ahead,
                },
            },
        );
    }

    placements
}
