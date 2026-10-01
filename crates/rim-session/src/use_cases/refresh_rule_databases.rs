//! [`RefreshRuleDatabases`]: "fetch rule databases into the global
//! cache" — [`RefreshRuleDatabases::execute`] for a manual, user-
//! requested refresh of any subset of the three sources, and
//! [`RefreshRuleDatabases::execute_automatic`] for the once-a-day
//! background refresh of the two auto-refresh-eligible ones
//! (`RuleDatabase::CommunityRules`/`RimmergeRules`; Steam Workshop is
//! manual-only). The one use case in this crate that ever touches
//! [`crate::ports::RuleDatabaseFetcher`].

use std::collections::BTreeMap;
use std::path::Path;

use crate::NetworkPolicy;
use crate::notifications::clock::seconds_since;
use crate::ports::{
    CachedDatabase, DatabaseStatus, FetchFailure, IMPORT_SOURCE_COMMUNITY_RULES,
    IMPORT_SOURCE_STEAM_DEPENDENCIES, ImportManifestStore, RefreshOutcome, RuleDatabase,
    RuleDatabaseFetcher, SkipReason,
};

/// One rule database's cache status, enriched with whether *this
/// profile's* own last import is stale against it — the fact both
/// `apps/cli`'s `db status` and the desktop's Databases card render as the
/// "Cached databases have changed since this profile last imported them —
/// Re-import" hint.
///
/// **Deliberately not a field on [`DatabaseStatus`] itself**: that type is
/// [`RuleDatabaseFetcher`]'s own port DTO, built from the cache manifest
/// alone — it has no profile in scope and cannot know what a profile last
/// imported. This struct is [`RefreshRuleDatabases::status`]'s own return
/// type, the one place both facts (the cache's current state,
/// [`crate::ports::ImportManifestStore`]'s own record of the profile's
/// last import) are already in hand together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDatabaseView {
    /// This source's own cache status — enabled, cached bytes (if any),
    /// last failure.
    pub status: DatabaseStatus,
    /// The sha256 this profile's own `<profile>/imports/manifest.json`
    /// last recorded for this source, if it has ever imported it.
    pub imported_sha256: Option<String>,
    /// Whether the cache holds bytes this profile has not imported, or has
    /// imported a different version of — see [`needs_reimport`] for the
    /// exact rule.
    pub needs_reimport: bool,
}

/// [`RuleDatabaseView::needs_reimport`]'s own rule, factored out so it is
/// unit-testable on its own shape without a real
/// [`DatabaseStatus`]/[`ImportManifestStore`] in hand:
///
/// - **Disabled -> always `false`, regardless of the other two
///   arguments**: [`crate::should_import_from_cache`] refuses to import a
///   disabled source at all, so a disabled-but-cached source would
///   otherwise print "needs reimport" forever, a signal no import could
///   ever act on or clear, since re-enabling the source is the only thing
///   that could ever make an import actually read it. Checked first,
///   before either `Option`, so this can never be shadowed by the shas
///   happening to differ.
/// - Enabled, cached present, no import record for this source -> `true`
///   (there are cached bytes this profile has never imported).
/// - Enabled, cached present, import record present, shas differ ->
///   `true`.
/// - Enabled, shas equal -> `false`.
/// - Enabled, no cached copy at all -> `false` (nothing to re-import
///   from).
///
/// **An import via `--rimsort-dir`/a local RimSort install clears this
/// exactly when it should, with no special-casing needed**: the import
/// manifest records the real sha256 of whatever bytes were actually
/// imported, regardless of source, so a local-clone import that happens
/// to match the cache's own current bytes (the common case — RimSort's
/// git clone and our HTTPS GET produce identical
/// bytes) clears the flag the same way a `--from-cache` import would; a
/// local clone that has genuinely diverged from the cache correctly
/// leaves it set, since that really is a difference between what's
/// cached and what this profile last imported.
#[must_use]
fn needs_reimport(
    enabled: bool,
    cached: Option<&CachedDatabase>,
    imported_sha256: Option<&str>,
) -> bool {
    if !enabled {
        return false;
    }
    match (cached, imported_sha256) {
        (Some(cached), Some(imported)) => cached.sha256 != imported,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// [`RuleDatabase`] -> [`crate::ports::ImportManifestStore`]'s own record
/// key. Policy, not presentation — defined once, here, beside
/// [`needs_reimport`], rather than in either interface: `apps/cli` and
/// `apps/desktop` both need it, and duplicating a `RuleDatabase -> &str`
/// match in two composition roots is exactly the kind of drift this
/// module's own [`IMPORT_SOURCE_COMMUNITY_RULES`]/[`IMPORT_SOURCE_STEAM_DEPENDENCIES`]
/// constants exist to prevent.
///
/// `None` for a database that is not imported into a profile at all:
/// [`RuleDatabase::RimmergeRules`]' four load-time sections are read
/// from the cache directly by [`crate::ports::ModKnowledgeStore`], and
/// its fifth (tag rules) has no importer yet, so it has no import record
/// to compare against and can never "need re-import".
fn import_source_key(database: RuleDatabase) -> Option<&'static str> {
    match database {
        RuleDatabase::CommunityRules => Some(IMPORT_SOURCE_COMMUNITY_RULES),
        RuleDatabase::SteamWorkshop => Some(IMPORT_SOURCE_STEAM_DEPENDENCIES),
        RuleDatabase::RimmergeRules => None,
    }
}

/// Fetches rule databases into the global cache, enforcing both network
/// policy toggles before ever calling the port.
///
/// **Takes `&NetworkPolicy`, not `&Session`.** Two reasons, both worth
/// keeping on record:
///
/// 1. `apps/cli`'s `db status`/`db refresh` must not pay for a
///    full mod scan just to read a cache manifest or fetch two files —
///    `NetworkPolicy` is app-global (loaded once through
///    `AppSettingsStore::load`, not per profile), and needs no scan or
///    even a profile directory to read.
/// 2. It makes the guarantee "a refresh touches only the global cache —
///    never the session, `rules.json`, or `<profile>/imports/`" true **by
///    signature**
///    rather than by inspection: this type cannot reach a [`crate::Session`]
///    even if some future edit wanted it to. A reviewer checking the
///    guarantee reads one `use` line, not this whole file.
///
/// **`status` also takes `profile_dir`, for the re-import badge** —
/// `Manifest: ImportManifestStore` is a second, equally cheap port
/// (`ImportManifestStore::load` is a plain-file read, no scan) rather than
/// a second use case, so a single call site still gets one ready-to-render
/// row per source. `execute` never reads `Manifest` at all; it is here
/// purely so `status` can compare the cache's current state against a
/// profile's own last import.
pub struct RefreshRuleDatabases<Fetcher, Manifest> {
    fetcher: Fetcher,
    manifest_store: Manifest,
}

impl<Fetcher: RuleDatabaseFetcher, Manifest: ImportManifestStore>
    RefreshRuleDatabases<Fetcher, Manifest>
{
    /// Builds the use case from its two ports.
    #[must_use]
    pub fn new(fetcher: Fetcher, manifest_store: Manifest) -> Self {
        Self {
            fetcher,
            manifest_store,
        }
    }

    /// Fetches `requested` into `cache_dir`, honouring
    /// `policy.allow_network` and each source's own fetch toggle. Exactly
    /// one outcome per **distinct** entry in `requested`,
    /// in `requested`'s own first-occurrence order — a database named
    /// twice is deduplicated (first occurrence wins) so one source can
    /// never yield two outcomes in the same result, even from a caller
    /// that accidentally asks for it more than once.
    ///
    /// Enforcement order (enforced, not merely documented):
    ///
    /// 1. `allow_network == false` — every requested database
    ///    comes back [`RefreshOutcome::Skipped`]`(`[`SkipReason::NetworkDisabled`]`)`
    ///    and [`RuleDatabaseFetcher::refresh`] is **never called at all**.
    ///    No URL is built, no TLS stack is initialised, no socket is
    ///    opened — this is the property the fake fetcher's
    ///    call-count assertion exists to check.
    /// 2. Otherwise, `requested` is partitioned by each source's own
    ///    fetch toggle (`fetch_community_rules`/`fetch_steam_workshop`):
    ///    a disabled source comes back [`RefreshOutcome::Skipped`]`(`[`SkipReason::SourceDisabled`]`)`
    ///    and is never passed to the port either.
    /// 3. If nothing survives that partition (every requested source is
    ///    disabled), the port is still never called — there is nothing
    ///    left to ask it for.
    /// 4. Whatever remains is fetched in one [`RuleDatabaseFetcher::refresh`]
    ///    call, and its outcomes are merged back into the full,
    ///    caller-ordered result.
    #[must_use]
    pub fn execute(
        &self,
        policy: &NetworkPolicy,
        cache_dir: &Path,
        requested: &[RuleDatabase],
    ) -> Vec<(RuleDatabase, RefreshOutcome)> {
        let ordered = dedupe(requested);

        if !policy.allow_network {
            return ordered
                .into_iter()
                .map(|database| {
                    (
                        database,
                        RefreshOutcome::Skipped {
                            reason: SkipReason::NetworkDisabled,
                        },
                    )
                })
                .collect();
        }

        let to_fetch: Vec<RuleDatabase> = ordered
            .iter()
            .copied()
            .filter(|database| policy.fetches(*database))
            .collect();
        let fetched: BTreeMap<RuleDatabase, RefreshOutcome> = if to_fetch.is_empty() {
            BTreeMap::new()
        } else {
            self.fetcher
                .refresh(cache_dir, &to_fetch)
                .into_iter()
                .collect()
        };

        ordered
            .into_iter()
            .map(|database| {
                let outcome = fetched.get(&database).cloned().unwrap_or_else(|| {
                    if policy.fetches(database) {
                        // A `RuleDatabaseFetcher` implementation that
                        // returns fewer entries than it was asked for is
                        // a port-contract violation, not a user-caused
                        // skip — rendering it as `Skipped { SourceDisabled }`
                        // here would defeat the very reason `SkipReason`
                        // has two variants ("a different fix
                        // from the user"), by making a third, unrelated
                        // condition look like one of them.
                        RefreshOutcome::Failed {
                            failure: FetchFailure::unclassified(
                                "the fetcher reported no outcome for this source",
                            ),
                        }
                    } else {
                        RefreshOutcome::Skipped {
                            reason: SkipReason::SourceDisabled,
                        }
                    }
                });
                (database, outcome)
            })
            .collect()
    }

    /// The once-a-day automatic refresh (`crate::use_cases::RunLaunchNetworkChecks`'s
    /// own caller). Does exactly what [`Self::execute`] does for the
    /// **auto-refresh-eligible** sources — [`RuleDatabase::CommunityRules`]
    /// and [`RuleDatabase::RimmergeRules`] — and nothing more:
    /// [`RuleDatabase::SteamWorkshop`] is never a candidate here at all
    /// (not even a `Skipped` entry), since automatic refresh is simply
    /// not a concept that applies to it
    /// (`RuleDatabase::is_auto_refresh_eligible`).
    ///
    /// `now` is an **injected** clock, per the root `CLAUDE.md`
    /// determinism rule — this is what makes the due-check
    /// deterministic in tests.
    ///
    /// Enforcement order:
    ///
    /// 1. `!policy.allow_network` — every eligible database comes back
    ///    [`RefreshOutcome::Skipped`]`(`[`SkipReason::NetworkDisabled`]`)`
    ///    and the port is never called.
    /// 2. Otherwise, `!policy.auto_refresh_rule_databases` — every
    ///    eligible database comes back
    ///    [`RefreshOutcome::Skipped`]`(`[`SkipReason::AutoRefreshDisabled`]`)`,
    ///    same effect (the port is never called) but a distinct reason:
    ///    network access itself is still on, only the automatic-refresh
    ///    toggle is off, and the two need a different fix from the user.
    /// 3. A disabled source (its own `fetch_*` toggle off) comes back
    ///    [`SkipReason::SourceDisabled`], same as [`Self::execute`].
    /// 4. A source whose last attempt (success, unchanged, *or*
    ///    failure — [`DatabaseStatus::last_attempt_at`]) was under 24 h
    ///    ago comes back [`SkipReason::NotDue`] — an offline machine
    ///    retries at most once a day, not at every launch. Never
    ///    attempted yet counts as due.
    /// 5. Whatever remains is fetched in one
    ///    [`RuleDatabaseFetcher::refresh`] call, same as [`Self::execute`].
    #[must_use]
    pub fn execute_automatic(
        &self,
        policy: &NetworkPolicy,
        cache_dir: &Path,
        now: jiff::Timestamp,
    ) -> Vec<(RuleDatabase, RefreshOutcome)> {
        // Derived, never hand-listed — see `RuleDatabase::ALL`'s own doc
        // comment for why this is safer than a second, independent list
        // of "the eligible ones".
        let auto_eligible: Vec<RuleDatabase> = RuleDatabase::ALL
            .into_iter()
            .filter(|database| database.is_auto_refresh_eligible())
            .collect();
        const DUE_AFTER_SECONDS: i64 = 24 * 60 * 60;

        if !policy.allow_network {
            return auto_eligible
                .into_iter()
                .map(|database| {
                    (
                        database,
                        RefreshOutcome::Skipped {
                            reason: SkipReason::NetworkDisabled,
                        },
                    )
                })
                .collect();
        }
        if !policy.auto_refresh_rule_databases {
            return auto_eligible
                .into_iter()
                .map(|database| {
                    (
                        database,
                        RefreshOutcome::Skipped {
                            reason: SkipReason::AutoRefreshDisabled,
                        },
                    )
                })
                .collect();
        }

        let enabled: BTreeMap<RuleDatabase, bool> = auto_eligible
            .iter()
            .map(|&database| (database, policy.fetches(database)))
            .collect();
        let last_attempt: BTreeMap<RuleDatabase, Option<jiff::Timestamp>> = self
            .fetcher
            .status(cache_dir, &enabled)
            .into_iter()
            .map(|status| (status.database, status.last_attempt_at))
            .collect();
        let due = |database: RuleDatabase| -> bool {
            match last_attempt.get(&database).copied().flatten() {
                None => true,
                // A future `attempted_at` (the clock moved back) has no
                // elapsed time and is due, never "too recent".
                Some(attempted_at) => seconds_since(now, attempted_at)
                    .is_none_or(|elapsed| elapsed >= DUE_AFTER_SECONDS),
            }
        };

        let to_fetch: Vec<RuleDatabase> = auto_eligible
            .iter()
            .copied()
            .filter(|&database| policy.fetches(database) && due(database))
            .collect();
        let fetched: BTreeMap<RuleDatabase, RefreshOutcome> = if to_fetch.is_empty() {
            BTreeMap::new()
        } else {
            self.fetcher
                .refresh(cache_dir, &to_fetch)
                .into_iter()
                .collect()
        };

        auto_eligible
            .into_iter()
            .map(|database| {
                let outcome = fetched.get(&database).cloned().unwrap_or_else(|| {
                    if !policy.fetches(database) {
                        RefreshOutcome::Skipped {
                            reason: SkipReason::SourceDisabled,
                        }
                    } else if !due(database) {
                        RefreshOutcome::Skipped {
                            reason: SkipReason::NotDue,
                        }
                    } else {
                        RefreshOutcome::Failed {
                            failure: FetchFailure::unclassified(
                                "the fetcher reported no outcome for this source",
                            ),
                        }
                    }
                });
                (database, outcome)
            })
            .collect()
    }

    /// Reads the cache manifest through the port — no network, exactly
    /// [`RuleDatabaseFetcher::status`]'s own contract — building the
    /// `enabled` map it needs from `policy`'s own three fetch toggles, then
    /// enriches each row with `profile_dir`'s own
    /// [`crate::ports::ImportManifestStore::load`] result via
    /// [`needs_reimport`].
    #[must_use]
    pub fn status(
        &self,
        policy: &NetworkPolicy,
        cache_dir: &Path,
        profile_dir: &Path,
    ) -> Vec<RuleDatabaseView> {
        let enabled = RuleDatabase::ALL
            .into_iter()
            .map(|database| (database, policy.fetches(database)))
            .collect::<BTreeMap<_, _>>();
        let imported = self.manifest_store.load(profile_dir);
        self.fetcher
            .status(cache_dir, &enabled)
            .into_iter()
            .map(|status| {
                let imported_sha256 = import_source_key(status.database)
                    .and_then(|key| imported.get(key))
                    .map(|record| record.sha256.clone());
                // A source with no import record concept at all can never
                // need re-importing, however fresh its cache is.
                let needs_reimport = import_source_key(status.database).is_some()
                    && needs_reimport(
                        status.enabled,
                        status.cached.as_ref(),
                        imported_sha256.as_deref(),
                    );
                RuleDatabaseView {
                    status,
                    imported_sha256,
                    needs_reimport,
                }
            })
            .collect()
    }
}

/// `requested`, first-occurrence-deduplicated, preserving its own order.
fn dedupe(requested: &[RuleDatabase]) -> Vec<RuleDatabase> {
    let mut seen = std::collections::BTreeSet::new();
    requested
        .iter()
        .copied()
        .filter(|database| seen.insert(*database))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::test_support::{FakeRuleDatabaseFetcher, InMemoryImportManifestStore};

    fn policy(
        fetch_community_rules: bool,
        fetch_steam_workshop: bool,
        allow_network: bool,
    ) -> NetworkPolicy {
        NetworkPolicy {
            fetch_community_rules,
            fetch_steam_workshop,
            allow_network,
            ..NetworkPolicy::default()
        }
    }

    /// A minimal [`DatabaseStatus`] naming only what
    /// [`RefreshRuleDatabases::execute_automatic`]'s due-check reads.
    fn status_for(
        database: RuleDatabase,
        last_attempt_at: Option<jiff::Timestamp>,
    ) -> DatabaseStatus {
        DatabaseStatus {
            database,
            enabled: true,
            path: PathBuf::from("x.json"),
            cached: None,
            last_failure: None,
            last_attempt_at,
        }
    }

    #[test]
    fn execute_automatic_never_calls_the_port_when_network_is_off() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new());
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = NetworkPolicy {
            allow_network: false,
            ..NetworkPolicy::default()
        };

        let outcomes =
            use_case.execute_automatic(&policy, &PathBuf::from("cache"), jiff::Timestamp::now());

        assert_eq!(outcomes.len(), 2, "community and rimmerge-rules only");
        assert!(outcomes.iter().all(|(_, outcome)| matches!(
            outcome,
            RefreshOutcome::Skipped {
                reason: SkipReason::NetworkDisabled
            }
        )));
        assert_eq!(use_case.fetcher.call_count(), 0);
    }

    #[test]
    fn execute_automatic_never_calls_the_port_when_the_auto_refresh_toggle_is_off() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new());
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = NetworkPolicy {
            auto_refresh_rule_databases: false,
            ..NetworkPolicy::default()
        };

        let outcomes =
            use_case.execute_automatic(&policy, &PathBuf::from("cache"), jiff::Timestamp::now());

        assert!(
            outcomes.iter().all(|(_, outcome)| matches!(
                outcome,
                RefreshOutcome::Skipped {
                    reason: SkipReason::AutoRefreshDisabled
                }
            )),
            "distinct from NetworkDisabled: allow_network is still on here, only the \
             auto-refresh toggle is off, and the user needs to know which switch to flip"
        );
        assert_eq!(use_case.fetcher.call_count(), 0);
    }

    #[test]
    fn execute_automatic_never_includes_steam_workshop_at_all() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
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
        ]));
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());

        let outcomes = use_case.execute_automatic(
            &NetworkPolicy::default(),
            &PathBuf::from("cache"),
            jiff::Timestamp::now(),
        );

        assert!(
            outcomes
                .iter()
                .all(|(database, _)| *database != RuleDatabase::SteamWorkshop),
            "Steam Workshop must never appear, not even as Skipped: {outcomes:?}"
        );
    }

    #[test]
    fn execute_automatic_skips_a_disabled_source_and_still_fetches_the_other() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([(
            RuleDatabase::RimmergeRules,
            RefreshOutcome::Unchanged {
                sha256: "b".to_string(),
            },
        )]))
        .with_status(vec![status_for(RuleDatabase::RimmergeRules, None)]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = NetworkPolicy {
            fetch_community_rules: false,
            ..NetworkPolicy::default()
        };

        let outcomes =
            use_case.execute_automatic(&policy, &PathBuf::from("cache"), jiff::Timestamp::now());

        let community = outcomes
            .iter()
            .find(|(database, _)| *database == RuleDatabase::CommunityRules)
            .expect("community row");
        assert!(matches!(
            community.1,
            RefreshOutcome::Skipped {
                reason: SkipReason::SourceDisabled
            }
        ));
        let rimmerge = outcomes
            .iter()
            .find(|(database, _)| *database == RuleDatabase::RimmergeRules)
            .expect("rimmerge row");
        assert!(matches!(rimmerge.1, RefreshOutcome::Unchanged { .. }));
    }

    #[test]
    fn execute_automatic_skips_a_source_attempted_under_24h_ago() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);
        let recent = now - jiff::Span::new().hours(1);
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new()).with_status(vec![
            status_for(RuleDatabase::CommunityRules, Some(recent)),
            status_for(RuleDatabase::RimmergeRules, Some(recent)),
        ]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());

        let outcomes =
            use_case.execute_automatic(&NetworkPolicy::default(), &PathBuf::from("cache"), now);

        assert!(
            outcomes.iter().all(|(_, outcome)| matches!(
                outcome,
                RefreshOutcome::Skipped {
                    reason: SkipReason::NotDue
                }
            )),
            "{outcomes:?}"
        );
        assert_eq!(use_case.fetcher.call_count(), 0);
    }

    #[test]
    fn execute_automatic_treats_a_source_attempted_in_the_future_as_due() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);
        let future = now + jiff::Span::new().hours(48);
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
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
        ]))
        .with_status(vec![
            status_for(RuleDatabase::CommunityRules, Some(future)),
            status_for(RuleDatabase::RimmergeRules, Some(future)),
        ]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());

        let outcomes =
            use_case.execute_automatic(&NetworkPolicy::default(), &PathBuf::from("cache"), now);

        assert!(
            outcomes
                .iter()
                .all(|(_, outcome)| matches!(outcome, RefreshOutcome::Unchanged { .. })),
            "{outcomes:?}"
        );
        assert_eq!(use_case.fetcher.call_count(), 1);
    }

    #[test]
    fn execute_automatic_fetches_a_source_never_attempted_and_one_due_after_24h() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(100);
        let stale = now - jiff::Span::new().hours(25);
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
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
        ]))
        .with_status(vec![
            status_for(RuleDatabase::CommunityRules, None),
            status_for(RuleDatabase::RimmergeRules, Some(stale)),
        ]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());

        let outcomes =
            use_case.execute_automatic(&NetworkPolicy::default(), &PathBuf::from("cache"), now);

        assert!(
            outcomes
                .iter()
                .all(|(_, outcome)| matches!(outcome, RefreshOutcome::Unchanged { .. })),
            "{outcomes:?}"
        );
        assert_eq!(use_case.fetcher.call_count(), 1, "one batched refresh call");
    }

    #[test]
    fn only_enabled_sources_are_requested_and_a_disabled_one_never_reaches_the_port() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([(
            RuleDatabase::CommunityRules,
            RefreshOutcome::Updated {
                sha256: "abc".to_string(),
                bytes: 10,
            },
        )]));
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, false, true);

        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules, RuleDatabase::SteamWorkshop],
        );

        let steam = outcomes
            .iter()
            .find(|(db, _)| *db == RuleDatabase::SteamWorkshop)
            .expect("steam workshop must have an outcome");
        assert_eq!(
            steam.1,
            RefreshOutcome::Skipped {
                reason: SkipReason::SourceDisabled
            }
        );
        // The request log, not just the outcome, is what proves the
        // disabled source never reached the port — an outcome-only
        // assertion would still pass if `refresh` itself decided to skip
        // it internally instead.
        let requests = use_case.fetcher.requests();
        assert_eq!(requests.len(), 1, "exactly one refresh call");
        assert_eq!(requests[0].1, vec![RuleDatabase::CommunityRules]);
    }

    #[test]
    fn a_failed_outcome_for_one_source_does_not_prevent_the_others_update() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([
            (
                RuleDatabase::CommunityRules,
                RefreshOutcome::Failed {
                    failure: FetchFailure::unclassified("connection timed out"),
                },
            ),
            (
                RuleDatabase::SteamWorkshop,
                RefreshOutcome::Updated {
                    sha256: "def".to_string(),
                    bytes: 20,
                },
            ),
        ]));
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, true, true);

        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules, RuleDatabase::SteamWorkshop],
        );

        assert_eq!(
            outcomes
                .iter()
                .find(|(db, _)| *db == RuleDatabase::CommunityRules)
                .expect("community must have an outcome")
                .1,
            RefreshOutcome::Failed {
                failure: FetchFailure::unclassified("connection timed out")
            }
        );
        assert_eq!(
            outcomes
                .iter()
                .find(|(db, _)| *db == RuleDatabase::SteamWorkshop)
                .expect("steam must have an outcome")
                .1,
            RefreshOutcome::Updated {
                sha256: "def".to_string(),
                bytes: 20
            }
        );
    }

    /// The offline switch is
    /// *enforced*, not documented. The call count is the assertion that
    /// matters here — an outcome-only test would still pass if this guard
    /// migrated into the adapter and the port were called anyway.
    #[test]
    fn network_refresh_disabled_skips_every_source_and_the_port_is_never_called() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new());
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, true, false);

        // Every source, the third included — the zero-call-count
        // assertion extends to every source: a new database must be
        // inside the offline switch, not beside it.
        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[
                RuleDatabase::CommunityRules,
                RuleDatabase::SteamWorkshop,
                RuleDatabase::RimmergeRules,
            ],
        );

        assert_eq!(outcomes.len(), 3);
        for (_, outcome) in &outcomes {
            assert_eq!(
                *outcome,
                RefreshOutcome::Skipped {
                    reason: SkipReason::NetworkDisabled
                }
            );
        }
        assert_eq!(
            use_case.fetcher.call_count(),
            0,
            "allow_network: false must never call the port at all"
        );
    }

    /// The third source's own fetch toggle is enforced the same way the
    /// other two are: off means the port never sees it.
    #[test]
    fn fetch_rimmerge_rules_off_never_reaches_the_port() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([(
            RuleDatabase::CommunityRules,
            RefreshOutcome::Updated {
                sha256: "abc".to_string(),
                bytes: 10,
            },
        )]));
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = NetworkPolicy {
            fetch_community_rules: true,
            fetch_steam_workshop: false,
            allow_network: true,
            fetch_rimmerge_rules: false,
            ..NetworkPolicy::default()
        };

        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules, RuleDatabase::RimmergeRules],
        );

        assert_eq!(
            outcomes
                .iter()
                .find(|(db, _)| *db == RuleDatabase::RimmergeRules)
                .expect("rimmerge rules must have an outcome")
                .1,
            RefreshOutcome::Skipped {
                reason: SkipReason::SourceDisabled
            }
        );
        assert_eq!(
            use_case.fetcher.requests()[0].1,
            vec![RuleDatabase::CommunityRules],
            "the disabled source must never reach the port"
        );
    }

    /// The two `SkipReason`s must stay distinguishable: a source disabled
    /// under an otherwise-enabled network, and the reverse.
    #[test]
    fn the_two_skip_reasons_are_distinguishable() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new());
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());

        let source_disabled = use_case.execute(
            &policy(false, false, true),
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules],
        );
        assert_eq!(
            source_disabled[0].1,
            RefreshOutcome::Skipped {
                reason: SkipReason::SourceDisabled
            }
        );

        let network_disabled = use_case.execute(
            &policy(true, false, false),
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules],
        );
        assert_eq!(
            network_disabled[0].1,
            RefreshOutcome::Skipped {
                reason: SkipReason::NetworkDisabled
            }
        );
    }

    /// A `RuleDatabaseFetcher` that violates its own "one entry per
    /// requested database" contract must surface as `Failed`, never as
    /// `Skipped { SourceDisabled }` — an unconditional `SourceDisabled`
    /// fallback would silently lie about *why* an enabled source came
    /// back empty (defeating the entire reason `SkipReason` has two
    /// variants: "a different fix from the user").
    #[test]
    fn a_missing_outcome_for_an_enabled_source_surfaces_as_failed_not_skipped() {
        let fetcher =
            FakeRuleDatabaseFetcher::new(BTreeMap::new()).omitting([RuleDatabase::CommunityRules]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, false, true);

        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules],
        );

        assert_eq!(
            outcomes[0].1,
            RefreshOutcome::Failed {
                failure: FetchFailure::unclassified(
                    "the fetcher reported no outcome for this source"
                )
            }
        );
    }

    #[test]
    fn a_repeated_database_in_requested_yields_exactly_one_outcome() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::from([(
            RuleDatabase::CommunityRules,
            RefreshOutcome::Updated {
                sha256: "abc".to_string(),
                bytes: 10,
            },
        )]));
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, false, true);

        let outcomes = use_case.execute(
            &policy,
            &PathBuf::from("cache"),
            &[RuleDatabase::CommunityRules, RuleDatabase::CommunityRules],
        );

        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            use_case.fetcher.requests()[0].1,
            vec![RuleDatabase::CommunityRules]
        );
    }

    /// A `RuleDatabaseFetcher` that genuinely writes bytes to `cache_dir`
    /// on `refresh` — the shared `FakeRuleDatabaseFetcher` is
    /// deliberately pure in-memory (no filesystem access at all, matching
    /// this crate's own "no filesystem" charter, `crates/rim-session/CLAUDE.md`),
    /// so this one-off double lives only in the single test below that
    /// needs to prove real bytes land only in the cache directory, never
    /// a profile directory — the same "a test that needs a shape the
    /// shared fixture doesn't have writes its own" convention this
    /// codebase already follows elsewhere (see `crates/rim-io/CLAUDE.md`).
    struct WritingFakeFetcher;

    impl RuleDatabaseFetcher for WritingFakeFetcher {
        fn status(
            &self,
            _cache_dir: &Path,
            _enabled: &BTreeMap<RuleDatabase, bool>,
        ) -> Vec<DatabaseStatus> {
            Vec::new()
        }

        fn refresh(
            &self,
            cache_dir: &Path,
            databases: &[RuleDatabase],
        ) -> Vec<(RuleDatabase, RefreshOutcome)> {
            databases
                .iter()
                .map(|database| {
                    std::fs::write(
                        cache_dir.join("communityRules.json"),
                        b"totally different content!!!",
                    )
                    .expect("WritingFakeFetcher: write into the real cache_dir");
                    (
                        *database,
                        RefreshOutcome::Updated {
                            sha256: "totally-different-content".to_string(),
                            bytes: 28,
                        },
                    )
                })
                .collect()
        }
    }

    /// **The refresh-touches-only-the-cache guarantee, proven for real** — not merely by the
    /// signature, and not merely by a scripted outcome: a refresh writes
    /// real bytes only into the cache directory it's given, never a
    /// stand-in profile directory, leaves the session's own rules
    /// untouched, and a resort forced afterward (through
    /// `Session::update_settings`, since `recompute_sort` itself is
    /// private to the `session` module) produces a byte-identical order
    /// to the one from before the refresh.
    ///
    /// Removing the `execute(...)` call below makes this test fail
    /// (`written` would read a file that was never created) — a test
    /// built on a fake that writes nothing, with no `Session` parameter
    /// on `execute`, could not fail that way.
    #[test]
    fn a_refresh_writes_only_the_cache_and_never_touches_rules_or_the_sort() {
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let profile_dir = tempfile::tempdir().expect("tempdir");

        let mut session = crate::test_support::session_fixture(&["b", "a"]);
        let rules_before = session.rules_snapshot();
        let sort_before = session.orders().suggested.clone();

        let use_case =
            RefreshRuleDatabases::new(WritingFakeFetcher, InMemoryImportManifestStore::new());
        let settings = session.settings();

        let _ = use_case.execute(
            &NetworkPolicy::default(),
            cache_dir.path(),
            &[RuleDatabase::CommunityRules],
        );

        let written = std::fs::read(cache_dir.path().join("communityRules.json"))
            .expect("the fake must have really written into the cache dir");
        assert_eq!(written, b"totally different content!!!");
        assert!(
            std::fs::read_dir(profile_dir.path())
                .expect("read profile dir")
                .next()
                .is_none(),
            "a refresh must never write anything to the profile directory"
        );
        assert_eq!(
            session.rules_snapshot(),
            rules_before,
            "a refresh must never modify the session's own rules"
        );

        session.update_settings(settings);
        assert_eq!(
            session.orders().suggested.as_slice(),
            sort_before.as_slice(),
            "a refresh must never change what the sorter produces"
        );
    }

    #[test]
    fn status_builds_the_enabled_map_from_settings() {
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new()).with_status(vec![]);
        let use_case = RefreshRuleDatabases::new(fetcher, InMemoryImportManifestStore::new());
        let policy = policy(true, false, true);

        let _ = use_case.status(&policy, &PathBuf::from("cache"), &PathBuf::from("profile"));

        let enabled = use_case
            .fetcher
            .status_requests()
            .into_iter()
            .next()
            .expect("status must have been called once");
        assert_eq!(enabled.get(&RuleDatabase::CommunityRules), Some(&true));
        assert_eq!(enabled.get(&RuleDatabase::SteamWorkshop), Some(&false));
    }

    // -- `needs_reimport`: the re-import badge rule --

    #[test]
    fn no_cached_copy_never_needs_reimport() {
        assert!(!needs_reimport(true, None, None));
        assert!(!needs_reimport(true, None, Some("abc")));
    }

    #[test]
    fn cached_with_no_import_record_needs_reimport() {
        let cached = CachedDatabase {
            sha256: "abc".to_string(),
            bytes: 10,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        assert!(needs_reimport(true, Some(&cached), None));
    }

    #[test]
    fn cached_and_imported_shas_differ_needs_reimport() {
        let cached = CachedDatabase {
            sha256: "abc".to_string(),
            bytes: 10,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        assert!(needs_reimport(true, Some(&cached), Some("def")));
    }

    #[test]
    fn cached_and_imported_shas_equal_does_not_need_reimport() {
        let cached = CachedDatabase {
            sha256: "abc".to_string(),
            bytes: 10,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        assert!(!needs_reimport(true, Some(&cached), Some("abc")));
    }

    /// A disabled source never needs a reimport,
    /// regardless of what the shas say — nothing could ever act on that
    /// signal, since `should_import_from_cache` refuses to import a
    /// disabled source at all.
    #[test]
    fn a_disabled_source_never_needs_reimport_even_with_differing_shas() {
        let cached = CachedDatabase {
            sha256: "abc".to_string(),
            bytes: 10,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        assert!(!needs_reimport(false, Some(&cached), Some("def")));
        assert!(!needs_reimport(false, Some(&cached), None));
    }

    #[test]
    fn import_source_key_matches_the_manifests_own_string_constants() {
        assert_eq!(
            import_source_key(RuleDatabase::CommunityRules),
            Some(IMPORT_SOURCE_COMMUNITY_RULES)
        );
        assert_eq!(
            import_source_key(RuleDatabase::SteamWorkshop),
            Some(IMPORT_SOURCE_STEAM_DEPENDENCIES)
        );
        assert_eq!(
            import_source_key(RuleDatabase::RimmergeRules),
            None,
            "this source is read from the cache, never imported into a profile"
        );
    }

    /// `status` end to end: a cache entry the profile never imported
    /// comes back `needs_reimport: true` with `imported_sha256: None`; one
    /// whose recorded import sha matches comes back `false`.
    #[test]
    fn status_enriches_each_row_with_the_profiles_own_import_record() {
        use crate::ports::ImportRecord;

        let cached = CachedDatabase {
            sha256: "current-sha".to_string(),
            bytes: 100,
            fetched_at: jiff::Timestamp::UNIX_EPOCH,
        };
        let fetcher = FakeRuleDatabaseFetcher::new(BTreeMap::new()).with_status(vec![
            DatabaseStatus {
                database: RuleDatabase::CommunityRules,
                enabled: true,
                path: PathBuf::from("communityRules.json"),
                cached: Some(cached.clone()),
                last_failure: None,
                last_attempt_at: None,
            },
            DatabaseStatus {
                database: RuleDatabase::SteamWorkshop,
                enabled: false,
                path: PathBuf::from("steamDB.json"),
                cached: Some(CachedDatabase {
                    sha256: "same-sha".to_string(),
                    ..cached
                }),
                last_failure: None,
                last_attempt_at: None,
            },
        ]);
        let manifest_store = InMemoryImportManifestStore::new();
        manifest_store
            .save(
                &PathBuf::from("profile"),
                &BTreeMap::from([(
                    IMPORT_SOURCE_STEAM_DEPENDENCIES.to_string(),
                    ImportRecord {
                        file: "steamDB.json".to_string(),
                        sha256: "same-sha".to_string(),
                        bytes: 100,
                        imported_at: "2026-09-09T00:00:00Z".to_string(),
                    },
                )]),
            )
            .expect("save must succeed");
        let use_case = RefreshRuleDatabases::new(fetcher, manifest_store);

        let views = use_case.status(
            &policy(true, false, true),
            &PathBuf::from("cache"),
            &PathBuf::from("profile"),
        );

        let community = views
            .iter()
            .find(|view| view.status.database == RuleDatabase::CommunityRules)
            .expect("community view");
        assert_eq!(community.imported_sha256, None);
        assert!(
            community.needs_reimport,
            "cached with no import record must need reimport"
        );

        let steam = views
            .iter()
            .find(|view| view.status.database == RuleDatabase::SteamWorkshop)
            .expect("steam view");
        assert_eq!(steam.imported_sha256, Some("same-sha".to_string()));
        assert!(
            !steam.needs_reimport,
            "matching shas must not need reimport"
        );
    }
}
