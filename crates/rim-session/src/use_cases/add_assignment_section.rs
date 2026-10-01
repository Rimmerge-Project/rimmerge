//! [`AddAssignmentSection`]: infers and adds a new section to an existing
//! assignment project.

use rim_resolve::domain::{AssignmentId, SectionError};

use super::create_assignment::{AssignmentCandidate, CreateAssignment, ProposeAssignmentError};
use crate::ports::{AssignmentProjectStore, DefSourceReader, NoAssignmentStore, StoreError};
use crate::{Session, UnknownAssignment};

/// Everything that can go wrong adding a section to an assignment project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddAssignmentSectionError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// Inferring the candidate's schema failed.
    #[error(transparent)]
    Propose(#[from] ProposeAssignmentError),
    /// `def_type` isn't owned by any member of the project's own effective
    /// reference set ("refuses a type the refs do not own").
    #[error("{0:?} is not owned by any member of this project's own reference set")]
    NotOwnedByRefs(String),
    /// `def_type` has no valid candidate at all under the project's
    /// current refs/targets, per
    /// [`CreateAssignment::infer_candidate_for_explicit_add`]'s own gate.
    /// Explicit adds do not gate on an empty target set, so nothing in
    /// this crate currently returns `None` down that path. The variant
    /// stays, and this call site keeps its `.ok_or_else`, for defensive
    /// completeness: the shared inference the explicit-add path reuses
    /// still returns `Option`, and this is where a future `None` from it
    /// would surface.
    #[error("{0:?} has no valid candidate under this project's own refs/targets")]
    NoValidCandidate(String),
    /// The project already has a section for `def_type`.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// Persisting the new section failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Infers and adds a new section to an existing assignment project.
pub struct AddAssignmentSection<Store, Reader> {
    store: Store,
    reader: Reader,
}

impl<Store: AssignmentProjectStore, Reader: DefSourceReader> AddAssignmentSection<Store, Reader> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(store: Store, reader: Reader) -> Self {
        Self { store, reader }
    }

    /// Infers a candidate for `def_type` against the project's own current
    /// `refs`/`excluded_refs`/`targets` (the same phase-2 inference
    /// [`CreateAssignment::infer_candidate`] runs for a brand-new project,
    /// via [`CreateAssignment::infer_candidate_for_explicit_add`] since
    /// `def_type` is a name the caller already chose, not one this use
    /// case is proposing — see that method's own doc comment for why a
    /// free-standing type is never refused here just because the
    /// project's target set is non-empty) and adds it as a new section —
    /// target-keyed or free-standing exactly as the inferred schema's own
    /// `has_target_key()` decides.
    ///
    /// # Errors
    ///
    /// See [`AddAssignmentSectionError`]. On any error, `session`'s own
    /// copy of the project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        def_type: &str,
    ) -> Result<AssignmentCandidate, AddAssignmentSectionError> {
        let snapshot = session
            .assignment_snapshot(id)
            .ok_or_else(|| UnknownAssignment(id.clone()))?;
        let mut project = snapshot.clone();

        // `NoAssignmentStore` here is a throwaway: only `infer_candidate`/
        // `list_candidates` are used below, and neither touches `self.store`
        // at all — this use case's own `self.store` (not `CreateAssignment`'s)
        // is what actually persists the result.
        let inferrer = CreateAssignment::new(NoAssignmentStore, &self.reader);
        let candidates = inferrer.list_candidates(
            session,
            project.refs(),
            project.excluded_refs(),
            project.targets(),
        );
        if !candidates.iter().any(|c| c.def_type == def_type) {
            return Err(AddAssignmentSectionError::NotOwnedByRefs(
                def_type.to_string(),
            ));
        }

        let candidate = inferrer
            .infer_candidate_for_explicit_add(
                session,
                project.refs(),
                project.excluded_refs(),
                project.targets(),
                def_type,
            )?
            .ok_or_else(|| AddAssignmentSectionError::NoValidCandidate(def_type.to_string()))?;

        project.add_section(candidate.schema.clone())?;

        session.upsert_assignment(project);
        let stored = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_assignment(snapshot);
            return Err(AddAssignmentSectionError::Store(error));
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{DefSourceError, ElementExpectation};
    use crate::test_support::{InMemoryAssignmentProjectStore, assignment_fixture};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, ModId, XmlLocator};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;

    struct FakeReader {
        by_ordinal: BTreeMap<u32, String>,
    }

    impl DefSourceReader for FakeReader {
        fn read_element(
            &self,
            locator: &XmlLocator,
            _expected: &ElementExpectation,
        ) -> Result<String, DefSourceError> {
            self.by_ordinal
                .get(&locator.element_path[0])
                .cloned()
                .ok_or_else(|| DefSourceError::Io {
                    file: locator.file.to_path_buf(),
                    message: "not seeded".to_string(),
                })
        }
    }

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
    }

    /// `a` owns 5 instances each of `other.Type` and `example.PartAssignmentDef`
    /// (the fixture project's own already-existing section type), both
    /// with a scalar `label` field only — a valid standalone candidate for
    /// either type once `targets` is empty.
    fn fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        let mut by_ordinal = BTreeMap::new();
        let mut ordinal = 0u32;
        for def_type in ["other.Type", "example.PartAssignmentDef"] {
            for i in 0..5u32 {
                let name = format!("{def_type}{i}");
                index.defs.insert(
                    (ModId::new("a"), (def_type.to_string(), name.clone())),
                    vec![DefEntry {
                        def_type: def_type.to_string(),
                        def_name: name.clone(),
                        may_require: Vec::new(),
                        may_require_any_of: Vec::new(),
                        parent_name: None,
                        locator: locator(ordinal),
                    }],
                );
                index
                    .owners_by_def
                    .insert((def_type.to_string(), name.clone()), vec![ModId::new("a")]);
                by_ordinal.insert(
                    ordinal,
                    format!(
                        "<{def_type}><defName>{name}</defName><label>Label{i}</label></{def_type}>"
                    ),
                );
                ordinal += 1;
            }
        }
        (index, FakeReader { by_ordinal })
    }

    /// The same session-construction dance every test here needs: swap
    /// `assignment_fixture`'s own empty `SourceIndex` for `fixture`'s real
    /// one, keeping the same already-loaded project (there's no public
    /// "set sources" — mirrors `set_assignment_row.rs`'s own pattern). T is
    /// left empty so a schema with no `TargetKey` field (`other.Type`'s
    /// own shape) is still a valid candidate.
    fn fixture_session(sources: SourceIndex) -> (Session, AssignmentId) {
        fixture_session_with_targets(sources, &[])
    }

    /// Same construction as [`fixture_session`], but lets a test choose the
    /// project's own target set T — needed to pin the explicit-add gate:
    /// a non-empty T must not block adding a free-standing section.
    fn fixture_session_with_targets(
        sources: SourceIndex,
        targets: &[&str],
    ) -> (Session, AssignmentId) {
        let (session, id) = assignment_fixture(&["a"], &["a"], targets);
        let project = session.assignment(&id).cloned().expect("loaded above");
        let mut session = crate::test_support::session_with_sources_and_mods(
            sources,
            crate::test_support::report_fixture(&["a", "b"]),
            &["a", "b"],
        );
        session.upsert_assignment(project);
        (session, id)
    }

    #[test]
    fn adds_a_new_free_standing_section_for_a_type_the_refs_own() {
        let (sources, reader) = fixture();
        let (mut session, id) = fixture_session(sources);
        let use_case = AddAssignmentSection::new(InMemoryAssignmentProjectStore::new(), reader);

        let candidate = use_case
            .execute(&mut session, &id, "other.Type")
            .expect("a type the refs own must add cleanly");

        assert_eq!(candidate.def_type, "other.Type");
        let project = session.assignment(&id).expect("still loaded");
        assert_eq!(project.sections().len(), 2);
        assert!(
            project
                .section("other.Type")
                .expect("the new section")
                .is_standalone(),
            "a schema with no TargetKey field is free-standing"
        );
        assert!(use_case.store.last_saved(&id).is_some());
    }

    /// An explicitly requested free-standing (no-`TargetKey`)
    /// section must add cleanly even though the project's own target set
    /// T is already non-empty (the ordinary case once any target-keyed
    /// section exists) — `add_section` (this fixture's own
    /// `example.PartAssignmentDef`) then a free-standing add must not need
    /// T-empty-first, never failing with
    /// `AddAssignmentSectionError::NoValidCandidate("other.Type")`.
    #[test]
    fn adds_a_free_standing_section_when_the_project_already_has_a_non_empty_target_set() {
        let (sources, reader) = fixture();
        let (mut session, id) = fixture_session_with_targets(sources, &["b"]);
        assert!(
            !session
                .assignment(&id)
                .expect("loaded above")
                .targets()
                .is_empty(),
            "the fixture must actually exercise a non-empty T"
        );
        let use_case = AddAssignmentSection::new(InMemoryAssignmentProjectStore::new(), reader);

        let candidate = use_case
            .execute(&mut session, &id, "other.Type")
            .expect("an explicitly requested free-standing type must add even when T is non-empty");

        assert_eq!(candidate.def_type, "other.Type");
        assert!(!candidate.schema.has_target_key());
        let project = session.assignment(&id).expect("still loaded");
        assert_eq!(project.sections().len(), 2);
        assert!(
            project.section("example.PartAssignmentDef").is_some(),
            "the pre-existing target-keyed section must survive the add"
        );
        assert!(
            project
                .section("other.Type")
                .expect("the new section")
                .is_standalone(),
            "a schema with no TargetKey field is free-standing"
        );
    }

    #[test]
    fn refuses_a_type_the_refs_do_not_own() {
        let (sources, reader) = fixture();
        let (mut session, id) = fixture_session(sources);
        let use_case = AddAssignmentSection::new(InMemoryAssignmentProjectStore::new(), reader);

        let result = use_case.execute(&mut session, &id, "nobody.owns.This");

        assert!(matches!(
            result,
            Err(AddAssignmentSectionError::NotOwnedByRefs(_))
        ));
        assert_eq!(
            session
                .assignment(&id)
                .expect("still loaded")
                .sections()
                .len(),
            1
        );
    }

    #[test]
    fn refuses_a_duplicate_section() {
        let (sources, reader) = fixture();
        let (mut session, id) = fixture_session(sources);
        // `assignment_fixture` already seeds a section for `example.PartAssignmentDef`,
        // and this fixture's own `a` genuinely owns real instances of that
        // type too (so the ownership check passes and inference succeeds)
        // — the only thing left to refuse is the duplicate itself.
        let use_case = AddAssignmentSection::new(InMemoryAssignmentProjectStore::new(), reader);

        let result = use_case.execute(&mut session, &id, "example.PartAssignmentDef");

        assert!(matches!(
            result,
            Err(AddAssignmentSectionError::Section(
                SectionError::AlreadyExists(_)
            ))
        ));
        assert_eq!(
            session
                .assignment(&id)
                .expect("still loaded")
                .sections()
                .len(),
            1,
            "the refusal must not touch the project"
        );
    }

    #[test]
    fn a_failed_save_never_makes_the_new_section_visible() {
        let (sources, reader) = fixture();
        let (mut session, id) = fixture_session(sources);
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = AddAssignmentSection::new(store, reader);

        let result = use_case.execute(&mut session, &id, "other.Type");

        assert!(matches!(result, Err(AddAssignmentSectionError::Store(_))));
        assert_eq!(
            session
                .assignment(&id)
                .expect("still loaded")
                .sections()
                .len(),
            1,
            "the new section must be rolled back when the save fails"
        );
    }
}
