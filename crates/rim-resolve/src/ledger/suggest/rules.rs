//! Suggestions for rule and placement findings: overruled rules, placement conflicts, inferred tags.

use super::{confidence, is_db_layer};
use crate::domain::{
    Action, Alternative, EdgeWinner, Placement, PromotedRuleKey, Rationale, RuleOrigin, Tag,
    TagSignal,
};
use crate::sort::Layer;
use rim_analyzer::domain::ModId;

/// The `RuleOverruled` confidence table for a dropped pair rule's winner:
/// `95` for `Hard`/`AnyOf`, `90` for
/// `Declared`/`UserDecision`, `70` for an imported (db) winner. Distinct
/// from [`winner_confidence`] (the `EdgeDropped` table) rather than
/// reusing it: a rule dropped by a `Declared`/`UserDecision` winner scores
/// 90 here, not 95 — overruling a *rule* (which may itself carry real
/// intent, an earlier decision or a curated community pairing) is scored
/// slightly more cautiously than overruling a bare engine edge. A
/// `Soft`/`Awareness`/`Inferred` winner can never actually reach this
/// function (every rule-origin layer precedes all three in `Layer`'s own
/// precedence order, so a rule can only ever be dropped by an
/// equal-or-earlier layer); the 55 fallback exists only so this stays a
/// total function.
///
/// A `UserDecision` winner
/// over a `UserDecision` loser is the same same-layer-tie shape
/// `EdgeDropped`'s own `winner_confidence` handles — the cycle-break tie-break
/// decided it, not one user decision actually outranking another, so it
/// folds to the base table's 55 ("needs input") instead of the flat 90 a
/// genuine `UserDecision`-over-db win gets. No other layer needs the same
/// treatment: a rule's own origin is never `Declared` (`RuleOrigin` has no
/// such variant), so a `Declared` winner over a rule is always a real,
/// asymmetric precedence win.
///
/// **`loser_overrides_declared` (the declared-edge override)**:
/// `RuleOrigin::UserDecision` alone cannot
/// tell a same-layer tie from a genuine cross-layer win — a
/// `UserDecision`-origin pair rule is
/// added at one of *two* layers (`Layer::UserDecision`, or
/// `Layer::DeclaredOverride` when `overrides_declared` is `true`), so this
/// flag carries that bit. A
/// `Layer::DeclaredOverride` winner beating a *plain* `UserDecision`/
/// imported rule (`loser_overrides_declared: false`) is a genuine,
/// asymmetric cross-layer win — that layer sits ahead of every other
/// rule-origin layer by design — so it joins the flat-90 bucket exactly
/// like a `Declared`/`UserDecision` winner does; two declared-edge-
/// override rules directly contradicting each other
/// (`loser_overrides_declared: true`) is the identical same-layer-tie
/// shape the `UserDecision`-over-`UserDecision` guard above exists for,
/// so it folds to the same base-table 55.
fn rule_overruled_confidence(
    winner_layer: Layer,
    loser_origin: RuleOrigin,
    loser_overrides_declared: bool,
) -> u8 {
    match winner_layer {
        Layer::Hard | Layer::AnyOf => 95,
        Layer::DeclaredOverride if loser_overrides_declared => 55,
        Layer::DeclaredOverride | Layer::Declared => 90,
        Layer::UserDecision if loser_origin == RuleOrigin::UserDecision => 55,
        Layer::UserDecision => 90,
        Layer::RimSortUser | Layer::RimSortCommunity | Layer::SteamDb => 70,
        Layer::Soft | Layer::Awareness | Layer::Inferred => 55,
    }
}

/// A pair rule
/// (imported, or the user's own prior decision) lost a cycle to a
/// stronger edge. `origin: UserDecision` is the same finding with a
/// rationale that says the user's own decision is the one that couldn't
/// hold, rather than an imported rule. `overrides_declared` (the
/// declared-edge override) says whether the
/// *losing* rule was itself flagged to override a declaration — always
/// `false` for a non-`UserDecision` origin.
pub(super) fn rule_overruled(
    after: &ModId,
    before: &ModId,
    origin: RuleOrigin,
    winner: Option<&EdgeWinner>,
    overrides_declared: bool,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    let Some(winner) = winner else {
        // A 3+-mod cycle: no single edge to blame, so the finding names the
        // witness cycle instead — same shape as `EdgeDropped`'s
        // longer-cycle case, with no confidence table row to pick from.
        return Suggestion {
            action: Action::Accept,
            confidence: confidence(55),
            rationale: Rationale::RuleOverruledLongerCycle {
                after: after.clone(),
                before: before.clone(),
                origin,
                overrides_declared,
            },
            alternatives: Vec::new(),
        };
    };

    let percent = rule_overruled_confidence(winner.layer, origin, overrides_declared);
    // `Reorder` is offered only against a db-origin winner, not merely
    // whenever the winner is not `Hard`: `Reorder` reasserts the
    // rule as a fresh `UserDecision` edge, which only ever changes the
    // outcome when the winner's own layer sits *after* `UserDecision` in
    // `LAYER_ORDER` (a db-origin winner) — against `Hard`/`AnyOf`/
    // `Declared`, or another `UserDecision`, the reassertion can never
    // win (it's added at or after the same precedence point the winner
    // already occupies), so offering it there would be a one-click no-op
    // that just spawns a second, identical finding next scan.
    let mut alternatives = Vec::new();
    if is_db_layer(winner.layer) {
        alternatives.push(Alternative {
            action: Action::Reorder {
                after: after.clone(),
                before: before.clone(),
            },
            rationale: Rationale::ReorderOverridingRuleWinner,
        });
    }
    // `Promote` never claims to beat the winner — it only
    // keeps the rule visible and toggle-proof even though it stays
    // overruled — so, unlike `Reorder`, it's offered regardless of the
    // winner's layer (consistent with `placement_overruled`'s own gate
    // below).
    if origin != RuleOrigin::UserDecision {
        alternatives.push(Alternative {
            action: Action::PromoteRule {
                rule: PromotedRuleKey::Pair {
                    after: after.clone(),
                    before: before.clone(),
                },
            },
            rationale: Rationale::PromoteRuleDespiteOverruled,
        });
    }

    Suggestion {
        action: Action::Accept,
        confidence: confidence(percent),
        rationale: Rationale::RuleOverruledByWinner {
            after: after.clone(),
            before: before.clone(),
            origin,
            overrides_declared,
            winner: winner.clone(),
        },
        alternatives,
    }
}

/// A `PlacementRule`
/// (Top/Bottom) was overruled by a stronger edge crossing the tier
/// boundary — the sorter already applied it, so this is disclosure plus
/// the decision point. No alternative can beat a `Hard` winner; `DropRule`
/// is offered
/// only when the winner is itself a pair rule from a db origin (imported —
/// dropping the user's own prior decision would be a stranger thing to
/// offer as a one-click fix than promoting the placement instead).
pub(super) fn placement_overruled(
    mod_id: &ModId,
    placement: Placement,
    origin: RuleOrigin,
    by: &EdgeWinner,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    // `PlacementRule` has no `overrides_declared` of its own (the
    // declared-edge override,, is a `PairRule`-
    // only field) — always `false` here.
    let percent = rule_overruled_confidence(by.layer, origin, false);

    // `Promote`'s gate matches `rule_overruled`'s own — never
    // conditioned on the winner's layer, since it never claims to beat
    // the winner, only to survive the import toggles. `DropRule` stays
    // gated to a db-layer winner: dropping the rule is only a sensible
    // one-click fix when the winner is itself an imported pair rule.
    let mut alternatives = Vec::new();
    if origin != RuleOrigin::UserDecision {
        alternatives.push(Alternative {
            action: Action::PromoteRule {
                rule: PromotedRuleKey::Placement {
                    mod_id: mod_id.clone(),
                },
            },
            rationale: Rationale::PromotePlacementDespiteOverruled { placement },
        });
    }
    if is_db_layer(by.layer) {
        alternatives.push(Alternative {
            action: Action::DropRule {
                after: by.after.clone(),
                before: by.before.clone(),
            },
            rationale: Rationale::DropRuleOverrulingPlacement {
                mod_id: mod_id.clone(),
                placement,
            },
        });
    }

    Suggestion {
        action: Action::Accept,
        confidence: confidence(percent),
        rationale: Rationale::PlacementOverruled {
            mod_id: mod_id.clone(),
            placement,
            by: by.clone(),
        },
        alternatives,
    }
}

/// A `PlacementRule` holds, but an advisory `Soft`/
/// `Awareness` relation can never be satisfied under it. The placement
/// wins; `Reorder` enforces the relation's own direction as a user
/// decision instead, after which the next build reports it as
/// `PlacementOverruled` with a `UserDecision` winner — the intended round
/// trip.
pub(super) fn placement_questioned(
    mod_id: &ModId,
    placement: Placement,
    other: &ModId,
    relation_kind: rim_analyzer::domain::EdgeKind,
    relation_detail: &str,
) -> crate::domain::Suggestion {
    // The relation's own direction, exactly as the advisory edge itself
    // specified it (see `ledger::findings::extract`'s own construction):
    // a `Bottom` `mod_id` was named as `edge.before` (`other after
    // mod_id`); a `Top` `mod_id` was named as `edge.after` (`mod_id after
    // other`).
    let (reorder_after, reorder_before) = match placement {
        Placement::Bottom => (other.clone(), mod_id.clone()),
        Placement::Top => (mod_id.clone(), other.clone()),
    };
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(85),
        rationale: Rationale::PlacementQuestioned {
            mod_id: mod_id.clone(),
            placement,
            relation_kind,
            relation_detail: relation_detail.to_string(),
        },
        alternatives: vec![Alternative {
            action: Action::Reorder {
                after: reorder_after,
                before: reorder_before,
            },
            rationale: Rationale::EnforceRelationAsUserDecision,
        }],
    }
}

/// A holding
/// placement pin's own extreme-edge region-ordering preference was
/// defeated by another mod's accepted, already-enforced edge — the edge
/// always wins (dropping it would break the other mod's own real
/// dependency), so this is disclosure of a correct fact, not a decision
/// point. **No alternatives**: unlike `placement_overruled`/
/// `rule_overruled`, there is nothing to promote, drop, or reorder — the
/// pin's own placement is untouched and the crossing edge is genuine.
pub(super) fn placement_ordering_overridden(
    mod_id: &ModId,
    pinned: &ModId,
    placement: Placement,
    by: &EdgeWinner,
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(95),
        rationale: Rationale::PlacementOrderingOverridden {
            mod_id: mod_id.clone(),
            pinned: pinned.clone(),
            placement,
            by: by.clone(),
        },
        alternatives: Vec::new(),
    }
}

/// The rule-level companion of [`placement_ordering_overridden`]: a
/// holding placement
/// promotes one or more other mods past its own tier boundary. Purely
/// disclosure, like [`placement_ordering_overridden`] — **no
/// alternative offered**: this names a fact about the placement's own
/// reach (a framework pin dragging its dependents along, or a leaf pin
/// with none), not a decision point. A user who decides the reach isn't
/// worth it already has a way to act on it — the rules page, or
/// `rimmerge rule set-placement`/deleting the rule — without this
/// finding growing an `Action` of its own.
///
/// `promoted.len()` is worded as "attributed to" rather than "promotes …
/// in total": a mod promoted past *two* pins' own boundaries is
/// attributed to only one of them (see
/// [`crate::domain::FindingKey::PlacementPromotesDependents`]'s own doc
/// comment), so a flat total would overclaim completeness for that mod's
/// sibling pin.
pub(super) fn placement_promotes_dependents(
    mod_id: &ModId,
    placement: Placement,
    promoted: &[ModId],
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(95),
        rationale: Rationale::PlacementPromotesDependents {
            mod_id: mod_id.clone(),
            placement,
            promoted: promoted.to_vec(),
        },
        alternatives: Vec::new(),
    }
}

pub(super) fn tag_inferred(
    mod_id: &ModId,
    tag: &Tag,
    matched: &[TagSignal],
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::AddTag {
            mod_id: mod_id.clone(),
            tag: tag.clone(),
        },
        confidence: crate::tags::infer::combined_confidence(matched),
        rationale: Rationale::TagInferredSignalCount {
            count: matched.len(),
        },
        alternatives: vec![Alternative {
            action: Action::RemoveTag {
                mod_id: mod_id.clone(),
                tag: tag.clone(),
            },
            rationale: Rationale::RejectInferredTag,
        }],
    }
}
