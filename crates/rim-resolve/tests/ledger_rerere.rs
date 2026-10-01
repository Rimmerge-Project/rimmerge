//! Rerere behavior: a decision re-applies after an unrelated list change
//! (the finding's key is unaffected), and orphans when the owner set
//! behind an owner-set-keyed finding changes (the key itself changes, so
//! the old decision no longer matches anything live).

use std::collections::BTreeSet;

use rim_analyzer::domain::{Conflict, DefOverride, LoadOrder, Mod, ModId};
use rim_resolve::domain::{
    Action, Confidence, Decision, DecisionSet, DefKey, FindingKey, OrderSource, ResolutionStatus,
    RuleSet, SorterOverrides, Tagging,
};
use rim_resolve::test_support::ReportBuilder;

fn threshold(percent: u8) -> Confidence {
    Confidence::new(percent).unwrap_or_else(|_| unreachable!())
}

fn build_ledger_for(
    report: &rim_analyzer::domain::Report,
    decisions: &DecisionSet,
    current: &LoadOrder,
) -> rim_resolve::domain::Ledger {
    let sort_outcome = rim_resolve::sort::sort(&rim_resolve::sort::SortInput {
        report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current,
        enforce: rim_resolve::sort::EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    });
    let mods_by_id: std::collections::BTreeMap<ModId, &Mod> =
        report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    rim_resolve::ledger::build(&rim_resolve::ledger::BuildLedgerInput {
        report,
        sort_outcome: &sort_outcome,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        decisions,
        threshold: threshold(80),
        source: OrderSource::Current,
        current,
        suggested: &sort_outcome.order,
        mods_by_id: &mods_by_id,
        show_dangling_def_references: false,
    })
}

#[test]
fn a_decision_re_applies_after_an_unrelated_list_change() {
    let builder = ReportBuilder::new().mod_("a").missing_mod("gone.one");
    let current = builder.insertion_order();
    let mut report = builder.build();
    report.missing_mods = vec![ModId::new("gone.one")];

    let key = FindingKey::MissingMod {
        mod_id: ModId::new("gone.one"),
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

    let before = build_ledger_for(&report, &decisions, &current);
    let entry_before = before
        .entries
        .iter()
        .find(|e| e.key == key)
        .expect("present before");
    assert_eq!(entry_before.status, ResolutionStatus::UserOverridden);

    // Unrelated list churn: a second, unrelated missing mod joins.
    report.missing_mods.push(ModId::new("gone.two"));
    let current = LoadOrder::new(vec![ModId::new("a")]);

    let after = build_ledger_for(&report, &decisions, &current);
    let entry_after = after
        .entries
        .iter()
        .find(|e| e.key == key)
        .expect("present after");
    assert_eq!(
        entry_after.status,
        ResolutionStatus::UserOverridden,
        "the decision must still apply: its key never changed"
    );
    assert!(
        after.entries.iter().any(|e| e.key
            == (FindingKey::MissingMod {
                mod_id: ModId::new("gone.two")
            })),
        "the unrelated new finding must also appear"
    );
}

#[test]
fn a_decision_orphans_when_the_owner_set_behind_it_changes() {
    let def_key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };
    let original_owners = [ModId::new("a"), ModId::new("b")];
    let original_finding_key = FindingKey::DefOverride {
        key: def_key.clone(),
        owners: original_owners.iter().cloned().collect(),
    };

    let mut decisions = DecisionSet::new();
    decisions
        .insert(Decision {
            key: original_finding_key.clone(),
            action: Action::PreferWinner {
                key: def_key.clone(),
                winner: ModId::new("b"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .unwrap();

    // A new owner `c` joins the same def override: the finding key's
    // owner set (part of its identity) changes.
    let builder = ReportBuilder::new().mod_("a").mod_("b").mod_("c");
    let current = builder.insertion_order();
    let mut report = builder.build();
    report.conflicts.push(Conflict::DefOverride(DefOverride {
        def_type: def_key.def_type.clone(),
        def_name: def_key.def_name.clone(),
        owners: vec![ModId::new("a"), ModId::new("b"), ModId::new("c")],
        overrides_vanilla: false,
        same_author: false,
    }));

    let ledger = build_ledger_for(&report, &decisions, &current);

    let new_finding_key = FindingKey::DefOverride {
        key: def_key,
        owners: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
            .into_iter()
            .collect(),
    };
    let new_entry = ledger
        .entries
        .iter()
        .find(|e| e.key == new_finding_key)
        .expect("the new three-owner finding must be present");
    assert!(
        new_entry.decision.is_none(),
        "the old two-owner decision must not silently apply to the new three-owner finding"
    );
    assert_ne!(new_entry.status, ResolutionStatus::UserOverridden);

    let live_keys: BTreeSet<FindingKey> = ledger.entries.iter().map(|e| e.key.clone()).collect();
    let orphaned: Vec<&FindingKey> = decisions.orphaned(&live_keys).map(|d| &d.key).collect();
    assert_eq!(
        orphaned,
        vec![&original_finding_key],
        "the old two-owner decision must be listed as orphaned"
    );
}
