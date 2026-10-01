//! Extracts every currently-live [`Finding`] from the analyzer's
//! [`Report`], the sorter's [`SortOutcome`], and the current [`Tagging`],
//! keyed by its stable [`FindingKey`] (the rerere identity).

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{Conflict, EdgeStatus, EdgeStrength, LoadOrder, ModId, Report};

use crate::domain::{
    Finding, FindingKey, GeneratedMods, Placement, RuleSet, TagProvenance, Tagging,
};
use crate::evaluate;
use crate::sort::{EdgeProvenance, Layer, OrderingEdge, SortOutcome, Tier, TierReason};
use conflicts::{
    insert_conflict, insert_keyed_translation_collisions, insert_undeclared_type_dependencies,
};
use generated::hidden_by_generated;
use provenance::{
    dropped_edge_strength, edge_winner, effective_pair_rule, effective_placement_origin,
    provenance_detail, sorted_pair,
};

mod conflicts;
mod generated;
mod provenance;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "findings/findings_tests.rs"]
mod tests;

/// Every finding currently live, keyed by its stable [`FindingKey`].
///
/// A [`super::build::build`] caller passes the *same* `sort_outcome` and
/// `tagging` used to build whichever order it's evaluating findings
/// against, plus `selected_order` — the specific order ([`OrderSource`](crate::domain::OrderSource)
/// `current` or `suggested`) that ledger is being built for. Every finding
/// kind but two is the same regardless of `selected_order` (only the
/// suggestion and `resolved_by_suggested` differ per source): the
/// exceptions are [`Finding::TextureOverride`], whose `winner` is the owner
/// loaded last in that order, and [`FindingKey::LazyReferenceViolated`], which is
/// necessarily evaluated against *some* order (a `Soft` edge is only
/// "violated" relative to one), and must be `selected_order` specifically
/// — evaluating it against `sort_outcome.order` (always the suggested
/// order) unconditionally would mean the `Current` ledger could never show
/// a lazy reference the current order violates but the suggested one
/// happens to satisfy.
#[must_use]
pub fn extract(
    report: &Report,
    sort_outcome: &SortOutcome,
    tagging: &Tagging,
    selected_order: &LoadOrder,
    rules: &RuleSet,
    show_dangling_def_references: bool,
) -> BTreeMap<FindingKey, Finding> {
    let mut findings = BTreeMap::new();

    for dropped in &sort_outcome.dropped {
        match &dropped.edge.provenance {
            EdgeProvenance::Engine { kind, detail } => {
                findings.insert(
                    FindingKey::EdgeDropped {
                        after: dropped.edge.after.clone(),
                        before: dropped.edge.before.clone(),
                        kind: *kind,
                    },
                    Finding::EdgeDropped {
                        after: dropped.edge.after.clone(),
                        before: dropped.edge.before.clone(),
                        kind: *kind,
                        detail: detail.clone(),
                        strength: dropped_edge_strength(dropped.edge.layer, *kind),
                        winner: dropped.winner.as_ref().map(edge_winner),
                    },
                );
                // The declared-edge override:
                // additive to the `EdgeDropped` finding just inserted
                // above, never instead of it — fires only when this was
                // genuinely a `Declared`-strength edge (the fact being
                // overridden) losing a direct two-mod cycle to a rule
                // specifically added at `Layer::DeclaredOverride`. A
                // longer (3+-mod) cycle has no single winner to name
                // (`dropped.winner` is `None`) and never produces this
                // finding — the same "no single edge to blame" shape
                // `RuleOverruled`'s own `winner: None` case already has.
                if dropped.edge.layer == Layer::Declared
                    && let Some(winner) = dropped.winner.as_ref()
                    && winner.layer == Layer::DeclaredOverride
                {
                    findings.insert(
                        FindingKey::DeclarationOverridden {
                            declared_after: dropped.edge.after.clone(),
                            declared_before: dropped.edge.before.clone(),
                            kind: *kind,
                        },
                        Finding::DeclarationOverridden {
                            declared_after: dropped.edge.after.clone(),
                            declared_before: dropped.edge.before.clone(),
                            kind: *kind,
                            detail: detail.clone(),
                            by: edge_winner(winner),
                        },
                    );
                }
            }
            // A dropped pair rule (imported, or the user's own prior
            // decision) — every origin gets a finding, not only the
            // `UserDecision` one `SortWarning::UserDecisionOverruled`
            // discloses.
            EdgeProvenance::Rule { origin, comment } => {
                // An imported pair rule and its own promoted `UserDecision`
                // copy share one key and can both independently lose the
                // same cycle to the same winner (the sorter adds a
                // rule-origin edge per origin — see `sort/graph.rs::run`'s
                // `Layer::UserDecision` arm calling `rule_pair_edges` for
                // that origin too). Key on the *effective* rule — the one
                // `rules.pairs()`'s own precedence order would actually
                // apply, mirroring `effective_placement_origin` above —
                // rather than this specific dropped edge's own origin, so
                // both collapse into the one finding the user actually
                // owns instead of two identical-looking rows. When the
                // effective rule is the promoted `UserDecision` copy,
                // this also withholds `Promote` for free
                // (`ledger::suggest::rule_overruled`'s own gate already
                // skips it for that origin).
                let effective_rule =
                    effective_pair_rule(&dropped.edge.after, &dropped.edge.before, rules);
                let origin = effective_rule.map_or(*origin, |rule| rule.origin);
                let overrides_declared = effective_rule.is_some_and(|rule| rule.overrides_declared);
                findings.insert(
                    FindingKey::RuleOverruled {
                        after: dropped.edge.after.clone(),
                        before: dropped.edge.before.clone(),
                        origin,
                    },
                    Finding::RuleOverruled {
                        after: dropped.edge.after.clone(),
                        before: dropped.edge.before.clone(),
                        origin,
                        comment: comment.clone(),
                        winner: dropped.winner.as_ref().map(edge_winner),
                        witness_cycle: dropped.witness_cycle.clone(),
                        overrides_declared,
                    },
                );
            }
            // Neither ever reaches `SortOutcome::dropped`: an `AnyOf`-layer
            // edge is only ever added once it can't create a cycle
            // (`sort/any_of.rs`'s own "avoids cycles proactively"), so
            // `sort/graph.rs::run` skips calling `break_cycles` for that
            // layer entirely; a `Tier` boundary/membership edge is never
            // `GraphEdgeKind::Real` in the first place; either would need a
            // new `Real`-graph-edge source before this arm could ever see
            // one.
            EdgeProvenance::AnyOf { .. } | EdgeProvenance::Tier { .. } => {}
        }
    }

    // A `PlacementRule` (Top/Bottom) overruled by a stronger `Real` edge
    // crossing the tier boundary — the case `TierReason::PromotedBy`
    // already records (`explanation.tier` stays the mod's *nominal* tier
    // even after promotion; only `tier_reason` changes — see
    // `sort::explain::build`), also as a finding. `Tier::Top`/`Tier::Bottom`
    // are only ever produced by a placement rule (`sort::tiers::assign_one`),
    // so a mod nominally in either tier is provably placement-pinned.
    for (mod_id, explanation) in &sort_outcome.placements {
        let TierReason::PromotedBy(winning_edge) = &explanation.tier_reason else {
            continue;
        };
        let placement = match explanation.tier {
            Tier::Top => Placement::Top,
            Tier::Bottom => Placement::Bottom,
            Tier::Core | Tier::Dlc | Tier::Body => continue,
        };
        let Some(origin) = effective_placement_origin(mod_id, rules) else {
            // Defensive: per the invariant above this can't happen (the
            // tier itself proves a placement rule exists), but this stays
            // total rather than panicking on a future sorter change that
            // invalidates it.
            continue;
        };
        findings.insert(
            FindingKey::PlacementOverruled {
                mod_id: mod_id.clone(),
                placement,
                origin,
            },
            Finding::PlacementOverruled {
                mod_id: mod_id.clone(),
                placement,
                origin,
                by: edge_winner(winning_edge),
                landed_at: explanation.position,
            },
        );
    }

    // `PlacementQuestioned`: an advisory `Soft`/`Awareness` edge that can
    // never be satisfied under a placement that *does* hold (the mod was
    // not overruled — see the loop above for that case). Unlike
    // `DeclarationQuestioned` (`Awareness` only), both strengths qualify
    // here: a `Bottom` mod can never load
    // before some non-`Bottom` mod required to load after it; a `Top` mod
    // can never load after some non-`Core`/`Dlc`/`Top` mod required to
    // load before it — structurally impossible regardless of which
    // strength excluded the edge from the graph.
    for (mod_id, explanation) in &sort_outcome.placements {
        // A mod whose placement was
        // itself overruled (`TierReason::PromotedBy`, the `PlacementOverruled`
        // loop above) isn't actually sitting at its nominal tier any more — it
        // already has its own `PlacementOverruled` finding; scanning its
        // remaining advisory edges as if the placement still held would
        // report a second, redundant finding for the same fact.
        if matches!(explanation.tier_reason, TierReason::PromotedBy(_)) {
            continue;
        }
        let placement = match explanation.tier {
            Tier::Top => Placement::Top,
            Tier::Bottom => Placement::Bottom,
            Tier::Core | Tier::Dlc | Tier::Body => continue,
        };
        for advisory in &explanation.advisory {
            let edge = &advisory.edge;
            let other = match placement {
                Placement::Bottom if edge.before == *mod_id => &edge.after,
                Placement::Top if edge.after == *mod_id => &edge.before,
                _ => continue,
            };
            // Already satisfied in the order this ledger is being built
            // for — `selected_order`, not `sort_outcome`'s own suggested
            // order (`sort_outcome` is always the outcome that produced
            // *suggested*, per this function's own doc comment; the two
            // differ exactly when this ledger is being built for
            // `OrderSource::Current`, and can also differ within
            // `sort_outcome` itself once a Reorder decision promotes the
            // relation into a real accepted edge while the raw analyzer
            // edge stays in `advisory` regardless). Nothing to question
            // once the relation already holds.
            if evaluate::ordering_status(&edge.after, &edge.before, selected_order)
                == EdgeStatus::Satisfied
            {
                continue;
            }
            let Some(other_explanation) = sort_outcome.placements.get(other) else {
                continue;
            };
            let structurally_impossible = match placement {
                Placement::Bottom => other_explanation.tier != Tier::Bottom,
                Placement::Top => {
                    !matches!(other_explanation.tier, Tier::Core | Tier::Dlc | Tier::Top)
                }
            };
            if !structurally_impossible {
                continue;
            }
            let EdgeProvenance::Engine {
                kind: relation_kind,
                detail: relation_detail,
            } = &edge.provenance
            else {
                // Every `PlacementExplanation::advisory` entry comes from
                // an engine edge (`sort/graph.rs::run`'s own
                // `advisory_edges`) — not a real input to guard against.
                continue;
            };
            findings.insert(
                FindingKey::PlacementQuestioned {
                    mod_id: mod_id.clone(),
                    placement,
                    relation: *relation_kind,
                },
                Finding::PlacementQuestioned {
                    mod_id: mod_id.clone(),
                    placement,
                    other: other.clone(),
                    relation_kind: *relation_kind,
                    relation_detail: relation_detail.clone(),
                },
            );
        }
    }

    // `PlacementOrderingOverridden`: a *holding* placement
    // pin's own extreme-edge region-ordering preference
    // (`sort::emit::placement_bias`) defeated by another mod's accepted
    // `Real` edge — a mod required *after* a holding `Bottom` pin, or
    // *before* a holding `Top` pin. Disjoint from both loops above: the
    // pin's own tier membership holds here (a pin whose placement was
    // itself overruled already has its own `PlacementOverruled` finding
    // from the first loop, skipped below the same way the
    // `PlacementQuestioned` loop skips a promoted mod's remaining advisory
    // edges), and the
    // crossing edge is always accepted/`Real` — never one of
    // `PlacementQuestioned`'s excluded `Soft`/`Awareness` advisory ones,
    // which structurally can never reach this case at all.
    for (mod_id, explanation) in &sort_outcome.placements {
        if matches!(explanation.tier_reason, TierReason::PromotedBy(_)) {
            continue;
        }
        let placement = match explanation.tier {
            Tier::Top => Placement::Top,
            Tier::Bottom => Placement::Bottom,
            Tier::Core | Tier::Dlc | Tier::Body => continue,
        };
        // A `Bottom` pin's own extreme edge is crossed by an accepted
        // outgoing edge requiring some other mod after it; a `Top` pin's
        // is crossed by an accepted incoming edge requiring some other
        // mod before it — the two mirror-image halves of one symmetric
        // rule.
        let crossing_edges: Vec<&OrderingEdge> = match placement {
            Placement::Bottom => explanation
                .upper_bounds
                .iter()
                .filter(|edge| edge.before == *mod_id)
                .collect(),
            Placement::Top => explanation
                .lower_bounds
                .iter()
                .filter(|edge| edge.after == *mod_id)
                .collect(),
        };
        for edge in crossing_edges {
            let other = match placement {
                Placement::Bottom => &edge.after,
                Placement::Top => &edge.before,
            };
            let Some(other_explanation) = sort_outcome.placements.get(other) else {
                continue;
            };
            // Two pins of the *same* placement directly ordered relative
            // to each other by a real edge is not this finding's case —
            // nothing was "merely promoted in", both sides are explicit
            // pins.
            let other_is_same_placement = match placement {
                Placement::Bottom => other_explanation.tier == Tier::Bottom,
                Placement::Top => other_explanation.tier == Tier::Top,
            };
            if other_is_same_placement {
                continue;
            }
            // `upper_bounds`/`lower_bounds` are pre-sorted layer-first
            // (Hard first) — `or_insert_with` keeps the strongest
            // crossing edge when more than one exists for the same
            // `(other, pin)` pair, rather than whichever happens to be
            // last in iteration order.
            findings
                .entry(FindingKey::PlacementOrderingOverridden {
                    mod_id: other.clone(),
                    pinned: mod_id.clone(),
                    placement,
                })
                .or_insert_with(|| Finding::PlacementOrderingOverridden {
                    mod_id: other.clone(),
                    pinned: mod_id.clone(),
                    placement,
                    by: edge_winner(edge),
                });
        }
    }

    // `PlacementPromotesDependents`: how many other mods each
    // holding placement promotes past its own tier boundary, attributed
    // via the identical nearest-placed-node walk `sort::cycles::
    // find_promotion_cause` already computed for every `TierReason::
    // PromotedBy` — never a hand-picked dependent-count
    // threshold. Reading `TierReason::PromotedBy(edge)`'s own `edge` back
    // and checking which of its two endpoints is itself a holding pin is
    // sufficient (not a second BFS): `find_promotion_cause`'s own walk
    // already guarantees that edge has the nearest reachable placed mod
    // as one endpoint whenever one exists in the closed SCC at all.
    let mut promoted_by_pin: BTreeMap<ModId, BTreeSet<ModId>> = BTreeMap::new();
    for (other_id, other_explanation) in &sort_outcome.placements {
        let TierReason::PromotedBy(edge) = &other_explanation.tier_reason else {
            continue;
        };
        let is_holding_pin = |id: &ModId| {
            sort_outcome
                .placements
                .get(id)
                .is_some_and(|p| matches!(p.tier_reason, TierReason::Placement(_)))
        };
        let attributed_pin = [&edge.before, &edge.after]
            .into_iter()
            .find(|candidate| is_holding_pin(candidate));
        if let Some(pin_id) = attributed_pin {
            promoted_by_pin
                .entry(pin_id.clone())
                .or_default()
                .insert(other_id.clone());
        }
    }
    for (mod_id, explanation) in &sort_outcome.placements {
        if !matches!(explanation.tier_reason, TierReason::Placement(_)) {
            continue;
        }
        let placement = match explanation.tier {
            Tier::Top => Placement::Top,
            Tier::Bottom => Placement::Bottom,
            Tier::Core | Tier::Dlc | Tier::Body => continue,
        };
        let Some(promoted) = promoted_by_pin.get(mod_id) else {
            continue;
        };
        if promoted.is_empty() {
            continue;
        }
        findings.insert(
            FindingKey::PlacementPromotesDependents {
                mod_id: mod_id.clone(),
                placement,
            },
            Finding::PlacementPromotesDependents {
                mod_id: mod_id.clone(),
                placement,
                promoted: promoted.iter().cloned().collect(),
            },
        );
    }

    // `DeclarationQuestioned`: an advisory
    // `Awareness`-strength edge pointing the opposite way from an
    // enforced `Declared`/db edge between the same two mods. `Soft`
    // advisories are excluded on purpose — a violated `Soft` edge already
    // has its own dedicated finding (`LazyReferenceViolated`), and the
    // fixed-rationale list (`ledger::suggest`) only ever names
    // `Awareness`-strength kinds. Every accepted edge between the same
    // pair as an advisory edge shows up in both endpoints' own
    // `lower_bounds`/`upper_bounds`, so checking one endpoint's
    // `PlacementExplanation` (the one the advisory edge is attached to)
    // is enough — no need to cross-reference the other mod's.
    for explanation in sort_outcome.placements.values() {
        for advisory in &explanation.advisory {
            // `explain::build`'s own `AdvisoryEdge.strength`
            // reports the real `EdgeStrength::Inferred` for a
            // `PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/
            // `PatchInvalidatesPredicate`/`PatchRemovedNodeCosmetic` edge
            // (`Layer::Inferred` has its
            // own branch in `sort::graph::run`, never folded into
            // `Layer::Awareness`'s bucket), so this one strength check
            // already excludes every one of them — no separate
            // per-`EdgeKind` guard is needed.
            if advisory.strength != EdgeStrength::Awareness {
                continue;
            }
            let Some(declared) = explanation
                .lower_bounds
                .iter()
                .chain(explanation.upper_bounds.iter())
                .find(|accepted| {
                    accepted.after == advisory.edge.before
                        && accepted.before == advisory.edge.after
                        && matches!(
                            accepted.layer,
                            Layer::Declared
                                | Layer::UserDecision
                                | Layer::RimSortUser
                                | Layer::RimSortCommunity
                                | Layer::SteamDb
                        )
                })
            else {
                continue;
            };
            let EdgeProvenance::Engine {
                kind: relation_kind,
                detail: relation_detail,
            } = &advisory.edge.provenance
            else {
                // Every `PlacementExplanation::advisory` entry comes from
                // an engine edge (`sort/graph.rs::run`'s own
                // `advisory_edges`) — not a real input to guard against.
                continue;
            };
            findings.insert(
                FindingKey::DeclarationQuestioned {
                    declared_after: declared.after.clone(),
                    declared_before: declared.before.clone(),
                    relation_kind: *relation_kind,
                },
                Finding::DeclarationQuestioned {
                    declared_after: declared.after.clone(),
                    declared_before: declared.before.clone(),
                    declared_layer: declared.layer,
                    declared_detail: provenance_detail(&declared.provenance),
                    relation_kind: *relation_kind,
                    relation_detail: relation_detail.clone(),
                },
            );
        }
    }

    for choice in &sort_outcome.any_of_choices {
        findings.insert(
            FindingKey::AnyOfChoice {
                after: choice.after.clone(),
                assembly: choice.assembly.clone(),
            },
            Finding::AnyOfChoice {
                after: choice.after.clone(),
                assembly: choice.assembly.clone(),
                candidates: choice.candidates.clone(),
            },
        );
    }

    for conflict in &report.conflicts {
        // Off by default — see `rim_session::Settings::show_dangling_def_references`'s
        // own doc comment for the measured false-positive rate behind
        // this gate. Never affects the sorter: this finding carries no
        // ordering edge, so skipping it here only changes what shows up
        // in the ledger.
        if !show_dangling_def_references && matches!(conflict, Conflict::DanglingDefReference(_)) {
            continue;
        }
        insert_conflict(&mut findings, conflict, selected_order);
    }
    insert_keyed_translation_collisions(&mut findings, &report.conflicts);
    insert_undeclared_type_dependencies(&mut findings, report);

    for mod_id in &report.missing_mods {
        findings.insert(
            FindingKey::MissingMod {
                mod_id: mod_id.clone(),
            },
            Finding::MissingMod {
                mod_id: mod_id.clone(),
            },
        );
    }

    for missing in &report.missing_dependencies {
        findings.insert(
            FindingKey::MissingDependency {
                mod_id: missing.mod_id.clone(),
                dependency: missing.dependency.id.clone(),
            },
            Finding::MissingDependency {
                mod_id: missing.mod_id.clone(),
                dependency: missing.dependency.id.clone(),
                display_name: missing.dependency.display_name.clone(),
            },
        );
    }

    for pair in &report.incompatible_active_pairs {
        let sorted = sorted_pair(pair.a.clone(), pair.b.clone());
        findings.insert(
            FindingKey::IncompatiblePair {
                pair: sorted.clone(),
            },
            Finding::IncompatiblePair {
                a: sorted.0,
                b: sorted.1,
            },
        );
    }

    for mod_id in &report.unsupported_version_mods {
        findings.insert(
            FindingKey::UnsupportedVersion {
                mod_id: mod_id.clone(),
            },
            Finding::UnsupportedVersion {
                mod_id: mod_id.clone(),
            },
        );
    }

    for edge in &report.undeclared_hard_dependencies {
        findings.insert(
            FindingKey::UndeclaredHardDependency {
                after: edge.after.clone(),
                before: edge.before.clone(),
            },
            Finding::UndeclaredHardDependency {
                after: edge.after.clone(),
                before: edge.before.clone(),
                detail: edge.detail.clone(),
            },
        );
    }

    for edge_report in &report.edges {
        let edge = &edge_report.edge;
        if edge.strength() != EdgeStrength::Soft {
            continue;
        }
        if evaluate::edge_status(edge, selected_order) != EdgeStatus::Violated {
            // Either satisfied anyway, or unevaluated (one side isn't
            // active) — nothing to surface.
            continue;
        }
        let after = edge.after.base();
        let before = edge.before.base();
        findings.insert(
            FindingKey::LazyReferenceViolated {
                after: after.clone(),
                before: before.clone(),
            },
            Finding::LazyReferenceViolated {
                after,
                before,
                detail: edge.detail.clone(),
            },
        );
    }

    for assignment in tagging.assignments() {
        if let TagProvenance::Inferred { matched, .. } = &assignment.provenance {
            findings.insert(
                FindingKey::TagInferred {
                    mod_id: assignment.mod_id.clone(),
                    tag: assignment.tag.clone(),
                },
                Finding::TagInferred {
                    mod_id: assignment.mod_id.clone(),
                    tag: assignment.tag.clone(),
                    matched: matched.clone(),
                },
            );
        }
    }

    let generated = GeneratedMods::from_report(report);
    findings.retain(|key, _| !hidden_by_generated(key, &generated));

    findings
}
