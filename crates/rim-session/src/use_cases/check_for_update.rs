//! [`CheckForUpdate`]: queries [`ReleaseFeed`] for a newer stable
//! Rimmerge release, recording the result in
//! `<base>/notifications.json`'s own `update_check` state. The cadence
//! (one automatic check per launch, no daily floor), rate-limit handling,
//! and ETag reuse are documented on [`CheckForUpdate::execute`].

use std::path::Path;

use crate::app_settings::NetworkPolicy;
use crate::notifications::LatestRelease;
use crate::ports::{
    FeedResponse, FetchFailure, NotificationStateStore, ReleaseFeed, ReleaseFeedError,
    UpdateCheckFailure, UpdateCheckSuccess,
};

/// How far past `now` a stored `retry_not_before` can honestly lie: the
/// rate-limit parser clamps `until` to 24 h ahead of the response. A stamp
/// further out was written while the clock ran ahead, and honouring it would
/// block every check, manual ones included, until the clock caught up.
const MAX_RETRY_HORIZON_SECONDS: i64 = 24 * 60 * 60;

/// Which caller is asking, and with what already known. The two request
/// shapes gate differently — see [`CheckForUpdate::execute`]'s own doc
/// comment for the exact rule each field feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCheckRequest {
    /// The launch-time check (`crate::use_cases::RunLaunchNetworkChecks`).
    Automatic {
        /// Whether this call has already run once in this process — the
        /// once-per-launch flag itself is an `AtomicBool` on the
        /// interface's own app state, not something this crate tracks;
        /// the caller passes its current value in.
        already_ran_this_launch: bool,
    },
    /// "Check now" (Settings → Updates). Ignores `check_for_updates`
    /// (the click is its own consent).
    Manual,
}

/// Why [`CheckForUpdate::execute`] made zero transport calls. Ordered by
/// which check runs first (the variants' declaration order); the first one
/// that applies is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCheckSkipReason {
    /// `Automatic` only: the first-run (`Welcome`) notice hasn't been
    /// answered yet.
    AwaitingFirstRun,
    /// `Automatic` only: this call has already run once this launch.
    AlreadyRanThisLaunch,
    /// `NetworkPolicy::allow_network` is off. Applies to both request
    /// kinds.
    NetworkDisabled,
    /// `Automatic` only: `NetworkPolicy::check_for_updates` is off.
    CheckDisabled,
    /// Both request kinds: GitHub's rate limit is still in effect.
    RateLimitedUntil(jiff::Timestamp),
}

/// What one actual transport call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckRunOutcome {
    /// A new (or first-ever) release was fetched and recorded.
    Updated {
        /// The newly-recorded release.
        latest: LatestRelease,
    },
    /// The server said `304 Not Modified` — whatever was already
    /// recorded is still current.
    Unchanged,
    /// The attempt failed. Never a silent condition — the Settings page
    /// shows this failure as persistent text (never a notice, never a
    /// toast).
    Failed {
        /// Why the attempt failed: a closed cause plus a short, bounded
        /// English detail, safe to show verbatim as a technical line.
        failure: FetchFailure,
    },
}

/// [`CheckForUpdate::execute`]'s full outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckForUpdateOutcome {
    /// No transport call was made.
    Skipped(UpdateCheckSkipReason),
    /// A transport call was made; here's what it did.
    Ran(UpdateCheckRunOutcome),
}

/// Checks GitHub for a newer stable release, gated by network policy and
/// (for the automatic request) once per launch, then records the result.
pub struct CheckForUpdate<Feed, State> {
    feed: Feed,
    state_store: State,
}

impl<Feed: ReleaseFeed, State: NotificationStateStore> CheckForUpdate<Feed, State> {
    /// Builds the use case from its two ports.
    #[must_use]
    pub fn new(feed: Feed, state_store: State) -> Self {
        Self { feed, state_store }
    }

    /// Runs one check, or explains why it skipped one. `welcome_completed_at`
    /// and the rate-limit back-off are both read off the same
    /// `<base>/notifications.json` this call also writes back to — see
    /// [`UpdateCheckSkipReason`] for the full gate order. A **project
    /// having loaded** is not checked here at all — that precondition
    /// belongs to the caller (`RunLaunchNetworkChecks` is only ever
    /// invoked once one has), since this use case has no `Session` in
    /// scope to check it against.
    ///
    /// **No `running` version parameter**: this use case never needs
    /// one — "is the recorded
    /// release actually newer than what's running" is
    /// `crate::notifications::evaluate::evaluate`'s own job (it derives
    /// the notice from `last_success.latest > running` at read time,
    /// never stored), and recording *every* successful check's release
    /// here, newer or not, is exactly what lets that derivation stay
    /// correct without this use case needing to duplicate the
    /// comparison.
    pub fn execute(
        &self,
        request: UpdateCheckRequest,
        policy: &NetworkPolicy,
        base: &Path,
        now: jiff::Timestamp,
    ) -> CheckForUpdateOutcome {
        let mut state = self.state_store.load(base);

        if let UpdateCheckRequest::Automatic {
            already_ran_this_launch,
        } = request
        {
            if state.welcome_completed_at.is_none() {
                return CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AwaitingFirstRun);
            }
            if already_ran_this_launch {
                return CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AlreadyRanThisLaunch);
            }
        }

        if !policy.allow_network {
            return CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::NetworkDisabled);
        }

        let is_automatic = matches!(request, UpdateCheckRequest::Automatic { .. });

        if is_automatic && !policy.check_for_updates {
            return CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::CheckDisabled);
        }

        // No daily floor: an automatic check runs on every launch, because a
        // floor hid a release from anyone who reopened the app within a day
        // of the previous check. `AlreadyRanThisLaunch` above keeps it to one
        // per process, the ETag below makes most launches a 304, and the
        // rate-limit back-off below still holds.
        if let Some(retry_not_before) = state.update_check.retry_not_before
            && now < retry_not_before
            && retry_not_before.as_second() - now.as_second() <= MAX_RETRY_HORIZON_SECONDS
        {
            return CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::RateLimitedUntil(
                retry_not_before,
            ));
        }

        state.update_check.last_attempt_at = Some(now);
        // Cleared unconditionally before the attempt: reaching this point
        // means either it was never set, or `now` is already past it (the
        // gate above already refused otherwise) — a stale past instant
        // must never linger once its own window has passed. The
        // `RateLimited` arm below sets a fresh one only if this attempt
        // itself gets rate-limited again.
        state.update_check.retry_not_before = None;
        let etag = state.update_check.etag.clone();
        let outcome = match self.feed.latest(etag.as_deref()) {
            Ok(FeedResponse::NotModified) => {
                if let Some(success) = &mut state.update_check.last_success {
                    success.checked_at = now;
                }
                state.update_check.last_failure = None;
                UpdateCheckRunOutcome::Unchanged
            }
            Ok(FeedResponse::Fresh { release, etag }) => {
                state.update_check.etag = etag;
                state.update_check.last_success = Some(UpdateCheckSuccess {
                    checked_at: now,
                    latest: release.clone(),
                });
                state.update_check.last_failure = None;
                UpdateCheckRunOutcome::Updated { latest: release }
            }
            Err(error) => {
                if let ReleaseFeedError::RateLimited { until } = error {
                    state.update_check.retry_not_before = Some(until);
                }
                let failure = FetchFailure::from(&error);
                state.update_check.last_failure = Some(UpdateCheckFailure {
                    checked_at: now,
                    reason: failure.detail.clone(),
                });
                UpdateCheckRunOutcome::Failed { failure }
            }
        };

        // Reload fresh rather than saving the `state` loaded at the top of
        // this call: the network call above can take up to 15s, during
        // which a dismissal, a mute, or a Welcome answer — made by this
        // same process or, for the app-global `notifications.json`, a
        // concurrent CLI invocation — could have been persisted. Saving
        // the stale `state` back would silently clobber it. Only
        // `update_check` is this call's own field to write.
        let mut fresh = self.state_store.load(base);
        fresh.update_check = state.update_check;
        // Best effort, mirroring `rim_io::databases::cache::fail`'s own
        // rule for its manifest: the check itself already ran and its
        // result is this call's own return value regardless, so a
        // failed write here costs at most a stale on-disk record, never
        // a lost or duplicated check.
        let _ = self.state_store.save(base, &fresh);

        CheckForUpdateOutcome::Ran(outcome)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;
    use crate::app_settings::NetworkPolicy;
    use crate::notifications::{AppVersion, NotificationKind};
    use crate::ports::{NotificationState, StoreError};

    #[derive(Default)]
    struct FakeReleaseFeed {
        response: RefCell<Option<Result<FeedResponse, ReleaseFeedError>>>,
        calls: RefCell<Vec<Option<String>>>,
    }

    impl FakeReleaseFeed {
        fn returning(response: Result<FeedResponse, ReleaseFeedError>) -> Self {
            Self {
                response: RefCell::new(Some(response)),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ReleaseFeed for FakeReleaseFeed {
        fn latest(&self, etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError> {
            self.calls.borrow_mut().push(etag.map(str::to_string));
            self.response
                .borrow_mut()
                .take()
                .expect("FakeReleaseFeed: latest() called more times than scripted")
        }
    }

    #[derive(Default)]
    struct InMemoryNotificationStateStore {
        saved: RefCell<Option<NotificationState>>,
    }

    impl NotificationStateStore for InMemoryNotificationStateStore {
        fn load(&self, _base: &Path) -> NotificationState {
            self.saved.borrow().clone().unwrap_or_default()
        }

        fn save(&self, _base: &Path, state: &NotificationState) -> Result<(), StoreError> {
            *self.saved.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    /// Scripts a different `load()` result on the *second* call than the
    /// first — simulating a write made by another process (or another
    /// use case in this one) while `execute`'s own network call was in
    /// flight, between its initial load and its final save.
    #[derive(Default)]
    struct InterleavedNotificationStateStore {
        loads: RefCell<std::collections::VecDeque<NotificationState>>,
        saved: RefCell<Option<NotificationState>>,
    }

    impl NotificationStateStore for InterleavedNotificationStateStore {
        fn load(&self, _base: &Path) -> NotificationState {
            let mut loads = self.loads.borrow_mut();
            if loads.len() > 1 {
                loads.pop_front().expect("non-empty")
            } else {
                loads.front().cloned().unwrap_or_default()
            }
        }

        fn save(&self, _base: &Path, state: &NotificationState) -> Result<(), StoreError> {
            *self.saved.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    fn answered_state() -> NotificationState {
        NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            ..NotificationState::default()
        }
    }

    fn store_with(state: NotificationState) -> InMemoryNotificationStateStore {
        let store = InMemoryNotificationStateStore::default();
        *store.saved.borrow_mut() = Some(state);
        store
    }

    fn fresh(version: &str) -> Result<FeedResponse, ReleaseFeedError> {
        Ok(FeedResponse::Fresh {
            release: LatestRelease {
                version: AppVersion::published(version).expect("valid version"),
                published_at: jiff::Timestamp::UNIX_EPOCH,
            },
            etag: Some("W/\"abc\"".to_string()),
        })
    }

    #[test]
    fn automatic_skips_when_the_first_run_notice_is_unanswered() {
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            InMemoryNotificationStateStore::default(),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AwaitingFirstRun)
        );
        assert!(use_case.feed.calls.borrow().is_empty());
    }

    #[test]
    fn automatic_skips_once_it_has_already_run_this_launch() {
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(answered_state()),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: true,
            },
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AlreadyRanThisLaunch)
        );
    }

    #[test]
    fn both_request_kinds_skip_when_network_is_off() {
        for request in [
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            UpdateCheckRequest::Manual,
        ] {
            let use_case = CheckForUpdate::new(
                FakeReleaseFeed::returning(fresh("0.2.0")),
                store_with(answered_state()),
            );
            let policy = NetworkPolicy {
                allow_network: false,
                ..NetworkPolicy::default()
            };

            let outcome = use_case.execute(
                request,
                &policy,
                &PathBuf::from("base"),
                jiff::Timestamp::now(),
            );

            assert_eq!(
                outcome,
                CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::NetworkDisabled)
            );
            assert!(use_case.feed.calls.borrow().is_empty());
        }
    }

    #[test]
    fn manual_ignores_check_for_updates_being_off() {
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(answered_state()),
        );
        let policy = NetworkPolicy {
            check_for_updates: false,
            ..NetworkPolicy::default()
        };

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &policy,
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert!(matches!(outcome, CheckForUpdateOutcome::Ran(_)));
    }

    #[test]
    fn automatic_skips_when_check_for_updates_is_off() {
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(answered_state()),
        );
        let policy = NetworkPolicy {
            check_for_updates: false,
            ..NetworkPolicy::default()
        };

        let outcome = use_case.execute(
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            &policy,
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::CheckDisabled)
        );
    }

    fn state_attempted_an_hour_ago(now: jiff::Timestamp) -> NotificationState {
        let an_hour_ago = now - jiff::Span::new().hours(1);
        NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            update_check: crate::ports::UpdateCheckState {
                last_attempt_at: Some(an_hour_ago),
                last_success: Some(UpdateCheckSuccess {
                    checked_at: an_hour_ago,
                    latest: LatestRelease {
                        version: AppVersion::published("1.0.0").expect("valid version"),
                        published_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        }
    }

    #[test]
    fn automatic_runs_on_a_new_launch_even_when_attempted_under_24h_ago() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);

        for request in [
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            UpdateCheckRequest::Manual,
        ] {
            let use_case = CheckForUpdate::new(
                FakeReleaseFeed::returning(fresh("0.2.0")),
                store_with(state_attempted_an_hour_ago(now)),
            );
            let outcome = use_case.execute(
                request,
                &NetworkPolicy::default(),
                &PathBuf::from("base"),
                now,
            );
            assert!(
                matches!(outcome, CheckForUpdateOutcome::Ran(_)),
                "{request:?}: {outcome:?}"
            );
        }
    }

    #[test]
    fn a_launch_an_hour_after_the_last_check_still_surfaces_a_new_release() {
        use crate::notifications::evaluate::evaluate;
        use crate::notifications::{GameMajorMinor, NotificationInputs};
        use crate::recommended_rules::StepSkip;

        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);
        let base = PathBuf::from("base");
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("1.1.0")),
            store_with(state_attempted_an_hour_ago(now)),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            &NetworkPolicy::default(),
            &base,
            now,
        );

        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Updated { .. })
        ));
        let inputs = NotificationInputs {
            app: crate::app_settings::AppSettings::default(),
            profile: crate::settings::Settings::default(),
            databases: Vec::new(),
            running: AppVersion::running("1.0.0").expect("valid version"),
            current_game_version: GameMajorMinor::parse("1.6"),
            acknowledged_game_version: Some(GameMajorMinor::parse("1.6")),
            recommended_rules_skip: StepSkip::NotSkipped,
        };
        let notices = evaluate(&inputs, &use_case.state_store.load(&base), now);
        assert!(
            notices
                .iter()
                .any(|notice| notice.kind() == NotificationKind::UpdateAvailable),
            "{notices:?}"
        );
    }

    #[test]
    fn both_request_kinds_respect_a_still_active_rate_limit() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let state = NotificationState {
            welcome_completed_at: Some(now),
            update_check: crate::ports::UpdateCheckState {
                retry_not_before: Some(now + jiff::Span::new().hours(1)),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };

        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(state),
        );
        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            now,
        );
        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::RateLimitedUntil(_))
        ));
    }

    #[test]
    fn a_retry_stamp_beyond_the_rate_limit_horizon_is_ignored_by_a_manual_check() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let state = NotificationState {
            update_check: crate::ports::UpdateCheckState {
                retry_not_before: Some(now + jiff::Span::new().hours(24 * 90)),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(state),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            now,
        );

        assert!(
            matches!(outcome, CheckForUpdateOutcome::Ran(_)),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_fresh_release_is_recorded_and_the_etag_is_saved() {
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(answered_state()),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Updated { .. })
        ));
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert_eq!(
            saved.update_check.last_success.unwrap().latest.version,
            AppVersion::published("0.2.0").expect("valid version")
        );
        assert_eq!(saved.update_check.etag, Some("W/\"abc\"".to_string()));
    }

    #[test]
    fn a_304_keeps_the_prior_success_and_clears_any_prior_failure() {
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            update_check: crate::ports::UpdateCheckState {
                etag: Some("W/\"abc\"".to_string()),
                last_success: Some(UpdateCheckSuccess {
                    checked_at: jiff::Timestamp::UNIX_EPOCH,
                    latest: LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                }),
                last_failure: Some(UpdateCheckFailure {
                    checked_at: jiff::Timestamp::UNIX_EPOCH,
                    reason: "old failure".to_string(),
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(Ok(FeedResponse::NotModified)),
            store_with(state),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Unchanged)
        );
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert!(saved.update_check.last_success.is_some());
        assert!(saved.update_check.last_failure.is_none());
    }

    #[test]
    fn a_rate_limit_error_records_retry_not_before_and_never_erases_last_success() {
        let until = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(2);
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            update_check: crate::ports::UpdateCheckState {
                last_success: Some(UpdateCheckSuccess {
                    checked_at: jiff::Timestamp::UNIX_EPOCH,
                    latest: LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(Err(ReleaseFeedError::RateLimited { until })),
            store_with(state),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::UNIX_EPOCH,
        );

        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Failed { .. })
        ));
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert_eq!(saved.update_check.retry_not_before, Some(until));
        assert!(
            saved.update_check.last_success.is_some(),
            "a failure must never erase a prior success"
        );
    }

    #[test]
    fn a_dismissal_made_concurrently_during_the_network_call_is_never_clobbered() {
        let initial = answered_state();
        let mut concurrently_dismissed = initial.clone();
        concurrently_dismissed
            .dismissed
            .insert(NotificationKind::UpdateAvailable, vec!["0.1.0".to_string()]);

        let store = InterleavedNotificationStateStore::default();
        *store.loads.borrow_mut() =
            std::collections::VecDeque::from([initial, concurrently_dismissed.clone()]);
        let use_case = CheckForUpdate::new(FakeReleaseFeed::returning(fresh("0.2.0")), store);

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Updated { .. })
        ));
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert_eq!(
            saved.dismissed, concurrently_dismissed.dismissed,
            "a dismissal saved while the network call was in flight must survive, \
             not be overwritten by the stale state this call loaded before it"
        );
        assert_eq!(
            saved.update_check.last_success.unwrap().latest.version,
            AppVersion::published("0.2.0").expect("valid version"),
            "this call's own update-check result must still be recorded"
        );
    }

    #[test]
    fn a_rate_limit_that_has_expired_is_cleared_on_the_next_successful_attempt() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            update_check: crate::ports::UpdateCheckState {
                // Already in the past relative to `now` — the gate above
                // would only have refused a call while `now < retry_not_before`.
                retry_not_before: Some(jiff::Timestamp::UNIX_EPOCH),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(fresh("0.2.0")),
            store_with(state),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            now,
        );

        assert!(matches!(outcome, CheckForUpdateOutcome::Ran(_)));
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert_eq!(
            saved.update_check.retry_not_before, None,
            "a stale, already-expired rate limit must not linger forever"
        );
    }

    #[test]
    fn a_transport_failure_is_recorded_and_never_erases_last_success() {
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            update_check: crate::ports::UpdateCheckState {
                last_success: Some(UpdateCheckSuccess {
                    checked_at: jiff::Timestamp::UNIX_EPOCH,
                    latest: LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let use_case = CheckForUpdate::new(
            FakeReleaseFeed::returning(Err(ReleaseFeedError::Transport(
                "connection refused".to_string(),
            ))),
            store_with(state),
        );

        let outcome = use_case.execute(
            UpdateCheckRequest::Manual,
            &NetworkPolicy::default(),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert!(matches!(
            outcome,
            CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Failed { .. })
        ));
        let saved = use_case
            .state_store
            .saved
            .borrow()
            .clone()
            .expect("must have saved");
        assert!(saved.update_check.last_success.is_some());
        assert!(saved.update_check.last_failure.is_some());
    }
}
