//! [`DecidePatchMerge`]: the patch twin of [`super::DecideMerge`] —
//! validates and stores a `Merge` decision's per-field choices against
//! that patch's own (scope-restricted) preview, persists it, and
//! refreshes the preview.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    Action, Decision, FieldPath, FindingKey, MergeChoice, MergeState, PatchId,
};

use super::plan_merge::{PlanMerge, PlanMergeError};
use crate::merge_workspace::PreviewSlot;
use crate::ports::{DefSourceReader, PatchProjectStore, StoreError};
use crate::{PatchDecideError, Session, UnknownPatch};

/// Everything that can go wrong deciding a patch merge's field choices.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecidePatchMergeError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// `key` isn't a def-override or patch-collision finding, or building
    /// its (scope-restricted) preview failed.
    #[error("building the preview: {0}")]
    Plan(#[from] PlanMergeError),
    /// A stored path isn't one of this def's actual fields under the
    /// patch's own (scope-restricted) preview.
    #[error("unknown field path: {0}")]
    UnknownField(FieldPath),
    /// A `MergeChoice::From` named a mod that isn't one of this finding's
    /// participants under the patch's scope — already restricted to scope
    /// members (plus Core), so this also catches an out-of-scope owner.
    #[error("{0} is not an owner of this finding")]
    UnknownOwner(ModId),
    /// A `MergeChoice::Drop` on a `PatchCollision` field — same rule as
    /// [`super::decide_merge::DecideMergeError::UnsupportedDrop`], applied
    /// to a patch's own scoped preview.
    #[error("{0}: dropping isn't supported for a patch collision")]
    UnsupportedDrop(FieldPath),
    /// Recording the decision failed [`rim_resolve::domain::PatchProject::decide`]'s
    /// own validation.
    #[error(transparent)]
    Patch(#[from] PatchDecideError),
    /// Persisting the decision failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

/// Validates `choices` against `key`'s current scoped preview, stores the
/// resulting `Action::Merge` decision on the patch (persisting it through
/// [`PatchProjectStore`] with the same rollback-on-save-failure shape as
/// every other mutating use case), refreshes the preview, and returns its
/// new [`MergeState`].
pub struct DecidePatchMerge<Store, Reader> {
    store: Store,
    planner: PlanMerge<Reader>,
}

impl<Store: PatchProjectStore, Reader: DefSourceReader> DecidePatchMerge<Store, Reader> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(store: Store, reader: Reader) -> Self {
        Self {
            store,
            planner: PlanMerge::new(reader),
        }
    }

    /// # Errors
    ///
    /// See [`DecidePatchMergeError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
        key: &FindingKey,
        choices: BTreeMap<FieldPath, MergeChoice>,
    ) -> Result<MergeState, DecidePatchMergeError> {
        let (def_key, is_patch_collision) = match key {
            FindingKey::DefOverride { key: def_key, .. } => (def_key.clone(), false),
            FindingKey::PatchCollision { key: def_key, .. } => (def_key.clone(), true),
            other => {
                return Err(DecidePatchMergeError::Plan(
                    PlanMergeError::UnsupportedFinding(other.clone()),
                ));
            }
        };
        if session.patch(id).is_none() {
            return Err(DecidePatchMergeError::Unknown(UnknownPatch(id.clone())));
        }

        let slot = PreviewSlot::patch(session.selected(), id.clone());
        let ctx = session.merge_context(slot.clone(), key)?;
        let preview = self.planner.execute_in(session, ctx, key)?;

        let valid_paths: BTreeSet<FieldPath> = preview
            .diff
            .fields
            .iter()
            .map(|field| field.path.clone())
            .collect();
        let owners: BTreeSet<ModId> = preview.owners.iter().cloned().collect();
        for (path, choice) in &choices {
            if !valid_paths.contains(path) {
                return Err(DecidePatchMergeError::UnknownField(path.clone()));
            }
            if is_patch_collision && matches!(choice, MergeChoice::Drop) {
                return Err(DecidePatchMergeError::UnsupportedDrop(path.clone()));
            }
            if let MergeChoice::From { mod_id } = choice
                && !owners.contains(mod_id)
            {
                return Err(DecidePatchMergeError::UnknownOwner(mod_id.clone()));
            }
        }

        let snapshot = session
            .patch_snapshot(id)
            .unwrap_or_else(|| unreachable!("checked above"));
        let decision = Decision {
            key: key.clone(),
            action: Action::Merge {
                key: def_key,
                choices,
            },
            note: None,
            decided_at: jiff::Timestamp::now(),
        };
        session.patch_decide(id, decision)?;

        let project = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("still loaded right after patch_decide succeeded"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, project) {
            session.restore_patch(snapshot);
            return Err(DecidePatchMergeError::Store(error));
        }

        let refreshed_ctx = session.merge_context(slot, key)?;
        let refreshed = self.planner.execute_in(session, refreshed_ctx, key)?;
        Ok(refreshed.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::Selector;
    use rim_resolve::domain::{DefKey, PatchScope};

    use super::*;
    use crate::test_support::{
        InMemoryPatchProjectStore, bionic_heart_fixture, bionic_heart_fixture_flat,
        patch_fixture_with_sources, patch_fixture_with_sources_and_mods, two_mod_list_fixture,
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
            sub_path: Some("label".to_string()),
            mods: [ModId::new("a.mod"), ModId::new("b.mod")]
                .into_iter()
                .collect(),
        }
    }

    /// Same rule as `DecideMerge`'s own regression test, applied to a
    /// patch's own scoped preview: a `Drop` on a `PatchCollision` field can
    /// never replay, so it must be rejected before it's ever stored.
    /// `two_mod_list_fixture` (not `whole_def_patch_collision_fixture`,
    /// which has only one contributing mod): the scoped participant rule
    /// needs at least two scope
    /// members actually contributing to produce a real (non-`CannotMerge`)
    /// diff at all.
    #[test]
    fn rejects_a_drop_choice_on_a_patch_collision() {
        let fixture = two_mod_list_fixture();
        let scope = PatchScope::new([ModId::new("a.mod"), ModId::new("b.mod")])
            .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources_and_mods(
            fixture.sources,
            fixture.report,
            scope,
            &["core.mod", "a.mod", "b.mod"],
        );
        let use_case = DecidePatchMerge::new(InMemoryPatchProjectStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert("label".parse().unwrap(), MergeChoice::Drop);

        let result = use_case.execute(&mut session, &id, &widget_collision_key(), choices);

        assert!(matches!(
            result,
            Err(DecidePatchMergeError::UnsupportedDrop(_))
        ));
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&widget_collision_key())
                .is_none(),
            "a rejected choice must never be recorded on the patch either"
        );
    }

    #[test]
    fn rejects_an_unknown_field_path() {
        let fixture = bionic_heart_fixture();
        let scope = PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let use_case = DecidePatchMerge::new(InMemoryPatchProjectStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "not/a/real/field".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        let result = use_case.execute(&mut session, &id, &finding_key(), choices);

        assert!(matches!(
            result,
            Err(DecidePatchMergeError::UnknownField(_))
        ));
    }

    #[test]
    fn stores_a_valid_choice_and_returns_the_refreshed_state() {
        // `bionic_heart_fixture_flat`, not `bionic_heart_fixture`: this
        // test is about the choice-storage mechanics, not the structural guard — see the
        // flat fixture's own doc comment.
        let fixture = bionic_heart_fixture_flat();
        let scope = PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let store = InMemoryPatchProjectStore::new();
        let use_case = DecidePatchMerge::new(store, fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        let state = use_case
            .execute(&mut session, &id, &finding_key(), choices)
            .expect("a valid choice must be accepted");

        assert!(matches!(state, MergeState::Complete { .. }));
        assert!(use_case.store.last_saved(&id).is_some());
        assert!(matches!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&finding_key())
                .map(|d| &d.action),
            Some(Action::Merge { .. })
        ));
        // The profile's own decisions must stay untouched by a patch
        // decision.
        assert!(session.decisions().get(&finding_key()).is_none());
    }

    /// The same guarantee `DecidePatch`'s own regression test proves for a
    /// plain decision: a patch's `Merge` decision — even one that builds
    /// and caches a real scoped preview — must never touch the profile's
    /// own ledger stats or sort outcome.
    #[test]
    fn a_patch_merge_decision_never_changes_the_profiles_own_ledger_or_sort() {
        let fixture = bionic_heart_fixture();
        let scope = PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let source = session.selected();
        let stats_before = session.ledger(source).stats;
        let sort_before = session.sort_outcome().clone();
        let use_case = DecidePatchMerge::new(InMemoryPatchProjectStore::new(), fixture.reader);
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        use_case
            .execute(&mut session, &id, &finding_key(), choices)
            .expect("a valid choice must be accepted");

        assert_eq!(
            session.ledger(source).stats,
            stats_before,
            "a patch merge decision must never touch the profile's own ledger stats"
        );
        assert_eq!(
            session.sort_outcome(),
            &sort_before,
            "a patch merge decision must never re-sort the profile"
        );
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let fixture = bionic_heart_fixture();
        let scope = PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, _id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let use_case = DecidePatchMerge::new(InMemoryPatchProjectStore::new(), fixture.reader);
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, &finding_key(), BTreeMap::new());

        assert!(matches!(result, Err(DecidePatchMergeError::Unknown(_))));
    }

    #[test]
    fn a_failed_save_rolls_back_the_decision() {
        let fixture = bionic_heart_fixture();
        let scope = PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let use_case = DecidePatchMerge::new(store, fixture.reader);

        let result = use_case.execute(&mut session, &id, &finding_key(), BTreeMap::new());

        assert!(matches!(result, Err(DecidePatchMergeError::Store(_))));
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&finding_key())
                .is_none(),
            "the decision must be rolled back when the save fails"
        );
    }
}
