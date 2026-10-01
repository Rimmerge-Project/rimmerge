//! `select_order`/`list_order`/`explain_placement`: the load-order page's
//! commands.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::OrderSource;

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

/// Lists every mod in `source`'s order, one row per mod.
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
        let order = session.orders().get(source).clone();
        let current = session.orders().current.clone();
        let placements = session.sort_outcome().placements.clone();
        // Cloned out before the `needs_input_by_mod`/`report` calls below
        // so no borrow of `session` outlives this statement — `Session`
        // is single-writer, so every one of its methods needs the whole
        // borrow to itself.
        let needs_input_by_mod = session.needs_input_by_mod(source).clone();
        let names: BTreeMap<ModId, String> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect();
        let hard_dependents: BTreeMap<ModId, usize> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.id.clone(), m.hard_dependents))
            .collect();

        let mut rows = Vec::with_capacity(order.as_slice().len());
        for (position, mod_id) in order.as_slice().iter().enumerate() {
            let needs_input_count = needs_input_by_mod
                .get(&mod_id.base())
                .copied()
                .unwrap_or_default();
            let placement = placements.get(mod_id);
            rows.push(OrderRowDto {
                mod_id: mod_id.as_str().to_string(),
                name: names.get(mod_id).cloned().unwrap_or_default(),
                position,
                previous_position: (source != OrderSource::Current)
                    .then(|| current.position(mod_id))
                    .flatten(),
                tier: placement
                    .map_or(rim_resolve::sort::Tier::Body, |p| p.tier)
                    .into(),
                tags: session
                    .tagging()
                    .tags_of(mod_id)
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                hard_dependents: hard_dependents.get(mod_id).copied().unwrap_or_default(),
                needs_input_count,
            });
        }
        Ok(rows)
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
/// suggested order (the only order the sorter explains). Resolves
/// `mod_id` by [`ModId::base`] on both sides, so an active `_steam`
/// variant is found by its base id and vice versa — findings and other
/// DTOs carry base ids, but the placements map (like the report itself)
/// is keyed by whichever exact id is actually active.
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
        let target = ModId::new(&mod_id).base();
        session
            .sort_outcome()
            .placements
            .values()
            .find(|explanation| explanation.mod_id.base() == target)
            .map(Into::into)
            .ok_or_else(|| CommandError::mod_not_found(&mod_id))
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
