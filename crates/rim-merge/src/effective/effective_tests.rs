//! Tests for the effective-def computation.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::fold::{Shadow, merge_shadow};
use super::*;
use crate::patch_behaviours::PatchOperationBehaviours;
use crate::tree::{Content, FieldNode};
use crate::xml;

fn active(ids: &[&str]) -> BTreeSet<ModId> {
    ids.iter().map(|id| ModId::new(*id)).collect()
}

fn context<'a>(
    active_mods: &'a BTreeSet<ModId>,
    mod_names_by_display: &'a BTreeMap<String, ModId>,
    def_type: &'a str,
    def_name: &'a str,
) -> ReplayContext<'a> {
    ReplayContext {
        active_mods,
        mod_names_by_display,
        def_type,
        def_name,
        selector: rim_analyzer::domain::Selector::DefName,
        def_exists: patch_eval::def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    }
}

fn owner_path(def: &EffectiveDef, path: &str) -> ModId {
    match def.provenance.get(&path.parse().unwrap()) {
        Some(Provenance::Owner(mod_id)) => mod_id.clone(),
        other => panic!("expected Owner at {path}, got {other:?}"),
    }
}

/// Every fixture below is hermetic — no session, no reader, no profile
/// — which is exactly why the permutation lives in this crate, and
/// every one was probe-verified against a revert (`counterfactual`'s
/// body stubbed to `CounterfactualOutcome::default()`).
///
/// `mod.a` is the irrelevant third party in each: it patches
/// `<label>`, which nothing else touches, so it succeeds under every
/// permutation and can never be the decisive block.
const IRRELEVANT_OP: &str = r#"<Operation Class="PatchOperationReplace">
        <xpath>Defs/ThingDef[defName="W"]/label</xpath>
        <value><label>y</label></value>
    </Operation>"#;

#[test]
fn contribution_blocks_groups_each_mod_s_contiguous_run() {
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: "<Operation/>",
        },
        PatchContribution {
            mod_id: &a,
            operation_xml: "<Operation/>",
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: "<Operation/>",
        },
    ];

    assert_eq!(
        contribution_blocks(&contributions),
        vec![
            ContributionBlock {
                mod_id: a,
                start: 0,
                len: 2,
            },
            ContributionBlock {
                mod_id: b,
                start: 2,
                len: 1,
            },
        ]
    );
}

#[test]
fn counterfactual_names_the_injector_when_the_subject_must_load_after_it() {
    // B's `Add` targets `container/injected`, a node C's own `Add`
    // creates — so under A, B, C the subject cannot find its target,
    // and the minimal move that fixes it is B after C.
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label><container></container></ThingDef>",
    )
    .unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container/injected</xpath>
                    <value><deep>2</deep></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container</xpath>
                    <value><injected><inner>1</inner></injected></value>
                </Operation>"#,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());
    assert!(
        !baseline.top_level_outcomes[1].succeeded,
        "the fixture's own premise: B fails under the baseline order"
    );

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: true,
            // B's own `<deep>2</deep>` shows up under `injected` only in
            // the fixed order (baseline: B fails outright, so it never
            // applies at all) — a real content difference, not cosmetic.
            final_def_unchanged: false,
        })
    );
    assert_eq!(outcome.attempts, 2, "K - 1 alternatives for K = 3");
    assert_eq!(outcome.rejected_for_regression, 0);
    assert_eq!(outcome.rejected_for_truncation, 0);
    assert_eq!(
        outcome.ties, 0,
        "the equal-distance move the other way failed"
    );
}

#[test]
fn counterfactual_names_the_destroyer_when_the_subject_must_load_before_it() {
    // The mirror: B's `Remove` targets `container/doomed`, which C's
    // own `Replace` of the whole `container` destroys — so under
    // A, C, B the subject has nothing left to remove, and the
    // minimal move that fixes it is B before C.
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label>\
             <container><doomed>1</doomed></container></ThingDef>",
    )
    .unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container</xpath>
                    <value><container><other>2</other></container></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/doomed</xpath>
                </Operation>"#,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());
    assert!(!baseline.top_level_outcomes[2].succeeded);

    let outcome = counterfactual(&input, &baseline, 2);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: false,
            // C's own `Replace` overwrites the *whole* `container` node
            // regardless of whether B's `Remove` ran first — the final
            // tree is `<container><other>2</other></container>` either
            // way (a `Replace` discards the old subtree, including
            // anything an earlier op did to it), so this fix only
            // changes which op's own failure would be logged.
            final_def_unchanged: true,
        }),
        "the decisive block is the one at the minimal successful \
             position, not the furthest one B could have been moved to"
    );
    assert_eq!(outcome.attempts, 2);
    assert_eq!(outcome.rejected_for_regression, 0);
}

#[test]
fn counterfactual_refuses_a_move_that_fixes_the_subject_and_breaks_another_operation() {
    // Strictly-better-only. Four blocks: A (irrelevant),
    // B (the subject: `Remove container/injected/inner`),
    // C (`Add injected/inner` — what B needs), D (two ops: a
    // `Replace` of `injected/inner`, then a `Remove` of `injected`).
    //
    // Only one of B's three alternative positions makes B succeed —
    // A, C, B, D — and that same move makes D's own `Replace`, which
    // succeeded in the baseline, fail. Moving B to the very end
    // instead is no good either: D's `Remove` has taken `injected`
    // away by then.
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label><container></container></ThingDef>",
    )
    .unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let d = ModId::new("mod.d");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c", "mod.d"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/injected/inner</xpath>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container</xpath>
                    <value><injected><inner>1</inner></injected></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &d,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container/injected/inner</xpath>
                    <value><inner>9</inner></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &d,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/injected</xpath>
                </Operation>"#,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());
    assert!(!baseline.top_level_outcomes[1].succeeded);
    assert!(
        baseline.top_level_outcomes[3].succeeded,
        "the regression this fixture is about is a baseline success"
    );

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(outcome.fix, None, "no strictly-better move exists");
    assert_eq!(outcome.attempts, 3, "K - 1 alternatives for K = 4");
    assert_eq!(outcome.rejected_for_regression, 1);
    assert_eq!(outcome.rejected_for_truncation, 0);
}

#[test]
fn counterfactual_never_reads_a_move_that_truncates_the_fold_as_a_success() {
    // The second refusal: `mod.u`'s operation is outside the
    // replay grammar, so a move that puts the subject *after* it
    // never gets replayed at all. The outcomes list simply stops —
    // which must read as "not observed", never as "succeeded".
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label><container></container></ThingDef>",
    )
    .unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let u = ModId::new("mod.u");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.u"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        // The subject: targets a node no order ever creates, so it
        // fails everywhere — the point is what the *other* refusal
        // gets counted as.
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container/nonexistent</xpath>
                    <value><nonexistent>1</nonexistent></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &u,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container[not(petness)="petness"]</xpath>
                    <value><container>z</container></value>
                </Operation>"#,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());
    assert!(matches!(
        baseline.completeness,
        Completeness::Partial {
            stopped_at: Stopper::Replay { op_index: 2, .. }
        }
    ));

    let outcome = counterfactual(&input, &baseline, 0);

    assert_eq!(outcome.fix, None);
    assert_eq!(outcome.attempts, 2);
    assert_eq!(
        outcome.rejected_for_truncation, 1,
        "moving the subject behind the unsupported operation is a refusal, not a fix"
    );
    assert_eq!(outcome.rejected_for_regression, 0);
}

/// Shared fixture text for the permutation-mapping and tie-break
/// tests below. Every one of these is aimed at one specific line of
/// [`counterfactual`], so they are kept apart from the behaviour
/// fixtures above.
const ADD_DEEP: &str = r#"<Operation Class="PatchOperationAdd">
        <xpath>Defs/ThingDef[defName="W"]/container/injected</xpath>
        <value><deep>2</deep></value>
    </Operation>"#;
const ADD_INJECTED: &str = r#"<Operation Class="PatchOperationAdd">
        <xpath>Defs/ThingDef[defName="W"]/container</xpath>
        <value><injected><inner>1</inner></injected></value>
    </Operation>"#;
const ALWAYS_FAILS: &str = r#"<Operation Class="PatchOperationReplace">
        <xpath>Defs/ThingDef[defName="W"]/container/missing</xpath>
        <value><missing>1</missing></value>
    </Operation>"#;
const ADD_EXTRA: &str = r#"<Operation Class="PatchOperationAdd">
        <xpath>Defs/ThingDef[defName="W"]/container</xpath>
        <value><extra>1</extra></value>
    </Operation>"#;

fn empty_container_def() -> FieldTree {
    xml::parse("<ThingDef><defName>W</defName><label>x</label><container></container></ThingDef>")
        .expect("fixture parses")
}

/// Builds the [`EffectiveInput`] the two mapping tests share, plus
/// the baseline [`compute`] produced for it.
fn baseline_for<'a>(
    winner: &'a ModId,
    raw: FieldTree,
    contributions: &'a [PatchContribution<'a>],
    active_mods: &'a BTreeSet<ModId>,
    names: &'a BTreeMap<String, ModId>,
    templates: &'a TemplateSet,
    template_owners: &'a BTreeMap<(String, String), ModId>,
) -> (EffectiveInput<'a>, EffectiveDef) {
    let input = EffectiveInput {
        winner,
        raw,
        contributions,
        context: context(active_mods, names, "ThingDef", "W"),
        templates,
        template_owners,
    };
    let baseline = compute(input.clone());
    (input, baseline)
}

#[test]
fn counterfactual_reads_the_subject_at_its_permuted_position_not_its_original_index() {
    // Guards the `position_of[subject]` mapping specifically. Blocks
    // are deliberately *unequal* in length — A(1), B(2, the subject's),
    // C(2) — so the subject's original index (1) and its position in
    // the alternative (3) differ, and the operation that happens to
    // sit at the original index in the alternative (C's first, which
    // always fails) gives the opposite verdict. With a bare
    // `top_level_outcomes[subject]` this test reports `fix: None`.
    let raw = empty_container_def();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        // The subject: needs `container/injected`, which only C's
        // second operation creates.
        PatchContribution {
            mod_id: &b,
            operation_xml: ADD_DEEP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: ADD_EXTRA,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: ALWAYS_FAILS,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: ADD_INJECTED,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    assert!(!baseline.top_level_outcomes[1].succeeded);

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: true,
            // Same shape as the injector test above: B's own addition
            // under `injected` only ever applies in the fixed order.
            final_def_unchanged: false,
        }),
        "the subject succeeds at its permuted position (3); the operation at its \
             original index (1) in the same alternative is C's own always-failing one"
    );
    assert_eq!(outcome.attempts, 2);
    assert_eq!(outcome.rejected_for_regression, 0);
}

#[test]
fn counterfactual_reads_each_regression_candidate_at_its_permuted_position() {
    // The mirror of the test above, aimed at `position_of[original]`
    // in the regression scan. Blocks A(1), B(2, the subject's), C(1).
    // C succeeds in the baseline *and* in the alternative — but the
    // operation sitting at C's original index (3) in the alternative
    // is the subject block's second operation, which always fails.
    // With a bare `top_level_outcomes[original]` this test reports
    // `fix: None` with `rejected_for_regression: 1`.
    let raw = empty_container_def();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: ADD_DEEP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: ALWAYS_FAILS,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: ADD_INJECTED,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    assert!(!baseline.top_level_outcomes[1].succeeded, "the subject");
    assert!(
        !baseline.top_level_outcomes[2].succeeded,
        "the subject block's second operation fails in the baseline too, so its \
             own failure in the alternative can never be a regression"
    );
    assert!(
        baseline.top_level_outcomes[3].succeeded,
        "C, the regression candidate"
    );

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: true,
            final_def_unchanged: false,
        })
    );
    assert_eq!(
        outcome.rejected_for_regression, 0,
        "C still succeeds — at its permuted position (1), not at its original index (3), \
             where the subject block's always-failing second operation now sits"
    );
}

#[test]
fn counterfactual_breaks_an_equal_distance_tie_later_move_first_and_counts_it() {
    // The tie-break, which nothing else exercises: `mod.b`'s
    // `Replace` of `container/x` works both when it runs *before*
    // `mod.a`'s `Remove` of that node and when it runs *after*
    // `mod.c` puts the node back — two successful moves, distance 1
    // each, in opposite directions. Flipping the tie-break to
    // earlier-move-first names `mod.a` here instead.
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label>\
             <container><x>1</x></container></ThingDef>",
    )
    .expect("fixture parses");
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/x</xpath>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container/x</xpath>
                    <value><x>9</x></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container</xpath>
                    <value><x>1</x></value>
                </Operation>"#,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    assert!(!baseline.top_level_outcomes[1].succeeded);

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: true,
            // Winning order: A removes the original `x`, C's `Add`
            // appends a fresh `<x>1</x>`, B's `Replace` then overwrites
            // *that* one to `<x>9</x>` — final content `9`. Baseline
            // order: A removes `x`, B's `Replace` fails outright (no
            // node to replace), C's `Add` appends `<x>1</x>` untouched —
            // final content `1`. The two finals genuinely differ.
            final_def_unchanged: false,
        }),
        "both directions work at distance 1; the later move wins"
    );
    assert_eq!(
        outcome.ties, 1,
        "the equal-distance move in the other direction, counted rather than discarded"
    );
    assert_eq!(outcome.attempts, 2);
    assert_eq!(outcome.rejected_for_regression, 0);
}

// -- `final_def_unchanged`: cosmetic versus content fixes ---------------

#[test]
fn counterfactual_fix_reports_final_def_unchanged_for_replace_then_remove() {
    // B's `Remove` and C's `Replace` collide on the identical leaf,
    // `container/x`. Whichever one runs first, the other's own
    // operation still succeeds against what's left — but the final
    // *content* of `x` ends up identical either way, matching the
    // "only the op that logs `failed` differs" shape for
    // `PatchRemovedNodeCosmetic`, here proven by replay instead of an
    // edge (a real-install shape: two compat patches independently
    // removing, then one of them replacing, the same disputed node).
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label>\
             <container><x>1</x></container></ThingDef>",
    )
    .expect("fixture parses");
    let winner = ModId::new("mod.owner");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/x</xpath>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/container/x</xpath>
                    <value><x><newval>2</newval></x></value>
                </Operation>"#,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    // Baseline (B, C): B removes `x` first, so C's own `Replace` has
    // nothing left to replace.
    assert!(baseline.top_level_outcomes[0].succeeded, "B removes x");
    assert!(
        !baseline.top_level_outcomes[1].succeeded,
        "C's Replace finds nothing: the fixture's own premise"
    );

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: b,
            subject_loads_after: false,
            // Winning order (C, B): C replaces `x` (still tagged `x`,
            // so B's own head still resolves it), then B removes it —
            // `container` ends up with no `x` at all. Baseline order
            // (B, C): B removes `x` outright, C's Replace no-ops —
            // `container` ends up with no `x` either. Identical either
            // way.
            final_def_unchanged: true,
        }),
        "C must load before B to find the node still there"
    );
}

#[test]
fn counterfactual_fix_reports_changed_for_add_then_remove_of_a_sibling_path() {
    // C's `Add` creates `container` with two children, `keep` and
    // `doomed`; B's `Remove` targets `doomed`, a *sibling* of `keep`
    // under that same newly-created parent. B needs C to run first just
    // to have something to remove from — but unlike the cosmetic case
    // above, the two final trees genuinely differ: one keeps `doomed`,
    // the other doesn't.
    let raw = xml::parse("<ThingDef><defName>W</defName><label>x</label></ThingDef>")
        .expect("fixture parses");
    let winner = ModId::new("mod.owner");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="W"]/container/doomed</xpath>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]</xpath>
                    <value><container><keep>1</keep><doomed>2</doomed></container></value>
                </Operation>"#,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    assert!(
        !baseline.top_level_outcomes[0].succeeded,
        "B's Remove finds no `container` at all yet: the fixture's own premise"
    );

    let outcome = counterfactual(&input, &baseline, 0);

    assert_eq!(
        outcome.fix,
        Some(CounterfactualFix {
            other: c,
            subject_loads_after: true,
            // Winning order (C, B): C creates `container` with both
            // children, B then removes `doomed` — `container` ends up
            // with `keep` alone. Baseline order (B, C): B's Remove fails
            // outright (nothing to remove from), so C's own creation is
            // never touched — `container` ends up with *both* children.
            // The two finals genuinely differ.
            final_def_unchanged: false,
        }),
        "B must load after C to have anything to remove"
    );
}

#[test]
fn counterfactual_refuses_an_input_whose_blocks_are_not_one_per_mod() {
    // The contiguity premise the permutation rests on, checked for *every* mod
    // rather than only the subject's — `top_level_operations` never
    // produces such an input, so this is defense in depth against the
    // day some other caller hands one in.
    let raw = empty_container_def();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    // `mod.a` owns two non-adjacent runs: moving "a's block" is not
    // a well-defined operation, and neither is naming it as the
    // decisive one.
    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: ADD_DEEP,
        },
        PatchContribution {
            mod_id: &a,
            operation_xml: ADD_INJECTED,
        },
    ];
    let (input, baseline) = baseline_for(
        &winner,
        raw,
        &contributions,
        &active_mods,
        &names,
        &templates,
        &template_owners,
    );
    assert!(!baseline.top_level_outcomes[1].succeeded);

    let outcome = counterfactual(&input, &baseline, 1);

    assert_eq!(outcome.fix, None);
    assert_eq!(outcome.attempts, 0, "nothing was replayed");
    assert!(
        outcome.refused_non_contiguous,
        "the refusal is reported, not silently indistinguishable from K = 1"
    );
}

#[test]
fn counterfactual_is_deterministic_across_runs() {
    let raw = xml::parse(
        "<ThingDef><defName>W</defName><label>x</label><container></container></ThingDef>",
    )
    .unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let b = ModId::new("mod.b");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &b,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container/injected</xpath>
                    <value><deep>2</deep></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/container</xpath>
                    <value><injected><inner>1</inner></injected></value>
                </Operation>"#,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());

    let first = counterfactual(&input, &baseline, 1);
    let second = counterfactual(&input, &baseline, 1);

    assert_eq!(first, second);
    // Not a vacuous equality: this input has a real fix, so the two
    // runs are agreeing on a non-default outcome (a stub returning
    // `CounterfactualOutcome::default()` twice would otherwise pass
    // this test).
    assert!(first.fix.is_some());
    assert_eq!(first.attempts, 2);
}

#[test]
fn counterfactual_refuses_a_subject_that_succeeded_in_the_baseline() {
    let raw = xml::parse("<ThingDef><defName>W</defName><label>x</label></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let a = ModId::new("mod.a");
    let c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &a,
            operation_xml: IRRELEVANT_OP,
        },
        PatchContribution {
            mod_id: &c,
            operation_xml: IRRELEVANT_OP,
        },
    ];
    let input = EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    };
    let baseline = compute(input.clone());
    assert!(baseline.top_level_outcomes.iter().all(|o| o.succeeded));

    assert_eq!(
        counterfactual(&input, &baseline, 0),
        CounterfactualOutcome::default(),
        "nothing to experiment on: the subject never failed"
    );
}

#[test]
fn a_lone_owner_with_no_patches_and_no_parent_attributes_every_field_to_the_owner() {
    let raw = xml::parse("<ThingDef><label>x</label><value>1</value></ThingDef>").unwrap();
    let winner = ModId::new("mod.a");
    let active_mods = active(&["mod.a"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert!(effective.caveats.is_empty());
    assert_eq!(effective.provenance.len(), 2);
    assert_eq!(owner_path(&effective, "label"), winner);
    assert_eq!(owner_path(&effective, "value"), winner);
}

#[test]
fn two_patchers_on_disjoint_fields_are_each_attributed_to_their_own_patcher_regardless_of_order() {
    let raw = xml::parse("<ThingDef><a>0</a><b>0</b></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let replace_a = PatchContribution {
        mod_id: &mod_a,
        operation_xml: r#"<Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="W"]/a</xpath>
                <value><a>1</a></value>
            </Operation>"#,
    };
    let replace_b = PatchContribution {
        mod_id: &mod_b,
        operation_xml: r#"<Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="W"]/b</xpath>
                <value><b>2</b></value>
            </Operation>"#,
    };

    // (contributions, expected op_index of `a`'s patcher, expected
    // op_index of `b`'s patcher) — swapping the order swaps which
    // index each patcher's own op landed at, but never which mod
    // touched which field (the fields are disjoint).
    let orderings = [
        (vec![replace_a, replace_b], 0, 1),
        (vec![replace_b, replace_a], 1, 0),
    ];

    for (contributions, a_op_index, b_op_index) in orderings {
        let effective = compute(EffectiveInput {
            winner: &winner,
            raw: raw.clone(),
            contributions: &contributions,
            context: context(&active_mods, &names, "ThingDef", "W"),
            templates: &templates,
            template_owners: &template_owners,
        });

        assert_eq!(effective.completeness, Completeness::Complete);
        assert_eq!(
            effective.provenance.get(&"a".parse().unwrap()),
            Some(&Provenance::Patch {
                mod_id: mod_a.clone(),
                op_index: a_op_index,
            })
        );
        assert_eq!(
            effective.provenance.get(&"b".parse().unwrap()),
            Some(&Provenance::Patch {
                mod_id: mod_b.clone(),
                op_index: b_op_index,
            })
        );
    }
}

#[test]
fn three_patchers_adding_disjoint_map_keys_are_each_credited_to_their_own_mod() {
    // Keyed children are already `Child` leaves with stable paths, so this
    // needs no change to `compute` at all — it's the same disjoint-
    // fields rule the test above already pins, just with the fields
    // living under a tag-keyed `<wildAnimals>` container instead of
    // directly under the def root.
    let raw =
        xml::parse("<BiomeDef><wildAnimals><Tortoise>0.4</Tortoise></wildAnimals></BiomeDef>")
            .unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let add_a = PatchContribution {
        mod_id: &mod_a,
        operation_xml: r#"<Operation Class="PatchOperationAdd">
                <xpath>Defs/BiomeDef[defName="W"]/wildAnimals</xpath>
                <value><Allosaurus>0.6</Allosaurus></value>
            </Operation>"#,
    };
    let add_b = PatchContribution {
        mod_id: &mod_b,
        operation_xml: r#"<Operation Class="PatchOperationAdd">
                <xpath>Defs/BiomeDef[defName="W"]/wildAnimals</xpath>
                <value><Mammoth>0.1</Mammoth></value>
            </Operation>"#,
    };
    let add_c = PatchContribution {
        mod_id: &mod_c,
        operation_xml: r#"<Operation Class="PatchOperationAdd">
                <xpath>Defs/BiomeDef[defName="W"]/wildAnimals</xpath>
                <value><Hyena>0.2</Hyena></value>
            </Operation>"#,
    };
    let contributions = vec![add_a, add_b, add_c];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "BiomeDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective
            .provenance
            .get(&"wildAnimals/Allosaurus".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_a,
            op_index: 0,
        })
    );
    assert_eq!(
        effective
            .provenance
            .get(&"wildAnimals/Mammoth".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_b,
            op_index: 1,
        })
    );
    assert_eq!(
        effective
            .provenance
            .get(&"wildAnimals/Hyena".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_c,
            op_index: 2,
        })
    );
    assert_eq!(
        effective
            .resolved
            .get(&"wildAnimals/Tortoise".parse().unwrap())
            .map(|node| &node.content),
        Some(&Content::Text("0.4".to_string())),
        "the pre-existing key survives untouched"
    );
}

#[test]
fn two_patchers_agreeing_on_the_same_map_key_credit_the_earlier_mod() {
    // The "agreeing patchers credit the earlier mod" rule
    // (this crate's own CLAUDE.md): mod A
    // adds the key; mod B's own `PatchOperationReplace` of that exact
    // key to the identical value produces no observable diff at the
    // second op (one `<Fox>` node throughout, never a duplicate tag —
    // a duplicate needs two independent `Add`s, not this), so the
    // field stays credited to whichever ran first.
    let raw = xml::parse("<BiomeDef><wildAnimals/></BiomeDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let add_a = PatchContribution {
        mod_id: &mod_a,
        operation_xml: r#"<Operation Class="PatchOperationAdd">
                <xpath>Defs/BiomeDef[defName="W"]/wildAnimals</xpath>
                <value><Fox>0.3</Fox></value>
            </Operation>"#,
    };
    let replace_b = PatchContribution {
        mod_id: &mod_b,
        operation_xml: r#"<Operation Class="PatchOperationReplace">
                <xpath>Defs/BiomeDef[defName="W"]/wildAnimals/Fox</xpath>
                <value><Fox>0.3</Fox></value>
            </Operation>"#,
    };
    let contributions = vec![add_a, replace_b];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "BiomeDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective
            .resolved
            .get(&"wildAnimals/Fox".parse().unwrap())
            .map(|node| &node.content),
        Some(&Content::Text("0.3".to_string()))
    );
    assert_eq!(
        effective
            .provenance
            .get(&"wildAnimals/Fox".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_a,
            op_index: 0,
        }),
        "B's redundant Replace produces no observable diff, so the key stays credited to A"
    );
}

proptest! {
    /// Two patchers replacing the *same* field: whichever comes later
    /// in `contributions` wins, however the pair is ordered — the
    /// "later wins" law, checked over both
    /// permutations rather than assumed from one.
    #[test]
    fn the_later_contribution_on_the_same_field_wins_regardless_of_order(reverse in any::<bool>()) {
        let raw = xml::parse("<ThingDef><a>0</a></ThingDef>").unwrap();
        let winner = ModId::new("mod.owner");
        let mod_a = ModId::new("mod.a");
        let mod_b = ModId::new("mod.b");
        let active_mods = active(&["mod.owner", "mod.a", "mod.b"]);
        let names = BTreeMap::new();
        let templates = TemplateSet::default();
        let template_owners = BTreeMap::new();

        let by_a = PatchContribution {
            mod_id: &mod_a,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/a</xpath>
                    <value><a>from-a</a></value>
                </Operation>"#,
        };
        let by_b = PatchContribution {
            mod_id: &mod_b,
            operation_xml: r#"<Operation Class="PatchOperationReplace">
                    <xpath>Defs/ThingDef[defName="W"]/a</xpath>
                    <value><a>from-b</a></value>
                </Operation>"#,
        };
        let contributions = if reverse { vec![by_b, by_a] } else { vec![by_a, by_b] };
        let expected_last = contributions[1].mod_id.clone();

        let effective = compute(EffectiveInput {
            winner: &winner,
            raw,
            contributions: &contributions,
            context: context(&active_mods, &names, "ThingDef", "W"),
            templates: &templates,
            template_owners: &template_owners,
        });

        prop_assert_eq!(effective.provenance.get(&"a".parse().unwrap()),
            Some(&Provenance::Patch { mod_id: expected_last, op_index: 1 })
        );
    }
}

#[test]
fn a_repeated_top_level_tag_in_the_raw_winner_merges_onto_the_ancestor_in_sequence_not_independently()
 {
    // A real-install shape (`FactionDef/Insect`): `merge_children_shadow`
    // must merge each `child_children` entry against the
    // progressively-updated node `inherit::merge_children` itself
    // re-reads on every match, not the *original*, unmodified parent.
    // Two sibling `<Wrapper>` elements sharing one tag — legal XML,
    // and exactly the shape a broad-matching `PatchOperationAdd`
    // produces — exercise it: the real fold chains both merges (`Bar`
    // from the first `Wrapper`, then `Baz` from the second, onto the
    // *result* of the first merge), so `Bar` ends up the raw winner's
    // own (`Provenance::Owner`). A shadow fold merging the second
    // `Wrapper` against the ancestor's own *original*, pre-merge copy
    // would silently revert `Bar`'s attribution back to the ancestor's
    // (`Provenance::Inherited`) even though the real resolved value is
    // genuinely the winner's.
    let ancestor_owner = ModId::new("mod.ancestor");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><Wrapper><Bar>base</Bar></Wrapper></ThingDef>"#)
            .unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        ancestor_owner.clone(),
    );

    let winner = ModId::new("mod.winner");
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><Wrapper><Bar>A</Bar></Wrapper><Wrapper><Baz>B</Baz></Wrapper></ThingDef>"#)
        .unwrap();
    let active_mods = active(&["mod.ancestor", "mod.winner"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        rim_merge_resolved_text(&effective, "Wrapper/Bar"),
        Some("A".to_string()),
        "the second sibling Wrapper's own merge must chain onto the first's result, not replace it"
    );
    assert_eq!(
        rim_merge_resolved_text(&effective, "Wrapper/Baz"),
        Some("B".to_string())
    );
    assert_eq!(
        owner_path(&effective, "Wrapper/Bar"),
        winner,
        "Bar is the raw winner's own declaration — the buggy fold misattributed it back to the ancestor"
    );
    assert_eq!(owner_path(&effective, "Wrapper/Baz"), winner);
}

#[test]
fn a_patch_emptied_container_merging_onto_a_non_empty_ancestor_falls_back_to_the_ancestors_own_content()
 {
    // A second real-install shape, companion to the sibling-tag one
    // above (`FactionDef/XGM_RoamingMonstrosities`): `Shadow::of` builds
    // `Shadow::Leaf` for an *empty* `Content::Children` (its own doc
    // comment says so explicitly), so `merge_shadow`'s own
    // `(Content::Children(_), Content::Children(_))` arm must check
    // emptiness, not only the *variant*, before unwrapping both sides
    // as `Shadow::Children`. A `PatchOperationRemove` taking the
    // winner's own container down to zero children (a real, common
    // shape: `crate::tree`'s own `Content::Children(vec![])` doc
    // comment on `Shadow` calls this "technically legal", not rare)
    // would otherwise reach that `unreachable!`.
    let ancestor_owner = ModId::new("mod.ancestor");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><Wrapper><Bar>base</Bar></Wrapper></ThingDef>"#)
            .unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        ancestor_owner.clone(),
    );

    let winner = ModId::new("mod.winner");
    let raw =
        xml::parse(r#"<ThingDef ParentName="Base"><Wrapper><Bar>own</Bar></Wrapper></ThingDef>"#)
            .unwrap();
    let remover = ModId::new("mod.remover");
    let remove_bar = PatchContribution {
        mod_id: &remover,
        operation_xml: r#"<Operation Class="PatchOperationRemove">
                <xpath>Defs/ThingDef[defName="W"]/Wrapper/Bar</xpath>
            </Operation>"#,
    };
    let active_mods = active(&["mod.ancestor", "mod.winner", "mod.remover"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[remove_bar],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        rim_merge_resolved_text(&effective, "Wrapper/Bar"),
        Some("base".to_string()),
        "the winner's own Bar was removed; the ancestor's own must survive"
    );
    assert_eq!(
        effective.provenance.get(&"Wrapper/Bar".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string()
            },
            owner: ancestor_owner,
        })
    );
}

#[test]
fn merge_shadow_of_a_container_emptied_by_a_remove_on_both_sides_returns_a_leaf_not_an_empty_children()
 {
    // Hardening. `Shadow::Children`'s own doc comment states the
    // invariant `merge_shadow`'s `(Content::Children, Content::Children)`
    // arm relies on, and the arm enforces it for itself: nothing ever
    // builds `Shadow::Children(vec![])` — `Shadow::of` returns
    // `Shadow::Leaf` for an empty `Content::Children` instead. Unguarded,
    // two empty sides here would fall through to
    // `merge_children_shadow` and return `Shadow::Children(vec![])`
    // regardless — a shape `zip_leaves` cannot read (its own
    // `Content::Children`-empty arm unwraps straight to
    // `Shadow::Leaf`, `unreachable!` otherwise).
    //
    // Both real call sites of `merge_shadow` (`fold_inheritance`)
    // only ever hand it a raw, unpatched ancestor as `parent` —
    // `xml::parse` never itself produces `Content::Children(vec![])`
    // (only `Content::Empty`, confirmed above by
    // `parse_content`/`Shadow::Children`'s own doc comment) — so
    // today only a `PatchOperationRemove`-emptied *child* side can
    // reach this arm with an empty `Content::Children`, never the
    // parent side too (see the sibling
    // `a_patch_emptied_container_merging_onto_a_non_empty_ancestor...`
    // test above, whose ancestor is non-empty). Both-empty is
    // therefore not reachable through `compute`'s own public API
    // today — this test calls the private `merge_shadow` directly,
    // the same defense-in-depth the doc comment above already
    // describes as an invariant worth holding regardless of whether
    // today's callers can trip it.
    let container = |tag: &str| FieldNode {
        tag: tag.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(Vec::new()),
    };
    let parent_provenance = Provenance::Owner(ModId::new("mod.parent"));
    let child_provenance = Provenance::Owner(ModId::new("mod.child"));

    let result = merge_shadow(
        &container("Wrapper"),
        &Shadow::Leaf(parent_provenance),
        &container("Wrapper"),
        &Shadow::Leaf(child_provenance.clone()),
    );

    assert_eq!(
        result,
        Shadow::Leaf(child_provenance),
        "must mirror Shadow::of's own empty-Content::Children rule (Shadow::Leaf), \
             attributing the redeclaration to the child side"
    );
}

/// The resolved text at `path`, for asserting `effective.resolved`
/// alongside its `provenance` in the same test.
fn rim_merge_resolved_text(def: &EffectiveDef, path: &str) -> Option<String> {
    let field_path: FieldPath = path.parse().unwrap();
    match def.resolved.get(&field_path)?.content {
        Content::Text(ref text) => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn a_two_template_parent_chain_attributes_inherited_fields_to_each_templates_owner_and_a_raw_shadow_stays_owner()
 {
    let root_owner = ModId::new("mod.core");
    let mid_owner = ModId::new("mod.mid");
    let child_owner = ModId::new("mod.child");

    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Root".to_string()),
        xml::parse(r#"<ThingDef Name="Root"><a>from-root</a><b>from-root</b></ThingDef>"#).unwrap(),
    );
    by_name.insert(
        ("ThingDef".to_string(), "Mid".to_string()),
        xml::parse(
            r#"<ThingDef Name="Mid" ParentName="Root"><b>from-mid</b><c>from-mid</c></ThingDef>"#,
        )
        .unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Root".to_string()),
        root_owner.clone(),
    );
    template_owners.insert(
        ("ThingDef".to_string(), "Mid".to_string()),
        mid_owner.clone(),
    );

    let raw = xml::parse(r#"<ThingDef ParentName="Mid"><c>from-raw</c></ThingDef>"#).unwrap();
    let active_mods = active(&["mod.core", "mod.mid", "mod.child"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &child_owner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective.provenance.get(&"a".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Root".to_string()
            },
            owner: root_owner,
        })
    );
    assert_eq!(
        effective.provenance.get(&"b".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Mid".to_string()
            },
            owner: mid_owner,
        }),
        "Mid shadows Root's own b — the closer ancestor wins"
    );
    assert_eq!(owner_path(&effective, "c"), child_owner);
}

#[test]
fn an_unsupported_contribution_stops_the_fold_and_leaves_the_prefixs_result_untouched() {
    let raw = xml::parse("<ThingDef><x>1</x><y>2</y></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let replace_x = r#"<Operation Class="PatchOperationReplace">
            <xpath>Defs/ThingDef[defName="W"]/x</xpath>
            <value><x>9</x></value>
        </Operation>"#;
    // No `<xpath>`, no `<operations>`/`<match>`/`<operation>` structure,
    // and not one of `patch_eval`'s known custom classes — falls
    // through every recognized shape to `Unsupported`.
    let unsupported = r#"<Operation Class="Some.Totally.Unknown.Class"></Operation>"#;
    let replace_y = r#"<Operation Class="PatchOperationReplace">
            <xpath>Defs/ThingDef[defName="W"]/y</xpath>
            <value><y>9</y></value>
        </Operation>"#;

    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: replace_x,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: unsupported,
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: replace_y,
        },
    ];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw: raw.clone(),
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    match &effective.completeness {
        Completeness::Partial {
            stopped_at: Stopper::Replay {
                mod_id, op_index, ..
            },
        } => {
            assert_eq!(mod_id, &mod_b);
            assert_eq!(*op_index, 1);
        }
        other => panic!("expected a Replay stopper at mod.b, got {other:?}"),
    }
    // `x` was successfully patched before the stop; `y` was never
    // reached, so it stays the winner's own raw value.
    assert_eq!(
        effective.provenance.get(&"x".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_a.clone(),
            op_index: 0
        })
    );
    assert_eq!(owner_path(&effective, "y"), winner);

    // `resolved` is exactly a direct replay of the good prefix alone —
    // nothing guessed past the stopper.
    let direct = patch_eval::replay(
        raw,
        &contributions[..1],
        &context(&active_mods, &names, "ThingDef", "W"),
    );
    assert!(direct.error.is_none());
    assert_eq!(effective.resolved, direct.tree);
}

/// Two real content mods' shape — one mod whole-def `Remove`s a def,
/// a later mod's own `PatchOperationConditional` tests a now-gone
/// sub-path and falls to its `<nomatch>`, whose own `Add` on the (also
/// now-gone) bare def head fails too. The headline assertion is that
/// this does not stop the whole fold at `Completeness::Partial`: both
/// contributions are reached and reported, the failing sub-path check
/// isn't guessed at, and `Caveat::DefRemoved` discloses the removal.
/// `context()`'s own `this_def_present: true` is deliberately left
/// unchanged (it's `compute`'s own local, rebuilt copy — see
/// `compute`'s comment on it — that must flip after contribution A,
/// not this fixture's starting input).
#[test]
fn a_whole_def_remove_followed_by_a_later_op_on_the_same_def_is_modelled() {
    let raw = xml::parse("<ThingDef><x>1</x></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let remover = ModId::new("example.hygienepatches");
    let conditional_mod = ModId::new("example.progression.temperature");
    let active_mods = active(&[
        "mod.owner",
        "example.hygienepatches",
        "example.progression.temperature",
    ]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let whole_def_remove = r#"<Operation Class="PatchOperationRemove">
            <xpath>Defs/ThingDef[defName="W"]</xpath>
        </Operation>"#;
    let conditional = r#"<Operation Class="PatchOperationConditional">
            <xpath>Defs/ThingDef[defName="W"]/researchPrerequisites</xpath>
            <nomatch Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="W"]</xpath>
                <value><researchPrerequisites><li>Foo</li></researchPrerequisites></value>
            </nomatch>
        </Operation>"#;

    let contributions = vec![
        PatchContribution {
            mod_id: &remover,
            operation_xml: whole_def_remove,
        },
        PatchContribution {
            mod_id: &conditional_mod,
            operation_xml: conditional,
        },
    ];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(
        effective.completeness,
        Completeness::Complete,
        "the whole-def Remove must no longer stop the fold: {:?}",
        effective.completeness
    );
    assert_eq!(effective.top_level_outcomes.len(), 2);
    assert!(
        effective.top_level_outcomes[0].succeeded,
        "the Remove itself succeeds"
    );
    let conditional_outcome = &effective.top_level_outcomes[1];
    assert!(
        !conditional_outcome.succeeded,
        "the Conditional's own nomatch Add fails once the def is gone"
    );
    assert!(
        conditional_outcome
            .identity
            .contains(r#"Defs/ThingDef[defName="W"]/researchPrerequisites"#),
        "{}",
        conditional_outcome.identity
    );
    assert!(
        effective
            .caveats
            .iter()
            .any(|c| matches!(c, Caveat::DefRemoved { mod_id } if *mod_id == remover)),
        "{:?}",
        effective.caveats
    );
    assert!(
        effective
            .caveats
            .iter()
            .any(|c| matches!(c, Caveat::FailedOp { mod_id, .. } if *mod_id == conditional_mod)),
        "{:?}",
        effective.caveats
    );
}

/// Emptying `tree.root.content` alone is not enough. `tree.parent_name`
/// is a separate field (`FieldTree`'s own doc comment — stripped off the
/// root at parse time, untouched by the removal), so an unguarded
/// `fold_inheritance` would still walk the `ParentName` chain over the
/// emptied tree and merge the ancestor's own fields back in — silently
/// resurrecting inherited content for a def RimWorld deleted from the
/// document outright (a real install's `ThingDef/ExampleFan` would
/// resolve to ExampleBadHygiene's full raw def, not empty). A deleted
/// XML node is never an inheritance *target* either — `compute`'s own
/// `def_removed` flag skips `fold_inheritance` entirely once set.
#[test]
fn a_whole_def_remove_does_not_resurrect_the_def_via_its_own_parent_name_chain() {
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><label>from-ancestor</label></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let ancestor_owner = ModId::new("mod.ancestor");
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        ancestor_owner.clone(),
    );

    let raw = xml::parse(r#"<ThingDef ParentName="Base"><label>own</label></ThingDef>"#).unwrap();
    let winner = ModId::new("mod.owner");
    let remover = ModId::new("example.hygienepatches");
    let active_mods = active(&["mod.ancestor", "mod.owner", "example.hygienepatches"]);
    let names = BTreeMap::new();

    let whole_def_remove = r#"<Operation Class="PatchOperationRemove">
            <xpath>Defs/ThingDef[defName="W"]</xpath>
        </Operation>"#;
    let contributions = vec![PatchContribution {
        mod_id: &remover,
        operation_xml: whole_def_remove,
    }];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective.resolved.root.content,
        Content::Empty,
        "the ancestor's own <label> must not be merged back in"
    );
    assert!(
        effective.provenance.is_empty(),
        "{:?}",
        effective.provenance
    );
}

/// A mod bundling an unrelated `/Defs` root `Add` (a whole new
/// top-level def) into the same patch file must not stop the fold
/// outright — without a recognized head for the bare document root,
/// the very first contribution would produce a `Replay` stopper and
/// every later contribution (including a `Replace` that genuinely
/// targets this def) would be "not reached". With the root add treated
/// as a no-op here, the fold runs to `Complete` and both root adds
/// contribute no provenance at all.
#[test]
fn a_root_add_before_and_after_a_real_patch_no_longer_stops_the_fold() {
    let raw = xml::parse("<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let patcher = ModId::new("mod.patcher");
    let active_mods = active(&["mod.owner", "mod.patcher"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let root_add = r#"<Operation Class="PatchOperationAdd">
            <xpath>/Defs</xpath>
            <value><WorldGenStepDef><defName>New</defName></WorldGenStepDef></value>
        </Operation>"#;
    let replace = r#"<Operation Class="PatchOperationReplace">
            <xpath>Defs/BiomeDef[defName="W"]/plantDensity</xpath>
            <value><plantDensity>0.9</plantDensity></value>
        </Operation>"#;

    let contributions = vec![
        PatchContribution {
            mod_id: &patcher,
            operation_xml: root_add,
        },
        PatchContribution {
            mod_id: &patcher,
            operation_xml: replace,
        },
        PatchContribution {
            mod_id: &patcher,
            operation_xml: root_add,
        },
    ];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "BiomeDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert!(effective.caveats.is_empty());
    assert_eq!(
        effective.provenance.get(&"plantDensity".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: patcher,
            op_index: 1,
        })
    );
}

#[test]
fn the_bionic_heart_worked_example_snapshots_a_mixed_owner_patch_and_inherited_provenance_table() {
    // Fixtures are the worked example already used by
    // `tests/closure.rs` — trimmed real Core/Example Bionics Fork XML.
    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/xml/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    }
    fn parse_siblings(multi_def_xml: &str) -> Vec<FieldTree> {
        let doc = roxmltree::Document::parse(multi_def_xml)
            .unwrap_or_else(|error| panic!("well-formed fixture XML: {error}"));
        doc.root_element()
            .children()
            .filter(roxmltree::Node::is_element)
            .map(|node| {
                xml::parse(&multi_def_xml[node.range()])
                    .unwrap_or_else(|error| panic!("well-formed def/template: {error}"))
            })
            .collect()
    }

    let bionics_bionic_heart = xml::parse(&fixture("bionics_bionic_heart.xml")).unwrap();

    let core_id = ModId::new("ludeon.rimworld");
    let bionics_id = ModId::new("example.bionicsfork");
    let compat_id = ModId::new("mod.compat");

    let mut by_name = BTreeMap::new();
    let mut template_owners = BTreeMap::new();
    for template in parse_siblings(&fixture("core_hediff_bases.xml")) {
        let name = template.name.clone().expect("Core template has a Name");
        template_owners.insert(("HediffDef".to_string(), name.clone()), core_id.clone());
        by_name.insert(("HediffDef".to_string(), name), template);
    }
    let bionics_template = xml::parse(&fixture("bionics_hediff_base.xml")).unwrap();
    let bionics_template_name = bionics_template.name.clone().unwrap();
    template_owners.insert(
        ("HediffDef".to_string(), bionics_template_name.clone()),
        bionics_id.clone(),
    );
    by_name.insert(
        ("HediffDef".to_string(), bionics_template_name),
        bionics_template,
    );
    let templates = TemplateSet::new(by_name);

    let active_mods = active(&["ludeon.rimworld", "example.bionicsfork", "mod.compat"]);
    let names = BTreeMap::new();
    let contributions = [PatchContribution {
        mod_id: &compat_id,
        operation_xml: r#"<Operation Class="PatchOperationReplace">
                <xpath>Defs/HediffDef[defName="BionicHeart"]/label</xpath>
                <value><label>cybernetic heart</label></value>
            </Operation>"#,
    }];

    let effective = compute(EffectiveInput {
        winner: &bionics_id,
        raw: bionics_bionic_heart,
        contributions: &contributions,
        context: context(&active_mods, &names, "HediffDef", "BionicHeart"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    insta::assert_debug_snapshot!("bionic_heart_effective_provenance", &effective.provenance);
}

/// The provenance invariant: whatever `compute` produces, `provenance` must
/// cover *exactly* `resolved`'s own leaf paths — never fewer (a dropped leaf)
/// and never more (a stale path from a stage inheritance folded away).
fn assert_provenance_is_exactly_resolved_leaves(effective: &EffectiveDef) {
    let resolved_paths: BTreeSet<FieldPath> =
        effective.resolved.leaves().map(|(path, _)| path).collect();
    let provenance_paths: BTreeSet<FieldPath> = effective.provenance.keys().cloned().collect();
    assert_eq!(provenance_paths, resolved_paths);
}

// -- Provenance attribution edge cases ------------------------------

#[test]
fn a_raw_empty_leaf_is_attributed_to_the_declaring_ancestor_not_the_winner() {
    // `<label/>` is "not an override" per
    // `crate::inherit::resolve`'s own doc comment, so the value the
    // game actually runs with comes from `Base` — a diff-based fold
    // would never override `label`'s initial `Owner(winner)` entry
    // if `attribute_inherited` only filled gaps.
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><defName>W</defName><label/></ThingDef>"#)
        .unwrap();
    let winner = ModId::new("mod.owner");
    let base_owner = ModId::new("mod.core");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><label>from base</label></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        base_owner.clone(),
    );
    let active_mods = active(&["mod.owner", "mod.core"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective
            .resolved
            .get(&"label".parse().unwrap())
            .unwrap()
            .content,
        Content::Text("from base".to_string())
    );
    assert_eq!(
        effective.provenance.get(&"label".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
            owner: base_owner,
        }),
        "an empty raw leaf never took effect — the ancestor that actually supplied \
             the value must get the credit, not the winner"
    );
    assert_eq!(owner_path(&effective, "defName"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_middle_template_redeclaring_the_same_value_still_wins_over_the_root() {
    // `Mid`'s own `<b>1</b>` renders identically to `Root`'s, so
    // a diff of rendered values sees nothing "change" at the `Mid`
    // step — the rule has to be "did this ancestor declare it", not
    // "did the merged value differ from before".
    let root_owner = ModId::new("mod.root");
    let mid_owner = ModId::new("mod.mid");
    let winner = ModId::new("mod.owner");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Root".to_string()),
        xml::parse(r#"<ThingDef Name="Root"><b>1</b></ThingDef>"#).unwrap(),
    );
    by_name.insert(
        ("ThingDef".to_string(), "Mid".to_string()),
        xml::parse(r#"<ThingDef Name="Mid" ParentName="Root"><b>1</b></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(("ThingDef".to_string(), "Root".to_string()), root_owner);
    template_owners.insert(
        ("ThingDef".to_string(), "Mid".to_string()),
        mid_owner.clone(),
    );
    let raw = xml::parse(r#"<ThingDef ParentName="Mid"><defName>W</defName></ThingDef>"#).unwrap();
    let active_mods = active(&["mod.root", "mod.mid", "mod.owner"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective.provenance.get(&"b".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Mid".to_string(),
            },
            owner: mid_owner,
        }),
        "Mid redeclares Root's own b with the identical value — it still wins, \
             since it's still an override"
    );
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_raw_li_item_colliding_with_a_templates_own_li_identity_keeps_both_items_attributed() {
    // Both `Class="A"` items only collide once they're combined
    // (neither owner's own container has a duplicate on its own), so
    // a chain-only fold's identity for `Base`'s item would never
    // match the positional identity the real merged tree assigns
    // it, and the whole subtree would vanish from `provenance`.
    let winner = ModId::new("mod.owner");
    let base_owner = ModId::new("mod.core");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(
            r#"<ThingDef Name="Base"><comps><li Class="A"><x>base</x></li></comps></ThingDef>"#,
        )
        .unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        base_owner.clone(),
    );
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><defName>W</defName><comps><li Class="A"><x>raw</x></li></comps></ThingDef>"#)
        .unwrap();
    let active_mods = active(&["mod.owner", "mod.core"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    // The merge law appends parent items before child items, so
    // `Base`'s own item is always first once both fall back to
    // position.
    assert_eq!(
        effective.provenance.get(&"comps/li[#0]".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
            owner: base_owner,
        })
    );
    assert_eq!(owner_path(&effective, "comps/li[#1]"), winner);
    assert!(
        !effective
            .provenance
            .contains_key(&"comps/li[@Class=A]".parse().unwrap()),
        "the pre-collision identity must not linger once both items fall back to position"
    );
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn an_empty_raw_container_that_inherits_real_children_is_addressed_by_those_children_not_itself() {
    // `comps` starts as a leaf (`Owner(winner)`, empty
    // content) but ends up a container once it inherits real
    // children — the stale top-level entry must not linger.
    let winner = ModId::new("mod.owner");
    let base_owner = ModId::new("mod.core");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><comps><li Class="A"/></comps></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        base_owner.clone(),
    );
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><defName>W</defName><comps/></ThingDef>"#)
        .unwrap();
    let active_mods = active(&["mod.owner", "mod.core"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert!(
        !effective.provenance.contains_key(&"comps".parse().unwrap()),
        "comps is no longer a leaf once it inherits a real child"
    );
    assert_eq!(
        effective
            .provenance
            .get(&"comps/li[@Class=A]".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
            owner: base_owner,
        })
    );
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_template_missing_from_template_owners_is_never_silently_unattributed() {
    // The session should always wire an owner for every template
    // it registers, but a gap here must be named, not guessed at or
    // silently dropped.
    let winner = ModId::new("mod.owner");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><label>from base</label></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let template_owners = BTreeMap::new(); // deliberately empty
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><defName>W</defName></ThingDef>"#).unwrap();
    let active_mods = active(&["mod.owner"]);
    let names = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective.provenance.get(&"label".parse().unwrap()),
        Some(&Provenance::UnattributedTemplate {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
        }),
        "a template missing from template_owners must be named, never guessed at \
             nor silently dropped"
    );
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_removed_field_is_absent_from_both_resolved_and_provenance() {
    let raw = xml::parse("<ThingDef><a>1</a><b>2</b></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let remover = ModId::new("mod.remover");
    let active_mods = active(&["mod.owner", "mod.remover"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();
    let contributions = [PatchContribution {
        mod_id: &remover,
        operation_xml: r#"<Operation Class="PatchOperationRemove">
                <xpath>Defs/ThingDef[defName="W"]/a</xpath>
            </Operation>"#,
    }];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert!(effective.resolved.get(&"a".parse().unwrap()).is_none());
    assert!(!effective.provenance.contains_key(&"a".parse().unwrap()));
    assert_eq!(owner_path(&effective, "b"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_contribution_matching_zero_nodes_surfaces_a_failed_op_caveat_and_stays_complete() {
    let raw = xml::parse("<ThingDef><a>1</a></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let active_mods = active(&["mod.owner", "mod.a"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();
    let contributions = [PatchContribution {
        mod_id: &mod_a,
        operation_xml: r#"<Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="W"]/nope</xpath>
                <value><nope>1</nope></value>
            </Operation>"#,
    }];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    assert!(matches!(effective.caveats.as_slice(),
        [Caveat::FailedOp { mod_id, .. }] if *mod_id == mod_a
    ));
    assert_eq!(owner_path(&effective, "a"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_malformed_contribution_stops_the_fold_and_surfaces_its_own_caveat() {
    let raw = xml::parse("<ThingDef><a>1</a></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let active_mods = active(&["mod.owner", "mod.a"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();
    let contributions = [PatchContribution {
        mod_id: &mod_a,
        operation_xml: "<Operation Class=\"PatchOperationReplace\"><xpath>unterminated",
    }];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    match &effective.completeness {
        Completeness::Partial {
            stopped_at: Stopper::Replay {
                mod_id, op_index, ..
            },
        } => {
            assert_eq!(mod_id, &mod_a);
            assert_eq!(*op_index, 0);
        }
        other => panic!("expected a Replay stopper for the malformed op, got {other:?}"),
    }
    assert!(matches!(effective.caveats.as_slice(),
        [Caveat::MalformedOperation { mod_id }] if *mod_id == mod_a
    ));
    assert_eq!(owner_path(&effective, "a"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn a_broken_parent_chain_stops_at_an_inherit_stopper_and_keeps_the_patch_stages_provenance() {
    let raw = xml::parse(r#"<ThingDef ParentName="Nope"><a>1</a></ThingDef>"#).unwrap();
    let winner = ModId::new("mod.owner");
    let active_mods = active(&["mod.owner"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default(); // "Nope" is never registered
    let template_owners = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    match &effective.completeness {
        Completeness::Partial {
            stopped_at: Stopper::Inherit(InheritError::MissingParent { def_type, name }),
        } => {
            assert_eq!(def_type, "ThingDef");
            assert_eq!(name, "Nope");
        }
        other => panic!("expected an Inherit stopper, got {other:?}"),
    }
    assert_eq!(owner_path(&effective, "a"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn inheritance_still_runs_against_the_prefix_when_the_patch_stage_stops_partway() {
    let raw = xml::parse(r#"<ThingDef ParentName="Base"><defName>W</defName><a>1</a></ThingDef>"#)
        .unwrap();
    let winner = ModId::new("mod.owner");
    let base_owner = ModId::new("mod.core");
    let mod_a = ModId::new("mod.a");
    let mut by_name = BTreeMap::new();
    by_name.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        xml::parse(r#"<ThingDef Name="Base"><fromBase>1</fromBase></ThingDef>"#).unwrap(),
    );
    let templates = TemplateSet::new(by_name);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        base_owner.clone(),
    );
    let active_mods = active(&["mod.owner", "mod.core", "mod.a"]);
    let names = BTreeMap::new();
    let contributions = [PatchContribution {
        mod_id: &mod_a,
        operation_xml: r#"<Operation Class="Some.Totally.Unknown.Class"></Operation>"#,
    }];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert!(matches!(
        effective.completeness,
        Completeness::Partial {
            stopped_at: Stopper::Replay { .. }
        }
    ));
    assert_eq!(
        effective.provenance.get(&"fromBase".parse().unwrap()),
        Some(&Provenance::Inherited {
            template: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
            owner: base_owner,
        }),
        "inheritance runs against the best tree available even when the patch stage stopped"
    );
    assert_eq!(owner_path(&effective, "a"), winner);
    assert_eq!(owner_path(&effective, "defName"), winner);
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

/// Deterministic Fisher-Yates over `0..len`, seeded by `seed` —
/// proptest has no built-in permutation strategy, and pulling in
/// `rand` for one test isn't worth a new dependency; a xorshift64
/// seeded by proptest's own generated `u64` still explores genuinely
/// different orderings across runs (and proptest can still shrink
/// the seed on a failure).
fn permutation_from_seed(seed: u64, len: usize) -> Vec<usize> {
    let mut state = if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    };
    let mut indices: Vec<usize> = (0..len).collect();
    for i in (1..len).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        indices.swap(i, j);
    }
    indices
}

proptest! {
    /// Generalized: the "later wins" law and
    /// disjoint-field order-independence, checked over genuinely
    /// randomized orderings of five patchers rather than one
    /// reversed pair.
    #[test]
    fn any_permutation_of_patchers_gives_last_writer_wins_on_a_shared_field_and_leaves_disjoint_fields_alone(seed in any::<u64>()) {
        let raw = xml::parse("<ThingDef><a>0</a><b>0</b><c>0</c><shared>0</shared></ThingDef>").unwrap();
        let winner = ModId::new("mod.owner");
        let mod_a = ModId::new("mod.a");
        let mod_b = ModId::new("mod.b");
        let mod_c = ModId::new("mod.c");
        let mod_shared1 = ModId::new("mod.shared1");
        let mod_shared2 = ModId::new("mod.shared2");
        let mut active_mods = active(&["mod.a", "mod.b", "mod.c", "mod.shared1", "mod.shared2"]);
        active_mods.insert(winner.clone());
        let names = BTreeMap::new();

        let replace_a = PatchContribution {
            mod_id: &mod_a,
            operation_xml: r#"<Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="W"]/a</xpath><value><a>A</a></value></Operation>"#,
        };
        let replace_b = PatchContribution {
            mod_id: &mod_b,
            operation_xml: r#"<Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="W"]/b</xpath><value><b>B</b></value></Operation>"#,
        };
        let replace_c = PatchContribution {
            mod_id: &mod_c,
            operation_xml: r#"<Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="W"]/c</xpath><value><c>C</c></value></Operation>"#,
        };
        let replace_shared1 = PatchContribution {
            mod_id: &mod_shared1,
            operation_xml: r#"<Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="W"]/shared</xpath><value><shared>one</shared></value></Operation>"#,
        };
        let replace_shared2 = PatchContribution {
            mod_id: &mod_shared2,
            operation_xml: r#"<Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="W"]/shared</xpath><value><shared>two</shared></value></Operation>"#,
        };

        let base = [replace_a, replace_b, replace_c, replace_shared1, replace_shared2];
        let order = permutation_from_seed(seed, base.len());
        let contributions: Vec<PatchContribution> = order.iter().map(|&i| base[i]).collect();

        let effective = compute(EffectiveInput {
            winner: &winner,
            raw,
            contributions: &contributions,
            context: context(&active_mods, &names, "ThingDef", "W"),
            templates: &TemplateSet::default(),
            template_owners: &BTreeMap::new(),
        });

        prop_assert_eq!(effective.completeness, Completeness::Complete);

        for (field, expected_mod) in [("a", &mod_a), ("b", &mod_b), ("c", &mod_c)] {
            let path: FieldPath = field.parse().unwrap();
            let attributed_to_expected_mod = matches!(effective.provenance.get(&path),
                Some(Provenance::Patch { mod_id, .. }) if mod_id == expected_mod
            );
            prop_assert!(attributed_to_expected_mod);
        }

        // Base index 3 is `mod_shared1`, 4 is `mod_shared2` — whichever
        // comes later in this specific ordering wins the shared field.
        let position_of = |target: usize| order.iter().position(|&i| i == target).unwrap();
        let (last_shared_mod, last_shared_index) = if position_of(3) > position_of(4) {
            (mod_shared1, position_of(3))
        } else {
            (mod_shared2, position_of(4))
        };
        prop_assert_eq!(effective.provenance.get(&"shared".parse().unwrap()),
            Some(&Provenance::Patch { mod_id: last_shared_mod, op_index: last_shared_index })
        );
    }

    /// Generalized: whatever `li` duplication shows up across the
    /// `ParentName` chain and the winner's own node, `provenance`
    /// always covers exactly `resolved`'s own leaf paths — the same
    /// invariant [`assert_provenance_is_exactly_resolved_leaves`]
    /// checks in the targeted tests above, now over many random
    /// shapes rather than one hand-picked one.
    #[test]
    fn provenance_always_matches_resolved_leaves_across_random_li_duplication(root_classes in proptest::collection::vec("[AB]", 0..3),
        mid_classes in proptest::collection::vec("[AB]", 0..3),
        raw_classes in proptest::collection::vec("[AB]", 0..3),
        raw_has_its_own_field in any::<bool>()) {
        fn li_list(classes: &[String]) -> String {
            classes.iter().map(|class| format!(r#"<li Class="{class}"/>"#)).collect()
        }

        let root_owner = ModId::new("mod.root");
        let mid_owner = ModId::new("mod.mid");
        let winner = ModId::new("mod.winner");

        let mut by_name = BTreeMap::new();
        by_name.insert(("ThingDef".to_string(), "Root".to_string()),
            xml::parse(&format!(r#"<ThingDef Name="Root"><comps>{}</comps></ThingDef>"#,
                li_list(&root_classes)
            ))
            .unwrap());
        by_name.insert(("ThingDef".to_string(), "Mid".to_string()),
            xml::parse(&format!(r#"<ThingDef Name="Mid" ParentName="Root"><comps>{}</comps></ThingDef>"#,
                li_list(&mid_classes)
            ))
            .unwrap());
        let templates = TemplateSet::new(by_name);
        let mut template_owners = BTreeMap::new();
        template_owners.insert(("ThingDef".to_string(), "Root".to_string()), root_owner);
        template_owners.insert(("ThingDef".to_string(), "Mid".to_string()), mid_owner);

        let own_field = if raw_has_its_own_field { "<shared>x</shared>" } else { "" };
        let raw = xml::parse(&format!(r#"<ThingDef ParentName="Mid"><defName>W</defName>{own_field}<comps>{}</comps></ThingDef>"#,
            li_list(&raw_classes)
        ))
        .unwrap();
        let active_mods = active(&["mod.root", "mod.mid", "mod.winner"]);
        let names = BTreeMap::new();

        let effective = compute(EffectiveInput {
            winner: &winner,
            raw,
            contributions: &[],
            context: context(&active_mods, &names, "ThingDef", "W"),
            templates: &templates,
            template_owners: &template_owners,
        });

        prop_assert_eq!(effective.completeness, Completeness::Complete);
        let resolved_paths: BTreeSet<FieldPath> =
            effective.resolved.leaves().map(|(path, _)| path).collect();
        let provenance_paths: BTreeSet<FieldPath> = effective.provenance.keys().cloned().collect();
        prop_assert_eq!(provenance_paths, resolved_paths);
    }
}

// -- "list case" dedup ----------------------------------------------

/// Two `PatchOperationAdd`s on an empty `comps`, each an `<li Class="Alpha">`
/// wrapping `<amount>{amount_a}</amount>`/`<amount>{amount_b}</amount>` —
/// the shared setup behind every test below.
fn two_colliding_adds(amount_a: &str, amount_b: &str) -> (EffectiveDef, ModId, ModId) {
    let raw = xml::parse("<ThingDef><comps></comps></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let add_a = PatchContribution {
        mod_id: &mod_a,
        operation_xml: &format!(
            r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                    <value><li Class="Alpha"><amount>{amount_a}</amount></li></value>
                </Operation>"#
        ),
    };
    let add_b = PatchContribution {
        mod_id: &mod_b,
        operation_xml: &format!(
            r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                    <value><li Class="Alpha"><amount>{amount_b}</amount></li></value>
                </Operation>"#
        ),
    };
    let contributions = vec![add_a, add_b];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });
    (effective, mod_a, mod_b)
}

#[test]
fn two_patchers_adding_same_identity_items_are_each_credited_to_their_own_mod() {
    // Both positional entries a colliding-identity collision produces
    // must not be credited to whichever mod happened to load last:
    // `mod_a`'s own item never changed once `mod_b`'s add forced the
    // renumbering.
    let (effective, mod_a, mod_b) = two_colliding_adds("1", "2");

    assert_eq!(effective.completeness, Completeness::Complete);
    assert_eq!(
        effective.provenance.get(&"comps/li[#0]".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_a,
            op_index: 0
        }),
        "mod_a's own item never changed once mod_b's add forced the identity collision — \
             it must keep its own attribution, not inherit mod_b's"
    );
    assert_eq!(
        effective.provenance.get(&"comps/li[#1]".parse().unwrap()),
        Some(&Provenance::Patch {
            mod_id: mod_b,
            op_index: 1
        })
    );
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn three_patchers_adding_same_identity_items_are_each_credited_to_their_own_mod() {
    // Generalizes the two-mod case: a third item joining an
    // already-colliding pair must still preserve every earlier item's
    // own attribution, not just the most recently renamed one.
    let raw = xml::parse("<ThingDef><comps></comps></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active_mods = active(&["mod.owner", "mod.a", "mod.b", "mod.c"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                    <value><li Class="Alpha"><amount>1</amount></li></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                    <value><li Class="Alpha"><amount>2</amount></li></value>
                </Operation>"#,
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: r#"<Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                    <value><li Class="Alpha"><amount>3</amount></li></value>
                </Operation>"#,
        },
    ];

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &contributions,
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(effective.completeness, Completeness::Complete);
    for (path, expected_mod, expected_op_index) in [
        ("comps/li[#0]", &mod_a, 0),
        ("comps/li[#1]", &mod_b, 1),
        ("comps/li[#2]", &mod_c, 2),
    ] {
        assert_eq!(
            effective.provenance.get(&path.parse().unwrap()),
            Some(&Provenance::Patch {
                mod_id: expected_mod.clone(),
                op_index: expected_op_index
            }),
            "{path} must keep its own contributor's attribution"
        );
    }
    assert_provenance_is_exactly_resolved_leaves(&effective);
}

#[test]
fn duplicate_of_earlier_sibling_detects_a_byte_identical_colliding_item() {
    let (effective, mod_a, _mod_b) = two_colliding_adds("1", "1");

    let second: FieldPath = "comps/li[#1]".parse().unwrap();
    let (earlier_path, earlier_mod) = duplicate_of_earlier_sibling(&effective, &second)
        .expect("mod_b's item is byte-identical to mod_a's own earlier one");
    assert_eq!(earlier_path, &"comps/li[#0]".parse().unwrap());
    assert_eq!(earlier_mod, &mod_a);

    let first: FieldPath = "comps/li[#0]".parse().unwrap();
    assert_eq!(
        duplicate_of_earlier_sibling(&effective, &first),
        None,
        "the group's own first member has no earlier sibling to match"
    );
}

#[test]
fn duplicate_of_earlier_sibling_is_none_when_colliding_content_actually_differs() {
    let (effective, _mod_a, _mod_b) = two_colliding_adds("1", "2");

    let second: FieldPath = "comps/li[#1]".parse().unwrap();
    assert_eq!(
        duplicate_of_earlier_sibling(&effective, &second),
        None,
        "two genuinely different items sharing one identity must never be folded \
             into an agreement"
    );
}

#[test]
fn duplicate_of_earlier_sibling_is_none_for_a_stable_identity_path() {
    let raw = xml::parse("<ThingDef><a>1</a></ThingDef>").unwrap();
    let winner = ModId::new("mod.owner");
    let active_mods = active(&["mod.owner"]);
    let names = BTreeMap::new();
    let templates = TemplateSet::default();
    let template_owners = BTreeMap::new();

    let effective = compute(EffectiveInput {
        winner: &winner,
        raw,
        contributions: &[],
        context: context(&active_mods, &names, "ThingDef", "W"),
        templates: &templates,
        template_owners: &template_owners,
    });

    assert_eq!(
        duplicate_of_earlier_sibling(&effective, &"a".parse().unwrap()),
        None
    );
}
