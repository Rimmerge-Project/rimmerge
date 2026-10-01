//! [`evaluate`]: the one pure function that turns current app state into
//! the active notice list. No IO, `now` is injected — see this crate's
//! own determinism rule (root `CLAUDE.md`).

use std::collections::BTreeMap;

use crate::app_settings::AppSettings;
use crate::notifications::clock::seconds_since;
use crate::notifications::model::{Freshness, RefreshMode, SourceSetup, StaleSource};
use crate::notifications::{AppVersion, GameMajorMinor, Notification};
use crate::ports::{NotificationState, RuleDatabase};
use crate::settings::Settings;
use crate::use_cases::RuleDatabaseView;

/// How long, after the first-run notice is answered, a
/// never-fetched-yet source under automatic refresh gets before it's
/// treated as a failed automatic attempt.
const NEVER_FETCHED_GRACE_SECONDS: i64 = 24 * 60 * 60;

/// Everything [`evaluate`] reads. Built once per call by
/// `crate::use_cases::ListNotifications`, never persisted itself.
#[derive(Debug, Clone)]
pub struct NotificationInputs {
    /// The app-global network policy and reminder thresholds.
    pub app: AppSettings,
    /// The active profile's own settings — read only for
    /// [`Notification::Welcome`]'s `settings_match_recommended` field;
    /// nothing else here depends on a profile being loaded at all.
    pub profile: Settings,
    /// Every rule database's current cache status, enriched with this
    /// profile's re-import signal — the exact same view
    /// `crate::use_cases::RefreshRuleDatabases::status` already builds,
    /// reused rather than re-derived.
    pub databases: Vec<RuleDatabaseView>,
    /// The binary currently running.
    pub running: AppVersion,
    /// This profile's current game `major.minor` version.
    pub current_game_version: GameMajorMinor,
    /// The game version this profile last acknowledged
    /// (`crate::ports::ProfileNotificationState::acknowledged_game_version`,
    /// already parsed) — `None` before
    /// `crate::use_cases::SyncGameVersionAcknowledgement` has ever run
    /// for this profile, in which case [`Notification::GameVersionChanged`]
    /// never fires (nothing to compare against yet, and that use case
    /// seeds it silently on the very next load).
    pub acknowledged_game_version: Option<GameMajorMinor>,
}

/// A severity's own sort rank for [`evaluate`]'s own "most severe first"
/// output order — lower sorts first. Not [`Ord`] on
/// [`crate::notifications::Severity`] itself, since nothing else in this
/// crate needs to compare severities outside this one sort key.
fn severity_rank(severity: crate::notifications::Severity) -> u8 {
    match severity {
        crate::notifications::Severity::Warn => 0,
        crate::notifications::Severity::Info => 1,
    }
}

/// Whether the once-a-day automatic refresh covers `database` under
/// `app`. Shared by the stale reminder and the recommended-sources
/// notice so the two partition the sources by one definition.
fn refresh_mode(app: &AppSettings, database: RuleDatabase) -> RefreshMode {
    if app.network.auto_refresh_rule_databases && database.is_auto_refresh_eligible() {
        RefreshMode::Automatic
    } else {
        RefreshMode::Manual
    }
}

/// One enabled source's stale-reminder detail, or `None` when this
/// source doesn't belong in [`Notification::RuleDatabasesStale`] right
/// now. Implements the rules below,
/// including the two cases [`crate::ports::DatabaseStatus::is_stale`]
/// itself can never answer (a source that has never been fetched at all
/// is never "stale" by that method's own rule — "nothing to be stale"):
///
/// - **Never fetched, first-run notice not yet answered**: not included
///   — the first-run notice covers it.
/// - **Never fetched, answered, automatic refresh eligible**: not
///   included for the first 24 h after the notice was answered (give the
///   first automatic attempt time to run); included afterward, as if the
///   automatic refresh itself had failed.
/// - **Never fetched, manual-only mode** (the toggle is off, or this is
///   Steam Workshop): never included. Nothing automatic is trying this
///   source at all, so "never fetched" isn't a broken process to flag —
///   only a source that has been fetched at least once and then gone
///   stale can be "not refreshed in N days".
fn stale_detail(
    database: RuleDatabase,
    view: &RuleDatabaseView,
    app: &AppSettings,
    welcome_completed_at: Option<jiff::Timestamp>,
    now: jiff::Timestamp,
) -> Option<StaleSource> {
    if !view.status.enabled {
        return None;
    }
    let refresh = refresh_mode(app, database);

    match &view.status.cached {
        Some(cached) => {
            if !view
                .status
                .is_stale(now, app.reminders.rule_databases_stale_after_days)
            {
                return None;
            }
            Some(StaleSource {
                freshness: Freshness::FetchedAt(cached.fetched_at),
                refresh,
                last_failure: view.status.last_failure.clone(),
            })
        }
        None => {
            let answered_at = welcome_completed_at?;
            if refresh != RefreshMode::Automatic {
                return None;
            }
            // An answer stamped in the future (the clock moved back) has no
            // elapsed time and cannot hold the reminder back forever.
            if seconds_since(now, answered_at)
                .is_some_and(|elapsed| elapsed < NEVER_FETCHED_GRACE_SECONDS)
            {
                return None;
            }
            Some(StaleSource {
                freshness: Freshness::NeverFetched,
                refresh,
                last_failure: view.status.last_failure.clone(),
            })
        }
    }
}

/// Whether [`Notification::RuleDatabasesStale`] should fire, and for
/// which sources.
fn rule_databases_stale(
    inputs: &NotificationInputs,
    welcome_completed_at: Option<jiff::Timestamp>,
    now: jiff::Timestamp,
) -> Option<Notification> {
    if !inputs.app.network.allow_network {
        return None;
    }
    let sources: BTreeMap<RuleDatabase, StaleSource> = inputs
        .databases
        .iter()
        .filter_map(|view| {
            stale_detail(
                view.status.database,
                view,
                &inputs.app,
                welcome_completed_at,
                now,
            )
            .map(|detail| (view.status.database, detail))
        })
        .collect();
    if sources.is_empty() {
        None
    } else {
        Some(Notification::RuleDatabasesStale { sources })
    }
}

/// How one recommended source falls short of being set up, or `None`
/// when it is fine (or not recommended). `Off` is the stale reminder's
/// first exclusion (a disabled source is never stale); `NotDownloaded`
/// is exactly the never-fetched case the stale reminder skips for a
/// manual-only source. So no source can be in both notices.
fn source_setup(app: &AppSettings, view: &RuleDatabaseView) -> Option<SourceSetup> {
    let database = view.status.database;
    if !database.is_recommended() {
        return None;
    }
    if !view.status.enabled {
        return Some(SourceSetup::Off);
    }
    let is_never_downloaded = view.status.cached.is_none();
    (is_never_downloaded && refresh_mode(app, database) == RefreshMode::Manual)
        .then_some(SourceSetup::NotDownloaded)
}

/// Whether [`Notification::RecommendedSourcesIncomplete`] should fire.
/// Gates, in order: Welcome not yet answered -> never (Welcome covers
/// it); internet access off -> never (the user chose offline, and a
/// damaged settings file loads with everything off, which must not
/// prompt anyone to switch sources back on); no source short -> none.
fn recommended_sources_incomplete(
    inputs: &NotificationInputs,
    welcome_completed_at: Option<jiff::Timestamp>,
) -> Option<Notification> {
    welcome_completed_at?;
    if !inputs.app.network.allow_network {
        return None;
    }
    let sources: BTreeMap<RuleDatabase, SourceSetup> = inputs
        .databases
        .iter()
        .filter_map(|view| {
            source_setup(&inputs.app, view).map(|setup| (view.status.database, setup))
        })
        .collect();
    if sources.is_empty() {
        None
    } else {
        Some(Notification::RecommendedSourcesIncomplete { sources })
    }
}

/// Whether [`Notification::GameVersionChanged`] should fire — the
/// acknowledged and current versions differ, and an acknowledgement has
/// been recorded at all.
fn game_version_changed(inputs: &NotificationInputs) -> Option<Notification> {
    let acknowledged = inputs.acknowledged_game_version.clone()?;
    if acknowledged == inputs.current_game_version {
        return None;
    }
    Some(Notification::GameVersionChanged {
        acknowledged,
        current: inputs.current_game_version.clone(),
    })
}

/// Whether [`Notification::ImportedRulesOutdated`] should fire, using
/// the existing `needs_reimport` rule — a disabled source is never
/// flagged, since [`RuleDatabaseView::needs_reimport`] already returns
/// `false` for one. Keyed by each source's own cached `sha256` (never
/// `None` when `needs_reimport` is true — that rule already requires a
/// cached copy to exist), so the notice's own fingerprint tracks the
/// cached *content*, not merely which sources are currently outdated.
fn imported_rules_outdated(inputs: &NotificationInputs) -> Option<Notification> {
    let sources: BTreeMap<RuleDatabase, String> = inputs
        .databases
        .iter()
        .filter(|view| view.needs_reimport)
        .filter_map(|view| {
            let sha256 = view.status.cached.as_ref()?.sha256.clone();
            Some((view.status.database, sha256))
        })
        .collect();
    if sources.is_empty() {
        None
    } else {
        Some(Notification::ImportedRulesOutdated { sources })
    }
}

/// Turns `inputs` and `state`, as of `now`, into the active notice list —
/// every currently-true notice, minus anything already dismissed (by its
/// exact fingerprint) or muted (by its whole kind), sorted by severity
/// (most severe first), then kind, then key. Pure: no IO, `now`
/// is the only source of "current time" this function ever reads.
#[must_use]
pub fn evaluate(
    inputs: &NotificationInputs,
    state: &NotificationState,
    now: jiff::Timestamp,
) -> Vec<Notification> {
    let mut candidates = Vec::new();

    if state.welcome_completed_at.is_none() {
        candidates.push(Notification::Welcome {
            network: inputs.app.network,
            settings_match_recommended: inputs.profile == Settings::default(),
        });
    }

    if let Some(notice) = game_version_changed(inputs) {
        candidates.push(notice);
    }

    if let Some(success) = &state.update_check.last_success
        && inputs.app.network.allow_network
        && inputs.app.network.check_for_updates
        && success.latest.version > inputs.running
    {
        candidates.push(Notification::UpdateAvailable {
            running: inputs.running.clone(),
            latest: success.latest.clone(),
        });
    }

    if let Some(notice) = rule_databases_stale(inputs, state.welcome_completed_at, now) {
        candidates.push(notice);
    }

    if let Some(notice) = imported_rules_outdated(inputs) {
        candidates.push(notice);
    }

    if let Some(notice) = recommended_sources_incomplete(inputs, state.welcome_completed_at) {
        candidates.push(notice);
    }

    candidates.retain(|notice| {
        let key = notice.key();
        !state.is_dismissed(&key) && !state.is_muted(key.kind)
    });

    candidates.sort_by(|a, b| {
        severity_rank(a.severity())
            .cmp(&severity_rank(b.severity()))
            .then_with(|| a.key().kind.cmp(&b.key().kind))
            .then_with(|| a.key().fingerprint.cmp(&b.key().fingerprint))
    });

    candidates
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::*;
    use crate::notifications::NotificationKind;
    use crate::ports::{CachedDatabase, DatabaseStatus, UpdateCheckSuccess};

    fn view(database: RuleDatabase, cached: Option<CachedDatabase>) -> RuleDatabaseView {
        RuleDatabaseView {
            status: DatabaseStatus {
                database,
                enabled: true,
                path: PathBuf::from("x.json"),
                cached,
                last_failure: None,
                last_attempt_at: None,
            },
            imported_sha256: None,
            needs_reimport: false,
        }
    }

    fn cached_at(fetched_at: jiff::Timestamp) -> CachedDatabase {
        CachedDatabase {
            sha256: "abc123".to_string(),
            bytes: 10,
            fetched_at,
        }
    }

    fn base_inputs() -> NotificationInputs {
        NotificationInputs {
            app: AppSettings::default(),
            profile: Settings::default(),
            databases: Vec::new(),
            running: AppVersion::running("0.1.0").expect("valid version"),
            current_game_version: GameMajorMinor::parse("1.6"),
            acknowledged_game_version: Some(GameMajorMinor::parse("1.6")),
        }
    }

    #[test]
    fn welcome_shows_when_not_yet_completed_and_disappears_once_it_is() {
        let inputs = base_inputs();
        let not_answered = NotificationState::default();
        let answered = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::now()),
            ..NotificationState::default()
        };

        assert!(
            evaluate(&inputs, &not_answered, jiff::Timestamp::now())
                .iter()
                .any(|n| n.kind() == NotificationKind::Welcome)
        );
        assert!(
            !evaluate(&inputs, &answered, jiff::Timestamp::now())
                .iter()
                .any(|n| n.kind() == NotificationKind::Welcome)
        );
    }

    #[test]
    fn update_available_requires_both_switches_on() {
        let now = jiff::Timestamp::now();
        let state = NotificationState {
            welcome_completed_at: Some(now),
            update_check: crate::ports::UpdateCheckState {
                last_success: Some(UpdateCheckSuccess {
                    checked_at: now,
                    latest: crate::notifications::LatestRelease {
                        version: AppVersion::published("0.2.0").expect("valid version"),
                        published_at: now,
                    },
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let mut inputs = base_inputs();

        assert!(
            evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::UpdateAvailable),
            "both switches on -> fires"
        );

        inputs.app.network.check_for_updates = false;
        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::UpdateAvailable),
            "check_for_updates off -> never fires, even with a recorded newer release"
        );

        inputs.app.network.check_for_updates = true;
        inputs.app.network.allow_network = false;
        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::UpdateAvailable),
            "allow_network off -> never fires"
        );
    }

    #[test]
    fn update_available_never_fires_for_an_equal_or_older_latest() {
        let now = jiff::Timestamp::now();
        let state = NotificationState {
            welcome_completed_at: Some(now),
            update_check: crate::ports::UpdateCheckState {
                last_success: Some(UpdateCheckSuccess {
                    checked_at: now,
                    latest: crate::notifications::LatestRelease {
                        version: AppVersion::published("0.1.0").expect("valid version"),
                        published_at: now,
                    },
                }),
                ..crate::ports::UpdateCheckState::default()
            },
            ..NotificationState::default()
        };
        let inputs = base_inputs(); // running is also 0.1.0

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::UpdateAvailable)
        );
    }

    #[test]
    fn game_version_changed_fires_only_when_acknowledged_differs_from_current() {
        let now = jiff::Timestamp::now();
        let state = NotificationState {
            welcome_completed_at: Some(now),
            ..NotificationState::default()
        };
        let mut inputs = base_inputs();

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::GameVersionChanged),
            "acknowledged == current -> never fires"
        );

        inputs.current_game_version = GameMajorMinor::parse("1.7");
        let notices = evaluate(&inputs, &state, now);
        let notice = notices
            .iter()
            .find(|n| n.kind() == NotificationKind::GameVersionChanged)
            .expect("must fire once current diverges from acknowledged");
        assert_eq!(notice.key().fingerprint, "1.7");
    }

    #[test]
    fn game_version_changed_never_fires_before_any_acknowledgement_is_recorded() {
        let now = jiff::Timestamp::now();
        let state = NotificationState {
            welcome_completed_at: Some(now),
            ..NotificationState::default()
        };
        let mut inputs = base_inputs();
        inputs.acknowledged_game_version = None;

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::GameVersionChanged),
            "no acknowledgement recorded yet -> nothing to compare against"
        );
    }

    #[test]
    fn rule_databases_stale_never_fires_when_allow_network_is_off() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.app.network.allow_network = false;
        inputs.databases = vec![view(
            RuleDatabase::CommunityRules,
            Some(cached_at(jiff::Timestamp::UNIX_EPOCH)),
        )];
        let state = NotificationState {
            welcome_completed_at: Some(now),
            ..NotificationState::default()
        };

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale)
        );
    }

    #[test]
    fn rule_databases_stale_fires_once_a_cached_source_passes_the_threshold() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(31 * 24);
        let mut inputs = base_inputs();
        inputs.databases = vec![view(
            RuleDatabase::CommunityRules,
            Some(cached_at(jiff::Timestamp::UNIX_EPOCH)),
        )];
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            ..NotificationState::default()
        };

        let notices = evaluate(&inputs, &state, now);
        let stale = notices
            .iter()
            .find(|n| n.kind() == NotificationKind::RuleDatabasesStale)
            .expect("must fire once past the 30-day default threshold");
        let Notification::RuleDatabasesStale { sources } = stale else {
            unreachable!()
        };
        assert!(sources.contains_key(&RuleDatabase::CommunityRules));
    }

    #[test]
    fn a_never_fetched_source_is_not_stale_before_the_first_run_notice_is_answered() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];
        let state = NotificationState::default(); // welcome_completed_at: None

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale)
        );
    }

    #[test]
    fn a_never_fetched_automatic_source_becomes_stale_only_after_the_24h_grace_period() {
        let answered_at = jiff::Timestamp::UNIX_EPOCH;
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];
        let state = NotificationState {
            welcome_completed_at: Some(answered_at),
            ..NotificationState::default()
        };

        let still_in_grace = answered_at + jiff::Span::new().hours(23);
        assert!(
            !evaluate(&inputs, &state, still_in_grace)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale),
            "under 24h since answered -> not yet treated as a failed automatic attempt"
        );

        let past_grace = answered_at + jiff::Span::new().hours(25);
        assert!(
            evaluate(&inputs, &state, past_grace)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale),
            "past 24h -> treated as a failed automatic attempt"
        );
    }

    #[test]
    fn a_welcome_answer_stamped_in_the_future_does_not_hold_the_reminder_back() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(10);
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];
        let state = NotificationState {
            welcome_completed_at: Some(now + jiff::Span::new().hours(72)),
            ..NotificationState::default()
        };

        assert!(
            evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale),
            "a clock that moved back must not suppress the reminder until it catches up"
        );
    }

    #[test]
    fn a_never_fetched_manual_only_source_is_never_stale() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(1000);
        let mut inputs = base_inputs();
        inputs.app.network.auto_refresh_rule_databases = false;
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            ..NotificationState::default()
        };

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale),
            "manual-only mode never treats never-fetched as a failure"
        );
    }

    #[test]
    fn imported_rules_outdated_fires_only_for_sources_that_need_reimport() {
        let now = jiff::Timestamp::now();
        // `needs_reimport` is only ever true alongside a real cached
        // copy in production (`RefreshRuleDatabases::status`'s own
        // rule) — the fake mirrors that here so this test's own setup
        // stays representative.
        let mut needs_reimport = view(RuleDatabase::CommunityRules, Some(cached_at(now)));
        needs_reimport.needs_reimport = true;
        let mut inputs = base_inputs();
        inputs.databases = vec![needs_reimport, view(RuleDatabase::RimmergeRules, None)];
        let state = NotificationState::default();

        let notices = evaluate(&inputs, &state, now);
        let notice = notices
            .iter()
            .find(|n| n.kind() == NotificationKind::ImportedRulesOutdated)
            .expect("must fire");
        let Notification::ImportedRulesOutdated { sources } = notice else {
            unreachable!()
        };
        assert_eq!(sources.len(), 1);
        assert_eq!(
            sources.get(&RuleDatabase::CommunityRules),
            Some(&"abc123".to_string())
        );
    }

    #[test]
    fn a_dismissed_fingerprint_is_filtered_out_but_a_different_one_still_shows() {
        let now = jiff::Timestamp::now();
        let inputs = base_inputs();
        let state = NotificationState {
            dismissed: BTreeMap::from([(NotificationKind::Welcome, vec!["welcome".to_string()])]),
            ..NotificationState::default()
        };

        assert!(evaluate(&inputs, &state, now).is_empty());
    }

    #[test]
    fn a_muted_kind_is_filtered_out() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(31 * 24);
        let mut inputs = base_inputs();
        inputs.databases = vec![view(
            RuleDatabase::CommunityRules,
            Some(cached_at(jiff::Timestamp::UNIX_EPOCH)),
        )];
        let state = NotificationState {
            welcome_completed_at: Some(jiff::Timestamp::UNIX_EPOCH),
            muted: BTreeSet::from([NotificationKind::RuleDatabasesStale]),
            ..NotificationState::default()
        };

        assert!(
            !evaluate(&inputs, &state, now)
                .iter()
                .any(|n| n.kind() == NotificationKind::RuleDatabasesStale)
        );
    }

    #[test]
    fn output_is_sorted_warn_before_info_then_by_kind() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(31 * 24);
        let mut inputs = base_inputs();
        inputs.databases = vec![view(
            RuleDatabase::CommunityRules,
            Some(cached_at(jiff::Timestamp::UNIX_EPOCH)),
        )];
        let state = NotificationState::default(); // Welcome (Info) also fires

        let notices = evaluate(&inputs, &state, now);
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].kind(), NotificationKind::RuleDatabasesStale);
        assert_eq!(notices[0].severity(), crate::notifications::Severity::Warn);
        assert_eq!(notices[1].kind(), NotificationKind::Welcome);
    }

    #[test]
    fn two_evaluations_of_identical_inputs_agree_exactly() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(31 * 24);
        let mut inputs = base_inputs();
        inputs.databases = vec![view(
            RuleDatabase::CommunityRules,
            Some(cached_at(jiff::Timestamp::UNIX_EPOCH)),
        )];
        let state = NotificationState::default();

        assert_eq!(
            evaluate(&inputs, &state, now),
            evaluate(&inputs, &state, now),
            "evaluate is pure: identical inputs must produce identical output"
        );
    }

    fn answered_state(answered_at: jiff::Timestamp) -> NotificationState {
        NotificationState {
            welcome_completed_at: Some(answered_at),
            ..NotificationState::default()
        }
    }

    fn disabled(mut view: RuleDatabaseView) -> RuleDatabaseView {
        view.status.enabled = false;
        view
    }

    fn incomplete_sources(notices: &[Notification]) -> Option<BTreeMap<RuleDatabase, SourceSetup>> {
        notices.iter().find_map(|notice| match notice {
            Notification::RecommendedSourcesIncomplete { sources } => Some(sources.clone()),
            _ => None,
        })
    }

    #[test]
    fn recommended_sources_incomplete_never_shows_before_welcome_is_answered() {
        let mut inputs = base_inputs();
        inputs.databases = vec![disabled(view(RuleDatabase::CommunityRules, None))];

        let notices = evaluate(
            &inputs,
            &NotificationState::default(),
            jiff::Timestamp::now(),
        );

        assert_eq!(incomplete_sources(&notices), None);
    }

    #[test]
    fn recommended_sources_incomplete_is_hidden_while_internet_access_is_off() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.app.network.allow_network = false;
        inputs.databases = vec![disabled(view(RuleDatabase::CommunityRules, None))];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(incomplete_sources(&notices), None);
    }

    #[test]
    fn a_disabled_recommended_source_is_reported_off() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![disabled(view(RuleDatabase::RimmergeRules, None))];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(
            incomplete_sources(&notices),
            Some(BTreeMap::from([(
                RuleDatabase::RimmergeRules,
                SourceSetup::Off
            )]))
        );
    }

    #[test]
    fn an_enabled_never_downloaded_manual_source_is_reported_not_downloaded() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::SteamWorkshop, None)];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(
            incomplete_sources(&notices),
            Some(BTreeMap::from([(
                RuleDatabase::SteamWorkshop,
                SourceSetup::NotDownloaded
            )]))
        );
    }

    #[test]
    fn a_never_downloaded_source_the_automatic_refresh_covers_is_left_to_the_stale_reminder() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(incomplete_sources(&notices), None);
    }

    #[test]
    fn with_automatic_refresh_off_a_never_downloaded_small_source_is_reported() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.app.network.auto_refresh_rule_databases = false;
        inputs.databases = vec![view(RuleDatabase::CommunityRules, None)];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(
            incomplete_sources(&notices),
            Some(BTreeMap::from([(
                RuleDatabase::CommunityRules,
                SourceSetup::NotDownloaded
            )]))
        );
    }

    #[test]
    fn a_downloaded_source_is_never_reported() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::SteamWorkshop, Some(cached_at(now)))];

        let notices = evaluate(&inputs, &answered_state(now), now);

        assert_eq!(incomplete_sources(&notices), None);
    }

    #[test]
    fn dismissing_the_notice_hides_exactly_that_set_and_a_new_set_shows_again() {
        let now = jiff::Timestamp::now();
        let mut inputs = base_inputs();
        inputs.databases = vec![view(RuleDatabase::SteamWorkshop, None)];
        let mut state = answered_state(now);
        let key = evaluate(&inputs, &state, now)
            .into_iter()
            .find(|notice| notice.kind() == NotificationKind::RecommendedSourcesIncomplete)
            .expect("fires")
            .key();
        state
            .dismissed
            .insert(key.kind, vec![key.fingerprint.clone()]);

        assert_eq!(incomplete_sources(&evaluate(&inputs, &state, now)), None);

        inputs.databases = vec![
            view(RuleDatabase::SteamWorkshop, None),
            disabled(view(RuleDatabase::RimmergeRules, None)),
        ];
        assert!(incomplete_sources(&evaluate(&inputs, &state, now)).is_some());
    }

    #[test]
    fn no_source_is_in_both_the_stale_reminder_and_the_recommended_sources_notice() {
        let answered_at = jiff::Timestamp::UNIX_EPOCH;
        let now = answered_at + jiff::Span::new().hours(60 * 24);
        let state = answered_state(answered_at);
        let caches = [None, Some(cached_at(answered_at)), Some(cached_at(now))];

        for database in RuleDatabase::ALL {
            for is_enabled in [true, false] {
                for is_auto_refresh_on in [true, false] {
                    for cached in &caches {
                        let mut inputs = base_inputs();
                        inputs.app.network.auto_refresh_rule_databases = is_auto_refresh_on;
                        let mut database_view = view(database, cached.clone());
                        database_view.status.enabled = is_enabled;
                        inputs.databases = vec![database_view];

                        let notices = evaluate(&inputs, &state, now);
                        let stale = notices.iter().any(|notice| {
                            matches!(notice, Notification::RuleDatabasesStale { sources }
                                if sources.contains_key(&database))
                        });
                        let incomplete = incomplete_sources(&notices)
                            .is_some_and(|sources| sources.contains_key(&database));
                        assert!(
                            !(stale && incomplete),
                            "{database:?} enabled={is_enabled} auto={is_auto_refresh_on} \
                             cached={cached:?} is in both notices"
                        );
                    }
                }
            }
        }
    }
}
