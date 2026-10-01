//! [`Decide`]: validates and records a user decision on a finding,
//! persisting it through [`DecisionStore`] before returning — and, when
//! built via [`Decide::with_rule_store`], also persisting an
//! [`Action::PromoteRule`] decision's own `RuleSet` mutation through
//! [`RuleStore`].

use std::path::Path;

use rim_resolve::domain::{Action, Decision, ResolveError};

use crate::Session;
use crate::ports::{DecisionStore, LoadedRules, RuleStore, StoreError, StoredRules};

/// Everything that can go wrong deciding a finding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecideError {
    /// The action failed domain validation. Currently unreachable —
    /// [`ResolveError`] is uninhabited — but kept as a seam for a future
    /// validation rule.
    #[error(transparent)]
    Invalid(#[from] ResolveError),
    /// Persisting the decision (or, for a `PromoteRule` decision, the
    /// promoted rule) failed.
    #[error("saving the decision: {0}")]
    Store(StoreError),
    /// An [`Action::PromoteRule`] decision arrived at a [`Decide`] built
    /// via [`Decide::new`] — no [`RuleStore`] to persist the promotion
    /// through. Rejected before anything changes, rather than promoting
    /// the rule in memory (`Session::decide` would happily do it — see
    /// its own doc comment) with no way to ever save it.
    #[error(
        "promoting a rule requires a Decide built with a rule store (see Decide::with_rule_store)"
    )]
    PromoteNeedsRuleStore,
}

/// [`Decide`]'s default second type parameter: an uninhabited stand-in
/// [`RuleStore`] so [`Decide::new`] keeps compiling for a caller that
/// never decides an [`Action::PromoteRule`] and has no rule store to
/// give it. A `Decide<_, NoRuleStore>` is always built with
/// `rule_store: None` ([`Decide::new`] is the only constructor that
/// produces one), so no method here is ever actually called — the
/// `match *self {}` arms just satisfy the trait for a type that can never
/// hold a value.
#[derive(Debug)]
pub enum NoRuleStore {}

impl RuleStore for NoRuleStore {
    fn load(&self, _dir: &Path) -> Result<LoadedRules, StoreError> {
        match *self {}
    }

    fn save(&self, _dir: &Path, _rules: &StoredRules) -> Result<(), StoreError> {
        match *self {}
    }
}

/// Validates and records a decision, persisting it through
/// [`DecisionStore`] before returning. Built via [`Decide::new`], every
/// action but [`Action::PromoteRule`] works exactly as before — that one
/// needs [`Decide::with_rule_store`] instead, since its real effect is a
/// `RuleSet` mutation (see [`Session::decide`]'s own doc comment) that
/// [`DecisionStore`] alone can never persist.
pub struct Decide<Decisions, Rules = NoRuleStore> {
    decision_store: Decisions,
    rule_store: Option<Rules>,
}

impl<Decisions: DecisionStore> Decide<Decisions, NoRuleStore> {
    /// Builds the use case with no rule store. A [`Decision`] whose action
    /// is [`Action::PromoteRule`] is rejected with
    /// [`DecideError::PromoteNeedsRuleStore`] — see
    /// [`Decide::with_rule_store`] for one that can persist it.
    #[must_use]
    pub fn new(decision_store: Decisions) -> Self {
        Self {
            decision_store,
            rule_store: None,
        }
    }
}

impl<Decisions: DecisionStore, Rules: RuleStore> Decide<Decisions, Rules> {
    /// Builds the use case with both ports, so an [`Action::PromoteRule`]
    /// decision's own `RuleSet` mutation is persisted — and, if either
    /// save fails, rolled back — alongside the decision itself.
    #[must_use]
    pub fn with_rule_store(decision_store: Decisions, rule_store: Rules) -> Self {
        Self {
            decision_store,
            rule_store: Some(rule_store),
        }
    }

    /// Returns whether the decision changed what the sorter builds (a
    /// resort happened).
    ///
    /// # Errors
    ///
    /// Returns [`DecideError::PromoteNeedsRuleStore`] for an
    /// [`Action::PromoteRule`] decision when this `Decide` has no rule
    /// store, before anything changes. Returns [`DecideError::Invalid`]
    /// when [`rim_resolve::domain::DecisionSet::insert`]'s validation
    /// rejects the action (currently unreachable). Returns
    /// [`DecideError::Store`] when persisting fails — in which case the
    /// in-memory decision (and, for a `PromoteRule` decision with a rule
    /// store, the promoted rule too) is rolled back so the session never
    /// runs ahead of what's on disk. [`Action::DropRule`] never touches
    /// the rule set at all (its effect is a `SorterOverrides` entry
    /// derived from the decision itself — see
    /// [`rim_resolve::domain::DecisionSet::sorter_overrides`]), so it
    /// needs no rule store and is unaffected by any of this.
    pub fn execute(&self, session: &mut Session, decision: Decision) -> Result<bool, DecideError> {
        let promotes_a_rule = matches!(decision.action, Action::PromoteRule { .. });
        if promotes_a_rule && self.rule_store.is_none() {
            return Err(DecideError::PromoteNeedsRuleStore);
        }

        let decisions_snapshot = session.decisions_snapshot();
        let rules_snapshot = promotes_a_rule.then(|| session.rules_snapshot());
        let resorted = session.decide(decision)?;

        if let Err(error) = self
            .decision_store
            .save(&session.paths().profile_dir, session.decisions())
        {
            session.restore_decisions(decisions_snapshot);
            if let Some(rules_snapshot) = rules_snapshot {
                session.restore_rules(rules_snapshot);
            }
            return Err(DecideError::Store(error));
        }

        if promotes_a_rule
            && let Some(rule_store) = &self.rule_store
            && let Err(error) = rule_store.save(&session.paths().profile_dir, session.rules())
        {
            session.restore_decisions(decisions_snapshot);
            if let Some(rules_snapshot) = rules_snapshot {
                session.restore_rules(rules_snapshot);
            }
            return Err(DecideError::Store(error));
        }

        Ok(resorted)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, DefKey, FindingKey, PromotedRuleKey, RuleOrigin};

    use super::*;
    use crate::test_support::{InMemoryDecisionStore, InMemoryRuleStore, session_fixture};

    fn imported_pair(after: &str, before: &str) -> rim_resolve::domain::Rule {
        rim_resolve::domain::Rule::Pair(rim_resolve::domain::PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
            overrides_declared: false,
        })
    }

    fn promote_rule_decision(after: &str, before: &str) -> Decision {
        Decision {
            key: FindingKey::RuleOverruled {
                after: ModId::new(after),
                before: ModId::new(before),
                origin: RuleOrigin::RimSortCommunity,
            },
            action: Action::PromoteRule {
                rule: PromotedRuleKey::Pair {
                    after: ModId::new(after),
                    before: ModId::new(before),
                },
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    /// A `Decide` built via `new` (no rule store) must reject a
    /// `PromoteRule` decision before anything changes, rather than
    /// promoting the rule in memory with no way to ever persist it.
    #[test]
    fn promote_rule_without_a_rule_store_is_rejected_before_anything_changes() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let use_case = Decide::new(InMemoryDecisionStore::new());

        let result = use_case.execute(&mut session, promote_rule_decision("a", "b"));

        assert!(matches!(result, Err(DecideError::PromoteNeedsRuleStore)));
        assert_eq!(
            session.rules().pairs.len(),
            1,
            "no promoted copy must be added"
        );
        assert!(
            session
                .decisions()
                .get(&FindingKey::RuleOverruled {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                    origin: RuleOrigin::RimSortCommunity,
                })
                .is_none(),
            "the decision itself must not be recorded either"
        );
    }

    /// Happy path: built via `with_rule_store`, a `PromoteRule` decision
    /// persists both the decision and the promoted rule.
    #[test]
    fn promote_rule_with_a_rule_store_persists_both_the_decision_and_the_promoted_rule() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let decision_store = InMemoryDecisionStore::new();
        let rule_store = InMemoryRuleStore::new();
        let use_case = Decide::with_rule_store(decision_store, rule_store);
        let key = FindingKey::RuleOverruled {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortCommunity,
        };

        let resorted = use_case
            .execute(&mut session, promote_rule_decision("a", "b"))
            .expect("promote with a rule store must succeed");

        assert!(resorted, "a genuine promotion must report true");
        assert!(
            use_case
                .decision_store
                .last_saved()
                .is_some_and(|decisions| decisions.get(&key).is_some()),
            "the decision itself must be persisted"
        );
        let saved_rules = use_case
            .rule_store
            .as_ref()
            .expect("built with_rule_store")
            .last_saved()
            .expect("the promoted rule must be persisted");
        assert_eq!(
            saved_rules.pairs.len(),
            2,
            "the import and its promoted copy must both be saved: {:?}",
            saved_rules.pairs
        );
    }

    /// `fail_next_save` on the rule store leaves neither the decision nor
    /// the promoted copy — both must roll back together, since a
    /// half-applied promotion (decision recorded, rule not persisted, or
    /// vice versa) would run the session ahead of what's actually on
    /// disk.
    #[test]
    fn a_failed_rule_store_save_rolls_back_both_the_decision_and_the_promoted_rule() {
        let mut session = session_fixture(&["a", "b"]);
        session.upsert_rule(imported_pair("a", "b"));
        let rule_store = InMemoryRuleStore::new();
        rule_store.fail_next_save();
        let use_case = Decide::with_rule_store(InMemoryDecisionStore::new(), rule_store);
        let key = FindingKey::RuleOverruled {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortCommunity,
        };

        let result = use_case.execute(&mut session, promote_rule_decision("a", "b"));

        assert!(matches!(result, Err(DecideError::Store(_))));
        assert!(
            session.decisions().get(&key).is_none(),
            "the decision must be rolled back when the rule-store save fails"
        );
        assert_eq!(
            session.rules().pairs.len(),
            1,
            "the promoted copy must be rolled back too, not just the decision"
        );
    }

    #[test]
    fn persists_the_decision_before_returning() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryDecisionStore::new();
        let use_case = Decide::new(store);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };

        let resorted = use_case
            .execute(
                &mut session,
                Decision {
                    key: key.clone(),
                    action: Action::Accept,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("decide must succeed");

        assert!(!resorted);
        assert_eq!(
            use_case
                .decision_store
                .last_saved()
                .and_then(|set| set.get(&key).cloned())
                .map(|d| d.action),
            Some(Action::Accept)
        );
    }

    #[test]
    fn accepts_a_merge_action_and_persists_it() {
        let mut session = session_fixture(&["a"]);
        let use_case = Decide::new(InMemoryDecisionStore::new());
        let key = DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        };

        let finding_key = FindingKey::DefOverride {
            key: key.clone(),
            owners: [ModId::new("a")].into_iter().collect(),
        };
        let result = use_case.execute(
            &mut session,
            Decision {
                key: finding_key.clone(),
                action: Action::Merge {
                    key,
                    choices: std::collections::BTreeMap::new(),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            },
        );

        assert!(result.is_ok());
        assert!(
            session.decisions().get(&finding_key).is_some(),
            "an accepted merge decision must be recorded on the session"
        );
        assert!(
            use_case.decision_store.last_saved().is_some(),
            "an accepted merge decision must reach the store"
        );
    }

    #[test]
    fn a_failed_save_rolls_back_the_decision() {
        let mut session = session_fixture(&["a"]);
        let store = InMemoryDecisionStore::new();
        store.fail_next_save();
        let use_case = Decide::new(store);
        let key = FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        };

        let result = use_case.execute(
            &mut session,
            Decision {
                key: key.clone(),
                action: Action::Accept,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            },
        );

        assert!(matches!(result, Err(DecideError::Store(_))));
        assert!(
            session.decisions().get(&key).is_none(),
            "the decision must be rolled back in memory when the save fails"
        );
    }
}
