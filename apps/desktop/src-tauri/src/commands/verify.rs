//! `verify_order`: the wrap of the on-demand
//! `rim_session::use_cases::VerifyOrder` pass for the apply dialog's
//! verify summary.
//!
//! **Deliberately never part of `Session::compute`, `list_findings`, or
//! any other automatic path** — same as `apps/cli`'s own `verify`
//! command (`apps/cli/src/commands/verify.rs`), and as
//! [`rim_session::use_cases::verify_order`]'s own module doc comment
//! commits to ("Explicit, on-demand only"). A `PatchWillFail` finding
//! never reaches the normal inbox: `VerifyOrder` is excluded from
//! `Session::compute` (a real-install pass takes far longer per order
//! source than the inbox's own <2s budget), nothing invalidates a cached
//! result at the right times, and a stale prediction sitting in the
//! always-visible inbox risks reading as current when it may no longer
//! be. This command's own result is never cached or persisted on the
//! session either — every call is a fresh pass, shown only in the apply
//! dialog's own verify view for exactly as long as that dialog stays
//! open.

use std::collections::BTreeMap;

use rim_analyzer::domain::{Mod, ModId};
use rim_resolve::ledger::SuggestContext;
use rim_session::use_cases::VerifyOrder;

use crate::dto::verify::{VerifyProgressEventDto, VerifyReportDto, VerifyRequestDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Runs [`VerifyOrder::execute_with_progress`] against the currently
/// loaded session under `request.source`, reporting progress through
/// `on_progress` as it goes.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn verify_order_inner(
    state: &AppState,
    request: VerifyRequestDto,
    mut on_progress: impl FnMut(VerifyProgressEventDto) + Send + 'static,
) -> Result<VerifyReportDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let verify = VerifyOrder::new(adapters.def_reader);
        let report =
            verify.execute_with_progress(session, request.source.into(), &mut |checked, total| {
                on_progress(VerifyProgressEventDto { checked, total })
            });
        // The per-row `Reorder` the dialog's
        // "Create pair rule" button acts on is computed here, through
        // `rim_resolve::ledger::suggest`, never re-derived in the
        // frontend — see `VerifyReorderDto`'s own doc comment for why the
        // direction is the one thing that must not be guessed at twice.
        // `mods_by_id` is built because `SuggestContext` requires it, not
        // because `PatchWillFail` reads it (it reads none of the
        // context) — see `dto::verify::reorder_of`'s own doc comment.
        let mods_by_id: BTreeMap<ModId, &Mod> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        let ctx = SuggestContext {
            report: session.report(),
            sort_outcome: session.sort_outcome(),
            current: &session.orders().current,
            mods_by_id: &mods_by_id,
        };
        Ok(VerifyReportDto::from_report(&report, &ctx))
    })
    .await
}

/// See [`verify_order_inner`]. Emits [`crate::commands::EVENT_VERIFY_PROGRESS`]
/// as the pass proceeds — the apply dialog's own progress bar listens for
/// it, the same pattern `project::load_project` already established for
/// [`crate::commands::EVENT_PROJECT_PROGRESS`].
///
/// # Errors
///
/// See [`verify_order_inner`].
#[tauri::command]
pub async fn verify_order(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: VerifyRequestDto,
) -> Result<VerifyReportDto, CommandError> {
    use tauri::Emitter;

    verify_order_inner(&state, request, move |progress| {
        let _ = app.emit(crate::commands::EVENT_VERIFY_PROGRESS, progress);
    })
    .await
}
