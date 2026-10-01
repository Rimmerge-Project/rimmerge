//! [`ClearAssignmentRow`]: removes one section's row — target-keyed or
//! free-standing — with the usual snapshot/persist/rollback shape.
//!
//! **Merged from `ClearAssignmentRow`/`ClearStandaloneAssignmentRow`**
//! — see
//! [`super::SetAssignmentRow`]'s own module doc comment for why.

use rim_resolve::domain::{AssignmentId, AssignmentRow, RowKey};

use crate::ports::{AssignmentProjectStore, StoreError};
use crate::{Session, UnknownAssignment};

/// Everything that can go wrong clearing an assignment row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClearAssignmentRowError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// Persisting the change failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Removes one section's row, if it has one.
pub struct ClearAssignmentRow<Store> {
    store: Store,
}

impl<Store: AssignmentProjectStore> ClearAssignmentRow<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`ClearAssignmentRowError`]. On any error, `session`'s own copy
    /// of the project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        def_type: &str,
        key: &RowKey,
    ) -> Result<Option<AssignmentRow>, ClearAssignmentRowError> {
        let snapshot = session
            .assignment_snapshot(id)
            .ok_or_else(|| UnknownAssignment(id.clone()))?;
        let mut project = snapshot.clone();
        let cleared = project.clear_row(def_type, key);

        session.upsert_assignment(project);
        let stored = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_assignment(snapshot);
            return Err(ClearAssignmentRowError::Store(error));
        }
        Ok(cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryAssignmentProjectStore, assignment_fixture};
    use rim_resolve::domain::{AssignmentRow, DefKey, TargetRef};
    use std::collections::BTreeMap;

    fn human_target() -> TargetRef {
        TargetRef {
            key_field: "speciesNames".parse().unwrap(),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Human".to_string(),
            },
        }
    }

    struct NoneKnown;
    impl rim_resolve::domain::KnownDefs for NoneKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
    }

    fn fixture_with_target_row() -> (Session, AssignmentId) {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let mut project = session.assignment(&id).cloned().expect("just created");
        project
            .set_row(
                "example.PartAssignmentDef",
                RowKey::Target(human_target()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "test_Human".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("valid row");
        session.upsert_assignment(project);
        (session, id)
    }

    /// A free-standing ("new def") section's own row, addressed by
    /// [`RowKey::Own`].
    fn fixture_with_free_standing_row() -> (Session, AssignmentId) {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &[]);
        let mut project = session.assignment(&id).cloned().expect("just created");
        project
            .add_section(rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartDef".to_string(),
                refs: std::collections::BTreeSet::new(),
                fields: BTreeMap::new(),
                target_shapes: BTreeMap::new(),
            })
            .expect("a fresh def type must add cleanly");
        project
            .set_row(
                "example.PartDef",
                RowKey::Own("mypatch_newpart_Tail".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "mypatch_newpart_Tail".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("valid row");
        session.upsert_assignment(project);
        (session, id)
    }

    #[test]
    fn clears_an_existing_target_keyed_row_and_persists_it() {
        let (mut session, id) = fixture_with_target_row();
        let use_case = ClearAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        let cleared = use_case
            .execute(
                &mut session,
                &id,
                "example.PartAssignmentDef",
                &RowKey::Target(human_target()),
            )
            .expect("clearing must succeed");

        assert!(cleared.is_some());
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows
                .is_empty()
        );
        assert!(use_case.store.last_saved(&id).is_some());
    }

    #[test]
    fn clears_an_existing_free_standing_row_and_persists_it() {
        let (mut session, id) = fixture_with_free_standing_row();
        let use_case = ClearAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        let cleared = use_case
            .execute(
                &mut session,
                &id,
                "example.PartDef",
                &RowKey::Own("mypatch_newpart_Tail".to_string()),
            )
            .expect("clearing must succeed");

        assert!(cleared.is_some());
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartDef")
                .expect("the section itself is untouched")
                .rows
                .is_empty()
        );
    }

    #[test]
    fn clearing_a_target_with_no_row_is_a_no_op_returning_none() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = ClearAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        let cleared = use_case
            .execute(
                &mut session,
                &id,
                "example.PartAssignmentDef",
                &RowKey::Target(human_target()),
            )
            .expect("clearing an absent row must still succeed");

        assert!(cleared.is_none());
    }

    #[test]
    fn clearing_from_an_unknown_section_is_a_no_op_returning_none() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = ClearAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        let cleared = use_case
            .execute(
                &mut session,
                &id,
                "no.such.type",
                &RowKey::Own("x".to_string()),
            )
            .expect("clearing from an unknown section must still succeed");

        assert!(cleared.is_none());
    }

    #[test]
    fn rejects_an_unknown_assignment() {
        let (mut session, _id) = fixture_with_target_row();
        let use_case = ClearAssignmentRow::new(InMemoryAssignmentProjectStore::new());
        let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(
            &mut session,
            &bogus,
            "example.PartAssignmentDef",
            &RowKey::Target(human_target()),
        );

        assert!(matches!(result, Err(ClearAssignmentRowError::Unknown(_))));
    }

    #[test]
    fn a_failed_save_rolls_back_the_clear() {
        let (mut session, id) = fixture_with_target_row();
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = ClearAssignmentRow::new(store);

        let result = use_case.execute(
            &mut session,
            &id,
            "example.PartAssignmentDef",
            &RowKey::Target(human_target()),
        );

        assert!(matches!(result, Err(ClearAssignmentRowError::Store(_))));
        assert_eq!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows
                .len(),
            1,
            "the row must be rolled back when the save fails"
        );
    }
}
