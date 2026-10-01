//! Tests for finding extraction.

use rim_analyzer::domain::{
    Conflict, DanglingCause, DanglingDefReference, DefOverride, LoadOrder, Report,
};

use super::*;
use crate::domain::{DefKey, RuleOrigin};
use crate::test_support::ReportBuilder;
use rim_analyzer::domain::EdgeKind;

fn insertion_order(report: &Report) -> LoadOrder {
    LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect())
}

/// A dropped `Soft`-strength `AssemblyRef` must carry `strength: Soft`
/// on its `Finding::EdgeDropped`, not `Hard` — `EdgeKind::strength()`
/// alone can't make that distinction (see
/// `Finding::EdgeDropped::strength`'s own doc comment), so this must
/// come from the dropped edge's own layer (`Layer::Soft`, since it was
/// only added to the graph at all because `enforce.soft` was `true`).
#[test]
fn a_dropped_soft_assembly_ref_finding_carries_soft_strength_not_hard() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .soft_edge("a", "b") // a after b
        .soft_edge("b", "a") // b after a: a genuine two-cycle, entirely Soft
        .build();
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers {
            soft: true,
            awareness: false,
            inferred: true,
        },
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    assert_eq!(
        sort_outcome.dropped.len(),
        1,
        "one edge of the two-cycle must be dropped"
    );

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    let dropped_finding = findings
        .values()
        .find_map(|finding| match finding {
            Finding::EdgeDropped { strength, .. } => Some(*strength),
            _ => None,
        })
        .expect("the dropped edge must surface as an EdgeDropped finding");
    assert_eq!(
        dropped_finding,
        EdgeStrength::Soft,
        "a dropped Soft edge must never be scored as Hard"
    );
}

/// An advisory (unenforced) Awareness edge is never added to the
/// graph, so it's never dropped either — it must never surface as an
/// `EdgeDropped` finding, which in turn means it can never be counted
/// in `resolved_by_suggested` (see `ledger::build::resolved_under_suggested`,
/// which only ever looks at `FindingKey::EdgeDropped`/`AnyOfChoice`/
/// `UndeclaredHardDependency` — none of which an excluded edge can
/// produce).
#[test]
fn an_advisory_edge_never_becomes_an_edge_dropped_finding() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .awareness_edge("a", "b")
        .build();
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    assert!(sort_outcome.dropped.is_empty());
    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );
    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::EdgeDropped { .. })),
        "an advisory edge must never produce an EdgeDropped finding"
    );
}

/// An advisory `Awareness`
/// edge pointing the opposite way from an accepted `Declared` edge
/// between the same two mods must surface as `DeclarationQuestioned`,
/// naming the opposing relation's own kind.
#[test]
fn an_advisory_edge_opposing_a_declared_edge_becomes_a_declaration_questioned_finding() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .declared_edge("a", "b") // a after b: accepted (Declared).
        .awareness_edge("b", "a") // b after a: the opposite direction, advisory.
        .build();
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    assert!(sort_outcome.dropped.is_empty());

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    let key = FindingKey::DeclarationQuestioned {
        declared_after: ModId::new("a"),
        declared_before: ModId::new("b"),
        relation_kind: rim_analyzer::domain::EdgeKind::MayRequire,
    };
    let Some(Finding::DeclarationQuestioned {
        declared_after,
        declared_before,
        relation_kind,
        ..
    }) = findings.get(&key)
    else {
        panic!(
            "expected a DeclarationQuestioned finding for {key:?}, found: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(*declared_after, ModId::new("a"));
    assert_eq!(*declared_before, ModId::new("b"));
    assert_eq!(*relation_kind, rim_analyzer::domain::EdgeKind::MayRequire);
}

/// A `Soft`-strength advisory edge must never produce a
/// `DeclarationQuestioned` finding, even when it opposes an accepted
/// `Declared` edge — `LazyReferenceViolated` already covers a violated
/// `Soft` edge, and `DeclarationQuestioned`'s fixed-rationale list is
/// `Awareness`-only.
#[test]
fn a_soft_advisory_edge_opposing_a_declared_edge_is_not_declaration_questioned() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .declared_edge("a", "b") // a after b: accepted (Declared).
        .soft_edge("b", "a") // b after a: the opposite direction, advisory (Soft).
        .build();
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::DeclarationQuestioned { .. })),
        "a Soft advisory edge must not produce DeclarationQuestioned: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// A db-origin (`RimSortUser`/`RimSortCommunity`/
/// `SteamDb`) pair rule opposed by an `Awareness`-strength derived
/// edge — the real shape of the two known contradicting real-install
/// pairs, both opposed by a `ParentTemplate` edge — surfaces
/// as `DeclarationQuestioned` through the ordinary path: `declared.layer`'s
/// own match arm covers every db layer, and this test names a db-origin
/// rule instead of an engine `Declared` edge as the accepted side. No
/// new finding kind is
/// needed (`RuleOverruled` cannot fire here — an `Awareness` edge is
/// never enforced, so nothing is ever dropped to produce a cycle for
/// it to report).
#[test]
fn a_db_pair_rule_opposing_an_awareness_edge_becomes_a_declaration_questioned_finding() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .awareness_edge("b", "a") // b after a: derived (e.g. ParentTemplate), advisory only.
        .build();
    let rules = RuleSet::new(vec![pair_rule("a", "b", RuleOrigin::SteamDb)]); // a after b.
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::DeclarationQuestioned {
        declared_after: ModId::new("a"),
        declared_before: ModId::new("b"),
        relation_kind: rim_analyzer::domain::EdgeKind::MayRequire,
    };
    assert!(
        findings.contains_key(&key),
        "expected {key:?} in {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(
        !findings
            .keys()
            .any(|k| matches!(k, FindingKey::RuleOverruled { .. })),
        "an Awareness edge is never enforced, so the rule is never dropped and \
             RuleOverruled must not fire: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// `Soft` is advisory by default (see `EnforcedLayers::default`), so a
/// lazily-resolved `AssemblyRef` the current order doesn't satisfy is
/// never fixed by the sorter — it must still surface as a
/// `LazyReferenceViolated` finding rather than being silently dropped,
/// so "show all" never hides it.
#[test]
fn a_violated_soft_edge_becomes_a_lazy_reference_violated_finding() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .soft_edge("a", "b") // a after b
        .build();
    // Insertion order is [a, b]: a before b, violating "a after b".
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    // Advisory by default: the sorter never touches the order to fix it.
    assert_eq!(sort_outcome.order.as_slice(), current.as_slice());

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(findings.contains_key(&FindingKey::LazyReferenceViolated {
        after: ModId::new("a"),
        before: ModId::new("b"),
    }));
}

#[test]
fn a_satisfied_soft_edge_produces_no_finding() {
    let report = ReportBuilder::new()
        .mod_("b")
        .mod_("a")
        .soft_edge("a", "b") // a after b
        .build();
    // Insertion order is [b, a]: already satisfies "a after b".
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::LazyReferenceViolated { .. }))
    );
}

#[test]
fn missing_mods_and_unsupported_versions_become_findings() {
    let report = ReportBuilder::new().mod_("a").missing_mod("gone").build();
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(findings.contains_key(&FindingKey::MissingMod {
        mod_id: ModId::new("gone")
    }));
}

#[test]
fn def_override_conflicts_become_findings() {
    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.conflicts.push(Conflict::DefOverride(DefOverride {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
        owners: vec![ModId::new("a"), ModId::new("b")],
        overrides_vanilla: false,
        same_author: false,
    }));
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };
    assert!(findings.contains_key(&key));
}

/// `Conflict::DanglingDefReference` becomes a finding only when
/// `show_dangling_def_references` is `true` — off by default (see
/// `rim_session::Settings::show_dangling_def_references`'s own doc
/// comment for the measured false-positive rate behind that default).
#[test]
fn dangling_def_reference_conflicts_are_gated_by_show_dangling_def_references() {
    let mut report = ReportBuilder::new().mod_("a").build();
    report
        .conflicts
        .push(Conflict::DanglingDefReference(DanglingDefReference {
            name: "GhostDef".to_string(),
            referrers: Vec::new(),
            truncated_referrers: 0,
            cause: DanglingCause::DefinedNowhere,
            likely_sound: false,
        }));
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let key = FindingKey::DanglingDefReference {
        name: "GhostDef".to_string(),
    };

    let findings_off = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );
    assert!(!findings_off.contains_key(&key));

    let findings_on = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        true,
    );
    assert!(findings_on.contains_key(&key));
}

/// Every kind that can name the generated merge mod as a mod id must
/// be dropped once it does, and an otherwise-identical finding naming
/// a real mod must survive untouched.
#[test]
fn every_listed_kind_naming_the_generated_merge_mod_is_dropped() {
    let generated = ModId::new("rimmerge.merge.3f9a1c2b7d5e");
    let real = ModId::new("example.bionicsfork");
    // No marker: relies on the id-prefix fallback, same as a cached report
    // written before generated mods carried a marker.
    let generated_mods =
        GeneratedMods::from_report(&ReportBuilder::new().mod_(generated.as_str()).build());
    let def_key = DefKey {
        def_type: "BiomeDef".to_string(),
        def_name: "TemperateForest".to_string(),
    };

    let cases: Vec<(FindingKey, FindingKey)> = vec![
        (
            FindingKey::PatchCollision {
                key: def_key.clone(),
                selector: rim_analyzer::domain::Selector::DefName,
                sub_path: None,
                mods: [generated.clone()].into_iter().collect(),
            },
            FindingKey::PatchCollision {
                key: def_key.clone(),
                selector: rim_analyzer::domain::Selector::DefName,
                sub_path: None,
                mods: [real.clone()].into_iter().collect(),
            },
        ),
        (
            FindingKey::TextureOverride {
                texture_path: "Things/Wall.png".to_string(),
                owners: [generated.clone()].into_iter().collect(),
            },
            FindingKey::TextureOverride {
                texture_path: "Things/Wall.png".to_string(),
                owners: [real.clone()].into_iter().collect(),
            },
        ),
        (
            FindingKey::UndeclaredHardDependency {
                after: generated.clone(),
                before: real.clone(),
            },
            FindingKey::UndeclaredHardDependency {
                after: real.clone(),
                before: ModId::new("another.mod"),
            },
        ),
        (
            FindingKey::LazyReferenceViolated {
                after: generated.clone(),
                before: real.clone(),
            },
            FindingKey::LazyReferenceViolated {
                after: real.clone(),
                before: ModId::new("another.mod"),
            },
        ),
        (
            FindingKey::UnsupportedVersion {
                mod_id: generated.clone(),
            },
            FindingKey::UnsupportedVersion {
                mod_id: real.clone(),
            },
        ),
        (
            FindingKey::MissingDependency {
                mod_id: generated.clone(),
                dependency: real.clone(),
            },
            FindingKey::MissingDependency {
                mod_id: real.clone(),
                dependency: ModId::new("another.mod"),
            },
        ),
        (
            FindingKey::TagInferred {
                mod_id: generated.clone(),
                tag: crate::domain::Tag::new("framework").unwrap(),
            },
            FindingKey::TagInferred {
                mod_id: real.clone(),
                tag: crate::domain::Tag::new("framework").unwrap(),
            },
        ),
    ];

    for (generated_key, real_key) in cases {
        assert!(
            hidden_by_generated(&generated_key, &generated_mods),
            "{generated_key:?} must be classified as naming the generated merge mod"
        );
        assert!(
            !hidden_by_generated(&real_key, &generated_mods),
            "{real_key:?} must not be classified as naming the generated merge mod"
        );
    }
}

/// A kind that can never name a mod owning `Defs/`/assemblies (the
/// generated mod ships neither) must never be filtered, even when its
/// own id field happens to collide with the generated prefix — this
/// pins the exhaustive match's "always false" arms.
#[test]
fn kinds_that_can_never_name_the_generated_mod_are_never_filtered() {
    let generated = ModId::new("rimmerge.merge.3f9a1c2b7d5e");
    let generated_mods =
        GeneratedMods::from_report(&ReportBuilder::new().mod_(generated.as_str()).build());
    let key = FindingKey::MissingMod { mod_id: generated };
    assert!(!hidden_by_generated(&key, &generated_mods));
}

// -- scope-aware hiding -----------------------------

fn patch_marker(scope: BTreeSet<ModId>) -> rim_analyzer::domain::GeneratedMarker {
    rim_analyzer::domain::GeneratedMarker {
        kind: rim_analyzer::domain::GeneratedKind::Patch,
        patch_id: None,
        scope: Some(scope),
    }
}

/// A finding naming the (unrestricted) profile merge mod is hidden
/// regardless of who else it names.
#[test]
fn a_finding_naming_the_profile_merge_mod_is_hidden() {
    let merge_mod = ModId::new("rimmerge.merge.abc123456789");
    let generated_mods = GeneratedMods::from_report(
        &ReportBuilder::new()
            .mod_with(merge_mod.as_str(), |m| {
                m.generated = Some(rim_analyzer::domain::GeneratedMarker {
                    kind: rim_analyzer::domain::GeneratedKind::Merge,
                    patch_id: None,
                    scope: None,
                });
            })
            .build(),
    );
    let key = FindingKey::DuplicateAssembly {
        assembly_name: "Foo".to_string(),
        owners: [merge_mod].into_iter().collect(),
    };
    // `DuplicateAssembly` is structurally impossible for a generated mod
    // (it ships no assemblies), so exercise the rule through a kind that
    // actually applies it: `TagInferred`.
    let tag_key = FindingKey::TagInferred {
        mod_id: ModId::new("rimmerge.merge.abc123456789"),
        tag: crate::domain::Tag::new("framework").unwrap(),
    };
    assert!(!hidden_by_generated(&key, &generated_mods)); // DuplicateAssembly: always false
    assert!(hidden_by_generated(&tag_key, &generated_mods));
}

/// A `PatchCollision` between a compat patch and a mod fully inside its
/// declared scope is hidden.
#[test]
fn a_finding_between_a_compat_patch_and_a_scope_member_is_hidden() {
    let patch = ModId::new("sample.abcompat");
    let member = ModId::new("fixture.moda");
    let scope: BTreeSet<ModId> = [member.clone()].into_iter().collect();
    let generated_mods = GeneratedMods::from_report(
        &ReportBuilder::new()
            .mod_with(patch.as_str(), |m| {
                m.generated = Some(patch_marker(scope));
            })
            .build(),
    );
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: rim_analyzer::domain::Selector::DefName,
        sub_path: None,
        mods: [patch, member].into_iter().collect(),
    };

    assert!(hidden_by_generated(&key, &generated_mods));
}

/// A `TextureOverride` between a compat patch and a mod *outside* its
/// declared scope must stay visible — the patch may be overwriting a
/// def or texture it doesn't actually own in this install.
#[test]
fn a_finding_between_a_compat_patch_and_a_mod_outside_its_scope_is_not_hidden() {
    let patch = ModId::new("sample.abcompat");
    let member = ModId::new("fixture.moda");
    let outsider = ModId::new("fixture.modc");
    let scope: BTreeSet<ModId> = [member.clone()].into_iter().collect();
    let generated_mods = GeneratedMods::from_report(
        &ReportBuilder::new()
            .mod_with(patch.as_str(), |m| {
                m.generated = Some(patch_marker(scope));
            })
            .build(),
    );
    let key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: [patch, member, outsider].into_iter().collect(),
    };

    assert!(!hidden_by_generated(&key, &generated_mods));
}

/// Two Rimmerge-generated compat patches with overlapping scopes,
/// colliding with each other: neither patch's declared scope covers
/// the other patch (a patch's scope lists ordinary mods, not other
/// patches), so the collision between them stays visible — the "two
/// active Rimmerge patches" risk, deliberately not solved.
#[test]
fn two_patches_with_overlapping_scopes_stay_visible_against_each_other() {
    let patch_a = ModId::new("sample.abcompat");
    let patch_b = ModId::new("someone.abcompat2");
    let shared_member = ModId::new("fixture.moda");
    let scope_a: BTreeSet<ModId> = [shared_member.clone()].into_iter().collect();
    let scope_b: BTreeSet<ModId> = [shared_member].into_iter().collect();
    let generated_mods = GeneratedMods::from_report(
        &ReportBuilder::new()
            .mod_with(patch_a.as_str(), |m| {
                m.generated = Some(patch_marker(scope_a));
            })
            .mod_with(patch_b.as_str(), |m| {
                m.generated = Some(patch_marker(scope_b));
            })
            .build(),
    );
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: rim_analyzer::domain::Selector::DefName,
        sub_path: None,
        mods: [patch_a, patch_b].into_iter().collect(),
    };

    assert!(!hidden_by_generated(&key, &generated_mods));
}

/// End-to-end through the public `extract` (re-exported as
/// `rim_resolve::ledger::extract_findings`): a `PatchCollision`
/// naming the generated merge mod must not survive the real pipeline
/// (analyzer `Conflict` -> sorter -> `extract`), while a sibling
/// collision naming a real mod must.
#[test]
fn extract_drops_a_patch_collision_naming_the_generated_merge_mod_but_keeps_a_real_one() {
    use rim_analyzer::domain::{
        PatchCollision, PatchCollisionEntry, PatchCollisionSeverity, Selector,
    };

    let generated = ModId::new("rimmerge.merge.abc123456789");
    let real = ModId::new("example.bionicsfork");
    let mut report = ReportBuilder::new()
        .mod_(generated.as_str())
        .mod_(real.as_str())
        .build();
    report
        .conflicts
        .push(Conflict::PatchCollision(PatchCollision {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
            selector: Selector::DefName,
            sub_path: Some("plantDensity".to_string()),
            mods: vec![PatchCollisionEntry {
                mod_id: generated.clone(),
                op_class: "PatchOperationReplace".to_string(),
            }],
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        }));
    report
        .conflicts
        .push(Conflict::PatchCollision(PatchCollision {
            def_type: "BiomeDef".to_string(),
            def_name: "Tundra".to_string(),
            selector: Selector::DefName,
            sub_path: Some("plantDensity".to_string()),
            mods: vec![PatchCollisionEntry {
                mod_id: real.clone(),
                op_class: "PatchOperationReplace".to_string(),
            }],
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        }));
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(
        !findings.keys().any(|key| matches!(key,
            FindingKey::PatchCollision { key: def_key, .. }
            if def_key.def_name == "TemperateForest"
        )),
        "the collision naming the generated merge mod must be dropped: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(
        findings.keys().any(|key| matches!(key,
            FindingKey::PatchCollision { key: def_key, .. }
            if def_key.def_name == "Tundra"
        )),
        "the sibling collision naming a real mod must survive: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// End-to-end through `extract`: a `TagInferred` assignment naming the
/// generated merge mod must be dropped, while an identical assignment
/// naming a real mod survives — the second line of defense behind
/// `infer_tags` itself already excluding the generated mod (see
/// `crate::tags::infer` tests).
#[test]
fn extract_drops_tag_inferred_naming_the_generated_merge_mod_but_keeps_a_real_one() {
    let generated = ModId::new("rimmerge.merge.abc123456789");
    let real = ModId::new("some.mod");
    let report = ReportBuilder::new()
        .mod_(generated.as_str())
        .mod_(real.as_str())
        .build();
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let tag = crate::domain::Tag::new("framework").unwrap();
    let provenance = || TagProvenance::Inferred {
        matched: Vec::new(),
        confidence: crate::domain::Confidence::new(30).unwrap(),
    };
    let tagging = Tagging::new(vec![
        crate::domain::TagAssignment {
            mod_id: generated.clone(),
            tag: tag.clone(),
            provenance: provenance(),
        },
        crate::domain::TagAssignment {
            mod_id: real.clone(),
            tag: tag.clone(),
            provenance: provenance(),
        },
    ]);

    let findings = extract(
        &report,
        &sort_outcome,
        &tagging,
        &sort_outcome.order,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(
        !findings.keys().any(|key| matches!(key,
            FindingKey::TagInferred { mod_id, .. } if *mod_id == generated
        )),
        "a tag inferred for the generated merge mod must be dropped: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(
        findings.keys().any(|key| matches!(key,
            FindingKey::TagInferred { mod_id, .. } if *mod_id == real
        )),
        "a tag inferred for a real mod must survive: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

fn extract_for(report: &Report) -> BTreeMap<FindingKey, Finding> {
    extract_for_with_rules(report, &crate::domain::RuleSet::default())
}

/// Like [`extract_for`], but against a caller-supplied [`RuleSet`] —
/// needed by the `RuleOverruled`/placement-finding tests, which name a
/// placement/pair rule's origin.
fn extract_for_with_rules(
    report: &Report,
    rules: &crate::domain::RuleSet,
) -> BTreeMap<FindingKey, Finding> {
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report,
        rules,
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    extract(
        report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        rules,
        false,
    )
}

#[test]
fn duplicate_template_name_conflicts_become_findings() {
    use rim_analyzer::domain::DuplicateTemplateName;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report
        .conflicts
        .push(Conflict::DuplicateTemplateName(DuplicateTemplateName {
            name: "WallBase".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        }));

    let findings = extract_for(&report);

    let key = FindingKey::DuplicateTemplateName {
        name: "WallBase".to_string(),
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };
    assert!(findings.contains_key(&key));
}

#[test]
fn sound_override_conflicts_become_findings() {
    use rim_analyzer::domain::SoundOverride;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report
        .conflicts
        .push(Conflict::SoundOverride(SoundOverride {
            path: "shot_fire".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
            same_author: false,
        }));

    let findings = extract_for(&report);

    let key = FindingKey::SoundOverride {
        path: "shot_fire".to_string(),
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };
    assert!(findings.contains_key(&key));
}

/// Two `KeyedTranslationCollision` conflicts over the same mod pair but
/// different keys must collapse into one `Finding` naming both keys —
/// not two separate findings — per `FindingKey::KeyedTranslationCollision`'s
/// own doc comment.
#[test]
fn keyed_translation_collisions_over_the_same_pair_are_grouped_into_one_finding() {
    use rim_analyzer::domain::KeyedTranslationCollision;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.conflicts.push(Conflict::KeyedTranslationCollision(
        KeyedTranslationCollision {
            key: "Greeting".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        },
    ));
    report.conflicts.push(Conflict::KeyedTranslationCollision(
        KeyedTranslationCollision {
            key: "Farewell".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        },
    ));

    let findings = extract_for(&report);

    let key = FindingKey::KeyedTranslationCollision {
        pair: (ModId::new("a"), ModId::new("b")),
    };
    match findings.get(&key) {
        Some(Finding::KeyedTranslationCollision { keys, .. }) => {
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(sorted, vec!["Farewell".to_string(), "Greeting".to_string()]);
        }
        other => {
            panic!("expected one grouped KeyedTranslationCollision finding, got {other:?}")
        }
    }
    assert_eq!(
        findings
            .keys()
            .filter(|k| matches!(k, FindingKey::KeyedTranslationCollision { .. }))
            .count(),
        1,
        "the pair must produce exactly one finding regardless of shared key count"
    );
}

/// Three owners sharing one collision key produce every pairwise
/// combination as its own finding (three mods -> three pairs).
#[test]
fn a_three_owner_keyed_translation_collision_produces_every_pair() {
    use rim_analyzer::domain::KeyedTranslationCollision;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").mod_("c").build();
    report.conflicts.push(Conflict::KeyedTranslationCollision(
        KeyedTranslationCollision {
            key: "Greeting".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b"), ModId::new("c")],
        },
    ));

    let findings = extract_for(&report);

    for (x, y) in [("a", "b"), ("a", "c"), ("b", "c")] {
        let key = FindingKey::KeyedTranslationCollision {
            pair: (ModId::new(x), ModId::new(y)),
        };
        assert!(findings.contains_key(&key), "missing pair ({x}, {y})");
    }
}

/// One `Conflict::RuntimePatchCollision` becomes one finding, keyed by
/// its own target — not grouped by mod pair (see
/// `FindingKey::RuntimePatchCollision`'s own doc comment for why).
/// Two conflicts sharing the same owners but different targets must
/// stay two separate findings.
#[test]
fn runtime_patch_collisions_are_keyed_per_target_not_per_pair() {
    use rim_analyzer::domain::RuntimePatchCollision;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report
        .conflicts
        .push(Conflict::RuntimePatchCollision(RuntimePatchCollision {
            target_type: "Verse.Pawn".to_string(),
            target_method: "Kill".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        }));
    report
        .conflicts
        .push(Conflict::RuntimePatchCollision(RuntimePatchCollision {
            target_type: "Verse.Pawn".to_string(),
            target_method: "TakeDamage".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        }));

    let findings = extract_for(&report);

    for target_method in ["Kill", "TakeDamage"] {
        let key = FindingKey::RuntimePatchCollision {
            target_type: "Verse.Pawn".to_string(),
            target_method: target_method.to_string(),
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        match findings.get(&key) {
            Some(Finding::RuntimePatchCollision { owners, .. }) => {
                assert_eq!(owners, &vec![ModId::new("a"), ModId::new("b")]);
            }
            other => {
                panic!("expected a RuntimePatchCollision finding for {key:?}, got {other:?}")
            }
        }
    }
    assert_eq!(
        findings
            .keys()
            .filter(|k| matches!(k, FindingKey::RuntimePatchCollision { .. }))
            .count(),
        2,
        "two distinct targets must produce two findings, not one grouped by pair"
    );
}

/// One `Conflict::TranspilerCollision`
/// becomes one finding, keyed by its own target — the same shape
/// `RuntimePatchCollision` uses, scoped to the transpiler-declaring
/// owners only.
#[test]
fn transpiler_collision_becomes_a_finding_keyed_by_its_own_target() {
    use rim_analyzer::domain::TranspilerCollision;

    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report
        .conflicts
        .push(Conflict::TranspilerCollision(TranspilerCollision {
            target_type: "Verse.Verb_LaunchProjectile".to_string(),
            target_method: "TryCastShot".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        }));

    let findings = extract_for(&report);

    let key = FindingKey::TranspilerCollision {
        target_type: "Verse.Verb_LaunchProjectile".to_string(),
        target_method: "TryCastShot".to_string(),
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };
    match findings.get(&key) {
        Some(Finding::TranspilerCollision { owners, .. }) => {
            assert_eq!(owners, &vec![ModId::new("a"), ModId::new("b")]);
        }
        other => panic!("expected a TranspilerCollision finding for {key:?}, got {other:?}"),
    }
}

/// A `UsesType` edge with no existing `Hard`/`Declared` edge in the
/// same direction becomes an `UndeclaredTypeDependency` finding, with
/// the type name recovered from the edge's own `detail` text.
#[test]
fn an_undeclared_uses_type_edge_becomes_a_finding() {
    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.edges.push(rim_analyzer::domain::EdgeReport {
        edge: rim_analyzer::domain::Edge {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind: EdgeKind::UsesType,
            detail: "names type 'Framework.Utils' from b's assembly".to_string(),
            load_time: true,
            subject: Some("Framework.Utils".to_string()),
        },
        status: rim_analyzer::domain::EdgeStatus::Unevaluated,
    });

    let findings = extract_for(&report);

    let key = FindingKey::UndeclaredTypeDependency {
        user: ModId::new("a"),
        provider: ModId::new("b"),
        type_name: "Framework.Utils".to_string(),
    };
    assert!(findings.contains_key(&key));
}

/// The same `UsesType` edge, but `a` already declares a `loadAfter` on
/// `b` (a `Declared`-strength edge in the same direction) — no
/// `UndeclaredTypeDependency` finding, since the relation is already
/// declared.
#[test]
fn a_declared_uses_type_edge_produces_no_undeclared_type_dependency_finding() {
    let mut report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .declared_edge("a", "b")
        .build();
    report.edges.push(rim_analyzer::domain::EdgeReport {
        edge: rim_analyzer::domain::Edge {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind: EdgeKind::UsesType,
            detail: "names type 'Framework.Utils' from b's assembly".to_string(),
            load_time: true,
            subject: Some("Framework.Utils".to_string()),
        },
        status: rim_analyzer::domain::EdgeStatus::Unevaluated,
    });

    let findings = extract_for(&report);

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::UndeclaredTypeDependency { .. })),
        "a declared relation must suppress the finding: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// An edge of the `PatchSelectsInjectedNode` kind — the path-shape
/// sibling of `UsesType` — must never become an
/// `UndeclaredTypeDependency` finding, even though it carries the same
/// `subject`/strength shape `UsesType` does: the finding rule keys on
/// the kind, so a path-shape edge never reaches it. Folding the path
/// shape back into `UsesType` (or dropping
/// `insert_undeclared_type_dependencies`'s own `!= EdgeKind::UsesType`
/// guard) would make this fail by producing a false "uses a type from
/// ... assembly" claim for what is really an XML path.
#[test]
fn a_patch_selects_injected_node_edge_produces_no_undeclared_type_dependency_finding() {
    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.edges.push(rim_analyzer::domain::EdgeReport {
            edge: rim_analyzer::domain::Edge {
                after: ModId::new("a"),
                before: ModId::new("b"),
                kind: EdgeKind::PatchSelectsInjectedNode,
                detail: "patch selects 'ThingDef/XFE_FueledSmelter/comps', injected by b but also written inline".to_string(),
                load_time: true,
                subject: Some("ThingDef/XFE_FueledSmelter/comps".to_string()),
            },
            status: rim_analyzer::domain::EdgeStatus::Unevaluated,
        });

    let findings = extract_for(&report);

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::UndeclaredTypeDependency { .. })),
        "a PatchSelectsInjectedNode edge must never surface as UndeclaredTypeDependency \
             (it names an XML path, not a type): {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// `Edge.subject` (`Report` schema 4) carries the used type, with no
/// `detail`-text parse. Builds a real
/// `UsesType` edge through `rim_analyzer::analysis::edges::uses_type_edges`
/// itself, not a hand-built literal, so this test also guards that
/// function's own `subject` wiring — not just this module's consumer
/// of it.
#[test]
fn a_real_uses_type_edge_carries_a_subject_and_becomes_a_finding() {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use rim_analyzer::analysis::edges::uses_type_edges;
    use rim_analyzer::analysis::indices::{ActiveMods, Indices};
    use rim_analyzer::domain::{AssemblyInfo, DeclaredOrder, Mod, ScannedMod, Source};

    fn bare_mod(id: &str) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            url: None,
            path: PathBuf::from(id),
            source: Source::Local,
            supported_versions: Vec::new(),
            declared: DeclaredOrder::default(),
            loaded_folders: Vec::new(),
            hard_dependents: 0,
            soft_dependents: 0,
            awareness_dependents: 0,
            is_framework_candidate: false,
            generated: None,
            workshop_id: None,
            load_folders_version_matched: None,
        }
    }

    fn scanned(info: Mod, assemblies: Vec<AssemblyInfo>) -> ScannedMod {
        ScannedMod {
            info,
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies,
            sounds: BTreeSet::new(),
            translation_keys: BTreeSet::new(),
            inline_types: BTreeSet::new(),
            manifest_order: Default::default(),
            texture_path_candidates: Vec::new(),
            inline_node_path_hashes: std::collections::HashSet::new(),
            if_mod_active_targets: Vec::new(),
            scan_cost: rim_analyzer::domain::ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    let mut user = scanned(bare_mod("user"), Vec::new());
    user.inline_types = BTreeSet::from(["Framework.Utils".to_string()]);
    let provider = scanned(
        bare_mod("provider"),
        vec![AssemblyInfo {
            file_name: "Framework".to_string(),
            // `assembly_owners` is keyed by exactly `AssemblyInfo.name`
            // (`dll_owner_of` lowercases the *type name* segment it
            // looks up, not this map's own keys), and the real scan
            // pipeline always stores a shipped assembly's name already
            // lowercased — mirrored here rather than relying on
            // `dll_owner_of`'s lookup to normalize a differently-cased
            // key.
            name: "framework".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let scanned_mods = vec![user, provider];
    let active = ActiveMods::build(&scanned_mods);
    let indices = Indices::build(
        &scanned_mods,
        &std::collections::HashSet::new(),
        &active,
        &std::collections::BTreeSet::new(),
    );

    let edges = uses_type_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1, "expected exactly one UsesType edge");
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("Framework.Utils"),
        "uses_type_edges must set subject to the type name"
    );

    let mut report = ReportBuilder::new().mod_("user").mod_("provider").build();
    report.edges.push(rim_analyzer::domain::EdgeReport {
        edge: edges[0].clone(),
        status: rim_analyzer::domain::EdgeStatus::Unevaluated,
    });

    let findings = extract_for(&report);

    let key = FindingKey::UndeclaredTypeDependency {
        user: ModId::new("user"),
        provider: ModId::new("provider"),
        type_name: "Framework.Utils".to_string(),
    };
    assert!(
        findings.contains_key(&key),
        "expected {key:?} in {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

// -- RuleOverruled ------------------------------------------

fn pair_rule(after: &str, before: &str, origin: RuleOrigin) -> crate::domain::Rule {
    crate::domain::Rule::Pair(crate::domain::PairRule {
        after: ModId::new(after),
        before: ModId::new(before),
        origin,
        comment: None,
        overrides_declared: false,
    })
}

/// [`pair_rule`]'s declared-edge-override sibling:
/// always `RuleOrigin::UserDecision` with `overrides_declared: true`
/// — the only shape the sorter actually honours the flag for.
fn override_pair_rule(after: &str, before: &str) -> crate::domain::Rule {
    crate::domain::Rule::Pair(crate::domain::PairRule {
        after: ModId::new(after),
        before: ModId::new(before),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: true,
    })
}

/// A db pair rule
/// dropped by a `Hard` winner yields `RuleOverruled` at confidence 95
/// with no `Reorder` alternative (a one-click override must never sit
/// next to a load-time fact).
#[test]
fn a_pair_rule_dropped_by_a_hard_edge_becomes_a_rule_overruled_finding_at_95_no_reorder() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .hard_edge("a", "b") // a after b (Hard): accepted first.
        .build();
    let rules = RuleSet::new(vec![pair_rule("b", "a", RuleOrigin::RimSortCommunity)]);
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::RuleOverruled {
        after: ModId::new("b"),
        before: ModId::new("a"),
        origin: RuleOrigin::RimSortCommunity,
    };
    let Some(Finding::RuleOverruled { winner, .. }) = findings.get(&key) else {
        panic!(
            "expected RuleOverruled for {key:?}, found: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    let winner = winner
        .as_ref()
        .expect("a direct 2-cycle must name a winner");
    assert_eq!(
        winner.layer,
        Layer::Hard,
        "the finding's own winner must be the Hard edge; confidence/alternatives for this \
             shape are covered directly in ledger::suggest's own tests"
    );
}

/// The same shape, but the losing rule's own origin is `UserDecision`
/// (the user's own prior decision, not an import) — same finding kind,
/// same key shape: a user decision overruled by anything is the same
/// finding with origin `UserDecision`.
#[test]
fn a_user_decision_pair_rule_dropped_by_a_hard_edge_is_still_rule_overruled() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .hard_edge("a", "b")
        .build();
    let rules = RuleSet::new(vec![pair_rule("b", "a", RuleOrigin::UserDecision)]);
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::RuleOverruled {
        after: ModId::new("b"),
        before: ModId::new("a"),
        origin: RuleOrigin::UserDecision,
    };
    assert!(
        findings.contains_key(&key),
        "expected {key:?} in {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// An imported pair rule
/// and its own promoted `UserDecision` copy (same key, per
/// `Session::promote_imported_rule`) can both lose the same cycle to
/// the same winner, since the sorter adds a rule-origin edge per
/// origin, independently. Without deduping, that would produce two
/// `RuleOverruled` findings — one per origin — for what a user
/// perceives as one rule. `effective_pair_rule_origin` collapses them
/// into the one keyed on the effective (user-owned) rule, which also
/// withholds `Promote` for free (`rule_overruled`'s own gate already
/// skips it for a `UserDecision` origin).
#[test]
fn a_pair_rule_and_its_promoted_copy_dropped_by_the_same_winner_collapse_to_one_finding() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .hard_edge("a", "b") // a after b (Hard): accepted first.
        .build();
    let rules = RuleSet::new(vec![
        pair_rule("b", "a", RuleOrigin::UserDecision),
        pair_rule("b", "a", RuleOrigin::RimSortCommunity),
    ]);
    let findings = extract_for_with_rules(&report, &rules);

    let user_key = FindingKey::RuleOverruled {
        after: ModId::new("b"),
        before: ModId::new("a"),
        origin: RuleOrigin::UserDecision,
    };
    let community_key = FindingKey::RuleOverruled {
        after: ModId::new("b"),
        before: ModId::new("a"),
        origin: RuleOrigin::RimSortCommunity,
    };
    assert!(
        findings.contains_key(&user_key),
        "expected one RuleOverruled keyed on the effective (UserDecision) rule: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(
        !findings.contains_key(&community_key),
        "the imported rule's own origin must not also produce a separate finding once a \
             promoted copy exists: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

// -- Declared-edge override -------------

/// The motivating case: a `UserDecision` pair rule flagged
/// `overrides_declared` beats a `Declared`-strength edge (the author's
/// own `loadAfter`) directly contradicting it — the `Declared` edge
/// is the one dropped, `EdgeDropped`'s own `winner` names the override
/// rule at `Layer::DeclaredOverride`, and the new, loud
/// `DeclarationOverridden` finding fires alongside it.
#[test]
fn an_override_flagged_pair_rule_beats_a_declared_edge_and_discloses_it() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .declared_edge("a", "b") // Declared: a after b.
        .build();
    let rules = RuleSet::new(vec![override_pair_rule("b", "a")]); // b after a: the opposite.
    let findings = extract_for_with_rules(&report, &rules);

    let dropped_key = FindingKey::EdgeDropped {
        after: ModId::new("a"),
        before: ModId::new("b"),
        kind: EdgeKind::LoadAfter,
    };
    let Some(Finding::EdgeDropped { winner, .. }) = findings.get(&dropped_key) else {
        panic!(
            "expected the Declared edge itself to be dropped: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    let winner = winner
        .as_ref()
        .expect("a direct 2-cycle must name a winner");
    assert_eq!(
        winner.layer,
        Layer::DeclaredOverride,
        "the override rule, not any other edge, must be the one that won"
    );

    let overridden_key = FindingKey::DeclarationOverridden {
        declared_after: ModId::new("a"),
        declared_before: ModId::new("b"),
        kind: EdgeKind::LoadAfter,
    };
    let Some(Finding::DeclarationOverridden { by, .. }) = findings.get(&overridden_key) else {
        panic!(
            "expected a DeclarationOverridden finding alongside EdgeDropped: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(by.after, ModId::new("b"));
    assert_eq!(by.before, ModId::new("a"));
    assert_eq!(by.layer, Layer::DeclaredOverride);
}

/// Control for the test above: the identical setup, minus the flag.
/// An *ordinary* `UserDecision` pair rule must keep losing to a
/// `Declared` edge exactly as it always has — `RuleOverruled` fires
/// (the rule is the one dropped), and `DeclarationOverridden` never
/// does. Guards against a regression that fires `DeclarationOverridden`
/// unconditionally for any `UserDecision`-vs-`Declared` contradiction.
#[test]
fn an_unflagged_pair_rule_still_loses_to_a_declared_edge() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .declared_edge("a", "b") // Declared: a after b.
        .build();
    let rules = RuleSet::new(vec![pair_rule("b", "a", RuleOrigin::UserDecision)]);
    let findings = extract_for_with_rules(&report, &rules);

    assert!(
        findings.contains_key(&FindingKey::RuleOverruled {
            after: ModId::new("b"),
            before: ModId::new("a"),
            origin: RuleOrigin::UserDecision,
        }),
        "the ordinary rule must be the one overruled: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::DeclarationOverridden { .. })),
        "an unflagged rule must never produce DeclarationOverridden: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// Requirement 5: the override must never beat a `Hard` edge. The
/// override rule is the one dropped (a `RuleOverruled` naming `Hard`
/// as the winner, the same shape any other rule origin already gets
/// against `Hard` — see
/// `a_user_decision_pair_rule_dropped_by_a_hard_edge_is_still_rule_overruled`
/// above), and no `DeclarationOverridden` fires (the dropped edge
/// here is a rule, not a `Declared`-strength engine edge, so
/// `EdgeDropped` never even applies).
#[test]
fn an_override_flagged_pair_rule_never_beats_a_hard_edge() {
    let report = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .hard_edge("a", "b") // Hard: a after b.
        .build();
    let rules = RuleSet::new(vec![override_pair_rule("b", "a")]); // b after a: the opposite.
    let findings = extract_for_with_rules(&report, &rules);

    let Some(Finding::RuleOverruled { winner, .. }) = findings.get(&FindingKey::RuleOverruled {
        after: ModId::new("b"),
        before: ModId::new("a"),
        origin: RuleOrigin::UserDecision,
    }) else {
        panic!(
            "expected the override rule itself to be overruled by Hard: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(
        winner.as_ref().map(|w| w.layer),
        Some(Layer::Hard),
        "Hard must win regardless of the override flag"
    );
    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::DeclarationOverridden { .. })),
        "a Hard-vs-rule contradiction is never a DeclarationOverridden: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

// -- PlacementOverruled / PlacementQuestioned -----------------

fn placement_rule(mod_id: &str, placement: Placement, origin: RuleOrigin) -> crate::domain::Rule {
    crate::domain::Rule::Placement(crate::domain::PlacementRule {
        mod_id: ModId::new(mod_id),
        placement,
        origin,
        comment: None,
    })
}

/// A `Bottom`-placed mod whose tier membership edge is dropped
/// because a `Hard` edge crosses the boundary (`TierReason::PromotedBy`)
/// yields `PlacementOverruled`, with `landed_at` equal to the mod's
/// actual emitted position.
#[test]
fn a_bottom_placement_crossed_by_a_hard_edge_becomes_placement_overruled() {
    let report = ReportBuilder::new()
        .mod_("pinned")
        // `puller` is DLC so its own tier membership binds at
        // `Layer::Hard` — already in the graph by the time `pinned`'s
        // own (RimSortCommunity-origin) Bottom membership is added, so
        // the cycle this Hard edge creates closes at `pinned`'s own
        // layer and its membership (not `puller`'s) is what breaks.
        // An ordinary default-`Body` `puller` would instead have its
        // own (much later, `Awareness`-bound) membership dropped
        // first, since a cycle can only close — and only that layer's
        // edges are ever droppable — once every participating edge
        // has actually been added to the graph.
        .dlc("puller")
        // `puller` ships a load-time reference onto `pinned`, forcing
        // `pinned` to load before `puller` regardless of its own
        // Bottom pin.
        .hard_edge("puller", "pinned")
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &rules,
        false,
    );

    let key = FindingKey::PlacementOverruled {
        mod_id: ModId::new("pinned"),
        placement: Placement::Bottom,
        origin: RuleOrigin::RimSortCommunity,
    };
    let Some(Finding::PlacementOverruled { by, landed_at, .. }) = findings.get(&key) else {
        panic!(
            "expected PlacementOverruled for {key:?}, found: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(by.layer, Layer::Hard);
    assert_eq!(
        Some(*landed_at),
        sort_outcome.order.position(&ModId::new("pinned")),
        "landed_at must equal the mod's actual emitted position"
    );
}

/// `PlacementQuestioned` and its intended round trip: a `Bottom` mod `M`
/// with an advisory (`Awareness`) edge naming a non-`Bottom` mod `X`
/// as `X after M` can never be satisfied under the placement —
/// `PlacementQuestioned`. Enforcing that same relation as a `Reorder`
/// user decision on the next build then reports `PlacementOverruled`
/// with a `UserDecision` winner.
#[test]
fn a_bottom_placement_with_an_unsatisfiable_advisory_edge_round_trips_through_reorder() {
    let report = ReportBuilder::new()
        .mod_("m")
        // `x` is DLC (see the previous test's own comment on why): the
        // Reorder decision below must actually promote `m` out of
        // Bottom, which needs `x`'s own tier membership already bound
        // to the graph before `m`'s is — never true for a default
        // `Body` `x`, whose membership binds at the very last
        // (`Awareness`) layer.
        .dlc("x")
        .awareness_edge("x", "m") // x after m: impossible if m is Bottom and x isn't.
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "m",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &rules,
        false,
    );

    let questioned_key = FindingKey::PlacementQuestioned {
        mod_id: ModId::new("m"),
        placement: Placement::Bottom,
        relation: EdgeKind::MayRequire,
    };
    assert!(
        findings.contains_key(&questioned_key),
        "expected PlacementQuestioned for {questioned_key:?}, found: {:?}",
        findings.keys().collect::<Vec<_>>()
    );

    // Round trip: enforce "x after m" as a UserDecision Reorder.
    let mut overrides = crate::domain::SorterOverrides::default();
    overrides.reorders.push((ModId::new("x"), ModId::new("m")));
    let sort_outcome_2 = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings_2 = extract(
        &report,
        &sort_outcome_2,
        &Tagging::default(),
        &sort_outcome_2.order,
        &rules,
        false,
    );

    let overruled_key = FindingKey::PlacementOverruled {
        mod_id: ModId::new("m"),
        placement: Placement::Bottom,
        origin: RuleOrigin::RimSortCommunity,
    };
    let Some(Finding::PlacementOverruled { by, .. }) = findings_2.get(&overruled_key) else {
        panic!(
            "expected PlacementOverruled for {overruled_key:?} after the Reorder, found: {:?}",
            findings_2.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(
        by.layer,
        Layer::UserDecision,
        "the round trip must name the user's own Reorder as the winner"
    );
    // The original
    // `PlacementQuestioned` must actually be gone once the round trip
    // resolves it — not merely joined by the new `PlacementOverruled`.
    assert!(
        !findings_2.contains_key(&questioned_key),
        "PlacementQuestioned for {questioned_key:?} must disappear once its own Reorder \
             resolves the relation: {:?}",
        findings_2.keys().collect::<Vec<_>>()
    );
}

/// The round trip's second case: the same shape, but the advisory
/// relation's other
/// endpoint (`x`) is a default `Body` mod instead of DLC. The
/// cycle-breaker then promotes `x` itself out of `Body` (its own
/// membership binds at the weakest, `Awareness`, layer — the very
/// last one added — while `m`'s Bottom membership and the Reorder
/// edge are both already locked in from earlier layers), not `m`'s
/// own placement. Since `PlacementOverruled` only ever fires for a
/// nominally `Top`/`Bottom` mod (`x`'s nominal tier stays `Body`), the
/// round trip ends at the now-satisfied relation with no finding at
/// all on either mod — not at a `PlacementOverruled`, unlike the
/// DLC-partner case above. This also exercises the "skip an advisory
/// edge already satisfied" rule: without it, `m`'s still-
/// advisory copy of the raw analyzer edge (now satisfied by the real
/// Reorder edge) would keep reporting `PlacementQuestioned` forever.
#[test]
fn a_bottom_placement_with_a_body_partners_advisory_edge_round_trips_to_a_satisfied_relation() {
    let report = ReportBuilder::new()
        .mod_("m")
        .mod_("x")
        .awareness_edge("x", "m") // x after m: impossible if m is Bottom and x isn't.
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "m",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &rules,
        false,
    );

    let questioned_key = FindingKey::PlacementQuestioned {
        mod_id: ModId::new("m"),
        placement: Placement::Bottom,
        relation: EdgeKind::MayRequire,
    };
    assert!(
        findings.contains_key(&questioned_key),
        "expected PlacementQuestioned for {questioned_key:?}, found: {:?}",
        findings.keys().collect::<Vec<_>>()
    );

    let mut overrides = crate::domain::SorterOverrides::default();
    overrides.reorders.push((ModId::new("x"), ModId::new("m")));
    let sort_outcome_2 = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings_2 = extract(
        &report,
        &sort_outcome_2,
        &Tagging::default(),
        &sort_outcome_2.order,
        &rules,
        false,
    );

    assert!(
        !findings_2.contains_key(&questioned_key),
        "the relation is now satisfied by the Reorder edge, so PlacementQuestioned must be \
             gone: {:?}",
        findings_2.keys().collect::<Vec<_>>()
    );
    assert!(
        !findings_2
            .keys()
            .any(|key| matches!(key, FindingKey::PlacementOverruled { .. })),
        "a Body partner's own promotion must never surface as PlacementOverruled (that kind \
             only ever fires for a nominally Top/Bottom mod): {:?}",
        findings_2.keys().collect::<Vec<_>>()
    );
}

/// Once a `Bottom` placement is itself overruled
/// (`TierReason::PromotedBy`, i.e. `PlacementOverruled`), the mod isn't
/// actually sitting in its nominal tier any more — a second,
/// unrelated advisory edge that would be "impossible under Bottom"
/// must not also report `PlacementQuestioned`, redundant with the
/// `PlacementOverruled` already reported for the same mod.
#[test]
fn a_placement_already_overruled_never_also_reports_placement_questioned() {
    let report = ReportBuilder::new()
        .mod_("pinned")
        .mod_("curious")
        // `puller` is DLC so its own tier membership binds at
        // `Layer::Hard`, already in the graph by the time `pinned`'s
        // own (RimSortCommunity-origin) Bottom membership is added —
        // see the sibling `PlacementOverruled` test's own comment.
        .dlc("puller")
        .hard_edge("puller", "pinned")
        // Unrelated to the Hard edge above: this can never be
        // satisfied while `pinned` is genuinely Bottom, but `pinned`
        // isn't any more once the Hard edge promotes it out.
        .awareness_edge("curious", "pinned")
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let current = insertion_order(&report);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &current,
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &sort_outcome.order,
        &rules,
        false,
    );

    assert!(
        findings.contains_key(&FindingKey::PlacementOverruled {
            mod_id: ModId::new("pinned"),
            placement: Placement::Bottom,
            origin: RuleOrigin::RimSortCommunity,
        }),
        "expected PlacementOverruled once pinned's own membership is dropped: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
    assert!(!findings.keys().any(|key| matches!(key, FindingKey::PlacementQuestioned { mod_id, .. } if *mod_id == ModId::new("pinned"))
            ),
            "an already-overruled placement must not also report PlacementQuestioned: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
}

// -- PlacementOrderingOverridden --------------------------------

/// A `Declared`
/// edge forcing `dependent` to load *after* a holding `Bottom` pin
/// fires `PlacementOrderingOverridden` — the simplest, canonical
/// reproduction (the pin's own placement holds; `PlacementOverruled`
/// must not also fire for it).
#[test]
fn a_declared_edge_forcing_a_mod_after_a_bottom_pin_fires_placement_ordering_overridden() {
    let report = ReportBuilder::new()
        .mod_("pinned")
        .mod_("dependent")
        .declares_load_after("dependent", "pinned")
        .declared_edge("dependent", "pinned")
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::PlacementOrderingOverridden {
        mod_id: ModId::new("dependent"),
        pinned: ModId::new("pinned"),
        placement: Placement::Bottom,
    };
    let Some(Finding::PlacementOrderingOverridden { by, .. }) = findings.get(&key) else {
        panic!(
            "expected PlacementOrderingOverridden for {key:?}, found: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(by.layer, Layer::Declared);
    assert!(
        !findings.contains_key(&FindingKey::PlacementOverruled {
            mod_id: ModId::new("pinned"),
            placement: Placement::Bottom,
            origin: RuleOrigin::RimSortCommunity,
        }),
        "the pin's own placement holds here — PlacementOverruled is a different finding for \
             a different case: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// The `Top` mirror: a `Declared` edge forcing `dependent` to load
/// *before* a holding `Top` pin.
#[test]
fn a_declared_edge_forcing_a_mod_before_a_top_pin_fires_placement_ordering_overridden() {
    let report = ReportBuilder::new()
        .mod_("pinned")
        .mod_("dependent")
        .declares_load_after("pinned", "dependent")
        .declared_edge("pinned", "dependent")
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Top,
        RuleOrigin::RimSortCommunity,
    )]);
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::PlacementOrderingOverridden {
        mod_id: ModId::new("dependent"),
        pinned: ModId::new("pinned"),
        placement: Placement::Top,
    };
    assert!(
        findings.contains_key(&key),
        "expected PlacementOrderingOverridden for {key:?}, found: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// A valid, uncontested `Bottom` pin — nothing requires anything else
/// relative to it — must never produce `PlacementOrderingOverridden`.
#[test]
fn an_uncontested_bottom_pin_produces_no_placement_ordering_overridden_finding() {
    let report = ReportBuilder::new().mod_("pinned").mod_("other").build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let findings = extract_for_with_rules(&report, &rules);

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::PlacementOrderingOverridden { .. })),
        "an uncontested pin must never report PlacementOrderingOverridden: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// Two `Bottom` pins directly ordered relative to each other by a
/// real edge is not this finding's case — both sides are explicit
/// pins, nothing was "merely promoted in" past either one.
#[test]
fn two_bottom_pins_ordered_relative_to_each_other_produce_no_placement_ordering_overridden_finding()
{
    let report = ReportBuilder::new()
        .mod_("first")
        .mod_("second")
        .declares_load_after("second", "first")
        .declared_edge("second", "first")
        .build();
    let rules = RuleSet::new(vec![
        placement_rule("first", Placement::Bottom, RuleOrigin::RimSortCommunity),
        placement_rule("second", Placement::Bottom, RuleOrigin::RimSortCommunity),
    ]);
    let findings = extract_for_with_rules(&report, &rules);

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::PlacementOrderingOverridden { .. })),
        "two pins of the same placement ordered relative to each other must not report \
             PlacementOrderingOverridden: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

// -- The rule-level companion: PlacementPromotesDependents ---------

/// A holding `Bottom` pin with two mods declared to load after it
/// fires `PlacementPromotesDependents` naming both.
#[test]
fn a_bottom_pin_with_two_dependents_fires_placement_promotes_dependents() {
    let report = ReportBuilder::new()
        .mod_("pinned")
        .mod_("dependent_a")
        .mod_("dependent_b")
        .declares_load_after("dependent_a", "pinned")
        .declared_edge("dependent_a", "pinned")
        .declares_load_after("dependent_b", "pinned")
        .declared_edge("dependent_b", "pinned")
        .build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let findings = extract_for_with_rules(&report, &rules);

    let key = FindingKey::PlacementPromotesDependents {
        mod_id: ModId::new("pinned"),
        placement: Placement::Bottom,
    };
    let Some(Finding::PlacementPromotesDependents { promoted, .. }) = findings.get(&key) else {
        panic!(
            "expected PlacementPromotesDependents for {key:?}, found: {:?}",
            findings.keys().collect::<Vec<_>>()
        );
    };
    assert_eq!(
        promoted,
        &vec![ModId::new("dependent_a"), ModId::new("dependent_b")]
    );
}

/// A mod
/// genuinely promoted past *two* `Bottom` pins' own boundaries (it
/// declares `loadAfter` both) is still only *attributed* to one of
/// them — `find_promotion_cause`'s BFS stops at the first placed node
/// it reaches, so exactly one of the two pins' own
/// `PlacementPromotesDependents` finding names `dependent`, never
/// both. This documents the intended behavior (the rationale says
/// "attributed to"; the BFS is not a multi-pin reach count) rather than
/// flagging a bug to fix.
#[test]
fn a_mod_promoted_past_two_pins_is_attributed_to_only_one() {
    let report = ReportBuilder::new()
        .mod_("pin_a")
        .mod_("pin_b")
        .mod_("dependent")
        .declares_load_after("dependent", "pin_a")
        .declared_edge("dependent", "pin_a")
        .declares_load_after("dependent", "pin_b")
        .declared_edge("dependent", "pin_b")
        .build();
    let rules = RuleSet::new(vec![
        placement_rule("pin_a", Placement::Bottom, RuleOrigin::RimSortCommunity),
        placement_rule("pin_b", Placement::Bottom, RuleOrigin::RimSortCommunity),
    ]);
    let findings = extract_for_with_rules(&report, &rules);

    let names_dependent = |mod_id: &str| {
        let key = FindingKey::PlacementPromotesDependents {
            mod_id: ModId::new(mod_id),
            placement: Placement::Bottom,
        };
        matches!(findings.get(&key),
            Some(Finding::PlacementPromotesDependents { promoted, .. })
                if promoted.contains(&ModId::new("dependent"))
        )
    };
    let attributions = [names_dependent("pin_a"), names_dependent("pin_b")]
        .into_iter()
        .filter(|named| *named)
        .count();
    assert_eq!(
        attributions,
        1,
        "dependent must be attributed to exactly one of the two pins it was promoted past, \
             found: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

/// An uncontested pin (no dependents at all) must never report
/// `PlacementPromotesDependents` — this is the common case, and the
/// whole point of attributing the count rather than using a static
/// threshold.
#[test]
fn an_uncontested_pin_produces_no_placement_promotes_dependents_finding() {
    let report = ReportBuilder::new().mod_("pinned").mod_("other").build();
    let rules = RuleSet::new(vec![placement_rule(
        "pinned",
        Placement::Bottom,
        RuleOrigin::RimSortCommunity,
    )]);
    let findings = extract_for_with_rules(&report, &rules);

    assert!(
        !findings
            .keys()
            .any(|key| matches!(key, FindingKey::PlacementPromotesDependents { .. })),
        "an uncontested pin must never report PlacementPromotesDependents: {:?}",
        findings.keys().collect::<Vec<_>>()
    );
}

fn texture_report(owners: &[&str]) -> Report {
    use rim_analyzer::domain::TextureOverride;

    let mut builder = ReportBuilder::new();
    for owner in owners {
        builder = builder.mod_(owner);
    }
    let mut report = builder.build();
    report
        .conflicts
        .push(Conflict::TextureOverride(TextureOverride {
            texture_path: "Things/Wall".to_string(),
            owners: owners.iter().map(|owner| ModId::new(*owner)).collect(),
            same_author: false,
        }));
    report
}

/// The `winner` of the one texture-override finding, extracted for `order`.
fn texture_winner_under(report: &Report, order: &LoadOrder) -> ModId {
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    let findings = extract(
        report,
        &sort_outcome,
        &Tagging::default(),
        order,
        &crate::domain::RuleSet::default(),
        false,
    );
    findings
        .values()
        .find_map(|finding| match finding {
            Finding::TextureOverride { winner, .. } => Some(winner.clone()),
            _ => None,
        })
        .expect("the report carries one texture override")
}

#[test]
fn a_texture_override_winner_follows_the_order_the_ledger_is_built_for() {
    let report = texture_report(&["a", "b"]);
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let suggested = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

    assert_eq!(texture_winner_under(&report, &current), ModId::new("b"));
    assert_eq!(texture_winner_under(&report, &suggested), ModId::new("a"));
}

#[test]
fn a_texture_override_keeps_its_owners_in_scan_order_whatever_the_selected_order() {
    let report = texture_report(&["a", "b"]);
    let suggested = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report: &report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(&report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });

    let findings = extract(
        &report,
        &sort_outcome,
        &Tagging::default(),
        &suggested,
        &crate::domain::RuleSet::default(),
        false,
    );

    assert!(findings.values().any(|finding| matches!(
        finding,
        Finding::TextureOverride { owners, .. }
            if *owners == vec![ModId::new("a"), ModId::new("b")]
    )));
}

#[test]
fn a_texture_override_owner_missing_from_the_order_never_beats_a_listed_one() {
    let report = texture_report(&["a", "b"]);
    let only_a = LoadOrder::new(vec![ModId::new("a")]);

    assert_eq!(texture_winner_under(&report, &only_a), ModId::new("a"));
}

/// Every finding extracted for `order`.
fn findings_under(report: &Report, order: &LoadOrder) -> BTreeMap<FindingKey, Finding> {
    let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
        report,
        rules: &crate::domain::RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &crate::domain::SorterOverrides::default(),
        current: &insertion_order(report),
        enforce: crate::sort::EnforcedLayers::default(),
        tie_break: crate::sort::TieBreak::PreserveCurrent,
    });
    extract(
        report,
        &sort_outcome,
        &Tagging::default(),
        order,
        &crate::domain::RuleSet::default(),
        false,
    )
}

#[test]
fn a_def_override_winner_follows_the_order_the_ledger_is_built_for() {
    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.conflicts.push(Conflict::DefOverride(DefOverride {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
        owners: vec![ModId::new("a"), ModId::new("b")],
        overrides_vanilla: false,
        same_author: false,
    }));
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let suggested = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

    let winner_under = |order: &LoadOrder| {
        findings_under(&report, order)
            .into_values()
            .find_map(|finding| match finding {
                Finding::DefOverride { winner, .. } => Some(winner),
                _ => None,
            })
            .expect("the report carries one def override")
    };

    assert_eq!(winner_under(&current), ModId::new("b"));
    assert_eq!(winner_under(&suggested), ModId::new("a"));
}

#[test]
fn a_patch_collision_winner_follows_the_order_the_ledger_is_built_for() {
    let mut report = ReportBuilder::new().mod_("a").mod_("b").build();
    report.conflicts.push(Conflict::PatchCollision(
        rim_analyzer::domain::PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: rim_analyzer::domain::Selector::DefName,
            sub_path: Some("label".to_string()),
            mods: ["a", "b"]
                .into_iter()
                .map(|id| rim_analyzer::domain::PatchCollisionEntry {
                    mod_id: ModId::new(id),
                    op_class: "PatchOperationReplace".to_string(),
                })
                .collect(),
            severity: rim_analyzer::domain::PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        },
    ));
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let suggested = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

    let winner_under = |order: &LoadOrder| {
        findings_under(&report, order)
            .into_values()
            .find_map(|finding| match finding {
                Finding::PatchCollision { winner, mods, .. } => {
                    assert_eq!(mods, vec![ModId::new("a"), ModId::new("b")]);
                    Some(winner)
                }
                _ => None,
            })
            .expect("the report carries one patch collision")
    };

    assert_eq!(winner_under(&current), ModId::new("b"));
    assert_eq!(winner_under(&suggested), ModId::new("a"));
}
