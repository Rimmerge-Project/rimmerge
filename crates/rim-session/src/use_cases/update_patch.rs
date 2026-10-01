//! [`UpdatePatch`]: applies any of a patch's mutable fields — identity,
//! scope, name, author, description — with the same
//! snapshot/persist/rollback-on-save-failure shape as every other
//! mutating use case in this crate.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    PatchId, PatchIdentityError, PatchModIdentity, PatchScope, PatchScopeError, ScopeChange,
};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{Session, UnknownPatch};

/// Any of a patch's mutable fields — `None` means "leave unchanged".
/// `package_id`/`display_name` are validated together (through
/// [`PatchModIdentity::new`]) whenever either is given, since a validated
/// identity needs both.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePatchInput {
    /// A new list label.
    pub name: Option<String>,
    /// A new published package id.
    pub package_id: Option<String>,
    /// A new published display name.
    pub display_name: Option<String>,
    /// A new `About.xml` author.
    pub author: Option<String>,
    /// A new `About.xml` description.
    pub description: Option<String>,
    /// A new scope.
    pub scope: Option<BTreeSet<ModId>>,
}

/// What [`UpdatePatch::execute`] did to the project's existing decisions,
/// when `scope` was part of the update — `None` when the scope wasn't
/// touched.
pub type UpdatePatchOutcome = Option<ScopeChange>;

/// Everything that can go wrong updating a patch — the same validation
/// [`super::CreatePatch`] applies, since a package id or scope member can
/// become invalid on an edit exactly as it could on creation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdatePatchError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// The new `package_id`/`display_name` failed
    /// [`PatchModIdentity::new`]'s own validation.
    #[error(transparent)]
    Identity(#[from] PatchIdentityError),
    /// Fewer than two distinct scope members were given.
    #[error(transparent)]
    Scope(#[from] PatchScopeError),
    /// A new scope member isn't currently active.
    #[error("{0} is not an active mod")]
    InactiveScopeMember(ModId),
    /// A new scope member is a Rimmerge-generated mod.
    #[error("{0} is a Rimmerge-generated mod and can't be a patch scope member")]
    GeneratedScopeMember(ModId),
    /// The new package id collides with an active mod's base id or
    /// another project's own package id.
    #[error("package id {0} is already used by an active mod or another patch")]
    PackageIdTaken(ModId),
    /// Persisting the update failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

/// Applies any of [`UpdatePatchInput`]'s given fields to one patch
/// project.
pub struct UpdatePatch<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> UpdatePatch<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`UpdatePatchError`]. On any error, `session`'s own copy of the
    /// project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
        input: UpdatePatchInput,
    ) -> Result<UpdatePatchOutcome, UpdatePatchError> {
        let snapshot = session
            .patch_snapshot(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let mut project = snapshot.clone();

        if let Some(name) = input.name {
            project.set_name(name);
        }

        if input.package_id.is_some() || input.display_name.is_some() {
            let package_id = input
                .package_id
                .as_deref()
                .unwrap_or_else(|| project.identity().package_id().as_str());
            let display_name = input
                .display_name
                .as_deref()
                .unwrap_or_else(|| project.identity().display_name());
            let identity = PatchModIdentity::new(package_id, display_name)?;

            let new_base = identity.package_id().base();
            let unchanged = new_base == project.identity().package_id().base();
            if !unchanged {
                let active = session.active_base_ids();
                let reserved = session.reserved_package_ids();
                if active.contains(&new_base) || reserved.contains(&new_base) {
                    return Err(UpdatePatchError::PackageIdTaken(
                        identity.package_id().clone(),
                    ));
                }
            }
            project.set_identity(identity);
        }

        if let Some(author) = input.author {
            project.set_author(author);
        }
        if let Some(description) = input.description {
            project.set_description(description);
        }

        let scope_change = if let Some(scope_ids) = input.scope {
            let active = session.active_base_ids();
            let reserved = session.reserved_package_ids();
            for mod_id in &scope_ids {
                let base = mod_id.base();
                if !active.contains(&base) {
                    return Err(UpdatePatchError::InactiveScopeMember(mod_id.clone()));
                }
                if reserved.contains(&base) {
                    return Err(UpdatePatchError::GeneratedScopeMember(mod_id.clone()));
                }
            }
            let scope = PatchScope::new(scope_ids)?;
            Some(project.set_scope(scope))
        } else {
            None
        };

        session.upsert_patch(project);
        let stored = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_patch(snapshot);
            return Err(UpdatePatchError::Store(error));
        }
        Ok(scope_change)
    }
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::Action;

    use super::*;
    use crate::test_support::{InMemoryPatchProjectStore, patch_fixture, session_fixture};

    #[test]
    fn renames_a_patch_and_persists_it() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());

        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdatePatchInput {
                    name: Some("renamed".to_string()),
                    ..UpdatePatchInput::default()
                },
            )
            .expect("rename must succeed");

        assert!(outcome.is_none(), "no scope change was requested");
        assert_eq!(
            session
                .patch(&id)
                .map(rim_resolve::domain::PatchProject::name),
            Some("renamed")
        );
    }

    /// Editing only the display name (no `package_id` given) must never
    /// trip `PackageIdTaken` against the project's *own* current id: the
    /// defaulting logic that fills in the unchanged half of the identity
    /// must compare the new base against the project's own base and treat
    /// that as `unchanged`, not as a collision.
    #[test]
    fn a_display_name_only_edit_does_not_trip_package_id_taken_against_its_own_id() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());

        use_case
            .execute(
                &mut session,
                &id,
                UpdatePatchInput {
                    display_name: Some("New Display Name".to_string()),
                    ..UpdatePatchInput::default()
                },
            )
            .expect("a display-name-only edit must never collide with the project's own id");

        let project = session.patch(&id).expect("still loaded");
        assert_eq!(project.identity().display_name(), "New Display Name");
        assert_eq!(project.identity().package_id(), &ModId::new("test.patch"));
    }

    /// Changing `package_id` to one already claimed by an active mod is
    /// rejected exactly like [`super::super::CreatePatch`]'s own check.
    #[test]
    fn rejects_a_package_id_already_used_by_an_active_mod_on_update() {
        let (mut session, id) = patch_fixture(&["a", "b", "vendor.moda"], &["a", "b"]);
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());

        let result = use_case.execute(
            &mut session,
            &id,
            UpdatePatchInput {
                package_id: Some("vendor.moda".to_string()),
                ..UpdatePatchInput::default()
            },
        );

        assert!(matches!(result, Err(UpdatePatchError::PackageIdTaken(_))));
    }

    /// A new scope naming a mod that isn't currently active is rejected
    /// exactly like [`super::super::CreatePatch`]'s own check.
    #[test]
    fn rejects_an_inactive_scope_member_on_update() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());

        let result = use_case.execute(
            &mut session,
            &id,
            UpdatePatchInput {
                scope: Some([ModId::new("a"), ModId::new("gone")].into_iter().collect()),
                ..UpdatePatchInput::default()
            },
        );

        assert_eq!(
            result,
            Err(UpdatePatchError::InactiveScopeMember(ModId::new("gone")))
        );
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, UpdatePatchInput::default());

        assert!(matches!(result, Err(UpdatePatchError::Unknown(_))));
    }

    #[test]
    fn shrinking_the_scope_reports_the_scope_change() {
        let (mut session, id) = patch_fixture(&["a", "b", "c", "d"], &["a", "b", "c"]);
        session
            .patch_decide(
                &id,
                rim_resolve::domain::Decision {
                    key: rim_resolve::domain::FindingKey::DefOverride {
                        key: rim_resolve::domain::DefKey {
                            def_type: "ThingDef".to_string(),
                            def_name: "Wall".to_string(),
                        },
                        owners: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
                            .into_iter()
                            .collect(),
                    },
                    action: Action::Ignore,
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("the scope admits this finding");
        let use_case = UpdatePatch::new(InMemoryPatchProjectStore::new());

        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdatePatchInput {
                    scope: Some([ModId::new("a"), ModId::new("d")].into_iter().collect()),
                    ..UpdatePatchInput::default()
                },
            )
            .expect("shrinking the scope must still succeed");

        let change = outcome.expect("a scope change was requested");
        assert!(
            !change.now_orphaned.is_empty(),
            "the Wall decision must be orphaned once b/c leave scope"
        );
    }

    #[test]
    fn a_failed_save_rolls_back_every_field() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let use_case = UpdatePatch::new(store);

        let result = use_case.execute(
            &mut session,
            &id,
            UpdatePatchInput {
                name: Some("renamed".to_string()),
                ..UpdatePatchInput::default()
            },
        );

        assert!(matches!(result, Err(UpdatePatchError::Store(_))));
        assert_ne!(
            session
                .patch(&id)
                .map(rim_resolve::domain::PatchProject::name),
            Some("renamed"),
            "the rename must be rolled back when the save fails"
        );
    }
}
