//! Suggestions for ordering-evidence findings: dropped edges, questioned or overridden declarations, `AnyOf` choices.

use super::{SuggestContext, confidence, is_db_layer, winner_confidence};
use crate::domain::{Action, Alternative, EdgeWinner, Rationale};
use crate::sort::Layer;
use rim_analyzer::domain::{EdgeStrength, ModId};

pub(super) fn edge_dropped(
    kind: rim_analyzer::domain::EdgeKind,
    strength: EdgeStrength,
    after: &ModId,
    before: &ModId,
    winner: Option<&EdgeWinner>,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    // A deliberate design choice: a cosmetic reorder is enforced when
    // free but dropped first among its `Inferred` siblings
    // (`sort::cycles::kind_rank`), and once dropped it's never a decision
    // point at all — the final defs are identical either order, so there
    // is nothing to weigh against `winner`/`strength` below, and no
    // `Reorder` alternative to offer (there is no "wrong direction" to
    // correct). Checked first, before the `winner`/`strength` branches
    // every other kind uses, so it short-circuits regardless of what
    // else the cycle looked like.
    if kind == rim_analyzer::domain::EdgeKind::PatchRemovedNodeCosmetic {
        return Suggestion {
            action: Action::Accept,
            confidence: confidence(95),
            rationale: Rationale::EdgeDroppedCosmetic,
            alternatives: Vec::new(),
        };
    }

    let drop_action = Action::DropEdge {
        after: after.clone(),
        before: before.clone(),
        kind,
    };
    let keep_alt = Alternative {
        action: Action::KeepEdge {
            after: after.clone(),
            before: before.clone(),
            kind,
        },
        rationale: Rationale::KeepEdgeInCycle,
    };
    // The alternatives offer Reorder to side with the loser — force
    // the dropped edge's own direction as a user decision instead.
    let reorder_with_loser_alt = Alternative {
        action: Action::Reorder {
            after: after.clone(),
            before: before.clone(),
        },
        rationale: Rationale::ReorderOverridingEdgeWinner,
    };

    // When the witness cycle
    // was a direct two-mod contradiction, name the edge that won instead
    // of only describing the one that lost. Longer cycles (`winner` is
    // `None`) keep the strength-based text below.
    if let Some(winner) = winner
        && let Some(percent) = winner_confidence(winner.layer, strength)
    {
        // Only a db-rule winner offers the "reorder to side with the
        // loser" alternative — see `is_db_layer`'s own doc comment.
        let alternatives = if is_db_layer(winner.layer) {
            vec![keep_alt, reorder_with_loser_alt]
        } else {
            vec![keep_alt]
        };
        return Suggestion {
            action: drop_action,
            confidence: confidence(percent),
            rationale: Rationale::EdgeDroppedWithWinner {
                after: after.clone(),
                before: before.clone(),
                winner: winner.clone(),
            },
            alternatives,
        };
    }

    // `FindingKey::EdgeDropped` only ever identifies an engine (`EdgeKind`)
    // drop, so the table's "Declared / RimSort / SteamDb" row collapses to
    // just `Declared` here — a dropped rule-origin edge has no `EdgeKind`
    // and never reaches this function (see `ledger::findings::extract`).
    // Branches on `strength` (read off the dropped edge's own layer),
    // never `kind.strength()`: that baseline can't tell a dropped lazy
    // `AssemblyRef` (`Soft`) from a load-time one (`Hard`) apart on its
    // own — see `Finding::EdgeDropped::strength`'s doc comment.
    match strength {
        EdgeStrength::Soft | EdgeStrength::Awareness => Suggestion {
            action: drop_action,
            confidence: confidence(90),
            rationale: Rationale::EdgeDroppedSoftOrAwareness,
            alternatives: vec![keep_alt],
        },
        // `Inferred`
        // (`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/
        // `PatchInvalidatesPredicate`) is heuristic *evidence* the
        // analyzer concluded from mod content, not a mere
        // presence/awareness signal — the "soft or awareness-only ...
        // rarely changes behavior" text above would be actively wrong
        // here: these are enforced by default (`EnforcedLayers::inferred`)
        // precisely because dropping one usually *does* change behavior
        // (a removed def re-targeted by a later patch, a retexture
        // applied before the content it replaces, an override applied
        // before its own origin mod, a predicate a later patch depends on
        // being invalidated first).
        // Scored between `Declared` (55, an author's own word beat by the
        // cycle tie-break) and `Soft`/`Awareness` (90, rarely matters):
        // real evidence, but never an author's or the user's own
        // declaration the way `Declared` is. Each kind
        // gets its own grounded wording (`inferred_edge_dropped_rationale`)
        // instead of one sentence naming all of them at once — `kind` is
        // already a parameter, so the specificity costs nothing here.
        EdgeStrength::Inferred => Suggestion {
            action: drop_action,
            confidence: confidence(70),
            rationale: Rationale::EdgeDroppedInferred {
                kind,
                after: after.clone(),
                before: before.clone(),
            },
            alternatives: vec![keep_alt],
        },
        EdgeStrength::Declared => Suggestion {
            action: drop_action,
            confidence: confidence(55),
            rationale: Rationale::EdgeDroppedDeclared,
            alternatives: vec![keep_alt],
        },
        EdgeStrength::Hard => Suggestion {
            action: drop_action,
            confidence: confidence(20),
            rationale: Rationale::EdgeDroppedHard,
            alternatives: vec![
                keep_alt,
                Alternative {
                    action: Action::RemoveMod {
                        mod_id: after.clone(),
                    },
                    rationale: Rationale::RemoveDependentModBrokenOrder,
                },
            ],
        },
    }
}

/// An advisory relation pointing the opposite way from
/// an enforced `Declared`/db edge. The declaration always wins (it was
/// never actually dropped — the advisory edge was never added to the
/// graph in the first place); `Accept` just confirms that, and `Reorder`
/// lets the user force the relation's own direction instead, as an
/// explicit decision.
pub(super) fn declaration_questioned(
    declared_after: &ModId,
    declared_before: &ModId,
    declared_layer: Layer,
    declared_detail: &str,
    relation_kind: rim_analyzer::domain::EdgeKind,
    relation_detail: &str,
) -> crate::domain::Suggestion {
    // Same rule as `edge_dropped`'s — offering a
    // one-click `Reorder` against the declaration is only appropriate
    // when that declaration is itself an imported db rule, never against
    // a `Hard` fact, an author's `Declared` order, or the user's own
    // prior `UserDecision`.
    let alternatives = if is_db_layer(declared_layer) {
        vec![Alternative {
            action: Action::Reorder {
                after: declared_before.clone(),
                before: declared_after.clone(),
            },
            rationale: Rationale::ReorderOppositeOfRelation,
        }]
    } else {
        Vec::new()
    };
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(85),
        rationale: Rationale::DeclarationQuestioned {
            declared_after: declared_after.clone(),
            declared_before: declared_before.clone(),
            declared_layer,
            declared_detail: declared_detail.to_string(),
            relation_kind,
            relation_detail: relation_detail.to_string(),
        },
        alternatives,
    }
}

/// The declared-edge override's own finding: a
/// `Declared`-strength engine edge lost a cycle to a rule the user
/// explicitly flagged to override it. The edge always wins (it was never
/// in doubt — the whole point of the flag is to make it win); this is
/// disclosure of a correct, if easy-to-miss, fact, not a decision point —
/// no alternative, mirroring [`placement_ordering_overridden`]'s own
/// shape exactly. Confidence is the flat 95
/// [`winner_confidence`] gives every `Layer::DeclaredOverride` winner.
pub(super) fn declaration_overridden(
    declared_after: &ModId,
    declared_before: &ModId,
    _kind: rim_analyzer::domain::EdgeKind,
    detail: &str,
    by: &EdgeWinner,
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(95),
        rationale: Rationale::DeclarationOverridden {
            declared_after: declared_after.clone(),
            declared_before: declared_before.clone(),
            detail: detail.to_string(),
            by: by.clone(),
        },
        alternatives: Vec::new(),
    }
}

pub(super) fn any_of_choice(
    after: &ModId,
    assembly: &str,
    candidates: &[ModId],
    ctx: &SuggestContext<'_>,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    let chosen = ctx
        .sort_outcome
        .any_of_choices
        .iter()
        .find(|c| c.after == *after && c.assembly == assembly)
        .and_then(|c| c.chosen.clone());

    let Some(chosen) = chosen else {
        return Suggestion {
            action: Action::Ignore,
            confidence: confidence(0),
            rationale: Rationale::AnyOfNoCandidate,
            alternatives: candidates
                .iter()
                .map(|candidate| Alternative {
                    action: Action::ChooseCandidate {
                        after: after.clone(),
                        chosen: candidate.clone(),
                    },
                    rationale: Rationale::ForceAnyOfCandidateAcceptingCycle,
                })
                .collect(),
        };
    };

    let action = Action::ChooseCandidate {
        after: after.clone(),
        chosen: chosen.clone(),
    };
    let alternatives: Vec<Alternative> = candidates
        .iter()
        .filter(|candidate| **candidate != chosen)
        .map(|candidate| Alternative {
            action: Action::ChooseCandidate {
                after: after.clone(),
                chosen: candidate.clone(),
            },
            rationale: Rationale::AnyOfAlternativeCandidate,
        })
        .collect();

    let already_before = ctx
        .current
        .position(&chosen)
        .zip(ctx.current.position(after))
        .is_some_and(|(chosen_pos, after_pos)| chosen_pos < after_pos);
    if already_before {
        return Suggestion {
            action,
            confidence: confidence(95),
            rationale: Rationale::AnyOfChosenAlreadyBefore,
            alternatives,
        };
    }

    let chosen_hard_dependents = ctx.mods_by_id.get(&chosen).map_or(0, |m| m.hard_dependents);
    let is_unique_max = chosen_hard_dependents > 0
        && candidates.iter().all(|candidate| {
            *candidate == chosen
                || ctx
                    .mods_by_id
                    .get(candidate)
                    .map_or(0, |m| m.hard_dependents)
                    < chosen_hard_dependents
        });
    if is_unique_max {
        return Suggestion {
            action,
            confidence: confidence(80),
            rationale: Rationale::AnyOfUniqueMaxDependents,
            alternatives,
        };
    }

    Suggestion {
        action,
        confidence: confidence(50),
        rationale: Rationale::AnyOfSmallestId,
        alternatives,
    }
}
