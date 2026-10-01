//! Tests for the def-conflict view.

use rim_analyzer::domain::{Edge, EdgeReport, EdgeStatus, ModId, Selector};
use rim_resolve::domain::{
    Action, Decision, DefKey, DefRef, FindingKey, MergeChoice, OrderSource, PairRule, Rule,
    RuleOrigin,
};

use super::rows::sort_field_rows;
use super::*;
use crate::merge_workspace::PreviewSlot;
use crate::test_support::{
    add_then_replace_map_key_fixture, add_then_replace_on_container_fixture,
    arid_shrubland_wild_animals_fixture, arid_shrubland_wild_animals_two_mod_fixture,
    bionic_heart_fixture, bionic_heart_fixture_flat,
    contested_field_patched_before_stopper_fixture, def_override_missing_template_fixture,
    def_override_missing_template_on_a_losing_owner_fixture, document_order_interleaved_fixture,
    duplicate_template_name_fixture, head_normal_fixture, identity_less_agreeing_list_item_fixture,
    identity_less_list_items_fixture, list_position_differs_from_lexical_order_fixture,
    list_with_owner_item_fixture, out_of_scope_patcher_agreeing_list_item_fixture,
    owner_agreeing_list_item_fixture, plant_density_fixture,
    plant_density_fixture_with_generated_patcher, replace_vs_replace_on_container_fixture,
    same_identity_agreeing_list_item_fixture, same_identity_list_item_fixture,
    session_with_sources, session_with_sources_and_mods, three_mod_agreeing_list_item_fixture,
    three_mod_list_fixture, three_mod_two_agree_one_differs_list_item_fixture,
    two_mod_list_fixture, unsupported_op_patch_collision_fixture,
    whole_def_patch_collision_fixture,
};
use crate::use_cases::{InspectDef, PlanMerge};
use rim_analyzer::domain::EdgeKind;
use rim_merge::diff::DiffClass;
use rim_merge::effective::EffectiveDef;
use rim_merge::tree::{Content, FieldTree};
use std::collections::{BTreeMap, BTreeSet};

fn bionic_heart_key() -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    }
}

fn bionic_heart_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        Selector::DefName,
    )
}

fn head_normal_key() -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: "HeadTypeDef".to_string(),
            def_name: "HeadNormal".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    }
}

fn head_normal_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "HeadTypeDef".to_string(),
            def_name: "HeadNormal".to_string(),
        },
        Selector::DefName,
    )
}

fn plant_density_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        Selector::DefName,
    )
}

fn plant_density_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".to_string()),
        mods: [ModId::new("x.mod"), ModId::new("y.mod")]
            .into_iter()
            .collect(),
    }
}

fn arid_shrubland_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        Selector::DefName,
    )
}

/// [`arid_shrubland_wild_animals_fixture`]'s own key — every
/// contributor.
fn arid_shrubland_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".to_string()),
        mods: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    }
}

/// [`arid_shrubland_wild_animals_two_mod_fixture`]'s own key —
/// `a.mod`/`b.mod` only.
fn arid_shrubland_two_mod_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".to_string()),
        mods: [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect(),
    }
}

fn widget_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    )
}

fn widget_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("label".to_string()),
        mods: [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect(),
    }
}

/// The analyzer's own keying for `two_mod_list_fixture`'s two
/// `PatchOperationAdd`s on `comps`:
/// `rim_analyzer::analysis::conflicts::patch_collisions` groups
/// a collision by the op's own xpath *target*, so two adds under
/// `ThingDef/Widget/comps` collide at `sub_path: Some("comps")`, not
/// `"label"` — `widget_key`'s own `sub_path` is not a key the
/// analyzer would actually produce for this fixture's `comps` list.
fn widget_comps_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect(),
    }
}

fn gadget_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gadget".to_string(),
        },
        Selector::DefName,
    )
}

fn gadget_comps_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gadget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    }
}

fn sprocket_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket".to_string(),
        },
        Selector::DefName,
    )
}

fn sprocket_comps_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [ModId::new("z.mod"), ModId::new("a.mod")]
            .into_iter()
            .collect(),
    }
}

fn wall2_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        Selector::DefName,
    )
}

fn contested_field_before_stopper_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("label".to_string()),
        mods: [ModId::new("c.mod"), ModId::new("y.mod")]
            .into_iter()
            .collect(),
    }
}

fn wall_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        Selector::DefName,
    )
}

fn wall_collision_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("label".to_string()),
        mods: [ModId::new("c.mod"), ModId::new("d.mod")]
            .into_iter()
            .collect(),
    }
}

/// BionicHeart: every non-`Unchanged` field is `CleanMerge` or
/// `ListEntry`, `after_merge` names the differing owner (the winner,
/// with only two owners) and `in_game` names the winner too — no
/// problems, since this def has no patchers at all.
///
/// Uses [`bionic_heart_fixture_flat`], not [`bionic_heart_fixture`]
/// itself: BIONICS's own `ParentName` genuinely differs from Core's on
/// that fixture, so the structural guard fires and its preview is
/// permanently `NeedsFieldInput` — `after_merge` is gated on the cached
/// preview being `Complete` (`build_def_override_fields`/`after_tree`),
/// so it would never populate for that fixture at all.
/// `bionic_heart_fixture_flat` keeps the real inheritance content, and
/// with it this test's inherited-field coverage (`defaultLabelColor`'s
/// `CleanMerge` row, `comps`'s `ListEntry` one) — the guard fires on a
/// *differing* `ParentName`, not merely on having one (see the fixture's
/// own doc comment for the duplicate-`Name`-registrant mechanism that
/// makes both true at once).
#[test]
fn bionic_heart_is_all_clean_merge_with_after_merge_and_in_game_naming_the_winner() {
    let fixture = bionic_heart_fixture_flat();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert_eq!(view.kind, DefConflictKind::DefOverride);
    assert!(view.problems.is_empty());
    let winner = ModId::new("example.bionicsfork");
    let mut saw_a_row = false;
    for row in view
        .fields
        .iter()
        .filter(|r| r.kind != FieldRowKind::Unchanged)
    {
        saw_a_row = true;
        // Every differing field is either a plain leaf (`CleanMerge`)
        // or the one `li` item BIONICS's own template chain contributes
        // (`ListEntry` — still a clean, uncontested toucher, just a
        // list item rather than a leaf); neither this fixture nor a
        // two-owner def override can ever produce a `Conflict`.
        assert!(
            matches!(row.kind, FieldRowKind::CleanMerge | FieldRowKind::ListEntry),
            "{:?}: {:?}",
            row.path,
            row.kind
        );
        let (in_game_mod, _) = row
            .in_game
            .as_ref()
            .unwrap_or_else(|| panic!("{:?} must have in_game populated", row.path));
        assert_eq!(in_game_mod, &winner, "{:?}", row.path);
        let (after_mod, _) = row.after_merge.as_ref().unwrap_or_else(|| {
            panic!(
                "{:?} must have after_merge populated (preview is Complete)",
                row.path
            )
        });
        assert_eq!(after_mod, &winner, "{:?}", row.path);
        assert_eq!(row.preference, Preference::None);
    }
    assert!(
        saw_a_row,
        "the fixture must have at least one differing field"
    );
}

/// Builds an [`EdgeReport`] for a `PatchSelectsInjectedNode` edge —
/// `status`/`load_time` are meaningless for this awareness kind (see
/// `Edge`'s own doc comment), so every test below fixes them to an
/// arbitrary constant rather than threading them through.
fn patch_selects_injected_node_edge(after: &str, before: &str, subject: &str) -> EdgeReport {
    EdgeReport {
        edge: Edge {
            after: ModId::new(after),
            before: ModId::new(before),
            kind: EdgeKind::PatchSelectsInjectedNode,
            detail: format!("patch selects '{subject}', injected by {before}"),
            load_time: true,
            subject: Some(subject.to_string()),
        },
        status: EdgeStatus::Unevaluated,
    }
}

/// A `PatchSelectsInjectedNode` edge whose own `subject` names exactly
/// this def (the whole-`<Defs>`-root shape, no sub-path) is surfaced —
/// the relation shows at the def the user is already looking at, not as
/// a fresh inbox finding.
#[test]
fn surfaces_a_whole_def_injected_node_relation() {
    let mut fixture = bionic_heart_fixture_flat();
    fixture.report.edges.push(patch_selects_injected_node_edge(
        "ludeon.rimworld",
        "example.bionicsfork",
        "HediffDef/BionicHeart",
    ));
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert_eq!(
        view.injected_node_relations,
        vec![InjectedNodeRelation {
            selector: ModId::new("ludeon.rimworld"),
            injector: ModId::new("example.bionicsfork"),
            subject: "HediffDef/BionicHeart".to_string(),
        }]
    );
}

/// A `PatchSelectsInjectedNode` edge naming a *sub-path* beneath this
/// def (`"{def_type}/{def_name}/{sub_path}"`, the element-injection
/// shape) is surfaced too — not only the bare whole-def shape the
/// previous test pins.
#[test]
fn surfaces_a_sub_path_injected_node_relation() {
    let mut fixture = bionic_heart_fixture_flat();
    fixture.report.edges.push(patch_selects_injected_node_edge(
        "ludeon.rimworld",
        "example.bionicsfork",
        "HediffDef/BionicHeart/comps",
    ));
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert_eq!(view.injected_node_relations.len(), 1);
    assert_eq!(
        view.injected_node_relations[0].subject,
        "HediffDef/BionicHeart/comps"
    );
}

/// Mutation-tests the filter's own *scope*, not just that it can find
/// something (`docs/.claude/agent-memory` convention: a guard that
/// only ever gets an existence test can still be wrong about its own
/// boundary). Three edges that must each be excluded for a different
/// reason, alongside one that must be included:
/// - a `PatchSelectsInjectedNode` edge naming an unrelated def
///   (`ThingDef/Other`) — a different def entirely;
/// - a `PatchSelectsInjectedNode` edge naming
///   `HediffDef/BionicHeartOverride` — a real risk for a naive
///   `subject.starts_with(prefix)` check (this def's own prefix,
///   `HediffDef/BionicHeart`, is itself a string-prefix of that name)
///   that the exact-match-or-`/`-boundary rule must reject;
/// - a `UsesType` edge (not `PatchSelectsInjectedNode`) with the exact
///   same subject as the one that *should* match — proves the kind
///   filter, not just the subject filter, is load-bearing.
#[test]
fn excludes_edges_outside_this_defs_own_scope() {
    let mut fixture = bionic_heart_fixture_flat();
    fixture.report.edges.push(patch_selects_injected_node_edge(
        "ludeon.rimworld",
        "example.bionicsfork",
        "HediffDef/BionicHeart",
    ));
    fixture.report.edges.push(patch_selects_injected_node_edge(
        "a.mod",
        "b.mod",
        "ThingDef/Other",
    ));
    fixture.report.edges.push(patch_selects_injected_node_edge(
        "c.mod",
        "d.mod",
        "HediffDef/BionicHeartOverride",
    ));
    fixture.report.edges.push(EdgeReport {
        edge: Edge {
            after: ModId::new("e.mod"),
            before: ModId::new("f.mod"),
            kind: EdgeKind::UsesType,
            detail: "uses type".to_string(),
            load_time: true,
            subject: Some("HediffDef/BionicHeart".to_string()),
        },
        status: EdgeStatus::Unevaluated,
    });
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert_eq!(
        view.injected_node_relations,
        vec![InjectedNodeRelation {
            selector: ModId::new("ludeon.rimworld"),
            injector: ModId::new("example.bionicsfork"),
            subject: "HediffDef/BionicHeart".to_string(),
        }],
        "only the one genuinely-scoped PatchSelectsInjectedNode edge must survive"
    );
}

/// No `PatchSelectsInjectedNode` edge at all (the ordinary case, and
/// what `bionic_heart_fixture_flat`'s own `ReportBuilder` output
/// already looks like with nothing pushed onto it) is an empty list,
/// never an error.
#[test]
fn no_injected_node_edges_is_an_empty_list() {
    let fixture = bionic_heart_fixture_flat();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert!(view.injected_node_relations.is_empty());
}

/// A `DefConflictView` requested before the def has been inspected is
/// rejected rather than silently built from nothing.
#[test]
fn a_def_override_without_a_cached_inspection_is_rejected() {
    let fixture = bionic_heart_fixture();
    let session = session_with_sources(fixture.sources, fixture.report);

    let result = session.def_conflict_view(&bionic_heart_key());

    assert!(matches!(result, Err(DefConflictViewError::NotInspected(_))));
}

/// HeadNormal: three owners disagreeing on two fields produces two
/// `Conflict` rows, each with `Preference::LoadOrder` naming the
/// current winner — and flips to name the new winner once the
/// selected order changes who loads last.
#[test]
fn head_normal_has_two_conflict_rows_with_load_order_preference_that_flips_with_the_order() {
    let fixture = head_normal_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = head_normal_key();
    let def_ref = head_normal_ref();
    let reader = fixture.reader;

    InspectDef::new(reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting Current must succeed");
    PlanMerge::new(reader.clone())
        .execute(&mut session, &key)
        .expect("planning Current must succeed");
    let current = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");
    let current_conflicts: Vec<_> = current
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::Conflict)
        .cloned()
        .collect();
    assert_eq!(current_conflicts.len(), 2);
    for row in &current_conflicts {
        assert_eq!(
            row.preference,
            Preference::LoadOrder {
                winner: ModId::new("b.mod")
            },
            "{:?}",
            row.path
        );
    }

    // Force `core.mod` to load after both `a.mod` and `b.mod` under
    // `Suggested` — flips the def's own winner from `b.mod` to
    // `core.mod`.
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("core.mod"),
        before: ModId::new("a.mod"),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: false,
    }));
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("core.mod"),
        before: ModId::new("b.mod"),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: false,
    }));
    session.select(OrderSource::Suggested);
    InspectDef::new(reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting Suggested must succeed");
    PlanMerge::new(reader)
        .execute(&mut session, &key)
        .expect("planning Suggested must succeed");
    let suggested = session
        .def_conflict_view(&key)
        .expect("building the flipped view must succeed");
    let suggested_conflicts: Vec<_> = suggested
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::Conflict)
        .collect();
    assert_eq!(suggested_conflicts.len(), 2);
    for row in suggested_conflicts {
        assert_eq!(
            row.preference,
            Preference::LoadOrder {
                winner: ModId::new("core.mod")
            },
            "{:?}",
            row.path
        );
    }
}

/// A stored `MergeChoice` on both of HeadNormal's conflict fields
/// changes their `preference` to `Preference::MergeChoice` and their
/// `after_merge` to the chosen owner's own value.
#[test]
fn head_normal_a_stored_merge_choice_changes_preference_and_after_merge() {
    let fixture = head_normal_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = head_normal_key();
    let def_key = DefKey {
        def_type: "HeadTypeDef".to_string(),
        def_name: "HeadNormal".to_string(),
    };
    let def_ref = head_normal_ref();
    let narrow_path: FieldPath = "narrowCrownType".parse().expect("valid field path");
    let beard_path: FieldPath = "beardOffset".parse().expect("valid field path");

    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    session
        .decide(Decision {
            key: key.clone(),
            action: Action::Merge {
                key: def_key,
                choices: [
                    (
                        narrow_path.clone(),
                        MergeChoice::From {
                            mod_id: ModId::new("a.mod"),
                        },
                    ),
                    (
                        beard_path.clone(),
                        MergeChoice::From {
                            mod_id: ModId::new("a.mod"),
                        },
                    ),
                ]
                .into_iter()
                .collect(),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("a merge decision is always valid");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning with the stored choices must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    for path in [&narrow_path, &beard_path] {
        let row = view
            .fields
            .iter()
            .find(|r| &r.path == path)
            .unwrap_or_else(|| panic!("{path} must have a row"));
        assert_eq!(
            row.preference,
            Preference::MergeChoice {
                choice: MergeChoice::From {
                    mod_id: ModId::new("a.mod")
                }
            },
            "{path}"
        );
        let (after_mod, _) = row
            .after_merge
            .as_ref()
            .unwrap_or_else(|| panic!("{path} must have after_merge once the preview is Complete"));
        assert_eq!(after_mod, &ModId::new("a.mod"), "{path}");
        // `in_game` reflects the
        // real, undecided fold (`b.mod` is the last owner and this is
        // a `DefOverride`, so it owns the raw value outright) —
        // unaffected by the stored `Merge` decision's own choice of
        // `a.mod`, which only ever changes `after_merge`/`preference`.
        let (in_game_mod, _) = row
            .in_game
            .as_ref()
            .unwrap_or_else(|| panic!("{path} must have in_game populated"));
        assert_eq!(in_game_mod, &ModId::new("b.mod"), "{path}");
    }
}

/// A `PreferWinner` decision on HeadNormal's finding changes each
/// conflict row's `preference` to `Preference::Decision`, naming the
/// decided winner — `in_game` still names the *real* winner (`b.mod`,
/// last in the selected order), since a decision changes who the
/// panel says *should* win, not what the effective def's own fold
/// actually produced.
#[test]
fn head_normal_a_prefer_winner_decision_changes_preference_but_not_in_game() {
    let fixture = head_normal_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = head_normal_key();
    let def_key = DefKey {
        def_type: "HeadTypeDef".to_string(),
        def_name: "HeadNormal".to_string(),
    };
    let def_ref = head_normal_ref();
    let narrow_path: FieldPath = "narrowCrownType".parse().expect("valid field path");

    session
        .decide(Decision {
            key: key.clone(),
            action: Action::PreferWinner {
                key: def_key,
                winner: ModId::new("a.mod"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("a PreferWinner decision is always valid");
    // Inspected/planned *after* the decision: a decision that changes
    // `DecisionSet::sorter_overrides()` re-sorts and clears the
    // inspection cache, so a caller must always build against the
    // decision's own post-state, never rely on a cache from before it.
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let row = view
        .fields
        .iter()
        .find(|r| r.path == narrow_path)
        .expect("narrowCrownType must have a row");
    assert_eq!(
        row.preference,
        Preference::Decision {
            winner: ModId::new("a.mod")
        }
    );
    let (in_game_mod, _) = row
        .in_game
        .as_ref()
        .expect("in_game must still be populated");
    assert_eq!(
        in_game_mod,
        &ModId::new("b.mod"),
        "in_game must still name the real fold's own winner"
    );
}

/// `plantDensity`: a genuine `Conflict` row for the contested field,
/// plus a `CleanMerge` context row for the other field `x.mod`'s ops
/// also touch.
#[test]
fn plant_density_collision_has_one_conflict_row_plus_the_other_touched_field() {
    let fixture = plant_density_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "x.mod", "y.mod"],
    );
    let key = plant_density_key();
    let def_ref = plant_density_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert!(matches!(
        view.kind,
        DefConflictKind::PatchCollision { sub_path: Some(_) }
    ));
    let conflicts: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::Conflict)
        .collect();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path.to_string(), "plantDensity");
    // `y.mod` loads last, so its
    // own patch is what the real, undecided fold actually runs with —
    // `in_game` names it — while `after_merge` stays `None`: the
    // preview is `NeedsFieldInput` (a genuine conflict, no stored
    // choice), so no complete merged tree was ever built to read a
    // value back out of.
    let (conflict_in_game_mod, _) = conflicts[0]
        .in_game
        .as_ref()
        .expect("in_game must be populated from the real fold");
    assert_eq!(conflict_in_game_mod, &ModId::new("y.mod"));
    assert!(
        conflicts[0].after_merge.is_none(),
        "NeedsFieldInput never has a complete merge tree to read after_merge from"
    );

    let context = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "wildPlantRegrowDays")
        .expect("the other field x.mod's ops touch must be its own row");
    assert_eq!(context.kind, FieldRowKind::CleanMerge);
    assert_eq!(
        context.values,
        vec![(ModId::new("x.mod"), Value::Leaf("5".to_string()))]
    );
    // A context row's own `after_merge` must also be gated on the
    // preview actually being `Complete` — this one must not show a merge
    // result the (`NeedsFieldInput`) preview never built.
    assert!(
        context.after_merge.is_none(),
        "a context row's after_merge must be None when the preview isn't Complete"
    );
}

/// `wildAnimals`'s three disjoint adds each get their own
/// `MapEntry` row and the contested `Cobra` key gets a `Conflict`
/// row with an empty `agreed_by` (a `Conflict` row keeps every
/// differing member in `values`, never splits any of them into
/// `agreed_by` — that fold is `Agreeing`-only, see
/// `two_mod_agreeing_map_entry_credits_the_earlier_mod_and_names_the_rest_in_agreed_by`
/// below). No row exists for `wildAnimals` itself — once
/// `collision_fields` expands a keyed map, the whole-container
/// fallback row must not fire.
#[test]
fn keyed_map_collision_yields_one_map_entry_row_per_key_plus_one_conflict_row() {
    let fixture = arid_shrubland_wild_animals_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let key = arid_shrubland_key();
    let def_ref = arid_shrubland_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert!(
        view.fields
            .iter()
            .all(|r| r.path.to_string() != "wildAnimals"),
        "no row for the container itself once its keys are expanded: {:?}",
        view.fields
    );
    let map_entries: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::MapEntry)
        .collect();
    assert_eq!(
        map_entries
            .iter()
            .map(|r| r.path.to_string())
            .collect::<BTreeSet<_>>(),
        [
            "wildAnimals/Allosaurus".to_string(),
            "wildAnimals/Mammoth".to_string(),
            "wildAnimals/Hyena".to_string(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "{map_entries:?}"
    );
    assert!(
        map_entries.iter().all(|r| r.agreed_by.is_empty()),
        "a lone OneSided contributor has no one to agree with: {map_entries:?}"
    );

    let cobra = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "wildAnimals/Cobra")
        .expect("the contested key must still get its own row");
    assert_eq!(cobra.kind, FieldRowKind::Conflict);
    assert!(
        cobra.agreed_by.is_empty(),
        "a Conflict row never splits into agreed_by: {cobra:?}"
    );
    let cobra_mods: BTreeSet<ModId> = cobra.values.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        cobra_mods,
        [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod")
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "every differing contributor stays in values on a Conflict row: {cobra:?}"
    );
}

/// The displayed classification must be the one the plan was built
/// from: for a shape where `a.mod` `Add`s a key and `b.mod`/`c.mod`
/// `Replace` it, the plan leaves `wildAnimals/Raptor` `unresolved`, so
/// this view must not show a clean `MapEntry` row for it. This crate
/// consumes `rim_merge::plan::plan_patch_collision`'s per-mod candidate
/// construction rather than mirroring it, so the view and the plan
/// cannot disagree — one merge shown, the same one planned.
#[test]
fn a_key_added_by_one_mod_and_replaced_by_others_is_displayed_as_the_conflict_it_is_planned_as() {
    let fixture = add_then_replace_map_key_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let key = arid_shrubland_key();
    let def_ref = arid_shrubland_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    let preview = PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");
    let raptor_path: FieldPath = "wildAnimals/Raptor".parse().expect("a parseable path");
    let planned_unresolved = preview.plan.unresolved.clone();
    let diff_class = preview
        .diff
        .fields
        .iter()
        .find(|field| field.path == raptor_path)
        .map(|field| field.class.clone())
        .expect("the contested key must have its own diff field");

    assert_eq!(
        planned_unresolved,
        vec![raptor_path.clone()],
        "the plan itself treats the key as needing input"
    );
    assert!(
        matches!(diff_class, DiffClass::Conflict { .. }),
        "the diff the plan was built from must classify it the same way: {diff_class:?}"
    );

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");
    let raptor = view
        .fields
        .iter()
        .find(|row| row.path == raptor_path)
        .expect("the contested key must get its own row");
    assert_eq!(
        raptor.kind,
        FieldRowKind::Conflict,
        "the displayed row must not read as a clean map entry: {raptor:?}"
    );
    let contributors: BTreeSet<ModId> = raptor.values.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        contributors,
        [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod")
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "every mod whose Replace really lands must be shown: {raptor:?}"
    );
}

/// The "also added by" fold: two mods
/// independently replacing a keyed map's own key to the *same* value
/// is `Agreeing`, not `Conflict` — the row keeps the earlier
/// contributor (`a.mod`) in `values` and names the later one
/// (`b.mod`) in `agreed_by`, exactly like a `ListEntry` row's own
/// dedup fold, extended to a map's own key agreement.
#[test]
fn two_mod_agreeing_map_entry_credits_the_earlier_mod_and_names_the_rest_in_agreed_by() {
    let fixture = arid_shrubland_wild_animals_two_mod_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = arid_shrubland_two_mod_key();
    let def_ref = arid_shrubland_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let cobra = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "wildAnimals/Cobra")
        .expect("the agreeing key must still get its own row");
    assert_eq!(cobra.kind, FieldRowKind::MapEntry);
    assert_eq!(
        cobra.values,
        vec![(ModId::new("a.mod"), Value::Leaf("0.5".to_string()))],
        "the earliest contributor keeps the row's own values: {cobra:?}"
    );
    assert_eq!(
        cobra.agreed_by,
        vec![ModId::new("b.mod")],
        "the later agreeing contributor is named in agreed_by, not a second row: {cobra:?}"
    );
}

/// A patcher flagged as Rimmerge-generated appears as a toucher, not
/// hidden.
#[test]
fn a_generated_patcher_is_flagged_not_hidden() {
    let fixture = plant_density_fixture_with_generated_patcher();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "x.mod", "y.mod"],
    );
    let key = plant_density_key();
    let def_ref = plant_density_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let x_toucher = view
        .touchers
        .iter()
        .find(|t| t.mod_id == ModId::new("x.mod") && t.role == ToucherRole::Patcher)
        .expect("x.mod must appear as a patcher toucher");
    assert!(x_toucher.is_generated);
}

/// Two mods adding different `li` items to the same list: each gets
/// its own `ListEntry` row naming its own mod, and the effective
/// list holds both.
#[test]
fn two_mods_adding_different_list_items_each_get_their_own_entry_row() {
    let fixture = two_mod_list_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 2, "{list_rows:?}");
    let mods: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        mods,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert!(
        view.fields
            .iter()
            .any(|r| r.kind == FieldRowKind::Conflict && r.path.to_string() == "label"),
        "the contested `label` field must still be its own row"
    );
}

/// Under the key the analyzer
/// would *actually* produce for `two_mod_list_fixture`'s own `comps`
/// collision (`sub_path: Some("comps")`, not `"label"` —
/// `widget_comps_key`'s own doc comment), the container-level
/// `Conflict` the engine's own collision diff computes on `comps`
/// itself (both mods'
/// whole-subtree candidates differ, since each only replays its own
/// add in isolation) must be demoted away: it's fully explained by the
/// two `ListEntry` rows underneath it, so no `comps` row of any kind
/// appears at all.
#[test]
fn a_container_conflict_fully_explained_by_list_children_is_dropped() {
    let fixture = two_mod_list_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert!(
        view.fields.iter().all(|r| r.path.to_string() != "comps"),
        "the container-level comps row must be dropped entirely, not merely reclassified: {:?}",
        view.fields
    );
    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 2, "{list_rows:?}");
    let mods: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        mods,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
}

/// Two mods each add a `li` item with the
/// *same* declared identity (`Class="CompProperties_Alpha"`) but
/// **different** content (`amount` `1` vs `2`) — `rim_merge::effective`'s
/// own identity fallback collapses both to `ItemId::Position`.
/// `rim_merge::effective::compute`'s own `renamed_list_item_source`
/// credits each positional entry to whichever mod's op actually
/// produced it, rather than crediting both to the later mod. Since the
/// two entries' content genuinely differs, no dedup applies — each keeps
/// its own single-value `ListEntry` row, and the container-level `comps`
/// row still stays a `Conflict` naming both mods: a `Position` identity
/// is snapshot-relative, and (per `every_positional_child_agrees`) the
/// two colliding siblings don't actually agree, so
/// [`is_container_conflict_explained_by_list_children`] still can't treat
/// it as "explained" — two mods adding entries with the same identity is
/// a Conflict.
#[test]
fn same_identity_list_items_with_differing_content_are_each_credited_to_their_own_mod() {
    let fixture = same_identity_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 2, "{list_rows:?}");
    let by_path: BTreeMap<String, &FieldRow> = list_rows
        .iter()
        .map(|row| (row.path.to_string(), *row))
        .collect();
    assert_eq!(
        by_path.keys().cloned().collect::<BTreeSet<_>>(),
        ["comps/li[#0]".to_string(), "comps/li[#1]".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "duplicate declared identity must fall back to position, per \
             rim-merge's own identify_all_li"
    );
    for (path, expected_mod) in [("comps/li[#0]", "a.mod"), ("comps/li[#1]", "b.mod")] {
        let row = by_path[path];
        let (mod_id, _) = row
            .values
            .first()
            .expect("a ListEntry row must have exactly one value");
        assert_eq!(
            mod_id,
            &ModId::new(expected_mod),
            "{path}: each positional entry must be credited to whichever mod's own \
                 op produced it, not whichever mod merely triggered the renumbering"
        );
        assert!(
            row.agreed_by.is_empty(),
            "{path}: genuinely different content must never be folded into an agreement"
        );
    }

    // The container-level `comps` row must stay a `Conflict`, never
    // demoted — the two items share a declared identity and genuinely
    // differing content, so there's a real ambiguity worth surfacing,
    // not a clean merge.
    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the comps Conflict row must not be dropped for a same-identity clash");
    assert_eq!(comps_row.kind, FieldRowKind::Conflict);
    let comps_mods: BTreeSet<ModId> = comps_row.values.iter().map(|(m, _)| m.clone()).collect();
    assert_eq!(
        comps_mods,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "both mods must still appear in the container row's own values"
    );
}

/// The "list case" dedup's own two-mod case: `a.mod` and `b.mod` each
/// add the *exact same* `li Class="CompProperties_Alpha"` item — byte-
/// identical content, unlike the differing-content fixture above.
/// Instead of two indistinguishable-looking `ListEntry` rows (one per
/// position), the two fold into one: `a.mod`'s own row,
/// `b.mod` named only in `agreed_by`. The container-level `comps` row
/// is unaffected by the fold (it's a different row, backed by the
/// engine's own collision diff rather than built from
/// `context_rows_from_provenance`) but is never a
/// `Conflict` here either: two mods replaying the *exact same* op end
/// up with structurally identical whole-subtree candidates regardless
/// of which one `move_mod_last` orders last, so that diff itself
/// already classifies it `Agreeing` — a `CleanMerge` row, the "clean
/// merge" case, never the artificial `Conflict`
/// [`is_container_conflict_explained_by_list_children`] exists to demote.
#[test]
fn two_mods_adding_the_exact_same_list_item_fold_into_one_agreed_on_row() {
    let fixture = same_identity_agreeing_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(
        list_rows.len(),
        1,
        "two byte-identical adds must fold into exactly one row: {list_rows:?}"
    );
    let row = list_rows[0];
    assert_eq!(row.path.to_string(), "comps/li[#0]");
    assert_eq!(row.values.len(), 1);
    assert_eq!(
        row.values[0].0,
        ModId::new("a.mod"),
        "the surviving row is credited to the first mod that added it"
    );
    assert_eq!(
        row.agreed_by,
        vec![ModId::new("b.mod")],
        "the second, byte-identical contributor is named in agreed_by, not a second row"
    );

    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the container summary row still exists, just not as a Conflict");
    assert_eq!(
        comps_row.kind,
        FieldRowKind::CleanMerge,
        "two byte-identical adds never disagree at the container level either, unlike \
             the differing-content fixture's own comps row"
    );
}

/// The "list case" dedup's own three-mod case: the fold isn't limited
/// to a single pair — every entry past the first folds into the same
/// row's `agreed_by`.
#[test]
fn three_mods_adding_the_exact_same_list_item_fold_into_one_agreed_on_row() {
    let fixture = three_mod_agreeing_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(
        list_rows.len(),
        1,
        "three byte-identical adds must still fold into exactly one row: {list_rows:?}"
    );
    let row = list_rows[0];
    assert_eq!(row.values.len(), 1);
    assert_eq!(row.values[0].0, ModId::new("a.mod"));
    assert_eq!(
        row.agreed_by,
        vec![ModId::new("b.mod"), ModId::new("c.mod")],
        "both later contributors are named in agreed_by, in load order"
    );
}

/// A `PatchCollision`
/// whose contested op targets the def node itself gets `sub_path:
/// None` from the analyzer, and `plan_merge.rs`'s `path_key` defaults
/// to the empty `FieldPath` for that case. With a single contributor
/// (trivially `OneSided`, no explicit choice needed) the plan comes
/// back `Complete`, so `build_patch_collision_fields` builds
/// `after_tree` via `after_merge_tree` -> `rim_merge::plan::
/// build_resolved_node`, which must not fold the whole-def
/// `Value::Item` field through `build_chain` (slicing
/// `path.segments().len() - 1` on the empty path panics with `attempt
/// to subtract with overflow`). Building this view returns `Ok`, with
/// the resolved node retagged to the def type, carrying `a.mod`'s own
/// added `techLevel` child.
#[test]
fn sub_path_none_patch_collision_that_replays_complete_does_not_panic() {
    let fixture = whole_def_patch_collision_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "a.mod"]);
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: None,
        mods: [ModId::new("a.mod")].into_iter().collect(),
    };
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    );
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must not panic, and must succeed");

    let root_row = view
        .fields
        .iter()
        .find(|row| row.path.segments().is_empty())
        .expect("the whole-def field surfaces its own row");
    let (mod_id, value) = root_row
        .after_merge
        .as_ref()
        .expect("the plan is Complete, so after_merge must be populated")
        .clone();
    assert_eq!(mod_id, ModId::new("a.mod"));
    let Value::Item(node) = value else {
        panic!("the whole-def field's after_merge value must be a whole node");
    };
    assert_eq!(node.tag, "ThingDef");
    let Content::Children(children) = &node.content else {
        panic!("expected the resolved def's own children: {node:?}");
    };
    assert!(
        children.iter().any(|child| child.tag == "techLevel"),
        "the resolved node must carry a.mod's own added child: {node:?}"
    );
}

/// An in-scope contributor's own add can be
/// byte-identical to the def's own already-declared (`Owner`) content
/// — the earlier "sibling" [`rim_merge::effective::duplicate_of_earlier_sibling`]
/// finds is never itself a [`Provenance::Patch`] by a `mods` member, so
/// folding into it would drop `a.mod`'s own contribution with nothing
/// left to attach `agreed_by` to. `a.mod` must still get its own row.
#[test]
fn an_add_that_duplicates_the_owners_own_declared_item_still_gets_its_own_row() {
    let fixture = owner_agreeing_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    let credited: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        credited,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "a.mod's own add must never vanish just because it duplicates the owner's \
             own declared content: {list_rows:?}"
    );
    assert!(
        list_rows.iter().all(|r| r.agreed_by.is_empty()),
        "the owner is never a party `context_rows_from_provenance` can name in \
             agreed_by — a.mod's row must be an ordinary, un-folded one: {list_rows:?}"
    );
}

/// Second variant: the byte-identical earlier
/// sibling can also be a [`Provenance::Patch`] by a real, active
/// patcher that simply isn't a party to *this* collision (`x.mod` is
/// active and touches the same def, but the collision's own `mods`
/// names only `a.mod`/`b.mod`) — the same silent-drop risk as the
/// owner case above, from a different provenance shape.
#[test]
fn an_add_that_duplicates_an_out_of_scope_patchers_item_still_gets_its_own_row() {
    let fixture = out_of_scope_patcher_agreeing_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "x.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    let credited: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        credited,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "a.mod's own add must never vanish just because it duplicates x.mod's own \
             out-of-scope content: {list_rows:?}"
    );
    assert!(
        list_rows.iter().all(|r| r.agreed_by.is_empty()),
        "x.mod is out of this collision's own scope and can never appear in \
             agreed_by: {list_rows:?}"
    );
}

/// Pins "two agree, one differs": the fold matches by real content, not
/// by a single fixed "head".
#[test]
fn three_mods_two_agree_one_differs_folds_only_the_agreeing_pair() {
    let fixture = three_mod_two_agree_one_differs_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(
        list_rows.len(),
        2,
        "a.mod/c.mod's agreeing pair folds to one row, b.mod's differing add keeps \
             its own: {list_rows:?}"
    );
    let a_row = list_rows
        .iter()
        .find(|r| r.values.first().map(|(m, _)| m) == Some(&ModId::new("a.mod")))
        .expect("a.mod's own row must exist");
    assert_eq!(a_row.agreed_by, vec![ModId::new("c.mod")]);
    let b_row = list_rows
        .iter()
        .find(|r| r.values.first().map(|(m, _)| m) == Some(&ModId::new("b.mod")))
        .expect("b.mod's own row must exist, un-folded");
    assert!(b_row.agreed_by.is_empty());

    // Like the two-mod differing-content precedent above: b.mod's own
    // genuine disagreement means the group as a whole still disagrees,
    // so the container-level `comps` row must stay a `Conflict`, never
    // demoted, even though a.mod/c.mod's own pair agrees.
    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the comps Conflict row must not be dropped while b.mod still disagrees");
    assert_eq!(comps_row.kind, FieldRowKind::Conflict);
}

/// A stated decision, not an accident — the fold applies
/// to an identity-less `li` whose content happens to be byte-identical
/// too, the same as a genuine declared-identity collision.
/// `duplicate_of_earlier_sibling` only ever compares real content at a
/// `Position`-identified path, never *why* the path fell back to
/// `Position` in the first place.
#[test]
fn identity_less_agreeing_adds_fold_into_one_row_too() {
    let fixture = identity_less_agreeing_list_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(
        list_rows.len(),
        1,
        "two byte-identical identity-less adds must fold into one row too: {list_rows:?}"
    );
    assert_eq!(list_rows[0].values[0].0, ModId::new("a.mod"));
    assert_eq!(list_rows[0].agreed_by, vec![ModId::new("b.mod")]);
}

fn trinket_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Trinket".to_string(),
        },
        Selector::DefName,
    )
}

fn trinket_comps_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Trinket".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("comps".to_string()),
        mods: [ModId::new("p.mod"), ModId::new("q.mod")]
            .into_iter()
            .collect(),
    }
}

/// [`is_container_conflict_explained_by_list_children`]'s "never
/// `ItemId::Position`" rule isn't only about two mods declaring the
/// *same* `Class` (the same-identity test above) — a plain,
/// identity-less `li` (no `Class`, no well-known key child, no text:
/// `<li><amount>N</amount></li>`) hits the identical `Position`
/// fallback *within each candidate's own isolated one-item subtree*,
/// even though the two items end up perfectly distinct, cleanly
/// attributed `ListEntry` rows in the real, merged list (`li[#0]`/
/// `li[#1]`, one per mod — no ambiguity there at all). The
/// container-level `comps` row must still stay a `Conflict`:
/// `candidate_list_item_ids` only ever sees each candidate's own
/// single-item snapshot, where "position 0" is trivially true for any
/// lone identity-less item and proves nothing about whether the two
/// candidates' items are "the same" one.
#[test]
fn identity_less_list_items_also_fall_back_to_position_and_keep_the_container_conflict() {
    let fixture = identity_less_list_items_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "p.mod", "q.mod"],
    );
    let key = trinket_comps_key();
    let def_ref = trinket_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    // The two items are perfectly distinct in the real, merged list —
    // no ambiguity there, unlike the same-Class fixture above.
    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 2, "{list_rows:?}");
    let mods: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        mods,
        [ModId::new("p.mod"), ModId::new("q.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );

    // But the container-level row must still stay a Conflict: each
    // candidate's own isolated subtree only ever contains one
    // identity-less item, so its own fallback identity (`Position(0)`)
    // is never trustworthy evidence the two candidates agree on
    // anything.
    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the comps Conflict row must not be dropped for identity-less items either");
    assert_eq!(comps_row.kind, FieldRowKind::Conflict);
}

/// `a.mod` and `b.mod` each
/// wholesale `PatchOperationReplace` the `comps` container with their
/// own distinct single `li` — `b.mod` loads last, so its own Replace
/// discards `a.mod`'s own contribution entirely in the real fold.
/// `a.mod`'s own candidate item never survives, so the demotion check
/// must fail: the container-level `Conflict` row stays.
#[test]
fn replace_vs_replace_on_a_container_keeps_the_conflict_row() {
    let fixture = replace_vs_replace_on_container_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the comps row must not be dropped — a.mod's own item was clobbered");
    assert_eq!(comps_row.kind, FieldRowKind::Conflict);
}

/// `a.mod` adds its own `li` to
/// `comps`; `b.mod`, loading after, wholesale-replaces the whole
/// container — the same real clobber as the Replace/Replace case
/// above, just via a different op class on the clobbering side.
#[test]
fn add_then_replace_on_a_container_keeps_the_conflict_row() {
    let fixture = add_then_replace_on_container_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let comps_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "comps")
        .expect("the comps row must not be dropped — a.mod's own item was clobbered");
    assert_eq!(comps_row.kind, FieldRowKind::Conflict);
}

/// A real, non-empty base list
/// (one pre-existing, `Owner`-credited `li`) must not defeat the
/// demotion — `a.mod`/`b.mod` each add their own further distinct
/// item, all three survive untouched, so the container-level
/// `Conflict` row still drops away, exactly like the empty-base case.
#[test]
fn a_container_conflict_with_a_real_base_list_item_still_drops() {
    let fixture = list_with_owner_item_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let key = widget_comps_key();
    let def_ref = widget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    assert!(
        view.fields.iter().all(|r| r.path.to_string() != "comps"),
        "a real, untouched base list item must not defeat the drop: {:?}",
        view.fields
    );
    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    let mods: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        mods,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "{list_rows:?}"
    );
}

/// A `DefOverride` whose winning owner's
/// `ParentName` chain can't be resolved at all makes `PlanMerge` hard-
/// error rather than cache anything — `def_conflict_view` must still
/// render a full panel from `EffectiveDef::provenance` alone, with a
/// `MissingTemplate` problem naming the gap, never `NotPlanned`.
#[test]
fn def_override_with_a_missing_template_still_renders_a_full_panel() {
    let fixture = def_override_missing_template_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "broken.mod"]);
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
        },
        owners: [ModId::new("core.mod"), ModId::new("broken.mod")]
            .into_iter()
            .collect(),
    };
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
        },
        Selector::DefName,
    );
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("InspectDef's own template_chain stops silently, never erroring");
    let plan_result = PlanMerge::new(fixture.reader).execute(&mut session, &key);
    assert!(
        matches!(
            plan_result,
            Err(crate::use_cases::PlanMergeError::MissingSource(_))
                | Err(crate::use_cases::PlanMergeError::Inherit(_))
        ),
        "sanity: PlanMerge must actually hard-error for this fixture, {plan_result:?}"
    );

    let view = session
        .def_conflict_view(&key)
        .expect("an empty panel is never acceptable — no NotPlanned here");

    assert!(
        view.problems
            .iter()
            .any(|p| matches!(p, Problem::MissingTemplate { name, .. } if name == "ReallyMissing")),
        "{:?}",
        view.problems
    );
    assert!(
        view.fields
            .iter()
            .any(|r| r.path.to_string() == "description"),
        "broken.mod's own other field must still show up: {:?}",
        view.fields
    );
    assert!(
        view.fields.iter().all(|r| r.kind != FieldRowKind::Conflict),
        "no diff exists without a preview, so nothing can be a genuine Conflict: {:?}",
        view.fields
    );
}

/// When the *losing* owner (`broken.mod`) has the broken chain and the
/// winner (`clean.mod`) is clean, `InspectDef`'s own
/// `effective.completeness` is `Complete` — plain
/// `Session::def_conflict_view` reports zero problems here, so the plan
/// failure would silently vanish — while
/// `Session::def_conflict_view_with_plan_failure`, given the swallowed
/// `PlanMergeError`, appends it as a `Problem` instead.
#[test]
fn def_conflict_view_with_plan_failure_reports_a_losing_owners_broken_chain() {
    let fixture = def_override_missing_template_on_a_losing_owner_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["broken.mod", "clean.mod"],
    );
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
        },
        owners: [ModId::new("broken.mod"), ModId::new("clean.mod")]
            .into_iter()
            .collect(),
    };
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
        },
        Selector::DefName,
    );
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("InspectDef's own template_chain stops silently, never erroring");
    let plan_result = PlanMerge::new(fixture.reader).execute(&mut session, &key);
    let plan_error = plan_result.expect_err(
        "sanity: PlanMerge must still hard-error even though the winner itself is clean",
    );

    let plain_view = session
        .def_conflict_view(&key)
        .expect("an empty panel is never acceptable — no NotPlanned here");
    assert!(
        plain_view.problems.is_empty(),
        "sanity: the plain accessor must NOT see the losing owner's own gap — that's \
             exactly the bug this fix addresses: {:?}",
        plain_view.problems
    );
    assert_eq!(plain_view.effective_completeness, Completeness::Complete);

    let view = session
        .def_conflict_view_with_plan_failure(&key, Some(&plan_error))
        .expect("an empty panel is never acceptable — no NotPlanned here");

    assert_eq!(view.problems.len(), 1, "{:?}", view.problems);
    assert_eq!(view.problems[0], problem_from_plan_failure(&plan_error));
}

/// A direct `rim-session`-level
/// proof that `Session::def_conflict_view_for_patch` actually reads
/// from the *patch's own* `PreviewSlot`, not the profile's — builds
/// `PreviewSlot::patch(...)` explicitly, plans under it via
/// `Session::merge_context`, confirms nothing was ever cached under
/// the profile's own slot, and only then asserts the scoped view's
/// own `fields`.
#[test]
fn def_conflict_view_for_patch_reads_rows_from_that_patchs_own_scoped_preview() {
    let fixture = bionic_heart_fixture();
    let scope = rim_resolve::domain::PatchScope::new([
        ModId::new("ludeon.rimworld"),
        ModId::new("example.bionicsfork"),
    ])
    .expect("2 distinct members satisfies PatchScope::new");
    let (mut session, patch_id) =
        crate::test_support::patch_fixture_with_sources(fixture.sources, fixture.report, scope);
    let key = bionic_heart_key();
    let def_ref = bionic_heart_ref();

    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");

    let slot = PreviewSlot::patch(session.selected(), patch_id.clone());
    let ctx = session
        .merge_context(slot.clone(), &key)
        .expect("a real, loaded patch resolves its own context");
    PlanMerge::new(fixture.reader)
        .execute_in(&mut session, ctx, &key)
        .expect("planning under the patch's own scope must succeed");

    // Nothing was ever planned under the profile's own slot — the
    // scoped view below can only have come from the patch's own
    // cached preview.
    assert!(
        session
            .merge_preview(&PreviewSlot::profile(session.selected()), &key)
            .is_none(),
        "sanity: the profile's own slot must stay empty"
    );

    let view = session
        .def_conflict_view_for_patch(&patch_id, &key)
        .expect("building the scoped view must succeed");

    assert_eq!(view.def_ref, def_ref);
    assert!(
        view.fields.iter().any(|f| f.path.to_string() == "label"),
        "the scoped preview's own field diff must populate `fields`: {:?}",
        view.fields
    );
}

/// A `ListEntry` row sorts by its own
/// document position, not always after every scalar row — `bionics.mod`'s
/// own raw XML declares `fieldA`, then `comps` (one `li`), then
/// `fieldZ`, and the field list must preserve exactly that order.
#[test]
fn list_entries_interleave_with_scalar_fields_in_document_order() {
    let fixture = document_order_interleaved_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "bionics.mod"],
    );
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket2".to_string(),
        },
        owners: [ModId::new("core.mod"), ModId::new("bionics.mod")]
            .into_iter()
            .collect(),
    };
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket2".to_string(),
        },
        Selector::DefName,
    );
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let ordered_paths: Vec<String> = view
        .fields
        .iter()
        .filter(|r| r.kind != FieldRowKind::Unchanged)
        .map(|r| r.path.to_string())
        .collect();
    assert_eq!(
        ordered_paths,
        vec![
            "fieldA".to_string(),
            "comps/li[@Class=CompProperties_Foo]".to_string(),
            "fieldZ".to_string(),
        ],
        "the list entry must sit between the two scalar fields, matching document order"
    );
}

/// Three mods each adding their
/// own distinct `li` item to the same list all get their own
/// `ListEntry` row.
#[test]
fn three_mods_adding_distinct_list_items_each_get_their_own_entry_row() {
    let fixture = three_mod_list_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let key = gadget_comps_key();
    let def_ref = gadget_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 3, "{list_rows:?}");
    let mods: BTreeSet<ModId> = list_rows
        .iter()
        .flat_map(|r| r.values.iter().map(|(m, _)| m.clone()))
        .collect();
    assert_eq!(
        mods,
        [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod")
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
}

/// `ListEntry` siblings sort by
/// their actual position in `effective.resolved`, not by `FieldPath`'s
/// own lexical `Ord` — `z.mod` loads (and so appends) first, but its
/// item's `Class` ("Zeta") sorts *after* `a.mod`'s ("Alpha").
#[test]
fn list_entries_sort_by_document_position_not_lexical_class_name() {
    let fixture = list_position_differs_from_lexical_order_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "z.mod", "a.mod"],
    );
    let key = sprocket_comps_key();
    let def_ref = sprocket_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed");

    let list_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.kind == FieldRowKind::ListEntry)
        .collect();
    assert_eq!(list_rows.len(), 2, "{list_rows:?}");
    let mods_in_order: Vec<ModId> = list_rows
        .iter()
        .map(|r| r.values.first().expect("one value each").0.clone())
        .collect();
    assert_eq!(
        mods_in_order,
        vec![ModId::new("z.mod"), ModId::new("a.mod")],
        "must follow document/insertion order (Zeta then Alpha), not \
             lexical Class-name order (which would put Alpha first)"
    );
}

/// `c.mod` successfully patches
/// the collision's own contested field (`label`) before `x.mod`'s
/// unrelated stopper poisons the whole preview — the contested field's
/// own row must still come back `Conflict`, never mis-kinded as a
/// clean, single-mod context row just because `c.mod`'s own
/// contribution reached it first.
#[test]
fn contested_field_reached_before_an_unrelated_stopper_stays_a_conflict_row() {
    let fixture = contested_field_patched_before_stopper_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "c.mod", "x.mod", "y.mod"],
    );
    let key = contested_field_before_stopper_key();
    let def_ref = wall2_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed (it comes back CannotMerge, not Err)");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed even past a stopper");

    let label_rows: Vec<_> = view
        .fields
        .iter()
        .filter(|r| r.path.to_string() == "label")
        .collect();
    assert_eq!(
        label_rows.len(),
        1,
        "the contested field must appear exactly once: {:?}",
        view.fields
    );
    let label_row = label_rows[0];
    assert_eq!(
        label_row.kind,
        FieldRowKind::Conflict,
        "never mis-kinded as CleanMerge just because c.mod reached it first"
    );
    let (in_game_mod, in_game_value) = label_row
        .in_game
        .as_ref()
        .expect("c.mod's own successful patch must be visible as in_game");
    assert_eq!(in_game_mod, &ModId::new("c.mod"));
    assert_eq!(in_game_value, &Value::Leaf("sturdy wall".to_string()));
    assert_eq!(
        label_row.values,
        vec![(ModId::new("c.mod"), Value::Leaf("sturdy wall".to_string()))],
        "values must mirror the same provenance entry in_game reads"
    );
    assert!(
        label_row.after_merge.is_none(),
        "no complete merge plan exists for a CannotMerge preview"
    );
}

/// `FindingKey::DuplicateTemplateName` has no `MergePreview` to build
/// fields from (`PlanMerge` does not support this kind) — the view
/// still builds, with touchers from both registering mods and an
/// empty field list, per `DefConflictKind::DuplicateTemplateName`'s
/// own doc comment.
#[test]
fn duplicate_template_name_has_touchers_and_no_fields() {
    let fixture = duplicate_template_name_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["a.mod", "b.mod"]);
    let key = FindingKey::DuplicateTemplateName {
        name: "Base".to_string(),
        owners: [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect(),
    };
    let def_ref = DefRef::name_only("Base");
    InspectDef::new(fixture.reader)
        .execute(&mut session, &def_ref)
        .expect("inspecting a duplicate template name must succeed");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed with no preview at all");

    assert_eq!(view.kind, DefConflictKind::DuplicateTemplateName);
    assert!(
        view.fields.is_empty(),
        "PlanMerge has no support for this finding kind yet"
    );
    let owners: BTreeSet<ModId> = view.touchers.iter().map(|t| t.mod_id.clone()).collect();
    assert_eq!(
        owners,
        [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "{:?}",
        view.touchers
    );
}

/// An unknown custom class mid-sequence stops the fold: the row
/// before it survives with `in_game` populated, the row past it (the
/// collision's own contested field, whose only contributor never
/// runs) is still shown with `in_game: None`, and a `Problem`
/// pinpoints the stopper.
#[test]
fn unsupported_op_mid_sequence_keeps_earlier_rows_and_names_the_stopper() {
    let fixture = unsupported_op_patch_collision_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "c.mod", "x.mod", "d.mod"],
    );
    let key = wall_collision_key();
    let def_ref = wall_ref();
    InspectDef::new(fixture.reader.clone())
        .execute(&mut session, &def_ref)
        .expect("inspecting must succeed");
    PlanMerge::new(fixture.reader)
        .execute(&mut session, &key)
        .expect("planning must succeed (it comes back CannotMerge, not Err)");

    let view = session
        .def_conflict_view(&key)
        .expect("building the view must succeed even past a stopper");

    assert!(matches!(
        view.effective_completeness,
        Completeness::Partial { .. }
    ));
    assert_eq!(view.problems.len(), 1);
    match &view.problems[0] {
        Problem::UnsupportedOp {
            mod_id,
            op_index,
            class,
            xpath,
            reason,
        } => {
            assert_eq!(mod_id, &ModId::new("x.mod"));
            // `x.mod`'s op is the
            // second across every active patcher's top-level
            // operations, in load order (c.mod's own op is index 0).
            assert_eq!(*op_index, 1);
            assert_eq!(class, "SomeThirdParty.WeirdOperation");
            assert_eq!(
                xpath.as_deref(),
                Some(r#"Defs/ThingDef[defName="Wall"]/fillPercent"#)
            );
            assert!(reason.contains("unsupported operation class"), "{reason}");
        }
        other => panic!("expected UnsupportedOp, got {other:?}"),
    }

    let description_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "description")
        .expect("c.mod's own field, before the stopper, must survive");
    assert!(
        description_row.in_game.is_some(),
        "a field set before the stopper must have in_game populated"
    );

    let label_row = view
        .fields
        .iter()
        .find(|r| r.path.to_string() == "label")
        .expect("the collision's own contested field is always shown");
    assert!(
        label_row.in_game.is_none(),
        "d.mod's own op never ran, so in_game must be None"
    );
}

/// A finding key naming neither a def override, a patch collision,
/// nor a duplicate template name has no field-by-field view at all.
#[test]
fn a_non_def_finding_key_is_rejected() {
    let fixture = bionic_heart_fixture();
    let session = session_with_sources(fixture.sources, fixture.report);
    let key = FindingKey::MissingMod {
        mod_id: ModId::new("ludeon.rimworld"),
    };

    let result = session.def_conflict_view(&key);

    assert!(matches!(
        result,
        Err(DefConflictViewError::UnsupportedFinding(_))
    ));
}

// -- `page` -------------------------------------------------------

/// A [`DefConflictView`] with one [`FieldRow`] per `kinds` entry, path
/// `field{i}`, everything else empty/`None` — `page` is pure
/// computation over an already-built view, so its own tests never
/// need a real session/fixture, mirroring
/// `crate::effective_fields::page`'s own test fixtures.
fn view_with_kinds(kinds: &[FieldRowKind]) -> DefConflictView {
    let fields = kinds
        .iter()
        .enumerate()
        .map(|(i, &kind)| FieldRow {
            path: format!("field{i}").parse().expect("valid field path"),
            kind,
            values: Vec::new(),
            agreed_by: Vec::new(),
            in_game: None,
            after_merge: None,
            preference: Preference::None,
        })
        .collect();
    DefConflictView {
        def_ref: bionic_heart_ref(),
        kind: DefConflictKind::DefOverride,
        touchers: Vec::new(),
        fields,
        problems: Vec::new(),
        effective_completeness: rim_merge::effective::Completeness::Complete,
        injected_node_relations: Vec::new(),
    }
}

#[test]
fn page_hides_unchanged_rows_when_only_changed() {
    let view = view_with_kinds(&[
        FieldRowKind::Conflict,
        FieldRowKind::Unchanged,
        FieldRowKind::CleanMerge,
    ]);

    let result = page(
        &view,
        &FieldRowFilter {
            only_changed: true,
            offset: 0,
            limit: 10,
        },
    );

    assert_eq!(result.total, 2);
    assert!(
        result
            .items
            .iter()
            .all(|row| row.kind != FieldRowKind::Unchanged)
    );
}

#[test]
fn page_keeps_unchanged_rows_when_only_changed_is_false() {
    let view = view_with_kinds(&[FieldRowKind::Conflict, FieldRowKind::Unchanged]);

    let result = page(
        &view,
        &FieldRowFilter {
            only_changed: false,
            offset: 0,
            limit: 10,
        },
    );

    assert_eq!(result.total, 2);
}

#[test]
fn page_offset_and_limit_page_through_the_matching_rows() {
    let kinds = vec![FieldRowKind::CleanMerge; 5];
    let view = view_with_kinds(&kinds);

    let result = page(
        &view,
        &FieldRowFilter {
            only_changed: false,
            offset: 2,
            limit: 2,
        },
    );

    assert_eq!(
        result.total, 5,
        "total counts every matching row, not just the page"
    );
    assert_eq!(result.items.len(), 2);
    let paths: Vec<String> = result
        .items
        .iter()
        .map(|row| row.path.to_string())
        .collect();
    assert_eq!(paths, vec!["field2".to_string(), "field3".to_string()]);
}

#[test]
fn page_is_clamped_to_max_page_size_regardless_of_the_requested_limit() {
    let kinds = vec![FieldRowKind::CleanMerge; crate::MAX_PAGE_SIZE + 10];
    let view = view_with_kinds(&kinds);

    let result = page(
        &view,
        &FieldRowFilter {
            only_changed: false,
            offset: 0,
            limit: 10_000,
        },
    );

    assert_eq!(result.total, crate::MAX_PAGE_SIZE + 10);
    assert_eq!(result.items.len(), crate::MAX_PAGE_SIZE);
}

/// A minimal [`FieldRow`] naming `path`/`kind` — every other field is
/// irrelevant to `sort_field_rows`, which never reads them.
fn sort_test_row(path: FieldPath, kind: FieldRowKind) -> FieldRow {
    FieldRow {
        path,
        kind,
        values: Vec::new(),
        agreed_by: Vec::new(),
        in_game: None,
        after_merge: None,
        preference: Preference::None,
    }
}

fn sort_test_leaf(tag: &str) -> rim_merge::tree::FieldNode {
    rim_merge::tree::FieldNode {
        tag: tag.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text("x".to_string()),
    }
}

fn sort_test_effective(root_children: Vec<rim_merge::tree::FieldNode>) -> EffectiveDef {
    EffectiveDef {
        resolved: FieldTree {
            root: rim_merge::tree::FieldNode {
                tag: "ThingDef".to_string(),
                attrs: BTreeMap::new(),
                content: Content::Children(root_children),
            },
            parent_name: None,
            name: None,
        },
        provenance: BTreeMap::new(),
        completeness: Completeness::Complete,
        caveats: Vec::new(),
        top_level_outcomes: Vec::new(),
        suppressed_filter_head_ops: 0,
    }
}

/// Pins the three-kind interleave `sort_field_rows`'s own doc comment
/// describes — `Conflict` always ranks first regardless of
/// document position, then `ListEntry`/`Unchanged` (and `CleanMerge`,
/// not exercised here) order by where they actually sit in `resolved`,
/// not by `FieldPath`'s own lexical `Ord` (`comps/li[...]` would
/// otherwise sort before `label` alphabetically, the wrong order for
/// this tree).
#[test]
fn sort_field_rows_ranks_conflict_first_then_orders_the_rest_by_document_position() {
    let li = rim_merge::tree::FieldNode {
        tag: "li".to_string(),
        attrs: BTreeMap::from([("Class".to_string(), "Comp".to_string())]),
        content: Content::Empty,
    };
    let comps = rim_merge::tree::FieldNode {
        tag: "comps".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(vec![li]),
    };
    // Document order: `comps/li[...]` (index 0), then `label` (index 1).
    let effective = sort_test_effective(vec![comps, sort_test_leaf("label")]);

    let li_path: FieldPath = "comps/li[@Class=Comp]".parse().expect("valid path");
    let label_path: FieldPath = "label".parse().expect("valid path");

    let mut rows = vec![
        sort_test_row(label_path.clone(), FieldRowKind::Unchanged),
        sort_test_row(li_path.clone(), FieldRowKind::ListEntry),
        sort_test_row(label_path.clone(), FieldRowKind::Conflict),
    ];
    sort_field_rows(&mut rows, &effective);

    assert_eq!(rows[0].kind, FieldRowKind::Conflict, "{rows:?}");
    assert_eq!(
        rows[1].path, li_path,
        "ListEntry sorts by its real document position ({rows:?}), not lexically after label"
    );
    assert_eq!(rows[2].path, label_path);
    assert_eq!(rows[2].kind, FieldRowKind::Unchanged);
}

/// Pins the `.unwrap_or(usize::MAX)` fallback the doc comment
/// describes — a row naming a path with *no*
/// descendant anywhere in `resolved` (a later patch removed the whole
/// entry) sorts last, not first (`None < Some(_)` would otherwise put
/// it first if this fell back to `0`/stayed unordered).
#[test]
fn sort_field_rows_sorts_a_fully_clobbered_row_last() {
    let effective = sort_test_effective(vec![sort_test_leaf("label")]);

    let label_path: FieldPath = "label".parse().expect("valid path");
    let removed_path: FieldPath = "comps/li[@Class=Removed]".parse().expect("valid path");

    let mut rows = vec![
        sort_test_row(removed_path.clone(), FieldRowKind::ListEntry),
        sort_test_row(label_path.clone(), FieldRowKind::CleanMerge),
    ];
    sort_field_rows(&mut rows, &effective);

    assert_eq!(rows[0].path, label_path, "{rows:?}");
    assert_eq!(rows[1].path, removed_path);
}
