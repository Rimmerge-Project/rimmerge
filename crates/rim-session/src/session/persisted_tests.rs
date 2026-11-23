use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    Action, Decision, DecisionSet, FindingKey, OrderSource, PatchId, PatchModIdentity,
    PatchProject, PatchScope, Placement, PlacementRule, Rule, RuleOrigin,
};

use super::*;
use crate::finding_index::FindingFilter;
use crate::ports::StoredRules;
use crate::test_support::{
    InMemoryAssignmentProjectStore, InMemoryDecisionStore, InMemoryPatchProjectStore,
    InMemoryRuleStore, assignment_fixture, patch_fixture, session_fixture,
};

/// The four stores a reload reads, all empty until a test saves into one.
#[derive(Default)]
struct Stores {
    decisions: InMemoryDecisionStore,
    rules: InMemoryRuleStore,
    patches: InMemoryPatchProjectStore,
    assignments: InMemoryAssignmentProjectStore,
}

impl Stores {
    fn borrowed(&self) -> PersistedStores<'_> {
        PersistedStores {
            decisions: &self.decisions,
            rules: &self.rules,
            patches: &self.patches,
            assignments: &self.assignments,
        }
    }

    fn save_top_placement(&self, mod_id: &str) {
        let rules = StoredRules {
            placements: vec![top_placement(mod_id)],
            ..StoredRules::default()
        };
        self.rules.save(dir(), &rules).expect("seed the rules");
    }

    fn save_ignore_decision(&self) {
        let mut set = DecisionSet::new();
        set.insert(ignore_decision().1).expect("valid decision");
        self.decisions
            .save(dir(), &set)
            .expect("seed the decisions");
    }
}

fn dir() -> &'static Path {
    Path::new("profile")
}

fn top_placement(mod_id: &str) -> PlacementRule {
    PlacementRule {
        mod_id: ModId::new(mod_id),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    }
}

fn ignore_decision() -> (FindingKey, Decision) {
    let key = FindingKey::UnsupportedVersion {
        mod_id: ModId::new("a"),
    };
    let decision = Decision {
        key: key.clone(),
        action: Action::Ignore,
        note: None,
        decided_at: jiff::Timestamp::UNIX_EPOCH,
    };
    (key, decision)
}

fn suggested(session: &Session) -> Vec<ModId> {
    session.orders().suggested.as_slice().to_vec()
}

#[test]
fn a_rule_saved_after_the_scan_read_it_shows_in_the_session_and_the_sort() {
    let mut session = session_fixture(&["a", "b"]);
    let before = suggested(&session);
    let stores = Stores::default();
    stores.save_top_placement("b");

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Rules]);
    assert_eq!(session.rules().placements.len(), 1);
    assert_ne!(suggested(&session), before, "the sort must be recomputed");
    assert_eq!(suggested(&session), [ModId::new("b"), ModId::new("a")]);
}

#[test]
fn a_decision_saved_after_the_scan_read_it_shows_in_the_session_and_drops_the_ledger() {
    let mut session = session_fixture(&["a"]);
    session.ensure_ledger(OrderSource::Current);
    let stores = Stores::default();
    stores.save_ignore_decision();

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Decisions]);
    assert!(session.decisions().get(&ignore_decision().0).is_some());
    assert!(
        session.ledgers[0].is_none(),
        "a changed decision must drop the cached ledger"
    );
}

#[test]
fn a_patch_saved_after_the_scan_read_it_appears_and_one_deleted_disappears() {
    let (with_patch, id) = patch_fixture(&["a", "b"], &["a", "b"]);
    let project = with_patch.patch(&id).expect("loaded").clone();
    let stores = Stores::default();
    stores.patches.save(dir(), &project).expect("seed");
    let mut session = session_fixture(&["a", "b"]);

    let changed = session.reload_persisted(stores.borrowed());
    assert_eq!(changed, [PersistedKind::Patches]);
    assert!(session.patch(&id).is_some());
    session
        .patch_findings(&id, OrderSource::Current, &FindingFilter::default())
        .expect("known patch");
    assert!(!session.scoped.is_empty(), "sanity check: cache built");

    stores.patches.delete(dir(), &id).expect("delete");
    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Patches]);
    assert!(session.patch(&id).is_none());
    assert!(
        session.scoped.is_empty(),
        "the removed patch's scoped ledger must go"
    );
}

#[test]
fn an_assignment_saved_after_the_scan_read_it_appears_and_drops_its_cached_coverage() {
    let (with_assignment, id) = assignment_fixture(&["a"], &["a"], &["a"]);
    let project = with_assignment.assignment(&id).expect("loaded").clone();
    let stores = Stores::default();
    stores.assignments.save(dir(), &project).expect("seed");
    let mut session = session_fixture(&["a"]);
    session.cache_assignment_coverage(
        OrderSource::Current,
        id.clone(),
        "ThingDef".to_string(),
        rim_resolve::domain::Coverage::default(),
    );

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Assignments]);
    assert!(session.assignment(&id).is_some());
    assert!(
        session
            .cached_assignment_coverage(OrderSource::Current, &id, "ThingDef")
            .is_none(),
        "coverage reflects the project's rows, so it must go"
    );
}

#[test]
fn nothing_changed_recomputes_nothing() {
    let mut session = session_fixture(&["a", "b"]);
    session.ensure_ledger(OrderSource::Current);
    let stores = Stores::default();

    let changed = session.reload_persisted(stores.borrowed());

    assert!(changed.is_empty());
    assert!(
        session.ledgers[0].is_some(),
        "an unchanged reload must keep the cached ledger"
    );
}

#[test]
fn rules_and_decisions_changing_together_are_both_reported() {
    let mut session = session_fixture(&["a", "b"]);
    let stores = Stores::default();
    stores.save_top_placement("b");
    stores.save_ignore_decision();

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Decisions, PersistedKind::Rules]);
    assert_eq!(suggested(&session), [ModId::new("b"), ModId::new("a")]);
}

#[test]
fn a_rules_file_that_cannot_be_re_read_keeps_the_scanned_rules_and_warns() {
    let mut session = session_fixture(&["a", "b"]);
    session.upsert_rule(Rule::Placement(top_placement("b")));
    let stores = Stores::default();
    stores.rules.fail_next_load();

    let changed = session.reload_persisted(stores.borrowed());

    assert!(changed.is_empty());
    assert_eq!(session.rules().placements.len(), 1, "the scan copy stays");
    assert_eq!(failure_kinds(&session), [PersistedKind::Rules]);
    assert_eq!(session.rule_load_warnings().count(), 1);
}

#[test]
fn each_file_that_cannot_be_re_read_names_itself_and_the_others_still_reload() {
    let mut session = session_fixture(&["a", "b"]);
    let stores = Stores::default();
    stores.decisions.fail_next_load();
    stores.patches.fail_next_load();
    stores.assignments.fail_next_load();
    stores.save_top_placement("b");

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Rules]);
    let failed = failure_kinds(&session);
    assert_eq!(
        failed,
        [
            PersistedKind::Decisions,
            PersistedKind::Patches,
            PersistedKind::Assignments
        ]
    );
}

fn dropped_cluster_warning(rule_id: &str) -> RulesLoadWarning {
    RulesLoadWarning::DroppedClusterRules {
        rule_ids: vec![rule_id.to_string()],
    }
}

fn failure_kinds(session: &Session) -> Vec<PersistedKind> {
    session
        .rule_load_warnings()
        .filter_map(|warning| match warning {
            RulesLoadWarning::PersistedReloadFailed { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect()
}

#[test]
fn a_failed_rules_re_read_keeps_the_scans_rules_file_warnings_and_orders_the_groups() {
    let knowledge = RulesLoadWarning::ModKnowledgeCacheUnreadable("bad".to_string());
    let mut session = session_fixture(&["a"]);
    session.set_knowledge_warnings(vec![knowledge.clone()]);
    session.set_rule_load_warnings(vec![dropped_cluster_warning("old")]);
    let stores = Stores::default();
    stores.rules.fail_next_load();

    session.reload_persisted(stores.borrowed());

    let warnings: Vec<_> = session.rule_load_warnings().collect();
    assert_eq!(warnings.len(), 3);
    assert_eq!(warnings[0], &dropped_cluster_warning("old"));
    assert!(matches!(
        warnings[1],
        RulesLoadWarning::PersistedReloadFailed {
            kind: PersistedKind::Rules,
            ..
        }
    ));
    assert_eq!(warnings[2], &knowledge);
}

#[test]
fn a_successful_rules_re_read_replaces_the_rules_file_warnings_and_clears_old_failures() {
    let knowledge = RulesLoadWarning::ModKnowledgeCacheUnreadable("bad".to_string());
    let mut session = session_fixture(&["a"]);
    session.set_knowledge_warnings(vec![knowledge.clone()]);
    session.set_rule_load_warnings(vec![dropped_cluster_warning("old")]);
    let stores = Stores::default();
    stores.rules.fail_next_load();
    session.reload_persisted(stores.borrowed());
    stores
        .rules
        .warn_on_next_load(vec![dropped_cluster_warning("new")]);

    session.reload_persisted(stores.borrowed());

    assert_eq!(
        session.rule_load_warnings().collect::<Vec<_>>(),
        [&dropped_cluster_warning("new"), &knowledge],
        "the new read's warnings replace the old; the failure is gone; the note stays"
    );
}

fn patch_named(package_id: &str, name: &str, created_at: jiff::Timestamp) -> PatchProject {
    let identity = PatchModIdentity::new(package_id, name).expect("valid identity");
    let id = PatchId::derive(
        "profile",
        identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).expect("two members");
    PatchProject::new(id, name.to_string(), identity, scope, created_at)
}

#[test]
fn a_changed_patch_drops_its_scoped_ledger_and_an_unchanged_one_keeps_it() {
    let epoch = jiff::Timestamp::UNIX_EPOCH;
    let unchanged = patch_named("test.keep", "Keep", epoch);
    let before = patch_named("test.change", "Before", epoch);
    let after = patch_named("test.change", "After", epoch);
    let (kept_id, changed_id) = (unchanged.id().clone(), before.id().clone());
    assert_eq!(changed_id, after.id().clone(), "same id, different content");
    let mut session = session_fixture(&["a", "b"]);
    session.upsert_patch(unchanged.clone());
    session.upsert_patch(before);
    for id in [&kept_id, &changed_id] {
        session
            .patch_findings(id, OrderSource::Current, &FindingFilter::default())
            .expect("known patch");
    }
    let stores = Stores::default();
    stores.patches.save(dir(), &unchanged).expect("seed");
    stores.patches.save(dir(), &after).expect("seed");

    let changed = session.reload_persisted(stores.borrowed());

    assert_eq!(changed, [PersistedKind::Patches]);
    assert!(
        session
            .scoped
            .contains_key(&(kept_id, OrderSource::Current)),
        "an unchanged patch keeps its scoped ledger"
    );
    assert!(
        !session
            .scoped
            .contains_key(&(changed_id, OrderSource::Current)),
        "a changed patch drops its scoped ledger"
    );
}
