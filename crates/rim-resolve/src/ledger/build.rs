//! Assembles a [`Ledger`] for one [`OrderSource`] from a [`Report`], the
//! sorter's [`SortOutcome`], the current [`Tagging`] and [`DecisionSet`],
//! and a confidence threshold.

use std::collections::BTreeMap;

use rim_analyzer::domain::{EdgeStatus, LoadOrder, Mod, ModId, Report};

use super::findings;
use super::suggest::{self, SuggestContext};
use crate::domain::{
    Confidence, DecisionSet, FindingKey, Ledger, LedgerStats, OrderSource, Resolution,
    ResolutionStatus, RuleSet, Tagging,
};
use crate::evaluate;
use crate::sort::SortOutcome;

/// Everything [`build`] needs: the analysis facts, the sorter's outcome,
/// current tagging/decisions, the confidence threshold, which
/// [`OrderSource`] to build for, both orders (see [`build`]'s doc comment
/// for why both are always required), and per-mod facts.
///
/// Every field is a reference or a `Copy` value, so this itself is `Copy`
/// — passing it by value never clones the data it points to.
#[derive(Clone, Copy)]
pub struct BuildLedgerInput<'a> {
    /// The full analyzer report.
    pub report: &'a Report,
    /// The sorter's outcome — assumed to be the very outcome that produced
    /// `suggested` (its `any_of_choices`/`dropped` are read as facts about
    /// `suggested` specifically).
    pub sort_outcome: &'a SortOutcome,
    /// The same rule set that fed the sorter to produce `sort_outcome`
    /// (`SortInput::rules`): placement findings read a mod's own placement
    /// rule back out of it to name
    /// the origin an overruled/questioned placement's finding key needs.
    pub rules: &'a RuleSet,
    /// Current tag assignments.
    pub tagging: &'a Tagging,
    /// Every decision on file.
    pub decisions: &'a DecisionSet,
    /// The confidence threshold separating `Auto` from `NeedsInput`.
    pub threshold: Confidence,
    /// Which order to build the ledger for.
    pub source: OrderSource,
    /// The order currently active in `ModsConfig.xml`.
    pub current: &'a LoadOrder,
    /// The order the sorter proposes.
    pub suggested: &'a LoadOrder,
    /// Every active mod, by id.
    pub mods_by_id: &'a BTreeMap<ModId, &'a Mod>,
    /// Mirrors `rim_session::Settings::show_dangling_def_references` —
    /// whether `Conflict::DanglingDefReference` entries in `report`
    /// become findings at all. Off by default (see that setting's own
    /// doc comment for the measured false-positive rate); this finding
    /// carries no ordering edge, so the flag never reaches the sorter.
    pub show_dangling_def_references: bool,
}

/// Builds the ledger for [`BuildLedgerInput::source`].
///
/// Both [`BuildLedgerInput::current`] and [`BuildLedgerInput::suggested`]
/// are needed regardless of `source`: [`Resolution::resolved_by_suggested`]
/// (populated only when `source` is [`OrderSource::Current`]) asks whether
/// the *other* order already fixes each finding.
#[must_use]
pub fn build(input: &BuildLedgerInput<'_>) -> Ledger {
    let BuildLedgerInput {
        report,
        sort_outcome,
        rules,
        tagging,
        decisions,
        threshold,
        source,
        current,
        suggested,
        mods_by_id,
        show_dangling_def_references,
    } = *input;

    let selected_order = match source {
        OrderSource::Current => current,
        OrderSource::Suggested => suggested,
    };
    let all_findings = findings::extract(
        report,
        sort_outcome,
        tagging,
        selected_order,
        rules,
        show_dangling_def_references,
    );
    // The any-of rule's "already before `after` today" preference is about
    // the user's *actual* current order specifically, regardless of which
    // order this ledger is being built for — `current`, never
    // `selected_order` (see `SuggestContext::current`'s own doc comment).
    let ctx = SuggestContext {
        report,
        sort_outcome,
        current,
        mods_by_id,
    };

    let mut entries = Vec::with_capacity(all_findings.len());
    let mut stats = LedgerStats::default();

    for (key, finding) in &all_findings {
        let suggestion = suggest::suggest(finding, &ctx);
        let decision = decisions.get(key).cloned();
        let status = match &decision {
            Some(_) => ResolutionStatus::UserOverridden,
            None if suggestion.confidence.meets(threshold) => ResolutionStatus::Auto,
            None => ResolutionStatus::NeedsInput,
        };
        let effective = decision
            .as_ref()
            .map_or_else(|| suggestion.action.clone(), |d| d.action.clone());
        let resolved_by_suggested = match source {
            OrderSource::Current => Some(resolved_under_suggested(key, sort_outcome, suggested)),
            OrderSource::Suggested => None,
        };

        match status {
            ResolutionStatus::Auto => stats.auto += 1,
            ResolutionStatus::NeedsInput => stats.needs_input += 1,
            ResolutionStatus::UserOverridden => stats.overridden += 1,
        }
        if resolved_by_suggested == Some(true) {
            stats.resolved_by_suggested += 1;
        }

        entries.push(Resolution {
            key: key.clone(),
            finding: finding.clone(),
            suggestion,
            status,
            effective,
            decision,
            resolved_by_suggested,
            // This crate has no XML to compute a merge preview from —
            // `rim-session` fills this in from its own preview cache
            // after building the ledger (see `Resolution::merge`'s doc
            // comment).
            merge: None,
            structural_guard_field: None,
            // The profile ledger has no scope; only `ledger::scoped`
            // populates this.
            scope: None,
        });
    }

    Ledger {
        source,
        entries,
        stats,
    }
}

/// Whether `suggested` already fixes `key`'s underlying ordering
/// violation. Only order-shaped finding kinds can ever be "fixed" by a
/// different order; everything else (a static content conflict, a mod
/// missing from disk, ...) is unaffected by load order and always `false`.
///
/// An advisory (unenforced `Soft`/`Awareness`, see
/// [`crate::sort::EnforcedLayers`]) engine edge can never make this `true`
/// by construction, not because of a check here: it's never added to the
/// sort graph, so it's never a candidate for [`SortOutcome::dropped`]
/// either, so [`super::findings::extract`] never produces a
/// `FindingKey::EdgeDropped` for it in the first place — there's no key
/// this function could ever be asked about for such an edge. See
/// `ledger::findings::tests::an_advisory_edge_never_becomes_an_edge_dropped_finding`.
fn resolved_under_suggested(
    key: &FindingKey,
    sort_outcome: &SortOutcome,
    suggested: &LoadOrder,
) -> bool {
    match key {
        FindingKey::EdgeDropped { after, before, .. }
        | FindingKey::UndeclaredHardDependency { after, before }
        | FindingKey::LazyReferenceViolated { after, before } => {
            matches!(
                evaluate::ordering_status(after, before, suggested),
                EdgeStatus::Satisfied
            )
        }
        FindingKey::AnyOfChoice { after, assembly } => sort_outcome
            .any_of_choices
            .iter()
            .find(|c| c.after == *after && c.assembly == *assembly)
            .is_none_or(|choice| choice.chosen.is_some()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::EdgeKind;

    use super::*;
    use crate::domain::{Action, Decision, RuleSet, SorterOverrides};

    /// A `Soft` edge (advisory by default, so the sorter never enforces it
    /// directly) that the *current* order violates, but that a separate
    /// `Declared` edge happens to satisfy in the *suggested* order anyway —
    /// so the two ledgers must disagree about whether it's live. Proves
    /// `findings::extract` is evaluated against `selected_order`, not
    /// unconditionally against `sort_outcome.order` (always suggested):
    /// with the old, unconditional behavior, the `Current` ledger would
    /// never see this violation at all.
    #[test]
    fn lazy_reference_violated_is_evaluated_against_the_selected_order() {
        let builder = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .soft_edge("a", "b") // a after b
            .declared_edge("a", "b"); // a after b too, but enforced
        let current = builder.insertion_order(); // [a, b]: violates "a after b"
        let report = builder.build();
        let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: crate::sort::EnforcedLayers::default(),
            tie_break: crate::sort::TieBreak::PreserveCurrent,
        });
        // The Declared edge is enforced regardless, so the suggested order
        // satisfies both edges even though Soft itself stayed advisory.
        assert_eq!(sort_outcome.order.position(&ModId::new("b")), Some(0));
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();
        let key = FindingKey::LazyReferenceViolated {
            after: ModId::new("a"),
            before: ModId::new("b"),
        };

        let current_ledger = build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });
        assert!(
            current_ledger.entries.iter().any(|e| e.key == key),
            "the Current ledger must see the violation the current order actually has"
        );

        let suggested_ledger = build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
            source: OrderSource::Suggested,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });
        assert!(
            !suggested_ledger.entries.iter().any(|e| e.key == key),
            "the Suggested ledger must not see a violation the suggested order doesn't have"
        );
    }

    /// `MissingMod` always suggests confidence 85 (see
    /// `suggest::missing_mod`) — a fixed, known value lets these tests pin
    /// the threshold at, just above, and just below it and assert
    /// `build()`'s own real status derivation, rather than reimplementing
    /// its `confidence.meets(threshold)` branch inline against a bare
    /// `Suggestion`.
    fn missing_mod_ledger(threshold_percent: u8) -> Ledger {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("gone")
            .build();
        let current = LoadOrder::new(vec![ModId::new("a")]);
        let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: crate::sort::EnforcedLayers::default(),
            tie_break: crate::sort::TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();
        build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(threshold_percent),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        })
    }

    fn missing_mod_status(ledger: &Ledger) -> ResolutionStatus {
        ledger
            .entries
            .iter()
            .find(|e| {
                e.key
                    == FindingKey::MissingMod {
                        mod_id: ModId::new("gone"),
                    }
            })
            .expect("missing_mod finding must be present")
            .status
    }

    #[test]
    fn status_is_auto_at_exactly_the_threshold() {
        // `Confidence::meets` is inclusive of the threshold: 85 meeting 85
        // must resolve `Auto`, not `NeedsInput`.
        assert_eq!(
            missing_mod_status(&missing_mod_ledger(85)),
            ResolutionStatus::Auto
        );
    }

    #[test]
    fn status_is_needs_input_just_above_the_threshold() {
        assert_eq!(
            missing_mod_status(&missing_mod_ledger(86)),
            ResolutionStatus::NeedsInput
        );
    }

    fn threshold(percent: u8) -> Confidence {
        Confidence::new(percent).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn missing_mod_is_auto_at_the_default_threshold() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("gone")
            .build();
        let current = LoadOrder::new(vec![ModId::new("a")]);
        let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: crate::sort::EnforcedLayers::default(),
            tie_break: crate::sort::TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();

        let ledger = build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });

        let entry = ledger
            .entries
            .iter()
            .find(|e| {
                e.key
                    == FindingKey::MissingMod {
                        mod_id: ModId::new("gone"),
                    }
            })
            .expect("missing_mod finding must be present");
        assert_eq!(entry.status, ResolutionStatus::Auto);
        assert_eq!(ledger.stats.auto, 1);
    }

    #[test]
    fn a_decision_makes_the_entry_user_overridden_regardless_of_confidence() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("gone")
            .build();
        let current = LoadOrder::new(vec![ModId::new("a")]);
        let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: crate::sort::EnforcedLayers::default(),
            tie_break: crate::sort::TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("gone"),
        };
        let mut decisions = DecisionSet::new();
        decisions
            .insert(Decision {
                key: key.clone(),
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .unwrap();

        let ledger = build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &decisions,
            threshold: threshold(80),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });

        let entry = ledger
            .entries
            .iter()
            .find(|e| e.key == key)
            .expect("present");
        assert_eq!(entry.status, ResolutionStatus::UserOverridden);
        assert_eq!(entry.effective, Action::Ignore);
        assert_eq!(ledger.stats.overridden, 1);
    }

    #[test]
    fn resolved_by_suggested_is_none_when_building_for_suggested() {
        let report = crate::test_support::ReportBuilder::new().mod_("a").build();
        let current = LoadOrder::new(vec![ModId::new("a")]);
        let sort_outcome = crate::sort::sort(&crate::sort::SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: crate::sort::EnforcedLayers::default(),
            tie_break: crate::sort::TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();

        let ledger = build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
            source: OrderSource::Suggested,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });

        assert!(
            ledger
                .entries
                .iter()
                .all(|e| e.resolved_by_suggested.is_none())
        );
    }

    /// The suggested order drops a hard edge to break a cycle, but the
    /// *current* order — with the very edge the sorter had to sacrifice —
    /// still leaves that edge violated. Reordering `b` after `a` in
    /// `suggested` (simulated directly here, since building a genuine
    /// cycle would also need a matching drop) demonstrates
    /// `resolved_by_suggested` flipping true once the suggested order
    /// actually satisfies the edge.
    #[test]
    fn resolved_by_suggested_is_true_when_the_suggested_order_satisfies_the_dropped_edge() {
        let suggested = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
        let sort_outcome = crate::sort::SortOutcome {
            order: LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]),
            placements: BTreeMap::new(),
            dropped: vec![crate::sort::DroppedEdge {
                edge: crate::sort::OrderingEdge {
                    after: ModId::new("b"),
                    before: ModId::new("a"),
                    layer: crate::sort::Layer::Declared,
                    provenance: crate::sort::EdgeProvenance::Engine {
                        kind: EdgeKind::LoadAfter,
                        detail: String::new(),
                    },
                },
                witness_cycle: Vec::new(),
                winner: None,
            }],
            any_of_choices: Vec::new(),
            warnings: Vec::new(),
            stats: crate::sort::DisturbanceStats::default(),
        };

        let resolved = resolved_under_suggested(
            &FindingKey::EdgeDropped {
                after: ModId::new("b"),
                before: ModId::new("a"),
                kind: EdgeKind::LoadAfter,
            },
            &sort_outcome,
            &suggested,
        );

        assert!(resolved, "suggested already places `a` before `b`");
    }
}
