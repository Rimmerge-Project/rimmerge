//! Describing an accepted edge's provenance, and the effective placement and pair rules a finding cites.

use rim_analyzer::domain::{EdgeKind, EdgeStrength, ModId};

use crate::domain::{EdgeWinner, RuleOrigin, RuleSet};
use crate::sort::{EdgeProvenance, Layer, OrderingEdge};

/// A human-readable description of one accepted edge's origin, shared by
/// [`edge_winner`] (the `EdgeDropped` winner) and the `DeclarationQuestioned`
/// loop below — both need to say what overruled/opposed an advisory
/// relation, whether that's an engine edge's own `detail`, a rule's
/// origin (plus its free-text comment, if any), or an any-of choice.
pub(super) fn provenance_detail(provenance: &EdgeProvenance) -> String {
    match provenance {
        EdgeProvenance::Engine { detail, .. } => detail.clone(),
        EdgeProvenance::AnyOf { assembly } => {
            format!("the chosen any-of candidate for {assembly}")
        }
        EdgeProvenance::Rule { origin, comment } => {
            let base = describe_rule_origin(*origin);
            comment
                .as_deref()
                .map_or_else(|| base.clone(), |note| format!("{base}: {note}"))
        }
        // Tier sentinel edges are never `Real` (see `GraphEdgeKind`), so a
        // `Real` edge's own provenance is never `Tier` in practice — kept
        // for the match's own exhaustiveness, not a real input.
        EdgeProvenance::Tier { .. } => "a tier boundary".to_string(),
    }
}

fn describe_rule_origin(origin: RuleOrigin) -> String {
    match origin {
        RuleOrigin::UserDecision => "your own decision".to_string(),
        RuleOrigin::RimSortUser => "a RimSort user rule".to_string(),
        RuleOrigin::RimSortCommunity => "a RimSort community rule".to_string(),
        RuleOrigin::SteamDb => "a Steam Workshop dependency".to_string(),
    }
}

pub(super) fn edge_winner(winner: &OrderingEdge) -> EdgeWinner {
    EdgeWinner {
        after: winner.after.clone(),
        before: winner.before.clone(),
        layer: winner.layer,
        detail: provenance_detail(&winner.provenance),
    }
}

/// A dropped engine edge's actual strength, read off the layer it was
/// dropped from rather than re-derived from its `EdgeKind` alone —
/// `EdgeKind::strength()` can't distinguish a lazily-resolved
/// `AssemblyRef` (`Soft`) from a load-time one (`Hard`) by itself (see
/// `Finding::EdgeDropped::strength`'s own doc comment). Every engine-kind
/// drop was added at exactly one of these five layers (see
/// `sort/graph.rs::run`); every other layer is a rule-origin one that
/// never carries an `EdgeKind` at all (only `EdgeProvenance::Engine` drops
/// become an `EdgeDropped` finding in the first place), so the
/// `kind.strength()` fallback there is unreachable in practice, not a
/// real input to guard against — named explicitly, one arm per layer
/// rather than a wildcard, so a new `Layer`
/// variant is a compile error here too, like every other exhaustive match
/// over this type in this crate (`PatchScope::membership`,
/// `hidden_by_generated`).
pub(super) fn dropped_edge_strength(layer: Layer, kind: EdgeKind) -> EdgeStrength {
    match layer {
        Layer::Hard => EdgeStrength::Hard,
        Layer::Declared => EdgeStrength::Declared,
        // `Layer::Inferred`'s own
        // real strength, same explicit-arm treatment as `Hard`/`Declared`/
        // `Soft`/`Awareness` above — never the `kind.strength()` fallback,
        // since a dropped `Layer::Inferred` edge always carries an
        // `EdgeKind` whose own `strength()` already agrees (every
        // heuristic `EdgeKind` is `Inferred`), but this stays an explicit
        // arm for the same reason the doc comment above gives: named
        // per-layer, not a wildcard, so a future `Layer` variant is a
        // compile error here too.
        Layer::Inferred => EdgeStrength::Inferred,
        Layer::Soft => EdgeStrength::Soft,
        Layer::Awareness => EdgeStrength::Awareness,
        Layer::AnyOf
        | Layer::DeclaredOverride
        | Layer::UserDecision
        | Layer::RimSortUser
        | Layer::RimSortCommunity
        | Layer::SteamDb => kind.strength(),
    }
}

pub(super) fn sorted_pair(a: ModId, b: ModId) -> (ModId, ModId) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The origin of whichever placement rule actually decided `mod_id`'s
/// tier assignment: the first rule naming its base id in `rules`' own
/// (already precedence-sorted, `RuleSet::merged`) order — the exact same
/// "first match wins" reduction `sort::tiers::assign` itself applies, kept
/// here rather than calling that private function directly (the tier
/// assignment is a fact this crate can re-derive from
/// `rules.placements()` without the sorter exposing a new seam for it).
pub(super) fn effective_placement_origin(mod_id: &ModId, rules: &RuleSet) -> Option<RuleOrigin> {
    let base = mod_id.base();
    rules
        .placements()
        .find(|rule| rule.mod_id.base() == base)
        .map(|rule| rule.origin)
}

/// [`effective_placement_origin`]'s own sibling for a pair rule: whichever
/// pair rule naming this exact `(after, before)` direction actually
/// decides it for a user, per `rules.pairs()`'s own (already precedence-
/// sorted) "first match wins" order. An imported pair rule and its own
/// promoted `UserDecision` copy (`Session::promote_imported_rule`) are
/// keyed identically, so when both exist this always resolves to the
/// `UserDecision` one — the effective rule a user perceives, collapsing
/// what would otherwise be two independent `RuleOverruled` findings (one
/// per origin, see the `dropped` loop above) into the one keyed on it.
/// Returns the whole rule, not just its origin, since a caller also needs
/// `overrides_declared`
/// to score `RuleOverruled`'s own confidence correctly — see
/// `Finding::RuleOverruled::overrides_declared`'s own doc comment for why
/// `origin` alone does not determine which layer this rule was added
/// at.
pub(super) fn effective_pair_rule<'a>(
    after: &ModId,
    before: &ModId,
    rules: &'a RuleSet,
) -> Option<&'a crate::domain::PairRule> {
    let after_base = after.base();
    let before_base = before.base();
    rules
        .pairs()
        .find(|rule| rule.after.base() == after_base && rule.before.base() == before_base)
}
