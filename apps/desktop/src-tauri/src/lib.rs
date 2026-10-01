//! `rimmerge-desktop`: the Tauri v2 interface layer over `rim-session`.
//!
//! Owns [`state::AppState`] (the loaded [`rim_session::Session`] plus the
//! `rim-io` adapter set), [`error::CommandError`] (the one serializable
//! failure shape every command returns), the [`dto`] module (the IPC
//! boundary shapes, each mirroring a `rim-analyzer`/`rim-resolve`/
//! `rim-session` type), and the [`commands`] every command lives in. No
//! business logic lives here or in any command — see each module's own
//! docs.

#[cfg(test)]
mod capabilities;
pub mod commands;
#[cfg(test)]
mod def_graphics_real_install;
#[cfg(test)]
mod def_inspector_timing;
pub mod dto;
pub mod error;
#[cfg(test)]
mod real_install_support;
#[cfg(test)]
mod real_install_timing;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;

use state::AppState;

pub use error::CommandError;

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri application fails to start — there is no
/// meaningful way to continue or recover from that at this entry point.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A minimal subscriber so `tracing::warn!` (a poisoned session lock,
    // a panicked background task — see `state::with_session`) actually
    // goes somewhere instead of being silently dropped. `try_init`
    // rather than `init`: a second `run()` call (there isn't one today,
    // but a test harness constructing the app more than once shouldn't
    // panic on "subscriber already set").
    let _ = tracing_subscriber::fmt::try_init();

    let outcome = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::project::load_project,
            commands::project::get_default_paths,
            commands::project::save_app_config,
            commands::project::get_default_rimsort_paths,
            commands::dashboard::get_dashboard,
            commands::order::select_order,
            commands::order::list_order,
            commands::order::explain_placement,
            commands::findings::list_findings,
            commands::findings::get_finding,
            commands::findings::decide,
            commands::findings::revert_decision,
            commands::rules::list_rules,
            commands::rules::import_rimsort,
            commands::rules::upsert_rule,
            commands::rules::delete_rule,
            commands::rules::promote_imported_rule,
            commands::rules::list_orphaned_decisions,
            commands::rules_databases::get_rule_databases,
            commands::rules_databases::refresh_rule_databases,
            commands::tags::list_tags,
            commands::tags::set_manual_tag,
            commands::mods::list_mods,
            commands::mods::get_mod,
            commands::mods::list_mod_names,
            commands::mods::get_mod_info,
            commands::mods::read_mod_preview,
            commands::mods::read_mod_icon,
            commands::mods::open_mod_link,
            commands::links::open_app_link,
            commands::links::get_app_version,
            commands::active_set::list_inactive_mods,
            commands::active_set::plan_activate_mods,
            commands::active_set::activate_mods,
            commands::active_set::plan_deactivate_mods,
            commands::active_set::deactivate_mods,
            commands::active_set::get_pending_active_changes,
            commands::active_set::rescan_project,
            commands::apply::apply,
            commands::apply::get_apply_preflight,
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::get_default_settings,
            commands::settings::get_app_settings,
            commands::settings::update_app_settings,
            commands::settings::reset_network_policy,
            commands::settings::enable_recommended_rule_databases,
            commands::notifications::list_notifications,
            commands::notifications::list_muted_notification_kinds,
            commands::notifications::dismiss_notification,
            commands::notifications::mute_notification_kind,
            commands::notifications::unmute_notification_kind,
            commands::notifications::complete_welcome,
            commands::notifications::reset_settings,
            commands::notifications::check_for_update,
            commands::notifications::run_launch_network_checks,
            commands::merge::get_merge_preview,
            commands::merge::set_merge_choices,
            commands::merge::get_merge_mod,
            commands::merge::preview_merge_mod_file,
            commands::textures::read_texture,
            commands::patch::list_patches,
            commands::patch::get_patch,
            commands::patch::create_patch,
            commands::patch::update_patch,
            commands::patch::delete_patch,
            commands::patch::list_patch_findings,
            commands::patch::get_patch_finding,
            commands::patch::decide_patch,
            commands::patch::revert_patch_decision,
            commands::patch::import_profile_decisions,
            commands::patch::prune_patch_decisions,
            commands::patch::get_patch_render,
            commands::patch::preview_patch_file,
            commands::patch::export_patch,
            commands::defs::list_mod_changes,
            commands::defs::inspect_def,
            commands::defs::search_defs,
            commands::def_graphics::resolve_def_graphic,
            commands::def_graphics::read_def_texture,
            commands::verify::verify_order,
            commands::def_conflict::get_def_conflict_view,
            commands::assignments::list_assignments,
            commands::assignments::get_assignment,
            commands::assignments::list_assignment_candidates,
            commands::assignments::infer_assignment_candidate,
            commands::assignments::create_assignment,
            commands::assignments::update_assignment,
            commands::assignments::set_assignment_row,
            commands::assignments::clear_assignment_row,
            commands::assignments::add_assignment_section,
            commands::assignments::remove_assignment_section,
            commands::assignments::delete_assignment,
            commands::assignments::get_assignment_coverage,
            commands::assignments::list_assignment_items,
            commands::assignments::copy_assignment_row_from,
            commands::assignments::export_assignment,
            commands::startup::get_startup_costs,
            commands::game_log::import_game_log,
            commands::def_cache::get_def_cache_carrier,
        ])
        .run(tauri::generate_context!());

    // `clippy::expect_used` is denied workspace-wide, so a failure here
    // panics through an explicit match rather than `.expect(...)` — this
    // is still the true top-level entry point, where a panic is the
    // right response to a startup failure there's no meaningful way to
    // recover from.
    if let Err(error) = outcome {
        panic!("error while running the Rimmerge tauri application: {error}");
    }
}
