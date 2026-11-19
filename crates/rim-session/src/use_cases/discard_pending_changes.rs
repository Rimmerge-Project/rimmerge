//! [`DiscardPendingChanges`]: throws away the working set's unscanned edits.

use crate::Session;
use crate::active_set::ActiveSet;

/// Resets [`Session::working`] to the scanned Current order.
pub struct DiscardPendingChanges;

impl DiscardPendingChanges {
    /// `working := Current`: every activation, deactivation and move not yet
    /// rescanned is undone. No rescan is needed, because Current is what the
    /// last scan saw. A settled reorder lives in Current itself, so it is
    /// not discarded here; that is [`crate::use_cases::RevertOrderToFile`].
    pub fn execute(session: &mut Session) {
        let current = session.orders().current.as_slice().to_vec();
        *session.working_mut() = ActiveSet::trusted(current);
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;
    use crate::active_set::Anchor;
    use crate::test_support::session_fixture_with_inactive;
    use crate::use_cases::{ActivateMods, MoveMods};

    #[test]
    fn discarding_resets_working_to_current_without_touching_current() {
        let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
        let plan = ActivateMods::plan(&session, &[ModId::new("x")], false);
        ActivateMods::execute(&mut session, &plan).expect("x is known");
        MoveMods::execute(&mut session, &[ModId::new("b")], &Anchor::Start).expect("valid");
        assert!(session.is_stale());

        DiscardPendingChanges::execute(&mut session);

        assert_eq!(
            session.working().ids(),
            [ModId::new("a"), ModId::new("b")],
            "Current's sequence, not the pre-move one"
        );
        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("a"), ModId::new("b")]
        );
        assert!(!session.is_stale());
    }

    #[test]
    fn discarding_keeps_a_settled_reorder() {
        let mut session = session_fixture_with_inactive(&["a", "b"], &[]);
        MoveMods::execute(&mut session, &[ModId::new("b")], &Anchor::Start).expect("valid");

        DiscardPendingChanges::execute(&mut session);

        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("b"), ModId::new("a")],
            "a settled reorder is Current, which Revert (not Discard) undoes"
        );
        assert_eq!(session.working().ids(), [ModId::new("b"), ModId::new("a")]);
    }
}
