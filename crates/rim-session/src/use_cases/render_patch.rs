//! [`RenderPatch`]: renders one compat patch's own merge mod candidate —
//! what `patch plan`, the export panel, and the file preview call. Never
//! writes.

use rim_resolve::domain::PatchId;

use super::render_merge_mod::{MergeModRender, RenderMergeMod, RenderMergeModError, RenderTarget};
use crate::ports::{AssetLocator, DefSourceReader};
use crate::{Session, UnknownPatch};

/// Everything that can go wrong rendering a patch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderPatchError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// Rendering itself failed.
    #[error(transparent)]
    Render(#[from] RenderMergeModError),
}

/// Renders `id`'s own merge mod candidate through
/// [`RenderMergeMod::render`]`(`[`RenderTarget::Patch`]`)`. Never writes
/// anything to disk.
pub struct RenderPatch<Reader, Assets> {
    render: RenderMergeMod<Reader, Assets>,
}

impl<Reader: DefSourceReader, Assets: AssetLocator> RenderPatch<Reader, Assets> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(reader: Reader, asset_locator: Assets) -> Self {
        Self {
            render: RenderMergeMod::new(reader, asset_locator),
        }
    }

    /// # Errors
    ///
    /// See [`RenderPatchError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
    ) -> Result<MergeModRender, RenderPatchError> {
        let project = session
            .patch(id)
            .cloned()
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        Ok(self.render.render(session, RenderTarget::Patch(&project))?)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, Decision, DefKey, FindingKey};

    use super::*;
    use crate::test_support::{
        FakeAssetLocator, bionic_heart_fixture, five_owner_scope_fixture,
        patch_fixture_with_sources, session_with_sources_and_mods,
    };

    fn five_owner_wall_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [
                ModId::new("ludeon.rimworld"),
                ModId::new("a.mod"),
                ModId::new("b.mod"),
                ModId::new("c.mod"),
                ModId::new("d.mod"),
            ]
            .into_iter()
            .collect(),
        }
    }

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

    #[test]
    fn renders_exactly_the_scope_as_dependencies_with_a_stable_hash() {
        let fixture = bionic_heart_fixture();
        let scope = rim_resolve::domain::PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        session
            .patch_decide(
                &id,
                Decision {
                    key: finding_key(),
                    action: Action::Merge {
                        key: DefKey {
                            def_type: "HediffDef".to_string(),
                            def_name: "BionicHeart".to_string(),
                        },
                        choices: std::collections::BTreeMap::new(),
                    },
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("merge is always a valid patch action");
        let use_case = RenderPatch::new(fixture.reader, FakeAssetLocator::default());

        let first = use_case
            .execute(&mut session, &id)
            .expect("rendering must succeed");
        let second = use_case
            .execute(&mut session, &id)
            .expect("rendering twice must still succeed");

        assert_eq!(
            first.dependencies,
            [
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork")
            ]
            .into_iter()
            .collect(),
            "a patch render always declares exactly its own scope"
        );
        assert_eq!(
            first.decisions_sha256, second.decisions_sha256,
            "the same decisions must hash the same across renders"
        );
        assert_eq!(
            first.decisions_sha256,
            session.patch(&id).expect("still loaded").decisions_sha256()
        );
    }

    /// The five-owner worked example, scoped to `{b.mod, d.mod}` and
    /// decided with a choice-less `Merge` (no per-field overrides):
    /// `b.mod`'s `statBases/MaxHitPoints` change is one-sided and must be
    /// carried forward as a real op, so rendering must succeed with exactly
    /// one op — the emitter's own dependency check must compare that op's
    /// `depends_on` (computed from the scope-restricted diff) against the
    /// scope-restricted participants, never the *full five-owner* diff's,
    /// which would refuse it as `UndeclaredDependency`.
    #[test]
    fn renders_the_five_owner_worked_example_with_one_real_op() {
        let fixture = five_owner_scope_fixture();
        let mut session = session_with_sources_and_mods(
            fixture.sources,
            fixture.report,
            &["ludeon.rimworld", "a.mod", "b.mod", "c.mod", "d.mod"],
        );
        let scope =
            rim_resolve::domain::PatchScope::new([ModId::new("b.mod"), ModId::new("d.mod")])
                .expect("two distinct members");
        let identity = rim_resolve::domain::PatchModIdentity::new("test.bdcompat", "BD Compat")
            .expect("valid identity");
        let id: PatchId = "abcdef012345".parse().expect("valid patch id");
        let mut project = rim_resolve::domain::PatchProject::new(
            id.clone(),
            "BD Compat Project".to_string(),
            identity,
            scope,
            jiff::Timestamp::UNIX_EPOCH,
        );
        project
            .decide(Decision {
                key: five_owner_wall_key(),
                action: Action::Merge {
                    key: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Wall".to_string(),
                    },
                    choices: std::collections::BTreeMap::new(),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("the scope admits this finding");
        session.upsert_patch(project);
        let use_case = RenderPatch::new(fixture.reader, FakeAssetLocator::default());

        let render = use_case
            .execute(&mut session, &id)
            .expect("rendering must succeed");

        assert!(
            render.rendered.is_some(),
            "a genuine, non-empty scoped merge must render"
        );
        assert_eq!(render.entries.len(), 1);
        assert_eq!(render.entries[0].op_count, 1);
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let fixture = bionic_heart_fixture();
        let scope = rim_resolve::domain::PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, _id) = patch_fixture_with_sources(fixture.sources, fixture.report, scope);
        let use_case = RenderPatch::new(fixture.reader, FakeAssetLocator::default());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus);

        assert!(matches!(result, Err(RenderPatchError::Unknown(_))));
    }
}
