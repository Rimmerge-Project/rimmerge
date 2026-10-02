//! Tauri commands: the interface layer between the Vue frontend and
//! `rim-session`'s use cases. Every command is thin — parse DTOs, run the
//! use case, map the result back to a DTO. No business logic lives here.
//!
//! Each module also exposes a `*_inner(state, ...)` function taking
//! `&crate::state::AppState` directly (never `tauri::State`/
//! `tauri::AppHandle`) so its logic runs — and is unit-tested — without a
//! live Tauri runtime; the `#[tauri::command]` wrapper is just that inner
//! call plus, on success, [`emit_session_changed`].

pub mod active_set;
pub mod apply;
pub mod assignments;
pub mod dashboard;
pub mod def_cache;
pub mod def_conflict;
pub mod def_graphics;
pub mod defs;
pub mod findings;
pub mod game_log;
pub mod links;
pub mod merge;
pub mod mods;
pub mod notifications;
pub mod order;
pub mod patch;
pub mod project;
pub mod recommended_rules;
pub mod rules;
pub mod rules_databases;
pub mod settings;
pub mod startup;
pub mod tags;
pub mod textures;
pub mod verify;

/// Every mod's display name, keyed by id — the lookup the merge and patch
/// commands need to turn a mod id into a display string.
pub(crate) fn mod_names(
    session: &rim_session::Session,
) -> std::collections::BTreeMap<rim_analyzer::domain::ModId, String> {
    session
        .report()
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.name.clone()))
        .collect()
}

/// The `project://progress` event name, emitted while [`project::load_project`]
/// scans an install.
pub const EVENT_PROJECT_PROGRESS: &str = "project://progress";

/// The `verify://progress` event name, emitted while [`verify::verify_order`]
/// runs — see that module's own doc comment.
pub const EVENT_VERIFY_PROGRESS: &str = "verify://progress";

/// The `rules://recommended-progress` event name, emitted while
/// [`recommended_rules::get_recommended_rules`] downloads and imports.
pub const EVENT_RECOMMENDED_RULES_PROGRESS: &str = "rules://recommended-progress";

/// The `session://changed` event name, emitted after any command that
/// mutates the session.
pub const EVENT_SESSION_CHANGED: &str = "session://changed";

/// Emits [`EVENT_SESSION_CHANGED`] with `reason`. The one place every
/// mutating command signals the frontend to invalidate its queries;
/// failure to emit (no listeners, a closed window) is not itself a
/// command failure, so this never returns an error.
pub(crate) fn emit_session_changed(
    app: &tauri::AppHandle,
    reason: crate::dto::project::SessionChangeReasonDto,
) {
    use tauri::Emitter;

    let _ = app.emit(
        EVENT_SESSION_CHANGED,
        crate::dto::project::SessionChangedEventDto { reason },
    );
}
