//! `get_startup_costs`: the `/startup` page's own per-mod cost table
//! Read-only,
//! straight off the current session's `Report.mod_costs` — no
//! computation of its own (see `dto/startup.rs`'s own doc comment for
//! why this DTO carries no display name to resolve either).

use crate::dto::startup::{ModCostRowDto, mod_cost_row};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// See [`get_startup_costs`]. Takes `&AppState` directly so it runs (and
/// is unit-tested) without a live Tauri runtime.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_startup_costs_inner(
    state: &AppState,
) -> Result<Vec<ModCostRowDto>, CommandError> {
    with_session(state, |session| {
        Ok(session
            .report()
            .mod_costs
            .iter()
            .map(mod_cost_row)
            .collect())
    })
    .await
}

/// Every active mod's startup-cost row, in the report's own scan order —
/// sorting is `StartupPage.vue`'s own job.
///
/// # Errors
///
/// See [`get_startup_costs_inner`].
#[tauri::command]
pub async fn get_startup_costs(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModCostRowDto>, CommandError> {
    get_startup_costs_inner(&state).await
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{ModCost, ModId};
    use rim_resolve::test_support::ReportBuilder;

    use super::*;
    use crate::test_support::session_with_temp_paths;

    fn sample_mod_cost(mod_id: &str) -> ModCost {
        ModCost {
            mod_id: ModId::new(mod_id),
            patch_ops: 1,
            slow_xpath_ops: 0,
            texture_files: 2,
            texture_bytes: 4096,
            dds_files: 1,
            assembly_count: 0,
            assembly_bytes: 0,
            def_count: 3,
            content_only: true,
            overridden_texture_bytes: 0,
            nameless_def_count: 0,
        }
    }

    #[tokio::test]
    async fn returns_one_row_per_mod_cost() {
        let mut report = ReportBuilder::new().mod_("a.mod").build();
        report.mod_costs = vec![sample_mod_cost("a.mod")];
        let (_temp_dir, session) =
            session_with_temp_paths(report, SourceIndex::default(), &["a.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);

        let rows = get_startup_costs_inner(&state)
            .await
            .expect("get_startup_costs");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].mod_id, "a.mod");
        assert_eq!(rows[0].texture_bytes, 4096);
    }
}
