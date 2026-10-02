//! The Dashboard's "Get the recommended rules" step, derived as one pure
//! function so the strip's status command and the notification evaluator
//! read the same answer. No ports, no IO: callers gather the facts.

use std::collections::BTreeMap;

use crate::app_settings::NetworkPolicy;
use crate::ports::{FetchFailure, RuleDatabase};
use crate::settings::Settings;
use crate::use_cases::RuleDatabaseView;

/// What the "Get the recommended rules" step shows. Closed; each variant
/// carries only its own fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendedRulesStep {
    /// Every recommended importable source has been imported into this
    /// profile at least once. "Imported once", not "the current cache is
    /// imported": a newer cache is `ImportedRulesOutdated`'s job.
    Done {
        /// Whether the profile's settings let imported rules reach the
        /// sort (`use_imported_pairs` or `use_imported_placements`).
        imported_rules_in_use: bool,
    },
    /// The user skipped the step for this profile and it is not done. Carries
    /// what "Get them now" would do, so the row discloses it like
    /// `NeedsAction` does. Never empty.
    Skipped {
        /// The sources still missing an import, with what each needs.
        sources: BTreeMap<RuleDatabase, SourceNeed>,
    },
    /// Something needs the network and a gate forbids it right now.
    Unavailable(Unavailable),
    /// What a click would do, per source. Never empty.
    NeedsAction {
        /// The sources still missing an import, with what each needs.
        sources: BTreeMap<RuleDatabase, SourceNeed>,
    },
}

/// Which network gate is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// `allow_network` is off (a damaged settings file loads with every
    /// switch off, so it reads the same).
    NetworkOff,
    /// The first-run notice has not been answered.
    AwaitingFirstRun,
}

/// One recommended, importable source this profile has never imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceNeed {
    /// Its fetch toggle is off: the click turns it on, then downloads it.
    TurnOn,
    /// On, nothing cached. `last_failure` is set when the last attempt
    /// failed.
    Download {
        /// Why the last download attempt failed, if it did.
        last_failure: Option<FetchFailure>,
    },
    /// Cached, never imported into this profile. Needs no network.
    Import,
}

impl SourceNeed {
    /// Whether satisfying this need contacts the network.
    #[must_use]
    pub fn needs_network(&self) -> bool {
        match self {
            Self::TurnOn | Self::Download { .. } => true,
            Self::Import => false,
        }
    }
}

/// Whether the first-run notice has been answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRun {
    /// Answered (`welcome_completed_at` is set).
    Answered,
    /// Not answered yet.
    Pending,
}

/// Whether the user skipped the step for this profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepSkip {
    /// The profile recorded a skip.
    Skipped,
    /// No skip recorded.
    NotSkipped,
}

impl From<Option<jiff::Timestamp>> for StepSkip {
    fn from(skipped_at: Option<jiff::Timestamp>) -> Self {
        match skipped_at {
            Some(_) => Self::Skipped,
            None => Self::NotSkipped,
        }
    }
}

/// Everything [`recommended_rules_step`] reads.
#[derive(Debug, Clone, Copy)]
pub struct RecommendedRulesFacts<'a> {
    /// The app-global network policy.
    pub policy: &'a NetworkPolicy,
    /// Per-source cache and import state (`RefreshRuleDatabases::status`).
    pub databases: &'a [RuleDatabaseView],
    /// Whether the first-run notice was answered.
    pub first_run: FirstRun,
    /// Whether this profile skipped the step.
    pub skip: StepSkip,
    /// The profile's settings; only the two imported-rules toggles are read.
    pub settings: &'a Settings,
}

/// Derives the step. In order, first match wins: Done, Skipped, then the
/// per-source needs gated by the network switch and the first-run notice
/// (only when some need contacts the network).
#[must_use]
pub fn recommended_rules_step(facts: &RecommendedRulesFacts<'_>) -> RecommendedRulesStep {
    let sources = needs(facts);
    if sources.is_empty() {
        return RecommendedRulesStep::Done {
            imported_rules_in_use: facts.settings.use_imported_pairs
                || facts.settings.use_imported_placements,
        };
    }
    match facts.skip {
        StepSkip::Skipped => return RecommendedRulesStep::Skipped { sources },
        StepSkip::NotSkipped => {}
    }
    if sources.values().any(SourceNeed::needs_network) {
        if !facts.policy.allow_network {
            return RecommendedRulesStep::Unavailable(Unavailable::NetworkOff);
        }
        match facts.first_run {
            FirstRun::Pending => {
                return RecommendedRulesStep::Unavailable(Unavailable::AwaitingFirstRun);
            }
            FirstRun::Answered => {}
        }
    }
    RecommendedRulesStep::NeedsAction { sources }
}

/// The considered sources (recommended and importable, walked from the
/// closed enum) that have no import record, each with what it needs. A
/// source the caller supplied no view for reads as never fetched: not
/// imported, not cached, no failure.
fn needs(facts: &RecommendedRulesFacts<'_>) -> BTreeMap<RuleDatabase, SourceNeed> {
    RuleDatabase::ALL
        .into_iter()
        .filter(|database| database.is_recommended() && database.is_importable())
        .filter_map(|database| {
            let view = facts
                .databases
                .iter()
                .find(|view| view.status.database == database);
            if view.is_some_and(|view| view.imported_sha256.is_some()) {
                return None;
            }
            let need = if !facts.policy.fetches(database) {
                SourceNeed::TurnOn
            } else if let Some(view) = view
                && view.status.cached.is_some()
            {
                SourceNeed::Import
            } else {
                SourceNeed::Download {
                    last_failure: view.and_then(|view| view.status.last_failure.clone()),
                }
            };
            Some((database, need))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ports::{CachedDatabase, DatabaseStatus};

    #[derive(Clone, Copy)]
    enum Cache {
        Empty,
        Present,
    }

    #[derive(Clone, Copy)]
    enum Import {
        Never,
        Once,
    }

    fn view(database: RuleDatabase, cache: Cache, import: Import) -> RuleDatabaseView {
        let cached = match cache {
            Cache::Empty => None,
            Cache::Present => Some(CachedDatabase {
                sha256: "newer".to_string(),
                bytes: 10,
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            }),
        };
        let imported_sha256 = match import {
            Import::Never => None,
            Import::Once => Some("older".to_string()),
        };
        RuleDatabaseView {
            status: DatabaseStatus {
                database,
                enabled: true,
                path: PathBuf::from("cache"),
                cached,
                last_failure: None,
                last_attempt_at: None,
            },
            imported_sha256,
            needs_reimport: false,
        }
    }

    /// One view per source, in `RuleDatabase::ALL` order (community,
    /// Steam, rimmerge-rules).
    fn all_views(cache: Cache, import: Import) -> Vec<RuleDatabaseView> {
        RuleDatabase::ALL
            .into_iter()
            .map(|database| view(database, cache, import))
            .collect()
    }

    fn step(
        policy: &NetworkPolicy,
        databases: &[RuleDatabaseView],
        first_run: FirstRun,
        skip: StepSkip,
    ) -> RecommendedRulesStep {
        recommended_rules_step(&RecommendedRulesFacts {
            policy,
            databases,
            first_run,
            skip,
            settings: &Settings::default(),
        })
    }

    fn network_off() -> NetworkPolicy {
        NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        }
    }

    #[test]
    fn done_when_every_recommended_importable_source_has_an_import_record() {
        let views = all_views(Cache::Present, Import::Once);

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::Done {
                imported_rules_in_use: true
            }
        );
    }

    #[test]
    fn a_source_without_an_import_record_keeps_the_step_open() {
        let mut views = all_views(Cache::Present, Import::Once);
        views[1].imported_sha256 = None;

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::NeedsAction {
                sources: BTreeMap::from([(RuleDatabase::SteamWorkshop, SourceNeed::Import)])
            }
        );
    }

    #[test]
    fn done_outranks_skipped_and_network_off() {
        let views = all_views(Cache::Present, Import::Once);

        let result = step(&network_off(), &views, FirstRun::Pending, StepSkip::Skipped);

        assert!(matches!(result, RecommendedRulesStep::Done { .. }));
    }

    #[test]
    fn done_reports_imported_rules_not_in_use_when_both_import_toggles_are_off() {
        let views = all_views(Cache::Present, Import::Once);
        let settings = Settings {
            use_imported_pairs: false,
            use_imported_placements: false,
            ..Settings::default()
        };

        let result = recommended_rules_step(&RecommendedRulesFacts {
            policy: &NetworkPolicy::default(),
            databases: &views,
            first_run: FirstRun::Answered,
            skip: StepSkip::NotSkipped,
            settings: &settings,
        });

        assert_eq!(
            result,
            RecommendedRulesStep::Done {
                imported_rules_in_use: false
            }
        );
    }

    #[test]
    fn a_newer_cache_after_an_import_still_reads_done() {
        let mut views = all_views(Cache::Present, Import::Once);
        for each in &mut views {
            each.needs_reimport = true;
        }

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        );

        assert!(matches!(result, RecommendedRulesStep::Done { .. }));
    }

    #[test]
    fn skipped_when_not_done_and_the_profile_skipped() {
        let views = all_views(Cache::Empty, Import::Never);

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::Skipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::Skipped {
                sources: BTreeMap::from([
                    (
                        RuleDatabase::CommunityRules,
                        SourceNeed::Download { last_failure: None }
                    ),
                    (
                        RuleDatabase::SteamWorkshop,
                        SourceNeed::Download { last_failure: None }
                    ),
                ])
            }
        );
    }

    #[test]
    fn a_disabled_source_needs_turn_on() {
        let views = all_views(Cache::Present, Import::Never);
        let policy = NetworkPolicy {
            fetch_steam_workshop: false,
            ..NetworkPolicy::default()
        };

        let RecommendedRulesStep::NeedsAction { sources } =
            step(&policy, &views, FirstRun::Answered, StepSkip::NotSkipped)
        else {
            panic!("expected NeedsAction");
        };

        assert_eq!(
            sources.get(&RuleDatabase::SteamWorkshop),
            Some(&SourceNeed::TurnOn)
        );
        assert_eq!(
            sources.get(&RuleDatabase::CommunityRules),
            Some(&SourceNeed::Import)
        );
    }

    #[test]
    fn an_enabled_uncached_source_needs_download_with_its_last_failure() {
        let mut views = all_views(Cache::Empty, Import::Never);
        let failure = FetchFailure::unclassified("offline");
        views[0].status.last_failure = Some(failure.clone());

        let RecommendedRulesStep::NeedsAction { sources } = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        ) else {
            panic!("expected NeedsAction");
        };

        assert_eq!(
            sources.get(&RuleDatabase::CommunityRules),
            Some(&SourceNeed::Download {
                last_failure: Some(failure)
            })
        );
        assert_eq!(
            sources.get(&RuleDatabase::SteamWorkshop),
            Some(&SourceNeed::Download { last_failure: None })
        );
    }

    #[test]
    fn a_cached_never_imported_source_needs_import() {
        let views = all_views(Cache::Present, Import::Never);

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::NeedsAction {
                sources: BTreeMap::from([
                    (RuleDatabase::CommunityRules, SourceNeed::Import),
                    (RuleDatabase::SteamWorkshop, SourceNeed::Import),
                ])
            }
        );
    }

    #[test]
    fn network_off_is_unavailable_only_when_a_need_requires_the_network() {
        let uncached = all_views(Cache::Empty, Import::Never);
        let mut mixed = all_views(Cache::Present, Import::Never);
        mixed[1].status.cached = None;
        let import_only = all_views(Cache::Present, Import::Never);

        for views in [&uncached, &mixed] {
            assert_eq!(
                step(
                    &network_off(),
                    views,
                    FirstRun::Answered,
                    StepSkip::NotSkipped
                ),
                RecommendedRulesStep::Unavailable(Unavailable::NetworkOff)
            );
        }
        assert!(matches!(
            step(
                &network_off(),
                &import_only,
                FirstRun::Answered,
                StepSkip::NotSkipped
            ),
            RecommendedRulesStep::NeedsAction { .. }
        ));
    }

    #[test]
    fn an_import_only_step_is_actionable_offline_and_before_first_run() {
        let views = all_views(Cache::Present, Import::Never);

        let result = step(
            &network_off(),
            &views,
            FirstRun::Pending,
            StepSkip::NotSkipped,
        );

        assert!(matches!(result, RecommendedRulesStep::NeedsAction { .. }));
    }

    #[test]
    fn awaiting_first_run_when_a_download_is_needed_and_welcome_is_unanswered() {
        let views = all_views(Cache::Empty, Import::Never);

        let result = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Pending,
            StepSkip::NotSkipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::Unavailable(Unavailable::AwaitingFirstRun)
        );
    }

    #[test]
    fn network_off_outranks_awaiting_first_run() {
        let views = all_views(Cache::Empty, Import::Never);

        let result = step(
            &network_off(),
            &views,
            FirstRun::Pending,
            StepSkip::NotSkipped,
        );

        assert_eq!(
            result,
            RecommendedRulesStep::Unavailable(Unavailable::NetworkOff)
        );
    }

    #[test]
    fn rimmerge_rules_is_never_a_considered_source() {
        let mut views = all_views(Cache::Present, Import::Once);
        views[2].imported_sha256 = None;
        views[2].status.cached = None;
        let policy = NetworkPolicy {
            fetch_rimmerge_rules: false,
            ..NetworkPolicy::default()
        };

        let result = step(&policy, &views, FirstRun::Answered, StepSkip::NotSkipped);

        assert!(matches!(result, RecommendedRulesStep::Done { .. }));
    }

    #[test]
    fn needs_action_lists_only_the_importable_sources() {
        // Views supplied in reverse order, rimmerge-rules included: only
        // the importable sources appear, in `RuleDatabase` order.
        let mut views = all_views(Cache::Empty, Import::Never);
        views.reverse();

        let RecommendedRulesStep::NeedsAction { sources } = step(
            &NetworkPolicy::default(),
            &views,
            FirstRun::Answered,
            StepSkip::NotSkipped,
        ) else {
            panic!("expected NeedsAction");
        };

        assert_eq!(
            sources.keys().copied().collect::<Vec<_>>(),
            vec![RuleDatabase::CommunityRules, RuleDatabase::SteamWorkshop]
        );
    }

    #[test]
    fn an_empty_view_slice_is_not_done() {
        let policy = NetworkPolicy {
            fetch_steam_workshop: false,
            ..NetworkPolicy::default()
        };

        let result = step(&policy, &[], FirstRun::Answered, StepSkip::NotSkipped);

        assert_eq!(
            result,
            RecommendedRulesStep::NeedsAction {
                sources: BTreeMap::from([
                    (
                        RuleDatabase::CommunityRules,
                        SourceNeed::Download { last_failure: None }
                    ),
                    (RuleDatabase::SteamWorkshop, SourceNeed::TurnOn),
                ])
            }
        );
    }

    #[test]
    fn done_reports_imported_rules_in_use_when_only_one_import_toggle_is_on() {
        let views = all_views(Cache::Present, Import::Once);
        for (use_imported_pairs, use_imported_placements) in [(false, true), (true, false)] {
            let settings = Settings {
                use_imported_pairs,
                use_imported_placements,
                ..Settings::default()
            };

            let result = recommended_rules_step(&RecommendedRulesFacts {
                policy: &NetworkPolicy::default(),
                databases: &views,
                first_run: FirstRun::Answered,
                skip: StepSkip::NotSkipped,
                settings: &settings,
            });

            assert_eq!(
                result,
                RecommendedRulesStep::Done {
                    imported_rules_in_use: true
                },
                "pairs={use_imported_pairs} placements={use_imported_placements}"
            );
        }
    }

    #[test]
    fn a_disabled_uncached_source_needs_turn_on_not_download() {
        let views = all_views(Cache::Empty, Import::Never);
        let policy = NetworkPolicy {
            fetch_steam_workshop: false,
            ..NetworkPolicy::default()
        };

        let RecommendedRulesStep::NeedsAction { sources } =
            step(&policy, &views, FirstRun::Answered, StepSkip::NotSkipped)
        else {
            panic!("expected NeedsAction");
        };

        assert_eq!(
            sources.get(&RuleDatabase::SteamWorkshop),
            Some(&SourceNeed::TurnOn)
        );
    }

    #[test]
    fn step_skip_reads_a_recorded_timestamp_as_skipped() {
        assert_eq!(StepSkip::from(None), StepSkip::NotSkipped);
        assert_eq!(
            StepSkip::from(Some(jiff::Timestamp::UNIX_EPOCH)),
            StepSkip::Skipped
        );
    }
}
