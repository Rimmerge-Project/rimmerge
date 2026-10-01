//! `get_settings`/`set_settings`/`get_default_settings`, and the
//! app-global `get_app_settings`/`update_app_settings`/
//! `reset_network_policy`.

use rim_session::ports::AppSettingsStore;

use crate::commands::emit_session_changed;
use crate::commands::project::profile_base;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::settings::{AppSettingsDto, AppSettingsResponseDto, SettingsDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// The current sorter/ledger settings.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_settings_inner(state: &AppState) -> Result<SettingsDto, CommandError> {
    with_session(state, |session| Ok(session.settings().into())).await
}

/// See [`get_settings_inner`].
///
/// # Errors
///
/// See [`get_settings_inner`].
#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsDto, CommandError> {
    get_settings_inner(&state).await
}

/// Replaces the sorter/ledger settings, persisting the change.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `settings.threshold` is
/// out of range, or [`CommandError`] when saving fails.
pub(crate) async fn set_settings_inner(
    state: &AppState,
    settings: SettingsDto,
) -> Result<SettingsDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let settings = settings.try_into()?;
        let use_case = rim_session::use_cases::UpdateSettings::new(adapters.rule_store);
        use_case.execute(session, settings)?;
        Ok(session.settings().into())
    })
    .await
}

/// See [`set_settings_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`set_settings_inner`].
#[tauri::command]
pub async fn set_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    settings: SettingsDto,
) -> Result<SettingsDto, CommandError> {
    let result = set_settings_inner(&state, settings).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::SettingsChanged);
    }
    result
}

/// The app-global network/reminder preferences
/// (`<base>/app-settings.json`) — one per machine, not per profile. Never
/// fails outright: a base that fails to resolve, or a file that fails to
/// parse, both degrade to [`rim_session::AppSettings::default`]/every
/// switch off respectively (see [`rim_session::ports::AppSettingsLoad`]'s
/// own doc comment) rather than a hard error, since a broken privacy
/// preference must never stop the app from starting. Takes no session
/// state, so it works even before a project is loaded — the same shape
/// [`get_default_settings`] and `save_app_config` already follow for
/// their own app-global files. Unlike `update_app_settings`/
/// `reset_network_policy`, this reports *how* the file loaded
/// ([`AppSettingsResponseDto::load_status`]) so the Settings page can
/// tell the user their settings file couldn't be read, rather than
/// silently showing every network switch off with no explanation.
///
/// # Errors
///
/// Returns [`CommandError::internal`] only when neither
/// `RIMMERGE_PROFILE_DIR` nor `LOCALAPPDATA` is set, so there is nowhere
/// on this machine to look.
#[tauri::command]
pub async fn get_app_settings() -> Result<AppSettingsResponseDto, CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to read \
             app-settings.json from",
        )
    })?;
    Ok(get_app_settings_inner(&base))
}

/// [`get_app_settings`], with `base` threaded in explicitly rather than
/// resolved from the real `RIMMERGE_PROFILE_DIR`/`LOCALAPPDATA`
/// environment — the seam this module's own tests use to stay hermetic,
/// the same convention `commands::rules_databases`' `get_rule_databases_with`
/// follows for its own env-derived inputs.
fn get_app_settings_inner(base: &std::path::Path) -> AppSettingsResponseDto {
    rim_io::JsonAppSettingsStore::new().load(base).into()
}

/// Replaces `<base>/app-settings.json` wholesale.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails, or [`CommandError::invalid_input`] when
/// `reminders.ruleDatabasesStaleAfterDays` is out of range.
#[tauri::command]
pub async fn update_app_settings(settings: AppSettingsDto) -> Result<AppSettingsDto, CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to write \
             app-settings.json",
        )
    })?;
    update_app_settings_at(&base, settings)
}

/// [`update_app_settings`], with `base` threaded in explicitly — the seam
/// this module's own tests use to stay hermetic, like
/// [`get_app_settings_inner`]. Validates before writing anything.
fn update_app_settings_at(
    base: &std::path::Path,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    let domain: rim_session::AppSettings = settings.try_into()?;
    let use_case =
        rim_session::use_cases::UpdateAppSettings::new(rim_io::JsonAppSettingsStore::new());
    use_case.execute(base, domain)?;
    Ok(domain.into())
}

/// Restores just the network half of the app settings to its defaults —
/// every switch back on — leaving the reminder threshold untouched. The
/// Settings page's own "Restore network defaults" action; separate from
/// [`update_app_settings`] so the frontend never has to know
/// [`rim_session::NetworkPolicy::default`] itself.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails.
#[tauri::command]
pub async fn reset_network_policy() -> Result<AppSettingsDto, CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to write \
             app-settings.json",
        )
    })?;
    reset_network_policy_at(&base)
}

/// [`reset_network_policy`], with `base` threaded in explicitly — see
/// [`update_app_settings_at`].
fn reset_network_policy_at(base: &std::path::Path) -> Result<AppSettingsDto, CommandError> {
    let use_case =
        rim_session::use_cases::ResetNetworkPolicy::new(rim_io::JsonAppSettingsStore::new());
    let result = use_case.execute(base)?;
    Ok(result.into())
}

/// Turns on every recommended rule-database source (which sources are
/// recommended is decided in Rust, never by the frontend) and returns the
/// saved settings. The caller follows it with the ordinary manual
/// `refresh_rule_databases`: the click that invoked this is the consent
/// to download. Never touches internet access itself.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the settings file is damaged or can't be written.
#[tauri::command]
pub async fn enable_recommended_rule_databases() -> Result<AppSettingsDto, CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to write \
             app-settings.json",
        )
    })?;
    enable_recommended_rule_databases_at(&base)
}

/// [`enable_recommended_rule_databases`], with `base` threaded in
/// explicitly — see [`update_app_settings_at`].
fn enable_recommended_rule_databases_at(
    base: &std::path::Path,
) -> Result<AppSettingsDto, CommandError> {
    let use_case =
        rim_session::use_cases::EnableRecommendedSources::new(rim_io::JsonAppSettingsStore::new());
    Ok(use_case.execute(base)?.into())
}

/// The sorter/ledger settings' own hard-coded defaults
/// ([`rim_session::Settings::default()`]) — never the current project's
/// settings, and never persisted. The Settings page's "Reset to
/// defaults" reads this to refill its form (the user still has to press
/// Save) rather than duplicating the default table in TypeScript. Takes
/// no session state, so it works even before a project is loaded.
#[tauri::command]
pub async fn get_default_settings() -> SettingsDto {
    rim_session::Settings::default().into()
}

#[cfg(test)]
mod tests {
    use crate::dto::settings::{AppSettingsLoadStatusDto, ReminderPolicyDto};

    use super::*;

    fn dto_with_stale_days(days: u16) -> AppSettingsDto {
        let mut dto: AppSettingsDto = rim_session::AppSettings::default().into();
        dto.reminders = ReminderPolicyDto {
            rule_databases_stale_after_days: days,
        };
        dto
    }

    #[test]
    fn update_app_settings_writes_the_settings_and_returns_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut requested = dto_with_stale_days(90);
        requested.network.allow_network = false;

        let returned = update_app_settings_at(dir.path(), requested).expect("must succeed");

        assert_eq!(returned, requested);
        let reloaded = get_app_settings_inner(dir.path());
        assert_eq!(reloaded.load_status, AppSettingsLoadStatusDto::Loaded);
        assert_eq!(reloaded.settings, requested);
    }

    #[test]
    fn update_app_settings_rejects_an_out_of_range_stale_threshold_and_writes_nothing() {
        for days in [0, 366] {
            let dir = tempfile::tempdir().expect("tempdir");

            let error = update_app_settings_at(dir.path(), dto_with_stale_days(days))
                .expect_err("an out-of-range threshold must be rejected");

            assert_eq!(error.code, crate::error::CommandErrorCode::InvalidInput);
            assert!(
                !dir.path().join("app-settings.json").exists(),
                "a rejected {days}-day threshold must not create the file"
            );
        }
    }

    #[test]
    fn reset_network_policy_turns_every_switch_on_and_keeps_the_reminders() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut off = dto_with_stale_days(90);
        off.network.allow_network = false;
        off.network.check_for_updates = false;
        off.network.auto_refresh_rule_databases = false;
        update_app_settings_at(dir.path(), off).expect("seed");

        let returned = reset_network_policy_at(dir.path()).expect("must succeed");

        let defaults: AppSettingsDto = rim_session::AppSettings::default().into();
        assert_eq!(returned.network, defaults.network);
        assert_eq!(returned.reminders.rule_databases_stale_after_days, 90);
        let reloaded = get_app_settings_inner(dir.path());
        assert_eq!(reloaded.settings, returned);
    }

    #[tokio::test]
    async fn get_default_settings_reports_the_pinned_default_threshold() {
        // Not `assert_eq!(dto, rim_session::Settings::default().into())` —
        // `get_default_settings`'s own body is exactly that expression, so
        // comparing against it would be tautological (true regardless of
        // what `Settings::default()` actually contains). This instead
        // pins a concrete, independently-known value.
        let dto = get_default_settings().await;
        assert_eq!(dto.threshold, 80);
    }

    #[test]
    fn get_app_settings_reports_missing_for_a_fresh_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        let response = get_app_settings_inner(dir.path());
        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Missing);
        assert!(response.settings.network.allow_network);
    }

    #[test]
    fn get_app_settings_reports_recovered_for_a_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("app-settings.json"), b"not json at all")
            .expect("write corrupt file");

        let response = get_app_settings_inner(dir.path());

        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Recovered);
        assert!(!response.settings.network.allow_network);
    }

    #[test]
    fn get_app_settings_reports_loaded_for_a_saved_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut settings = rim_session::AppSettings::default();
        settings.network.allow_network = false;
        rim_io::JsonAppSettingsStore::new()
            .save(dir.path(), &settings)
            .expect("save");

        let response = get_app_settings_inner(dir.path());

        assert_eq!(response.load_status, AppSettingsLoadStatusDto::Loaded);
        assert!(!response.settings.network.allow_network);
    }

    #[test]
    fn enable_recommended_rule_databases_turns_sources_on_and_persists_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut seeded = dto_with_stale_days(30);
        seeded.network.fetch_steam_workshop = false;
        seeded.network.allow_network = false;
        update_app_settings_at(dir.path(), seeded).expect("seed");

        let returned = enable_recommended_rule_databases_at(dir.path()).expect("must succeed");

        assert!(returned.network.fetch_steam_workshop);
        assert!(!returned.network.allow_network);
        assert_eq!(get_app_settings_inner(dir.path()).settings, returned);
    }

    #[test]
    fn enable_recommended_rule_databases_leaves_a_damaged_file_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("app-settings.json");
        std::fs::write(&path, b"not json").expect("seed corrupt file");

        let error = enable_recommended_rule_databases_at(dir.path()).expect_err("must refuse");

        assert_eq!(error.code, crate::error::CommandErrorCode::ProfileIoFailed);
        assert_eq!(std::fs::read(&path).expect("read back"), b"not json");
    }
}
