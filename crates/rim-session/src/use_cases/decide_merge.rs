//! [`DecideMerge`]: validates and stores a `Merge` decision's per-field
//! choices, persists it, and refreshes the preview.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Action, Decision, FieldPath, FindingKey, MergeChoice, MergeState};

use super::plan_merge::{PlanMerge, PlanMergeError};
use crate::Session;
use crate::ports::{DecisionStore, DefSourceReader, StoreError};

/// Everything that can go wrong deciding a merge's field choices.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecideMergeError {
    /// `key` isn't a def-override or patch-collision finding, or building
    /// its preview failed.
    #[error("building the preview: {0}")]
    Plan(#[from] PlanMergeError),
    /// A stored path isn't one of this def's actual fields.
    #[error("unknown field path: {0}")]
    UnknownField(FieldPath),
    /// A `MergeChoice::From` named a mod that isn't one of this finding's
    /// owners.
    #[error("{0} is not an owner of this finding")]
    UnknownOwner(ModId),
    /// A `MergeChoice::Drop` on a `PatchCollision` field — RimWorld's own
    /// patches replay to a value; there is no "drop" operation a
    /// collision's replay can express (see
    /// `rim_merge::plan::Caveat::UnsupportedDrop`, which `plan_patch_collision`
    /// already reports as a caveat on such a choice). Rejected here too,
    /// before the choice is ever stored, so the merge editor's Drop button
    /// never persists a choice it can't actually apply
    ///
    #[error("{0}: dropping isn't supported for a patch collision")]
    UnsupportedDrop(FieldPath),
    /// Persisting the decision failed.
    #[error("saving the decision: {0}")]
    Store(StoreError),
}

/// Validates `choices` against `key`'s current preview, stores the
/// resulting `Action::Merge` decision (persisting it through
/// [`DecisionStore`] with the same rollback-on-save-failure shape as
/// [`super::Decide`]), refreshes the preview, and returns its new
/// [`MergeState`].
pub struct DecideMerge<Decisions, Reader> {
    decision_store: Decisions,
    planner: PlanMerge<Reader>,
}

impl<Decisions: DecisionStore, Reader: DefSourceReader> DecideMerge<Decisions, Reader> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(decision_store: Decisions, reader: Reader) -> Self {
        Self {
            decision_store,
            planner: PlanMerge::new(reader),
        }
    }

    /// # Errors
    ///
    /// See [`DecideMergeError`].
    pub fn execute(
        &self,
        session: &mut Session,
        key: &FindingKey,
        choices: BTreeMap<FieldPath, MergeChoice>,
    ) -> Result<MergeState, DecideMergeError> {
        let (def_key, is_patch_collision) = match key {
            FindingKey::DefOverride { key: def_key, .. } => (def_key.clone(), false),
            FindingKey::PatchCollision { key: def_key, .. } => (def_key.clone(), true),
            other => {
                return Err(DecideMergeError::Plan(PlanMergeError::UnsupportedFinding(
                    other.clone(),
                )));
            }
        };

        let preview = self.planner.execute(session, key)?;
        let valid_paths: BTreeSet<FieldPath> = preview
            .diff
            .fields
            .iter()
            .map(|field| field.path.clone())
            .collect();
        let owners: BTreeSet<ModId> = preview.owners.iter().cloned().collect();
        for (path, choice) in &choices {
            if !valid_paths.contains(path) {
                return Err(DecideMergeError::UnknownField(path.clone()));
            }
            if is_patch_collision && matches!(choice, MergeChoice::Drop) {
                return Err(DecideMergeError::UnsupportedDrop(path.clone()));
            }
            if let MergeChoice::From { mod_id } = choice
                && !owners.contains(mod_id)
            {
                return Err(DecideMergeError::UnknownOwner(mod_id.clone()));
            }
        }

        let snapshot = session.decisions_snapshot();
        let decision = Decision {
            key: key.clone(),
            action: Action::Merge {
                key: def_key,
                choices,
            },
            note: None,
            decided_at: jiff::Timestamp::now(),
        };
        // `Action::Merge` always validates (see `ResolveError`'s own doc
        // comment: it's an empty enum today), so this can't actually fail
        // — `unwrap_or_else` names the invariant rather than propagating
        // an unreachable error variant through this method's signature.
        session
            .decide(decision)
            .unwrap_or_else(|error| match error {});

        if let Err(error) = self
            .decision_store
            .save(&session.paths().profile_dir, session.decisions())
        {
            session.restore_decisions(snapshot);
            return Err(DecideMergeError::Store(error));
        }

        let refreshed = self.planner.execute(session, key)?;
        Ok(refreshed.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::{ModId, Selector};
    use rim_resolve::domain::DefKey;

    use super::*;
    use crate::test_support::{
        InMemoryDecisionStore, bionic_heart_fixture, bionic_heart_fixture_flat,
        session_with_sources, session_with_sources_and_mods, whole_def_patch_collision_fixture,
    };

    fn finding_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "HediffDef".to_string(),
                def_name: "BionicHeart".to_string(),
            },
            owners: [
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork"),
            ]
            .into_iter()
            .collect(),
        }
    }

    fn widget_collision_key() -> FindingKey {
        FindingKey::PatchCollision {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Widget".to_string(),
            },
            selector: Selector::DefName,
            sub_path: None,
            mods: [ModId::new("a.mod")].into_iter().collect(),
        }
    }

    #[test]
    fn rejects_an_unknown_field_path() {
        let fixture = bionic_heart_fixture();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let use_case = DecideMerge::new(InMemoryDecisionStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "not/a/real/field".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        let result = use_case.execute(&mut session, &finding_key(), choices);

        assert!(matches!(result, Err(DecideMergeError::UnknownField(_))));
    }

    #[test]
    fn rejects_a_from_choice_naming_a_non_owner() {
        let fixture = bionic_heart_fixture();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let use_case = DecideMerge::new(InMemoryDecisionStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("not.an.owner"),
            },
        );

        let result = use_case.execute(&mut session, &finding_key(), choices);

        assert!(matches!(result, Err(DecideMergeError::UnknownOwner(_))));
    }

    /// A patch collision can never replay a `Drop`
    /// (`rim_merge::plan::Caveat::UnsupportedDrop` — patches replay to a
    /// value, there's no "drop" operation), so storing one would leave the
    /// row silently stuck on "absent" with only a header caveat to explain
    /// why. It must be rejected up front instead, before anything is
    /// persisted.
    #[test]
    fn rejects_a_drop_choice_on_a_patch_collision() {
        let fixture = whole_def_patch_collision_fixture();
        let mut session =
            session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "a.mod"]);
        let use_case = DecideMerge::new(InMemoryDecisionStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert("".parse().unwrap(), MergeChoice::Drop);

        let result = use_case.execute(&mut session, &widget_collision_key(), choices);

        assert!(matches!(result, Err(DecideMergeError::UnsupportedDrop(_))));
        assert!(
            use_case.decision_store.last_saved().is_none(),
            "a rejected choice must never reach the store"
        );
        assert!(
            session.decisions().get(&widget_collision_key()).is_none(),
            "a rejected choice must never be recorded on the session either"
        );
    }

    #[test]
    fn stores_a_valid_choice_and_returns_the_refreshed_state() {
        // `bionic_heart_fixture_flat`, not `bionic_heart_fixture`: this
        // test is about the choice-storage mechanics, not the structural guard — see the
        // flat fixture's own doc comment.
        let fixture = bionic_heart_fixture_flat();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let store = InMemoryDecisionStore::new();
        let use_case = DecideMerge::new(store, fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        let state = use_case
            .execute(&mut session, &finding_key(), choices)
            .expect("a valid choice must be accepted");

        assert!(matches!(state, MergeState::Complete { .. }));
        assert!(
            use_case.decision_store.last_saved().is_some(),
            "the decision must reach the store"
        );
        assert!(matches!(
            session.decisions().get(&finding_key()).map(|d| &d.action),
            Some(Action::Merge { .. })
        ));
    }

    /// A [`DefSourceReader`] that counts every
    /// [`DefSourceReader::read_element`] call it forwards — see
    /// `plan_merge`'s own identically-named test helper; duplicated here
    /// (not shared) since this one only backs a single measurement test,
    /// per this crate's own "tolerate minor duplication" convention.
    struct CountingReader<R> {
        inner: R,
        reads: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl<R: DefSourceReader> DefSourceReader for CountingReader<R> {
        fn read_element(
            &self,
            locator: &rim_analyzer::domain::XmlLocator,
            expected: &crate::ports::ElementExpectation,
        ) -> Result<String, crate::ports::DefSourceError> {
            self.reads.set(self.reads.get() + 1);
            self.inner.read_element(locator, expected)
        }
    }

    /// A measurement:
    /// how many source reads (each one also a fresh XML parse and
    /// `rim_merge::patch_eval::replay` in the real reader) one "click" —
    /// one [`DecideMerge::execute`] call — costs, in steady state (the
    /// frontend's `useMergePreviewQuery` has already fetched and cached
    /// this finding's preview once, exactly as the merge editor page
    /// always has one open before a user can click anything in it).
    ///
    /// Not a regression test on its own (the number itself isn't a
    /// contract) — run with `--nocapture` to see the reported counts; kept
    /// here, not deleted, so the next person touching this cache can
    /// re-run the same measurement instead of re-deriving it from scratch.
    #[test]
    fn measures_source_reads_per_click_in_steady_state() {
        use crate::test_support::two_mod_list_fixture;

        let fixture = two_mod_list_fixture();
        let mut session = session_with_sources_and_mods(
            fixture.sources,
            fixture.report,
            &["core.mod", "a.mod", "b.mod"],
        );
        let reads = std::rc::Rc::new(std::cell::Cell::new(0));
        let reader = CountingReader {
            inner: fixture.reader,
            reads: reads.clone(),
        };
        let key = FindingKey::PatchCollision {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Widget".to_string(),
            },
            selector: rim_analyzer::domain::Selector::DefName,
            sub_path: Some("label".to_string()),
            mods: [ModId::new("a.mod"), ModId::new("b.mod")]
                .into_iter()
                .collect(),
        };

        // Steady state: the editor's own preview query already built and
        // cached this finding once before the user can click anything.
        let planner = PlanMerge::new(&reader);
        planner
            .execute(&mut session, &key)
            .expect("priming the cache must succeed");
        reads.set(0);

        // One click: pick `a.mod`'s value for the contested `label` field.
        let use_case = DecideMerge::new(InMemoryDecisionStore::new(), &reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("a.mod"),
            },
        );
        use_case
            .execute(&mut session, &key, choices)
            .expect("the click must succeed");

        eprintln!(
            "merge-collision read-cost measurement: {} source read(s) for one click (two_mod_list_fixture, 2-mod collision), steady state",
            reads.get()
        );
    }
}
