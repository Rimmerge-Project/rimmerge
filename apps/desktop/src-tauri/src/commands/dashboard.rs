//! `get_dashboard`: the summary counts the dashboard page renders.

use rim_resolve::domain::OrderSource;

use crate::dto::dashboard::{DashboardDto, LedgerStatsPairDto, count_edges};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Builds the dashboard's summary counts for the currently selected
/// order.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_dashboard_inner(state: &AppState) -> Result<DashboardDto, CommandError> {
    with_session(state, |session| {
        let selected = session.selected();
        let order = session.orders().get(selected).clone();

        // Scoped so the immutable borrow of `report` ends before the
        // `&mut self` calls (`sort_outcome`, `ledger`) below — `Session`
        // is single-writer, so every one of its methods needs the whole
        // borrow to itself.
        let (mod_count, edges_by_strength, edges_violated_by_source, conflicts_by_kind) = {
            let report = session.report();
            let (edges_by_strength, edges_violated_by_source) = count_edges(&report.edges, &order);
            (
                report.mods.len(),
                edges_by_strength,
                edges_violated_by_source,
                report.conflicts.as_slice().into(),
            )
        };
        let moved_mods = session.sort_outcome().stats.positions_changed;
        let sort_provenance = session.sort_provenance().into();

        let current_stats = session.ledger(OrderSource::Current).stats;
        let suggested_stats = session.ledger(OrderSource::Suggested).stats;
        let needs_input_by_kind =
            crate::dto::finding::needs_input_by_kind(&session.ledger(selected).entries);

        Ok(DashboardDto {
            mod_count,
            edges_by_strength,
            edges_violated_by_source,
            conflicts_by_kind,
            ledger_stats: LedgerStatsPairDto {
                current: current_stats.into(),
                suggested: suggested_stats.into(),
            },
            needs_input_by_kind,
            moved_mods,
            selected: selected.into(),
            file_matches_suggested: session.file_matches(OrderSource::Suggested),
            file_matches_current: session.file_matches(OrderSource::Current),
            sort_provenance,
        })
    })
    .await
}

/// See `get_dashboard_inner`.
///
/// # Errors
///
/// See `get_dashboard_inner`.
#[tauri::command]
pub async fn get_dashboard(
    state: tauri::State<'_, AppState>,
) -> Result<DashboardDto, CommandError> {
    get_dashboard_inner(&state).await
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, Decision, FindingKey};
    use rim_session::test_support::session_fixture;

    use super::*;

    fn state_with(session: rim_session::Session) -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        state
    }

    #[tokio::test]
    async fn the_file_matches_the_suggested_order_when_nothing_would_move() {
        let state = state_with(session_fixture(&["a", "b"]));

        let dashboard = get_dashboard_inner(&state).await.expect("dashboard");

        assert!(dashboard.file_matches_suggested);
    }

    #[tokio::test]
    async fn the_file_does_not_match_the_suggested_order_when_the_sorter_moves_a_mod() {
        let mut session = session_fixture(&["a", "b"]);
        session
            .decide(Decision {
                key: FindingKey::UndeclaredHardDependency {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                action: Action::Reorder {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("reorder is always a valid action");
        let state = state_with(session);

        let dashboard = get_dashboard_inner(&state).await.expect("dashboard");

        assert!(!dashboard.file_matches_suggested);
    }
}
