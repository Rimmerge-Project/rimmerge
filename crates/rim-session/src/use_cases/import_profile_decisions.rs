//! [`ImportProfileDecisions`]: copies the profile's own `Merge`/`ShipAsset`
//! decisions into a patch project — the one explicit bridge between the
//! two independent decision sets.
//! Never the reverse direction.

use rim_resolve::domain::{Action, FindingKey, PatchDecisionError, PatchId};

use crate::ports::{PatchProjectStore, StoreError};
use crate::{PatchDecideError, Session, UnknownPatch};

/// What [`ImportProfileDecisions::execute`] did: every key it copied in,
/// and every key it skipped along with why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// Keys whose profile decision was copied into the patch.
    pub imported: Vec<FindingKey>,
    /// Keys whose profile decision the patch's scope rejected, and why.
    pub skipped: Vec<(FindingKey, PatchDecisionError)>,
}

/// Everything that can go wrong importing profile decisions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportProfileDecisionsError {
    /// No patch project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// Persisting the imported decisions failed.
    #[error("saving the patch: {0}")]
    Store(StoreError),
}

/// Copies the profile's `Merge`/`ShipAsset` decisions on the given keys
/// (or, when none are given, every key the patch's own scoped ledger
/// admits) into the patch, one save at the end.
pub struct ImportProfileDecisions<Store> {
    store: Store,
}

impl<Store: PatchProjectStore> ImportProfileDecisions<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// See [`ImportProfileDecisionsError`]. On [`ImportProfileDecisionsError::Store`],
    /// every import from this call is rolled back.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &PatchId,
        keys: Option<&[FindingKey]>,
    ) -> Result<ImportReport, ImportProfileDecisionsError> {
        let snapshot = session
            .patch_snapshot(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let source = session.selected();

        let candidate_keys: Vec<FindingKey> = match keys {
            Some(keys) => keys.to_vec(),
            None => session
                .patch_ledger(id, source)?
                .entries
                .iter()
                .map(|entry| entry.key.clone())
                .collect(),
        };

        let mut report = ImportReport::default();
        for key in candidate_keys {
            let Some(profile_decision) = session.decisions().get(&key).cloned() else {
                continue;
            };
            if !matches!(
                profile_decision.action,
                Action::Merge { .. } | Action::ShipAsset { .. }
            ) {
                continue;
            }
            match session.patch_decide(id, profile_decision) {
                Ok(()) => report.imported.push(key),
                Err(PatchDecideError::Invalid(error)) => report.skipped.push((key, error)),
                Err(PatchDecideError::Unknown(unknown)) => {
                    return Err(ImportProfileDecisionsError::Unknown(unknown));
                }
            }
        }

        let project = session
            .patch(id)
            .unwrap_or_else(|| unreachable!("still loaded throughout this call"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, project) {
            session.restore_patch(snapshot);
            return Err(ImportProfileDecisionsError::Store(error));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Decision, DefKey};

    use super::*;
    use crate::test_support::{
        InMemoryDecisionStore, InMemoryPatchProjectStore, patch_fixture, patch_fixture_with_sources,
    };
    use crate::use_cases::Decide;

    fn wall_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        }
    }

    fn out_of_scope_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Door".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("c")].into_iter().collect(),
        }
    }

    fn accept_key() -> FindingKey {
        FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        }
    }

    /// A scope of `{a, b}`: `wall_key` (owners `{a, b}`) is admitted and
    /// decided `Merge` on the profile — must be imported. `out_of_scope_key`
    /// (owners `{a, c}`) is admitted by nothing (`c` isn't in scope and
    /// there's only one scope member, `a`) and decided `Merge` too — must
    /// be rejected by `PatchProject::decide` itself and reported as
    /// skipped, not silently dropped. `accept_key` is an in-scope-shaped
    /// key decided `Accept` (not `Merge`/`ShipAsset`) — filtered out before
    /// ever reaching `decide`, so it appears in neither list.
    #[test]
    fn imports_only_in_scope_merge_and_ship_asset_decisions() {
        let (mut session, id) = patch_fixture(&["a", "b", "c"], &["a", "b"]);
        for (key, action) in [
            (
                wall_key(),
                Action::Merge {
                    key: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Wall".to_string(),
                    },
                    choices: std::collections::BTreeMap::new(),
                },
            ),
            (
                out_of_scope_key(),
                Action::Merge {
                    key: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Door".to_string(),
                    },
                    choices: std::collections::BTreeMap::new(),
                },
            ),
            (accept_key(), Action::Accept),
        ] {
            Decide::new(InMemoryDecisionStore::new())
                .execute(
                    &mut session,
                    Decision {
                        key,
                        action,
                        note: None,
                        decided_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                )
                .expect("every seeded profile decision is valid");
        }
        let use_case = ImportProfileDecisions::new(InMemoryPatchProjectStore::new());

        let report = use_case
            .execute(
                &mut session,
                &id,
                Some(&[wall_key(), out_of_scope_key(), accept_key()]),
            )
            .expect("import must succeed");

        assert_eq!(report.imported, vec![wall_key()]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, out_of_scope_key());
        assert!(matches!(
            report.skipped[0].1,
            rim_resolve::domain::PatchDecisionError::OutOfScope(_)
        ));
        let patch = session.patch(&id).expect("still loaded");
        assert!(patch.decisions().get(&wall_key()).is_some());
        assert!(patch.decisions().get(&out_of_scope_key()).is_none());
        assert!(
            patch.decisions().get(&accept_key()).is_none(),
            "an Accept decision (not Merge/ShipAsset) must never be imported"
        );
    }

    /// `keys: None` means "every key the patch's own scoped ledger
    /// admits" — the enumeration path, never exercised by the other tests
    /// here (which all pass an explicit key list). Needs a report with a
    /// genuine `Conflict::DefOverride` so `patch_ledger` actually has an
    /// entry to enumerate.
    #[test]
    fn imports_every_admitted_key_when_none_are_given() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("ludeon.rimworld")
            .mod_("example.bionicsfork")
            .def_override(
                "ThingDef",
                "Wall",
                &["ludeon.rimworld", "example.bionicsfork"],
            )
            .build();
        let scope = rim_resolve::domain::PatchScope::new([
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ])
        .expect("two distinct members");
        let (mut session, id) = patch_fixture_with_sources(
            rim_analyzer::analysis::SourceIndex::default(),
            report,
            scope,
        );
        let key = FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork"),
            ]
            .into_iter()
            .collect(),
        };
        Decide::new(InMemoryDecisionStore::new())
            .execute(
                &mut session,
                Decision {
                    key: key.clone(),
                    action: Action::Merge {
                        key: DefKey {
                            def_type: "ThingDef".to_string(),
                            def_name: "Wall".to_string(),
                        },
                        choices: std::collections::BTreeMap::new(),
                    },
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("merge is always a valid profile decision");
        let use_case = ImportProfileDecisions::new(InMemoryPatchProjectStore::new());

        let report_outcome = use_case
            .execute(&mut session, &id, None)
            .expect("import must succeed");

        assert_eq!(report_outcome.imported, vec![key.clone()]);
        assert!(report_outcome.skipped.is_empty());
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&key)
                .is_some()
        );
    }

    #[test]
    fn rejects_an_unknown_patch() {
        let (mut session, _id) = patch_fixture(&["a", "b"], &["a", "b"]);
        let use_case = ImportProfileDecisions::new(InMemoryPatchProjectStore::new());
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = use_case.execute(&mut session, &bogus, None);

        assert!(matches!(
            result,
            Err(ImportProfileDecisionsError::Unknown(_))
        ));
    }

    #[test]
    fn a_failed_save_rolls_back_every_import() {
        let (mut session, id) = patch_fixture(&["a", "b"], &["a", "b"]);
        Decide::new(InMemoryDecisionStore::new())
            .execute(
                &mut session,
                Decision {
                    key: wall_key(),
                    action: Action::Merge {
                        key: DefKey {
                            def_type: "ThingDef".to_string(),
                            def_name: "Wall".to_string(),
                        },
                        choices: std::collections::BTreeMap::new(),
                    },
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                },
            )
            .expect("merge is always a valid profile decision");
        let store = InMemoryPatchProjectStore::new();
        store.fail_next_save();
        let use_case = ImportProfileDecisions::new(store);

        let result = use_case.execute(&mut session, &id, Some(&[wall_key()]));

        assert!(matches!(result, Err(ImportProfileDecisionsError::Store(_))));
        assert!(
            session
                .patch(&id)
                .expect("still loaded")
                .decisions()
                .get(&wall_key())
                .is_none(),
            "the import must be rolled back when the save fails"
        );
    }
}
