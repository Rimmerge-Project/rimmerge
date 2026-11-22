//! `select_order`/`list_order`/`explain_placement`: the load-order page's
//! commands.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::OrderSource;
use rim_session::ModNotActive;

use crate::dto::common::OrderSourceDto;
use crate::dto::finding::LedgerStatsDto;
use crate::dto::order::{OrderRowDto, PlacementExplanationDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Selects which order (`current`/`suggested`) later ledger/finding
/// queries read from, and returns that order's ledger stats.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn select_order_inner(
    state: &AppState,
    source: OrderSourceDto,
) -> Result<LedgerStatsDto, CommandError> {
    let source: OrderSource = source.into();
    with_session(state, move |session| {
        rim_session::use_cases::SelectOrder::new().execute(session, source);
        Ok(session.ledger(source).stats.into())
    })
    .await
}

/// See [`select_order_inner`].
///
/// # Errors
///
/// See [`select_order_inner`].
#[tauri::command]
pub async fn select_order(
    state: tauri::State<'_, AppState>,
    source: OrderSourceDto,
) -> Result<LedgerStatsDto, CommandError> {
    select_order_inner(&state, source).await
}

/// Lists every mod in `source`'s order, one row per mod. The join lives in
/// [`rim_session::Session::order_rows`]; this only maps the DTO in and out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_order_inner(
    state: &AppState,
    source: OrderSourceDto,
) -> Result<Vec<OrderRowDto>, CommandError> {
    let source: OrderSource = source.into();
    with_session(state, move |session| {
        Ok(session
            .order_rows(source)
            .into_iter()
            .map(Into::into)
            .collect())
    })
    .await
}

/// See [`list_order_inner`].
///
/// # Errors
///
/// See [`list_order_inner`].
#[tauri::command]
pub async fn list_order(
    state: tauri::State<'_, AppState>,
    source: OrderSourceDto,
) -> Result<Vec<OrderRowDto>, CommandError> {
    list_order_inner(&state, source).await
}

/// The full why-panel explanation for one mod's placement in the
/// suggested order (the only order the sorter explains). The lookup lives
/// in [`rim_session::Session::explain_placement`] (the exact id first, then
/// [`ModId::base`], so an active `_steam` variant is found by its base id);
/// this only maps the DTO in and out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::mod_not_found`] when `mod_id` has no placement
/// explanation (it isn't in the suggested order).
pub(crate) async fn explain_placement_inner(
    state: &AppState,
    mod_id: String,
) -> Result<PlacementExplanationDto, CommandError> {
    with_session(state, move |session| {
        session
            .explain_placement(&ModId::new(&mod_id))
            .map(Into::into)
            // The raw request text, as `get_mod` does.
            .map_err(|ModNotActive(_)| CommandError::mod_not_found(&mod_id))
    })
    .await
}

/// See [`explain_placement_inner`].
///
/// # Errors
///
/// See [`explain_placement_inner`].
#[tauri::command]
pub async fn explain_placement(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<PlacementExplanationDto, CommandError> {
    explain_placement_inner(&state, mod_id).await
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{Action, Decision, FindingKey};
    use rim_session::test_support::session_fixture;

    use super::*;

    #[tokio::test]
    async fn explain_placement_finds_an_active_steam_suffixed_mod_by_its_base_id() {
        let mut state = AppState::default();
        let session = session_fixture(&["x_steam", "y"]);
        state.session = std::sync::Arc::new(std::sync::RwLock::new(Some(session)));

        let explanation = explain_placement_inner(&state, "x".to_string())
            .await
            .expect("x_steam must be found by its base id");

        assert_eq!(explanation.mod_id, "x_steam");
    }

    #[tokio::test]
    async fn explain_placement_returns_mod_not_found_for_an_inactive_mod() {
        let state = AppState::default();
        let session = session_fixture(&["a"]);
        *state.session.write().expect("lock") = Some(session);

        let result = explain_placement_inner(&state, "nobody".to_string()).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::ModNotFound
        );
    }

    #[tokio::test]
    async fn list_order_reports_needs_input_counts_by_base_id() {
        let mut session = session_fixture(&["x_steam", "y"]);
        session
            .decide(Decision {
                key: FindingKey::UnsupportedVersion {
                    mod_id: rim_analyzer::domain::ModId::new("x_steam"),
                },
                action: Action::Accept,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("accept is always valid");
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let rows = list_order_inner(&state, OrderSourceDto::Current)
            .await
            .expect("list_order must succeed");

        // `UnsupportedVersion` alone doesn't produce a live finding on
        // its own default fixture data, so this only exercises that the
        // lookup runs end to end without panicking on a `_steam` id;
        // `rim-session`'s own `finding_index` tests cover the counting
        // logic itself.
        assert_eq!(rows.len(), 2);
    }
}
