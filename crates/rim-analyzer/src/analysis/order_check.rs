//! Evaluates every [`Edge`] against the current [`LoadOrder`] and flags
//! `AssemblyRef` edges that carry no matching declared edge.

use std::collections::HashSet;

use crate::domain::{Edge, EdgeKind, EdgeReport, EdgeStatus, EdgeStrength, LoadOrder, ModId};

/// Pairs every edge with whether the current load order satisfies it.
/// An edge with an endpoint outside the load order (which shouldn't
/// normally happen — edges are only built between active mods — but
/// isn't an invariant worth panicking over) evaluates to
/// [`EdgeStatus::Unevaluated`] rather than crashing the whole run.
#[must_use]
pub fn evaluate(edges: Vec<Edge>, load_order: &LoadOrder) -> Vec<EdgeReport> {
    edges
        .into_iter()
        .map(|edge| {
            let status = match load_order.is_before(&edge.before, &edge.after) {
                Some(true) => EdgeStatus::Satisfied,
                Some(false) => EdgeStatus::Violated,
                None => EdgeStatus::Unevaluated,
            };
            EdgeReport { edge, status }
        })
        .collect()
}

/// `AssemblyRef` edges with no `Declared`-strength edge in the same
/// direction between the same two mods — a hard runtime dependency the
/// author never wrote down. An `AssemblyRef` edge downgraded to
/// `Awareness` strength by an ambiguous owner (see [`Edge::strength`])
/// is excluded: which mod actually satisfies it isn't certain, so it
/// isn't a hard dependency to declare in the first place.
#[must_use]
pub fn undeclared_hard_dependencies(edge_reports: &[EdgeReport]) -> Vec<Edge> {
    let declared_pairs: HashSet<(&ModId, &ModId)> = edge_reports
        .iter()
        .filter(|r| r.edge.strength() == EdgeStrength::Declared)
        .map(|r| (&r.edge.after, &r.edge.before))
        .collect();

    edge_reports
        .iter()
        .filter(|r| r.edge.kind == EdgeKind::AssemblyRef && r.edge.strength() == EdgeStrength::Hard)
        .filter(|r| !declared_pairs.contains(&(&r.edge.after, &r.edge.before)))
        .map(|r| r.edge.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(after: &str, before: &str, kind: EdgeKind) -> Edge {
        Edge {
            after: ModId::new(after),
            before: ModId::new(before),
            kind,
            detail: String::new(),
            load_time: true,
            subject: None,
        }
    }

    #[test]
    fn satisfied_when_before_mod_loads_first() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let reports = evaluate(vec![edge("b", "a", EdgeKind::AssemblyRef)], &load_order);
        assert_eq!(reports[0].status, EdgeStatus::Satisfied);
    }

    #[test]
    fn violated_when_before_mod_loads_after() {
        let load_order = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);
        let reports = evaluate(vec![edge("b", "a", EdgeKind::AssemblyRef)], &load_order);
        assert_eq!(reports[0].status, EdgeStatus::Violated);
    }

    #[test]
    fn unevaluated_when_an_endpoint_is_not_in_the_load_order() {
        let load_order = LoadOrder::new(vec![ModId::new("a")]);
        let reports = evaluate(vec![edge("b", "a", EdgeKind::AssemblyRef)], &load_order);
        assert_eq!(reports[0].status, EdgeStatus::Unevaluated);
    }

    #[test]
    fn undeclared_excludes_pairs_with_a_matching_declared_edge() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let reports = evaluate(
            vec![
                edge("b", "a", EdgeKind::AssemblyRef),
                edge("b", "a", EdgeKind::LoadAfter),
            ],
            &load_order,
        );
        assert!(undeclared_hard_dependencies(&reports).is_empty());
    }

    /// `modDependencies` is `Declared`-strength (the author's own statement
    /// of requirement — see [`EdgeKind::strength`]), so a `ModDependency`
    /// edge in the same direction as a `Hard` `AssemblyRef` *does* exempt it
    /// from `undeclared_hard_dependencies`: the dependency isn't undeclared,
    /// it's declared via `modDependencies` rather than `loadAfter`.
    #[test]
    fn mod_dependency_counts_as_declared_for_the_undeclared_check() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let reports = evaluate(
            vec![
                edge("b", "a", EdgeKind::AssemblyRef),
                edge("b", "a", EdgeKind::ModDependency),
            ],
            &load_order,
        );
        assert!(undeclared_hard_dependencies(&reports).is_empty());
    }

    /// `MayRequire` stays `Awareness`-strength (a presence gate, not an
    /// ordering promise), so it must never count as the "declared" edge
    /// that would exempt an `AssemblyRef` from `undeclared_hard_dependencies`.
    #[test]
    fn awareness_edges_do_not_count_as_declared_for_the_undeclared_check() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let reports = evaluate(
            vec![
                edge("b", "a", EdgeKind::AssemblyRef),
                edge("b", "a", EdgeKind::MayRequire),
            ],
            &load_order,
        );
        assert_eq!(undeclared_hard_dependencies(&reports).len(), 1);
    }

    #[test]
    fn undeclared_includes_assembly_ref_with_no_declared_counterpart() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let reports = evaluate(vec![edge("b", "a", EdgeKind::AssemblyRef)], &load_order);
        let undeclared = undeclared_hard_dependencies(&reports);
        assert_eq!(undeclared.len(), 1);
        assert_eq!(undeclared[0].kind, EdgeKind::AssemblyRef);
    }

    /// A lazily-resolved (`Soft`-strength) `AssemblyRef` isn't a hard
    /// dependency to declare in the first place.
    #[test]
    fn undeclared_excludes_lazy_assembly_ref_edges() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let mut lazy = edge("b", "a", EdgeKind::AssemblyRef);
        lazy.load_time = false;
        let reports = evaluate(vec![lazy], &load_order);
        assert!(undeclared_hard_dependencies(&reports).is_empty());
    }

    #[test]
    fn undeclared_ignores_declared_edges_in_the_opposite_direction() {
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        // b needs a (AssemblyRef), but the only declared edge runs the other way.
        let reports = evaluate(
            vec![
                edge("b", "a", EdgeKind::AssemblyRef),
                edge("a", "b", EdgeKind::LoadAfter),
            ],
            &load_order,
        );
        assert_eq!(undeclared_hard_dependencies(&reports).len(), 1);
    }
}
