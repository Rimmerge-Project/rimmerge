//! [`MoveMods`]: reorders mods inside the working active set. No port and no
//! persistence: only [`crate::use_cases::Apply`] writes `ModsConfig.xml`.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::active_set::{Anchor, MoveError};

/// What a successful [`MoveMods::execute`] did beyond the move itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveOutcome {
    /// The move was the user's first reorder while Suggested was selected,
    /// so the selection is now Current: a manual order means the user
    /// intends their own, and Apply writes the selected order. The caller
    /// tells the user, since Suggested itself is unchanged.
    pub did_switch_to_current: bool,
}

/// Moves mods within [`Session::working`].
pub struct MoveMods;

impl MoveMods {
    /// Moves `ids` (kept together, in their current relative order) next to
    /// `anchor`. When the working set then holds the same ids as Current,
    /// Current takes its sequence (`Session::settle_current_onto_working`):
    /// nothing is written and the file keeps what it held. With pending
    /// membership edits the move stays in the working set, and the next
    /// rescan carries it. A move that changes nothing (the mods are already
    /// there) switches no selection.
    ///
    /// # Errors
    ///
    /// See [`MoveError`]; the session is untouched on an error.
    pub fn execute(
        session: &mut Session,
        ids: &[ModId],
        anchor: &Anchor,
    ) -> Result<MoveOutcome, MoveError> {
        let before = session.working().ids().to_vec();
        session.working_mut().move_to(ids, anchor)?;
        if session.working().ids() == before.as_slice() {
            return Ok(MoveOutcome {
                did_switch_to_current: false,
            });
        }
        session.settle_current_onto_working();
        Ok(MoveOutcome {
            did_switch_to_current: session.switch_to_current_after_reorder(),
        })
    }
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::OrderSource;
    use rim_resolve::sort::TieBreak;

    use super::*;
    use crate::settings::Settings;
    use crate::test_support::{session_fixture, session_fixture_with_inactive};
    use crate::use_cases::{ActivateMods, DeactivateMods};

    fn mods(ids: &[&str]) -> Vec<ModId> {
        ids.iter().map(|id| ModId::new(*id)).collect()
    }

    fn current_of(session: &Session) -> Vec<&str> {
        session
            .orders()
            .current
            .as_slice()
            .iter()
            .map(ModId::as_str)
            .collect()
    }

    fn working_of(session: &Session) -> Vec<&str> {
        session.working().ids().iter().map(ModId::as_str).collect()
    }

    fn file_of(session: &Session) -> Vec<&str> {
        session
            .file_active_mods()
            .iter()
            .map(ModId::as_str)
            .collect()
    }

    #[test]
    fn a_move_over_equal_membership_settles_into_current_and_leaves_the_file_alone() {
        let mut session = session_fixture(&["a", "b", "c"]);

        MoveMods::execute(&mut session, &mods(&["c"]), &Anchor::Start).expect("valid move");

        assert_eq!(current_of(&session), ["c", "a", "b"]);
        assert_eq!(working_of(&session), ["c", "a", "b"]);
        assert_eq!(file_of(&session), ["a", "b", "c"], "nothing was written");
        assert!(!session.is_stale(), "a settled move is never stale");
        assert!(
            !session.file_matches(OrderSource::Current),
            "the moved order is not in the file until an Apply"
        );
    }

    #[test]
    fn a_move_with_pending_membership_stays_in_working_only() {
        let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
        let plan = ActivateMods::plan(&session, &mods(&["x"]), false);
        ActivateMods::execute(&mut session, &plan).expect("x is known");

        MoveMods::execute(&mut session, &mods(&["b"]), &Anchor::Start).expect("valid move");

        assert_eq!(working_of(&session), ["b", "a", "x"]);
        assert_eq!(current_of(&session), ["a", "b"], "Current is untouched");
        assert!(
            session.is_stale(),
            "the pending activation is still pending"
        );
    }

    #[test]
    fn a_refused_move_changes_nothing() {
        let mut session = session_fixture(&["a", "b"]);
        session.select(OrderSource::Suggested);

        let error = MoveMods::execute(
            &mut session,
            &mods(&["a"]),
            &Anchor::After(ModId::new("gone")),
        )
        .expect_err("gone is not active");

        assert_eq!(error, MoveError::AnchorNotActive(ModId::new("gone")));
        assert_eq!(current_of(&session), ["a", "b"]);
        assert_eq!(session.selected(), OrderSource::Suggested);
    }

    #[test]
    fn the_first_reorder_while_suggested_is_selected_switches_to_current() {
        let mut session = session_fixture(&["a", "b", "c"]);
        session.select(OrderSource::Suggested);

        let first = MoveMods::execute(&mut session, &mods(&["c"]), &Anchor::Start).expect("valid");
        let second = MoveMods::execute(&mut session, &mods(&["b"]), &Anchor::Start).expect("valid");

        assert!(first.did_switch_to_current);
        assert!(!second.did_switch_to_current, "already on Current");
        assert_eq!(session.selected(), OrderSource::Current);
    }

    #[test]
    fn a_move_that_changes_nothing_keeps_the_selection() {
        let mut session = session_fixture(&["a", "b", "c"]);
        session.select(OrderSource::Suggested);

        let outcome =
            MoveMods::execute(&mut session, &mods(&["a"]), &Anchor::Start).expect("valid");

        assert!(!outcome.did_switch_to_current);
        assert_eq!(session.selected(), OrderSource::Suggested);
    }

    #[test]
    fn a_settled_move_re_sorts_under_preserve_current() {
        let mut session = session_fixture(&["z", "a"]);
        session.update_settings(Settings {
            tie_break: TieBreak::PreserveCurrent,
            ..session.settings()
        });
        assert_eq!(session.orders().suggested.as_slice(), mods(&["z", "a"]));

        MoveMods::execute(&mut session, &mods(&["z"]), &Anchor::End).expect("valid");

        assert_eq!(
            session.orders().suggested.as_slice(),
            mods(&["a", "z"]),
            "PreserveCurrent ranks by Current's sequence, which just changed"
        );
    }

    #[test]
    fn a_settled_move_does_not_re_sort_under_rebuild() {
        let mut session = session_fixture(&["z", "a"]);
        let suggested_before = session.orders().suggested.as_slice().to_vec();

        MoveMods::execute(&mut session, &mods(&["z"]), &Anchor::End).expect("valid");

        assert_eq!(session.orders().suggested.as_slice(), suggested_before);
    }

    #[test]
    fn set_current_order_still_makes_working_take_the_files_order() {
        let mut session = session_fixture(&["a", "b", "c"]);
        MoveMods::execute(&mut session, &mods(&["c"]), &Anchor::Start).expect("valid");

        session.set_current_order(rim_analyzer::domain::LoadOrder::new(mods(&["b", "a", "c"])));

        assert_eq!(working_of(&session), ["b", "a", "c"]);
        assert_eq!(current_of(&session), ["b", "a", "c"]);
    }

    #[test]
    fn deactivating_then_reactivating_without_an_anchor_restores_every_slot() {
        let mut session = session_fixture(&["a", "b", "c", "d"]);
        let removal = DeactivateMods::plan(&session, &mods(&["b", "c"]));
        DeactivateMods::execute(&mut session, &removal);
        assert_eq!(working_of(&session), ["a", "d"]);

        // The plan lists c before b: placement follows Current, not arrival.
        let addition = ActivateMods::plan(&session, &mods(&["c", "b"]), false);
        ActivateMods::execute(&mut session, &addition).expect("known");

        assert_eq!(working_of(&session), ["a", "b", "c", "d"]);
        assert_eq!(current_of(&session), ["a", "b", "c", "d"]);
        assert!(!session.is_stale());
    }

    #[test]
    fn an_anchored_activation_over_equal_membership_settles_into_current() {
        let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
        let removal = DeactivateMods::plan(&session, &mods(&["b"]));
        DeactivateMods::execute(&mut session, &removal);
        let addition = ActivateMods::plan(&session, &mods(&["b"]), false);

        ActivateMods::execute_at(&mut session, &addition, &Anchor::Start).expect("valid");

        assert_eq!(current_of(&session), ["b", "a"]);
        assert_eq!(file_of(&session), ["a", "b"]);
    }

    #[test]
    fn an_anchored_activation_with_a_stale_anchor_changes_nothing() {
        let mut session = session_fixture_with_inactive(&["a"], &["x"]);
        let addition = ActivateMods::plan(&session, &mods(&["x"]), false);

        let error =
            ActivateMods::execute_at(&mut session, &addition, &Anchor::After(ModId::new("gone")))
                .expect_err("gone is not active");

        assert_eq!(error, MoveError::AnchorNotActive(ModId::new("gone")));
        assert_eq!(working_of(&session), ["a"]);
    }
}
