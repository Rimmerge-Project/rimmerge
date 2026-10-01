//! Any-of constraint resolution: for each [`Constraint::AnyOf`], either
//! the requirement is
//! already met by an existing path, or exactly one candidate is chosen and
//! wired in as a [`Layer::AnyOf`] edge — never both, and never an edge
//! that would close a cycle.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use petgraph::algo::DfsSpace;
use rim_analyzer::domain::{Constraint, Mod, ModId};

use super::graph::{GraphEdge, GraphEdgeKind, Indices, SortGraph};
use super::{AnyOfChoice, EdgeProvenance, Layer, OrderingEdge, SortInput};

/// Orders `candidates` by the preference [`resolve`] tries them in: the
/// user's own `ChooseCandidate` decision first; then whichever candidate
/// already loads before `after` in the current order, nearest first; then
/// highest `hard_dependents`; then smallest id.
fn order_candidates(
    candidates: &[ModId],
    after: &ModId,
    assembly: &str,
    input: &SortInput<'_>,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> Vec<ModId> {
    let chosen_override = input
        .overrides
        .chosen_candidates
        .get(&(after.clone(), assembly.to_string()));
    let after_position = input.current.position(after);

    let mut ordered = candidates.to_vec();
    ordered.sort_by_key(|candidate| {
        let is_override = chosen_override == Some(candidate);
        let position = input.current.position(candidate);
        let is_before_after = matches!((position, after_position),
            (Some(p), Some(ap)) if p < ap
        );
        let hard_dependents = mods_by_id.get(candidate).map_or(0, |m| m.hard_dependents);
        // Nearness only discriminates among candidates that *are* already
        // before `after`; candidates that aren't fall straight through to
        // the hard-dependents/id tie-break with a neutral value here.
        let nearness = if is_before_after {
            Reverse(position.unwrap_or(0))
        } else {
            Reverse(0)
        };
        (
            u8::from(!is_override),
            u8::from(!is_before_after),
            nearness,
            Reverse(hard_dependents),
            candidate.clone(),
        )
    });
    ordered
}

/// Resolves every any-of constraint in `input.report`, adding at most one
/// [`Layer::AnyOf`] edge per constraint directly to `graph`.
#[must_use]
pub(super) fn resolve(
    graph: &mut SortGraph,
    indices: &Indices,
    input: &SortInput<'_>,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> Vec<AnyOfChoice> {
    let mut choices = Vec::new();
    let mut space = DfsSpace::new(&*graph);

    for constraint in &input.report.constraints {
        let Constraint::AnyOf {
            after,
            assembly,
            candidates,
            ..
        } = constraint;

        let Some(&after_idx) = indices.mod_index.get(after) else {
            continue; // `after` isn't active; nothing to satisfy.
        };
        let active_candidates: Vec<ModId> = candidates
            .iter()
            .filter(|c| indices.mod_index.contains_key(*c))
            .cloned()
            .collect();
        if active_candidates.is_empty() {
            choices.push(AnyOfChoice {
                after: after.clone(),
                assembly: assembly.clone(),
                chosen: None,
                candidates: candidates.clone(),
            });
            continue;
        }

        let already_satisfied = active_candidates.iter().any(|candidate| {
            let idx = indices.mod_index[candidate];
            petgraph::algo::has_path_connecting(&*graph, idx, after_idx, Some(&mut space))
        });
        if already_satisfied {
            // Nothing to decide: no edge is added, and no `AnyOfChoice` is
            // recorded either — a recorded `chosen: None` should always
            // mean "no candidate worked" (the ledger's 0-confidence
            // finding), never "was already fine".
            continue;
        }

        let ordering = order_candidates(&active_candidates, after, assembly, input, mods_by_id);
        let mut chosen = None;
        for candidate in ordering {
            let candidate_idx = indices.mod_index[&candidate];
            let would_cycle = petgraph::algo::has_path_connecting(
                &*graph,
                after_idx,
                candidate_idx,
                Some(&mut space),
            );
            if would_cycle {
                continue;
            }
            graph.add_edge(
                candidate_idx,
                after_idx,
                GraphEdge {
                    layer: Layer::AnyOf,
                    kind: GraphEdgeKind::Real(OrderingEdge {
                        after: after.clone(),
                        before: candidate.clone(),
                        layer: Layer::AnyOf,
                        provenance: EdgeProvenance::AnyOf {
                            assembly: assembly.clone(),
                        },
                    }),
                },
            );
            chosen = Some(candidate);
            break;
        }

        choices.push(AnyOfChoice {
            after: after.clone(),
            assembly: assembly.clone(),
            chosen,
            candidates: active_candidates,
        });
    }

    choices
}

#[cfg(test)]
mod tests {
    use petgraph::stable_graph::StableDiGraph;
    use rim_analyzer::domain::LoadOrder;

    use super::*;
    use crate::domain::{RuleSet, SorterOverrides, Tagging};
    use crate::sort::graph::Node;
    use crate::test_support::ReportBuilder;

    fn build_indices(graph: &mut SortGraph, ids: &[&str]) -> Indices {
        let mut mod_index = BTreeMap::new();
        for id in ids {
            mod_index.insert(ModId::new(*id), graph.add_node(Node::Mod(ModId::new(*id))));
        }
        Indices {
            mod_index,
            tier_start: BTreeMap::new(),
            tier_end: BTreeMap::new(),
        }
    }

    #[test]
    fn picks_the_candidate_already_before_after_in_the_current_order() {
        let report = ReportBuilder::new()
            .mod_("dependent")
            .mod_("candidate.a")
            .mod_("candidate.b")
            .any_of(
                "dependent",
                "shared.dll",
                &["candidate.a", "candidate.b"],
                true,
            )
            .build();
        let mut graph: SortGraph = StableDiGraph::new();
        let indices = build_indices(&mut graph, &["dependent", "candidate.a", "candidate.b"]);
        let current = LoadOrder::new(vec![
            ModId::new("candidate.b"),
            ModId::new("dependent"),
            ModId::new("candidate.a"),
        ]);
        let rules = RuleSet::default();
        let tagging = Tagging::default();
        let overrides = SorterOverrides::default();
        let input = SortInput {
            report: &report,
            rules: &rules,
            tagging: &tagging,
            overrides: &overrides,
            current: &current,
            enforce: super::super::EnforcedLayers::default(),
            tie_break: super::super::TieBreak::PreserveCurrent,
        };
        let mods_by_id = super::super::graph::index_mods(&report);

        let choices = resolve(&mut graph, &indices, &input, &mods_by_id);

        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].chosen, Some(ModId::new("candidate.b")));
    }

    #[test]
    fn skips_a_candidate_that_would_close_a_cycle() {
        let report = ReportBuilder::new()
            .mod_("dependent")
            .mod_("candidate.a")
            .mod_("candidate.b")
            .declared_edge("candidate.a", "dependent") // candidate.a must load after dependent
            .any_of(
                "dependent",
                "shared.dll",
                &["candidate.a", "candidate.b"],
                true,
            )
            .build();
        let mut graph: SortGraph = StableDiGraph::new();
        let indices = build_indices(&mut graph, &["dependent", "candidate.a", "candidate.b"]);
        // candidate.a -> dependent already exists (dependent must load after candidate.a is false;
        // here it's the reverse: dependent must load before candidate.a).
        graph.add_edge(
            indices.mod_index[&ModId::new("dependent")],
            indices.mod_index[&ModId::new("candidate.a")],
            GraphEdge {
                layer: Layer::Declared,
                kind: GraphEdgeKind::Real(OrderingEdge {
                    after: ModId::new("candidate.a"),
                    before: ModId::new("dependent"),
                    layer: Layer::Declared,
                    provenance: EdgeProvenance::Engine {
                        kind: rim_analyzer::domain::EdgeKind::LoadAfter,
                        detail: String::new(),
                    },
                }),
            },
        );
        let current = LoadOrder::new(vec![
            ModId::new("dependent"),
            ModId::new("candidate.a"),
            ModId::new("candidate.b"),
        ]);
        let rules = RuleSet::default();
        let tagging = Tagging::default();
        let overrides = SorterOverrides::default();
        let input = SortInput {
            report: &report,
            rules: &rules,
            tagging: &tagging,
            overrides: &overrides,
            current: &current,
            enforce: super::super::EnforcedLayers::default(),
            tie_break: super::super::TieBreak::PreserveCurrent,
        };
        let mods_by_id = super::super::graph::index_mods(&report);

        let choices = resolve(&mut graph, &indices, &input, &mods_by_id);

        // candidate.a would close a cycle (dependent already precedes it),
        // so candidate.b must be chosen instead even though candidate.a
        // sits earlier in the current order.
        assert_eq!(choices[0].chosen, Some(ModId::new("candidate.b")));
    }

    /// When *every* candidate would close a cycle (each is already
    /// reachable from `after`), no edge is added and the choice records
    /// `chosen: None` — a genuine "stuck" constraint, not the "already
    /// satisfied, nothing to decide" case (which never pushes an
    /// `AnyOfChoice` at all — see `resolve`'s own doc comment on
    /// `already_satisfied`).
    #[test]
    fn chosen_is_none_when_every_candidate_would_close_a_cycle() {
        let report = ReportBuilder::new()
            .mod_("dependent")
            .mod_("candidate.a")
            .mod_("candidate.b")
            .any_of(
                "dependent",
                "shared.dll",
                &["candidate.a", "candidate.b"],
                true,
            )
            .build();
        let mut graph: SortGraph = StableDiGraph::new();
        let indices = build_indices(&mut graph, &["dependent", "candidate.a", "candidate.b"]);
        // dependent already precedes *both* candidates, so adding either
        // one as "candidate before dependent" would close a cycle.
        for candidate in ["candidate.a", "candidate.b"] {
            graph.add_edge(
                indices.mod_index[&ModId::new("dependent")],
                indices.mod_index[&ModId::new(candidate)],
                GraphEdge {
                    layer: Layer::Declared,
                    kind: GraphEdgeKind::Real(OrderingEdge {
                        after: ModId::new(candidate),
                        before: ModId::new("dependent"),
                        layer: Layer::Declared,
                        provenance: EdgeProvenance::Engine {
                            kind: rim_analyzer::domain::EdgeKind::LoadAfter,
                            detail: String::new(),
                        },
                    }),
                },
            );
        }
        let current = LoadOrder::new(vec![
            ModId::new("dependent"),
            ModId::new("candidate.a"),
            ModId::new("candidate.b"),
        ]);
        let rules = RuleSet::default();
        let tagging = Tagging::default();
        let overrides = SorterOverrides::default();
        let input = SortInput {
            report: &report,
            rules: &rules,
            tagging: &tagging,
            overrides: &overrides,
            current: &current,
            enforce: super::super::EnforcedLayers::default(),
            tie_break: super::super::TieBreak::PreserveCurrent,
        };
        let mods_by_id = super::super::graph::index_mods(&report);

        let choices = resolve(&mut graph, &indices, &input, &mods_by_id);

        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].chosen, None);
        assert_eq!(
            choices[0].candidates,
            vec![ModId::new("candidate.a"), ModId::new("candidate.b")]
        );
    }
}
