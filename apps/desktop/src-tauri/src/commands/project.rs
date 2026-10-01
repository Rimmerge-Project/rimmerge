//! `load_project`/`get_default_paths`/`get_default_rimsort_paths`:
//! scanning a RimWorld install into a fresh session, and prefilling the
//! setup/rules pages' folder pickers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rim_resolve::domain::OrderSource;
use rim_session::ProjectPaths;
use rim_session::ports::DefSourceReader;
use tauri::async_runtime::spawn_blocking;

use crate::dto::project::{
    AppConfigDto, DefaultPathsDto, DefaultPathsErrorDto, DefaultPathsWarningDto, ProgressEventDto,
    ProjectPathsDto, ProjectSummaryDto, RuleWarningDto, ScanWarningDto,
};
use crate::dto::rule::RimSortPathsDto;
use crate::error::CommandError;
use crate::state::{AppState, replace_session, with_session};

/// The order a freshly loaded project opens on. The desktop's own
/// presentation default, so it lives here in the composition root and not
/// in `Session::new`: the CLI's `ledger`/`defs`/`patch` read the session's
/// default selection and stay on the current order.
const INITIAL_ORDER_SOURCE: OrderSource = OrderSource::Suggested;

/// Builds a [`ProjectSummaryDto`] from a freshly (re)scanned `session` and
/// wires in its `DefSourceReader` — the shared tail [`load_project_inner`]
/// and `commands::active_set::rescan_project_inner` both need, so a real
/// load and a rescan
/// report identically shaped summaries from one place instead of two
/// hand-copied bodies drifting apart. Takes `def_reader` directly rather
/// than `&Adapters`: both call sites move the rest of `Adapters`' fields
/// into a `LoadProject` first, which partially moves the struct and makes
/// borrowing it as a whole afterward a compile error — a single already-
/// cloned field has no such restriction.
pub(crate) fn finish_loaded_session(
    session: &mut rim_session::Session,
    def_reader: Arc<dyn DefSourceReader + Send + Sync>,
    start: Instant,
) -> ProjectSummaryDto {
    // Gives the session a real
    // `DefSourceReader` so `Session::redecide_clean_merge_at`'s lazy
    // clean-merge preview pass can compute a missing preview instead of
    // only reusing whatever's already cached.
    session.set_def_source_reader(def_reader);
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    let report = session.report();
    let warnings: Vec<ScanWarningDto> = report.warnings.iter().map(ScanWarningDto::from).collect();
    // The rules file's own load warnings (a pre-migration `rules.json`
    // whose cluster rules were dropped on load, plus the mod-knowledge
    // data's own notes) are kept separate from the scan's own
    // notes above: they're actionable and few, so the frontend keeps
    // toasting them individually rather than folding them into "Scan
    // notes" (see `ProjectSummaryDto::rule_warnings`'s own doc comment).
    let rule_warnings: Vec<RuleWarningDto> = session
        .rule_load_warnings()
        .iter()
        .map(RuleWarningDto::from)
        .collect();

    // Silently seeds this profile's own acknowledged game version on its
    // very first load — never fires `GameVersionChanged` on that first
    // load, since there's nothing yet to compare against. An
    // already-recorded acknowledgement is left untouched; a genuine
    // change is picked up by `evaluate()` on the next `list_notifications`
    // call. A failed write here is a non-fatal notice-feature nicety,
    // never worth failing the whole load over.
    let profile_dir = session.paths().profile_dir.clone();
    let current_game_version =
        rim_session::notifications::GameMajorMinor::parse(&report.metadata.game_version);
    let sync = rim_session::use_cases::SyncGameVersionAcknowledgement::new(
        rim_io::JsonProfileNotificationStateStore::new(),
    );
    if let Err(error) = sync.execute(&profile_dir, &current_game_version) {
        tracing::warn!(error = %error, "failed to sync game-version acknowledgement");
    }

    ProjectSummaryDto {
        mod_count: report.mods.len(),
        game_version: report.metadata.game_version.clone(),
        elapsed_ms,
        warnings,
        rule_warnings,
        selected: session.selected().into(),
    }
}

/// Scans `paths` and loads persisted decisions/rules into a fresh
/// session, reporting progress through `on_progress` as the scan
/// proceeds, then replaces [`AppState::session`][crate::state::AppState::session]
/// with it via [`replace_session`].
///
/// Held for the whole call under [`AppState::load_lock`][crate::state::AppState::load_lock]:
/// two concurrent loads racing to scan and then replace the session could
/// otherwise interleave, leaving the session built from one call's paths
/// but the summary returned from the other's.
///
/// # Errors
///
/// Returns [`CommandError`] when reading `ModsConfig.xml`, scanning, or
/// loading persisted decisions/rules fails.
pub(crate) async fn load_project_inner(
    state: &AppState,
    paths: ProjectPathsDto,
    mut on_progress: impl FnMut(ProgressEventDto) + Send + 'static,
) -> Result<ProjectSummaryDto, CommandError> {
    let _permit = state.load_lock.lock().await;

    let adapters = state.adapters.clone();
    let project_paths: ProjectPaths = paths.into();

    let (session, summary) = spawn_blocking(move || {
        let use_case = rim_session::use_cases::LoadProject::new(
            adapters.scanner,
            adapters.config_store,
            adapters.decision_store,
            adapters.rule_store,
            adapters.patch_store,
            adapters.assignment_store,
            adapters.mod_knowledge_store.clone(),
            crate::commands::rules_databases::network_policy().fetch_rimmerge_rules,
        );

        let start = Instant::now();
        let def_reader = adapters.def_reader.clone();
        let mut session = use_case.execute(project_paths, &mut |progress| {
            on_progress(progress.into());
        })?;
        rim_session::use_cases::SelectOrder::new().execute(&mut session, INITIAL_ORDER_SOURCE);
        let summary = finish_loaded_session(&mut session, def_reader, start);

        Ok::<_, CommandError>((session, summary))
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "load_project scan task panicked");
        Err(CommandError::internal("background task panicked"))
    })?;

    replace_session(state, session).await?;
    Ok(summary)
}

/// See [`load_project_inner`]; the [`tauri::AppHandle`] here only exists
/// to emit [`crate::commands::EVENT_PROJECT_PROGRESS`] — the inner
/// function takes a plain callback so it runs without a live Tauri
/// runtime.
///
/// # Errors
///
/// See [`load_project_inner`].
#[tauri::command]
pub async fn load_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    paths: ProjectPathsDto,
) -> Result<ProjectSummaryDto, CommandError> {
    use tauri::Emitter;

    load_project_inner(&state, paths, move |progress| {
        let _ = app.emit(crate::commands::EVENT_PROJECT_PROGRESS, progress);
    })
    .await
}

/// The base directory `profile_dir`s are hashed under: `RIMMERGE_PROFILE_DIR`
/// when set (the Playwright CDP smoke tier points this at a scratch
/// directory so it never touches the real profile store), else
/// `%LOCALAPPDATA%\rimmerge`. `pub(crate)` so
/// `commands::rules_databases` can derive the same
/// `rim_io::databases_dir`-relative cache directory `apps/cli`'s own
/// `--cache-dir` default uses — the one base every interface's
/// `profile_dir`/cache directory is hashed under.
pub(crate) fn profile_base() -> Option<PathBuf> {
    std::env::var("RIMMERGE_PROFILE_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("LOCALAPPDATA").map(|local| PathBuf::from(local).join("rimmerge"))
        })
        .ok()
}

/// The default game/workshop/`ModsConfig.xml`/profile paths, straight
/// off [`rim_io::resolve_project_paths`] — the one precedence ladder
/// `apps/cli`'s own
/// `resolve_paths` also goes through, so the two interfaces can never
/// disagree about where this machine's install is. Touches the
/// filesystem only to read `config.json` and, during detection, each
/// candidate's `Version.txt`/`Data/Core/` — no scan, no
/// `ModsConfig.xml` read.
///
/// Never an `Err`: a machine with no detectable install still has to see
/// the setup form, so the failure comes back as
/// [`DefaultPathsDto::error`] with every candidate location named, and a
/// resolved-but-suspicious install as [`DefaultPathsDto::warning`].
///
/// Deliberately **not** `spawn_blocking`, unlike every command that
/// touches the session: the whole call is a handful of `stat`s (each
/// candidate's `Version.txt`/`Data/Core/`, plus one small `config.json`
/// read), which is far below the threshold where moving work off the
/// async runtime pays for the task hop.
#[tauri::command]
pub async fn get_default_paths() -> DefaultPathsDto {
    let empty = DefaultPathsDto {
        game_dir: None,
        workshop_dir: None,
        mods_config: None,
        profile_dir: None,
        warning: None,
        warning_code: None,
        error: None,
        error_code: None,
    };
    let Some(base) = profile_base() else {
        return DefaultPathsDto {
            error: Some(
                "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so rimmerge has nowhere \
                 to keep its own files"
                    .to_string(),
            ),
            error_code: Some(DefaultPathsErrorDto::NoProfileBase),
            ..empty
        };
    };
    match rim_io::resolve_project_paths(rim_io::PathOverrides::new(base)) {
        Ok(resolved) => {
            let warning_code =
                resolved
                    .warning
                    .as_ref()
                    .map(|_| DefaultPathsWarningDto::NotAnInstall {
                        game_dir: resolved.paths.game_dir.display().to_string(),
                    });
            DefaultPathsDto {
                game_dir: Some(resolved.paths.game_dir.display().to_string()),
                workshop_dir: Some(resolved.paths.workshop_dir.display().to_string()),
                mods_config: Some(resolved.paths.mods_config.display().to_string()),
                profile_dir: Some(resolved.paths.profile_dir.display().to_string()),
                warning: resolved.warning,
                warning_code,
                error: None,
                error_code: None,
            }
        }
        Err(error) => DefaultPathsDto {
            error_code: Some(path_resolution_error_to_dto(&error)),
            error: Some(error.to_string()),
            ..empty
        },
    }
}

/// [`rim_io::PathResolutionError`] -> [`DefaultPathsErrorDto`], a pure
/// mapping pulled out of [`get_default_paths`] so it can be unit tested
/// without touching real environment variables or the filesystem (the
/// ladder itself is `rim_io`'s own to test, against its injectable
/// `Environment` trait — this is just the DTO shape on top of it).
fn path_resolution_error_to_dto(error: &rim_io::PathResolutionError) -> DefaultPathsErrorDto {
    match error {
        rim_io::PathResolutionError::GameDirNotFound { candidates } => {
            DefaultPathsErrorDto::GameDirNotFound {
                candidates: candidates
                    .iter()
                    .map(|candidate| candidate.display().to_string())
                    .collect(),
            }
        }
        rim_io::PathResolutionError::ModsConfigNotFound { reason } => {
            DefaultPathsErrorDto::ModsConfigNotFound {
                reason: reason.clone(),
            }
        }
    }
}

/// Pins `config`'s three paths into `<base>/config.json`
/// ([`rim_io::AppConfig`]) so the next launch prefills them without
/// detection — the Setup page's "Save as defaults", and the desktop
/// counterpart of `rimmerge config set`.
///
/// An empty string unpins that path (the Setup page's field is cleared
/// rather than absent), so what is written is always exactly what the
/// form shows.
///
/// # Errors
///
/// Returns [`CommandError`] when no base directory can be resolved, or
/// when the write fails.
#[tauri::command]
pub async fn save_app_config(config: AppConfigDto) -> Result<(), CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to write \
             config.json",
        )
    })?;
    rim_io::AppConfig {
        game_dir: pinned_path(config.game_dir),
        workshop_dir: pinned_path(config.workshop_dir),
        mods_config: pinned_path(config.mods_config),
    }
    .save(&base)
    .map_err(CommandError::from)
}

/// A form field as a pinned path: blank (or absent) means "unpin", never
/// "pin the empty path", and whatever survives is absolutized through
/// [`rim_io::pinned_path`] — `config.json` outlives the working directory
/// the app was launched from, so a relative path stored here would mean
/// something different on the next start.
fn pinned_path(value: Option<String>) -> Option<PathBuf> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .map(|text| rim_io::pinned_path(&PathBuf::from(text)))
}

/// RimSort's own default database directory, `%LOCALAPPDATA%\RimSort\dbs`,
/// prefilling the rules page's import dialog — the same layout
/// `apps/cli`'s own `import` command expects (`userRules.json` directly
/// under it, `communityRules.json`/`steamDB.json` under their
/// subfolders). Returns `None` for the whole struct when `%LOCALAPPDATA%`
/// itself isn't set; each of the three fields is `None` **individually**
/// when that specific file doesn't exist on disk, so a machine with no
/// RimSort install at all prefills nothing and reaches the all-`None`
/// import instead of failing with a hard `ImportError` on three
/// nonexistent paths.
///
/// Community/steam fall back to the global rule-database cache
/// (`rim_session::resolve_from_cache`, the same function `apps/cli`'s own
/// `import --from-cache` uses) when RimSort's own file isn't there, so a
/// user with no local RimSort install who fetched the cache can import
/// from it and clear the Databases card's re-import badge.
/// Falling back is attempted **per field independently** (a RimSort
/// install missing only `steamDB.json`, say, still gets its community
/// rules from the local file and steam from the cache, not one or the
/// other exclusively), and only when at least one of the two is missing
/// locally — an install that already has both files never touches the
/// cache or the session lock at all. The cache lookup honours
/// `NetworkPolicy::fetch_community_rules`/`fetch_steam_workshop` (a source
/// disabled there is never suggested from the cache, matching
/// `should_import_from_cache`'s own CLI-side rule) and fails soft: if no
/// project is loaded yet or the cache directory can't be resolved, this
/// simply falls back to the RimSort-only prefill, never a hard error on
/// what is, after all, only a form's default values.
#[tauri::command]
pub async fn get_default_rimsort_paths(
    state: tauri::State<'_, AppState>,
) -> Result<Option<RimSortPathsDto>, CommandError> {
    Ok(get_default_rimsort_paths_inner(&state).await)
}

/// See [`get_default_rimsort_paths`].
pub(crate) async fn get_default_rimsort_paths_inner(state: &AppState) -> Option<RimSortPathsDto> {
    let dbs_dir = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?)
        .join("RimSort")
        .join("dbs");
    let existing_path = |path: PathBuf| path.is_file().then(|| path.display().to_string());

    let local_community = existing_path(
        dbs_dir
            .join("Community-Rules-Database")
            .join("communityRules.json"),
    );
    let local_steam = existing_path(dbs_dir.join("Steam-Workshop-Database").join("steamDB.json"));

    let (cache_community, cache_steam) = if local_community.is_some() && local_steam.is_some() {
        (None, None)
    } else {
        match crate::commands::rules_databases::cache_dir() {
            Ok(cache_dir) => {
                let policy = crate::commands::rules_databases::network_policy();
                cache_fallback_paths(state, cache_dir, policy).await
            }
            Err(_) => (None, None),
        }
    };

    Some(RimSortPathsDto {
        user_rules: existing_path(dbs_dir.join("userRules.json")),
        community_rules: local_community.or(cache_community),
        steam_db: local_steam.or(cache_steam),
    })
}

/// The global-cache half of [`get_default_rimsort_paths_inner`]'s
/// fallback. Takes `cache_dir` explicitly, rather than resolving
/// [`crate::commands::rules_databases::cache_dir`] itself, for the
/// identical hermetic-test reason that module's own `_with_cache_dir`
/// seam exists — a real `%LOCALAPPDATA%\rimmerge\databases`
/// appearing on the machine a test runs on must never change this test's
/// outcome. Fails soft to `(None, None)` when no project is loaded —
/// this is a form's own default values, not something worth a hard error
/// over.
async fn cache_fallback_paths(
    state: &AppState,
    cache_dir: PathBuf,
    policy: rim_session::NetworkPolicy,
) -> (Option<String>, Option<String>) {
    let (community, steam) = with_session(state, move |_session| {
        let fetcher = rim_io::GithubRuleDatabaseFetcher::new();
        let paths = rim_session::resolve_from_cache(&fetcher, &policy, &cache_dir, None);
        Ok((paths.community_rules, paths.steam_db))
    })
    .await
    .unwrap_or((None, None));
    (
        community.map(|path| path.display().to_string()),
        steam.map(|path| path.display().to_string()),
    )
}

#[cfg(test)]
mod tests {
    use rim_session::test_support::report_fixture;

    use super::*;

    /// A `LoadProject` use case whose adapters are all in-memory: doesn't
    /// exercise `load_project_inner` directly (that needs a real
    /// `Adapters`, which only wraps real `rim_io` file adapters — see
    /// `AppState::adapters`'s own doc comment), but proves the
    /// `load_lock`/`replace_session` plumbing works by driving it the
    /// same way the real command does, on a session built in memory.
    #[tokio::test]
    async fn replace_session_makes_the_session_visible_afterward() {
        let state = AppState::default();
        // `AppState::adapters` always wraps the real file-based `rim_io`
        // adapters (see its own doc comment), so exercising the full
        // `load_project_inner` here would touch real paths; this checks
        // the piece `load_project_inner` actually adds — storing the
        // freshly built session via `replace_session` — directly.
        let session = rim_session::Session::new(
            rim_session::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report_fixture(&["a"]),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            rim_session::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );

        replace_session(&state, session)
            .await
            .expect("replace_session must succeed");

        assert!(state.session.read().expect("lock").is_some());
    }

    // -- `load_project_inner` --

    #[tokio::test]
    async fn load_project_opens_on_the_suggested_order_and_the_session_agrees() {
        let (_scratch, paths) = crate::test_support::scratch_sample_game();
        let state = AppState::default();

        let summary = load_project_inner(&state, paths, |_| {})
            .await
            .expect("loading the scratch sample game must succeed");

        assert_eq!(
            summary.selected,
            crate::dto::common::OrderSourceDto::Suggested
        );
        let selected = state
            .session
            .read()
            .expect("lock")
            .as_ref()
            .map(rim_session::Session::selected);
        assert_eq!(selected, Some(OrderSource::Suggested));
    }

    // -- `cache_fallback_paths` --

    /// A project with the default `NetworkPolicy` (community fetch on, steam
    /// off) and a real cached `communityRules.json` on disk resolves that
    /// path; steam stays `None` since its own fetch toggle is off, even
    /// though `resolve_from_cache` never even needs to check for a real
    /// steam file to reach that answer.
    #[tokio::test]
    async fn cache_fallback_paths_resolves_only_the_enabled_and_cached_source() {
        let steam_off = rim_session::NetworkPolicy {
            fetch_steam_workshop: false,
            ..rim_session::NetworkPolicy::default()
        };
        let (_temp_dir, session) = crate::test_support::session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let community_path = cache_dir.path().join("communityRules.json");
        std::fs::write(&community_path, b"{}").expect("write community fixture");

        let (community, steam) =
            cache_fallback_paths(&state, cache_dir.path().to_path_buf(), steam_off).await;

        assert_eq!(community, Some(community_path.display().to_string()));
        assert_eq!(steam, None, "a disabled source is never suggested");
    }

    /// No project loaded: fails soft to `(None, None)` rather than
    /// erroring — this is only ever a dialog's own default values.
    #[tokio::test]
    async fn cache_fallback_paths_fails_soft_with_no_project_loaded() {
        let state = AppState::default();
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let (community, steam) = cache_fallback_paths(
            &state,
            cache_dir.path().to_path_buf(),
            rim_session::NetworkPolicy::default(),
        )
        .await;

        assert_eq!(community, None);
        assert_eq!(steam, None);
    }

    /// An enabled source with no real cache file on disk resolves to
    /// `None`, matching `should_import_from_cache`'s own "enabled alone
    /// isn't enough" rule.
    #[tokio::test]
    async fn cache_fallback_paths_finds_nothing_in_an_empty_cache_directory() {
        let (_temp_dir, session) = crate::test_support::session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let (community, steam) = cache_fallback_paths(
            &state,
            cache_dir.path().to_path_buf(),
            rim_session::NetworkPolicy::default(),
        )
        .await;

        assert_eq!(community, None);
        assert_eq!(steam, None);
    }

    // -- `path_resolution_error_to_dto` --

    #[test]
    fn path_resolution_error_to_dto_carries_every_candidate() {
        let error = rim_io::PathResolutionError::GameDirNotFound {
            candidates: vec![PathBuf::from("C:/a"), PathBuf::from("C:/b")],
        };

        let dto = path_resolution_error_to_dto(&error);

        assert_eq!(
            dto,
            DefaultPathsErrorDto::GameDirNotFound {
                candidates: vec!["C:/a".to_string(), "C:/b".to_string()],
            }
        );
    }

    #[test]
    fn path_resolution_error_to_dto_carries_the_reason() {
        let error = rim_io::PathResolutionError::ModsConfigNotFound {
            reason: "%USERPROFILE% is not set".to_string(),
        };

        let dto = path_resolution_error_to_dto(&error);

        assert_eq!(
            dto,
            DefaultPathsErrorDto::ModsConfigNotFound {
                reason: "%USERPROFILE% is not set".to_string(),
            }
        );
    }
}
