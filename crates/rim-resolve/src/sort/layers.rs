//! Builds the [`OrderingEdge`]s for every non-tier, non-any-of layer: the
//! analyzer's own engine edges (`Hard`/`Declared`/`Soft`/`Awareness`) and
//! the imported pair rules (`UserDecision`/`RimSortUser`/`RimSortCommunity`/
//! `SteamDb`).
//!
//! Every function here is pure: given the facts and the active set, it
//! returns the edges to add and the [`SortWarning`]s for anything it had
//! to skip (a rule naming an inactive mod).

use std::collections::BTreeSet;

use rim_analyzer::domain::{EdgeStrength, ModId, Report};

use super::{EdgeProvenance, Layer, OrderingEdge, SortWarning};
use crate::domain::{PairRule, RuleOrigin, RuleSet, SorterOverrides};

/// Finds the active mod whose [`ModId::base`] matches `target`'s, if any —
/// rule files (RimSort imports, user rules) commonly name a mod's bare id
/// even when the active copy carries a `_steam` suffix.
fn resolve_active(target: &ModId, active: &BTreeSet<ModId>) -> Option<ModId> {
    let base = target.base();
    active.iter().find(|id| id.base() == base).cloned()
}

/// Engine edges of exactly `strength`, skipping any pair the caller has
/// `DropEdge`-overridden. Both endpoints are always active already — the
/// analyzer only ever builds edges between active mods — so no warnings
/// are produced here.
pub(super) fn engine_edges_of_strength(
    report: &Report,
    strength: EdgeStrength,
    layer: Layer,
    overrides: &SorterOverrides,
) -> Vec<OrderingEdge> {
    report
        .edges
        .iter()
        .map(|report| &report.edge)
        .filter(|edge| edge.strength() == strength)
        .filter(|edge| {
            !overrides
                .dropped_edges
                .contains(&(edge.after.clone(), edge.before.clone(), edge.kind))
        })
        .map(|edge| OrderingEdge {
            after: edge.after.clone(),
            before: edge.before.clone(),
            layer,
            provenance: EdgeProvenance::Engine {
                kind: edge.kind,
                detail: edge.detail.clone(),
            },
        })
        .collect()
}

/// The user's own `Reorder`/`PreferWinner`-expanded pairs, as
/// [`Layer::UserDecision`] edges, resolved through [`ModId::base`] against
/// the active set exactly like [`rule_pair_edges`] — a decision persists
/// (the rerere scheme) past whatever prompted it, so by the time it's
/// re-applied the named mod could carry a different `_steam` suffix than
/// it did when the decision was made, or no longer be active at all. A
/// pair naming a mod that isn't active any more is skipped and reported as
/// a [`SortWarning::RuleNamesInactiveMod`], the same as any other
/// rule-origin edge.
pub(super) fn user_decision_edges(
    overrides: &SorterOverrides,
    active: &BTreeSet<ModId>,
) -> (Vec<OrderingEdge>, Vec<SortWarning>) {
    let mut edges = Vec::new();
    let mut warnings = Vec::new();

    for (after, before) in &overrides.reorders {
        let resolved_after = resolve_active(after, active);
        let resolved_before = resolve_active(before, active);
        match (resolved_after, resolved_before) {
            (Some(after), Some(before)) => edges.push(OrderingEdge {
                after,
                before,
                layer: Layer::UserDecision,
                provenance: EdgeProvenance::Rule {
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                },
            }),
            (None, _) => warnings.push(SortWarning::RuleNamesInactiveMod {
                rule_origin: RuleOrigin::UserDecision,
                mod_id: after.clone(),
            }),
            (_, None) => warnings.push(SortWarning::RuleNamesInactiveMod {
                rule_origin: RuleOrigin::UserDecision,
                mod_id: before.clone(),
            }),
        }
    }

    (edges, warnings)
}

/// Shared core of [`rule_pair_edges`]/[`declared_override_pair_edges`]:
/// every pair rule of `origin` matching `predicate`, resolved against the
/// active set via [`ModId::base`], skipping any pair the caller has
/// `DropRule`-overridden — a fresh decision-layer override rather than a
/// rules-file edit,
/// so the underlying rule stays listed and re-applies the moment the
/// decision is reverted. A pair naming an inactive mod is skipped and
/// reported as a [`SortWarning`].
fn rule_pair_edges_matching(
    rules: &RuleSet,
    origin: RuleOrigin,
    layer: Layer,
    active: &BTreeSet<ModId>,
    overrides: &SorterOverrides,
    mut predicate: impl FnMut(&PairRule) -> bool,
) -> (Vec<OrderingEdge>, Vec<SortWarning>) {
    let mut edges = Vec::new();
    let mut warnings = Vec::new();

    for pair in rules
        .pairs()
        .filter(|pair| pair.origin == origin)
        .filter(|pair| predicate(pair))
        .filter(|pair| {
            !overrides
                .dropped_rules
                .contains(&(pair.after.clone(), pair.before.clone()))
        })
    {
        let after = resolve_active(&pair.after, active);
        let before = resolve_active(&pair.before, active);
        match (after, before) {
            (Some(after), Some(before)) => edges.push(OrderingEdge {
                after,
                before,
                layer,
                provenance: EdgeProvenance::Rule {
                    origin,
                    comment: pair.comment.clone(),
                },
            }),
            (None, _) => warnings.push(SortWarning::RuleNamesInactiveMod {
                rule_origin: origin,
                mod_id: pair.after.clone(),
            }),
            (_, None) => warnings.push(SortWarning::RuleNamesInactiveMod {
                rule_origin: origin,
                mod_id: pair.before.clone(),
            }),
        }
    }

    (edges, warnings)
}

/// Imported pair rules of one `origin`, added at `layer`. For
/// `origin: RuleOrigin::UserDecision` specifically, a pair whose own
/// `overrides_declared` is `true` is excluded — that pair's edge is added
/// once, earlier, by [`declared_override_pair_edges`] at
/// [`Layer::DeclaredOverride`] instead; adding it again here would be a
/// harmless-but-redundant duplicate edge at best, and a duplicate
/// `RuleOverruled`/witness-cycle entry at worst. Every other origin is
/// unaffected — `overrides_declared` is honoured only for `UserDecision`
/// rows (the type's own doc comment).
pub(super) fn rule_pair_edges(
    rules: &RuleSet,
    origin: RuleOrigin,
    layer: Layer,
    active: &BTreeSet<ModId>,
    overrides: &SorterOverrides,
) -> (Vec<OrderingEdge>, Vec<SortWarning>) {
    rule_pair_edges_matching(rules, origin, layer, active, overrides, |pair| {
        origin != RuleOrigin::UserDecision || !pair.overrides_declared
    })
}

/// The declared-edge override's own edges:
/// every `RuleOrigin::UserDecision` pair rule with `overrides_declared:
/// true`, added at [`Layer::DeclaredOverride`] — see that variant's own
/// doc comment for why this layer sits ahead of `Declared` rather than
/// reusing `Layer::UserDecision`.
pub(super) fn declared_override_pair_edges(
    rules: &RuleSet,
    active: &BTreeSet<ModId>,
    overrides: &SorterOverrides,
) -> (Vec<OrderingEdge>, Vec<SortWarning>) {
    rule_pair_edges_matching(
        rules,
        RuleOrigin::UserDecision,
        Layer::DeclaredOverride,
        active,
        overrides,
        |pair| pair.overrides_declared,
    )
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::EdgeKind;

    use super::*;
    use crate::domain::{PairRule, Rule};

    #[test]
    fn engine_edges_of_strength_filters_by_strength_and_drop_override() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .mod_("c")
            .hard_edge("a", "b")
            .declared_edge("a", "c")
            .build();
        let mut overrides = SorterOverrides::default();
        overrides
            .dropped_edges
            .insert((ModId::new("a"), ModId::new("b"), EdgeKind::AssemblyRef));

        let edges = engine_edges_of_strength(&report, EdgeStrength::Hard, Layer::Hard, &overrides);

        assert!(edges.is_empty(), "the only Hard edge was drop-overridden");
    }

    #[test]
    fn user_decision_edges_resolves_through_the_steam_suffix() {
        let mut overrides = SorterOverrides::default();
        overrides.reorders.push((ModId::new("a"), ModId::new("b")));
        let active: BTreeSet<ModId> = [ModId::new("a"), ModId::new("b_steam")]
            .into_iter()
            .collect();

        let (edges, warnings) = user_decision_edges(&overrides, &active);

        assert!(warnings.is_empty());
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].before, ModId::new("b_steam"));
    }

    /// A `Reorder` decision persists (the rerere scheme) past whatever
    /// prompted it — by the time it's re-applied, the mod it names could
    /// have been removed entirely. That must warn, not silently vanish.
    #[test]
    fn user_decision_edges_warns_on_a_mod_no_longer_active() {
        let mut overrides = SorterOverrides::default();
        overrides
            .reorders
            .push((ModId::new("a"), ModId::new("gone")));
        let active: BTreeSet<ModId> = [ModId::new("a")].into_iter().collect();

        let (edges, warnings) = user_decision_edges(&overrides, &active);

        assert!(edges.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(matches!(&warnings[0],
            SortWarning::RuleNamesInactiveMod { rule_origin: RuleOrigin::UserDecision, mod_id }
                if *mod_id == ModId::new("gone")
        ));
    }

    #[test]
    fn rule_pair_edges_resolves_through_the_steam_suffix() {
        let rules = RuleSet::new(vec![Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortUser,
            comment: None,
            overrides_declared: false,
        })]);
        let active: BTreeSet<ModId> = [ModId::new("a"), ModId::new("b_steam")]
            .into_iter()
            .collect();

        let (edges, warnings) = rule_pair_edges(
            &rules,
            RuleOrigin::RimSortUser,
            Layer::RimSortUser,
            &active,
            &SorterOverrides::default(),
        );

        assert!(warnings.is_empty());
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].before, ModId::new("b_steam"));
    }

    #[test]
    fn rule_pair_edges_warns_on_an_inactive_mod() {
        let rules = RuleSet::new(vec![Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("gone"),
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
            overrides_declared: false,
        })]);
        let active: BTreeSet<ModId> = [ModId::new("a")].into_iter().collect();

        let (edges, warnings) = rule_pair_edges(
            &rules,
            RuleOrigin::RimSortCommunity,
            Layer::RimSortCommunity,
            &active,
            &SorterOverrides::default(),
        );

        assert!(edges.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(matches!(&warnings[0],
            SortWarning::RuleNamesInactiveMod { mod_id, .. } if *mod_id == ModId::new("gone")
        ));
    }
}
