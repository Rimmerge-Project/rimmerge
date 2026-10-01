//! The small, state-mutating notification use cases:
//! [`ListNotifications`] (read-only), [`DismissNotification`],
//! [`MuteNotificationKind`]/[`UnmuteNotificationKind`], and
//! [`CompleteWelcome`]. [`crate::use_cases::CheckForUpdate`] and
//! [`crate::use_cases::RunLaunchNetworkChecks`] are large enough to keep
//! their own files.

use std::path::Path;

use crate::app_settings::AppSettings;
use crate::notifications::evaluate::{NotificationInputs, evaluate};
use crate::notifications::{
    AppVersion, GameMajorMinor, Notification, NotificationKey, NotificationKind,
};
use crate::ports::{
    AppSettingsStore, NotificationState, NotificationStateStore, ProfileNotificationStateStore,
    StoreError,
};
use crate::settings::Settings;
use crate::use_cases::RuleDatabaseView;

/// This profile's current game version and what it last acknowledged —
/// bundled into one parameter for [`ListNotifications::execute`], the
/// same convention `databases`/`running` already follow: the caller
/// already has both, from its own already-loaded `Session` and a
/// [`ProfileNotificationStateStore`] read, and reuses rather than
/// re-derives them.
#[derive(Debug, Clone)]
pub struct GameVersionAcknowledgement {
    /// This profile's current game `major.minor` version.
    pub current: GameMajorMinor,
    /// The version this profile last acknowledged, if any.
    pub acknowledged: Option<GameMajorMinor>,
}

/// Builds the active notice list. Read-only — never touches
/// `notifications.json`'s own writer path.
///
/// `databases` and `running` are supplied by the caller rather than
/// fetched here, so this use case owns no `RuleDatabaseFetcher`/
/// `ReleaseFeed` port of its own: the desktop composition root already
/// builds a `Vec<RuleDatabaseView>` for the Databases card
/// (`RefreshRuleDatabases::status`) and already knows the running
/// version, so this call reuses both rather than re-deriving either.
pub struct ListNotifications<AppStore, StateStore> {
    app_settings_store: AppStore,
    state_store: StateStore,
}

impl<AppStore: AppSettingsStore, StateStore: NotificationStateStore>
    ListNotifications<AppStore, StateStore>
{
    /// Builds the use case from its two ports.
    #[must_use]
    pub fn new(app_settings_store: AppStore, state_store: StateStore) -> Self {
        Self {
            app_settings_store,
            state_store,
        }
    }

    /// Every active notice, as of `now`.
    #[must_use]
    pub fn execute(
        &self,
        base: &Path,
        profile: Settings,
        databases: Vec<RuleDatabaseView>,
        running: AppVersion,
        game_version: GameVersionAcknowledgement,
        now: jiff::Timestamp,
    ) -> Vec<Notification> {
        let app = self.app_settings_store.load(base).settings();
        let state = self.state_store.load(base);
        let inputs = NotificationInputs {
            app,
            profile,
            databases,
            running,
            current_game_version: game_version.current,
            acknowledged_game_version: game_version.acknowledged,
        };
        evaluate(&inputs, &state, now)
    }
}

/// Dismisses one occurrence — its `NotificationKey`'s fingerprint moves
/// to the back of its kind's own dismissed list (capped at
/// [`NotificationState::MAX_DISMISSED_PER_KIND`], oldest dropped first).
/// A different fingerprint of the same kind shows again on its own.
///
/// Dismissing a [`NotificationKind::Welcome`] key **also answers it**:
/// `welcome_completed_at` is set to `now`, exactly as [`CompleteWelcome`]
/// would. Closing the Welcome card from the bell's own × (rather than
/// clicking one of its buttons) is documented behavior — "dismissing the
/// card without clicking either also counts as 'keep these settings,'
/// since the explanation was visible first"
/// (`docs/desktop.md`'s Notifications section) — and every automatic
/// network gate (`CheckForUpdate`, `RunLaunchNetworkChecks`) reads
/// `welcome_completed_at`, not the dismissed-fingerprint list, so
/// skipping this would leave those gates closed forever even though the
/// notice itself is gone. Answering Welcome also pins a missing
/// `app-settings.json` (see `pin_policy_if_missing`).
pub struct DismissNotification<AppStore, StateStore> {
    app_settings_store: AppStore,
    state_store: StateStore,
}

impl<AppStore: AppSettingsStore, StateStore: NotificationStateStore>
    DismissNotification<AppStore, StateStore>
{
    /// Builds the use case from its two ports.
    #[must_use]
    pub fn new(app_settings_store: AppStore, state_store: StateStore) -> Self {
        Self {
            app_settings_store,
            state_store,
        }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when a write fails. There is no
    /// in-memory `Session`-style aggregate to roll back here (unlike
    /// this crate's other mutating use cases) — `state` is loaded fresh
    /// on every call and simply dropped on a failed save, leaving
    /// nothing to undo. Dismissing Welcome pins the settings file
    /// first, so a failed pin leaves Welcome unanswered and visible.
    pub fn execute(
        &self,
        base: &Path,
        key: &NotificationKey,
        now: jiff::Timestamp,
    ) -> Result<(), StoreError> {
        if key.kind == NotificationKind::Welcome {
            pin_policy_if_missing(&self.app_settings_store, base)?;
        }
        let mut state = self.state_store.load(base);
        let fingerprints = state.dismissed.entry(key.kind).or_default();
        fingerprints.retain(|existing| existing != &key.fingerprint);
        fingerprints.push(key.fingerprint.clone());
        if fingerprints.len() > NotificationState::MAX_DISMISSED_PER_KIND {
            let excess = fingerprints.len() - NotificationState::MAX_DISMISSED_PER_KIND;
            fingerprints.drain(0..excess);
        }
        if key.kind == NotificationKind::Welcome {
            state.welcome_completed_at = Some(now);
        }
        self.state_store.save(base, &state)
    }
}

/// Mutes a whole [`NotificationKind`] — "Don't remind me again". Only a
/// kind whose [`crate::notifications::Notification::dismissal`] is
/// [`crate::notifications::Dismissal::OccurrenceOrMute`] ever offers
/// this button, but this use case itself doesn't check that — a stray
/// call for a dismiss-only kind is simply never consulted by
/// `evaluate`.
pub struct MuteNotificationKind<StateStore> {
    state_store: StateStore,
}

impl<StateStore: NotificationStateStore> MuteNotificationKind<StateStore> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(state_store: StateStore) -> Self {
        Self { state_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, base: &Path, kind: NotificationKind) -> Result<(), StoreError> {
        let mut state = self.state_store.load(base);
        state.muted.insert(kind);
        self.state_store.save(base, &state)
    }
}

/// Reverses [`MuteNotificationKind`].
pub struct UnmuteNotificationKind<StateStore> {
    state_store: StateStore,
}

impl<StateStore: NotificationStateStore> UnmuteNotificationKind<StateStore> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(state_store: StateStore) -> Self {
        Self { state_store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, base: &Path, kind: NotificationKind) -> Result<(), StoreError> {
        let mut state = self.state_store.load(base);
        state.muted.remove(&kind);
        self.state_store.save(base, &state)
    }
}

/// Answers the first-run notice — "Keep these settings"/"Use recommended
/// settings" call this alone, "Turn off internet access" after its own
/// `UpdateAppSettings` write. When `app-settings.json` is missing, the
/// answer also writes it (see `pin_policy_if_missing`), so a later
/// build changing a default never silently changes what the user
/// answered. Idempotent to call more than once: `welcome_completed_at`
/// simply advances to the latest call — the only externally visible
/// fact is "answered", which stays true either way.
pub struct CompleteWelcome<AppStore, StateStore> {
    app_settings_store: AppStore,
    state_store: StateStore,
}

impl<AppStore: AppSettingsStore, StateStore: NotificationStateStore>
    CompleteWelcome<AppStore, StateStore>
{
    /// Builds the use case from its two ports.
    #[must_use]
    pub fn new(app_settings_store: AppStore, state_store: StateStore) -> Self {
        Self {
            app_settings_store,
            state_store,
        }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when a write fails. The settings file is
    /// pinned first, so a failed pin leaves Welcome unanswered.
    pub fn execute(&self, base: &Path, now: jiff::Timestamp) -> Result<(), StoreError> {
        pin_policy_if_missing(&self.app_settings_store, base)?;
        let mut state = self.state_store.load(base);
        state.welcome_completed_at = Some(now);
        self.state_store.save(base, &state)
    }
}

/// Writes [`AppSettings::default`] to `<base>/app-settings.json` when the
/// file is missing, so the policy a user was shown (and answered) on
/// Welcome stays theirs when a later build changes a default. A `Loaded`
/// file is already pinned, and a `Recovered` (corrupt) one is never
/// overwritten implicitly: the fail-closed state is the user's to repair
/// from Settings. It is one [`AppSettingsStore::save_if_missing`] call,
/// never a `load` followed by a `save`: that pair would let this write
/// replace a file another writer created in between, and the user's own
/// choice with it. Shared by [`CompleteWelcome`] and
/// [`DismissNotification`]'s Welcome branch so the answer paths cannot
/// diverge.
fn pin_policy_if_missing(store: &impl AppSettingsStore, base: &Path) -> Result<(), StoreError> {
    store.save_if_missing(base, &AppSettings::default())
}

/// Silently seeds this profile's own acknowledged game version the very
/// first time a project loads — an already-recorded acknowledgement is
/// left untouched. Called once per load/rescan (the desktop's own
/// `finish_loaded_session`, right after a fresh `Session` is built), not
/// from [`ListNotifications`], so a plain read of the notice list never
/// carries this side effect. Firing
/// [`Notification::GameVersionChanged`] itself is `evaluate`'s job
/// (comparing this seeded value against the session's current version
/// on every later load) — this use case only ever seeds the first value,
/// so a fresh profile never sees a spurious notice on its first load.
pub struct SyncGameVersionAcknowledgement<Store> {
    store: Store,
}

impl<Store: ProfileNotificationStateStore> SyncGameVersionAcknowledgement<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, profile_dir: &Path, current: &GameMajorMinor) -> Result<(), StoreError> {
        let mut state = self.store.load(profile_dir);
        if state.acknowledged_game_version.is_some() {
            return Ok(());
        }
        state.acknowledged_game_version = Some(current.to_string());
        self.store.save(profile_dir, &state)
    }
}

/// Acknowledges a new game version — "dismissing"
/// [`Notification::GameVersionChanged`] calls this instead of the
/// generic [`DismissNotification`], since this notice's own dismissal
/// state lives in the per-profile store, not `<base>/notifications.json`:
/// dismissing acknowledges the new version.
pub struct AcknowledgeGameVersion<Store> {
    store: Store,
}

impl<Store: ProfileNotificationStateStore> AcknowledgeGameVersion<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn execute(&self, profile_dir: &Path, version: &GameMajorMinor) -> Result<(), StoreError> {
        let mut state = self.store.load(profile_dir);
        state.acknowledged_game_version = Some(version.to_string());
        self.store.save(profile_dir, &state)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;
    use crate::app_settings::AppSettings;
    use crate::ports::AppSettingsLoad;

    #[derive(Default)]
    struct InMemoryAppSettingsStore {
        settings: RefCell<Option<AppSettings>>,
        is_recovered: RefCell<bool>,
        fail_next_save: RefCell<bool>,
        /// A file another writer creates between this store's `load` and
        /// its next write: `load` still answers `Missing`, then the first
        /// `save_if_missing` finds the winner's file there.
        racing_writer: RefCell<Option<AppSettings>>,
    }

    impl InMemoryAppSettingsStore {
        fn losing_a_race_to(winner: AppSettings) -> Self {
            Self {
                racing_writer: RefCell::new(Some(winner)),
                ..Self::default()
            }
        }

        fn loaded(settings: AppSettings) -> Self {
            Self {
                settings: RefCell::new(Some(settings)),
                ..Self::default()
            }
        }

        fn recovered() -> Self {
            Self {
                is_recovered: RefCell::new(true),
                ..Self::default()
            }
        }

        fn failing_next_save() -> Self {
            Self {
                fail_next_save: RefCell::new(true),
                ..Self::default()
            }
        }

        fn saved(&self) -> Option<AppSettings> {
            *self.settings.borrow()
        }
    }

    impl AppSettingsStore for InMemoryAppSettingsStore {
        fn load(&self, _base: &Path) -> AppSettingsLoad {
            if *self.is_recovered.borrow() {
                return AppSettingsLoad::Recovered {
                    reason: "corrupt".to_string(),
                };
            }
            self.settings
                .borrow()
                .map_or(AppSettingsLoad::Missing, AppSettingsLoad::Loaded)
        }

        fn save(&self, _base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
            if *self.fail_next_save.borrow() {
                *self.fail_next_save.borrow_mut() = false;
                return Err(StoreError("boom".to_string()));
            }
            *self.settings.borrow_mut() = Some(*settings);
            Ok(())
        }

        fn save_if_missing(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
            if let Some(winner) = self.racing_writer.borrow_mut().take() {
                // The other writer's file appeared after this caller last
                // looked: it stands, and nothing is written.
                *self.settings.borrow_mut() = Some(winner);
                return Ok(());
            }
            if *self.is_recovered.borrow() || self.settings.borrow().is_some() {
                return Ok(());
            }
            self.save(base, settings)
        }
    }

    #[derive(Default)]
    struct InMemoryNotificationStateStore {
        state: RefCell<Option<NotificationState>>,
        fail_next_save: RefCell<bool>,
    }

    impl InMemoryNotificationStateStore {
        fn fail_next_save(&self) {
            *self.fail_next_save.borrow_mut() = true;
        }
    }

    impl NotificationStateStore for InMemoryNotificationStateStore {
        fn load(&self, _base: &Path) -> NotificationState {
            self.state.borrow().clone().unwrap_or_default()
        }

        fn save(&self, _base: &Path, state: &NotificationState) -> Result<(), StoreError> {
            if *self.fail_next_save.borrow() {
                *self.fail_next_save.borrow_mut() = false;
                return Err(StoreError("boom".to_string()));
            }
            *self.state.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryProfileNotificationStateStore {
        state: RefCell<Option<crate::ports::ProfileNotificationState>>,
    }

    impl ProfileNotificationStateStore for InMemoryProfileNotificationStateStore {
        fn load(&self, _profile_dir: &Path) -> crate::ports::ProfileNotificationState {
            self.state.borrow().clone().unwrap_or_default()
        }

        fn save(
            &self,
            _profile_dir: &Path,
            state: &crate::ports::ProfileNotificationState,
        ) -> Result<(), StoreError> {
            *self.state.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    fn running() -> AppVersion {
        AppVersion::running("0.1.0").expect("valid version")
    }

    /// No `GameVersionChanged` notice: acknowledged equals current.
    fn no_game_version_change() -> GameVersionAcknowledgement {
        GameVersionAcknowledgement {
            current: GameMajorMinor::parse("1.6"),
            acknowledged: Some(GameMajorMinor::parse("1.6")),
        }
    }

    #[test]
    fn list_notifications_reads_through_both_ports_and_evaluates() {
        let use_case = ListNotifications::new(
            InMemoryAppSettingsStore::default(),
            InMemoryNotificationStateStore::default(),
        );

        let notices = use_case.execute(
            &PathBuf::from("base"),
            Settings::default(),
            Vec::new(),
            running(),
            no_game_version_change(),
            jiff::Timestamp::now(),
        );

        assert_eq!(
            notices.len(),
            1,
            "a brand-new machine shows exactly the Welcome notice"
        );
        assert_eq!(notices[0].kind(), NotificationKind::Welcome);
    }

    #[test]
    fn dismiss_notification_removes_it_from_the_next_list_and_answers_welcome() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let dismiss = DismissNotification::new(InMemoryAppSettingsStore::default(), state_store);
        let now = jiff::Timestamp::now();

        dismiss
            .execute(
                &base,
                &NotificationKey {
                    kind: NotificationKind::Welcome,
                    fingerprint: "welcome".to_string(),
                },
                now,
            )
            .expect("dismiss must succeed");

        assert_eq!(
            dismiss.state_store.load(&base).welcome_completed_at,
            Some(now),
            "closing Welcome from the bell's x must answer it too, or every \
             automatic network gate stays closed forever"
        );

        let list = ListNotifications::new(InMemoryAppSettingsStore::default(), dismiss.state_store);
        let notices = list.execute(
            &base,
            Settings::default(),
            Vec::new(),
            running(),
            no_game_version_change(),
            jiff::Timestamp::now(),
        );
        assert!(notices.is_empty());
    }

    #[test]
    fn dismissing_a_non_welcome_kind_never_answers_welcome() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let dismiss = DismissNotification::new(InMemoryAppSettingsStore::default(), state_store);

        dismiss
            .execute(
                &base,
                &NotificationKey {
                    kind: NotificationKind::UpdateAvailable,
                    fingerprint: "0.2.0".to_string(),
                },
                jiff::Timestamp::now(),
            )
            .expect("dismiss must succeed");

        assert_eq!(dismiss.state_store.load(&base).welcome_completed_at, None);
    }

    #[test]
    fn dismissing_welcome_reopens_the_automatic_update_check_gate() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let dismiss = DismissNotification::new(InMemoryAppSettingsStore::default(), state_store);
        let now = jiff::Timestamp::now();

        dismiss
            .execute(
                &base,
                &NotificationKey {
                    kind: NotificationKind::Welcome,
                    fingerprint: "welcome".to_string(),
                },
                now,
            )
            .expect("dismiss must succeed");

        struct NeverCalledFeed;
        impl crate::ports::ReleaseFeed for NeverCalledFeed {
            fn latest(
                &self,
                _etag: Option<&str>,
            ) -> Result<crate::ports::FeedResponse, crate::ports::ReleaseFeedError> {
                Ok(crate::ports::FeedResponse::Fresh {
                    release: crate::notifications::LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: jiff::Timestamp::UNIX_EPOCH,
                    },
                    etag: None,
                })
            }
        }

        let check = crate::use_cases::CheckForUpdate::new(NeverCalledFeed, dismiss.state_store);
        let outcome = check.execute(
            crate::use_cases::UpdateCheckRequest::Automatic {
                already_ran_this_launch: false,
            },
            &crate::app_settings::NetworkPolicy::default(),
            &base,
            now,
        );

        assert!(
            matches!(outcome, crate::use_cases::CheckForUpdateOutcome::Ran(_)),
            "answering Welcome via dismiss must reopen the automatic update-check gate, \
             not just remove the notice: got {outcome:?}"
        );
    }

    #[test]
    fn dismiss_notification_caps_at_64_and_drops_the_oldest() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let dismiss = DismissNotification::new(InMemoryAppSettingsStore::default(), state_store);

        for i in 0..70 {
            dismiss
                .execute(
                    &base,
                    &NotificationKey {
                        kind: NotificationKind::UpdateAvailable,
                        fingerprint: format!("0.{i}.0"),
                    },
                    jiff::Timestamp::now(),
                )
                .expect("dismiss must succeed");
        }

        let saved = dismiss.state_store.load(&base);
        let fingerprints = &saved.dismissed[&NotificationKind::UpdateAvailable];
        assert_eq!(
            fingerprints.len(),
            NotificationState::MAX_DISMISSED_PER_KIND
        );
        assert_eq!(
            fingerprints.last().expect("non-empty"),
            "0.69.0",
            "the most recent dismissal is kept"
        );
        assert!(
            !fingerprints.contains(&"0.0.0".to_string()),
            "the oldest dismissal must have been dropped"
        );
    }

    #[test]
    fn a_failed_dismiss_save_is_reported_and_never_partially_applied() {
        let state_store = InMemoryNotificationStateStore::default();
        state_store.fail_next_save();
        let dismiss = DismissNotification::new(InMemoryAppSettingsStore::default(), state_store);

        let result = dismiss.execute(
            &PathBuf::from("base"),
            &NotificationKey {
                kind: NotificationKind::Welcome,
                fingerprint: "welcome".to_string(),
            },
            jiff::Timestamp::now(),
        );

        assert!(result.is_err());
        assert!(
            dismiss
                .state_store
                .load(&PathBuf::from("base"))
                .dismissed
                .is_empty()
        );
    }

    #[test]
    fn mute_then_unmute_round_trips() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let mute = MuteNotificationKind::new(state_store);

        mute.execute(&base, NotificationKind::RuleDatabasesStale)
            .expect("mute must succeed");
        assert!(
            mute.state_store
                .load(&base)
                .is_muted(NotificationKind::RuleDatabasesStale)
        );

        let unmute = UnmuteNotificationKind::new(mute.state_store);
        unmute
            .execute(&base, NotificationKind::RuleDatabasesStale)
            .expect("unmute must succeed");
        assert!(
            !unmute
                .state_store
                .load(&base)
                .is_muted(NotificationKind::RuleDatabasesStale)
        );
    }

    #[test]
    fn complete_welcome_records_the_timestamp_and_removes_the_notice() {
        let state_store = InMemoryNotificationStateStore::default();
        let base = PathBuf::from("base");
        let now = jiff::Timestamp::now();
        let complete = CompleteWelcome::new(InMemoryAppSettingsStore::default(), state_store);

        complete.execute(&base, now).expect("must succeed");

        assert_eq!(
            complete.state_store.load(&base).welcome_completed_at,
            Some(now)
        );
        let list =
            ListNotifications::new(InMemoryAppSettingsStore::default(), complete.state_store);
        assert!(
            !list
                .execute(
                    &base,
                    Settings::default(),
                    Vec::new(),
                    running(),
                    no_game_version_change(),
                    now,
                )
                .iter()
                .any(|n| n.kind() == NotificationKind::Welcome)
        );
    }

    fn welcome_key() -> NotificationKey {
        NotificationKey {
            kind: NotificationKind::Welcome,
            fingerprint: "welcome".to_string(),
        }
    }

    #[test]
    fn complete_welcome_pins_the_default_policy_when_the_file_is_missing() {
        let complete = CompleteWelcome::new(
            InMemoryAppSettingsStore::default(),
            InMemoryNotificationStateStore::default(),
        );

        complete
            .execute(&PathBuf::from("base"), jiff::Timestamp::now())
            .expect("must succeed");

        assert_eq!(
            complete.app_settings_store.saved(),
            Some(AppSettings::default())
        );
    }

    #[test]
    fn dismissing_welcome_pins_the_default_policy_when_the_file_is_missing() {
        let dismiss = DismissNotification::new(
            InMemoryAppSettingsStore::default(),
            InMemoryNotificationStateStore::default(),
        );

        dismiss
            .execute(
                &PathBuf::from("base"),
                &welcome_key(),
                jiff::Timestamp::now(),
            )
            .expect("must succeed");

        assert_eq!(
            dismiss.app_settings_store.saved(),
            Some(AppSettings::default())
        );
    }

    #[test]
    fn answering_welcome_leaves_an_existing_policy_untouched() {
        let mut chosen = AppSettings::default();
        chosen.network.fetch_steam_workshop = false;
        let complete = CompleteWelcome::new(
            InMemoryAppSettingsStore::loaded(chosen),
            InMemoryNotificationStateStore::default(),
        );

        complete
            .execute(&PathBuf::from("base"), jiff::Timestamp::now())
            .expect("must succeed");

        assert_eq!(complete.app_settings_store.saved(), Some(chosen));
    }

    #[test]
    fn answering_welcome_never_overwrites_a_file_another_writer_just_created() {
        let mut winner = AppSettings::default();
        winner.network.allow_network = false;
        let base = PathBuf::from("base");

        let complete = CompleteWelcome::new(
            InMemoryAppSettingsStore::losing_a_race_to(winner),
            InMemoryNotificationStateStore::default(),
        );
        complete
            .execute(&base, jiff::Timestamp::now())
            .expect("losing the race is success");
        assert_eq!(complete.app_settings_store.saved(), Some(winner));
        assert!(
            complete
                .state_store
                .load(&base)
                .welcome_completed_at
                .is_some()
        );

        let dismiss = DismissNotification::new(
            InMemoryAppSettingsStore::losing_a_race_to(winner),
            InMemoryNotificationStateStore::default(),
        );
        dismiss
            .execute(&base, &welcome_key(), jiff::Timestamp::now())
            .expect("losing the race is success");
        assert_eq!(dismiss.app_settings_store.saved(), Some(winner));
        assert!(
            dismiss
                .state_store
                .load(&base)
                .welcome_completed_at
                .is_some()
        );
    }

    #[test]
    fn answering_welcome_never_overwrites_a_recovered_file() {
        let base = PathBuf::from("base");
        let complete = CompleteWelcome::new(
            InMemoryAppSettingsStore::recovered(),
            InMemoryNotificationStateStore::default(),
        );
        complete
            .execute(&base, jiff::Timestamp::now())
            .expect("must succeed");
        assert_eq!(complete.app_settings_store.saved(), None);

        let dismiss = DismissNotification::new(
            InMemoryAppSettingsStore::recovered(),
            InMemoryNotificationStateStore::default(),
        );
        dismiss
            .execute(&base, &welcome_key(), jiff::Timestamp::now())
            .expect("must succeed");
        assert_eq!(dismiss.app_settings_store.saved(), None);
    }

    #[test]
    fn dismissing_another_kind_never_pins_the_policy() {
        let dismiss = DismissNotification::new(
            InMemoryAppSettingsStore::default(),
            InMemoryNotificationStateStore::default(),
        );

        dismiss
            .execute(
                &PathBuf::from("base"),
                &NotificationKey {
                    kind: NotificationKind::UpdateAvailable,
                    fingerprint: "0.2.0".to_string(),
                },
                jiff::Timestamp::now(),
            )
            .expect("must succeed");

        assert_eq!(dismiss.app_settings_store.saved(), None);
    }

    #[test]
    fn a_failed_pin_leaves_welcome_unanswered_on_both_answer_paths() {
        let base = PathBuf::from("base");
        let complete = CompleteWelcome::new(
            InMemoryAppSettingsStore::failing_next_save(),
            InMemoryNotificationStateStore::default(),
        );
        assert!(complete.execute(&base, jiff::Timestamp::now()).is_err());
        assert_eq!(complete.state_store.load(&base).welcome_completed_at, None);

        let dismiss = DismissNotification::new(
            InMemoryAppSettingsStore::failing_next_save(),
            InMemoryNotificationStateStore::default(),
        );
        assert!(
            dismiss
                .execute(&base, &welcome_key(), jiff::Timestamp::now())
                .is_err()
        );
        let state = dismiss.state_store.load(&base);
        assert_eq!(state.welcome_completed_at, None);
        assert!(state.dismissed.is_empty());
    }

    #[test]
    fn sync_game_version_acknowledgement_seeds_only_the_first_time() {
        let store = InMemoryProfileNotificationStateStore::default();
        let profile_dir = PathBuf::from("profile");
        let sync = SyncGameVersionAcknowledgement::new(store);

        sync.execute(&profile_dir, &GameMajorMinor::parse("1.6"))
            .expect("must succeed");
        assert_eq!(
            sync.store.load(&profile_dir).acknowledged_game_version,
            Some("1.6".to_string())
        );

        // A later load with a different current version must not
        // overwrite an already-recorded acknowledgement — only
        // `AcknowledgeGameVersion` (an explicit dismiss) does that.
        sync.execute(&profile_dir, &GameMajorMinor::parse("1.7"))
            .expect("must succeed");
        assert_eq!(
            sync.store.load(&profile_dir).acknowledged_game_version,
            Some("1.6".to_string()),
            "already-seeded acknowledgement is left untouched"
        );
    }

    #[test]
    fn acknowledge_game_version_overwrites_whatever_was_stored() {
        let store = InMemoryProfileNotificationStateStore::default();
        let profile_dir = PathBuf::from("profile");
        let acknowledge = AcknowledgeGameVersion::new(store);

        acknowledge
            .execute(&profile_dir, &GameMajorMinor::parse("1.7"))
            .expect("must succeed");

        assert_eq!(
            acknowledge
                .store
                .load(&profile_dir)
                .acknowledged_game_version,
            Some("1.7".to_string())
        );
    }
}
