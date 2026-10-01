//! `list_rules`/`import_rimsort`/`upsert_rule`/`delete_rule`/
//! `list_orphaned_decisions`: the rules page's commands.

use std::collections::BTreeSet;

use rim_resolve::domain::{OrderSource, Rule};
use rim_session::{RuleKey, Session};

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
            .iter()
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
/// # Errors
///
/// Returns [`CommandError`] when a database file can't be read, or when
/// saving the merged rules fails.
pub(crate) async fn import_rimsort_inner(
    state: &AppState,
    paths: RimSortPathsDto,
) -> Result<ImportReportDto, CommandError> {
    let rule_store = state.adapters.rule_store;
    with_session(state, move |session| {
        let importer = rim_io::RimSortImporter::new(session.paths().profile_dir.clone());
        let use_case = rim_session::use_cases::ImportRimSort::new(
            importer,
            rule_store,
            rim_io::JsonImportManifestStore::new(),
        );
        let imported = use_case.execute(session, &paths.into())?;
        Ok((&imported).into())
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
}
