//! `list_notifications`/`dismiss_notification`/`mute_notification_kind`/
//! `unmute_notification_kind`/`list_muted_notification_kinds`/
//! `complete_welcome`/`check_for_update`/`run_launch_network_checks`/
//! `reset_settings`.
//!
//! `check_for_update` and `run_launch_network_checks` never run inside
//! [`with_session`]'s `spawn_blocking` — neither needs a live `Session`
//! at all (both take `base`/`cache_dir`/policy directly), so each uses a
//! **bare** `spawn_blocking` instead: no network call may run inside
//! `with_session`, whose closure holds the session lock for its whole
//! duration — unlike the pre-existing `refresh_rule_databases` (manual,
//! user-initiated) command, which does run its fetch inside
//! `with_session`.

use std::path::{Path, PathBuf};

use rim_session::notifications::{AppVersion, GameMajorMinor, GameMajorMinorParseError};
use rim_session::ports::{NotificationStateStore as _, ProfileNotificationStateStore as _};
use rim_session::use_cases::{
    AcknowledgeGameVersion, CheckForUpdate, CompleteWelcome, DismissNotification,
    ListNotifications, MuteNotificationKind, ProfileNotificationFacts, RefreshRuleDatabases,
    ResetSettings, RunLaunchNetworkChecks, UnmuteNotificationKind, UpdateCheckRequest,
};
use rim_session::{NetworkPolicy, StepSkip};
use tauri::async_runtime::spawn_blocking;

use crate::commands::emit_session_changed;
use crate::commands::project::profile_base;
use crate::commands::rules_databases::{cache_dir, network_policy};
use crate::dto::notifications::{
    CheckForUpdateOutcomeDto, LaunchNetworkChecksOutcomeDto, NotificationDto, NotificationKeyDto,
    NotificationKindDto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// This build's own running version, for [`Notification::UpdateAvailable`]'s
/// comparison. `CARGO_PKG_VERSION` is expected to always be valid, short
/// semver — but it's still a string read at runtime from a build-time
/// env var, not a value this module constructs itself, so a malformed or
/// over-length one (`AppVersion::running` rejects anything over 32
/// characters) comes back as an ordinary [`CommandError::internal`]
/// rather than crashing the whole command via `unreachable!`/`panic!`.
///
/// [`Notification::UpdateAvailable`]: rim_session::notifications::Notification::UpdateAvailable
fn running_version() -> Result<AppVersion, CommandError> {
    AppVersion::running(env!("CARGO_PKG_VERSION")).map_err(|error| {
        CommandError::internal(format!(
            "this build's own CARGO_PKG_VERSION is invalid: {error}"
        ))
    })
}

fn resolve_base() -> Result<PathBuf, CommandError> {
    profile_base().ok_or_else(|| {
        CommandError::internal(
            "neither RIMMERGE_PROFILE_DIR nor LOCALAPPDATA is set, so there is nowhere to read \
             or write notifications.json",
        )
    })
}

/// Every active notice, evaluated fresh against the current app
/// settings, this profile's rule-database status, and the running
/// version. Requires a loaded project — the Setup page never shows a
/// notice; every notice is evaluated only after load, in the shell.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError::internal`] when the app-global base or the
/// rule-database cache directory can't be resolved.
pub(crate) async fn list_notifications_inner(
    state: &AppState,
) -> Result<Vec<NotificationDto>, CommandError> {
    let base = resolve_base()?;
    let cache_dir = cache_dir()?;
    let policy = network_policy();
    let running = running_version()?;
    list_notifications_with(state, base, cache_dir, policy, running).await
}

/// [`list_notifications_inner`], with the base directory, rule-database
/// cache directory, network policy, and running version all threaded in
/// explicitly rather than resolved from the real
/// `RIMMERGE_PROFILE_DIR`/`LOCALAPPDATA` environment — the seam this
/// module's own tests use to stay hermetic, the same convention
/// `commands::rules_databases`'s own `get_rule_databases_with` follows.
async fn list_notifications_with(
    state: &AppState,
    base: PathBuf,
    cache_dir: PathBuf,
    policy: NetworkPolicy,
    running: AppVersion,
) -> Result<Vec<NotificationDto>, CommandError> {
    let notification_state_store = state.adapters.notification_state_store;

    with_session(state, move |session| {
        let profile_dir = session.paths().profile_dir.clone();
        let refresh = RefreshRuleDatabases::new(
            rim_io::GithubRuleDatabaseFetcher::new(),
            rim_io::JsonImportManifestStore::new(),
        );
        let views = refresh.status(&policy, &cache_dir, &profile_dir);

        let current = GameMajorMinor::parse(&session.report().metadata.game_version);
        let profile_state = rim_io::JsonProfileNotificationStateStore::new().load(&profile_dir);
        let recommended_rules_skip = StepSkip::from(profile_state.recommended_rules_skipped_at);
        let acknowledged = profile_state
            .acknowledged_game_version
            .map(|raw| GameMajorMinor::parse(&raw));

        let list = ListNotifications::new(
            rim_io::JsonAppSettingsStore::new(),
            notification_state_store,
        );
        let notices = list.execute(
            &base,
            session.settings(),
            views,
            running.clone(),
            ProfileNotificationFacts {
                current,
                acknowledged,
                recommended_rules_skip,
            },
            jiff::Timestamp::now(),
        );
        Ok(notices.into_iter().map(Into::into).collect())
    })
    .await
}

/// Every active notice, evaluated fresh — see this module's own
/// `list_notifications_inner` for the full condition/threshold rules.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError::internal`] when the app-global base or the
/// rule-database cache directory can't be resolved.
#[tauri::command]
pub async fn list_notifications(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<NotificationDto>, CommandError> {
    list_notifications_inner(&state).await
}

/// Dismisses one notice occurrence.
///
/// [`NotificationKindDto::GameVersionChanged`] is special-cased: that
/// notice's own dismissal state lives in the per-profile store (its
/// `key.fingerprint` is the version being acknowledged), never the
/// app-global `<base>/notifications.json` every other kind uses — see
/// `rim_session::use_cases::AcknowledgeGameVersion`'s own doc comment.
/// That path requires a loaded project (for `profile_dir`); every other
/// kind doesn't.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails, or [`CommandError::no_project_loaded`] when
/// dismissing [`NotificationKindDto::GameVersionChanged`] with no
/// project loaded.
#[tauri::command]
pub async fn dismiss_notification(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    key: NotificationKeyDto,
) -> Result<(), CommandError> {
    let base = resolve_base()?;
    dismiss_notification_inner(&state, &base, key).await?;
    emit_session_changed(&app, SessionChangeReasonDto::NotificationsChanged);
    Ok(())
}

/// [`dismiss_notification`]'s routing: [`NotificationKindDto::GameVersionChanged`]
/// goes to [`dismiss_game_version_notification`] (the per-profile store),
/// every other kind to [`dismiss_notification_at`] (the app-global store)
/// — see [`dismiss_notification`]'s own doc comment for why this kind is
/// special-cased. Split out from the `#[tauri::command]` itself, with
/// `base` threaded in explicitly, so both routing arms are unit-testable
/// without a Tauri runtime.
async fn dismiss_notification_inner(
    state: &AppState,
    base: &Path,
    key: NotificationKeyDto,
) -> Result<(), CommandError> {
    if key.kind == NotificationKindDto::GameVersionChanged {
        dismiss_game_version_notification(state, key.fingerprint).await
    } else {
        dismiss_notification_at(base, key)
    }
}

/// [`dismiss_notification_inner`]'s [`NotificationKindDto::GameVersionChanged`]
/// half — see [`dismiss_notification`]'s own doc comment for why this kind
/// is special-cased.
async fn dismiss_game_version_notification(
    state: &AppState,
    fingerprint: String,
) -> Result<(), CommandError> {
    let version: GameMajorMinor =
        fingerprint
            .parse()
            .map_err(|error: GameMajorMinorParseError| {
                CommandError::invalid_input(error.to_string())
            })?;
    with_session(state, move |session| {
        let profile_dir = session.paths().profile_dir.clone();
        let use_case =
            AcknowledgeGameVersion::new(rim_io::JsonProfileNotificationStateStore::new());
        use_case.execute(&profile_dir, &version)?;
        Ok(())
    })
    .await
}

/// [`dismiss_notification_inner`]'s non-[`NotificationKindDto::GameVersionChanged`]
/// half, with `base` threaded in explicitly — the seam this module's own
/// tests use to stay hermetic, the same convention `commands::settings`'s
/// own `get_app_settings_inner` follows.
fn dismiss_notification_at(base: &Path, key: NotificationKeyDto) -> Result<(), CommandError> {
    let use_case = DismissNotification::new(
        rim_io::JsonAppSettingsStore::new(),
        rim_io::JsonNotificationStateStore::new(),
    );
    use_case.execute(base, &key.into(), jiff::Timestamp::now())?;
    Ok(())
}

/// Mutes a whole notice kind — "Don't remind me again".
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails.
#[tauri::command]
pub async fn mute_notification_kind(
    app: tauri::AppHandle,
    kind: NotificationKindDto,
) -> Result<(), CommandError> {
    let base = resolve_base()?;
    mute_notification_kind_at(&base, kind)?;
    emit_session_changed(&app, SessionChangeReasonDto::NotificationsChanged);
    Ok(())
}

/// See [`dismiss_notification_at`]'s own doc comment for why `base` is
/// threaded in explicitly.
fn mute_notification_kind_at(base: &Path, kind: NotificationKindDto) -> Result<(), CommandError> {
    let use_case = MuteNotificationKind::new(rim_io::JsonNotificationStateStore::new());
    use_case.execute(base, kind.into())?;
    Ok(())
}

/// Reverses [`mute_notification_kind`].
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails.
#[tauri::command]
pub async fn unmute_notification_kind(
    app: tauri::AppHandle,
    kind: NotificationKindDto,
) -> Result<(), CommandError> {
    let base = resolve_base()?;
    unmute_notification_kind_at(&base, kind)?;
    emit_session_changed(&app, SessionChangeReasonDto::NotificationsChanged);
    Ok(())
}

/// See [`dismiss_notification_at`]'s own doc comment for why `base` is
/// threaded in explicitly.
fn unmute_notification_kind_at(base: &Path, kind: NotificationKindDto) -> Result<(), CommandError> {
    let use_case = UnmuteNotificationKind::new(rim_io::JsonNotificationStateStore::new());
    use_case.execute(base, kind.into())?;
    Ok(())
}

/// The set of muted notice kinds — Settings → Network's own "muted
/// kinds" list, each with an Unmute button. Unlike
/// [`list_notifications`], this reads `<base>/notifications.json`
/// directly rather than through [`ListNotifications`]/`evaluate()`: a
/// muted kind's own notice is exactly what `evaluate()` filters *out*,
/// so there is no notice list to read it back from.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved.
#[tauri::command]
pub async fn list_muted_notification_kinds() -> Result<Vec<NotificationKindDto>, CommandError> {
    let base = resolve_base()?;
    Ok(list_muted_notification_kinds_at(&base))
}

/// See [`dismiss_notification_at`]'s own doc comment for why `base` is
/// threaded in explicitly.
fn list_muted_notification_kinds_at(base: &Path) -> Vec<NotificationKindDto> {
    rim_io::JsonNotificationStateStore::new()
        .load(base)
        .muted
        .into_iter()
        .map(Into::into)
        .collect()
}

/// Answers the first-run notice. When `app-settings.json` is missing the
/// answer also writes it, pinning the policy the user was shown.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved or
/// the write fails.
#[tauri::command]
pub async fn complete_welcome(app: tauri::AppHandle) -> Result<(), CommandError> {
    let base = resolve_base()?;
    complete_welcome_at(&base, jiff::Timestamp::now())?;
    emit_session_changed(&app, SessionChangeReasonDto::NotificationsChanged);
    Ok(())
}

/// See [`dismiss_notification_at`]'s own doc comment for why `base` is
/// threaded in explicitly.
fn complete_welcome_at(base: &Path, now: jiff::Timestamp) -> Result<(), CommandError> {
    let use_case = CompleteWelcome::new(
        rim_io::JsonAppSettingsStore::new(),
        rim_io::JsonNotificationStateStore::new(),
    );
    use_case.execute(base, now)?;
    Ok(())
}

/// Resets this profile's sorter/ledger settings to
/// [`rim_session::Settings::default`] — "Use recommended settings" on
/// the `Welcome` notice.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError`] when saving fails.
pub(crate) async fn reset_settings_inner(state: &AppState) -> Result<(), CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = ResetSettings::new(adapters.rule_store);
        use_case.execute(session)?;
        Ok(())
    })
    .await
}

/// Resets this profile's sorter/ledger settings to
/// [`rim_session::Settings::default`] and emits `session://changed` on
/// success — see this module's own `reset_settings_inner` for the
/// session-mutating half.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or [`CommandError`] when saving fails.
#[tauri::command]
pub async fn reset_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), CommandError> {
    let result = reset_settings_inner(&state).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::SettingsChanged);
    }
    result
}

/// "Check now" (Settings → Updates) — ignores the `check_for_updates`
/// toggle and the 24 h throttle (the click is its own consent), but
/// still honours `allow_network` and an active rate limit. Runs on a
/// bare background thread; touches no session state.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the base can't be resolved,
/// or when the background task itself panics.
#[tauri::command]
pub async fn check_for_update(
    state: tauri::State<'_, AppState>,
) -> Result<CheckForUpdateOutcomeDto, CommandError> {
    let base = resolve_base()?;
    let policy = network_policy();
    check_for_update_with(&state, base, policy).await
}

/// [`check_for_update`], with `base`/`policy` threaded in explicitly —
/// the seam this module's own tests use to stay hermetic, the same
/// convention [`list_notifications_with`] follows.
async fn check_for_update_with(
    state: &AppState,
    base: PathBuf,
    policy: NetworkPolicy,
) -> Result<CheckForUpdateOutcomeDto, CommandError> {
    let feed = state.adapters.release_feed.clone();

    spawn_blocking(move || {
        let use_case = CheckForUpdate::new(feed, rim_io::JsonNotificationStateStore::new());
        Ok(use_case
            .execute(
                UpdateCheckRequest::Manual,
                &policy,
                &base,
                jiff::Timestamp::now(),
            )
            .into())
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "check_for_update task panicked");
        Err(CommandError::internal("background task panicked"))
    })
}

/// The once-per-launch automatic check, called once by the shell right
/// after a project first loads. A second call in the same process is
/// safe (self-gates via `AppState::launch_checks_ran`) but does nothing.
/// Runs on a bare background thread; touches no session state — the
/// caller re-queries `list_notifications` afterward to see any result.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the app-global base or the
/// rule-database cache directory can't be resolved, or when the
/// background task itself panics.
#[tauri::command]
pub async fn run_launch_network_checks(
    state: tauri::State<'_, AppState>,
) -> Result<LaunchNetworkChecksOutcomeDto, CommandError> {
    let base = resolve_base()?;
    let cache_dir = cache_dir()?;
    let policy = network_policy();
    run_launch_network_checks_with(&state, base, cache_dir, policy).await
}

/// [`run_launch_network_checks`], with `base`/`cache_dir`/`policy`
/// threaded in explicitly — the same hermetic-test seam
/// [`check_for_update_with`] follows. **Not hermetic on its own for the
/// database-refresh half**: `rim_io::GithubRuleDatabaseFetcher` is still
/// built inline here, with no seam on [`AppState`] (see
/// `apps/desktop/CLAUDE.md`'s "no seam for the rule-database fetcher"
/// rule) — a test covering that half stays hermetic by keeping
/// `policy.allow_network`/`policy.auto_refresh_rule_databases` off
/// rather than by faking the port.
async fn run_launch_network_checks_with(
    state: &AppState,
    base: PathBuf,
    cache_dir: PathBuf,
    policy: NetworkPolicy,
) -> Result<LaunchNetworkChecksOutcomeDto, CommandError> {
    let feed = state.adapters.release_feed.clone();
    let notification_state_store = state.adapters.notification_state_store;
    let already_ran = state
        .launch_checks_ran
        .swap(true, std::sync::atomic::Ordering::SeqCst);

    spawn_blocking(move || {
        let use_case = RunLaunchNetworkChecks::new(
            CheckForUpdate::new(feed, notification_state_store),
            RefreshRuleDatabases::new(
                rim_io::GithubRuleDatabaseFetcher::new(),
                rim_io::JsonImportManifestStore::new(),
            ),
        );
        Ok(use_case
            .execute(
                already_ran,
                &policy,
                &cache_dir,
                &base,
                jiff::Timestamp::now(),
            )
            .into())
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "run_launch_network_checks task panicked");
        Err(CommandError::internal("background task panicked"))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rim_session::AppSettings;
    use rim_session::ports::{
        AppSettingsLoad, AppSettingsStore as _, FeedResponse, NotificationState, ReleaseFeed,
        ReleaseFeedError,
    };

    use super::*;
    use crate::dto::fetch_failure::{FetchFailureCauseDto, FetchFailureDto};
    use crate::dto::notifications::{
        NotificationDataDto, UpdateCheckRunOutcomeDto, UpdateCheckSkipReasonDto,
    };
    use crate::dto::rule_databases::{RefreshOutcomeDto, RuleDatabaseDto, SkipReasonDto};
    use crate::dto::settings::SettingsDto;
    use crate::state::Adapters;
    use crate::test_support::session_fixture_with_temp_paths;

    /// A [`ReleaseFeed`] fake, scripted with a single response and
    /// counting how many times it was actually called — every test below
    /// that must prove the real network was never touched asserts on
    /// [`Self::call_count`] rather than only the returned outcome, the
    /// same "the call count is the assertion that matters" rule
    /// `rim_session::use_cases::refresh_rule_databases`'s own fake
    /// fetcher follows.
    #[derive(Default)]
    struct FakeReleaseFeed {
        calls: Mutex<u32>,
        response: Mutex<Option<Result<FeedResponse, ReleaseFeedError>>>,
    }

    impl FakeReleaseFeed {
        /// Never scripted a response — panics with a clear message if
        /// `latest` is ever actually called.
        fn never_called() -> Self {
            Self {
                calls: Mutex::new(0),
                response: Mutex::new(None),
            }
        }

        fn returning(response: Result<FeedResponse, ReleaseFeedError>) -> Self {
            Self {
                calls: Mutex::new(0),
                response: Mutex::new(Some(response)),
            }
        }

        fn call_count(&self) -> u32 {
            *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }

    impl ReleaseFeed for FakeReleaseFeed {
        fn latest(&self, _etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError> {
            *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            self.response
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .expect("FakeReleaseFeed: latest() called more times than scripted")
        }
    }

    fn fresh_release(version: &str) -> Result<FeedResponse, ReleaseFeedError> {
        Ok(FeedResponse::Fresh {
            release: rim_session::notifications::LatestRelease {
                version: AppVersion::published(version).expect("valid version"),
                published_at: jiff::Timestamp::UNIX_EPOCH,
            },
            etag: None,
        })
    }

    fn running() -> AppVersion {
        AppVersion::running("0.1.0").expect("valid version")
    }

    fn state_with_feed(feed: Arc<FakeReleaseFeed>) -> AppState {
        AppState {
            adapters: Adapters {
                release_feed: feed,
                ..Adapters::default()
            },
            ..AppState::default()
        }
    }

    fn seed_welcome_answered(base: &Path) {
        rim_io::JsonNotificationStateStore::new()
            .save(
                base,
                &NotificationState {
                    welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
                    ..NotificationState::default()
                },
            )
            .expect("seed welcome answered");
    }

    // -- `run_launch_network_checks_with` --

    #[tokio::test]
    async fn run_launch_network_checks_does_nothing_before_welcome_is_answered() {
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let feed = Arc::new(FakeReleaseFeed::never_called());
        let state = state_with_feed(feed.clone());

        let outcome = run_launch_network_checks_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
        )
        .await
        .expect("must succeed");

        assert_eq!(
            outcome.update_check,
            CheckForUpdateOutcomeDto::Skipped {
                reason: UpdateCheckSkipReasonDto::AwaitingFirstRun
            }
        );
        assert!(outcome.database_refresh.is_empty());
        assert_eq!(feed.call_count(), 0);
    }

    #[tokio::test]
    async fn run_launch_network_checks_runs_once_then_is_a_no_op_on_a_second_call_this_launch() {
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");
        seed_welcome_answered(base.path());
        let feed = Arc::new(FakeReleaseFeed::returning(fresh_release("0.9.9")));
        let state = state_with_feed(feed.clone());
        // `auto_refresh_rule_databases` is off so the real
        // `GithubRuleDatabaseFetcher` this use case builds inline is
        // never actually called — see `run_launch_network_checks_with`'s
        // own doc comment.
        let policy = NetworkPolicy {
            auto_refresh_rule_databases: false,
            ..NetworkPolicy::default()
        };

        let first = run_launch_network_checks_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            policy,
        )
        .await
        .expect("first call must succeed");
        assert!(matches!(
            first.update_check,
            CheckForUpdateOutcomeDto::Ran { .. }
        ));
        assert_eq!(feed.call_count(), 1);

        let second = run_launch_network_checks_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            policy,
        )
        .await
        .expect("second call must succeed");

        assert_eq!(
            second.update_check,
            CheckForUpdateOutcomeDto::Skipped {
                reason: UpdateCheckSkipReasonDto::AlreadyRanThisLaunch
            }
        );
        assert!(second.database_refresh.is_empty());
        assert_eq!(
            feed.call_count(),
            1,
            "the second call must never touch the feed again"
        );
    }

    #[tokio::test]
    async fn run_launch_network_checks_does_nothing_when_network_is_disallowed() {
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");
        seed_welcome_answered(base.path());
        let feed = Arc::new(FakeReleaseFeed::never_called());
        let state = state_with_feed(feed.clone());
        let policy = NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        };

        let outcome = run_launch_network_checks_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            policy,
        )
        .await
        .expect("must succeed");

        assert_eq!(
            outcome.update_check,
            CheckForUpdateOutcomeDto::Skipped {
                reason: UpdateCheckSkipReasonDto::NetworkDisabled
            }
        );
        assert_eq!(
            outcome.database_refresh.len(),
            2,
            "community and rimmerge-rules — an empty list would make the loop below \
             pass vacuously without checking anything"
        );
        for (_, refresh_outcome) in &outcome.database_refresh {
            assert!(matches!(
                refresh_outcome,
                RefreshOutcomeDto::Skipped {
                    reason: SkipReasonDto::NetworkDisabled
                }
            ));
        }
        assert_eq!(feed.call_count(), 0);
    }

    // -- `check_for_update_with` --

    #[tokio::test]
    async fn check_for_update_honours_allow_network_false() {
        let base = tempfile::tempdir().expect("tempdir");
        let feed = Arc::new(FakeReleaseFeed::never_called());
        let state = state_with_feed(feed.clone());
        let policy = NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        };

        let outcome = check_for_update_with(&state, base.path().to_path_buf(), policy)
            .await
            .expect("must succeed");

        assert_eq!(
            outcome,
            CheckForUpdateOutcomeDto::Skipped {
                reason: UpdateCheckSkipReasonDto::NetworkDisabled
            }
        );
        assert_eq!(feed.call_count(), 0);
    }

    #[tokio::test]
    async fn check_for_update_records_a_successful_result() {
        let base = tempfile::tempdir().expect("tempdir");
        let feed = Arc::new(FakeReleaseFeed::returning(fresh_release("0.5.0")));
        let state = state_with_feed(feed);

        let outcome =
            check_for_update_with(&state, base.path().to_path_buf(), NetworkPolicy::default())
                .await
                .expect("must succeed");

        assert_eq!(
            outcome,
            CheckForUpdateOutcomeDto::Ran {
                outcome: UpdateCheckRunOutcomeDto::Updated {
                    latest_version: "0.5.0".to_string()
                }
            }
        );
        let recorded = rim_io::JsonNotificationStateStore::new()
            .load(base.path())
            .update_check
            .last_success
            .expect("a successful check must be recorded");
        assert_eq!(recorded.latest.version.to_string(), "0.5.0");
    }

    #[tokio::test]
    async fn check_for_update_surfaces_a_feed_failure_as_an_outcome_not_a_panic() {
        let base = tempfile::tempdir().expect("tempdir");
        let feed = Arc::new(FakeReleaseFeed::returning(Err(
            ReleaseFeedError::Transport("connection reset".to_string()),
        )));
        let state = state_with_feed(feed);

        let outcome =
            check_for_update_with(&state, base.path().to_path_buf(), NetworkPolicy::default())
                .await
                .expect("a transport failure must come back as an outcome, never a command error");

        assert_eq!(
            outcome,
            CheckForUpdateOutcomeDto::Ran {
                outcome: UpdateCheckRunOutcomeDto::Failed {
                    failure: FetchFailureDto {
                        cause: FetchFailureCauseDto::Transport,
                        detail: "connection reset".to_string()
                    }
                }
            }
        );
    }

    // -- notification state round trips against a temp base dir --

    #[test]
    fn mute_then_unmute_round_trips_against_the_real_store() {
        let base = tempfile::tempdir().expect("tempdir");

        mute_notification_kind_at(base.path(), NotificationKindDto::RuleDatabasesStale)
            .expect("mute must succeed");
        assert_eq!(
            list_muted_notification_kinds_at(base.path()),
            vec![NotificationKindDto::RuleDatabasesStale]
        );

        unmute_notification_kind_at(base.path(), NotificationKindDto::RuleDatabasesStale)
            .expect("unmute must succeed");
        assert!(list_muted_notification_kinds_at(base.path()).is_empty());
    }

    #[test]
    fn dismiss_notification_at_records_the_fingerprint_in_the_real_store() {
        let base = tempfile::tempdir().expect("tempdir");
        let key = NotificationKeyDto {
            kind: NotificationKindDto::UpdateAvailable,
            fingerprint: "0.9.0".to_string(),
        };

        dismiss_notification_at(base.path(), key.clone()).expect("dismiss must succeed");

        let state = rim_io::JsonNotificationStateStore::new().load(base.path());
        assert!(state.is_dismissed(&key.into()));
    }

    #[test]
    fn dismiss_notification_at_closing_welcome_also_answers_it() {
        let base = tempfile::tempdir().expect("tempdir");
        let key = NotificationKeyDto {
            kind: NotificationKindDto::Welcome,
            fingerprint: "welcome".to_string(),
        };

        dismiss_notification_at(base.path(), key).expect("dismiss must succeed");

        assert!(
            rim_io::JsonNotificationStateStore::new()
                .load(base.path())
                .welcome_completed_at
                .is_some(),
            "closing Welcome from the bell's x must also answer it, or automatic \
             network checks stay gated forever"
        );
    }

    #[tokio::test]
    async fn dismiss_notification_inner_routes_game_version_changed_to_the_per_profile_store() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let base = tempfile::tempdir().expect("tempdir");
        let key = NotificationKeyDto {
            kind: NotificationKindDto::GameVersionChanged,
            fingerprint: "1.7".to_string(),
        };

        dismiss_notification_inner(&state, base.path(), key)
            .await
            .expect("dismiss must succeed");

        let acknowledged = rim_io::JsonProfileNotificationStateStore::new()
            .load(&profile_dir)
            .acknowledged_game_version;
        assert_eq!(acknowledged, Some("1.7".to_string()));
        assert!(
            rim_io::JsonNotificationStateStore::new()
                .load(base.path())
                .dismissed
                .is_empty(),
            "GameVersionChanged must never touch the app-global dismissed list"
        );
    }

    #[tokio::test]
    async fn dismiss_notification_inner_routes_every_other_kind_to_the_base_store() {
        let state = AppState::default();
        let base = tempfile::tempdir().expect("tempdir");
        let key = NotificationKeyDto {
            kind: NotificationKindDto::RuleDatabasesStale,
            fingerprint: "community_rules@never".to_string(),
        };

        dismiss_notification_inner(&state, base.path(), key.clone())
            .await
            .expect("dismiss must succeed, with no project loaded at all");

        let saved = rim_io::JsonNotificationStateStore::new().load(base.path());
        assert!(saved.is_dismissed(&key.into()));
    }

    #[test]
    fn complete_welcome_at_records_the_timestamp() {
        let base = tempfile::tempdir().expect("tempdir");
        let now = jiff::Timestamp::now();

        complete_welcome_at(base.path(), now).expect("must succeed");

        assert_eq!(
            rim_io::JsonNotificationStateStore::new()
                .load(base.path())
                .welcome_completed_at,
            Some(now)
        );
    }

    #[test]
    fn complete_welcome_at_writes_app_settings_when_missing() {
        let base = tempfile::tempdir().expect("tempdir");

        complete_welcome_at(base.path(), jiff::Timestamp::now()).expect("must succeed");

        assert_eq!(
            rim_io::JsonAppSettingsStore::new().load(base.path()),
            AppSettingsLoad::Loaded(AppSettings::default()),
            "answering Welcome must pin the policy that was shown"
        );
    }

    #[test]
    fn dismiss_notification_at_closing_welcome_writes_app_settings_when_missing() {
        let base = tempfile::tempdir().expect("tempdir");
        let key = NotificationKeyDto {
            kind: NotificationKindDto::Welcome,
            fingerprint: "welcome".to_string(),
        };

        dismiss_notification_at(base.path(), key).expect("dismiss must succeed");

        assert_eq!(
            rim_io::JsonAppSettingsStore::new().load(base.path()),
            AppSettingsLoad::Loaded(AppSettings::default())
        );
    }

    #[test]
    fn answering_welcome_keeps_a_saved_policy_and_a_corrupt_file() {
        let saved_base = tempfile::tempdir().expect("tempdir");
        let mut chosen = AppSettings::default();
        chosen.network.fetch_steam_workshop = false;
        rim_io::JsonAppSettingsStore::new()
            .save(saved_base.path(), &chosen)
            .expect("seed settings");
        complete_welcome_at(saved_base.path(), jiff::Timestamp::now()).expect("must succeed");
        assert_eq!(
            rim_io::JsonAppSettingsStore::new().load(saved_base.path()),
            AppSettingsLoad::Loaded(chosen)
        );

        let corrupt_base = tempfile::tempdir().expect("tempdir");
        let corrupt_path = corrupt_base.path().join("app-settings.json");
        std::fs::write(&corrupt_path, b"not json").expect("seed corrupt file");
        complete_welcome_at(corrupt_base.path(), jiff::Timestamp::now()).expect("must succeed");
        assert_eq!(
            std::fs::read(&corrupt_path).expect("read back"),
            b"not json",
            "a corrupt file is never overwritten implicitly"
        );
    }

    #[tokio::test]
    async fn dismiss_notification_for_game_version_changed_acknowledges_it_per_profile() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        dismiss_game_version_notification(&state, "1.7".to_string())
            .await
            .expect("dismiss must succeed");

        let acknowledged = rim_io::JsonProfileNotificationStateStore::new()
            .load(&profile_dir)
            .acknowledged_game_version;
        assert_eq!(acknowledged, Some("1.7".to_string()));
    }

    #[tokio::test]
    async fn dismissing_game_version_changed_with_a_garbage_fingerprint_is_rejected_and_writes_nothing()
     {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let result = dismiss_game_version_notification(&state, "not a version".to_string()).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::InvalidInput
        );
        assert_eq!(
            rim_io::JsonProfileNotificationStateStore::new()
                .load(&profile_dir)
                .acknowledged_game_version,
            None,
            "a rejected fingerprint must never be persisted as an acknowledgement"
        );
    }

    #[tokio::test]
    async fn list_notifications_with_reports_the_acknowledged_and_current_game_versions() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");
        // The session fixture's report says game version "1.6"; the
        // profile last acknowledged 1.5.
        let store = rim_io::JsonProfileNotificationStateStore::new();
        let mut profile_state = store.load(&profile_dir);
        profile_state.acknowledged_game_version = Some("1.5".to_string());
        store
            .save(&profile_dir, &profile_state)
            .expect("seed the acknowledgement");

        let notices = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await
        .expect("list must succeed");

        let changed = notices
            .iter()
            .find_map(|notice| match &notice.data {
                NotificationDataDto::GameVersionChanged(data) => Some(data),
                _ => None,
            })
            .expect("a differing acknowledged version must produce the notice");
        assert_eq!(changed.acknowledged, "1.5");
        assert_eq!(changed.current, "1.6");
    }

    #[tokio::test]
    async fn list_notifications_with_passes_the_profiles_recorded_skip_to_the_evaluator() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let profile_dir = session.paths().profile_dir.clone();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");
        complete_welcome_at(base.path(), jiff::Timestamp::now()).expect("answer Welcome");
        let steam_listed = |notices: &[NotificationDto]| {
            notices.iter().any(|notice| match &notice.data {
                NotificationDataDto::RecommendedSourcesIncomplete(data) => {
                    data.sources.contains_key(&RuleDatabaseDto::Steam)
                }
                _ => false,
            })
        };

        let offered = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await
        .expect("list must succeed");
        assert!(
            !steam_listed(&offered),
            "while the step offers Steam, the bell leaves it to the strip"
        );

        let store = rim_io::JsonProfileNotificationStateStore::new();
        let mut profile_state = store.load(&profile_dir);
        profile_state.recommended_rules_skipped_at = Some(jiff::Timestamp::UNIX_EPOCH);
        store
            .save(&profile_dir, &profile_state)
            .expect("seed the skip");
        let skipped = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await
        .expect("list must succeed");
        assert!(
            steam_listed(&skipped),
            "a skipped step hands Steam back to the bell notice"
        );
    }

    #[tokio::test]
    async fn dismiss_notification_for_game_version_changed_fails_with_no_project_loaded() {
        let state = AppState::default();

        let result = dismiss_game_version_notification(&state, "1.7".to_string()).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    #[tokio::test]
    async fn list_notifications_with_no_longer_shows_welcome_once_it_is_completed() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let before = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await
        .expect("list must succeed");
        assert!(
            before
                .iter()
                .any(|n| matches!(n.data, NotificationDataDto::Welcome(_))),
            "a brand-new machine shows the Welcome notice"
        );

        complete_welcome_at(base.path(), jiff::Timestamp::now()).expect("must succeed");

        let after = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await
        .expect("list must succeed");
        assert!(
            !after
                .iter()
                .any(|n| matches!(n.data, NotificationDataDto::Welcome(_))),
            "answering Welcome must remove it from the next list"
        );
    }

    #[tokio::test]
    async fn list_notifications_fails_with_no_project_loaded() {
        let state = AppState::default();
        let base = tempfile::tempdir().expect("tempdir");
        let cache_dir = tempfile::tempdir().expect("tempdir");

        let result = list_notifications_with(
            &state,
            base.path().to_path_buf(),
            cache_dir.path().to_path_buf(),
            NetworkPolicy::default(),
            running(),
        )
        .await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    // -- `reset_settings_inner` --

    #[tokio::test]
    async fn reset_settings_restores_the_default_and_persists_it() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let default_dto: SettingsDto = rim_session::Settings::default().into();
        let non_default = SettingsDto {
            threshold: 50,
            ..default_dto
        };
        crate::commands::settings::set_settings_inner(&state, non_default)
            .await
            .expect("seed a non-default settings value");

        reset_settings_inner(&state)
            .await
            .expect("reset must succeed");

        let restored = crate::commands::settings::get_settings_inner(&state)
            .await
            .expect("get settings must succeed");
        assert_eq!(restored, default_dto);
    }

    #[tokio::test]
    async fn reset_settings_fails_with_no_project_loaded() {
        let state = AppState::default();

        let result = reset_settings_inner(&state).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }
}
