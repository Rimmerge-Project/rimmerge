//! The notice model: [`Notification`] and everything a caller needs to
//! render one — [`NotificationKind`], [`Severity`], [`NotificationKey`],
//! [`NotificationAction`], [`Dismissal`] — plus [`StaleSource`],
//! [`Freshness`], and [`RefreshMode`], which [`Notification::RuleDatabasesStale`]
//! carries per source.
//!
//! Every notice's behaviour ([`Notification::kind`], [`Notification::severity`],
//! [`Notification::key`], [`Notification::actions`], [`Notification::dismissal`])
//! is **derived** from the notice itself, never stored separately —
//! "tell, don't ask". Rust never produces the notice's rendered text —
//! that's the frontend's job, through i18n keys keyed by [`NotificationKind`]
//! and each [`NotificationAction`] (see `apps/desktop/CLAUDE.md`'s
//! "Backend text reaching the UI arrives as codes/enums" rule).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::app_settings::NetworkPolicy;
use crate::notifications::{AppVersion, GameMajorMinor, LatestRelease};
use crate::ports::{FetchFailure, RuleDatabase};

/// Whether a cached rule database has ever been successfully fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Freshness {
    /// Never a successful fetch.
    NeverFetched,
    /// The last successful fetch (or `304`-confirmed-unchanged).
    FetchedAt(jiff::Timestamp),
}

/// Whether the once-a-day automatic refresh covers this source at all —
/// `Manual` covers both "the `auto_refresh_rule_databases` toggle is
/// off" and "this source is never auto-refresh-eligible" (Steam
/// Workshop). The reminder's own wording differs between the two
/// (`RefreshMode::Automatic` says "automatic refresh hasn't
/// succeeded…", `RefreshMode::Manual` says "not refreshed in N
/// days…") — see `crate::use_cases::ListNotifications` for how each is
/// decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefreshMode {
    /// The `auto_refresh_rule_databases` toggle is on and this source is
    /// auto-refresh-eligible — the once-a-day refresh should be keeping
    /// it current, and staleness means that isn't working.
    Automatic,
    /// The toggle is off, or this source is never auto-refresh-eligible
    /// (Steam Workshop) — staleness means the user hasn't refreshed it
    /// by hand.
    Manual,
}

/// One stale source's own detail, as [`Notification::RuleDatabasesStale`]
/// carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleSource {
    /// When this source was last successfully fetched, if ever.
    pub freshness: Freshness,
    /// Whether the once-a-day automatic refresh is expected to cover
    /// this source.
    pub refresh: RefreshMode,
    /// The last refresh attempt's failure, if the most recent attempt
    /// failed: a closed cause the interface translates, plus the
    /// adapter's English text as a technical detail.
    pub last_failure: Option<FetchFailure>,
}

/// Why a recommended rule database is not set up, as
/// [`Notification::RecommendedSourcesIncomplete`] carries it per source.
/// The two cases partition with [`Notification::RuleDatabasesStale`]: a
/// source in either map never appears in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceSetup {
    /// The source's own fetch toggle is off.
    Off,
    /// The toggle is on and nothing was ever downloaded, and the
    /// automatic refresh does not cover the source (Steam Workshop, or
    /// any source while automatic refresh is off), so nothing will
    /// download it unless the user asks.
    NotDownloaded,
}

/// Which kind of notice this is. **`Ord`'s declaration order is display
/// priority** — the most urgent kind sorts first
/// (`crate::notifications::evaluate::evaluate`'s own sort key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationKind {
    /// See [`Notification::Welcome`].
    Welcome,
    /// See [`Notification::GameVersionChanged`].
    GameVersionChanged,
    /// See [`Notification::UpdateAvailable`].
    UpdateAvailable,
    /// See [`Notification::RuleDatabasesStale`].
    RuleDatabasesStale,
    /// See [`Notification::ImportedRulesOutdated`].
    ImportedRulesOutdated,
    /// See [`Notification::RecommendedSourcesIncomplete`]. Declared last:
    /// the lowest display priority.
    RecommendedSourcesIncomplete,
}

/// A notice's urgency — PrimeVue's `"info"`/`"warn"` severities,
/// **never** `"warning"` (see `apps/desktop/CLAUDE.md`'s own trap on
/// this exact typo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informational — nothing is broken, nothing needs attention soon.
    Info,
    /// Something the user should look at, though nothing is blocked.
    Warn,
}

/// Identifies one notice occurrence for dismissal/mute bookkeeping.
/// `fingerprint` is [`Notification`]-kind-specific (see each variant's
/// own fingerprint rule in [`Notification::key`]) — a new fingerprint
/// means a new occurrence, which shows again even if the same kind was
/// dismissed before. Serialized (in `<base>/notifications.json`) as
/// `"<kind>:<fingerprint>"`; see `crate::ports::NotificationState`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NotificationKey {
    /// Which kind this occurrence belongs to.
    pub kind: NotificationKind,
    /// This occurrence's own identity within its kind.
    pub fingerprint: String,
}

/// One button a notice can offer. Rust never phrases the *label* — the
/// frontend renders each variant through its own i18n key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationAction {
    /// [`Notification::Welcome`]: keep the default network settings —
    /// calls `CompleteWelcome`.
    KeepNetworkSettings,
    /// [`Notification::Welcome`]: turn off `allow_network`, then call
    /// `CompleteWelcome`.
    TurnOffNetwork,
    /// [`Notification::Welcome`]: reset this profile's `Settings` to
    /// [`crate::Settings::default`] — shown only while they differ (see
    /// [`Notification::Welcome`]'s own `settings_match_recommended`
    /// field).
    ApplyRecommendedSettings,
    /// [`Notification::RuleDatabasesStale`]: refresh now (a manual
    /// [`crate::use_cases::RefreshRuleDatabases::execute`] call, which
    /// ignores every automatic-cadence gate — a click is its own
    /// consent).
    RefreshRuleDatabases,
    /// [`Notification::ImportedRulesOutdated`]: open the Databases
    /// card, where the existing Re-import button already lives.
    OpenRuleDatabases,
    /// Opens Settings → Network.
    OpenSettings,
    /// [`Notification::UpdateAvailable`]: opens the release's own page
    /// on GitHub.
    ShowReleasePage,
    /// [`Notification::GameVersionChanged`]: opens the compat-patches
    /// page, where the user can re-export patches/patch-maker mods that
    /// still declare the old version.
    OpenPatches,
    /// [`Notification::RecommendedSourcesIncomplete`]: turn on every
    /// recommended source, then run a manual refresh (the click is its
    /// own consent to the download).
    EnableRecommendedSources,
}

/// How a notice can be cleared. [`Self::Occurrence`] means "dismiss
/// only" (the ✕ button); [`Self::OccurrenceOrMute`] additionally offers
/// "Don't remind me again", which mutes the whole
/// [`NotificationKind`] until explicitly unmuted (`crate::use_cases::UnmuteNotificationKind`)
/// — see each [`Notification`] variant's own doc comment for which one
/// it returns from [`Notification::dismissal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismissal {
    /// Dismiss only — this exact occurrence goes away; the same
    /// [`NotificationKind`] shows again on its next, different
    /// fingerprint.
    Occurrence,
    /// Dismiss, or mute the whole kind until unmuted.
    OccurrenceOrMute,
}

/// One active notice, as [`crate::use_cases::ListNotifications`] builds
/// it. Every notice **can be dismissed** — anything that must never be
/// dismissible (the pending-active-set banner) stays outside this system
/// entirely (see `apps/desktop/CLAUDE.md`'s own persistent-`Message`
/// convention for that case).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    /// First launch, mainly about the network: shown until the user
    /// answers it, with the current policy's switches and (when this
    /// profile's settings differ from the defaults) a reset button.
    Welcome {
        /// The current app-global network policy, for the card's own
        /// three switches.
        network: NetworkPolicy,
        /// Whether this profile's `Settings` already match
        /// [`crate::Settings::default`] — the "Use recommended
        /// settings" button shows only when this is `false`.
        settings_match_recommended: bool,
    },
    /// This profile's own RimWorld version has changed since it last
    /// acknowledged one — exported compat patches and patch-maker mods
    /// still declare the old `major.minor` in `supportedVersions` until
    /// re-exported. Dismissing this notice **acknowledges** `current` (stored
    /// per-profile, via `crate::use_cases::AcknowledgeGameVersion` —
    /// never the app-global dismissed-fingerprint list every other kind
    /// uses).
    GameVersionChanged {
        /// The version this profile last acknowledged.
        acknowledged: GameMajorMinor,
        /// The version currently in use.
        current: GameMajorMinor,
    },
    /// A newer stable Rimmerge release than the one running.
    UpdateAvailable {
        /// The binary currently running.
        running: AppVersion,
        /// The newer published release.
        latest: LatestRelease,
    },
    /// One or more enabled rule-database sources have gone stale; the
    /// per-situation rules that put a source in `sources` are documented
    /// on `crate::notifications::evaluate`.
    RuleDatabasesStale {
        /// Every stale, enabled source's own detail.
        sources: BTreeMap<RuleDatabase, StaleSource>,
    },
    /// This profile has cached rule-database content it hasn't imported
    /// yet — the passive badge-only signal. Keyed by each source's
    /// own cached `sha256` (not merely presence), so [`Self::key`]'s
    /// fingerprint changes whenever the cached content itself changes,
    /// not just when the *set* of outdated sources changes — dismissing
    /// today's stale `community_rules` copy must not permanently hide
    /// tomorrow's differently-shaped one too.
    ImportedRulesOutdated {
        /// Every source with unreimported cached content, mapped to that
        /// content's own cached `sha256`.
        sources: BTreeMap<RuleDatabase, String>,
    },
    /// Not every recommended rule database is set up. Shown only after
    /// Welcome is answered and never while internet access is off; the
    /// per-source rules are documented on `crate::notifications::evaluate`.
    RecommendedSourcesIncomplete {
        /// Every recommended source that is off, or on but never
        /// downloaded and not covered by the automatic refresh.
        sources: BTreeMap<RuleDatabase, SourceSetup>,
    },
}

/// The manifest key -> stable text used in a
/// [`Notification::RuleDatabasesStale`]/[`Notification::ImportedRulesOutdated`]
/// fingerprint — deliberately not `RuleDatabase`'s `Debug` output, for
/// the identical reason `rim_io`'s own manifest `key_for` isn't: a
/// future `Debug` wording change must never silently change an
/// already-stored fingerprint (which would re-show every dismissed/muted
/// notice at once).
fn source_key(database: RuleDatabase) -> &'static str {
    match database {
        RuleDatabase::CommunityRules => "community_rules",
        RuleDatabase::SteamWorkshop => "steam_workshop",
        RuleDatabase::RimmergeRules => "rimmerge_rules",
    }
}

impl Notification {
    /// Which kind this notice is.
    #[must_use]
    pub fn kind(&self) -> NotificationKind {
        match self {
            Self::Welcome { .. } => NotificationKind::Welcome,
            Self::GameVersionChanged { .. } => NotificationKind::GameVersionChanged,
            Self::UpdateAvailable { .. } => NotificationKind::UpdateAvailable,
            Self::RuleDatabasesStale { .. } => NotificationKind::RuleDatabasesStale,
            Self::ImportedRulesOutdated { .. } => NotificationKind::ImportedRulesOutdated,
            Self::RecommendedSourcesIncomplete { .. } => {
                NotificationKind::RecommendedSourcesIncomplete
            }
        }
    }

    /// This notice's urgency.
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::Welcome { .. }
            | Self::ImportedRulesOutdated { .. }
            | Self::RecommendedSourcesIncomplete { .. } => Severity::Info,
            Self::UpdateAvailable { .. } => Severity::Info,
            Self::GameVersionChanged { .. } | Self::RuleDatabasesStale { .. } => Severity::Warn,
        }
    }

    /// This notice's own identity, for dismissal/mute bookkeeping. See
    /// each variant's own fingerprint rule below.
    #[must_use]
    pub fn key(&self) -> NotificationKey {
        let fingerprint = match self {
            // A constant fingerprint — `Welcome` is answered once,
            // not per-occurrence.
            Self::Welcome { .. } => "welcome".to_string(),
            // The current version. Dismissing acknowledges it (via
            // the per-profile store, not the fingerprint list this
            // method's own key feeds) — a later, different current
            // version is a new fingerprint and shows again.
            Self::GameVersionChanged { current, .. } => current.to_string(),
            // The latest version. Dismissing skips that version;
            // the next version is a new fingerprint and shows again.
            Self::UpdateAvailable { latest, .. } => latest.version.to_string(),
            // A sorted list of `source@last_success|never`
            // entries — `sources` is already sorted (`BTreeMap`).
            Self::RuleDatabasesStale { sources } => sources
                .iter()
                .map(|(database, stale)| {
                    let freshness = match stale.freshness {
                        Freshness::NeverFetched => "never".to_string(),
                        Freshness::FetchedAt(at) => at.to_string(),
                    };
                    format!("{}@{freshness}", source_key(*database))
                })
                .collect::<Vec<_>>()
                .join(";"),
            // A sorted list of `source@cached_sha12` entries —
            // shared with `crate::use_cases::ListNotifications`, which
            // builds this same fingerprint from a `RuleDatabaseView`
            // without constructing a throwaway `Notification` first.
            Self::ImportedRulesOutdated { sources } => imported_rules_outdated_fingerprint(sources),
            // A sorted list of `source@off|not_downloaded` entries:
            // dismissing hides exactly this set, and it reappears when
            // the set or a source state changes.
            Self::RecommendedSourcesIncomplete { sources } => sources
                .iter()
                .map(|(database, setup)| {
                    let setup = match setup {
                        SourceSetup::Off => "off",
                        SourceSetup::NotDownloaded => "not_downloaded",
                    };
                    format!("{}@{setup}", source_key(*database))
                })
                .collect::<Vec<_>>()
                .join(";"),
        };
        NotificationKey {
            kind: self.kind(),
            fingerprint,
        }
    }

    /// The buttons this notice offers, in display order.
    #[must_use]
    pub fn actions(&self) -> Vec<NotificationAction> {
        match self {
            Self::Welcome { .. } => vec![
                NotificationAction::KeepNetworkSettings,
                NotificationAction::TurnOffNetwork,
                NotificationAction::ApplyRecommendedSettings,
            ],
            Self::GameVersionChanged { .. } => vec![NotificationAction::OpenPatches],
            Self::UpdateAvailable { .. } => vec![
                NotificationAction::ShowReleasePage,
                NotificationAction::OpenSettings,
            ],
            Self::RuleDatabasesStale { .. } => vec![
                NotificationAction::RefreshRuleDatabases,
                NotificationAction::OpenSettings,
            ],
            Self::ImportedRulesOutdated { .. } => vec![NotificationAction::OpenRuleDatabases],
            Self::RecommendedSourcesIncomplete { .. } => vec![
                NotificationAction::EnableRecommendedSources,
                NotificationAction::OpenRuleDatabases,
            ],
        }
    }

    /// How this notice can be cleared. `Welcome`/`UpdateAvailable`
    /// dismiss only; the two rule-database notices can also be muted.
    #[must_use]
    pub fn dismissal(&self) -> Dismissal {
        match self {
            Self::Welcome { .. }
            | Self::GameVersionChanged { .. }
            | Self::UpdateAvailable { .. } => Dismissal::Occurrence,
            Self::RuleDatabasesStale { .. }
            | Self::ImportedRulesOutdated { .. }
            | Self::RecommendedSourcesIncomplete { .. } => Dismissal::OccurrenceOrMute,
        }
    }
}

/// [`Notification::ImportedRulesOutdated`]'s own fingerprint rule —
/// `source@cached_sha12`, one entry per source, sorted (the map is
/// already sorted, being a `BTreeMap`). [`Notification::key`] calls this
/// directly; it's a free function rather than inlined there purely so it
/// has its own name to test against in isolation (`imported_rules_outdated_fingerprint_truncates_sha_to_12_and_is_sorted`,
/// below).
#[must_use]
pub fn imported_rules_outdated_fingerprint(entries: &BTreeMap<RuleDatabase, String>) -> String {
    let mut out = String::new();
    for (index, (database, sha256)) in entries.iter().enumerate() {
        if index > 0 {
            out.push(';');
        }
        let sha12: String = sha256.chars().take(12).collect();
        let _ = write!(out, "{}@{sha12}", source_key(*database));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(version: &str) -> LatestRelease {
        LatestRelease {
            version: AppVersion::published(version).expect("valid version"),
            published_at: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn welcome_is_dismiss_only_with_a_constant_fingerprint() {
        let notice = Notification::Welcome {
            network: NetworkPolicy::default(),
            settings_match_recommended: true,
        };
        assert_eq!(notice.kind(), NotificationKind::Welcome);
        assert_eq!(notice.dismissal(), Dismissal::Occurrence);
        assert_eq!(notice.key().fingerprint, "welcome");
    }

    #[test]
    fn game_version_changed_fingerprints_by_current_and_is_dismiss_only() {
        let notice = Notification::GameVersionChanged {
            acknowledged: GameMajorMinor::parse("1.6"),
            current: GameMajorMinor::parse("1.7"),
        };
        assert_eq!(notice.kind(), NotificationKind::GameVersionChanged);
        assert_eq!(notice.key().fingerprint, "1.7");
        assert_eq!(notice.dismissal(), Dismissal::Occurrence);
        assert_eq!(notice.severity(), Severity::Warn);
        assert_eq!(notice.actions(), vec![NotificationAction::OpenPatches]);
    }

    #[test]
    fn update_available_fingerprints_by_version_and_is_dismiss_only() {
        let notice = Notification::UpdateAvailable {
            running: AppVersion::running("0.1.0").expect("valid version"),
            latest: latest("0.2.0"),
        };
        assert_eq!(notice.key().fingerprint, "0.2.0");
        assert_eq!(notice.dismissal(), Dismissal::Occurrence);
        assert_eq!(notice.severity(), Severity::Info);
    }

    #[test]
    fn a_different_latest_version_is_a_different_occurrence() {
        let older = Notification::UpdateAvailable {
            running: AppVersion::running("0.1.0").expect("valid version"),
            latest: latest("0.2.0"),
        };
        let newer = Notification::UpdateAvailable {
            running: AppVersion::running("0.1.0").expect("valid version"),
            latest: latest("0.3.0"),
        };
        assert_ne!(older.key(), newer.key());
    }

    #[test]
    fn rule_databases_stale_can_be_muted_and_is_warn() {
        let notice = Notification::RuleDatabasesStale {
            sources: BTreeMap::new(),
        };
        assert_eq!(notice.dismissal(), Dismissal::OccurrenceOrMute);
        assert_eq!(notice.severity(), Severity::Warn);
        assert_eq!(
            notice.actions(),
            vec![
                NotificationAction::RefreshRuleDatabases,
                NotificationAction::OpenSettings
            ]
        );
    }

    #[test]
    fn rule_databases_stale_fingerprint_is_sorted_and_stable() {
        let notice = Notification::RuleDatabasesStale {
            sources: BTreeMap::from([
                (
                    RuleDatabase::SteamWorkshop,
                    StaleSource {
                        freshness: Freshness::NeverFetched,
                        refresh: RefreshMode::Manual,
                        last_failure: None,
                    },
                ),
                (
                    RuleDatabase::CommunityRules,
                    StaleSource {
                        freshness: Freshness::FetchedAt(jiff::Timestamp::UNIX_EPOCH),
                        refresh: RefreshMode::Automatic,
                        last_failure: None,
                    },
                ),
            ]),
        };
        assert_eq!(
            notice.key().fingerprint,
            format!(
                "community_rules@{};steam_workshop@never",
                jiff::Timestamp::UNIX_EPOCH
            )
        );
    }

    #[test]
    fn imported_rules_outdated_can_be_muted_and_its_only_action_is_open_databases() {
        let notice = Notification::ImportedRulesOutdated {
            sources: BTreeMap::from([(RuleDatabase::CommunityRules, "abc123".to_string())]),
        };
        assert_eq!(notice.dismissal(), Dismissal::OccurrenceOrMute);
        assert_eq!(
            notice.actions(),
            vec![NotificationAction::OpenRuleDatabases]
        );
    }

    #[test]
    fn imported_rules_outdated_fingerprint_changes_when_the_cached_content_changes() {
        // Regression: a fingerprint built only from the set of stale
        // source names would let one dismissal hide the notice forever,
        // even after the cached content itself changed.
        let before = Notification::ImportedRulesOutdated {
            sources: BTreeMap::from([(RuleDatabase::CommunityRules, "aaaaaaaaaaaa".to_string())]),
        };
        let after = Notification::ImportedRulesOutdated {
            sources: BTreeMap::from([(RuleDatabase::CommunityRules, "bbbbbbbbbbbb".to_string())]),
        };
        assert_ne!(before.key(), after.key());
    }

    #[test]
    fn imported_rules_outdated_fingerprint_truncates_sha_to_12_and_is_sorted() {
        let entries = BTreeMap::from([
            (
                RuleDatabase::SteamWorkshop,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            ),
            (
                RuleDatabase::CommunityRules,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ),
        ]);
        assert_eq!(
            imported_rules_outdated_fingerprint(&entries),
            "community_rules@aaaaaaaaaaaa;steam_workshop@bbbbbbbbbbbb"
        );
    }

    #[test]
    fn recommended_sources_incomplete_is_info_mutable_with_two_actions() {
        let notice = Notification::RecommendedSourcesIncomplete {
            sources: BTreeMap::from([(RuleDatabase::SteamWorkshop, SourceSetup::NotDownloaded)]),
        };
        assert_eq!(
            notice.kind(),
            NotificationKind::RecommendedSourcesIncomplete
        );
        assert_eq!(notice.severity(), Severity::Info);
        assert_eq!(notice.dismissal(), Dismissal::OccurrenceOrMute);
        assert_eq!(
            notice.actions(),
            vec![
                NotificationAction::EnableRecommendedSources,
                NotificationAction::OpenRuleDatabases
            ]
        );
    }

    #[test]
    fn recommended_sources_incomplete_fingerprint_is_sorted_and_tracks_each_state() {
        let both = Notification::RecommendedSourcesIncomplete {
            sources: BTreeMap::from([
                (RuleDatabase::SteamWorkshop, SourceSetup::NotDownloaded),
                (RuleDatabase::CommunityRules, SourceSetup::Off),
            ]),
        };
        assert_eq!(
            both.key().fingerprint,
            "community_rules@off;steam_workshop@not_downloaded"
        );

        let steam_off = Notification::RecommendedSourcesIncomplete {
            sources: BTreeMap::from([(RuleDatabase::SteamWorkshop, SourceSetup::Off)]),
        };
        let steam_pending = Notification::RecommendedSourcesIncomplete {
            sources: BTreeMap::from([(RuleDatabase::SteamWorkshop, SourceSetup::NotDownloaded)]),
        };
        assert_ne!(
            steam_off.key(),
            steam_pending.key(),
            "turning a source on but not downloading it is a new occurrence"
        );
    }

    #[test]
    fn notification_kind_declaration_order_is_display_priority() {
        assert!(NotificationKind::Welcome < NotificationKind::GameVersionChanged);
        assert!(NotificationKind::GameVersionChanged < NotificationKind::UpdateAvailable);
        assert!(NotificationKind::UpdateAvailable < NotificationKind::RuleDatabasesStale);
        assert!(NotificationKind::RuleDatabasesStale < NotificationKind::ImportedRulesOutdated);
        assert!(
            NotificationKind::ImportedRulesOutdated
                < NotificationKind::RecommendedSourcesIncomplete
        );
    }
}
