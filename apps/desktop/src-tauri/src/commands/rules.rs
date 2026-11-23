//! `list_rules`/`import_rimsort`/`upsert_rule`/`delete_rule`/
//! `list_orphaned_decisions`: the rules page's commands.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rim_resolve::domain::{OrderSource, Rule};
use rim_session::ports::{RimSortImporter, RimSortPaths};
use rim_session::use_cases::ImportRimSort;
use rim_session::{RuleKey, Session};

use tauri::async_runtime::spawn_blocking;

use crate::commands::emit_session_changed;
use crate::dto::finding::OrphanedDecisionDto;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::rule::{
    DeleteRuleRequestDto, ImportReportDto, RimSortPathsDto, RuleDto, RuleFilterDto, RuleKeyDto,
    RuleSetDto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Builds a [`RuleSetDto`] from `session`'s current rules plus its
/// [`Session::rule_load_warnings`] — every rules-page command builds its
/// response this way (not just `list_rules`) so a load warning stays
/// visible through the very next mutation, not only immediately after
/// `load_project`.
fn rule_set_dto(session: &Session) -> RuleSetDto {
    RuleSetDto {
        warnings: session
            .rule_load_warnings()
            .map(ToString::to_string)
            .collect(),
        ..session.rules().into()
    }
}

/// Lists every rule in effect, optionally filtered to one origin.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_rules_inner(
    state: &AppState,
    filter: RuleFilterDto,
) -> Result<RuleSetDto, CommandError> {
    with_session(state, move |session| {
        let mut rules = rule_set_dto(session);
        if let Some(origin) = filter.origin {
            rules.pairs.retain(|rule| rule.origin == origin);
            rules.placements.retain(|rule| rule.origin == origin);
            rules.incompatibles.retain(|rule| rule.origin == origin);
        }
        Ok(rules)
    })
    .await
}

/// See [`list_rules_inner`].
///
/// # Errors
///
/// See [`list_rules_inner`].
#[tauri::command]
pub async fn list_rules(
    state: tauri::State<'_, AppState>,
    filter: RuleFilterDto,
) -> Result<RuleSetDto, CommandError> {
    list_rules_inner(&state, filter).await
}

/// Imports RimSort's three database files, replacing any previously
/// imported rules.
///
/// **Lock protocol** (the `file_baseline` shape): a short `with_session`
/// probes what the import reads from the session (the active-mod map and
/// the profile folder the importer snapshots into), the files are read and
/// parsed on a bare `spawn_blocking` with the lock released (a Steam database
/// takes seconds), and a second short `with_session` applies the result, saves
/// the rules and records the manifest. The commit refuses with
/// `project_changed` when the session no longer has the profile folder or
/// the active-mod map the read was made against: the rules were matched to
/// those mods, so applying them to another project (or to one a rescan
/// changed) would be wrong, and nothing has been written yet at that point
/// but the importer's own snapshot copies under the profile it was told.
///
/// # Errors
///
/// Returns `network_path_refused` for a network share and
/// [`CommandError::invalid_input`] for a relative or device path, `project_changed`
/// when the project changed during the read, or [`CommandError`] when a
/// database file can't be read, or when saving the merged rules fails.
pub(crate) async fn import_rimsort_inner(
    state: &AppState,
    paths: RimSortPathsDto,
) -> Result<ImportReportDto, CommandError> {
    import_rimsort_with(state, paths, rim_io::RimSortImporter::new).await
}

/// [`import_rimsort_inner`] over an importer built from the profile folder by
/// `make_importer` (generic so a test can slow the read down).
///
/// # Errors
///
/// See `import_rimsort_inner`.
pub(crate) async fn import_rimsort_with<Importer>(
    state: &AppState,
    paths: RimSortPathsDto,
    make_importer: impl FnOnce(PathBuf) -> Importer,
) -> Result<ImportReportDto, CommandError>
where
    Importer: RimSortImporter + Send + 'static,
{
    let paths = RimSortPaths::try_from(paths)?;
    let (active, profile_dir) = with_session(state, |session| {
        Ok((
            session.active_mods_by_base_id(),
            session.paths().profile_dir.clone(),
        ))
    })
    .await?;
    let use_case = ImportRimSort::new(
        make_importer(profile_dir.clone()),
        state.adapters.rule_store,
        rim_io::JsonImportManifestStore::new(),
    );
    let (use_case, read, active) = spawn_blocking(move || {
        let read = use_case.read(&paths, &active);
        (use_case, read, active)
    })
    .await
    .map_err(|join_error| {
        tracing::warn!(error = %join_error, "RimSort import read task panicked");
        CommandError::internal("background task panicked")
    })?;
    let imported = read?;
    with_session(state, move |session| {
        if session.paths().profile_dir != profile_dir || session.active_mods_by_base_id() != active
        {
            return Err(CommandError::project_changed());
        }
        let applied = use_case.apply(session, imported)?;
        Ok((&applied).into())
    })
    .await
}

/// See [`import_rimsort_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`import_rimsort_inner`].
#[tauri::command]
pub async fn import_rimsort(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    paths: RimSortPathsDto,
) -> Result<ImportReportDto, CommandError> {
    let result = import_rimsort_inner(&state, paths).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Imported);
    }
    result
}

/// Adds or replaces a rule, persisting the change.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `rule` fails domain
/// validation, or [`CommandError`] when saving fails.
pub(crate) async fn upsert_rule_inner(
    state: &AppState,
    rule: RuleDto,
) -> Result<RuleSetDto, CommandError> {
    let rule_store = state.adapters.rule_store;
    with_session(state, move |session| {
        let rule: Rule = rule.try_into()?;
        let use_case = rim_session::use_cases::UpsertRule::new(rule_store);
        use_case.execute(session, rule)?;
        Ok(rule_set_dto(session))
    })
    .await
}

/// See [`upsert_rule_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`upsert_rule_inner`].
#[tauri::command]
pub async fn upsert_rule(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    rule: RuleDto,
) -> Result<RuleSetDto, CommandError> {
    let result = upsert_rule_inner(&state, rule).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::RuleUpserted);
    }
    result
}

/// Removes a rule, persisting the change. `request.origin`, when given,
/// restricts the deletion to that origin's own row — so deleting an
/// imported rule's row never also removes a promoted `UserDecision` copy
/// sharing the same [`RuleKey`].
///
/// # Errors
///
/// Returns [`CommandError`] when saving fails. `request.key -> RuleKey` is
/// an infallible field-for-field mapping (`ModId::new` never fails), so
/// this never actually returns `invalid_input`.
pub(crate) async fn delete_rule_inner(
    state: &AppState,
    request: DeleteRuleRequestDto,
) -> Result<RuleSetDto, CommandError> {
    let rule_store = state.adapters.rule_store;
    with_session(state, move |session| {
        let key: RuleKey = request.key.try_into()?;
        let origin = request.origin.map(rim_resolve::domain::RuleOrigin::from);
        let use_case = rim_session::use_cases::DeleteRule::new(rule_store);
        use_case.execute(session, &key, origin)?;
        Ok(rule_set_dto(session))
    })
    .await
}

/// See [`delete_rule_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`delete_rule_inner`].
#[tauri::command]
pub async fn delete_rule(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DeleteRuleRequestDto,
) -> Result<RuleSetDto, CommandError> {
    let result = delete_rule_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::RuleDeleted);
    }
    result
}

/// Promotes an imported pair or placement rule to a `UserDecision`-owned
/// copy, persisting the change. A no-op (returns the unchanged rule set)
/// when no imported rule matches `key`, a `UserDecision` rule already
/// exists at that key, or `key` is [`RuleKey::Incompatible`] — see
/// [`rim_session::Session::promote_imported_rule`].
///
/// # Errors
///
/// Returns [`CommandError`] when saving fails. `key -> RuleKey` is an
/// infallible field-for-field mapping (`ModId::new` never fails), so this
/// never actually returns `invalid_input`.
pub(crate) async fn promote_imported_rule_inner(
    state: &AppState,
    key: RuleKeyDto,
) -> Result<RuleSetDto, CommandError> {
    let rule_store = state.adapters.rule_store;
    with_session(state, move |session| {
        let key: RuleKey = key.try_into()?;
        let use_case = rim_session::use_cases::PromoteImportedRule::new(rule_store);
        use_case.execute(session, &key)?;
        Ok(rule_set_dto(session))
    })
    .await
}

/// See [`promote_imported_rule_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`promote_imported_rule_inner`].
#[tauri::command]
pub async fn promote_imported_rule(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    key: RuleKeyDto,
) -> Result<RuleSetDto, CommandError> {
    let result = promote_imported_rule_inner(&state, key).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::RuleUpserted);
    }
    result
}

/// Lists decisions whose key no longer matches a live finding in either
/// order's ledger — prunable via `revert_decision` on the rules page.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_orphaned_decisions_inner(
    state: &AppState,
) -> Result<Vec<OrphanedDecisionDto>, CommandError> {
    with_session(state, |session| {
        let mut live = BTreeSet::new();
        for source in [OrderSource::Current, OrderSource::Suggested] {
            live.extend(
                session
                    .ledger(source)
                    .entries
                    .iter()
                    .map(|resolution| resolution.key.clone()),
            );
        }

        let decisions = session.decisions().clone();
        Ok(decisions.orphaned(&live).map(Into::into).collect())
    })
    .await
}

/// See [`list_orphaned_decisions_inner`].
///
/// # Errors
///
/// See [`list_orphaned_decisions_inner`].
#[tauri::command]
pub async fn list_orphaned_decisions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<OrphanedDecisionDto>, CommandError> {
    list_orphaned_decisions_inner(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::common::RuleOriginDto;
    use crate::dto::rule::PairRuleDto;
    use crate::test_support::session_fixture_with_temp_paths;

    #[tokio::test]
    async fn list_rules_filters_by_origin() {
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        upsert_rule_inner(
            &state,
            RuleDto::Pair(PairRuleDto {
                after: "a".to_string(),
                before: "b".to_string(),
                origin: RuleOriginDto::UserDecision,
                comment: None,
                promoted_from: None,
                already_promoted: false,
                overrides_declared: false,
            }),
        )
        .await
        .expect("upsert must succeed");

        assert!(
            temp_dir.path().join("profile").join("rules.json").exists(),
            "the upsert must have persisted rules.json under this test's own temp dir"
        );

        let all = list_rules_inner(&state, RuleFilterDto { origin: None })
            .await
            .expect("list_rules must succeed");
        assert_eq!(all.pairs.len(), 1);

        let filtered_out = list_rules_inner(
            &state,
            RuleFilterDto {
                origin: Some(RuleOriginDto::RimSortUser),
            },
        )
        .await
        .expect("list_rules must succeed");
        assert!(filtered_out.pairs.is_empty());

        let filtered_in = list_rules_inner(
            &state,
            RuleFilterDto {
                origin: Some(RuleOriginDto::UserDecision),
            },
        )
        .await
        .expect("list_rules must succeed");
        assert_eq!(filtered_in.pairs.len(), 1);
    }

    #[tokio::test]
    async fn list_rules_surfaces_rule_load_warnings() {
        let (_temp_dir, mut session) = session_fixture_with_temp_paths(&["a"]);
        session.set_rule_load_warnings(vec![
            rim_session::ports::RulesLoadWarning::DroppedClusterRules {
                rule_ids: vec!["stale-cluster".to_string()],
            },
        ]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let rules = list_rules_inner(&state, RuleFilterDto { origin: None })
            .await
            .expect("list_rules must succeed");

        assert_eq!(rules.warnings.len(), 1);
        assert!(
            rules.warnings[0].contains("stale-cluster"),
            "got {:?}",
            rules.warnings
        );
    }

    /// Seeds both an imported pair and its promoted `UserDecision` copy at
    /// the same key and deletes only the imported one, asserting the promoted
    /// copy survives — with only one rule at the deleted key, deleting with
    /// `Some(origin)` and deleting with `None` would produce an identical
    /// result and could never distinguish "origin threading works" from
    /// "origin is silently ignored". `Session::delete_rule`/
    /// `DeleteRule::execute` consume `origin` (`remove_rule_matching`).
    #[tokio::test]
    async fn delete_rule_by_origin_leaves_a_promoted_copy_sharing_the_key_in_place() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        upsert_rule_inner(
            &state,
            RuleDto::Pair(PairRuleDto {
                after: "a".to_string(),
                before: "b".to_string(),
                origin: RuleOriginDto::RimSortCommunity,
                comment: Some("imported".to_string()),
                promoted_from: None,
                already_promoted: false,
                overrides_declared: false,
            }),
        )
        .await
        .expect("upsert must succeed");
        promote_imported_rule_inner(
            &state,
            RuleKeyDto::Pair {
                after: "a".to_string(),
                before: "b".to_string(),
            },
        )
        .await
        .expect("promote must succeed");

        let result = delete_rule_inner(
            &state,
            DeleteRuleRequestDto {
                key: RuleKeyDto::Pair {
                    after: "a".to_string(),
                    before: "b".to_string(),
                },
                origin: Some(RuleOriginDto::RimSortCommunity),
            },
        )
        .await
        .expect("delete must succeed");

        assert_eq!(
            result.pairs.len(),
            1,
            "the imported original must be gone, the promoted copy must remain: {:?}",
            result.pairs
        );
        assert_eq!(
            result.pairs[0].origin,
            RuleOriginDto::UserDecision,
            "the surviving row must be the user's own promoted copy: {:?}",
            result.pairs
        );
    }

    #[tokio::test]
    async fn promote_imported_rule_adds_a_user_decision_copy_alongside_the_import() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        upsert_rule_inner(
            &state,
            RuleDto::Pair(PairRuleDto {
                after: "a".to_string(),
                before: "b".to_string(),
                origin: RuleOriginDto::RimSortCommunity,
                comment: Some("imported".to_string()),
                promoted_from: None,
                already_promoted: false,
                overrides_declared: false,
            }),
        )
        .await
        .expect("upsert must succeed");

        let promoted = promote_imported_rule_inner(
            &state,
            RuleKeyDto::Pair {
                after: "a".to_string(),
                before: "b".to_string(),
            },
        )
        .await
        .expect("promote must succeed");

        assert_eq!(
            promoted.pairs.len(),
            2,
            "the import stays alongside the promoted copy"
        );
        assert!(
            promoted
                .pairs
                .iter()
                .any(|rule| rule.origin == RuleOriginDto::UserDecision)
        );
        assert!(
            promoted
                .pairs
                .iter()
                .any(|rule| rule.origin == RuleOriginDto::RimSortCommunity),
            "the imported original must still be listed as its source"
        );
    }

    #[tokio::test]
    async fn promote_imported_rule_is_a_no_op_with_nothing_imported() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let result = promote_imported_rule_inner(
            &state,
            RuleKeyDto::Pair {
                after: "a".to_string(),
                before: "b".to_string(),
            },
        )
        .await
        .expect("a no-op promote still returns Ok");

        assert!(result.pairs.is_empty());
    }

    /// A rule or decision edit must never wait behind a scan: it takes only
    /// the session lock, so the swap that re-reads the files is the one
    /// place that can see an edit made mid-scan.
    #[tokio::test]
    async fn rule_and_decision_commands_never_wait_on_the_load_lock() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let scan_in_progress = state.load_lock.lock().await;
        let rule = RuleDto::Pair(PairRuleDto {
            after: "a".to_string(),
            before: "b".to_string(),
            origin: RuleOriginDto::UserDecision,
            comment: None,
            promoted_from: None,
            already_promoted: false,
            overrides_declared: false,
        });
        let decision = crate::dto::finding::DecideRequestDto {
            key: "unsupported_version:a".to_string(),
            action: crate::dto::finding::ActionDto::Ignore,
            note: None,
        };

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            upsert_rule_inner(&state, rule).await?;
            crate::commands::findings::decide_inner(&state, decision).await
        })
        .await;

        drop(scan_in_progress);
        assert!(
            outcome
                .expect("the commands must not wait for the load lock")
                .is_ok(),
            "both commands succeed while a scan holds the load lock"
        );
    }

    // -- import_rimsort lock protocol --

    use std::sync::Arc;

    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{PairRule, RuleOrigin};
    use rim_session::ports::{ImportError, ImportedRules};
    use rim_session::test_support::{FakeRimSortImporter, session_fixture_in_profile};

    use crate::error::CommandErrorCode;
    use crate::test_support::Gate;

    /// An importer whose read waits at a gate, then answers one pair rule.
    struct GatedImporter {
        gate: Arc<Gate>,
        answer: FakeRimSortImporter,
    }

    impl GatedImporter {
        fn new(gate: &Arc<Gate>) -> Self {
            let imported = ImportedRules {
                user_rules: Some(vec![Rule::Pair(PairRule {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                    origin: RuleOrigin::RimSortUser,
                    comment: None,
                    overrides_declared: false,
                })]),
                community_rules: None,
                steam_dependencies: None,
                skipped_inactive_rules: 0,
                skipped_inactive_steam: 0,
                provenance: std::collections::BTreeMap::new(),
            };
            Self {
                gate: Arc::clone(gate),
                answer: FakeRimSortImporter::new(imported),
            }
        }
    }

    impl RimSortImporter for GatedImporter {
        fn import(
            &self,
            paths: &RimSortPaths,
            active: &std::collections::BTreeMap<ModId, Option<u64>>,
        ) -> Result<ImportedRules, ImportError> {
            self.gate.block();
            self.answer.import(paths, active)
        }
    }

    /// A user-rules path for an importer that never opens it.
    fn user_rules_request() -> RimSortPathsDto {
        RimSortPathsDto {
            user_rules: Some(
                std::env::temp_dir()
                    .join("userRules.json")
                    .display()
                    .to_string(),
            ),
            community_rules: None,
            steam_db: None,
        }
    }

    /// While the files are read another session command must run; were the
    /// lock held across the read, the read would give up on its own timeout.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_session_lock_is_free_while_the_rimsort_files_are_read() {
        let profile = tempfile::tempdir().expect("profile");
        let state = AppState::default();
        *state.session.write().expect("lock") =
            Some(session_fixture_in_profile(&["a", "b"], profile.path()));
        let gate = Gate::new();

        let import =
            import_rimsort_with(&state, user_rules_request(), |_| GatedImporter::new(&gate));
        let other_command = async {
            gate.wait_until_blocked().await;
            let meanwhile = with_session(&state, |session| Ok(session.working().ids().len())).await;
            gate.release();
            meanwhile
        };
        let (imported, meanwhile) = tokio::join!(import, other_command);

        assert_eq!(meanwhile.expect("a second command ran"), 2);
        assert!(gate.was_released(), "the lock blocked the read");
        imported.expect("the import applied");
        let rules = with_session(&state, |session| Ok(session.rules().pairs.len()))
            .await
            .expect("session");
        assert_eq!(rules, 1);
        assert!(profile.path().join("rules.json").is_file());
    }

    /// Runs an import whose read is interrupted by `swap` replacing the
    /// loaded session, and returns what the import answered.
    async fn import_interrupted_by(
        first: &std::path::Path,
        swap: impl FnOnce() -> Session,
    ) -> (AppState, Result<ImportReportDto, CommandError>) {
        let state = AppState::default();
        *state.session.write().expect("lock") =
            Some(session_fixture_in_profile(&["a", "b"], first));
        let gate = Gate::new();

        let import =
            import_rimsort_with(&state, user_rules_request(), |_| GatedImporter::new(&gate));
        let replace = async {
            gate.wait_until_blocked().await;
            *state.session.write().expect("lock") = Some(swap());
            gate.release();
        };
        let (result, ()) = tokio::join!(import, replace);
        (state, result)
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_project_swapped_while_the_files_are_read_is_refused_and_nothing_is_saved() {
        let first = tempfile::tempdir().expect("first");
        let second = tempfile::tempdir().expect("second");

        let (state, result) = import_interrupted_by(first.path(), || {
            session_fixture_in_profile(&["a", "b"], second.path())
        })
        .await;

        assert_eq!(
            result.expect_err("refused").code,
            CommandErrorCode::ProjectChanged
        );
        let rules = with_session(&state, |session| Ok(session.rules().pairs.len()))
            .await
            .expect("session");
        assert_eq!(rules, 0, "the new project did not receive the rules");
        assert_eq!(std::fs::read_dir(second.path()).expect("list").count(), 0);
        assert!(!first.path().join("rules.json").exists());
    }

    /// A rescan swaps in a session for the same profile folder with another
    /// active set; the rules were matched against the old one.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_rescan_that_changed_the_active_mods_during_the_read_is_refused() {
        let profile = tempfile::tempdir().expect("profile");

        let (_state, result) = import_interrupted_by(profile.path(), || {
            session_fixture_in_profile(&["a"], profile.path())
        })
        .await;

        assert_eq!(
            result.expect_err("refused").code,
            CommandErrorCode::ProjectChanged
        );
        assert!(!profile.path().join("rules.json").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_network_share_is_refused_before_the_session_is_consulted() {
        let state = AppState::default();

        let result = import_rimsort_inner(
            &state,
            RimSortPathsDto {
                user_rules: None,
                community_rules: Some(r"\\server\share\communityRules.json".to_string()),
                steam_db: None,
            },
        )
        .await;

        assert_eq!(
            result.expect_err("refused").code,
            CommandErrorCode::NetworkPathRefused
        );
    }
}
