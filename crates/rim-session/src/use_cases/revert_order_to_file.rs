//! [`RevertOrderToFile`]: puts Current's sequence back to what
//! `ModsConfig.xml` holds.

use rim_analyzer::domain::LoadOrder;

use crate::Session;

/// Why [`RevertOrderToFile::execute`] refused. The session is untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RevertOrderError {
    /// The working set has edits no rescan has picked up. Discard them (or
    /// rescan) first: otherwise the next rescan would scan the working
    /// sequence and undo the revert.
    #[error("the active list has changes that were not scanned yet")]
    PendingScanChanges,
    /// Current and the file hold different mods, so there is no sequence
    /// to restore: only the same ids in another order can be reverted.
    #[error("the file lists different mods than the current order")]
    MembershipDiffers,
}

/// Reverts a settled reorder.
pub struct RevertOrderToFile;

impl RevertOrderToFile {
    /// Current's sequence := the file's, when both hold the same ids. The
    /// working set follows (the file is the truth, the direction of
    /// [`Session::set_current_order`]). Nothing is written. Already equal is
    /// a no-op.
    ///
    /// # Errors
    ///
    /// See [`RevertOrderError`].
    pub fn execute(session: &mut Session) -> Result<(), RevertOrderError> {
        if session.is_stale() {
            return Err(RevertOrderError::PendingScanChanges);
        }
        let mut current_ids = session.orders().current.as_slice().to_vec();
        let mut file_ids = session.file_active_mods().to_vec();
        if current_ids == file_ids {
            return Ok(());
        }
        file_ids.sort();
        current_ids.sort();
        if file_ids != current_ids {
            return Err(RevertOrderError::MembershipDiffers);
        }
        let order = LoadOrder::new(session.file_active_mods().to_vec());
        session.set_current_order(order);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;
    use crate::active_set::Anchor;
    use crate::test_support::{session_fixture, session_fixture_with_inactive};
    use crate::use_cases::{ActivateMods, MoveMods};

    #[test]
    fn reverting_puts_current_and_working_back_to_the_files_sequence() {
        let mut session = session_fixture(&["a", "b", "c"]);
        MoveMods::execute(&mut session, &[ModId::new("c")], &Anchor::Start).expect("valid");
        assert!(!session.file_matches(rim_resolve::domain::OrderSource::Current));

        RevertOrderToFile::execute(&mut session).expect("same ids as the file");

        let abc = [ModId::new("a"), ModId::new("b"), ModId::new("c")];
        assert_eq!(session.orders().current.as_slice(), abc);
        assert_eq!(session.working().ids(), abc);
        assert!(session.file_matches(rim_resolve::domain::OrderSource::Current));
    }

    #[test]
    fn reverting_an_order_already_in_the_file_changes_nothing() {
        let mut session = session_fixture(&["a", "b"]);

        RevertOrderToFile::execute(&mut session).expect("nothing to revert is not an error");

        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("a"), ModId::new("b")]
        );
    }

    #[test]
    fn reverting_refuses_when_current_holds_other_mods_than_the_file() {
        let mut session = session_fixture(&["a", "b"]);
        // An imported order: scanned Current differs from the file in
        // membership, so there is no sequence to restore.
        session.set_current_order(rim_analyzer::domain::LoadOrder::new(vec![ModId::new("a")]));
        session.set_file_active_mods(vec![ModId::new("a"), ModId::new("b")]);

        let error = RevertOrderToFile::execute(&mut session).expect_err("membership differs");

        assert_eq!(error, RevertOrderError::MembershipDiffers);
        assert_eq!(session.orders().current.as_slice(), [ModId::new("a")]);
    }

    #[test]
    fn reverting_refuses_while_the_working_set_has_unscanned_changes() {
        let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
        MoveMods::execute(&mut session, &[ModId::new("b")], &Anchor::Start).expect("valid");
        let plan = ActivateMods::plan(&session, &[ModId::new("x")], false);
        ActivateMods::execute(&mut session, &plan).expect("x is known");

        let error = RevertOrderToFile::execute(&mut session).expect_err("working is stale");

        assert_eq!(error, RevertOrderError::PendingScanChanges);
        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("b"), ModId::new("a")],
            "the settled move is left alone"
        );
    }
}
