//! Evaluates the analyzer's raw facts — engine edges, any-of constraints,
//! and def-override owner sets — against an arbitrary [`LoadOrder`].
//!
//! `rim_analyzer::domain::Report` already carries each edge's/constraint's
//! status, but only as evaluated against the order active at scan time.
//! The ledger needs the same facts re-evaluated against *either*
//! [`crate::domain::OrderSource`] — most importantly, "would the suggested
//! order fix this" — so every function here is a pure re-evaluation that
//! takes the order as a parameter instead of trusting a cached status.

use rim_analyzer::domain::{Constraint, ConstraintStatus, Edge, EdgeStatus, LoadOrder, ModId};

/// Whether `order` places `before` strictly ahead of `after` — the shape
/// every ordering constraint in this crate reduces to, regardless of
/// whether it came from an [`Edge`], a dropped sorter edge, or an
/// undeclared-hard-dependency fact.
#[must_use]
pub fn ordering_status(after: &ModId, before: &ModId, order: &LoadOrder) -> EdgeStatus {
    match (order.position(before), order.position(after)) {
        (Some(before_pos), Some(after_pos)) => {
            if before_pos < after_pos {
                EdgeStatus::Satisfied
            } else {
                EdgeStatus::Violated
            }
        }
        _ => EdgeStatus::Unevaluated,
    }
}

/// Whether `order` satisfies `edge`.
#[must_use]
pub fn edge_status(edge: &Edge, order: &LoadOrder) -> EdgeStatus {
    ordering_status(&edge.after, &edge.before, order)
}

/// Whether `order` satisfies an any-of constraint: at least one candidate
/// must load before `after`. Constraints have no "unevaluated" state (see
/// [`ConstraintStatus`]) — a constraint naming a mod absent from `order`
/// simply can't be satisfied by it.
#[must_use]
pub fn constraint_status(constraint: &Constraint, order: &LoadOrder) -> ConstraintStatus {
    let Constraint::AnyOf {
        after, candidates, ..
    } = constraint;
    let Some(after_pos) = order.position(after) else {
        return ConstraintStatus::Violated;
    };
    let satisfied = candidates
        .iter()
        .any(|candidate| order.position(candidate).is_some_and(|pos| pos < after_pos));
    if satisfied {
        ConstraintStatus::Satisfied
    } else {
        ConstraintStatus::Violated
    }
}

/// The owner that actually wins a `DefOverride`/`PatchCollision`-style
/// contest under `order`: the last owner to load, among those `order`
/// actually places. `None` when none of `owners` appears in `order`.
#[must_use]
pub fn def_override_winner(owners: &[ModId], order: &LoadOrder) -> Option<ModId> {
    owners
        .iter()
        .filter(|id| order.position(id).is_some())
        .max_by_key(|id| order.position(id))
        .cloned()
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::EdgeKind;

    use super::*;

    fn order(ids: &[&str]) -> LoadOrder {
        LoadOrder::new(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn ordering_status_is_satisfied_when_before_loads_first() {
        let order = order(&["framework", "addon"]);
        assert_eq!(
            ordering_status(&ModId::new("addon"), &ModId::new("framework"), &order),
            EdgeStatus::Satisfied
        );
    }

    #[test]
    fn ordering_status_is_violated_when_before_loads_after() {
        let order = order(&["addon", "framework"]);
        assert_eq!(
            ordering_status(&ModId::new("addon"), &ModId::new("framework"), &order),
            EdgeStatus::Violated
        );
    }

    #[test]
    fn ordering_status_is_unevaluated_when_either_side_is_absent() {
        let order = order(&["addon"]);
        assert_eq!(
            ordering_status(&ModId::new("addon"), &ModId::new("framework"), &order),
            EdgeStatus::Unevaluated
        );
    }

    #[test]
    fn edge_status_delegates_to_ordering_status() {
        let order = order(&["framework", "addon"]);
        let edge = Edge {
            after: ModId::new("addon"),
            before: ModId::new("framework"),
            kind: EdgeKind::LoadAfter,
            detail: String::new(),
            load_time: true,
            subject: None,
        };
        assert_eq!(edge_status(&edge, &order), EdgeStatus::Satisfied);
    }

    #[test]
    fn constraint_status_is_satisfied_by_any_one_candidate() {
        let order = order(&["candidate.b", "dependent"]);
        let constraint = Constraint::AnyOf {
            after: ModId::new("dependent"),
            assembly: "shared.dll".to_string(),
            candidates: vec![ModId::new("candidate.a"), ModId::new("candidate.b")],
            load_time: true,
            status: ConstraintStatus::Violated,
        };
        assert_eq!(
            constraint_status(&constraint, &order),
            ConstraintStatus::Satisfied
        );
    }

    #[test]
    fn constraint_status_is_violated_when_after_is_absent() {
        let order = order(&["candidate.a"]);
        let constraint = Constraint::AnyOf {
            after: ModId::new("dependent"),
            assembly: "shared.dll".to_string(),
            candidates: vec![ModId::new("candidate.a")],
            load_time: true,
            status: ConstraintStatus::Satisfied,
        };
        assert_eq!(
            constraint_status(&constraint, &order),
            ConstraintStatus::Violated
        );
    }

    #[test]
    fn def_override_winner_is_the_last_owner_in_order() {
        let order = order(&["b", "a", "c"]);
        let winner =
            def_override_winner(&[ModId::new("a"), ModId::new("b"), ModId::new("c")], &order);
        assert_eq!(winner, Some(ModId::new("c")));
    }

    #[test]
    fn def_override_winner_ignores_owners_absent_from_order() {
        let order = order(&["a"]);
        let winner = def_override_winner(&[ModId::new("a"), ModId::new("ghost")], &order);
        assert_eq!(winner, Some(ModId::new("a")));
    }

    #[test]
    fn def_override_winner_is_none_when_no_owner_is_in_order() {
        let order = order(&["unrelated"]);
        let winner = def_override_winner(&[ModId::new("a"), ModId::new("b")], &order);
        assert_eq!(winner, None);
    }
}
