//! [`PreflightApply`]: lists the hard problems in the order
//! [`crate::use_cases::Apply`] would write, so an interface can show and
//! confirm them first. Read-only — no ports.

use rim_resolve::domain::OrderSource;
use rim_resolve::preflight::PreflightItem;

use crate::Session;

/// The hard problems in one order, ready for an interface to present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPreflight {
    /// The order the problems were found in.
    pub source: OrderSource,
    /// The problems, sorted (see [`rim_resolve::preflight::hard_problems`]).
    pub items: Vec<PreflightItem>,
}

impl ApplyPreflight {
    /// Whether the user must confirm before applying: some problem is
    /// still unanswered. Derived, never stored — a list whose every
    /// problem was decided in the Inbox asks nothing.
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        self.items.iter().any(|item| !item.acknowledged)
    }
}

/// Finds the hard problems in `source`'s order.
#[derive(Debug, Default, Clone, Copy)]
pub struct PreflightApply;

impl PreflightApply {
    /// Builds the use case. Takes no ports: it reads the session only.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The hard problems in `source`'s order, as `Apply` would write it.
    ///
    /// `&mut Session` only because the session builds its ledger cache
    /// lazily, as every other ledger read does.
    #[must_use]
    pub fn execute(self, session: &mut Session, source: OrderSource) -> ApplyPreflight {
        ApplyPreflight {
            source,
            items: session.hard_problems(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, Decision, FindingKey};
    use rim_resolve::preflight::{HardProblem, MissingModOutcome};
    use rim_resolve::test_support::ReportBuilder;

    use super::*;
    use crate::test_support::{session_fixture, session_with_sources_and_mods};

    fn session_with_missing_mods(missing: &[&str]) -> Session {
        let mut builder = ReportBuilder::new().mod_("a");
        for id in missing {
            builder = builder.missing_mod(id);
        }
        session_with_sources_and_mods(SourceIndex::default(), builder.build(), &["a"])
    }

    fn accept(session: &mut Session, mod_id: &str) {
        session
            .decide(Decision {
                key: FindingKey::MissingMod {
                    mod_id: ModId::new(mod_id),
                },
                action: Action::Accept,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("Accept is a valid action");
    }

    #[test]
    fn a_session_with_no_problems_requires_no_confirmation() {
        let mut session = session_fixture(&["a", "b"]);

        let preflight = PreflightApply::new().execute(&mut session, OrderSource::Suggested);

        assert_eq!(preflight.source, OrderSource::Suggested);
        assert!(preflight.items.is_empty());
        assert!(!preflight.requires_confirmation());
    }

    #[test]
    fn the_suggested_order_omits_a_missing_mod_and_says_it_is_removed() {
        let mut session = session_with_missing_mods(&["gone"]);

        let preflight = PreflightApply::new().execute(&mut session, OrderSource::Suggested);

        assert_eq!(
            preflight
                .items
                .iter()
                .map(|item| &item.problem)
                .collect::<Vec<_>>(),
            vec![&HardProblem::MissingMod {
                mod_id: ModId::new("gone"),
                outcome: MissingModOutcome::RemovedFromActiveList,
            }]
        );
        assert!(preflight.requires_confirmation());
    }

    #[test]
    fn preflight_requires_confirmation_only_with_an_unacknowledged_problem() {
        let mut session = session_with_missing_mods(&["gone", "other"]);
        accept(&mut session, "gone");

        let preflight = PreflightApply::new().execute(&mut session, OrderSource::Suggested);

        let acknowledged: Vec<bool> = preflight
            .items
            .iter()
            .map(|item| item.acknowledged)
            .collect();
        assert_eq!(preflight.items.len(), 2);
        assert!(acknowledged.contains(&true) && acknowledged.contains(&false));
        assert!(preflight.requires_confirmation());
    }

    #[test]
    fn preflight_with_every_problem_decided_requires_none() {
        let mut session = session_with_missing_mods(&["gone"]);
        accept(&mut session, "gone");

        let preflight = PreflightApply::new().execute(&mut session, OrderSource::Suggested);

        assert_eq!(preflight.items.len(), 1);
        assert!(preflight.items[0].acknowledged);
        assert!(!preflight.requires_confirmation());
    }
}
