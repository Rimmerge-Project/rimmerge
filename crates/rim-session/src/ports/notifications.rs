//! [`ReleaseFeed`]: the version-check port. Implemented by
//! `rim_io::release_feed::GithubReleaseFeed`; a fake in tests.
//! [`NotificationStateStore`]/[`NotificationState`]: dismissal/mute
//! bookkeeping plus the update-check record, persisted at
//! `<base>/notifications.json` — a sibling of `app-settings.json`,
//! **deliberately loaded leniently** (see [`NotificationStateStore::load`]'s
//! own doc comment), unlike `app-settings.json`'s fail-closed rule.
//! Implemented by `rim_io::notifications::JsonNotificationStateStore`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::notifications::{LatestRelease, NotificationKind};
use crate::ports::StoreError;

/// One [`ReleaseFeed::latest`] query's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedResponse {
    /// A new (or first-ever) release, with the `ETag` to store for the
    /// next conditional GET.
    Fresh {
        /// The parsed release.
        release: LatestRelease,
        /// The response's own `ETag`, when it sent one.
        etag: Option<String>,
    },
    /// The server said `304 Not Modified` — the caller keeps whatever
    /// release it already had on record.
    NotModified,
}

/// Why a release-feed query failed. Never constructed for "no request was
/// made" — that is the caller's own `Skipped` outcome (see
/// `crate::use_cases::CheckForUpdate`), not this port's concern. Every
/// text field is already bounded and never echoes a response body.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseFeedError {
    /// A transport-level failure: DNS, connect, TLS, or a timeout.
    #[error("{0}")]
    Transport(String),
    /// `404` — no release has ever been published.
    #[error("no release has been published yet")]
    NotPublished,
    /// An HTTP status other than `200`/`304`/`404`/a rate-limit status.
    #[error("unexpected HTTP status {0}")]
    HttpStatus(u16),
    /// GitHub's rate limit was reached. `until` is already clamped to at
    /// most 24 h ahead of when the response was parsed.
    #[error("GitHub's rate limit is exhausted until {until}")]
    RateLimited {
        /// When it is safe to try again.
        until: jiff::Timestamp,
    },
    /// The response declared, or turned out to carry, more bytes than
    /// this endpoint's cap.
    #[error("response exceeded the size limit")]
    TooLarge,
    /// The response body didn't deserialize into the expected shape.
    #[error("response did not parse as a release")]
    Malformed,
    /// `tag_name` parsed as semver but carries a pre-release or build
    /// suffix, so it isn't a stable published release.
    #[error("the published tag is not a stable version")]
    NotAStableVersion,
}

/// Queries GitHub's Releases API for the latest published Rimmerge
/// release. Implemented by `rim_io::release_feed::GithubReleaseFeed`
/// against the real transport; a fake in every test, so `cargo nextest
/// run --workspace --all-features` opens no socket.
pub trait ReleaseFeed {
    /// Sends the request, with `etag` as a conditional `If-None-Match`
    /// when given one.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseFeedError`] for any transport, HTTP, size, or
    /// parse failure.
    fn latest(&self, etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError>;
}

/// Forwards to `T`'s own impl — the same blanket `Arc` pattern
/// `crate::ports::scan`'s `DefSourceReader`/`AssetLocator`/`ModAboutReader`
/// impls follow, so a composition root can keep this port behind an
/// `Arc<dyn ReleaseFeed + Send + Sync>` in its adapter set (so a test can
/// substitute a fake) while `crate::use_cases::CheckForUpdate<Feed: ReleaseFeed, _>`
/// still accepts it directly.
impl<T: ReleaseFeed + ?Sized> ReleaseFeed for std::sync::Arc<T> {
    fn latest(&self, etag: Option<&str>) -> Result<FeedResponse, ReleaseFeedError> {
        (**self).latest(etag)
    }
}

/// The most recent update-check success `crate::use_cases::CheckForUpdate`
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckSuccess {
    /// When this check ran.
    pub checked_at: jiff::Timestamp,
    /// The release it found — kept even once a newer check fails, so the
    /// notice (derived from `last_success.latest > running`, never
    /// stored separately) survives an offline stretch.
    pub latest: LatestRelease,
}

/// The most recent update-check failure `crate::use_cases::CheckForUpdate`
/// recorded. Never erases [`UpdateCheckSuccess`] — the two fields are
/// independent, both kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckFailure {
    /// When this check ran.
    pub checked_at: jiff::Timestamp,
    /// A short, already-bounded, human-readable reason — safe to show
    /// verbatim as persistent Settings-page text.
    pub reason: String,
}

/// The update-check half of [`NotificationState`] — modelled on the rule-
/// database cache manifest (`rim_io::databases::manifest::Manifest`):
/// `last_attempt_at` advances on every attempt (success *or* failure) and
/// is a record only: the automatic check runs once per launch and no
/// longer reads it, and `retry_not_before` alone holds back a rate-limited
/// retry. `last_success`/`last_failure` are independent and both persist
/// across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateCheckState {
    /// When the last check (of any outcome) ran.
    pub last_attempt_at: Option<jiff::Timestamp>,
    /// The `ETag` to send as `If-None-Match` on the next check, if a
    /// previous one recorded one.
    pub etag: Option<String>,
    /// Set only while GitHub's rate limit is in effect — both the
    /// automatic and "Check now" paths refuse to call the port again
    /// before this instant.
    pub retry_not_before: Option<jiff::Timestamp>,
    /// The most recent success, if there has ever been one.
    pub last_success: Option<UpdateCheckSuccess>,
    /// The most recent failure, if the last attempt failed (independent
    /// of `last_success` — a failure never clears it).
    pub last_failure: Option<UpdateCheckFailure>,
}

/// Dismissal/mute bookkeeping plus the update-check record — what the
/// app remembers between launches, as opposed to `AppSettings`, which is
/// what the user chose. Loaded **leniently**
/// (see [`NotificationStateStore::load`]): losing this file costs at
/// most re-showing already-seen notices, including the first-run notice
/// — which in turn safely re-closes the automatic network gate, so the
/// failure mode is self-correcting, not silent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationState {
    /// When the first-run (`Welcome`) notice was answered — `None`
    /// means it hasn't been yet, which is also what gates every
    /// automatic network feature (`crate::use_cases::RunLaunchNetworkChecks`).
    pub welcome_completed_at: Option<jiff::Timestamp>,
    /// Every kind's own dismissed fingerprints, oldest first, capped at
    /// [`Self::MAX_DISMISSED_PER_KIND`] — a `Vec`, not a `BTreeSet`,
    /// because "most recent kept" needs insertion order, which a sorted
    /// set can't express. A fingerprint already present is never
    /// duplicated (`crate::use_cases::DismissNotification` moves it to
    /// the back instead of pushing a second copy).
    pub dismissed: BTreeMap<NotificationKind, Vec<String>>,
    /// Every muted kind. Only a kind whose
    /// [`crate::notifications::Notification::dismissal`] is
    /// [`crate::notifications::Dismissal::OccurrenceOrMute`] is ever
    /// muted in practice, but this set itself places no such
    /// restriction — a stray entry for a dismiss-only kind is simply
    /// never consulted.
    pub muted: BTreeSet<NotificationKind>,
    /// The update-check record.
    pub update_check: UpdateCheckState,
}

impl NotificationState {
    /// How many dismissed fingerprints one kind may keep, oldest
    /// dropped first — bounds the file's growth over years of use
    /// without ever needing a migration or a manual clear.
    pub const MAX_DISMISSED_PER_KIND: usize = 64;

    /// Whether `key` was dismissed and hasn't since changed fingerprint.
    #[must_use]
    pub fn is_dismissed(&self, key: &crate::notifications::NotificationKey) -> bool {
        self.dismissed
            .get(&key.kind)
            .is_some_and(|fingerprints| fingerprints.iter().any(|f| f == &key.fingerprint))
    }

    /// Whether `kind` is muted.
    #[must_use]
    pub fn is_muted(&self, kind: NotificationKind) -> bool {
        self.muted.contains(&kind)
    }
}

/// Per-profile game-version acknowledgement —
/// `<profile>/notifications.json`, distinct from the app-global
/// `<base>/notifications.json` ([`NotificationState`]) because it's per
/// *install*, not per machine. Loaded **leniently**,
/// for the identical reason its app-global sibling is (see
/// [`NotificationStateStore::load`]'s own doc comment): losing it costs
/// at most re-showing [`crate::notifications::NotificationKind::GameVersionChanged`]
/// once.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileNotificationState {
    /// The game `major.minor` version this profile last acknowledged, as
    /// [`crate::notifications::GameMajorMinor`]'s own `Display` output —
    /// `None` before the first project load has ever recorded one.
    pub acknowledged_game_version: Option<String>,
    /// When the user skipped the Dashboard's "Get the recommended rules"
    /// step for this profile — `None` until they do. Guide progress, not a
    /// setting; it lives here because this file is already the home of
    /// per-profile remembered UI state (see
    /// `crate::recommended_rules::StepSkip` for how it is read).
    pub recommended_rules_skipped_at: Option<jiff::Timestamp>,
}

/// Reads and writes `<profile>/notifications.json`. Implemented by
/// `rim_io::notifications::JsonProfileNotificationStateStore`; a fake in
/// tests.
pub trait ProfileNotificationStateStore {
    /// Loads the file under `profile_dir`. Deliberately lenient, exactly
    /// like [`NotificationStateStore::load`].
    fn load(&self, profile_dir: &Path) -> ProfileNotificationState;

    /// Writes `state` to `profile_dir`, atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, profile_dir: &Path, state: &ProfileNotificationState) -> Result<(), StoreError>;
}

/// Reads and writes `<base>/notifications.json`. Implemented by
/// `rim_io::notifications::JsonNotificationStateStore`; a fake in tests.
pub trait NotificationStateStore {
    /// Loads the file under `base`. **Deliberately lenient** — a
    /// missing, corrupt, or unknown-version file loads as
    /// [`NotificationState::default`], the same rule the rule-database
    /// cache manifest already follows and a **documented deviation**
    /// from `crates/rim-io/CLAUDE.md`'s usual "unknown version is a hard
    /// error" rule. This is state, not a preference: losing it costs at
    /// most re-showing notices, never a silently-changed behaviour (see
    /// this type's own doc comment for why that's safe even for the
    /// first-run notice).
    fn load(&self, base: &Path) -> NotificationState;

    /// Writes `state` to `base`, atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, base: &Path, state: &NotificationState) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::NotificationKey;

    #[test]
    fn is_dismissed_matches_kind_and_fingerprint_together() {
        let mut state = NotificationState::default();
        state
            .dismissed
            .insert(NotificationKind::UpdateAvailable, vec!["0.2.0".to_string()]);

        assert!(state.is_dismissed(&NotificationKey {
            kind: NotificationKind::UpdateAvailable,
            fingerprint: "0.2.0".to_string(),
        }));
        assert!(!state.is_dismissed(&NotificationKey {
            kind: NotificationKind::UpdateAvailable,
            fingerprint: "0.3.0".to_string(),
        }));
        assert!(!state.is_dismissed(&NotificationKey {
            kind: NotificationKind::Welcome,
            fingerprint: "0.2.0".to_string(),
        }));
    }

    #[test]
    fn is_muted_checks_the_muted_set() {
        let mut state = NotificationState::default();
        state.muted.insert(NotificationKind::RuleDatabasesStale);

        assert!(state.is_muted(NotificationKind::RuleDatabasesStale));
        assert!(!state.is_muted(NotificationKind::ImportedRulesOutdated));
    }
}
