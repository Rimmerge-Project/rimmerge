//! `RefreshFileBaseline`: re-reads `ModsConfig.xml` so the Mods page's
//! pending markers and the "changed outside Rimmerge" notice stay live.
//!
//! Two phases, so a caller never holds the session across the read:
//! [`RefreshFileBaseline::fetch`] needs no `Session` (only the
//! [`FileProbe`] a short lock produced), and [`FileObservation::commit`]
//! swaps the result in under a second short lock. The use case only reads:
//! it never writes the file, and never touches `working` or the orders.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::file_baseline::{BaselineChange, BaselineIssue, FileProbe};
use crate::ports::ModsConfigStore;

/// What a read of the file found, ready to commit to the session it was
/// probed from. Only [`RefreshFileBaseline::fetch`] builds one.
#[derive(Debug)]
pub struct FileObservation {
    probe: FileProbe,
    outcome: Result<Vec<ModId>, BaselineIssue>,
}

impl FileObservation {
    /// Swaps this observation into `session`. A success replaces the
    /// baseline list; a failure keeps the last valid list and records why.
    /// An observation older than a write the session made since it was
    /// probed is discarded, so a read that began before an `Apply` can
    /// never put the pre-apply list back.
    pub fn commit(self, session: &mut Session) -> BaselineChange {
        session.observe_file(&self.probe, self.outcome)
    }
}

/// Reads the file through the port.
#[derive(Debug)]
pub struct RefreshFileBaseline<S> {
    store: S,
}

impl<S: ModsConfigStore> RefreshFileBaseline<S> {
    /// Builds the use case over `store`.
    #[must_use]
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Phase 1: reads the file `probe` names. Touches no session.
    #[must_use]
    pub fn fetch(&self, probe: FileProbe) -> FileObservation {
        let outcome = self
            .store
            .read(probe.path())
            .map(|file| file.active_mods)
            .map_err(|error| BaselineIssue::from(error.kind()));
        FileObservation { probe, outcome }
    }

    /// Both phases in one call, for a caller that already holds the session
    /// (the tests; the CLI never refreshes the baseline, it reads the file
    /// once per command).
    pub fn execute(&self, session: &mut Session) -> BaselineChange {
        self.fetch(session.file_probe()).commit(session)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::LoadOrder;
    use rim_resolve::domain::OrderSource;

    use super::*;
    use crate::active_set::Anchor;
    use crate::file_baseline::{ExternalFileChange, FileBaseline};
    use crate::mod_lists::ModListQuery;
    use crate::ports::{ConfigErrorKind, ModsConfigFile};
    use crate::test_support::{InMemoryModsConfigStore, session_fixture_with_inactive};
    use crate::use_cases::{ActivateMods, DeactivateMods, MoveMods};
    use crate::{BaselineIssue, MembershipMark, Session};

    fn ids(ids: &[&str]) -> Vec<ModId> {
        ids.iter().map(|id| ModId::new(*id)).collect()
    }

    fn file(active: &[&str]) -> ModsConfigFile {
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids(active),
            known_expansions: Vec::new(),
        }
    }

    /// Active a and b, installed but inactive x and y; the fake file
    /// starts out holding exactly a and b.
    fn fixture() -> (Session, InMemoryModsConfigStore) {
        (
            session_fixture_with_inactive(&["a", "b"], &["x", "y"]),
            InMemoryModsConfigStore::new(file(&["a", "b"])),
        )
    }

    fn refresh(session: &mut Session, store: &InMemoryModsConfigStore) -> BaselineChange {
        RefreshFileBaseline::new(store).execute(session)
    }

    fn active_marks(session: &Session) -> Vec<(String, MembershipMark)> {
        session
            .working_mod_list(&ModListQuery::default())
            .items
            .into_iter()
            .map(|row| (row.mod_id.to_string(), row.membership))
            .collect()
    }

    fn inactive_marks(session: &Session) -> Vec<(String, MembershipMark)> {
        session
            .inactive_mod_list(&ModListQuery::default())
            .items
            .into_iter()
            .map(|row| (row.mod_id.to_string(), row.membership))
            .collect()
    }

    fn activate(session: &mut Session, id: &str) {
        let plan = ActivateMods::plan(session, &ids(&[id]), false);
        ActivateMods::execute(session, &plan).expect("known mod");
    }

    fn deactivate(session: &mut Session, id: &str) {
        let plan = DeactivateMods::plan(session, &ids(&[id]));
        DeactivateMods::execute(session, &plan);
    }

    fn has_no_marks(session: &Session) -> bool {
        active_marks(session)
            .iter()
            .all(|(_, mark)| *mark == MembershipMark::None)
    }

    #[test]
    fn a_valid_read_replaces_the_baseline() {
        let (mut session, store) = fixture();
        store.set_file(file(&["a", "b", "x"]));

        let change = refresh(&mut session, &store);

        assert_eq!(change, BaselineChange::Changed);
        assert_eq!(session.file_active_mods(), ids(&["a", "b", "x"]).as_slice());
        assert_eq!(session.file_baseline(), FileBaseline::Fresh);
    }

    #[test]
    fn reading_the_same_list_again_changes_nothing() {
        let (mut session, store) = fixture();

        assert_eq!(refresh(&mut session, &store), BaselineChange::Unchanged);
    }

    #[test]
    fn a_failed_read_keeps_the_last_valid_list_and_names_the_reason() {
        for (kind, reason) in [
            (ConfigErrorKind::Missing, BaselineIssue::Missing),
            (ConfigErrorKind::Unreadable, BaselineIssue::Unreadable),
            (ConfigErrorKind::Malformed, BaselineIssue::Malformed),
        ] {
            let (mut session, store) = fixture();
            store.fail_reads(kind);

            let change = refresh(&mut session, &store);

            assert_eq!(change, BaselineChange::Changed, "{kind:?}");
            assert_eq!(session.file_active_mods(), ids(&["a", "b"]).as_slice());
            assert_eq!(session.file_baseline(), FileBaseline::Stale { reason });
        }
    }

    #[test]
    fn a_failure_never_turns_into_an_empty_baseline_or_new_markers() {
        let (mut session, store) = fixture();
        store.fail_reads(ConfigErrorKind::Malformed);

        refresh(&mut session, &store);

        assert!(!session.file_active_mods().is_empty());
        assert!(has_no_marks(&session));
    }

    #[test]
    fn the_same_failure_again_is_not_a_change() {
        let (mut session, store) = fixture();
        store.fail_reads(ConfigErrorKind::Missing);
        refresh(&mut session, &store);

        assert_eq!(refresh(&mut session, &store), BaselineChange::Unchanged);
    }

    #[test]
    fn a_valid_file_after_a_failure_recovers_the_baseline() {
        let (mut session, store) = fixture();
        store.fail_reads(ConfigErrorKind::Malformed);
        refresh(&mut session, &store);
        store.set_file(file(&["a", "b"]));

        let change = refresh(&mut session, &store);

        assert_eq!(change, BaselineChange::Changed, "the status flipped back");
        assert_eq!(session.file_baseline(), FileBaseline::Fresh);
    }

    #[test]
    fn pending_edits_are_marked_against_the_file_by_base_id() {
        let (mut session, store) = fixture();
        activate(&mut session, "x");
        deactivate(&mut session, "b");

        refresh(&mut session, &store);

        assert_eq!(
            active_marks(&session),
            [
                ("a".to_string(), MembershipMark::None),
                ("x".to_string(), MembershipMark::Added)
            ]
        );
        assert_eq!(
            inactive_marks(&session),
            [
                ("b".to_string(), MembershipMark::Removed),
                ("y".to_string(), MembershipMark::None)
            ]
        );
    }

    #[test]
    fn a_steam_suffixed_copy_in_the_file_is_the_same_mod() {
        let (mut session, store) = fixture();
        store.set_file(file(&["a", "b_steam"]));

        refresh(&mut session, &store);

        assert!(has_no_marks(&session));
    }

    #[test]
    fn a_reorder_alone_is_never_a_marker() {
        let (mut session, store) = fixture();
        store.set_file(file(&["b", "a"]));

        refresh(&mut session, &store);

        assert!(has_no_marks(&session));
        assert_eq!(
            session.external_file_change(),
            ExternalFileChange::Reordered,
            "the outside reorder is reported"
        );
    }

    #[test]
    fn a_membership_change_is_not_also_reported_as_a_reorder() {
        let (mut session, store) = fixture();
        store.set_file(file(&["b", "a", "x"]));

        refresh(&mut session, &store);

        assert_eq!(
            session.external_file_change(),
            ExternalFileChange::Membership {
                added: 1,
                removed: 0
            }
        );
    }

    #[test]
    fn an_unchanged_file_and_our_own_write_report_no_reorder() {
        let (mut session, store) = fixture();
        refresh(&mut session, &store);
        assert_eq!(session.external_file_change(), ExternalFileChange::None);

        store.set_file(file(&["b", "a"]));
        refresh(&mut session, &store);
        assert_ne!(session.external_file_change(), ExternalFileChange::None);
        session.set_current_order(LoadOrder::new(ids(&["b", "a"])));

        assert_eq!(session.external_file_change(), ExternalFileChange::None);
    }

    #[test]
    fn writing_the_same_order_back_clears_the_reorder() {
        let (mut session, store) = fixture();
        store.set_file(file(&["b", "a"]));
        refresh(&mut session, &store);

        store.set_file(file(&["a", "b"]));
        refresh(&mut session, &store);

        assert_eq!(session.external_file_change(), ExternalFileChange::None);
    }

    #[test]
    fn an_outside_write_that_matches_a_pending_change_clears_its_marker() {
        let (mut session, store) = fixture();
        activate(&mut session, "x");
        refresh(&mut session, &store);
        assert_eq!(active_marks(&session)[2].1, MembershipMark::Added);

        store.set_file(file(&["a", "b", "x"]));
        refresh(&mut session, &store);

        assert_eq!(active_marks(&session)[2].1, MembershipMark::None);
    }

    #[test]
    fn an_outside_add_and_remove_recalculate_the_markers_and_are_counted() {
        let (mut session, store) = fixture();
        store.set_file(file(&["a", "y"]));

        refresh(&mut session, &store);

        assert_eq!(
            active_marks(&session),
            [
                ("a".to_string(), MembershipMark::None),
                ("b".to_string(), MembershipMark::Added)
            ]
        );
        assert_eq!(
            inactive_marks(&session),
            [
                ("x".to_string(), MembershipMark::None),
                ("y".to_string(), MembershipMark::Removed)
            ]
        );
        assert_eq!(
            session.external_file_change(),
            ExternalFileChange::Membership {
                added: 1,
                removed: 1
            }
        );
    }

    #[test]
    fn a_refresh_only_reads() {
        let (mut session, store) = fixture();
        activate(&mut session, "x");
        let working = session.working().ids().to_vec();
        let current = session.orders().current.as_slice().to_vec();
        store.set_file(file(&["y", "a"]));

        refresh(&mut session, &store);

        assert!(store.writes().is_empty());
        assert_eq!(session.working().ids(), working.as_slice());
        assert_eq!(session.orders().current.as_slice(), current.as_slice());
    }

    #[test]
    fn writing_the_file_ourselves_is_not_an_outside_change() {
        let (mut session, store) = fixture();
        store.set_file(file(&["a", "b", "x"]));
        refresh(&mut session, &store);
        assert_ne!(session.external_file_change(), ExternalFileChange::None);

        session.set_current_order(LoadOrder::new(ids(&["a", "b", "x"])));

        assert_eq!(session.external_file_change(), ExternalFileChange::None);
    }

    #[test]
    fn an_outside_edit_alone_is_no_unapplied_work_of_our_own() {
        let (mut session, store) = fixture();
        store.set_file(file(&["b", "a", "x"]));

        refresh(&mut session, &store);
        let pending = session.pending_changes();

        assert!(pending.unapplied.is_empty());
        assert!(!pending.order_unapplied);
        assert!(
            !session.file_matches(OrderSource::Current),
            "the live comparison does follow the outside edit"
        );
    }

    #[test]
    fn an_outside_reorder_alone_is_no_unapplied_order_of_our_own() {
        let (mut session, store) = fixture();
        store.set_file(file(&["b", "a"]));

        refresh(&mut session, &store);
        let pending = session.pending_changes();

        assert!(!pending.order_unapplied);
        assert!(!session.file_matches(OrderSource::Current));
    }

    #[test]
    fn an_own_move_and_an_outside_edit_report_only_the_own_move() {
        let (mut session, store) = fixture();
        MoveMods::execute(&mut session, &ids(&["b"]), &Anchor::Start).expect("active mod");
        store.set_file(file(&["a", "b", "x"]));

        refresh(&mut session, &store);
        let pending = session.pending_changes();

        assert!(pending.order_unapplied, "the move is ours and unwritten");
        assert!(
            pending.unapplied.is_empty(),
            "the move changes no membership"
        );
    }

    #[test]
    fn writing_our_own_order_clears_the_own_facts() {
        let (mut session, store) = fixture();
        MoveMods::execute(&mut session, &ids(&["b"]), &Anchor::Start).expect("active mod");
        assert!(session.pending_changes().order_unapplied);

        session.set_current_order(LoadOrder::new(ids(&["b", "a"])));
        refresh(&mut session, &store);

        assert!(!session.pending_changes().order_unapplied);
    }

    #[test]
    fn a_read_that_began_before_our_own_write_is_discarded() {
        let (mut session, store) = fixture();
        let observation = RefreshFileBaseline::new(&store).fetch(session.file_probe());
        session.set_current_order(LoadOrder::new(ids(&["a", "b", "x"])));

        let change = observation.commit(&mut session);

        assert_eq!(change, BaselineChange::Unchanged);
        assert_eq!(session.file_active_mods(), ids(&["a", "b", "x"]).as_slice());
    }
}
