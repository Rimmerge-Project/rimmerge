//! The sorter's graph plumbing: node/edge representation, index
//! bookkeeping, and [`run`] — the orchestrator that wires every other
//! submodule together into one [`super::SortOutcome`].

use std::collections::BTreeMap;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use rim_analyzer::domain::{Mod, ModId, Report};

use super::tiers::Tier;
use super::{OrderingEdge, SortInput, SortOutcome};

/// One node in the sort graph: either a real mod, or one of the ten tier
/// sentinels (`T_start`/`T_end` per [`Tier`]).
///
/// Ordering matters only as a deterministic final tie-break (see
/// [`super::emit`]); it has no bearing on tier precedence, which lives in
/// [`Tier`]'s own `Ord`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Node {
    TierStart(Tier),
    TierEnd(Tier),
    Mod(ModId),
}

/// What one graph edge represents: a real, explanation-worthy constraint
/// between two mods, or internal tier-sentinel plumbing that never
/// surfaces as an [`OrderingEdge`] on its own (though *dropping* one can
/// still be observed — see [`super::explain::TierReason::PromotedBy`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum GraphEdgeKind {
    /// A genuine mod-to-mod constraint, reportable as-is.
    Real(OrderingEdge),
    /// Binds `mod_id` into `tier`'s start/end sentinel pair.
    Membership { mod_id: ModId, tier: Tier },
    /// Chains one tier's end sentinel to the next tier's start sentinel.
    Boundary,
}

/// A graph edge's metadata: which layer it belongs to (for layer-batched
/// cycle breaking) and what it represents. `Ord` (content-based, not
/// identity-based) lets `cycles::break_cycles` fall back to it as a final,
/// permutation-invariant cycle-drop tie-break when two candidate edges
/// share the same source/target/`hard_dependents` — e.g. two distinct
/// engine edges between the same mod pair — where node identity alone
/// can't distinguish them and falling back to graph iteration order would
/// make the result depend on input edge order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GraphEdge {
    pub layer: super::Layer,
    pub kind: GraphEdgeKind,
}

/// The sort graph itself. A `StableDiGraph` (rather than the plain
/// `Graph`) so edge removal during cycle breaking never invalidates
/// indices held elsewhere.
pub(super) type SortGraph = StableDiGraph<Node, GraphEdge>;

/// Every node's [`NodeIndex`], keyed for `O(log n)` lookup by [`ModId`] or
/// [`Tier`].
pub(super) struct Indices {
    pub mod_index: BTreeMap<ModId, NodeIndex>,
    pub tier_start: BTreeMap<Tier, NodeIndex>,
    pub tier_end: BTreeMap<Tier, NodeIndex>,
}

impl Indices {
    pub fn node_index_of(&self, node: &Node) -> NodeIndex {
        match node {
            Node::Mod(id) => self.mod_index[id],
            Node::TierStart(tier) => self.tier_start[tier],
            Node::TierEnd(tier) => self.tier_end[tier],
        }
    }
}

/// This node's `hard_dependents` count for tie-break purposes: a real
/// mod's own field, or `0` for a tier sentinel (never contested).
pub(super) fn hard_dependents_of(node: &Node, mods_by_id: &BTreeMap<ModId, &Mod>) -> usize {
    match node {
        Node::Mod(id) => mods_by_id.get(id).map_or(0, |m| m.hard_dependents),
        Node::TierStart(_) | Node::TierEnd(_) => 0,
    }
}

/// Builds `mods_by_id` from the report — the one place every submodule
/// gets a `&Mod` by id.
pub(super) fn index_mods(report: &Report) -> BTreeMap<ModId, &Mod> {
    report.mods.iter().map(|m| (m.id.clone(), m)).collect()
}

fn add_real_edges(graph: &mut SortGraph, indices: &Indices, edges: Vec<OrderingEdge>) {
    for edge in edges {
        let Some(&before_idx) = indices.mod_index.get(&edge.before) else {
            continue;
        };
        let Some(&after_idx) = indices.mod_index.get(&edge.after) else {
            continue;
        };
        graph.add_edge(
            before_idx,
            after_idx,
            GraphEdge {
                layer: edge.layer,
                kind: GraphEdgeKind::Real(edge),
            },
        );
    }
}

/// The eleven layers, in the exact precedence order [`run`] adds them —
/// [`super::Layer::ALL`] directly, never a hand-maintained duplicate of
/// the enum's variant list with nothing to catch a variant missing from
/// it.
const LAYER_ORDER: [super::Layer; 11] = super::Layer::ALL;

pub(super) fn run(input: &SortInput<'_>) -> SortOutcome {
    use rim_analyzer::domain::EdgeStrength;

    use super::{Layer, cycles, layers};
    use crate::domain::RuleOrigin;

    let mods_by_id = index_mods(input.report);
    let active: std::collections::BTreeSet<ModId> = mods_by_id.keys().cloned().collect();
    let tier_of = super::tiers::assign(&mods_by_id, input.rules);

    let mut graph: SortGraph = StableDiGraph::new();
    let mut mod_index = BTreeMap::new();
    let mut tier_start = BTreeMap::new();
    let mut tier_end = BTreeMap::new();
    for tier in Tier::ALL {
        tier_start.insert(tier, graph.add_node(Node::TierStart(tier)));
        tier_end.insert(tier, graph.add_node(Node::TierEnd(tier)));
    }
    for id in &active {
        mod_index.insert(id.clone(), graph.add_node(Node::Mod(id.clone())));
    }
    let indices = Indices {
        mod_index,
        tier_start,
        tier_end,
    };

    let mut warnings = Vec::new();
    let mut dropped = Vec::new();
    let mut promotions = BTreeMap::new();
    let mut any_of_choices = Vec::new();
    let mut advisory_edges = Vec::new();

    for &layer in &LAYER_ORDER {
        super::tiers::add_sentinel_edges_for_layer(&mut graph, &indices, &tier_of, layer);

        match layer {
            Layer::Hard => {
                let edges = layers::engine_edges_of_strength(
                    input.report,
                    EdgeStrength::Hard,
                    layer,
                    input.overrides,
                );
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::AnyOf => {
                any_of_choices = super::any_of::resolve(&mut graph, &indices, input, &mods_by_id);
                continue; // any_of avoids cycles proactively; no batch break needed.
            }
            Layer::DeclaredOverride => {
                // The declared-edge override:
                // a `UserDecision`-origin `PairRule` explicitly flagged
                // `overrides_declared`. Added here, one layer ahead of
                // `Declared`, so it's already an accepted graph edge by
                // the time `Declared`'s own edges are added below — a
                // conflicting author `loadAfter`/`modDependencies` edge
                // is the one `break_cycles` drops, never this one (see
                // `Layer`'s own doc comment). An unflagged `UserDecision`
                // pair keeps adding its edge at `Layer::UserDecision`
                // (`layers::rule_pair_edges` excludes a flagged pair from
                // that layer so it is never added twice).
                let (edges, w) =
                    layers::declared_override_pair_edges(input.rules, &active, input.overrides);
                warnings.extend(w);
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::Declared => {
                let edges = layers::engine_edges_of_strength(
                    input.report,
                    EdgeStrength::Declared,
                    layer,
                    input.overrides,
                );
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::UserDecision => {
                // Two sources feed this layer: `overrides.reorders` (a
                // `Reorder`/`PreferWinner` decision) and any `PairRule`
                // stored directly at `RuleOrigin::UserDecision` in
                // `input.rules` — the shape a *promoted* imported rule
                // takes (`Session::promote_imported_rule`). Without the
                // second source, a promoted pair rule would sit in the
                // `RuleSet` unread by any pair-edge-adding code (only the
                // `RimSortUser`/`RimSortCommunity`/`SteamDb` layers call
                // `rule_pair_edges` otherwise), so "promote" would only work
                // for placements (`tiers::assign` reads
                // `rules.placements()` for any origin).
                let (mut edges, mut w) = layers::user_decision_edges(input.overrides, &active);
                let (rule_edges, rule_warnings) = layers::rule_pair_edges(
                    input.rules,
                    RuleOrigin::UserDecision,
                    layer,
                    &active,
                    input.overrides,
                );
                edges.extend(rule_edges);
                w.extend(rule_warnings);
                warnings.extend(w);
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::RimSortUser => {
                let (edges, w) = layers::rule_pair_edges(
                    input.rules,
                    RuleOrigin::RimSortUser,
                    layer,
                    &active,
                    input.overrides,
                );
                warnings.extend(w);
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::RimSortCommunity => {
                let (edges, w) = layers::rule_pair_edges(
                    input.rules,
                    RuleOrigin::RimSortCommunity,
                    layer,
                    &active,
                    input.overrides,
                );
                warnings.extend(w);
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::SteamDb => {
                let (edges, w) = layers::rule_pair_edges(
                    input.rules,
                    RuleOrigin::SteamDb,
                    layer,
                    &active,
                    input.overrides,
                );
                warnings.extend(w);
                add_real_edges(&mut graph, &indices, edges);
            }
            Layer::Soft => {
                let edges = layers::engine_edges_of_strength(
                    input.report,
                    EdgeStrength::Soft,
                    layer,
                    input.overrides,
                );
                if input.enforce.soft {
                    add_real_edges(&mut graph, &indices, edges);
                } else {
                    // Never added as a real constraint, but `Awareness`
                    // (the layer right after this one) is still `Body`'s
                    // own tier-membership layer (see `sort/tiers.rs`) —
                    // `add_sentinel_edges_for_layer` can add real new
                    // edges on a later iteration even when this one adds
                    // none, so falling through to `break_cycles` below,
                    // rather than a `continue`, must stay unconditional.
                    advisory_edges.extend(edges);
                }
            }
            Layer::Inferred => {
                // A real layer of its own, never folded into `Awareness`.
                // `PatchRemovedNode`/`RetextureAfterOwner`/
                // `DefOverrideAfterOrigin` — heuristic evidence, not an
                // author's own declaration, but still enforced by default
                // (`EnforcedLayers::inferred` defaults `true`, unlike
                // `soft`/`awareness`): see that field's own doc comment.
                let edges = layers::engine_edges_of_strength(
                    input.report,
                    EdgeStrength::Inferred,
                    layer,
                    input.overrides,
                );
                if input.enforce.inferred {
                    add_real_edges(&mut graph, &indices, edges);
                } else {
                    // See the comment on the `Soft` arm above:
                    // `add_sentinel_edges_for_layer` may still have added
                    // a real sentinel edge for this iteration even though
                    // no engine edge did — `break_cycles` below must still
                    // run regardless.
                    advisory_edges.extend(edges);
                }
            }
            Layer::Awareness => {
                let edges = layers::engine_edges_of_strength(
                    input.report,
                    EdgeStrength::Awareness,
                    layer,
                    input.overrides,
                );
                if input.enforce.awareness {
                    add_real_edges(&mut graph, &indices, edges);
                } else {
                    // See the comment on the `Soft` arm above: this is
                    // `Body`'s own tier-membership layer (the weakest
                    // one), so
                    // `add_sentinel_edges_for_layer` already added real
                    // new edges for this iteration even though no engine
                    // edge did — `break_cycles` below must still run.
                    advisory_edges.extend(edges);
                }
            }
        }

        let result = cycles::break_cycles(
            &mut graph,
            layer,
            &input.overrides.kept_edges,
            &mods_by_id,
            &tier_of,
            input.current,
        );
        dropped.extend(result.dropped);
        promotions.extend(result.promotions);
        warnings.extend(result.warnings);
    }

    let base_positions = base_positions_for(input, &mods_by_id, &active);
    let emission = super::emit::run(&graph, &indices, &base_positions, &tier_of, input.tie_break);

    // Missing mods the user chose to keep are reinserted *before* building
    // placement explanations, and explanations are built against this
    // final order — not `emission.order` — so `PlacementExplanation.position`
    // always agrees with `outcome.order.position(id)`: reinserting a
    // missing mod shifts every later mod's index, and an explanation built
    // against the pre-reinsertion order would report a position that no
    // longer matches where the mod actually ended up.
    let final_order = rim_analyzer::domain::LoadOrder::new(reinsert_kept_missing_mods(
        emission.order.clone(),
        input,
    ));

    let placements = super::explain::build(
        &graph,
        &indices,
        &tier_of,
        &promotions,
        &dropped,
        &advisory_edges,
        &emission,
        input.current,
        &final_order,
    );

    let stats = super::disturbance::compute(&final_order, input.current);

    SortOutcome {
        order: final_order,
        placements,
        dropped,
        any_of_choices,
        warnings,
        stats,
    }
}

/// Every active mod's pre-propagation base key, resolved for whichever
/// [`super::TieBreak`] mode `input` names — every id in `active` gets an
/// entry in both modes, so [`super::emit::run`]'s own lookup never needs
/// an "absent from the active set" fallback:
///
/// - [`super::TieBreak::PreserveCurrent`]: the mod's own position in
///   `input.current`, or `active.len()` (past every mod that does have
///   one) for a mod `current` doesn't list at all.
/// - [`super::TieBreak::Rebuild`]: the mod's rank when `active` is sorted
///   by [`normalized_rebuild_name`] (case-insensitive, with a leading
///   bracketed/parenthesised tag
///   or version prefix stripped first so `[CF] Vanilla Expanded` ranks
///   with the rest of the Vanilla Expanded family instead of alongside
///   every other `[...]`-prefixed pack), then by [`ModId`] as the
///   tie-break for two mods sharing a normalized name — `input.current`
///   plays no part in this mode's base key at all.
fn base_positions_for(
    input: &SortInput<'_>,
    mods_by_id: &BTreeMap<ModId, &Mod>,
    active: &std::collections::BTreeSet<ModId>,
) -> BTreeMap<ModId, usize> {
    match input.tie_break {
        super::TieBreak::PreserveCurrent => {
            let last = active.len();
            active
                .iter()
                .map(|id| (id.clone(), input.current.position(id).unwrap_or(last)))
                .collect()
        }
        super::TieBreak::Rebuild => {
            // `sort_by_cached_key`, not `sort_by_key`:
            // the key allocates a `String` (`normalized_rebuild_name`)
            // plus a `ModId` clone — `sort_by_key` recomputes it on every
            // comparison the sort makes (~10k allocations on a 1000-mod
            // list), `sort_by_cached_key` computes each element's key
            // exactly once.
            let mut ordered: Vec<&ModId> = active.iter().collect();
            ordered.sort_by_cached_key(|id| {
                let name = mods_by_id
                    .get(*id)
                    .map_or_else(String::new, |m| normalized_rebuild_name(&m.name));
                (name, (*id).clone())
            });
            ordered
                .into_iter()
                .enumerate()
                .map(|(rank, id)| (id.clone(), rank))
                .collect()
        }
    }
}

/// The `Rebuild` name-ranking normalization (leading tags only): strips
/// a leading bracketed (`[...]`) or parenthesised (`(...)`) tag — a
/// version prefix (`[1.6] `), a curator tag (`[CF] `), a category note
/// (`(Retexture) `) — repeatedly, so more than one leading tag (`[CF]
/// [1.6] Pack`) is fully stripped, then collapses internal whitespace runs
/// to one space and lowercases. **Leading only, deliberately**: a
/// *trailing* tag (`(Continued)`, `[1.5 - 1.6]`) is left alone, since it
/// never changes which mods a leading-tag strip would otherwise separate
/// from same-prefix siblings. Matching by leading character alone (never a
/// full bracket-tag
/// vocabulary) is deliberately permissive: a name that merely *starts*
/// with `(`/`[` for some other reason strips the same way a real tag
/// would, which is harmless here (the stripped text still sorts somewhere
/// reasonable) and keeps this dependency-free rather than pulling in a
/// regex crate for a handful of fixed patterns.
fn normalized_rebuild_name(name: &str) -> String {
    let mut rest = name.trim_start();
    loop {
        let mut chars = rest.chars();
        let Some(opening) = chars.next() else {
            break;
        };
        let closing = match opening {
            '[' => ']',
            '(' => ')',
            _ => break,
        };
        let Some(closing_byte_index) = chars.as_str().find(closing) else {
            break;
        };
        // `chars.as_str()` starts right after `opening`, so the found
        // index is relative to that slice; adding `opening`'s own byte
        // length plus the closing char's own byte length lands just past
        // the whole tag.
        let after_tag = opening.len_utf8() + closing_byte_index + closing.len_utf8();
        rest = rest[after_tag..].trim_start();
    }
    rest.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `missing_mods` never
/// enter the graph at all. A mod the user `Ignore`d on its `MissingMod`
/// finding (`overrides.kept_missing_mods`) is re-inserted right after
/// whichever mod preceded it in the *current* order — the nearest active
/// mod still emitted, walking backward past any other missing mod. Every
/// other missing mod is simply omitted from the suggested order.
fn reinsert_kept_missing_mods(mut order: Vec<ModId>, input: &SortInput<'_>) -> Vec<ModId> {
    for missing_id in &input.report.missing_mods {
        if !input.overrides.kept_missing_mods.contains(missing_id) {
            continue;
        }
        let Some(current_pos) = input.current.position(missing_id) else {
            // Not even in the current order (nothing to anchor to);
            // appending keeps every kept id present somewhere, which
            // matters more than exactly where.
            order.push(missing_id.clone());
            continue;
        };
        let preceding_active = input.current.as_slice()[..current_pos]
            .iter()
            .rev()
            .find(|id| order.contains(id));
        match preceding_active.and_then(|prev| order.iter().position(|id| id == prev)) {
            Some(insert_after) => order.insert(insert_after + 1, missing_id.clone()),
            None => order.insert(0, missing_id.clone()),
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::normalized_rebuild_name;
    use crate::sort::Layer;

    /// `LAYER_ORDER` (and
    /// `Layer::ALL`, its single source of truth) is a hand-maintained
    /// array — nothing stops a new variant compiling fine while simply
    /// missing from it, silently dropping every edge of that layer, not
    /// even advisory (the `match` driving `run`'s own layer loop is
    /// exhaustive over `Layer`, so a *new* variant is caught there, but
    /// that match reads `LAYER_ORDER`'s own entries, not the enum
    /// directly, so it can never catch an entry *missing* from the array
    /// in the first place). This closes the gap two ways: the inner
    /// `match` is exhaustive over `Layer` itself (a variant `Layer::ALL`
    /// doesn't even declare fails to compile here), and the length/
    /// duplicate check below catches a variant declared but left out of
    /// `Layer::ALL`.
    ///
    /// What would make this fail: removing a variant from `Layer::ALL`
    /// while leaving it in the enum (compiles, `seen.len()` mismatches);
    /// adding a new `Layer` variant without adding a matching arm here
    /// (compile error, by construction).
    #[test]
    fn layer_all_contains_every_variant_exactly_once() {
        fn assert_every_variant_is_named(layer: Layer) {
            match layer {
                Layer::Hard
                | Layer::AnyOf
                | Layer::DeclaredOverride
                | Layer::Declared
                | Layer::UserDecision
                | Layer::RimSortUser
                | Layer::RimSortCommunity
                | Layer::SteamDb
                | Layer::Inferred
                | Layer::Soft
                | Layer::Awareness => {}
            }
        }

        let mut seen = std::collections::BTreeSet::new();
        for layer in Layer::ALL {
            assert_every_variant_is_named(layer);
            assert!(
                seen.insert(layer),
                "Layer::ALL lists {layer:?} more than once"
            );
        }
        assert_eq!(
            seen.len(),
            11,
            "Layer::ALL must list every Layer variant exactly once — if this \
             fails after adding a variant, Layer::ALL (and this count) need \
             updating too"
        );
    }

    /// One strip case per
    /// leading-tag shape, plus the trailing-tag non-case —
    /// changing any one row's expected output (or making the function a
    /// no-op) fails this test.
    #[test]
    fn normalized_rebuild_name_strips_leading_tags_but_not_trailing_ones() {
        let cases = [
            ("[CF] Vanilla Expanded", "vanilla expanded"),
            ("[1.6] Vanilla Expanded", "vanilla expanded"),
            ("[1001] Vanilla Expanded", "vanilla expanded"),
            ("(Retexture) Vanilla Expanded", "vanilla expanded"),
            // More than one leading tag: both are stripped.
            ("[CF] [1.6] Vanilla Expanded", "vanilla expanded"),
            // A trailing tag is never touched.
            (
                "Vanilla Expanded (Continued)",
                "vanilla expanded (continued)",
            ),
            (
                "Vanilla Expanded [1.5 - 1.6]",
                "vanilla expanded [1.5 - 1.6]",
            ),
            // Internal whitespace collapses; casing folds regardless of
            // whether a tag was stripped at all.
            ("  Some   Mod  Name  ", "some mod name"),
            ("No Tag Here", "no tag here"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalized_rebuild_name(input), expected, "input: {input:?}");
        }
    }
}
