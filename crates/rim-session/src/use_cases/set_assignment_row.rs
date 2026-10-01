//! [`SetAssignmentRow`]: validates and records one section's row —
//! target-keyed or free-standing — through
//! [`rim_resolve::domain::AssignmentProject::set_row`], with the usual
//! snapshot/persist/rollback shape.
//!
//! One use case covers both row kinds, taking `(def_type, RowKey)`
//! explicitly — a `RowKey::Target` row or a `RowKey::Own` row — since the
//! caller already knows which section it's addressing.

use rim_resolve::domain::{AssignmentId, AssignmentRow, AssignmentRowError, RowKey};

use crate::assignment_refs::SessionKnownDefs;
use crate::ports::{AssignmentProjectStore, StoreError};
use crate::{Session, UnknownAssignment};

/// Everything that can go wrong setting one assignment row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetAssignmentRowError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// [`AssignmentProject::set_row`](rim_resolve::domain::AssignmentProject::set_row)
    /// rejected the row.
    #[error(transparent)]
    Row(#[from] AssignmentRowError),
    /// Persisting the change failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Validates and records one section's row.
pub struct SetAssignmentRow<Store> {
    store: Store,
}

impl<Store: AssignmentProjectStore> SetAssignmentRow<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`SetAssignmentRowError`]. On any error, `session`'s own copy
    /// of the project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        def_type: &str,
        key: RowKey,
        row: AssignmentRow,
    ) -> Result<Option<AssignmentRow>, SetAssignmentRowError> {
        let snapshot = session
            .assignment_snapshot(id)
            .ok_or_else(|| UnknownAssignment(id.clone()))?;
        let mut project = snapshot.clone();

        let own_package_id = project.identity().package_id().clone();
        // `known` reads `snapshot` (the pristine pre-mutation project), not
        // `project` itself — this one `set_row` call only ever changes one
        // row of one section, so every *other* section's own free-standing
        // names (what `own_instances` answers from) are identical either
        // way, and reading `snapshot` avoids borrowing `project` while it's
        // being mutated.
        let known = SessionKnownDefs::new(session, own_package_id, &snapshot);
        let replaced = project.set_row(def_type, key, row, &known)?;

        session.upsert_assignment(project);
        let stored = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_assignment(snapshot);
            return Err(SetAssignmentRowError::Store(error));
        }
        Ok(replaced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryAssignmentProjectStore, assignment_fixture};
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{DefKey, TargetRef};
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

    #[test]
    fn sets_and_persists_a_valid_target_keyed_row() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        use_case
            .execute(
                &mut session,
                &id,
                "example.PartAssignmentDef",
                RowKey::Target(human_target()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "test_Human".to_string(),
                    note: None,
                },
            )
            .expect("a valid row must be accepted");

        let project = session.assignment(&id).expect("still loaded");
        assert_eq!(
            project
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows
                .len(),
            1
        );
        assert!(use_case.store.last_saved(&id).is_some());
    }

    /// A free-standing ("new def") section's own row, addressed by
    /// [`RowKey::Own`].
    #[test]
    fn sets_and_persists_a_valid_free_standing_row() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &[]);
        let mut project = session.assignment(&id).cloned().expect("loaded");
        project
            .add_section(rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartDef".to_string(),
                refs: std::collections::BTreeSet::new(),
                fields: BTreeMap::new(),
                target_shapes: BTreeMap::new(),
            })
            .expect("a fresh def type must add cleanly");
        session.upsert_assignment(project);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        use_case
            .execute(
                &mut session,
                &id,
                "example.PartDef",
                RowKey::Own("mypatch_newpart_Tail".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "mypatch_newpart_Tail".to_string(),
                    note: None,
                },
            )
            .expect("a valid free-standing row must be accepted");

        let project = session.assignment(&id).expect("still loaded");
        assert_eq!(
            project
                .section("example.PartDef")
                .expect("the new section")
                .rows
                .len(),
            1
        );
    }

    /// A free-standing row set on one section is visible as a known own
    /// instance to a *different* section's item-slot validation — the
    /// whole point of `SessionKnownDefs::own_instances`.
    #[test]
    fn a_free_standing_rows_own_name_is_a_known_item_for_another_section() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let mut project = session.assignment(&id).cloned().expect("loaded");
        project
            .add_section(rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartDef".to_string(),
                refs: std::collections::BTreeSet::new(),
                fields: BTreeMap::new(),
                target_shapes: BTreeMap::new(),
            })
            .expect("a fresh def type must add cleanly");
        let mut schema = project
            .section("example.PartAssignmentDef")
            .expect("the fixture's own section")
            .schema
            .clone();
        schema
            .add_field(
                "parts".parse().unwrap(),
                rim_resolve::domain::FieldRole::ItemSlot {
                    def_type: "example.PartDef".to_string(),
                },
                rim_resolve::domain::Cardinality::List,
            )
            .expect("a fresh field must add cleanly");
        project.set_section_schema("example.PartAssignmentDef", schema);
        session.upsert_assignment(project);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());
        use_case
            .execute(
                &mut session,
                &id,
                "example.PartDef",
                RowKey::Own("MyPart".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "MyPart".to_string(),
                    note: None,
                },
            )
            .expect("the free-standing part row");

        let result = use_case.execute(
            &mut session,
            &id,
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::from([(
                    "parts".parse().unwrap(),
                    rim_resolve::domain::RowValue::Names(vec!["MyPart".to_string()]),
                )]),
                def_name: "test_Human".to_string(),
                note: None,
            },
        );

        assert!(
            result.is_ok(),
            "MyPart must be recognized as a known own item, not rejected as unknown: {result:?}"
        );
    }

    #[test]
    fn rejects_a_row_on_a_non_target_key_field() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());
        let bad_target = TargetRef {
            key_field: "notAField".parse().unwrap(),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Human".to_string(),
            },
        };

        let result = use_case.execute(
            &mut session,
            &id,
            "example.PartAssignmentDef",
            RowKey::Target(bad_target),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "test_Human".to_string(),
                note: None,
            },
        );

        assert!(matches!(result, Err(SetAssignmentRowError::Row(_))));
    }

    #[test]
    fn rejects_an_unknown_assignment() {
        let (mut session, _id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());
        let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(
            &mut session,
            &bogus,
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "test_Human".to_string(),
                note: None,
            },
        );

        assert!(matches!(result, Err(SetAssignmentRowError::Unknown(_))));
    }

    #[test]
    fn a_failed_save_rolls_back_the_row() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = SetAssignmentRow::new(store);

        let result = use_case.execute(
            &mut session,
            &id,
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "test_Human".to_string(),
                note: None,
            },
        );

        assert!(matches!(result, Err(SetAssignmentRowError::Store(_))));
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows
                .is_empty(),
            "the row must be rolled back when the save fails"
        );
    }

    /// The session's own view must exclude this project's own previously-
    /// exported mod (`KnownDefs`'s own doc comment)
    /// — otherwise every re-save of an already-exported row would
    /// spuriously collide with itself once that row's `defName` shows up
    /// as an active def under the project's own package id.
    #[test]
    fn a_def_name_matching_this_projects_own_package_id_is_not_treated_as_a_collision() {
        let (session, id) = assignment_fixture(&["a", "test.assignment"], &["a"], &["b"]);
        // Simulate this project's own prior export being active: some def
        // of the schema's type, owned by the project's own package id,
        // already exists in `owners_by_def`.
        let mut sources = session.sources().clone();
        sources.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "test_assignment_Human".to_string(),
            ),
            vec![ModId::new("test.assignment")],
        );
        // There's no public "set sources" — rebuild a session carrying
        // this fixture's sources instead, re-adding the project.
        let project = session.assignment(&id).cloned().expect("loaded above");
        let mut session = crate::test_support::session_with_sources_and_mods(
            sources,
            crate::test_support::report_fixture(&["a", "test.assignment"]),
            &["a", "test.assignment"],
        );
        session.upsert_assignment(project);
        let use_case = SetAssignmentRow::new(InMemoryAssignmentProjectStore::new());

        let result = use_case.execute(
            &mut session,
            &id,
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "test_assignment_Human".to_string(),
                note: None,
            },
        );

        assert!(
            result.is_ok(),
            "the project's own previously-exported defName must not collide with itself: {result:?}"
        );
    }
}
