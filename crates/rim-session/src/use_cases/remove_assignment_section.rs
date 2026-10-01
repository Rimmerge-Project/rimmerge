//! [`RemoveAssignmentSection`]: removes a section from an assignment
//! project.

use rim_resolve::domain::{AssignmentId, Section, SectionError};

use crate::ports::{AssignmentProjectStore, StoreError};
use crate::{Session, UnknownAssignment};

/// Everything that can go wrong removing a section from an assignment
/// project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoveAssignmentSectionError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// Without `force`, the section's own free-standing rows are still
    /// referenced by another section's `ItemSlot` value —
    /// [`SectionError::SectionInUse`] names each referencing row.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// Persisting the removal failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Removes one section from an assignment project, with the usual
/// snapshot/persist/rollback shape.
pub struct RemoveAssignmentSection<Store> {
    store: Store,
}

impl<Store: AssignmentProjectStore> RemoveAssignmentSection<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Removes `def_type`'s own section, if the project has one. `Ok(None)`
    /// when it doesn't (idempotent, mirroring
    /// [`rim_resolve::domain::AssignmentProject::remove_section`] itself).
    ///
    /// Without `force`, refuses ([`SectionError::SectionInUse`]) when the
    /// section's own free-standing row names are still named by another
    /// section's `ItemSlot` value. With `force`, the section is removed
    /// regardless and those references are left dangling — a later
    /// export surfaces each as a skip rather than this silently scrubbing them.
    ///
    /// # Errors
    ///
    /// See [`RemoveAssignmentSectionError`]. On any error, `session`'s own
    /// copy of the project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        def_type: &str,
        force: bool,
    ) -> Result<Option<Section>, RemoveAssignmentSectionError> {
        let snapshot = session
            .assignment_snapshot(id)
            .ok_or_else(|| UnknownAssignment(id.clone()))?;
        let mut project = snapshot.clone();

        let removed = project.remove_section(def_type, force)?;

        session.upsert_assignment(project);
        let stored = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_assignment(snapshot);
            return Err(RemoveAssignmentSectionError::Store(error));
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryAssignmentProjectStore, assignment_fixture};
    use rim_resolve::domain::{
        AssignmentRow, AssignmentSchema, Cardinality, FieldRole, KnownDefs, RowKey, RowValue,
        TargetRef,
    };
    use std::collections::{BTreeMap, BTreeSet};

    struct NoneKnown;
    impl KnownDefs for NoneKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
    }

    /// A [`KnownDefs`] fake recognizing one fixed `(def_type, name)` as a
    /// known own instance — for a test row that references a free-standing
    /// row this same test just set, without needing a real
    /// `SessionKnownDefs`/session round trip.
    struct OwnKnown {
        def_type: &'static str,
        name: &'static str,
    }
    impl KnownDefs for OwnKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
        fn own_instances(&self, def_type: &str) -> BTreeSet<String> {
            if def_type == self.def_type {
                BTreeSet::from([self.name.to_string()])
            } else {
                BTreeSet::new()
            }
        }
    }

    fn standalone_schema(def_type: &str) -> AssignmentSchema {
        AssignmentSchema {
            def_type: def_type.to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        }
    }

    #[test]
    fn removes_an_existing_section_and_persists_it() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = RemoveAssignmentSection::new(InMemoryAssignmentProjectStore::new());

        let removed = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef", false)
            .expect("removing an unreferenced section must succeed");

        assert!(removed.is_some());
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .is_none()
        );
        assert!(use_case.store.last_saved(&id).is_some());
    }

    #[test]
    fn removing_an_unknown_def_type_is_a_no_op_returning_none() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = RemoveAssignmentSection::new(InMemoryAssignmentProjectStore::new());

        let removed = use_case
            .execute(&mut session, &id, "no.such.type", false)
            .expect("removing an absent section must still succeed");

        assert!(removed.is_none());
    }

    /// Without `force`, a section whose own free-standing rows are still
    /// referenced by another section's `ItemSlot` value is refused.
    #[test]
    fn without_force_refuses_a_section_still_referenced_by_another() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let mut project = session.assignment(&id).cloned().expect("loaded");
        project
            .add_section(standalone_schema("example.PartDef"))
            .expect("a fresh def type must add cleanly");
        project
            .set_row(
                "example.PartDef",
                RowKey::Own("MyPart".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "MyPart".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("a valid free-standing row");
        // The already-existing `example.PartAssignmentDef` section's schema gains
        // an `ItemSlot` field naming `example.PartDef`, then a row using it
        // to reference the free-standing part above.
        let mut schema = project
            .section("example.PartAssignmentDef")
            .expect("the fixture's own section")
            .schema
            .clone();
        schema
            .add_field(
                "parts".parse().expect("valid path"),
                FieldRole::ItemSlot {
                    def_type: "example.PartDef".to_string(),
                },
                Cardinality::List,
            )
            .expect("a fresh field must add cleanly");
        project.set_section_schema("example.PartAssignmentDef", schema);
        project
            .set_row(
                "example.PartAssignmentDef",
                RowKey::Target(TargetRef {
                    key_field: "speciesNames".parse().expect("valid path"),
                    def: rim_resolve::domain::DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Human".to_string(),
                    },
                }),
                AssignmentRow {
                    values: BTreeMap::from([(
                        "parts".parse().expect("valid path"),
                        RowValue::Names(vec!["MyPart".to_string()]),
                    )]),
                    def_name: "test_Human".to_string(),
                    note: None,
                },
                &OwnKnown {
                    def_type: "example.PartDef",
                    name: "MyPart",
                },
            )
            .expect("a valid row referencing the free-standing part");
        session.upsert_assignment(project);
        let use_case = RemoveAssignmentSection::new(InMemoryAssignmentProjectStore::new());

        let result = use_case.execute(&mut session, &id, "example.PartDef", false);

        assert!(matches!(
            result,
            Err(RemoveAssignmentSectionError::Section(
                SectionError::SectionInUse { .. }
            ))
        ));
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartDef")
                .is_some(),
            "the refusal must not touch the project"
        );
    }

    /// `force` removes the section regardless, leaving the referencing
    /// row's own value dangling for a later export to skip.
    #[test]
    fn force_removes_a_referenced_section_leaving_the_reference_dangling() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let mut project = session.assignment(&id).cloned().expect("loaded");
        project
            .add_section(standalone_schema("example.PartDef"))
            .expect("a fresh def type must add cleanly");
        project
            .set_row(
                "example.PartDef",
                RowKey::Own("MyPart".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "MyPart".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("a valid free-standing row");
        session.upsert_assignment(project);
        let use_case = RemoveAssignmentSection::new(InMemoryAssignmentProjectStore::new());

        let removed = use_case
            .execute(&mut session, &id, "example.PartDef", true)
            .expect("force must remove regardless");

        assert!(removed.is_some());
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartDef")
                .is_none()
        );
    }

    #[test]
    fn rejects_an_unknown_assignment() {
        let (mut session, _id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let use_case = RemoveAssignmentSection::new(InMemoryAssignmentProjectStore::new());
        let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, "example.PartAssignmentDef", false);

        assert!(matches!(
            result,
            Err(RemoveAssignmentSectionError::Unknown(_))
        ));
    }

    #[test]
    fn a_failed_save_rolls_back_the_removal() {
        let (mut session, id) = assignment_fixture(&["a"], &["a"], &["b"]);
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = RemoveAssignmentSection::new(store);

        let result = use_case.execute(&mut session, &id, "example.PartAssignmentDef", false);

        assert!(matches!(
            result,
            Err(RemoveAssignmentSectionError::Store(_))
        ));
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .is_some(),
            "the removal must be rolled back when the save fails"
        );
    }
}
