//! [`AppSettings`]: app-global preferences — one per machine (per
//! `<base>`), never per profile. Persisted by `rim_io`'s
//! `JsonAppSettingsStore` at `<base>/app-settings.json`, a sibling of
//! `config.json`, `profiles/`, and `databases/`.
//!
//! **Why app-global, not part of the per-profile [`crate::Settings`]:**
//! the master network switch, the update check, and automatic
//! rule-database refresh all act before or outside any one profile's own
//! scope (a first-run notice shown once per machine, a background check
//! at launch before a profile has even finished loading) — see
//! `docs/privacy-and-network.md`.

use crate::ports::RuleDatabase;

/// App-global preferences: one per machine (per `<base>`), not per
/// install/profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppSettings {
    /// Network access and the automatic-refresh feature toggles.
    pub network: NetworkPolicy,
    /// Staleness-reminder thresholds.
    pub reminders: ReminderPolicy,
}

/// Everything that gates a network request — the master switch, the
/// per-feature automatic toggles, and the three per-source fetch toggles
/// (moved here from [`crate::Settings`], which they never should have
/// been part of: two installs opening the same machine's `<base>` must
/// never disagree about whether GitHub gets contacted). Every field here
/// is read before a URL is ever built — see `rim_io::net::allowlist` and
/// `docs/privacy-and-network.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkPolicy {
    /// The master switch. `false` = no request of any kind, automatic or
    /// manual, for any feature below. Default `true`, but nothing
    /// automatic ever runs until the first-run notice has been answered
    /// — see `crate::use_cases::RunLaunchNetworkChecks`.
    pub allow_network: bool,
    /// Automatic, once-per-launch check for a newer
    /// Rimmerge release (`api.github.com`). Default `true`. "Check now"
    /// ignores this toggle — a click is its own consent.
    pub check_for_updates: bool,
    /// Automatic, at-most-once-a-day refresh of every enabled,
    /// auto-refresh-eligible rule database (community rules,
    /// rimmerge-rules — never the Steam Workshop database, see
    /// `crate::ports::RuleDatabase::is_auto_refresh_eligible`). Default
    /// `true`. A manual "Refresh" click ignores this toggle.
    pub auto_refresh_rule_databases: bool,
    /// Whether a refresh (automatic or manual) is allowed to fetch
    /// `communityRules.json` at all. Default `true` — the file is small
    /// (about 394 KB). A source this settles as disabled is never
    /// requested: `RefreshRuleDatabases` reports
    /// `RefreshOutcome::Skipped { SourceDisabled }` for it and never
    /// calls the `RuleDatabaseFetcher` port.
    pub fetch_community_rules: bool,
    /// Whether a refresh is allowed to fetch `steamDB.json` — up to
    /// ~49 MB. Default `true` (the source is recommended), but **never
    /// auto-refreshed even when enabled**: a download that size must
    /// never happen without the user asking
    /// (`RuleDatabase::is_auto_refresh_eligible` is `false` for this
    /// source unconditionally).
    pub fetch_steam_workshop: bool,
    /// Whether a refresh is allowed to fetch this project's own
    /// `rimmerge-rules.json`. Default `true`: it is small, it is ours,
    /// and a stale copy is the difference between `verify` modelling a
    /// framework's own patch operations and reporting them
    /// `Unsupported`. Turning it off leaves the vendored defaults
    /// compiled into this binary in effect — never nothing.
    pub fetch_rimmerge_rules: bool,
}

impl NetworkPolicy {
    /// Whether a refresh may fetch `database` — that source's own fetch
    /// toggle. The one mapping from a source to its toggle: every caller
    /// asks here instead of matching on the source itself. Does not
    /// consult [`Self::allow_network`]; the master switch is checked
    /// separately by each caller.
    #[must_use]
    pub fn fetches(&self, database: RuleDatabase) -> bool {
        match database {
            RuleDatabase::CommunityRules => self.fetch_community_rules,
            RuleDatabase::SteamWorkshop => self.fetch_steam_workshop,
            RuleDatabase::RimmergeRules => self.fetch_rimmerge_rules,
        }
    }
}

impl NetworkPolicy {
    /// This policy with every [`RuleDatabase::is_recommended`] source's
    /// fetch toggle on. Never touches `allow_network`,
    /// `check_for_updates`, or `auto_refresh_rule_databases`: turning
    /// sources on is not a decision about internet access itself.
    #[must_use]
    pub fn with_recommended_sources(self) -> Self {
        RuleDatabase::ALL
            .into_iter()
            .filter(|database| database.is_recommended())
            .fold(self, |policy, database| policy.with_source(database, true))
    }

    fn with_source(mut self, database: RuleDatabase, is_enabled: bool) -> Self {
        match database {
            RuleDatabase::CommunityRules => self.fetch_community_rules = is_enabled,
            RuleDatabase::SteamWorkshop => self.fetch_steam_workshop = is_enabled,
            RuleDatabase::RimmergeRules => self.fetch_rimmerge_rules = is_enabled,
        }
        self
    }
}

impl Default for NetworkPolicy {
    /// Every automatic feature on, and each source's fetch toggle set
    /// from [`RuleDatabase::is_recommended`], so "default" and
    /// "recommended" cannot drift apart. The Steam Workshop database is
    /// therefore on, yet still never downloaded automatically
    /// ([`RuleDatabase::is_auto_refresh_eligible`]). See each field's own
    /// doc comment.
    fn default() -> Self {
        Self {
            allow_network: true,
            check_for_updates: true,
            auto_refresh_rule_databases: true,
            fetch_community_rules: RuleDatabase::CommunityRules.is_recommended(),
            fetch_steam_workshop: RuleDatabase::SteamWorkshop.is_recommended(),
            fetch_rimmerge_rules: RuleDatabase::RimmergeRules.is_recommended(),
        }
    }
}

/// How many days a cached rule database may go without a successful
/// refresh before the "hasn't refreshed in a while" reminder fires.
/// Bounded `1..=365` so neither "never" nor "every launch" can be
/// configured — see [`Self::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StaleAfterDays(u16);

/// [`StaleAfterDays::new`]'s only failure: the given value falls outside
/// `1..=365`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("stale-after-days must be between 1 and 365, got {0}")]
pub struct StaleAfterDaysError(pub u16);

impl StaleAfterDays {
    /// The lower bound (inclusive).
    pub const MIN: u16 = 1;
    /// The upper bound (inclusive).
    pub const MAX: u16 = 365;

    /// Validates `days` against `1..=365`.
    ///
    /// # Errors
    ///
    /// Returns [`StaleAfterDaysError`] when `days` is `0` or over 365.
    pub fn new(days: u16) -> Result<Self, StaleAfterDaysError> {
        if (Self::MIN..=Self::MAX).contains(&days) {
            Ok(Self(days))
        } else {
            Err(StaleAfterDaysError(days))
        }
    }

    /// The wrapped day count.
    #[must_use]
    pub fn get(self) -> u16 {
        self.0
    }
}

impl Default for StaleAfterDays {
    /// 30 days — the same threshold `DatabaseStatus::is_stale` used as a
    /// hardcoded constant before this became a setting.
    fn default() -> Self {
        Self(30)
    }
}

/// Thresholds governing which passive reminders fire. Currently just the
/// one, but kept as its own struct (rather than a bare field on
/// [`AppSettings`]) so a second threshold never has to flatten into
/// [`NetworkPolicy`] or force a breaking shape change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReminderPolicy {
    /// See [`StaleAfterDays`].
    pub rule_databases_stale_after_days: StaleAfterDays,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_settings_default_matches_network_and_reminder_defaults() {
        let settings = AppSettings::default();
        assert_eq!(settings.network, NetworkPolicy::default());
        assert_eq!(settings.reminders, ReminderPolicy::default());
    }

    #[test]
    fn network_policy_defaults_are_all_on() {
        let policy = NetworkPolicy::default();
        assert!(policy.allow_network);
        assert!(policy.check_for_updates);
        assert!(policy.auto_refresh_rule_databases);
        assert!(policy.fetch_community_rules);
        assert!(policy.fetch_steam_workshop);
        assert!(policy.fetch_rimmerge_rules);
    }

    #[test]
    fn default_fetches_exactly_the_recommended_sources() {
        let policy = NetworkPolicy::default();
        for database in RuleDatabase::ALL {
            assert_eq!(policy.fetches(database), database.is_recommended());
        }
    }

    #[test]
    fn fetches_reads_each_sources_own_toggle() {
        let policy = NetworkPolicy {
            fetch_community_rules: true,
            fetch_steam_workshop: false,
            fetch_rimmerge_rules: false,
            ..NetworkPolicy::default()
        };
        assert!(policy.fetches(RuleDatabase::CommunityRules));
        assert!(!policy.fetches(RuleDatabase::SteamWorkshop));
        assert!(!policy.fetches(RuleDatabase::RimmergeRules));
    }

    #[test]
    fn with_recommended_sources_turns_sources_on_and_touches_nothing_else() {
        let off = NetworkPolicy {
            allow_network: false,
            check_for_updates: false,
            auto_refresh_rule_databases: false,
            fetch_community_rules: false,
            fetch_steam_workshop: false,
            fetch_rimmerge_rules: false,
        };

        let result = off.with_recommended_sources();

        for database in RuleDatabase::ALL {
            assert_eq!(result.fetches(database), database.is_recommended());
        }
        assert!(!result.allow_network);
        assert!(!result.check_for_updates);
        assert!(!result.auto_refresh_rule_databases);
    }

    #[test]
    fn steam_is_recommended_yet_never_auto_refreshed() {
        assert!(RuleDatabase::SteamWorkshop.is_recommended());
        assert!(!RuleDatabase::SteamWorkshop.is_auto_refresh_eligible());
    }

    #[test]
    fn stale_after_days_rejects_zero_and_366_but_accepts_the_bounds() {
        assert!(StaleAfterDays::new(0).is_err());
        assert!(StaleAfterDays::new(366).is_err());
        assert_eq!(StaleAfterDays::new(1).unwrap().get(), 1);
        assert_eq!(StaleAfterDays::new(365).unwrap().get(), 365);
    }

    #[test]
    fn stale_after_days_defaults_to_30() {
        assert_eq!(StaleAfterDays::default().get(), 30);
    }

    #[test]
    fn reminder_policy_default_uses_the_default_stale_after_days() {
        assert_eq!(
            ReminderPolicy::default().rule_databases_stale_after_days,
            StaleAfterDays::default()
        );
    }
}
