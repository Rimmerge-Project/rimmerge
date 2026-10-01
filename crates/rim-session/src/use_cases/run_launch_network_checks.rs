//! [`RunLaunchNetworkChecks`]: the once-per-launch orchestrator —
//! [`crate::use_cases::CheckForUpdate`]`(Automatic)` then
//! [`crate::use_cases::RefreshRuleDatabases::execute_automatic`], both
//! gated by the same first-run/once-per-launch rule. Desktop only (the
//! CLI never runs anything automatically — see
//! `crates/rim-session/CLAUDE.md`'s own "no sort path touches the
//! network" guarantee, which this use case is careful never to touch:
//! it only ever refreshes rule-database *caches* and checks for an
//! update, never scans, sorts, or reads a session).

use std::path::Path;

use crate::app_settings::NetworkPolicy;
use crate::ports::{
    ImportManifestStore, NotificationStateStore, RefreshOutcome, ReleaseFeed, RuleDatabase,
    RuleDatabaseFetcher,
};
use crate::use_cases::check_for_update::{
    CheckForUpdate, CheckForUpdateOutcome, UpdateCheckRequest, UpdateCheckSkipReason,
};
use crate::use_cases::refresh_rule_databases::RefreshRuleDatabases;

/// [`RunLaunchNetworkChecks::execute`]'s full result — both sub-checks'
/// own outcomes, for a caller that wants to log or surface them (neither
/// is shown as a notice on its own; the update notice/stale reminder are
/// both *derived*, at read time, from what this call persists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchNetworkChecksOutcome {
    /// What [`crate::use_cases::CheckForUpdate`] did.
    pub update_check: CheckForUpdateOutcome,
    /// What the automatic rule-database refresh did, per source — empty
    /// when skipped by the shared first-run/once-per-launch gate (never
    /// empty merely because every source happened to be up to date;
    /// that case still returns one `Unchanged`/`Skipped` entry per
    /// eligible source).
    pub database_refresh: Vec<(RuleDatabase, RefreshOutcome)>,
}

/// Composes [`CheckForUpdate`] and [`RefreshRuleDatabases`], run one
/// after the other, both bounded by their own ports' timeouts, neither
/// depending on the other's outcome beyond the shared gate below.
pub struct RunLaunchNetworkChecks<Feed, State, Fetcher, Manifest> {
    check_for_update: CheckForUpdate<Feed, State>,
    refresh_databases: RefreshRuleDatabases<Fetcher, Manifest>,
}

impl<
    Feed: ReleaseFeed,
    State: NotificationStateStore,
    Fetcher: RuleDatabaseFetcher,
    Manifest: ImportManifestStore,
> RunLaunchNetworkChecks<Feed, State, Fetcher, Manifest>
{
    /// Builds the use case from its two composed use cases — the
    /// desktop composition root already builds both for other callers
    /// (Settings → "Check now", the Databases card), so this just
    /// reuses them rather than taking four raw ports of its own.
    #[must_use]
    pub fn new(
        check_for_update: CheckForUpdate<Feed, State>,
        refresh_databases: RefreshRuleDatabases<Fetcher, Manifest>,
    ) -> Self {
        Self {
            check_for_update,
            refresh_databases,
        }
    }

    /// Runs both checks, gated together: **before either ever calls its
    /// port**, this refuses to do anything at all while the first-run
    /// notice is unanswered, or once already run this launch.
    /// `already_ran_this_launch` is the interface's own `AtomicBool` —
    /// this crate keeps no in-process state of its own (see
    /// `crate::use_cases::CheckForUpdate::execute`'s identical note).
    /// The first-run gate itself is read once, through
    /// [`CheckForUpdate`]'s own `AwaitingFirstRun` skip reason, rather
    /// than a second, separate `NotificationStateStore::load` here.
    #[must_use]
    pub fn execute(
        &self,
        already_ran_this_launch: bool,
        policy: &NetworkPolicy,
        cache_dir: &Path,
        base: &Path,
        now: jiff::Timestamp,
    ) -> LaunchNetworkChecksOutcome {
        let update_check = self.check_for_update.execute(
            UpdateCheckRequest::Automatic {
                already_ran_this_launch,
            },
            policy,
            base,
            now,
        );

        let awaiting_first_run = matches!(
            update_check,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AwaitingFirstRun)
        );
        let database_refresh = if already_ran_this_launch || awaiting_first_run {
            Vec::new()
        } else {
            self.refresh_databases
                .execute_automatic(policy, cache_dir, now)
        };

        LaunchNetworkChecksOutcome {
            update_check,
            database_refresh,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;
    use crate::notifications::AppVersion;
    use crate::ports::{FeedResponse, NotificationState, ReleaseFeedError, StoreError};
    use crate::test_support::{FakeRuleDatabaseFetcher, InMemoryImportManifestStore};

    #[derive(Default)]
    struct FakeReleaseFeed {
        response: RefCell<Option<Result<FeedResponse, ReleaseFeedError>>>,
    }

    impl FakeReleaseFeed {
        fn returning(response: Result<FeedResponse, ReleaseFeedError>) -> Self {
            Self {
                response: RefCell::new(Some(response)),
            }
        }
    }

    impl ReleaseFeed for FakeReleaseFeed {
        fn latest(&self, _etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError> {
            self.response
                .borrow_mut()
                .take()
                .expect("FakeReleaseFeed: latest() called more times than scripted")
        }
    }

    #[derive(Default)]
    struct InMemoryNotificationStateStore {
        state: RefCell<Option<NotificationState>>,
    }

    impl NotificationStateStore for InMemoryNotificationStateStore {
        fn load(&self, _base: &Path) -> NotificationState {
            self.state.borrow().clone().unwrap_or_default()
        }

        fn save(&self, _base: &Path, state: &NotificationState) -> Result<(), StoreError> {
            *self.state.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    fn fresh(version: &str) -> Result<FeedResponse, ReleaseFeedError> {
        Ok(FeedResponse::Fresh {
            release: crate::notifications::LatestRelease {
                version: AppVersion::published(version).expect("valid version"),
                published_at: jiff::Timestamp::UNIX_EPOCH,
            },
            etag: None,
        })
    }

    /// `answered`: seeds `welcome_completed_at` before the state store is
    /// moved into `CheckForUpdate` — has to happen before construction,
    /// since neither use case exposes its ports back out afterward.
    fn use_case(
        answered: bool,
    ) -> RunLaunchNetworkChecks<
        FakeReleaseFeed,
        InMemoryNotificationStateStore,
        FakeRuleDatabaseFetcher,
        InMemoryImportManifestStore,
    > {
        let state_store = InMemoryNotificationStateStore::default();
        if answered {
            state_store
                .save(
                    &PathBuf::from("base"),
                    &NotificationState {
                        welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
                        ..NotificationState::default()
                    },
                )
                .expect("seed");
        }
        RunLaunchNetworkChecks::new(
            CheckForUpdate::new(FakeReleaseFeed::returning(fresh("0.2.0")), state_store),
            RefreshRuleDatabases::new(
                FakeRuleDatabaseFetcher::new(std::collections::BTreeMap::from([
                    (
                        RuleDatabase::CommunityRules,
                        RefreshOutcome::Unchanged {
                            sha256: "a".to_string(),
                        },
                    ),
                    (
                        RuleDatabase::RimmergeRules,
                        RefreshOutcome::Unchanged {
                            sha256: "b".to_string(),
                        },
                    ),
                ])),
                InMemoryImportManifestStore::new(),
            ),
        )
    }

    #[test]
    fn both_checks_are_skipped_before_the_first_run_notice_is_answered() {
        let use_case = use_case(false);

        let outcome = use_case.execute(
            false,
            &NetworkPolicy::default(),
            &PathBuf::from("cache"),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome.update_check,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AwaitingFirstRun)
        );
        assert!(outcome.database_refresh.is_empty());
    }

    #[test]
    fn both_checks_are_skipped_once_already_run_this_launch() {
        let use_case = use_case(true);

        let outcome = use_case.execute(
            true,
            &NetworkPolicy::default(),
            &PathBuf::from("cache"),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            outcome.update_check,
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::AlreadyRanThisLaunch)
        );
        assert!(outcome.database_refresh.is_empty());
    }

    #[test]
    fn both_checks_run_once_answered_and_not_yet_run_this_launch() {
        let use_case = use_case(true);

        let outcome = use_case.execute(
            false,
            &NetworkPolicy::default(),
            &PathBuf::from("cache"),
            &PathBuf::from("base"),
            jiff::Timestamp::now(),
        );

        assert!(matches!(
            outcome.update_check,
            CheckForUpdateOutcome::Ran(_)
        ));
        assert_eq!(
            outcome.database_refresh.len(),
            2,
            "community and rimmerge-rules"
        );
    }
}
