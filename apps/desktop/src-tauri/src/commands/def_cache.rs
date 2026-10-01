//! `get_def_cache_carrier`: whether an active mod carries
//! a known def-cache plugin, for the apply dialog's own note. Thin
//! wrap of `rim_session::use_cases::FindDefCacheCarrier`; the probe
//! itself (real filesystem IO) lives in `rim-io`
//! (`AppState::adapters::def_cache_probe`) per that port's own doc comment
//! on why this crate can't do the probe itself.

use rim_session::use_cases::FindDefCacheCarrier;

use crate::dto::def_cache::DefCacheCarrierDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// See [`get_def_cache_carrier`]. Takes `&AppState` directly so it runs
/// (and is unit-tested) without a live Tauri runtime.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_def_cache_carrier_inner(
    state: &AppState,
) -> Result<DefCacheCarrierDto, CommandError> {
    let probe = state.adapters.def_cache_probe;
    with_session(state, move |session| {
        let use_case =
            FindDefCacheCarrier::new(probe, session.mod_knowledge().def_cache_carriers().to_vec());
        Ok(DefCacheCarrierDto {
            carrier_mod_id: use_case.execute(session).map(|id| id.as_str().to_string()),
        })
    })
    .await
}

/// Finds the active mod (if any) carrying a known def-cache plugin.
///
/// # Errors
///
/// See [`get_def_cache_carrier_inner`].
#[tauri::command]
pub async fn get_def_cache_carrier(
    state: tauri::State<'_, AppState>,
) -> Result<DefCacheCarrierDto, CommandError> {
    get_def_cache_carrier_inner(&state).await
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_resolve::test_support::ReportBuilder;

    use super::*;
    use crate::test_support::session_with_temp_paths;

    #[tokio::test]
    async fn reports_none_when_no_active_mod_is_a_carrier() {
        let report = ReportBuilder::new().mod_("a.mod").build();
        let (_temp_dir, session) =
            session_with_temp_paths(report, SourceIndex::default(), &["a.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);

        let dto = get_def_cache_carrier_inner(&state)
            .await
            .expect("get_def_cache_carrier");

        assert_eq!(dto.carrier_mod_id, None);
    }

    /// The carrier list comes from the session's own loaded knowledge, so this
    /// test
    /// supplies an invented one rather than naming a real plugin — the
    /// real, embedded row is pinned by `rim-io`'s own `mod_knowledge.rs`
    /// contract test instead.
    #[tokio::test]
    async fn reports_the_carrier_when_its_loaded_folder_holds_a_matching_dll() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let carrier_folder = temp_dir.path().join("perf.mod");
        std::fs::create_dir_all(carrier_folder.join("Plugins")).expect("create dir");
        std::fs::write(carrier_folder.join("Plugins").join("ExampleCache.dll"), b"")
            .expect("write dll");
        let report = ReportBuilder::new()
            .mod_with("perf.mod", |m| {
                m.loaded_folders = vec![carrier_folder];
            })
            .build();
        let (_temp_dir, mut session) =
            session_with_temp_paths(report, SourceIndex::default(), &["perf.mod"]);
        session.set_mod_knowledge(rim_session::ModKnowledge::new(
            std::collections::BTreeMap::new(),
            rim_merge::patch_behaviours::PatchOperationBehaviours::default(),
            vec![rim_session::ports::DefCacheCarrier {
                id: "example".to_string(),
                plugin_dir: "Plugins".to_string(),
                file_prefix: "ExampleCache".to_string(),
                file_extension: ".dll".to_string(),
                recursive: true,
                log_line_prefix: "EXAMPLECACHE:".to_string(),
            }],
            rim_session::ports::LogShapes::default(),
        ));
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);

        let dto = get_def_cache_carrier_inner(&state)
            .await
            .expect("get_def_cache_carrier");

        assert_eq!(dto.carrier_mod_id, Some("perf.mod".to_string()));
    }

    /// A session whose knowledge was never loaded knows no carrier at
    /// all, so the note never shows — the honest degradation, not a
    /// guess.
    #[tokio::test]
    async fn reports_none_when_no_carrier_is_loaded_even_with_a_matching_dll_on_disk() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let carrier_folder = temp_dir.path().join("perf.mod");
        std::fs::create_dir_all(carrier_folder.join("Plugins")).expect("create dir");
        std::fs::write(carrier_folder.join("Plugins").join("ExampleCache.dll"), b"")
            .expect("write dll");
        let report = ReportBuilder::new()
            .mod_with("perf.mod", |m| {
                m.loaded_folders = vec![carrier_folder];
            })
            .build();
        let (_temp_dir, session) =
            session_with_temp_paths(report, SourceIndex::default(), &["perf.mod"]);
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);

        let dto = get_def_cache_carrier_inner(&state)
            .await
            .expect("get_def_cache_carrier");

        assert_eq!(dto.carrier_mod_id, None);
    }
}
