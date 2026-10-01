//! `get_rule_databases`/`refresh_rule_databases`: the rules page's
//! Databases card. Neither
//! command emits `session://changed` — a refresh touches only the global
//! rule-database cache, never the session, `rules.json`, or
//! `<profile>/imports/`, so there is nothing for
//! the frontend to invalidate beyond re-fetching this card's own query.
//!
//! `refresh_rule_databases_with`'s real fetch runs on a **bare**
//! `spawn_blocking`, never inside `with_session`'s own — see that
//! function's own doc comment for why holding the session lock across a
//! real network fetch would be a real, user-visible problem, not just a
//! style nit.

use std::path::PathBuf;

use rim_session::ports::{AppSettingsStore, RuleDatabase};
use rim_session::use_cases::RefreshRuleDatabases;
use rim_session::{AppSettings, NetworkPolicy, StaleAfterDays};

use crate::commands::project::profile_base;
use crate::dto::rule_databases::{
    RefreshRuleDatabasesRequestDto, RuleDatabaseRefreshResultDto, RuleDatabaseViewDto,
    rule_database_view_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Every requested database whenever a caller names no sources — the
/// desktop card's own Refresh button refreshes every enabled source in
/// one call, `execute` itself decides which of these are actually
/// enabled, the same as `apps/cli`'s own `db refresh` no-flag default.
const ALL_DATABASES: [RuleDatabase; 3] = [
    RuleDatabase::CommunityRules,
    RuleDatabase::SteamWorkshop,
    RuleDatabase::RimmergeRules,
];

/// The global rule-database cache directory: `rim_io::databases_dir`
/// under the same base [`profile_base`] already resolves `profile_dir`s
/// against, so the Playwright CDP smoke tier's `RIMMERGE_PROFILE_DIR`
/// override relocates this too.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when neither `RIMMERGE_PROFILE_DIR`
/// nor `LOCALAPPDATA` is set — the same condition under which
/// `get_default_paths`' own `profile_dir` already comes back `None`.
pub(crate) fn cache_dir() -> Result<PathBuf, CommandError> {
    profile_base()
        .map(|base| rim_io::databases_dir(&base))
        .ok_or_else(|| {
            CommandError::internal("could not resolve the rule-database cache directory")
        })
}

/// The app-global [`AppSettings`], read through
/// [`rim_io::JsonAppSettingsStore`] against the same
/// [`profile_base`] `cache_dir` resolves against — never a per-profile
/// `Settings` field (see `rim_session::app_settings::NetworkPolicy`'s own
/// doc comment for why network policy moved out of `Settings`). Never
/// fails outright: a base that fails to resolve reads as
/// [`AppSettings::default`], the same "degrade to shipped behaviour"
/// every other optional app-global lookup in this crate follows.
pub(crate) fn app_settings() -> AppSettings {
    profile_base().map_or_else(AppSettings::default, |base| {
        rim_io::JsonAppSettingsStore::new().load(&base).settings()
    })
}

/// [`app_settings`]`().network` — kept as its own function since most
/// callers only ever need the network half.
pub(crate) fn network_policy() -> NetworkPolicy {
    app_settings().network
}

/// Lists each rule database's cache status, enriched with this profile's
/// own re-import signal.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the cache directory can't be
/// resolved.
pub(crate) async fn get_rule_databases_inner(
    state: &AppState,
) -> Result<Vec<RuleDatabaseViewDto>, CommandError> {
    let settings = app_settings();
    get_rule_databases_with(
        state,
        cache_dir()?,
        settings.network,
        settings.reminders.rule_databases_stale_after_days,
    )
    .await
}

/// [`get_rule_databases_inner`], with the cache directory, network
/// policy, and stale threshold all threaded in explicitly rather than
/// resolved from the real `RIMMERGE_PROFILE_DIR`/`LOCALAPPDATA`
/// environment and `app-settings.json` — the seam this module's own
/// tests use to stay hermetic: a test that called
/// [`cache_dir`]/[`app_settings`] directly would depend on this
/// machine's own real `%LOCALAPPDATA%\rimmerge` state, and would start
/// failing (or, worse, silently reaching the real network) the moment
/// that state changed — the exact machine-dependent flake the
/// production command must never have, and a test must never mask.
async fn get_rule_databases_with(
    state: &AppState,
    cache_dir: PathBuf,
    policy: NetworkPolicy,
    stale_after_days: StaleAfterDays,
) -> Result<Vec<RuleDatabaseViewDto>, CommandError> {
    with_session(state, move |session| {
        let profile_dir = session.paths().profile_dir.clone();
        let use_case = RefreshRuleDatabases::new(
            rim_io::GithubRuleDatabaseFetcher::new(),
            rim_io::JsonImportManifestStore::new(),
        );
        let now = jiff::Timestamp::now();
        Ok(use_case
            .status(&policy, &cache_dir, &profile_dir)
            .iter()
            .map(|view| rule_database_view_dto(view, now, stale_after_days))
            .collect())
    })
    .await
}

/// See [`get_rule_databases_inner`].
///
/// # Errors
///
/// See [`get_rule_databases_inner`].
#[tauri::command]
pub async fn get_rule_databases(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RuleDatabaseViewDto>, CommandError> {
    get_rule_databases_inner(&state).await
}

/// Refreshes rule databases into the global cache, honouring
/// `NetworkPolicy::allow_network` (the offline switch)
/// and each source's own fetch toggle — see
/// [`RefreshRuleDatabases::execute`]'s own doc comment for the exact
/// enforcement order. Runs on the session's blocking thread (a real
/// network fetch, tens of MB for the Steam Workshop database), which is
/// exactly what [`with_session`]'s `spawn_blocking` is for.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request` names no
/// source, [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the cache directory can't be
/// resolved. A per-source fetch failure is never an error here — it comes
/// back as [`rim_session::ports::RefreshOutcome::Failed`] inside the
/// returned list, exactly like every other outcome.
pub(crate) async fn refresh_rule_databases_inner(
    state: &AppState,
    request: Option<RefreshRuleDatabasesRequestDto>,
) -> Result<Vec<RuleDatabaseRefreshResultDto>, CommandError> {
    let requested = requested_databases(request)?;
    refresh_rule_databases_with(state, cache_dir()?, network_policy(), requested).await
}

/// The sources a refresh call asks for: every source when `request` is
/// absent, exactly the named ones otherwise.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] for a request that names no
/// source: an empty list would silently refresh nothing, which is never
/// what a caller meant.
fn requested_databases(
    request: Option<RefreshRuleDatabasesRequestDto>,
) -> Result<Vec<RuleDatabase>, CommandError> {
    let Some(request) = request else {
        return Ok(ALL_DATABASES.to_vec());
    };
    if request.sources.is_empty() {
        return Err(CommandError::invalid_input(
            "a refresh request must name at least one rule database",
        ));
    }
    Ok(request
        .sources
        .into_iter()
        .map(RuleDatabase::from)
        .collect())
}

/// See [`get_rule_databases_with`]'s own doc comment — the identical
/// hermetic-test seam, for the refresh side.
///
/// **Never fetches inside [`with_session`].** The use case here needs no
/// live `Session` at all (`cache_dir`/`policy` are threaded in directly,
/// same as `commands::notifications`'s `check_for_update_with`), so
/// `with_session` is used only for its brief `no_project_loaded` gate —
/// the real fetch (up to three HTTP requests, tens of MB for the Steam
/// Workshop database) runs on a bare `spawn_blocking` afterward, never
/// while `with_session`'s own `spawn_blocking` closure is holding the
/// session write lock. Holding that lock across a real network fetch
/// would block every other command needing the session (`list_mods`,
/// `findings`, …) for the whole fetch's duration.
async fn refresh_rule_databases_with(
    state: &AppState,
    cache_dir: PathBuf,
    policy: NetworkPolicy,
    requested: Vec<RuleDatabase>,
) -> Result<Vec<RuleDatabaseRefreshResultDto>, CommandError> {
    with_session(state, |_session| Ok(())).await?;

    tauri::async_runtime::spawn_blocking(move || {
        let use_case = RefreshRuleDatabases::new(
            rim_io::GithubRuleDatabaseFetcher::new(),
            rim_io::JsonImportManifestStore::new(),
        );
        Ok(use_case
            .execute(&policy, &cache_dir, &requested)
            .into_iter()
            .map(|(database, outcome)| RuleDatabaseRefreshResultDto {
                database: database.into(),
                outcome: outcome.into(),
            })
            .collect())
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "refresh_rule_databases task panicked");
        Err(CommandError::internal("background task panicked"))
    })
}

/// See [`refresh_rule_databases_inner`]. Deliberately does **not** emit
/// `session://changed` — see this module's own doc comment.
///
/// # Errors
///
/// See [`refresh_rule_databases_inner`].
#[tauri::command]
pub async fn refresh_rule_databases(
    state: tauri::State<'_, AppState>,
    request: Option<RefreshRuleDatabasesRequestDto>,
) -> Result<Vec<RuleDatabaseRefreshResultDto>, CommandError> {
    refresh_rule_databases_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::rule_databases::RuleDatabaseDto;
    use crate::test_support::session_fixture_with_temp_paths;

    /// `get_rule_databases` against the default `NetworkPolicy` (community
    /// on, steam off) and an empty (never-touched) scratch cache directory
    /// — both sources come back `enabled` matching their own toggle,
    /// `cached: None`, `needsReimport: false` (nothing cached to need
    /// re-importing).
    #[tokio::test]
    async fn get_rule_databases_reports_every_source_never_fetched() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let views = get_rule_databases_with(
            &state,
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            StaleAfterDays::default(),
        )
        .await
        .expect("get_rule_databases must succeed");

        assert_eq!(views.len(), 3);
        let community = views
            .iter()
            .find(|v| v.database == crate::dto::rule_databases::RuleDatabaseDto::Community)
            .expect("community row");
        assert!(community.enabled, "fetch_community_rules defaults to true");
        assert_eq!(community.cached, None);
        assert!(!community.needs_reimport);

        let steam = views
            .iter()
            .find(|v| v.database == crate::dto::rule_databases::RuleDatabaseDto::Steam)
            .expect("steam row");
        assert!(steam.enabled, "fetch_steam_workshop defaults to true");

        let rimmerge = views
            .iter()
            .find(|v| v.database == crate::dto::rule_databases::RuleDatabaseDto::Rimmerge)
            .expect("rimmerge rules row");
        assert!(rimmerge.enabled, "fetch_rimmerge_rules defaults to true");
        assert!(
            !rimmerge.needs_reimport,
            "this source is read from the cache, never imported into a profile"
        );
    }

    /// The failure path: no project loaded at all.
    #[tokio::test]
    async fn get_rule_databases_fails_with_no_project_loaded() {
        let state = AppState::default();
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let result = get_rule_databases_with(
            &state,
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            StaleAfterDays::default(),
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    /// `refresh_rule_databases` with the network switch off: every
    /// requested source comes back `Skipped { NetworkDisabled }`, and —
    /// since this is an enforced, not merely documented, guarantee — never
    /// opens a socket (there is no fake seam to assert a
    /// call count through at this layer, but a real `GithubRuleDatabaseFetcher`
    /// would hang or fail this test outright if the guard in
    /// `RefreshRuleDatabases::execute` were ever bypassed, since this test
    /// runs with no network available in CI).
    #[tokio::test]
    async fn refresh_rule_databases_with_network_off_skips_every_source() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let policy = NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        };

        let results = refresh_rule_databases_with(
            &state,
            cache_dir.path().to_path_buf(),
            policy,
            ALL_DATABASES.to_vec(),
        )
        .await
        .expect("refresh_rule_databases must succeed");

        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(matches!(
                result.outcome,
                crate::dto::rule_databases::RefreshOutcomeDto::Skipped {
                    reason: crate::dto::rule_databases::SkipReasonDto::NetworkDisabled
                }
            ));
        }
    }

    /// The failure path: no project loaded at all.
    #[tokio::test]
    async fn refresh_rule_databases_fails_with_no_project_loaded() {
        let state = AppState::default();
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let result = refresh_rule_databases_with(
            &state,
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            ALL_DATABASES.to_vec(),
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    /// The stale notice names its sources and the refresh touches exactly
    /// those: asking for one source answers for that one, never for the
    /// other two (the Steam Workshop download in particular).
    #[tokio::test]
    async fn a_named_source_list_is_the_whole_refresh() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let policy = NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        };
        let requested = requested_databases(Some(RefreshRuleDatabasesRequestDto {
            sources: vec![
                RuleDatabaseDto::Community,
                RuleDatabaseDto::Rimmerge,
                RuleDatabaseDto::Community,
            ],
        }))
        .expect("a non-empty request is valid");

        let results =
            refresh_rule_databases_with(&state, cache_dir.path().to_path_buf(), policy, requested)
                .await
                .expect("refresh_rule_databases must succeed");

        let databases: Vec<RuleDatabaseDto> = results.iter().map(|r| r.database).collect();
        assert_eq!(
            databases,
            [RuleDatabaseDto::Community, RuleDatabaseDto::Rimmerge]
        );
    }

    #[test]
    fn no_request_means_every_source() {
        assert_eq!(
            requested_databases(None).expect("no request is valid"),
            ALL_DATABASES.to_vec()
        );
    }

    #[test]
    fn an_empty_source_list_is_refused_rather_than_refreshing_nothing() {
        let error = requested_databases(Some(RefreshRuleDatabasesRequestDto {
            sources: Vec::new(),
        }))
        .expect_err("an empty list names nothing to refresh");

        assert_eq!(error.code, crate::error::CommandErrorCode::InvalidInput);
    }
}
