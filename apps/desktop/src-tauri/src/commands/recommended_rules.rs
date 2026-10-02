//! `get_recommended_rules_step`/`get_recommended_rules`/
//! `skip_recommended_rules_step`: the Dashboard's "Get the recommended
//! rules" step. The step's state is derived by
//! `rim_session::recommended_rules_step`; this module only gathers its
//! facts and maps the result.
//!
//! `get_recommended_rules` runs in two phases so the network never runs
//! under the session lock: phase 1 (downloads) is a **bare**
//! `spawn_blocking`, phase 2 (import) runs inside [`with_session`]. A
//! process-wide `RunningGuard` keeps a double click or a second window
//! from starting a second run.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rim_session::ports::{
    NotificationStateStore as _, ProfileNotificationStateStore as _, RuleDatabaseFetcher,
};
use rim_session::use_cases::{
    FetchedRecommendedRules, GetRecommendedRules, ImportRimSort, RecommendedRulesContext,
    RefreshRuleDatabases, SkipRecommendedRules,
};
use rim_session::{
    FirstRun, NetworkPolicy, RecommendedRulesFacts, Settings, StepSkip, recommended_rules_step,
};
use tauri::Emitter;

use crate::commands::project::profile_base;
use crate::commands::rules_databases::{cache_dir, network_policy};
use crate::commands::{EVENT_RECOMMENDED_RULES_PROGRESS, emit_session_changed};
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::recommended_rules::{
    RecommendedRulesProgressEventDto, RecommendedRulesReportDto, RecommendedRulesStepDto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Where a progress tick goes: the real command emits an event, a test
/// collects them.
type ProgressSink = Arc<dyn Fn(RecommendedRulesProgressEventDto) + Send + Sync>;

/// Holds [`AppState::recommended_rules_running`] for as long as it lives.
/// The flag is cleared on drop, so a panic or an early return can never
/// leave it set.
#[derive(Debug)]
pub(crate) struct RunningGuard {
    flag: Arc<AtomicBool>,
}

impl RunningGuard {
    /// Takes the flag, or `None` when a run already holds it.
    pub(crate) fn acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self {
                flag: Arc::clone(flag),
            })
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

/// The facts both commands read from outside the session, resolved by the
/// `_inner` functions and threaded in by the tests.
#[derive(Debug, Clone)]
struct Environment {
    /// The app-global base holding `app-settings.json`.
    base: PathBuf,
    /// The rule-database cache directory.
    cache_dir: PathBuf,
    /// Whether the first-run notice was answered.
    first_run: FirstRun,
}

/// Resolves the real base, cache directory and Welcome state.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved.
fn environment(state: &AppState) -> Result<Environment, CommandError> {
    let base = profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so the app-global base \
             can't be resolved",
        )
    })?;
    let first_run = match state
        .adapters
        .notification_state_store
        .load(&base)
        .welcome_completed_at
    {
        Some(_) => FirstRun::Answered,
        None => FirstRun::Pending,
    };
    Ok(Environment {
        base,
        cache_dir: cache_dir()?,
        first_run,
    })
}

/// The step's current state for the loaded profile.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the base can't be resolved.
pub(crate) async fn get_recommended_rules_step_inner(
    state: &AppState,
) -> Result<RecommendedRulesStepDto, CommandError> {
    get_recommended_rules_step_with(
        state,
        environment(state)?,
        network_policy(),
        rim_io::GithubRuleDatabaseFetcher::new(),
    )
    .await
}

/// [`get_recommended_rules_step_inner`], with the environment, policy and
/// fetcher threaded in — the hermetic seam, like `get_rule_databases_with`.
/// The fetcher is only asked for `status` (a cache read); it never
/// downloads here.
async fn get_recommended_rules_step_with<Fetcher>(
    state: &AppState,
    environment: Environment,
    policy: NetworkPolicy,
    fetcher: Fetcher,
) -> Result<RecommendedRulesStepDto, CommandError>
where
    Fetcher: RuleDatabaseFetcher + Send + 'static,
{
    let is_running = Arc::clone(&state.recommended_rules_running);
    with_session(state, move |session| {
        if is_running.load(Ordering::SeqCst) {
            return Ok(RecommendedRulesStepDto::InProgress);
        }
        let profile_dir = session.paths().profile_dir.clone();
        let views = RefreshRuleDatabases::new(fetcher, rim_io::JsonImportManifestStore::new())
            .status(&policy, &environment.cache_dir, &profile_dir);
        let skip = StepSkip::from(
            rim_io::JsonProfileNotificationStateStore::new()
                .load(&profile_dir)
                .recommended_rules_skipped_at,
        );
        let settings = session.settings();
        Ok(recommended_rules_step(&RecommendedRulesFacts {
            policy: &policy,
            databases: &views,
            first_run: environment.first_run,
            skip,
            settings: &settings,
        })
        .into())
    })
    .await
}

/// See `get_recommended_rules_step_inner`.
///
/// # Errors
///
/// See `get_recommended_rules_step_inner`.
#[tauri::command]
pub async fn get_recommended_rules_step(
    state: tauri::State<'_, AppState>,
) -> Result<RecommendedRulesStepDto, CommandError> {
    get_recommended_rules_step_inner(&state).await
}

/// Turns on, downloads and imports the recommended rule databases,
/// reporting progress through `sink`.
///
/// # Errors
///
/// Returns [`CommandError::already_running`] when a run is in progress,
/// [`CommandError::no_project_loaded`] when no project is loaded,
/// `RecommendedRulesUnavailable` when a network gate is closed,
/// `AppSettingsDamaged` for a damaged `app-settings.json`, a profile-I/O
/// error when turning sources on fails to save, and
/// [`CommandError::internal`] when the download task panics.
async fn get_recommended_rules_inner(
    state: &AppState,
    sink: ProgressSink,
) -> Result<RunOutcome, CommandError> {
    get_recommended_rules_with(
        state,
        environment(state)?,
        rim_io::GithubRuleDatabaseFetcher::new(),
        sink,
    )
    .await
}

/// What a whole click returns: the report for the frontend, and whether
/// the session changed (decided in `rim-session`, so the command only
/// emits the event).
#[derive(Debug)]
struct RunOutcome {
    report: RecommendedRulesReportDto,
    changes_session: bool,
}

/// What phase 1 reads: the environment plus the loaded profile's facts.
struct PhaseOneInput {
    environment: Environment,
    profile_dir: PathBuf,
    settings: Settings,
}

type RecommendedRulesUseCase<Fetcher> =
    GetRecommendedRules<rim_io::JsonAppSettingsStore, Fetcher, rim_io::JsonImportManifestStore>;

/// [`get_recommended_rules_inner`], with the environment and fetcher
/// threaded in. Order: guard, project gate, phase 1 (bare
/// `spawn_blocking`, never under the session lock), phase 2 (inside
/// [`with_session`]). The guard travels into both blocking closures, so a
/// dropped future cannot clear the flag while a detached task still runs.
async fn get_recommended_rules_with<Fetcher>(
    state: &AppState,
    environment: Environment,
    fetcher: Fetcher,
    sink: ProgressSink,
) -> Result<RunOutcome, CommandError>
where
    Fetcher: RuleDatabaseFetcher + Send + 'static,
{
    let running = RunningGuard::acquire(&state.recommended_rules_running)
        .ok_or_else(CommandError::already_running)?;
    let (profile_dir, settings) = with_session(state, |session| {
        Ok((session.paths().profile_dir.clone(), session.settings()))
    })
    .await?;
    let input = PhaseOneInput {
        environment,
        profile_dir,
        settings,
    };
    let (use_case, fetched, running) =
        fetch_off_lock(running, fetcher, input, Arc::clone(&sink)).await?;

    let rule_store = state.adapters.rule_store;
    with_session(state, move |session| {
        let _running = running;
        let importer = rim_io::RimSortImporter::new(session.paths().profile_dir.clone());
        let import =
            ImportRimSort::new(importer, rule_store, rim_io::JsonImportManifestStore::new());
        let report = use_case.import(session, fetched, &import, &mut |progress| {
            sink(progress.into());
        });
        Ok(RunOutcome {
            report: RecommendedRulesReportDto::from(&report),
            changes_session: report.import.changes_session(),
        })
    })
    .await
}

/// Phase 1: a bare `spawn_blocking` (the network never runs under the
/// session lock). The guard moves into the task and comes back with the
/// results; if the task panics, the guard drops during the unwind.
async fn fetch_off_lock<Fetcher>(
    running: RunningGuard,
    fetcher: Fetcher,
    input: PhaseOneInput,
    sink: ProgressSink,
) -> Result<
    (
        RecommendedRulesUseCase<Fetcher>,
        FetchedRecommendedRules,
        RunningGuard,
    ),
    CommandError,
>
where
    Fetcher: RuleDatabaseFetcher + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let use_case = GetRecommendedRules::new(
            rim_io::JsonAppSettingsStore::new(),
            fetcher,
            rim_io::JsonImportManifestStore::new(),
        );
        let context = RecommendedRulesContext {
            base: &input.environment.base,
            cache_dir: &input.environment.cache_dir,
            profile_dir: &input.profile_dir,
            settings: &input.settings,
            first_run: input.environment.first_run,
        };
        let fetched = use_case.fetch(&context, &mut |progress| sink(progress.into()))?;
        Ok::<_, CommandError>((use_case, fetched, running))
    })
    .await
    .map_err(|join_error| {
        tracing::warn!(error = %join_error, "get_recommended_rules download task panicked");
        CommandError::internal("background task panicked")
    })?
}

/// See `get_recommended_rules_inner`. Emits
/// [`EVENT_RECOMMENDED_RULES_PROGRESS`] as it goes, and `session://changed`
/// when rules were imported.
///
/// # Errors
///
/// See `get_recommended_rules_inner`.
#[tauri::command]
pub async fn get_recommended_rules(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RecommendedRulesReportDto, CommandError> {
    let progress_app = app.clone();
    let sink: ProgressSink = Arc::new(move |progress| {
        let _ = progress_app.emit(EVENT_RECOMMENDED_RULES_PROGRESS, progress);
    });
    let outcome = get_recommended_rules_inner(&state, sink).await?;
    if outcome.changes_session {
        emit_session_changed(&app, SessionChangeReasonDto::Imported);
    }
    Ok(outcome.report)
}

/// Records that the loaded profile skipped the step (idempotent).
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or a profile-I/O error when `notifications.json` can't be written.
pub(crate) async fn skip_recommended_rules_step_inner(
    state: &AppState,
    now: jiff::Timestamp,
) -> Result<(), CommandError> {
    with_session(state, move |session| {
        let profile_dir = session.paths().profile_dir.clone();
        SkipRecommendedRules::new(rim_io::JsonProfileNotificationStateStore::new())
            .execute(&profile_dir, now)?;
        Ok(())
    })
    .await
}

/// See `skip_recommended_rules_step_inner`; also emits `session://changed`
/// (`NotificationsChanged`): the bell's notice list depends on the flag.
///
/// # Errors
///
/// See `skip_recommended_rules_step_inner`.
#[tauri::command]
pub async fn skip_recommended_rules_step(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), CommandError> {
    skip_recommended_rules_step_inner(&state, jiff::Timestamp::now()).await?;
    emit_session_changed(&app, SessionChangeReasonDto::NotificationsChanged);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use rim_session::ports::{
        CachedDatabase, DatabaseStatus, FetchFailure, RefreshOutcome, RuleDatabase,
    };
    use rim_session::test_support::FakeRuleDatabaseFetcher;

    use super::*;
    use crate::dto::recommended_rules::{
        DownloadingDto, ImportStepDto, SourceNeedDto, UnavailableReasonDto,
    };
    use crate::dto::rule_databases::{RefreshOutcomeDto, RuleDatabaseDto};
    use crate::error::{CommandErrorCode, CommandErrorDetail};
    use crate::test_support::session_fixture_with_temp_paths;

    const COMMUNITY_JSON: &str = r#"{"timestamp":1,"rules":{}}"#;
    const STEAM_JSON: &str = r#"{"version":1,"database":{}}"#;

    /// A loaded session plus temp base and cache directories.
    struct Harness {
        _session_dir: tempfile::TempDir,
        base: tempfile::TempDir,
        cache: tempfile::TempDir,
        state: AppState,
        profile_dir: PathBuf,
    }

    fn harness() -> Harness {
        let (session_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        Harness {
            _session_dir: session_dir,
            base: tempfile::tempdir().expect("tempdir"),
            cache: tempfile::tempdir().expect("tempdir"),
            state,
            profile_dir,
        }
    }

    impl Harness {
        fn environment(&self, first_run: FirstRun) -> Environment {
            Environment {
                base: self.base.path().to_path_buf(),
                cache_dir: self.cache.path().to_path_buf(),
                first_run,
            }
        }

        /// Writes the two importable cache files and returns the fake's
        /// status rows: both cached, the cache-read rimmerge-rules row not.
        fn cached_rows(&self) -> Vec<DatabaseStatus> {
            let community = self.cache.path().join("communityRules.json");
            let steam = self.cache.path().join("steamDB.json");
            std::fs::write(&community, COMMUNITY_JSON).expect("write cache");
            std::fs::write(&steam, STEAM_JSON).expect("write cache");
            vec![
                row(RuleDatabase::CommunityRules, Some(community)),
                row(RuleDatabase::SteamWorkshop, Some(steam)),
                row(RuleDatabase::RimmergeRules, None),
            ]
        }
    }

    fn row(database: RuleDatabase, cached_at: Option<PathBuf>) -> DatabaseStatus {
        DatabaseStatus {
            database,
            enabled: true,
            path: cached_at
                .clone()
                .unwrap_or_else(|| PathBuf::from("absent.json")),
            cached: cached_at.map(|_| CachedDatabase {
                sha256: "cached".to_string(),
                bytes: 1,
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            }),
            last_failure: None,
            last_attempt_at: None,
        }
    }

    fn uncached_rows() -> Vec<DatabaseStatus> {
        RuleDatabase::ALL
            .map(|database| row(database, None))
            .to_vec()
    }

    fn fetcher(rows: Vec<DatabaseStatus>) -> FakeRuleDatabaseFetcher {
        FakeRuleDatabaseFetcher::new(BTreeMap::new()).with_status(rows)
    }

    fn collecting_sink() -> (
        ProgressSink,
        Arc<Mutex<Vec<RecommendedRulesProgressEventDto>>>,
    ) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink: ProgressSink = Arc::new(move |event| {
            sink_events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(event);
        });
        (sink, events)
    }

    fn no_sink() -> ProgressSink {
        collecting_sink().0
    }

    /// A sink that records whether `flag` was set at each event.
    fn flag_recording_sink(flag: &Arc<AtomicBool>) -> (ProgressSink, Arc<Mutex<Vec<bool>>>) {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let sink_observed = Arc::clone(&observed);
        let flag = Arc::clone(flag);
        let sink: ProgressSink = Arc::new(move |_| {
            sink_observed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(flag.load(Ordering::SeqCst));
        });
        (sink, observed)
    }

    fn failed_download() -> RefreshOutcome {
        RefreshOutcome::Failed {
            failure: FetchFailure::unclassified("offline"),
        }
    }

    // -- the guard --

    #[test]
    fn the_guard_is_exclusive_and_clears_on_drop() {
        let flag = Arc::new(AtomicBool::new(false));

        let first = RunningGuard::acquire(&flag).expect("free flag");
        assert!(RunningGuard::acquire(&flag).is_none(), "held flag refuses");
        drop(first);

        assert!(RunningGuard::acquire(&flag).is_some(), "released on drop");
    }

    #[test]
    fn the_guard_is_released_when_its_holder_panics() {
        let flag = Arc::new(AtomicBool::new(false));

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = RunningGuard::acquire(&flag).expect("free flag");
            panic!("simulated failure while holding the guard");
        }));

        assert!(outcome.is_err());
        assert!(
            !flag.load(Ordering::SeqCst),
            "a panic must not leave it held"
        );
    }

    // -- `get_recommended_rules_step` --

    #[tokio::test]
    async fn the_step_fails_with_no_project_loaded() {
        let state = AppState::default();
        let base = tempfile::tempdir().expect("tempdir");
        let cache = tempfile::tempdir().expect("tempdir");

        let result = get_recommended_rules_step_with(
            &state,
            Environment {
                base: base.path().to_path_buf(),
                cache_dir: cache.path().to_path_buf(),
                first_run: FirstRun::Answered,
            },
            NetworkPolicy::default(),
            fetcher(uncached_rows()),
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
    }

    #[tokio::test]
    async fn the_step_reports_in_progress_while_a_run_holds_the_guard() {
        let harness = harness();
        let guard =
            RunningGuard::acquire(&harness.state.recommended_rules_running).expect("free flag");

        let during = get_recommended_rules_step_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            NetworkPolicy::default(),
            fetcher(uncached_rows()),
        )
        .await
        .expect("step");
        drop(guard);
        let after = get_recommended_rules_step_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            NetworkPolicy::default(),
            fetcher(uncached_rows()),
        )
        .await
        .expect("step");

        assert_eq!(during, RecommendedRulesStepDto::InProgress);
        assert!(matches!(after, RecommendedRulesStepDto::NeedsAction(_)));
    }

    #[tokio::test]
    async fn the_step_offers_an_import_for_cached_never_imported_sources() {
        let harness = harness();

        let step = get_recommended_rules_step_with(
            &harness.state,
            harness.environment(FirstRun::Pending),
            NetworkPolicy {
                allow_network: false,
                ..NetworkPolicy::default()
            },
            fetcher(harness.cached_rows()),
        )
        .await
        .expect("step");

        let RecommendedRulesStepDto::NeedsAction(needs) = step else {
            panic!("expected needsAction, got {step:?}");
        };
        let databases: Vec<_> = needs.sources.iter().map(|entry| entry.database).collect();
        assert_eq!(
            databases,
            vec![RuleDatabaseDto::Community, RuleDatabaseDto::Steam]
        );
        assert!(
            needs
                .sources
                .iter()
                .all(|entry| entry.need == SourceNeedDto::Import),
            "an import needs no network, so the gates do not matter"
        );
    }

    // -- `skip_recommended_rules_step` --

    #[tokio::test]
    async fn skipping_writes_the_profiles_notifications_file_and_the_step_reads_skipped() {
        let harness = harness();
        let now = jiff::Timestamp::now();

        skip_recommended_rules_step_inner(&harness.state, now)
            .await
            .expect("skip");

        assert!(
            harness.profile_dir.join("notifications.json").is_file(),
            "the skip is stored in the profile, not the app-global base"
        );
        let step = get_recommended_rules_step_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            NetworkPolicy::default(),
            fetcher(uncached_rows()),
        )
        .await
        .expect("step");
        let RecommendedRulesStepDto::Skipped(skipped) = step else {
            panic!("expected skipped, got {step:?}");
        };
        assert!(
            !skipped.sources.is_empty(),
            "skipped still lists its sources"
        );
    }

    #[tokio::test]
    async fn skipping_fails_with_no_project_loaded() {
        let result =
            skip_recommended_rules_step_inner(&AppState::default(), jiff::Timestamp::now()).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
    }

    // -- `get_recommended_rules` --

    #[tokio::test]
    async fn a_second_concurrent_run_is_refused_and_leaves_the_first_running() {
        let harness = harness();
        let _first =
            RunningGuard::acquire(&harness.state.recommended_rules_running).expect("free flag");

        let second = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            fetcher(uncached_rows()),
            no_sink(),
        )
        .await;

        assert_eq!(second.unwrap_err().code, CommandErrorCode::AlreadyRunning);
        assert!(
            harness
                .state
                .recommended_rules_running
                .load(Ordering::SeqCst),
            "the refused call must not release the first run's guard"
        );
    }

    #[tokio::test]
    async fn a_run_with_no_project_fails_and_releases_the_guard() {
        let state = AppState::default();
        let base = tempfile::tempdir().expect("tempdir");

        let result = get_recommended_rules_with(
            &state,
            Environment {
                base: base.path().to_path_buf(),
                cache_dir: base.path().join("cache"),
                first_run: FirstRun::Answered,
            },
            fetcher(uncached_rows()),
            no_sink(),
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
        assert!(!state.recommended_rules_running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn the_guard_is_released_after_the_download_task_panics() {
        let harness = harness();

        // No scripted outcome for the needed download: the fake panics
        // inside phase 1's blocking task.
        let result = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            fetcher(uncached_rows()),
            no_sink(),
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::Internal);
        assert!(
            !harness
                .state
                .recommended_rules_running
                .load(Ordering::SeqCst),
            "a panicked run must not leave the step stuck in progress"
        );
    }

    #[tokio::test]
    async fn a_closed_gate_is_refused_before_anything_is_downloaded() {
        let harness = harness();

        // The first-run notice is unanswered, so the download is refused
        // before the (unscripted, panicking) fake is ever asked to fetch.
        let error = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Pending),
            fetcher(uncached_rows()),
            no_sink(),
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, CommandErrorCode::RecommendedRulesUnavailable);
        assert_eq!(
            error.detail,
            Some(CommandErrorDetail::RecommendedRulesUnavailable {
                reason: UnavailableReasonDto::AwaitingFirstRun
            })
        );
        assert!(
            !harness
                .state
                .recommended_rules_running
                .load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn an_import_only_run_imports_both_sources_and_reports_counts() {
        let harness = harness();
        let (sink, events) = collecting_sink();

        let outcome = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Pending),
            fetcher(harness.cached_rows()),
            sink,
        )
        .await
        .expect("run");

        let report = outcome.report;
        assert!(report.downloads.is_empty(), "nothing needed the network");
        let ImportStepDto::Imported(imported) = &report.import else {
            panic!("expected imported, got {:?}", report.import);
        };
        assert_eq!(
            imported.databases,
            vec![RuleDatabaseDto::Community, RuleDatabaseDto::Steam]
        );
        assert_eq!(imported.report.community_rules, Some(0));
        assert_eq!(imported.report.steam_dependencies, Some(0));
        assert!(outcome.changes_session);
        assert_eq!(
            *events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![RecommendedRulesProgressEventDto::Importing(
                crate::dto::recommended_rules::ImportingDto {
                    databases: vec![RuleDatabaseDto::Community, RuleDatabaseDto::Steam]
                }
            )]
        );
        assert!(
            !harness
                .state
                .recommended_rules_running
                .load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn a_rerun_after_the_import_needs_nothing_and_changes_nothing() {
        let harness = harness();
        get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            fetcher(harness.cached_rows()),
            no_sink(),
        )
        .await
        .expect("first run");

        // The fake would panic if the second run asked it to download.
        let second = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            fetcher(harness.cached_rows()),
            no_sink(),
        )
        .await
        .expect("second run");

        assert_eq!(second.report.import, ImportStepDto::NotNeeded);
        assert!(second.report.downloads.is_empty());
        assert!(!second.changes_session, "NotNeeded emits no event");
    }

    #[tokio::test]
    async fn the_guard_is_held_while_the_import_reports_progress() {
        let harness = harness();
        let (sink, held) = flag_recording_sink(&harness.state.recommended_rules_running);

        get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            fetcher(harness.cached_rows()),
            sink,
        )
        .await
        .expect("run");

        let held = held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(!held.is_empty(), "the import reported progress");
        assert!(held.iter().all(|is_held| *is_held), "{held:?}");
    }

    #[tokio::test]
    async fn the_guard_is_held_while_the_downloads_report_progress() {
        let harness = harness();
        let (sink, held) = flag_recording_sink(&harness.state.recommended_rules_running);
        let download_fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
            (RuleDatabase::CommunityRules, failed_download()),
            (RuleDatabase::SteamWorkshop, failed_download()),
        ]))
        .with_status(uncached_rows());

        get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            download_fetcher,
            sink,
        )
        .await
        .expect("run");

        let held = held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(held.len(), 2, "one event per download");
        assert!(held.iter().all(|is_held| *is_held), "{held:?}");
    }

    #[tokio::test]
    async fn a_failed_download_reports_each_source_and_changes_nothing() {
        let harness = harness();
        let (sink, events) = collecting_sink();
        let download_fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
            (RuleDatabase::CommunityRules, failed_download()),
            (RuleDatabase::SteamWorkshop, failed_download()),
        ]))
        .with_status(uncached_rows());

        let outcome = get_recommended_rules_with(
            &harness.state,
            harness.environment(FirstRun::Answered),
            download_fetcher,
            sink,
        )
        .await
        .expect("run");

        assert_eq!(
            *events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![
                RecommendedRulesProgressEventDto::Downloading(DownloadingDto {
                    database: RuleDatabaseDto::Community
                }),
                RecommendedRulesProgressEventDto::Downloading(DownloadingDto {
                    database: RuleDatabaseDto::Steam
                }),
            ]
        );
        let downloads = &outcome.report.downloads;
        assert_eq!(downloads.len(), 2);
        assert!(
            downloads
                .iter()
                .all(|result| matches!(result.outcome, RefreshOutcomeDto::Failed { .. })),
            "{downloads:?}"
        );
        assert_eq!(outcome.report.import, ImportStepDto::NotNeeded);
        assert!(!outcome.changes_session);
    }
}
