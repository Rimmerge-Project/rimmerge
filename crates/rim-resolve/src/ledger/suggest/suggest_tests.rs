//! Tests for the confidence table.

use rim_analyzer::domain::{DefOverride, EdgeKind};

use super::*;
use crate::domain::{
    Action, Alternative, DefKey, EdgeWinner, Placement, PromotedRuleKey, Rationale, RuleOrigin,
};
use rim_analyzer::domain::{Conflict, PatchCollisionSeverity, Selector};

fn empty_ctx<'a>(
    report: &'a Report,
    sort_outcome: &'a SortOutcome,
    current: &'a LoadOrder,
    mods_by_id: &'a BTreeMap<ModId, &'a Mod>,
) -> SuggestContext<'a> {
    SuggestContext {
        report,
        sort_outcome,
        current,
        mods_by_id,
    }
}

fn empty_outcome(report: &Report) -> SortOutcome {
    crate::sort::sort(&crate::sort::SortInput {
        report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &crate::domain::Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &LoadOrder::new(Vec::new()),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    })
}

/// [`patch_collision`]'s suggestion for the `selector: DefName` collision at
/// `sub_path`, judged with `winner` as the contributor that runs last.
fn collision_suggestion(
    report: &Report,
    key: &DefKey,
    sub_path: Option<&str>,
    mods: &[ModId],
    winner: &ModId,
) -> crate::domain::Suggestion {
    let sort_outcome = empty_outcome(report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(mods.to_vec());
    let ctx = empty_ctx(report, &sort_outcome, &current, &mods_by_id);
    patch_collision(
        &CollisionTarget {
            key,
            selector: Selector::DefName,
            sub_path,
            mods,
            winner,
        },
        &ctx,
    )
}

#[test]
fn edge_dropped_hard_edge_gets_confidence_twenty() {
    let suggestion = edge_dropped(
        EdgeKind::ForceLoadAfter,
        EdgeStrength::Hard,
        &ModId::new("a"),
        &ModId::new("b"),
        None,
    );
    assert_eq!(suggestion.confidence.percent(), 20);
    assert_eq!(
        suggestion.action,
        Action::DropEdge {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind: EdgeKind::ForceLoadAfter
        }
    );
}

#[test]
fn edge_dropped_awareness_edge_gets_confidence_ninety() {
    let suggestion = edge_dropped(
        EdgeKind::MayRequire,
        EdgeStrength::Awareness,
        &ModId::new("a"),
        &ModId::new("b"),
        None,
    );
    assert_eq!(suggestion.confidence.percent(), 90);
}

/// A dropped `PatchRemovedNodeCosmetic` edge auto-accepts at 95 with no
/// `Reorder` alternative, regardless of `winner`/`strength` — a
/// deliberate design choice: the final defs are identical either order,
/// so there is nothing to weigh or reorder against.
#[test]
fn dropped_cosmetic_edge_suggests_accept_95_without_reorder() {
    let suggestion = edge_dropped(
        EdgeKind::PatchRemovedNodeCosmetic,
        EdgeStrength::Inferred,
        &ModId::new("a"),
        &ModId::new("b"),
        None,
    );
    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(
        suggestion.alternatives.is_empty(),
        "a cosmetic drop offers no Reorder alternative"
    );
}

/// Even with a `winner` supplied (the shape every other kind branches on
/// first), the cosmetic special case still short-circuits to the same
/// fixed Accept/95/no-alternatives shape.
#[test]
fn dropped_cosmetic_edge_ignores_a_supplied_winner() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::Declared,
        detail: "loadAfter".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::PatchRemovedNodeCosmetic,
        EdgeStrength::Inferred,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(suggestion.alternatives.is_empty());
}

/// The bug this field exists to prevent: `EdgeKind::AssemblyRef`'s own
/// `strength()` baseline is always `Hard` (see `rim-analyzer`'s
/// `edge.rs`) — only the specific `Edge`'s `load_time` flag actually
/// distinguishes a lazily-resolved (`Soft`) reference from a
/// load-time one, and that flag doesn't travel with a dropped edge.
/// Branching on `kind.strength()` instead of the layer-derived
/// `strength` field would score every dropped `AssemblyRef` — Soft or
/// Hard alike — as if it were Hard (confidence 20, "genuinely in
/// conflict"), even one dropped only because the caller opted in to
/// enforcing `Soft` for a session and it happened to lose a cycle.
#[test]
fn a_dropped_soft_assembly_ref_is_not_scored_as_hard() {
    let suggestion = edge_dropped(
        EdgeKind::AssemblyRef,
        EdgeStrength::Soft,
        &ModId::new("a"),
        &ModId::new("b"),
        None,
    );
    assert_eq!(
        suggestion.confidence.percent(),
        90,
        "a Soft-strength drop must be scored as Soft, not Hard, regardless of EdgeKind alone"
    );
}

/// A `Hard` winner gets confidence 95 regardless of what the
/// loser's own strength was. A `Hard` winner
/// never offers `Reorder` — only a db-rule winner does (see
/// `is_db_layer`) — so the only alternative is the usual `KeepEdge`.
#[test]
fn edge_dropped_with_a_hard_winner_gets_confidence_ninety_five_and_no_reorder() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::Hard,
        detail: "ships a load-time AssemblyRef".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::ModDependency,
        EdgeStrength::Declared,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a Hard winner must never offer a one-click Reorder against a load-time fact: {:?}",
        suggestion.alternatives
    );
}

/// The declared-edge override: a
/// `Layer::DeclaredOverride` winner joins `Hard`/`AnyOf`'s own
/// unconditional 95 bucket — it never faces a same-layer engine-edge
/// tie the way `Declared`/`UserDecision` must guard against (no
/// engine edge is ever added at that layer), so every loser it
/// actually beats is a genuine, deliberate cross-layer win.
#[test]
fn edge_dropped_with_a_declared_override_winner_gets_confidence_ninety_five() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::DeclaredOverride,
        detail: "your own declared-edge override".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::LoadAfter,
        EdgeStrength::Declared,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(suggestion.confidence.percent(), 95);
}

/// An imported-rule (`db`) winner overruling a `Declared` loser
/// gets confidence 70 ("db-over-declared"). A db-rule winner is
/// the one case that *does* offer `Reorder` to side with the loser.
#[test]
fn edge_dropped_with_a_db_winner_over_a_declared_loser_gets_confidence_seventy_and_offers_reorder()
{
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::SteamDb,
        detail: "a Steam Workshop dependency".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::ModDependency,
        EdgeStrength::Declared,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(suggestion.confidence.percent(), 70);
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            }),
        "a db winner must offer Reorder to side with the loser: {:?}",
        suggestion.alternatives
    );
}

/// A db winner over a merely-advisory (`Soft`/`Awareness`) loser
/// keeps the base table's higher 90 confidence rather than being
/// pulled down to the "db-over-declared" 70 row.
#[test]
fn edge_dropped_with_a_db_winner_over_an_awareness_loser_keeps_confidence_ninety() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::SteamDb,
        detail: "a Steam Workshop dependency".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::MayRequire,
        EdgeStrength::Awareness,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(
        suggestion.confidence.percent(),
        90,
        "a Soft/Awareness loser must keep the base table's 90 row regardless of the winner"
    );
}

/// A same-layer `Declared`-vs-`Declared`
/// contradiction — broken only by the drop tie-break, not by one
/// author's word out-ranking the other's — must fall through to the
/// base table's 55 ("needs input") row rather than being scored as a
/// confident 95 win.
#[test]
fn edge_dropped_with_a_declared_winner_over_a_declared_loser_stays_needs_input() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::Declared,
        detail: "b declares loadAfter a".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::ModDependency,
        EdgeStrength::Declared,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(
        suggestion.confidence.percent(),
        55,
        "author vs author decided by tie-break is not evidence"
    );
    assert_eq!(
        suggestion.alternatives,
        vec![Alternative {
            action: Action::KeepEdge {
                after: ModId::new("a"),
                before: ModId::new("b"),
                kind: EdgeKind::ModDependency,
            },
            rationale: Rationale::KeepEdgeInCycle,
        }]
    );
}

/// A `Declared`/`UserDecision` winner over a strictly weaker
/// (`Soft`/`Awareness`) loser is still a real precedence win, scored
/// at 95 — only a same-strength `Declared` loser falls back to the
/// base table.
#[test]
fn edge_dropped_with_a_declared_winner_over_an_awareness_loser_gets_confidence_ninety_five() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::Declared,
        detail: "b declares loadAfter a".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::MayRequire,
        EdgeStrength::Awareness,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a Declared winner must never offer a one-click Reorder against an author's own declaration: {:?}",
        suggestion.alternatives
    );
}

/// A `Soft`/`Awareness` winner falls through to the existing
/// strength-based table — there is no special case for it.
#[test]
fn edge_dropped_with_a_soft_winner_falls_through_to_the_existing_table() {
    let winner = EdgeWinner {
        after: ModId::new("b"),
        before: ModId::new("a"),
        layer: Layer::Soft,
        detail: "a lazily-resolved AssemblyRef".to_string(),
    };
    let suggestion = edge_dropped(
        EdgeKind::MayRequire,
        EdgeStrength::Awareness,
        &ModId::new("a"),
        &ModId::new("b"),
        Some(&winner),
    );
    assert_eq!(
        suggestion.confidence.percent(),
        90,
        "must fall back to the Awareness-strength row, unaffected by the Soft winner"
    );
}

/// `DeclarationQuestioned`'s `Reorder`
/// alternative must never be offered against an author's `Declared`
/// order — only against an imported db rule.
#[test]
fn declaration_questioned_against_a_declared_layer_offers_no_reorder() {
    let suggestion = declaration_questioned(
        &ModId::new("a"),
        &ModId::new("b"),
        Layer::Declared,
        "a declares loadAfter b",
        EdgeKind::FindMod,
        "a's patch checks for b via FindMod",
    );
    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 85);
    assert_eq!(
        suggestion.alternatives,
        Vec::new(),
        "a Declared layer must never offer a one-click Reorder against the author's own order"
    );
}

/// A db-rule layer (below `UserDecision`) is the one case
/// `DeclarationQuestioned` offers `Reorder` for.
#[test]
fn declaration_questioned_against_a_db_layer_offers_reorder() {
    let suggestion = declaration_questioned(
        &ModId::new("a"),
        &ModId::new("b"),
        Layer::RimSortCommunity,
        "a RimSort community rule",
        EdgeKind::FindMod,
        "a's patch checks for b via FindMod",
    );
    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 85);
    assert_eq!(
        suggestion.alternatives,
        vec![Alternative {
            action: Action::Reorder {
                after: ModId::new("b"),
                before: ModId::new("a"),
            },
            rationale: Rationale::ReorderOppositeOfRelation,
        }]
    );
}

#[test]
fn lazy_reference_violated_gets_confidence_ninety_and_offers_reorder() {
    let suggestion = lazy_reference_violated(&ModId::new("a"), &ModId::new("b"));
    assert_eq!(suggestion.confidence.percent(), 90);
    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(
        suggestion.alternatives,
        vec![Alternative {
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            rationale: Rationale::PinRelationExplicitlyAnyway,
        }]
    );
}

#[test]
fn missing_mod_meets_the_default_eighty_threshold() {
    let suggestion = missing_mod(&ModId::new("gone"));
    let threshold = Confidence::new(80).unwrap_or_else(|_| unreachable!());
    assert!(suggestion.confidence.meets(threshold));
}

#[test]
fn missing_dependency_does_not_meet_the_default_threshold() {
    let suggestion = missing_dependency();
    let threshold = Confidence::new(80).unwrap_or_else(|_| unreachable!());
    assert!(!suggestion.confidence.meets(threshold));
}

#[test]
fn def_override_winner_declares_relation_gets_confidence_ninety_five() {
    let report = {
        let mut r = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_with("b", |m| m.declared.load_after = vec![ModId::new("a")])
            .build();
        r.conflicts.push(Conflict::DefOverride(DefOverride {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
            overrides_vanilla: false,
            same_author: false,
        }));
        r
    };
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);

    let suggestion = def_override(
        &DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        &[ModId::new("a"), ModId::new("b")],
        &ModId::new("b"),
        &ctx,
    );

    assert_eq!(suggestion.confidence.percent(), 95);
    assert_eq!(suggestion.action, Action::Accept);
}

fn merge_action(key: &DefKey) -> Action {
    Action::Merge {
        key: key.clone(),
        choices: BTreeMap::new(),
    }
}

#[test]
fn def_override_shadows_framework_offers_a_merge_alternative() {
    let report = {
        let mut r = crate::test_support::ReportBuilder::new()
            .mod_with("framework", |m| m.is_framework_candidate = true)
            .mod_("leaf")
            .build();
        r.conflicts.push(Conflict::DefOverride(DefOverride {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            owners: vec![ModId::new("framework"), ModId::new("leaf")],
            overrides_vanilla: false,
            same_author: false,
        }));
        r
    };
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("framework"), ModId::new("leaf")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };

    let suggestion = def_override(
        &key,
        &[ModId::new("framework"), ModId::new("leaf")],
        &ModId::new("leaf"),
        &ctx,
    );

    // Pins the action too, not just the confidence and the Merge
    // alternative: a future change turning this branch's own action into
    // `Accept` would ship past every other test here green, silently
    // reintroducing exactly what `redecide_for_clean_merge`'s own
    // `ShadowsFramework` special-case exists to guard against.
    assert_eq!(
        suggestion.action,
        Action::PreferWinner {
            key: key.clone(),
            winner: ModId::new("framework"),
        }
    );
    assert_eq!(suggestion.confidence.percent(), 40);
    assert!(
        suggestion
            .alternatives
            .iter()
            .any(|alt| alt.action == merge_action(&key)),
        "shadows_framework must offer a Merge alternative: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn def_override_with_no_strong_signal_offers_a_merge_alternative() {
    let report = crate::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .build();
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };

    // No `Conflict::DefOverride` entry in the report at all: the
    // "unexplained" early-return path.
    let suggestion = def_override(
        &key,
        &[ModId::new("a"), ModId::new("b")],
        &ModId::new("b"),
        &ctx,
    );

    assert_eq!(suggestion.confidence.percent(), 60);
    assert!(
        suggestion
            .alternatives
            .iter()
            .any(|alt| alt.action == merge_action(&key)),
        "an unexplained def override must offer a Merge alternative: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn patch_collision_contested_lists_merge_first_among_alternatives() {
    let report = {
        let mut r = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .build();
        r.conflicts.push(Conflict::PatchCollision(
            rim_analyzer::domain::PatchCollision {
                def_type: "BiomeDef".to_string(),
                def_name: "TemperateForest".to_string(),
                selector: rim_analyzer::domain::Selector::DefName,
                sub_path: Some("plantDensity".to_string()),
                mods: vec![
                    rim_analyzer::domain::PatchCollisionEntry {
                        mod_id: ModId::new("a"),
                        op_class: "PatchOperationReplace".to_string(),
                    },
                    rim_analyzer::domain::PatchCollisionEntry {
                        mod_id: ModId::new("b"),
                        op_class: "PatchOperationReplace".to_string(),
                    },
                ],
                severity: PatchCollisionSeverity::Contested,
                removed_by: Vec::new(),
            },
        ));
        r
    };
    let key = DefKey {
        def_type: "BiomeDef".to_string(),
        def_name: "TemperateForest".to_string(),
    };

    let suggestion = collision_suggestion(
        &report,
        &key,
        Some("plantDensity"),
        &[ModId::new("a"), ModId::new("b")],
        &ModId::new("b"),
    );

    assert_eq!(suggestion.confidence.percent(), 50);
    assert_eq!(
        suggestion.alternatives.first().map(|alt| &alt.action),
        Some(&merge_action(&key)),
        "Merge must be listed first among a contested collision's alternatives: {:?}",
        suggestion.alternatives
    );
}

/// A `label` collision between `op_classes`' mods in the order given. When
/// `last_declares_the_others`, the last mod declares `loadAfter` on every
/// other contributor.
fn label_collision_report(
    last_declares_the_others: bool,
    op_classes: &[(&str, &str)],
) -> (Report, DefKey, Vec<ModId>) {
    let ids: Vec<ModId> = op_classes.iter().map(|(id, _)| ModId::new(*id)).collect();
    let mut builder = crate::test_support::ReportBuilder::new();
    for (index, mod_id) in ids.iter().enumerate() {
        let declares = last_declares_the_others && index + 1 == ids.len();
        let others: Vec<ModId> = ids.iter().filter(|id| *id != mod_id).cloned().collect();
        builder = builder.mod_with(mod_id.as_str(), |m| {
            if declares {
                m.declared.load_after = others;
            }
        });
    }
    let mut report = builder.build();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "ExampleDef".to_string(),
            selector: Selector::DefName,
            sub_path: Some("label".to_string()),
            mods: op_classes
                .iter()
                .map(
                    |(mod_id, op_class)| rim_analyzer::domain::PatchCollisionEntry {
                        mod_id: ModId::new(*mod_id),
                        op_class: (*op_class).to_string(),
                    },
                )
                .collect(),
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        },
    ));
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "ExampleDef".to_string(),
    };
    let mods = op_classes.iter().map(|(id, _)| ModId::new(*id)).collect();
    (report, key, mods)
}

#[test]
fn a_contested_collision_whose_winner_declares_the_relation_is_accepted_at_eighty() {
    let (report, key, mods) = label_collision_report(
        true,
        &[
            ("a", "PatchOperationReplace"),
            ("b", "PatchOperationReplace"),
            ("c", "PatchOperationReplace"),
        ],
    );

    let suggestion =
        collision_suggestion(&report, &key, Some("label"), &mods, &mods[mods.len() - 1]);

    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 80);
    assert_eq!(
        suggestion.rationale,
        Rationale::PatchCollisionWinnerDeclaresRelation {
            winner: ModId::new("c"),
            others: vec![ModId::new("a"), ModId::new("b")],
            field: "label".to_string(),
        }
    );
    assert_eq!(
        suggestion.alternatives.first().map(|alt| &alt.action),
        Some(&merge_action(&key)),
        "Merge must stay the first alternative: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn a_contested_collision_without_the_flag_stays_at_fifty() {
    let (report, key, mods) = label_collision_report(
        false,
        &[
            ("a", "PatchOperationReplace"),
            ("b", "PatchOperationReplace"),
        ],
    );

    let suggestion =
        collision_suggestion(&report, &key, Some("label"), &mods, &mods[mods.len() - 1]);

    assert_eq!(suggestion.confidence.percent(), 50);
    assert_eq!(suggestion.rationale, Rationale::PatchCollisionContested);
}

#[test]
fn an_accepted_declared_winner_still_collapses_to_ninety_five_on_a_zero_op_merge() {
    let (report, key, mods) = label_collision_report(
        true,
        &[
            ("a", "PatchOperationReplace"),
            ("b", "PatchOperationReplace"),
        ],
    );
    let suggestion =
        collision_suggestion(&report, &key, Some("label"), &mods, &mods[mods.len() - 1]);

    let redecided = crate::domain::redecide_for_clean_merge(
        suggestion,
        &key,
        &crate::domain::MergeState::Complete { op_count: 0 },
        false,
        None,
        crate::domain::MergeFindingKind::PatchCollision,
    );

    assert_eq!(redecided.confidence.percent(), 95);
    assert_eq!(redecided.rationale, Rationale::MergeCompleteNothingToMerge);
}

/// The real `GeneDef/Iris_Red` shape —
/// one def carrying both an additive whole-def collision and a
/// contested sub-path collision. Conflicts are ordered by `(def_type,
/// def_name, selector, sub_path)`, so the additive, whole-def one
/// (`sub_path: None`) sorts before the contested `renderNodeProperties`
/// one; a lookup that ignores `selector`/`sub_path` takes the first
/// match and wrongly hands the contested finding the additive 95
/// rationale.
#[test]
fn severity_lookup_matches_the_exact_selector_and_sub_path() {
    let mut report = crate::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .build();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "GeneDef".to_string(),
            def_name: "Iris_Red".to_string(),
            selector: Selector::DefName,
            sub_path: None,
            mods: vec![
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("a"),
                    op_class: "PatchOperationAdd".to_string(),
                },
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("b"),
                    op_class: "PatchOperationAdd".to_string(),
                },
            ],
            severity: PatchCollisionSeverity::Additive,
            removed_by: Vec::new(),
        },
    ));
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "GeneDef".to_string(),
            def_name: "Iris_Red".to_string(),
            selector: Selector::DefName,
            sub_path: Some("renderNodeProperties".to_string()),
            mods: vec![
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("a"),
                    op_class: "PatchOperationReplace".to_string(),
                },
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("b"),
                    op_class: "PatchOperationReplace".to_string(),
                },
            ],
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        },
    ));
    let key = DefKey {
        def_type: "GeneDef".to_string(),
        def_name: "Iris_Red".to_string(),
    };
    let mods = [ModId::new("a"), ModId::new("b")];

    let additive = collision_suggestion(&report, &key, None, &mods, &mods[1]);
    assert_eq!(
        additive.confidence.percent(),
        95,
        "the whole-def additive collision must keep its own 95-confidence rationale"
    );

    let contested =
        collision_suggestion(&report, &key, Some("renderNodeProperties"), &mods, &mods[1]);
    assert_eq!(
        contested.confidence.percent(),
        50,
        "the contested sub-path collision must not inherit the additive collision's severity"
    );
    assert_eq!(
        contested.alternatives.first().map(|alt| &alt.action),
        Some(&merge_action(&key)),
        "Merge must lead the contested collision's alternatives: {:?}",
        contested.alternatives
    );
}

#[test]
fn texture_override_offers_a_ship_asset_alternative_per_owner() {
    let suggestion = texture_override("Things/Wall.png", &[ModId::new("a"), ModId::new("b")]);

    for owner in [ModId::new("a"), ModId::new("b")] {
        assert!(
            suggestion.alternatives.iter().any(|alt| alt.action
                == Action::ShipAsset {
                    texture_path: "Things/Wall.png".to_string(),
                    from: owner.clone(),
                }),
            "expected a ShipAsset alternative for {owner}: {:?}",
            suggestion.alternatives
        );
    }
}

// `ResolutionStatus` derivation itself (confidence vs. threshold,
// decision overrides) is `ledger::build::build`'s own rule, not
// `suggest`'s — see `ledger::build::tests` for end-to-end coverage
// that actually calls `build()`, rather than reimplementing the rule
// inline against a bare `Suggestion` the way a test here would have
// to.

// -- per-target runtime-patch grouping --------------------

#[test]
fn runtime_patch_collision_offers_a_prefer_winner_alternative_per_owner() {
    let report = crate::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .build();
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);

    let suggestion = runtime_patch_collision(
        "Verse.Pawn",
        "Kill",
        &[ModId::new("a"), ModId::new("b")],
        &ctx,
    );

    assert_eq!(suggestion.confidence.percent(), 85);
    for owner in [ModId::new("a"), ModId::new("b")] {
        assert!(
            suggestion.alternatives.iter().any(|alt| alt.action
                == Action::PreferWinner {
                    key: DefKey::synthesize_for_runtime_target("Verse.Pawn", "Kill"),
                    winner: owner.clone(),
                }),
            "expected a PreferWinner alternative for {owner}: {:?}",
            suggestion.alternatives
        );
    }
}

#[test]
fn runtime_patch_collision_names_the_last_loaded_owner_as_the_current_winner() {
    let report = crate::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .build();
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    // `b` loads after `a` today, so `b`'s patch runs last.
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);

    let suggestion = runtime_patch_collision(
        "Verse.Pawn",
        "Kill",
        &[ModId::new("a"), ModId::new("b")],
        &ctx,
    );

    assert!(
        suggestion.rationale.to_string().contains("b"),
        "the rationale must name the last-loaded owner: {}",
        suggestion.rationale
    );
}

// -- TranspilerCollision ------------------

#[test]
fn transpiler_collision_offers_no_alternative_and_names_every_owner() {
    let suggestion = transpiler_collision(
        "Verse.Verb_LaunchProjectile",
        "TryCastShot",
        &[
            ModId::new("example.turrets"),
            ModId::new("example.vehiclemap"),
            ModId::new("example.weavecore"),
        ],
    );

    assert_eq!(
        suggestion.confidence.percent(),
        55,
        "below runtime_patch_collision's 85 — this is a fragility warning, not a resolved fact"
    );
    assert_eq!(suggestion.action, Action::Accept);
    assert!(
        suggestion.alternatives.is_empty(),
        "no direction should be asserted — the right transpiler order isn't derivable here"
    );
    assert!(
        suggestion.rationale.to_string().contains('3'),
        "{}",
        suggestion.rationale
    );
    assert!(
        suggestion.rationale.to_string().contains("rule set-pair"),
        "{}",
        suggestion.rationale
    );
}

// -- RuleOverruled --------------------------------------------

fn edge_winner(after: &str, before: &str, layer: Layer) -> EdgeWinner {
    EdgeWinner {
        after: ModId::new(after),
        before: ModId::new(before),
        layer,
        detail: "a test edge".to_string(),
    }
}

#[test]
fn rule_overruled_by_hard_gets_confidence_ninety_five_and_no_reorder() {
    let winner = edge_winner("b", "a", Layer::Hard);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortCommunity,
        Some(&winner),
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a Hard winner must never offer Reorder: {:?}",
        suggestion.alternatives
    );
}

/// The declared-edge override: a
/// `Layer::DeclaredOverride` winner beating a *plain* rule
/// (`loser_overrides_declared: false`) is a genuine, asymmetric
/// cross-layer win — same 90 bucket as `Declared`, and `Reorder` is
/// never offered (mirrors the `Declared`-winner test below: that
/// layer sits ahead of `UserDecision` too, so reasserting the loser
/// there can never win).
#[test]
fn rule_overruled_by_declared_override_beats_a_plain_rule_at_ninety() {
    let winner = edge_winner("b", "a", Layer::DeclaredOverride);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortCommunity,
        Some(&winner),
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 90);
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a DeclaredOverride winner sits above every rule-origin layer, so Reorder can never \
             beat it: {:?}",
        suggestion.alternatives
    );
}

/// The tie case the flag exists to catch: two declared-edge-override
/// rules directly contradicting each other are both at
/// `Layer::DeclaredOverride` — the drop tie-break decided it, not one
/// override actually outranking the other, so this folds to the same
/// base-table 55 the `UserDecision`-vs-`UserDecision` tie gets.
#[test]
fn rule_overruled_by_declared_override_over_another_override_is_a_same_layer_tie() {
    let winner = edge_winner("b", "a", Layer::DeclaredOverride);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::UserDecision,
        Some(&winner),
        true, // the losing rule is itself override-flagged.
    );

    assert_eq!(suggestion.confidence.percent(), 55);
}

/// [`declaration_overridden`]'s own shape: a flat 95, disclosure only
/// (no alternative) — mirroring `placement_ordering_overridden`.
#[test]
fn declaration_overridden_scores_ninety_five_with_no_alternative() {
    let winner = edge_winner("b", "a", Layer::DeclaredOverride);
    let suggestion = declaration_overridden(
        &ModId::new("a"),
        &ModId::new("b"),
        rim_analyzer::domain::EdgeKind::LoadAfter,
        "a mod dependency",
        &winner,
    );

    assert_eq!(suggestion.confidence.percent(), 95);
    assert_eq!(suggestion.action, Action::Accept);
    assert!(
        suggestion.alternatives.is_empty(),
        "disclosure only, per PlacementOrderingOverridden's own precedent: {:?}",
        suggestion.alternatives
    );
    let rationale_text = suggestion.rationale.to_string();
    assert!(
        rationale_text.contains('b') && rationale_text.contains('a'),
        "{}",
        suggestion.rationale
    );
}

/// A `Declared` winner sits
/// *above* `UserDecision` in `LAYER_ORDER`, so reasserting the losing
/// rule as a fresh `UserDecision` edge (`Reorder`) can never beat it —
/// a self-defeating no-op that would just spawn a second, identical
/// finding next scan. `Promote` still makes sense (it never claims to
/// beat the winner, only to survive the import toggles), so it must
/// still be offered.
#[test]
fn rule_overruled_by_declared_never_offers_reorder_but_still_offers_promote() {
    let winner = edge_winner("b", "a", Layer::Declared);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortCommunity,
        Some(&winner),
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 90);
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a Declared winner sits above UserDecision in LAYER_ORDER, so Reorder can never \
             beat it: {:?}",
        suggestion.alternatives
    );
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::PromoteRule {
                rule: PromotedRuleKey::Pair {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
            }),
        "an imported rule must still offer Promote: {:?}",
        suggestion.alternatives
    );
}

/// A `UserDecision` winner is the other self-defeating case —
/// reasserting the loser as a *second* `UserDecision` edge on the same
/// pair just pits one user decision against another at the same
/// layer, which the tie-break resolves arbitrarily rather than
/// actually enforcing the reassertion.
#[test]
fn rule_overruled_by_a_user_decision_winner_offers_no_reorder() {
    let winner = edge_winner("b", "a", Layer::UserDecision);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortCommunity,
        Some(&winner),
        false,
    );

    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::Reorder { .. })),
        "a UserDecision winner must never offer Reorder: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn rule_overruled_by_db_gets_confidence_seventy_and_offers_reorder() {
    let winner = edge_winner("b", "a", Layer::SteamDb);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortUser,
        Some(&winner),
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 70);
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            }),
        "a db-layer winner must still offer Reorder: {:?}",
        suggestion.alternatives
    );
}

/// A `UserDecision` winner over a `UserDecision` loser is
/// the same same-layer-tie shape `EdgeDropped`'s `winner_confidence`
/// handles — the tie-break decided it, not one user
/// decision outranking another, so it must fall to 55 ("needs
/// input"), not the flat 90 a genuine `UserDecision`-over-db win gets.
#[test]
fn rule_overruled_by_a_user_decision_winner_over_a_user_decision_loser_needs_input() {
    let winner = edge_winner("b", "a", Layer::UserDecision);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::UserDecision,
        Some(&winner),
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 55);
}

/// A `UserDecision`-origin rule never offers `Promote` — it's
/// already the user's own decision, nothing imported to promote.
#[test]
fn rule_overruled_for_a_user_decision_origin_never_offers_promote() {
    let winner = edge_winner("b", "a", Layer::Declared);
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::UserDecision,
        Some(&winner),
        false,
    );

    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::PromoteRule { .. })),
        "a UserDecision-origin rule must never offer Promote: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn rule_overruled_with_no_winner_names_a_longer_cycle() {
    let suggestion = rule_overruled(
        &ModId::new("a"),
        &ModId::new("b"),
        RuleOrigin::RimSortCommunity,
        None,
        false,
    );

    assert_eq!(suggestion.confidence.percent(), 55);
    assert!(suggestion.alternatives.is_empty());
}

// -- PlacementOverruled / PlacementQuestioned ------------------

/// `Promote`'s gate must be consistent between
/// `rule_overruled` and `placement_overruled` — `rule_overruled`
/// already offers `Promote` regardless of the winner's layer (it
/// never claims to beat the winner, only to survive the import
/// toggles), so `placement_overruled` must too, even against a `Hard`
/// winner. `DropRule` stays gated to a db-layer winner (the only case
/// where dropping the rule that overruled the placement is a
/// sensible one-click fix).
#[test]
fn placement_overruled_by_hard_still_offers_promote_but_no_drop_rule() {
    let by = edge_winner("puller", "pinned", Layer::Hard);
    let suggestion = placement_overruled(
        &ModId::new("pinned"),
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
        &by,
    );

    assert_eq!(suggestion.confidence.percent(), 95);
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::PromoteRule {
                rule: PromotedRuleKey::Placement {
                    mod_id: ModId::new("pinned"),
                },
            }),
        "Promote never claims to beat the winner, so it must survive even against Hard: {:?}",
        suggestion.alternatives
    );
    assert!(
        !suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::DropRule { .. })),
        "a Hard winner is never itself a droppable pair rule: {:?}",
        suggestion.alternatives
    );
}

#[test]
fn placement_overruled_by_a_db_pair_rule_offers_promote_and_drop_rule() {
    let by = edge_winner("x", "pinned", Layer::RimSortUser);
    let suggestion = placement_overruled(
        &ModId::new("pinned"),
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
        &by,
    );

    assert_eq!(suggestion.confidence.percent(), 70);
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::PromoteRule {
                rule: PromotedRuleKey::Placement {
                    mod_id: ModId::new("pinned"),
                },
            }),
        "expected a Promote alternative: {:?}",
        suggestion.alternatives
    );
    assert!(
        suggestion.alternatives.iter().any(|alt| alt.action
            == Action::DropRule {
                after: ModId::new("x"),
                before: ModId::new("pinned"),
            }),
        "expected a DropRule alternative for the winning db pair rule: {:?}",
        suggestion.alternatives
    );
}

/// A promoted (`UserDecision`-origin) placement overruled by a
/// `Declared` edge: `Promote` is never offered (already the user's own
/// decision) and `DropRule` is never offered either (the winner isn't
/// a pair rule at all here, just an engine edge).
#[test]
fn placement_overruled_for_a_user_decision_origin_by_declared_offers_nothing() {
    let by = edge_winner("x", "pinned", Layer::Declared);
    let suggestion = placement_overruled(
        &ModId::new("pinned"),
        Placement::Bottom,
        RuleOrigin::UserDecision,
        &by,
    );

    assert_eq!(suggestion.confidence.percent(), 90);
    assert!(suggestion.alternatives.is_empty());
}

#[test]
fn placement_questioned_for_a_bottom_mod_reorders_the_relations_own_direction() {
    let suggestion = placement_questioned(
        &ModId::new("m"),
        Placement::Bottom,
        &ModId::new("x"),
        rim_analyzer::domain::EdgeKind::MayRequire,
        "x may require m",
    );

    assert_eq!(suggestion.confidence.percent(), 85);
    assert_eq!(
        suggestion.alternatives,
        vec![Alternative {
            action: Action::Reorder {
                after: ModId::new("x"),
                before: ModId::new("m"),
            },
            rationale: Rationale::EnforceRelationAsUserDecision,
        }]
    );
}

#[test]
fn placement_questioned_for_a_top_mod_reorders_the_relations_own_direction() {
    let suggestion = placement_questioned(
        &ModId::new("m"),
        Placement::Top,
        &ModId::new("x"),
        rim_analyzer::domain::EdgeKind::MayRequire,
        "m may require x",
    );

    assert_eq!(
        suggestion.alternatives,
        vec![Alternative {
            action: Action::Reorder {
                after: ModId::new("m"),
                before: ModId::new("x"),
            },
            rationale: Rationale::EnforceRelationAsUserDecision,
        }]
    );
}

/// `Accept` at confidence 70 (deliberately under the
/// 80 `Auto` threshold, since a false positive here costs the user a
/// mod) with no alternatives — disabling the mod is
/// the user's own act in their mod manager, not something this
/// engine offers a one-click action for.
#[test]
fn contributes_nothing_suggests_accept_at_seventy_with_no_alternatives() {
    let suggestion = contributes_nothing(&ModId::new("inert.mod"));

    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 70);
    assert!(suggestion.alternatives.is_empty());
    assert!(suggestion.rationale.to_string().contains("inert.mod"));
}

/// `Accept` at confidence 90, informational (no `Reorder`/`Merge`
/// alternative), naming the bad-texture placeholder — the same shape
/// `missing_texture_path`'s own rationale takes.
#[test]
fn undecodable_texture_suggests_accept_with_placeholder_rationale() {
    let suggestion = undecodable_texture();

    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 90);
    assert!(suggestion.alternatives.is_empty());
    assert!(suggestion.rationale.to_string().contains("placeholder"));
}

/// `DiscardedAddition` (the deliberate-override finding) scores exactly
/// 80 — the default `Auto` threshold — regardless of whether a
/// `PatchCollision` exists for the same def; per `docs/concepts/ledger.md`
/// this auto-accepts by default, since the replacer's own author already
/// chose this outcome on purpose. With no `PatchCollision` present for
/// the def, there's nothing to merge, so no `Merge` alternative is
/// offered.
#[test]
fn discarded_addition_without_a_patch_collision_scores_eighty_with_no_merge_alternative() {
    let report = crate::test_support::ReportBuilder::new()
        .mod_("adder")
        .mod_("replacer")
        .build();
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("adder"), ModId::new("replacer")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };

    let suggestion = discarded_addition(
        &ModId::new("replacer"),
        &ModId::new("adder"),
        &key,
        "comps",
        "comps/li[0]",
        &ctx,
    );

    assert_eq!(suggestion.action, Action::Accept);
    assert_eq!(suggestion.confidence.percent(), 80);
    assert!(
        suggestion
            .confidence
            .meets(crate::domain::Confidence::new(80).unwrap()),
        "80 must meet the default 80 Auto threshold, per docs/concepts/ledger.md"
    );
    assert!(suggestion.alternatives.is_empty());
}

/// The same finding, but a `PatchCollision` exists for the same def: the
/// user can still keep the discarded content field by field without
/// contradicting the replacer's own declared order, so a `Merge`
/// alternative is offered — confidence is unaffected.
#[test]
fn discarded_addition_with_a_patch_collision_offers_a_merge_alternative() {
    let mut report = crate::test_support::ReportBuilder::new()
        .mod_("adder")
        .mod_("replacer")
        .build();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: Some("comps".to_string()),
            mods: vec![
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("adder"),
                    op_class: "PatchOperationAdd".to_string(),
                },
                rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new("replacer"),
                    op_class: "PatchOperationReplace".to_string(),
                },
            ],
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        },
    ));
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("adder"), ModId::new("replacer")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };

    let suggestion = discarded_addition(
        &ModId::new("replacer"),
        &ModId::new("adder"),
        &key,
        "comps",
        "comps/li[0]",
        &ctx,
    );

    assert_eq!(suggestion.confidence.percent(), 80);
    assert!(
        suggestion
            .alternatives
            .iter()
            .any(|alt| alt.action == merge_action(&key)),
        "a DiscardedAddition with a PatchCollision for the same def must offer Merge: {:?}",
        suggestion.alternatives
    );
}

/// Current order `[w, a]` (so `a` wins today); `w` declares `loadAfter a`,
/// so the sorted `[a, w]` makes `w` the winner it asked to be.
fn declared_winner_report() -> Report {
    crate::test_support::ReportBuilder::new()
        .mod_with("w", |m| m.declared.load_after = vec![ModId::new("a")])
        .mod_("a")
        .build()
}

#[test]
fn a_def_override_is_explained_by_the_winner_of_the_order_it_is_judged_under() {
    let mut report = declared_winner_report();
    report.conflicts.push(Conflict::DefOverride(DefOverride {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
        owners: vec![ModId::new("w"), ModId::new("a")],
        overrides_vanilla: false,
        same_author: false,
    }));
    let sort_outcome = empty_outcome(&report);
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let current = LoadOrder::new(vec![ModId::new("w"), ModId::new("a")]);
    let ctx = empty_ctx(&report, &sort_outcome, &current, &mods_by_id);
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };
    let owners = [ModId::new("w"), ModId::new("a")];

    let under_current = def_override(&key, &owners, &ModId::new("a"), &ctx);
    let under_suggested = def_override(&key, &owners, &ModId::new("w"), &ctx);

    assert_eq!(under_current.confidence.percent(), 60);
    assert_eq!(under_suggested.confidence.percent(), 95);
}

#[test]
fn a_patch_collision_is_explained_by_the_winner_of_the_order_it_is_judged_under() {
    let mut report = declared_winner_report();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: Some("label".to_string()),
            mods: ["w", "a"]
                .into_iter()
                .map(|id| rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new(id),
                    op_class: "PatchOperationReplace".to_string(),
                })
                .collect(),
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        },
    ));
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };
    let mods = [ModId::new("w"), ModId::new("a")];

    let under_current = collision_suggestion(&report, &key, Some("label"), &mods, &mods[1]);
    let under_suggested = collision_suggestion(&report, &key, Some("label"), &mods, &mods[0]);

    assert_eq!(under_current.confidence.percent(), 50);
    assert_eq!(under_suggested.confidence.percent(), 80);
}

#[test]
fn a_remover_other_than_the_winner_keeps_a_declared_collision_at_fifty() {
    let mut report = crate::test_support::ReportBuilder::new()
        .mod_with("w", |m| m.declared.load_after = vec![ModId::new("a")])
        .mod_("a")
        .mod_("r")
        .build();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: Some("comps/li/label".to_string()),
            mods: ["a", "w"]
                .into_iter()
                .map(|id| rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new(id),
                    op_class: "PatchOperationReplace".to_string(),
                })
                .collect(),
            severity: PatchCollisionSeverity::Contested,
            removed_by: vec![ModId::new("r")],
        },
    ));
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };
    let mods = [ModId::new("a"), ModId::new("w")];

    let suggestion = collision_suggestion(&report, &key, Some("comps/li/label"), &mods, &mods[1]);

    assert_eq!(suggestion.confidence.percent(), 50);
}
