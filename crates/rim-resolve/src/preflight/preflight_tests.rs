use std::collections::BTreeMap;

use rim_analyzer::domain::{EdgeKind, LoadOrder, Mod, ModId, Report};

use super::*;
use crate::domain::{
    Action, Confidence, Decision, DecisionSet, FindingKey, OrderSource, RuleSet, SorterOverrides,
    Tagging,
};
use crate::ledger::{BuildLedgerInput, build};
use crate::sort::{EnforcedLayers, SortInput, TieBreak, sort};
use crate::test_support::ReportBuilder;

fn id(raw: &str) -> ModId {
    ModId::new(raw)
}

fn order(ids: &[&str]) -> LoadOrder {
    LoadOrder::new(ids.iter().map(|raw| id(raw)).collect())
}

fn decisions_on(keys: Vec<FindingKey>) -> DecisionSet {
    let mut decisions = DecisionSet::new();
    for key in keys {
        decisions
            .insert(Decision {
                key,
                action: Action::Accept,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("Accept is valid on every finding");
    }
    decisions
}

/// The order `source` would write plus its real ledger, through the real
/// sorter and ledger builder.
fn written(
    report: &Report,
    current: &LoadOrder,
    source: OrderSource,
    decisions: &DecisionSet,
) -> (LoadOrder, Ledger) {
    let outcome = sort(&SortInput {
        report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current,
        enforce: EnforcedLayers::default(),
        tie_break: TieBreak::PreserveCurrent,
    });
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let ledger = build(&BuildLedgerInput {
        report,
        sort_outcome: &outcome,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        decisions,
        threshold: Confidence::new(80).unwrap_or_else(|_| unreachable!()),
        source,
        current,
        suggested: &outcome.order,
        mods_by_id: &mods_by_id,
        show_dangling_def_references: false,
    });
    let order = match source {
        OrderSource::Current => current.clone(),
        OrderSource::Suggested => outcome.order,
    };
    (order, ledger)
}

fn problems_of(report: &Report, current: &LoadOrder, source: OrderSource) -> Vec<PreflightItem> {
    let (order, ledger) = written(report, current, source, &DecisionSet::new());
    hard_problems(report, &order, &ledger)
}

fn problems_in(report: &Report, current: &LoadOrder) -> Vec<HardProblem> {
    problems_of(report, current, OrderSource::Current)
        .into_iter()
        .map(|item| item.problem)
        .collect()
}

#[test]
fn a_clean_order_has_no_hard_problems() {
    let builder = ReportBuilder::new().mod_("a").mod_("b").hard_edge("b", "a");
    let current = builder.insertion_order();

    assert!(problems_of(&builder.build(), &current, OrderSource::Current).is_empty());
}

#[test]
fn a_missing_dependency_is_installed_inactive_when_discovery_found_it() {
    let builder = ReportBuilder::new()
        .mod_("app")
        .inactive("lib")
        .missing_dependency("app", "lib", Some("Lib"));
    let current = builder.insertion_order();

    assert_eq!(
        problems_in(&builder.build(), &current),
        vec![HardProblem::MissingDependency {
            mod_id: id("app"),
            dependency: id("lib"),
            display_name: Some("Lib".to_string()),
            availability: Availability::InstalledInactive,
        }]
    );
}

#[test]
fn a_missing_dependency_is_not_installed_when_discovery_never_saw_it() {
    let builder = ReportBuilder::new()
        .mod_("app")
        .missing_dependency("app", "lib", None);
    let current = builder.insertion_order();

    assert_eq!(
        problems_in(&builder.build(), &current),
        vec![HardProblem::MissingDependency {
            mod_id: id("app"),
            dependency: id("lib"),
            display_name: None,
            availability: Availability::NotInstalled,
        }]
    );
}

#[test]
fn an_inactive_workshop_copy_still_counts_as_installed() {
    let builder = ReportBuilder::new()
        .mod_("app")
        .inactive("lib_steam")
        .missing_dependency("app", "lib", None);
    let current = builder.insertion_order();

    let problems = problems_in(&builder.build(), &current);

    assert!(matches!(
        problems.as_slice(),
        [HardProblem::MissingDependency {
            availability: Availability::InstalledInactive,
            ..
        }]
    ));
}

#[test]
fn a_dependency_the_written_order_contains_is_not_a_problem() {
    let builder = ReportBuilder::new()
        .mod_("app")
        .mod_("lib")
        .missing_dependency("app", "lib", None);
    let current = builder.insertion_order();

    assert!(problems_in(&builder.build(), &current).is_empty());
}

#[test]
fn a_dependency_that_is_listed_but_not_on_disk_does_not_satisfy_the_requirement() {
    let report = ReportBuilder::new()
        .mod_("app")
        .missing_mod("lib")
        .missing_dependency("app", "lib", None)
        .build();

    let problems = problems_in(&report, &order(&["app", "lib"]));

    assert!(problems.iter().any(|problem| matches!(
        problem,
        HardProblem::MissingDependency {
            availability: Availability::NotInstalled,
            ..
        }
    )));
}

#[test]
fn a_dependency_of_a_mod_the_order_omits_is_not_a_problem() {
    let report = ReportBuilder::new()
        .mod_("app")
        .mod_("other")
        .missing_dependency("app", "lib", None)
        .build();

    assert!(problems_in(&report, &order(&["other"])).is_empty());
}

#[test]
fn an_incompatible_pair_is_reported_sorted_when_both_mods_are_in_the_order() {
    let builder = ReportBuilder::new()
        .mod_("zeta")
        .mod_("alpha")
        .incompatible_pair("zeta", "alpha");
    let current = builder.insertion_order();

    assert_eq!(
        problems_in(&builder.build(), &current),
        vec![HardProblem::IncompatiblePair {
            a: id("alpha"),
            b: id("zeta"),
        }]
    );
}

#[test]
fn an_incompatible_pair_is_ignored_when_one_side_is_not_in_the_order() {
    let report = ReportBuilder::new()
        .mod_("alpha")
        .mod_("zeta")
        .incompatible_pair("alpha", "zeta")
        .build();

    assert!(problems_in(&report, &order(&["alpha"])).is_empty());
}

#[test]
fn a_missing_mod_the_order_omits_reads_removed_from_the_active_list() {
    let report = ReportBuilder::new().mod_("a").missing_mod("gone").build();

    assert_eq!(
        problems_in(&report, &order(&["a"])),
        vec![HardProblem::MissingMod {
            mod_id: id("gone"),
            outcome: MissingModOutcome::RemovedFromActiveList,
        }]
    );
}

#[test]
fn a_kept_missing_mod_reads_kept_in_the_active_list() {
    let report = ReportBuilder::new().mod_("a").missing_mod("gone").build();

    assert_eq!(
        problems_in(&report, &order(&["a", "gone"])),
        vec![HardProblem::MissingMod {
            mod_id: id("gone"),
            outcome: MissingModOutcome::KeptInActiveList,
        }]
    );
}

#[test]
fn a_hard_edge_violated_by_current_is_reported_but_satisfied_by_suggested_is_not() {
    // "b" must load after "a", but the current order has it first.
    let builder = ReportBuilder::new().mod_("b").mod_("a").hard_edge("b", "a");
    let current = builder.insertion_order();
    let report = builder.build();

    let on_current = problems_of(&report, &current, OrderSource::Current);
    let on_suggested = problems_of(&report, &current, OrderSource::Suggested);

    assert_eq!(
        on_current,
        vec![PreflightItem {
            problem: HardProblem::LoadRequirementViolated {
                after: id("b"),
                before: id("a"),
                kind: EdgeKind::AssemblyRef,
            },
            acknowledged: false,
        }]
    );
    assert!(on_suggested.is_empty());
}

#[test]
fn declared_soft_and_awareness_violations_are_never_hard_problems() {
    let builder = ReportBuilder::new()
        .mod_("b")
        .mod_("a")
        .declared_edge("b", "a")
        .soft_edge("b", "a")
        .awareness_edge("b", "a");
    let current = builder.insertion_order();

    assert!(problems_in(&builder.build(), &current).is_empty());
}

#[test]
fn an_unsatisfied_any_of_constraint_is_reported_with_sorted_candidates() {
    let builder = ReportBuilder::new()
        .mod_("dependent")
        .mod_("cand.b")
        .mod_("cand.a")
        .any_of("dependent", "shared.dll", &["cand.b", "cand.a"], true);
    let current = builder.insertion_order();

    assert_eq!(
        problems_in(&builder.build(), &current),
        vec![HardProblem::AnyOfUnsatisfied {
            after: id("dependent"),
            candidates: [id("cand.a"), id("cand.b")].into_iter().collect(),
        }]
    );
}

#[test]
fn an_any_of_constraint_with_a_candidate_loading_first_is_not_reported() {
    let builder = ReportBuilder::new()
        .mod_("cand.a")
        .mod_("dependent")
        .any_of("dependent", "shared.dll", &["cand.a", "cand.b"], true);
    let current = builder.insertion_order();

    assert!(problems_in(&builder.build(), &current).is_empty());
}

#[test]
fn an_any_of_constraint_on_a_mod_the_order_omits_is_not_reported() {
    let report = ReportBuilder::new()
        .mod_("cand.a")
        .mod_("dependent")
        .any_of("dependent", "shared.dll", &["cand.a"], true)
        .build();

    assert!(problems_in(&report, &order(&["cand.a"])).is_empty());
}

#[test]
fn a_problem_whose_finding_the_user_decided_is_acknowledged() {
    let report = ReportBuilder::new()
        .mod_("app")
        .mod_("x")
        .mod_("y")
        .missing_mod("gone")
        .incompatible_pair("x", "y")
        .missing_dependency("app", "lib", None)
        .build();
    let current = order(&["app", "x", "y"]);
    let decisions = decisions_on(vec![
        FindingKey::MissingMod { mod_id: id("gone") },
        FindingKey::IncompatiblePair {
            pair: (id("x"), id("y")),
        },
        FindingKey::MissingDependency {
            mod_id: id("app"),
            dependency: id("lib"),
        },
    ]);
    let (order, ledger) = written(&report, &current, OrderSource::Current, &decisions);

    let items = hard_problems(&report, &order, &ledger);

    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|item| item.acknowledged));
}

#[test]
fn an_undecided_problem_stays_unacknowledged_beside_a_decided_one() {
    let report = ReportBuilder::new()
        .mod_("a")
        .missing_mod("gone")
        .missing_mod("other")
        .build();
    let current = order(&["a"]);
    let decisions = decisions_on(vec![FindingKey::MissingMod { mod_id: id("gone") }]);
    let (order, ledger) = written(&report, &current, OrderSource::Current, &decisions);

    let items = hard_problems(&report, &order, &ledger);

    let acknowledged: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.problem {
            HardProblem::MissingMod { mod_id, .. } => Some((mod_id.clone(), item.acknowledged)),
            _ => None,
        })
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(acknowledged, vec![(id("gone"), true), (id("other"), false)]);
}

#[test]
fn a_dropped_hard_edge_the_user_decided_is_acknowledged() {
    // Two hard edges that contradict each other: the sorter must drop one,
    // so the suggested order violates it and the ledger has its finding.
    let builder = ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .hard_edge("a", "b")
        .hard_edge("b", "a");
    let current = builder.insertion_order();
    let report = builder.build();
    let (_, undecided) = written(
        &report,
        &current,
        OrderSource::Suggested,
        &DecisionSet::new(),
    );
    let dropped = undecided
        .entries
        .iter()
        .find(|entry| matches!(entry.key, FindingKey::EdgeDropped { .. }))
        .expect("the cycle must drop one edge")
        .key
        .clone();

    let decisions = decisions_on(vec![dropped]);
    let (order, ledger) = written(&report, &current, OrderSource::Suggested, &decisions);
    let items = hard_problems(&report, &order, &ledger);

    assert!(!items.is_empty());
    assert!(items.iter().all(|item| item.acknowledged));
}

#[test]
fn a_violated_edge_with_no_finding_is_never_acknowledged() {
    let builder = ReportBuilder::new().mod_("b").mod_("a").hard_edge("b", "a");
    let current = builder.insertion_order();

    let items = problems_of(&builder.build(), &current, OrderSource::Current);

    assert_eq!(items.len(), 1);
    assert!(!items[0].acknowledged);
}

#[test]
fn an_unsatisfied_any_of_is_never_acknowledged_even_with_a_choice_decided() {
    let builder = ReportBuilder::new()
        .mod_("dependent")
        .mod_("cand.a")
        .any_of("dependent", "shared.dll", &["cand.a"], true);
    let current = builder.insertion_order();
    let report = builder.build();
    let decisions = decisions_on(vec![FindingKey::AnyOfChoice {
        after: id("dependent"),
        assembly: "shared.dll".to_string(),
    }]);
    let (order, ledger) = written(&report, &current, OrderSource::Current, &decisions);

    let items = hard_problems(&report, &order, &ledger);

    assert_eq!(items.len(), 1);
    assert!(!items[0].acknowledged);
}

#[test]
fn hard_problems_output_is_sorted_and_identical_across_runs() {
    let forward = ReportBuilder::new()
        .mod_("app")
        .mod_("b")
        .mod_("a")
        .mod_("x")
        .mod_("y")
        .missing_mod("gone")
        .missing_mod("also.gone")
        .incompatible_pair("x", "y")
        .missing_dependency("app", "lib", None)
        .hard_edge("b", "a");
    let reversed = ReportBuilder::new()
        .mod_("app")
        .mod_("b")
        .mod_("a")
        .mod_("x")
        .mod_("y")
        .hard_edge("b", "a")
        .missing_dependency("app", "lib", None)
        .incompatible_pair("y", "x")
        .missing_mod("also.gone")
        .missing_mod("gone");
    let current = order(&["app", "b", "a", "x", "y"]);

    let first = problems_of(&forward.build(), &current, OrderSource::Current);
    let second = problems_of(&reversed.build(), &current, OrderSource::Current);

    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted);
    assert_eq!(first, second);
    assert_eq!(first.len(), 5);
    assert!(matches!(
        first[0].problem,
        HardProblem::MissingDependency { .. }
    ));
}

#[test]
fn an_unsatisfied_lazy_any_of_constraint_is_not_a_hard_problem() {
    let builder = ReportBuilder::new()
        .mod_("dependent")
        .mod_("cand.a")
        .any_of("dependent", "shared.dll", &["cand.a", "cand.b"], false);
    let current = builder.insertion_order();

    assert!(problems_in(&builder.build(), &current).is_empty());
}
