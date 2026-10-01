//! `list_findings`/`get_finding`/`decide`/`revert_decision`: the inbox's
//! commands. All four operate on [`rim_session::Session::selected`] — the
//! order source `select_order` last set — never a source of their own,
//! by design (only
//! `list_order`/`explain_placement` take an explicit source, since the
//! load-order page has its own independent source switch).

use std::str::FromStr;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Action, Decision, FindingKey};
use rim_resolve::sort::compute_disturbance;

use crate::commands::emit_session_changed;
use crate::dto::finding::{
    DecideRequestDto, DecideResultDto, FindingFilterDto, FindingPageDto, ResolutionDetailDto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Pages the selected order's ledger.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_findings_inner(
    state: &AppState,
    filter: FindingFilterDto,
) -> Result<FindingPageDto, CommandError> {
    with_session(state, move |session| {
        let source = session.selected();
        let page = session.findings(source, &filter.into());
        let items = page
            .items
            .iter()
            .filter_map(|key| session.resolution(source, key).map(Into::into))
            .collect();
        Ok(FindingPageDto {
            total: page.total,
            items,
        })
    })
    .await
}

/// See [`list_findings_inner`].
///
/// # Errors
///
/// See [`list_findings_inner`].
#[tauri::command]
pub async fn list_findings(
    state: tauri::State<'_, AppState>,
    filter: FindingFilterDto,
) -> Result<FindingPageDto, CommandError> {
    list_findings_inner(&state, filter).await
}

/// One finding's full detail from the selected order's ledger.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `key` doesn't parse, or
/// [`CommandError::finding_not_found`] when it isn't currently live.
pub(crate) async fn get_finding_inner(
    state: &AppState,
    key: String,
) -> Result<ResolutionDetailDto, CommandError> {
    with_session(state, move |session| {
        let finding_key = FindingKey::from_str(&key)?;
        let source = session.selected();
        session
            .resolution(source, &finding_key)
            .map(Into::into)
            .ok_or_else(|| CommandError::finding_not_found(&key))
    })
    .await
}

/// See [`get_finding_inner`].
///
/// # Errors
///
/// See [`get_finding_inner`].
#[tauri::command]
pub async fn get_finding(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<ResolutionDetailDto, CommandError> {
    get_finding_inner(&state, key).await
}

/// How many mods changed position between two suggested-order snapshots
/// — the delta a single decision (or its reversal) actually caused, not
/// the suggested order's total displacement from the current one (see
/// [`rim_resolve::sort::SortOutcome::stats`] for that separate figure).
fn moved_mods_between(before: &[ModId], after: &[ModId]) -> usize {
    use rim_analyzer::domain::LoadOrder;

    compute_disturbance(
        &LoadOrder::new(after.to_vec()),
        &LoadOrder::new(before.to_vec()),
    )
    .positions_changed
}

/// Records a decision on a finding in the selected order's ledger,
/// persisting it before returning.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request.key`/`.action`
/// don't parse, or [`CommandError`] when persisting the decision fails.
pub(crate) async fn decide_inner(
    state: &AppState,
    request: DecideRequestDto,
) -> Result<DecideResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let key = FindingKey::from_str(&request.key)?;
        let action = Action::try_from(request.action)?;
        let decision = Decision {
            key,
            action,
            note: request.note,
            decided_at: jiff::Timestamp::now(),
        };

        let before = session.orders().suggested.as_slice().to_vec();

        // `with_rule_store`, not `new`: an `Action::PromoteRule` decision's
        // real effect is a `RuleSet` mutation
        // that only a rule store can persist — `Decide::new` would reject
        // it with `DecideError::PromoteNeedsRuleStore` instead.
        let use_case = rim_session::use_cases::Decide::with_rule_store(
            adapters.decision_store,
            adapters.rule_store,
        );
        use_case.execute(session, decision)?;

        let after = session.orders().suggested.as_slice();
        let resorted = before != after;
        let moved_mods = if resorted {
            moved_mods_between(&before, after)
        } else {
            0
        };
        let stats = session.ledger(session.selected()).stats.into();

        Ok(DecideResultDto {
            stats,
            resorted,
            moved_mods,
        })
    })
    .await
}

/// See [`decide_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`decide_inner`].
#[tauri::command]
pub async fn decide(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DecideRequestDto,
) -> Result<DecideResultDto, CommandError> {
    let result = decide_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Decided);
    }
    result
}

/// Removes a decision on a finding in the selected order's ledger,
/// persisting the change before returning.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `key` doesn't parse,
/// [`CommandError::finding_not_found`] when there was no decision on
/// `key` to remove, or [`CommandError`] when persisting the removal
/// fails.
pub(crate) async fn revert_decision_inner(
    state: &AppState,
    key: String,
) -> Result<DecideResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let finding_key = FindingKey::from_str(&key)?;
        let before = session.orders().suggested.as_slice().to_vec();

        let use_case = rim_session::use_cases::RevertDecision::new(adapters.decision_store);
        let removed = use_case.execute(session, &finding_key)?;
        if removed.is_none() {
            return Err(CommandError::finding_not_found(&key));
        }

        let after = session.orders().suggested.as_slice();
        let resorted = before != after;
        let moved_mods = if resorted {
            moved_mods_between(&before, after)
        } else {
            0
        };
        let stats = session.ledger(session.selected()).stats.into();

        Ok(DecideResultDto {
            stats,
            resorted,
            moved_mods,
        })
    })
    .await
}

/// See [`revert_decision_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`revert_decision_inner`].
#[tauri::command]
pub async fn revert_decision(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    key: String,
) -> Result<DecideResultDto, CommandError> {
    let result = revert_decision_inner(&state, key).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::Reverted);
    }
    result
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{PairRule, Rule, RuleOrigin};
    use rim_session::ports::RuleStore;

    use super::*;
    use crate::dto::finding::{ActionDto, PromotedRuleKeyDto};
    use crate::test_support::session_fixture_with_temp_paths;

    #[test]
    fn moved_mods_between_counts_positions_changed_between_two_snapshots() {
        let before = vec![ModId::new("a"), ModId::new("b"), ModId::new("c")];
        let after = vec![ModId::new("b"), ModId::new("a"), ModId::new("c")];

        assert_eq!(moved_mods_between(&before, &after), 2);
    }

    #[test]
    fn moved_mods_between_is_zero_for_identical_snapshots() {
        let order = vec![ModId::new("a"), ModId::new("b")];

        assert_eq!(moved_mods_between(&order, &order), 0);
    }

    #[tokio::test]
    async fn revert_decision_returns_finding_not_found_for_a_key_with_no_decision() {
        // `RevertDecision::execute` is a no-op when there is nothing to
        // remove (see its own doc comment): it must not write
        // `decisions.json`, so this pins that against a temp-dir session
        // rather than `session_fixture`'s relative paths.
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a", "b"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let result = revert_decision_inner(&state, "missing_mod:nobody".to_string()).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::FindingNotFound
        );
        assert!(
            !temp_dir
                .path()
                .join("profile")
                .join("decisions.json")
                .exists(),
            "reverting a key with no decision must not write decisions.json"
        );
    }

    /// Deciding an `Action::PromoteRule` from the inbox must
    /// persist the promoted copy to `rules.json`, not just to the
    /// in-memory session — `Decide::new` (no rule store) would silently
    /// leave nothing on disk for the next launch to load, which is
    /// exactly the gap `Decide::with_rule_store` closes.
    #[tokio::test]
    async fn decide_with_promote_rule_persists_the_promoted_copy_to_rules_json() {
        let (temp_dir, mut session) = session_fixture_with_temp_paths(&["a", "b"]);
        session.upsert_rule(Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::SteamDb,
            comment: None,
            overrides_declared: false,
        }));
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let result = decide_inner(
            &state,
            DecideRequestDto {
                key: "rule_overruled:a:b:steam_db".to_string(),
                action: ActionDto::PromoteRule {
                    rule: PromotedRuleKeyDto::Pair {
                        after: "a".to_string(),
                        before: "b".to_string(),
                    },
                },
                note: None,
            },
        )
        .await;

        assert!(result.is_ok(), "{result:?}");
        let loaded = rim_io::JsonRuleStore
            .load(&temp_dir.path().join("profile"))
            .expect("rules.json must load back");
        assert!(
            loaded
                .rules
                .pairs
                .iter()
                .any(|pair| pair.origin == RuleOrigin::UserDecision
                    && pair.after.as_str() == "a"
                    && pair.before.as_str() == "b"),
            "rules.json must gain a userDecision copy of the promoted pair, got {:?}",
            loaded.rules.pairs
        );
    }
}
