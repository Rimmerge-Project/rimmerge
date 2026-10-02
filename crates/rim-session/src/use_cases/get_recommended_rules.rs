//! [`GetRecommendedRules`]: the Dashboard's one-click "Get the recommended
//! rules" step, in two phases so the network never runs under the session
//! lock. Phase 1 ([`GetRecommendedRules::fetch`]) takes no `Session` and
//! downloads what is missing; phase 2 ([`GetRecommendedRules::import`])
//! imports what the cache now holds. Phase 2 consumes phase 1's
//! [`FetchedRecommendedRules`], so it cannot run without it.
//!
//! This composes [`EnableRecommendedSources`], [`RefreshRuleDatabases`],
//! [`ImportRimSort`] and [`recommended_rules_step`]; it copies none of them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::Session;
use crate::app_settings::NetworkPolicy;
use crate::ports::{
    AppSettingsLoad, AppSettingsStore, ImportError, ImportManifestStore, ImportedRules,
    RefreshOutcome, RimSortImporter, RimSortPaths, RuleDatabase, RuleDatabaseFetcher, RuleStore,
    StoreError,
};
use crate::recommended_rules::{
    FirstRun, RecommendedRulesFacts, RecommendedRulesStep, SourceNeed, StepSkip, Unavailable,
    recommended_rules_step,
};
use crate::settings::Settings;
use crate::use_cases::{
    EnableRecommendedSources, ImportRimSort, ImportRimSortError, RefreshRuleDatabases,
    RuleDatabaseView,
};

/// Where phase 1 reads and writes, and the facts it needs that no port
/// supplies. The caller already has all of them.
#[derive(Debug, Clone, Copy)]
pub struct RecommendedRulesContext<'a> {
    /// The app-global base directory holding `app-settings.json`.
    pub base: &'a Path,
    /// The rule-database cache directory.
    pub cache_dir: &'a Path,
    /// The profile directory the import will be recorded under.
    pub profile_dir: &'a Path,
    /// The loaded profile's settings. Required by the shared derivation;
    /// phase 1 never reports Done's `imported_rules_in_use`.
    pub settings: &'a Settings,
    /// Whether the first-run notice was answered.
    pub first_run: FirstRun,
}

/// Why phase 1 refused. Nothing was fetched, and nothing was written
/// unless stated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GetRecommendedRulesError {
    /// A network gate closed between the step being shown and the click.
    #[error("the recommended rules cannot be downloaded right now")]
    Unavailable(Unavailable),
    /// `app-settings.json` is damaged. It is repaired from Settings, never
    /// overwritten here.
    #[error("app-settings.json is damaged and was left unchanged")]
    SettingsDamaged,
    /// Turning the recommended sources on failed to save. Nothing was
    /// fetched.
    #[error("saving the recommended sources: {0}")]
    Settings(StoreError),
}

/// Progress, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendedRulesProgress {
    /// One source's download is starting.
    Downloading {
        /// The source being downloaded.
        database: RuleDatabase,
    },
    /// The import of the listed sources is starting.
    Importing {
        /// The sources being imported.
        databases: BTreeSet<RuleDatabase>,
    },
}

/// Why an import left `rules.json` unchanged. Narrower than
/// [`ImportRimSortError`] on purpose: a failed import record is not a
/// failure here (the rules were saved), so it is not representable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportFailure {
    /// A database file could not be read or parsed.
    #[error(transparent)]
    Import(ImportError),
    /// Saving the merged rules failed and the import was rolled back.
    #[error("saving imported rules: {0}")]
    Store(StoreError),
}

/// What the import half did.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportStep {
    /// Nothing needed importing: no save, no resort.
    NotNeeded,
    /// The listed sources were imported and recorded.
    Imported {
        /// The sources imported by this click.
        databases: BTreeSet<RuleDatabase>,
        /// What the importer reported (rule lists, skip counts, provenance).
        imported: ImportedRules,
    },
    /// The rules were imported and saved, but the import record could not
    /// be written, so the step will not read Done until a rerun.
    ImportedManifestNotRecorded {
        /// The sources imported by this click.
        databases: BTreeSet<RuleDatabase>,
    },
    /// The import failed. `rules.json` is unchanged.
    Failed {
        /// Why.
        error: ImportFailure,
    },
    /// Another profile was loaded while downloading, so nothing was
    /// imported. The cache is updated; the new profile's step offers an
    /// import-only click.
    ProfileChanged,
}

impl ImportStep {
    /// Whether the session's rules changed, so an interface must announce
    /// it: true for an import whose rules were saved, whether or not its
    /// record was.
    #[must_use]
    pub fn changes_session(&self) -> bool {
        match self {
            Self::Imported { .. } | Self::ImportedManifestNotRecorded { .. } => true,
            Self::NotNeeded | Self::Failed { .. } | Self::ProfileChanged => false,
        }
    }
}

/// The result of a whole click. A per-source download failure is an
/// outcome here, never an error: being offline is normal.
#[derive(Debug, Clone, PartialEq)]
pub struct RecommendedRulesReport {
    /// One outcome per source phase 1 tried to download.
    pub downloads: BTreeMap<RuleDatabase, RefreshOutcome>,
    /// What the import half did.
    pub import: ImportStep,
}

/// Phase 1's result, consumed by phase 2. Private fields: it can only be
/// made by [`GetRecommendedRules::fetch`].
#[derive(Debug)]
pub struct FetchedRecommendedRules {
    profile_dir: PathBuf,
    cache_dir: PathBuf,
    policy: NetworkPolicy,
    /// The sources this profile had no import record for when phase 1
    /// looked: the only ones phase 2 may import. A source already
    /// imported is never re-imported by this step, so a finished step
    /// stays a no-op even when the cache holds newer bytes.
    missing_import: BTreeSet<RuleDatabase>,
    downloads: BTreeMap<RuleDatabase, RefreshOutcome>,
}

/// Turns on, downloads, and imports the recommended rule databases.
pub struct GetRecommendedRules<AppStore, Fetcher, Manifest> {
    app_store: AppStore,
    fetcher: Fetcher,
    manifest_store: Manifest,
}

impl<AppStore: AppSettingsStore, Fetcher: RuleDatabaseFetcher, Manifest: ImportManifestStore>
    GetRecommendedRules<AppStore, Fetcher, Manifest>
{
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(app_store: AppStore, fetcher: Fetcher, manifest_store: Manifest) -> Self {
        Self {
            app_store,
            fetcher,
            manifest_store,
        }
    }

    /// Phase 1. Takes no `Session` by signature, for the same reason
    /// [`RefreshRuleDatabases`] does: the network never runs under the
    /// session lock.
    ///
    /// # Errors
    ///
    /// [`GetRecommendedRulesError::SettingsDamaged`] for a damaged
    /// `app-settings.json`; [`GetRecommendedRulesError::Unavailable`] when a
    /// download is needed and a gate is closed (a skip never blocks: the
    /// click is the user acting); [`GetRecommendedRulesError::Settings`]
    /// when turning sources on fails to save.
    pub fn fetch(
        &self,
        context: &RecommendedRulesContext<'_>,
        progress: &mut dyn FnMut(RecommendedRulesProgress),
    ) -> Result<FetchedRecommendedRules, GetRecommendedRulesError> {
        let policy = self.load_policy(context.base)?;
        let Some(needs) = self.pending_needs(context, &policy)? else {
            return Ok(Self::fetched(
                context,
                policy,
                BTreeSet::new(),
                BTreeMap::new(),
            ));
        };
        let policy = self.turn_on_if_needed(context.base, policy, &needs)?;
        let downloads = self.download(context, &policy, &needs, progress);
        Ok(Self::fetched(
            context,
            policy,
            needs.keys().copied().collect(),
            downloads,
        ))
    }

    /// Phase 2. Imports what the cache now holds for the sources phase 1
    /// found missing an import record, whatever phase 1's download outcomes
    /// were: a failed Steam download never loses a successful community
    /// import, and an older cached copy of a source whose download failed
    /// is a real, complete file. One `ImportRimSort` call means one rules
    /// save, one resort and one session event. `user_rules` is always
    /// `None`, so RimSort user rules stay untouched.
    #[must_use]
    pub fn import<
        Importer: RimSortImporter,
        Rules: RuleStore,
        ImportManifest: ImportManifestStore,
    >(
        &self,
        session: &mut Session,
        fetched: FetchedRecommendedRules,
        import: &ImportRimSort<Importer, Rules, ImportManifest>,
        progress: &mut dyn FnMut(RecommendedRulesProgress),
    ) -> RecommendedRulesReport {
        let step = self.import_step(session, &fetched, import, progress);
        RecommendedRulesReport {
            downloads: fetched.downloads,
            import: step,
        }
    }

    fn load_policy(&self, base: &Path) -> Result<NetworkPolicy, GetRecommendedRulesError> {
        match self.app_store.load(base) {
            AppSettingsLoad::Loaded(settings) => Ok(settings.network),
            AppSettingsLoad::Missing => Ok(NetworkPolicy::default()),
            AppSettingsLoad::Recovered { .. } => Err(GetRecommendedRulesError::SettingsDamaged),
        }
    }

    /// What a click would do, or `None` when there is nothing to do.
    fn pending_needs(
        &self,
        context: &RecommendedRulesContext<'_>,
        policy: &NetworkPolicy,
    ) -> Result<Option<BTreeMap<RuleDatabase, SourceNeed>>, GetRecommendedRulesError> {
        let views = self.views(policy, context.cache_dir, context.profile_dir);
        let step = recommended_rules_step(&RecommendedRulesFacts {
            policy,
            databases: &views,
            first_run: context.first_run,
            skip: StepSkip::NotSkipped,
            settings: context.settings,
        });
        match step {
            RecommendedRulesStep::NeedsAction { sources } => Ok(Some(sources)),
            // `Skipped` cannot be derived with `StepSkip::NotSkipped`; it
            // shares the nothing-to-do arm so the match stays closed.
            RecommendedRulesStep::Done { .. } | RecommendedRulesStep::Skipped { .. } => Ok(None),
            RecommendedRulesStep::Unavailable(reason) => {
                Err(GetRecommendedRulesError::Unavailable(reason))
            }
        }
    }

    fn views(
        &self,
        policy: &NetworkPolicy,
        cache_dir: &Path,
        profile_dir: &Path,
    ) -> Vec<RuleDatabaseView> {
        RefreshRuleDatabases::new(&self.fetcher, &self.manifest_store).status(
            policy,
            cache_dir,
            profile_dir,
        )
    }

    fn fetched(
        context: &RecommendedRulesContext<'_>,
        policy: NetworkPolicy,
        missing_import: BTreeSet<RuleDatabase>,
        downloads: BTreeMap<RuleDatabase, RefreshOutcome>,
    ) -> FetchedRecommendedRules {
        FetchedRecommendedRules {
            profile_dir: context.profile_dir.to_path_buf(),
            cache_dir: context.cache_dir.to_path_buf(),
            policy,
            missing_import,
            downloads,
        }
    }

    /// Turns the recommended sources on when any need says so, and returns
    /// the policy the downloads must use.
    fn turn_on_if_needed(
        &self,
        base: &Path,
        policy: NetworkPolicy,
        needs: &BTreeMap<RuleDatabase, SourceNeed>,
    ) -> Result<NetworkPolicy, GetRecommendedRulesError> {
        let is_turn_on_needed = needs
            .values()
            .any(|need| matches!(need, SourceNeed::TurnOn));
        if !is_turn_on_needed {
            return Ok(policy);
        }
        EnableRecommendedSources::new(&self.app_store)
            .execute(base)
            .map(|saved| saved.network)
            .map_err(GetRecommendedRulesError::Settings)
    }

    /// Downloads each source that needs the network, one source per call,
    /// in `RuleDatabase` order (the map's order), so a failure cannot
    /// affect the others and the small file is never stuck behind the
    /// large one.
    fn download(
        &self,
        context: &RecommendedRulesContext<'_>,
        policy: &NetworkPolicy,
        needs: &BTreeMap<RuleDatabase, SourceNeed>,
        progress: &mut dyn FnMut(RecommendedRulesProgress),
    ) -> BTreeMap<RuleDatabase, RefreshOutcome> {
        let refresh = RefreshRuleDatabases::new(&self.fetcher, &self.manifest_store);
        let mut downloads = BTreeMap::new();
        for (database, need) in needs {
            if !need.needs_network() {
                continue;
            }
            progress(RecommendedRulesProgress::Downloading {
                database: *database,
            });
            downloads.extend(refresh.execute(policy, context.cache_dir, &[*database]));
        }
        downloads
    }

    fn import_step<
        Importer: RimSortImporter,
        Rules: RuleStore,
        ImportManifest: ImportManifestStore,
    >(
        &self,
        session: &mut Session,
        fetched: &FetchedRecommendedRules,
        import: &ImportRimSort<Importer, Rules, ImportManifest>,
        progress: &mut dyn FnMut(RecommendedRulesProgress),
    ) -> ImportStep {
        if session.paths().profile_dir != fetched.profile_dir {
            return ImportStep::ProfileChanged;
        }
        let paths_by_database = self.paths_to_import(fetched);
        if paths_by_database.is_empty() {
            return ImportStep::NotNeeded;
        }
        let databases: BTreeSet<RuleDatabase> = paths_by_database.keys().copied().collect();
        progress(RecommendedRulesProgress::Importing {
            databases: databases.clone(),
        });
        match import.execute(session, &rimsort_paths(&paths_by_database)) {
            Ok(imported) => ImportStep::Imported {
                databases,
                imported,
            },
            Err(ImportRimSortError::Manifest(_)) => {
                ImportStep::ImportedManifestNotRecorded { databases }
            }
            Err(ImportRimSortError::Import(error)) => ImportStep::Failed {
                error: ImportFailure::Import(error),
            },
            Err(ImportRimSortError::Store(error)) => ImportStep::Failed {
                error: ImportFailure::Store(error),
            },
        }
    }

    /// The cache files to import: sources phase 1 found without an import
    /// record that the cache holds now. A source already imported is left
    /// to `ImportedRulesOutdated`, never re-imported by this step.
    fn paths_to_import(
        &self,
        fetched: &FetchedRecommendedRules,
    ) -> BTreeMap<RuleDatabase, PathBuf> {
        self.views(&fetched.policy, &fetched.cache_dir, &fetched.profile_dir)
            .into_iter()
            .filter(|view| {
                view.needs_reimport && fetched.missing_import.contains(&view.status.database)
            })
            .map(|view| (view.status.database, view.status.path))
            .collect()
    }
}

/// `user_rules` is always `None`: RimSort user rules are never part of this
/// step.
fn rimsort_paths(paths_by_database: &BTreeMap<RuleDatabase, PathBuf>) -> RimSortPaths {
    RimSortPaths {
        user_rules: None,
        community_rules: paths_by_database
            .get(&RuleDatabase::CommunityRules)
            .cloned(),
        steam_db: paths_by_database.get(&RuleDatabase::SteamWorkshop).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{PairRule, Rule, RuleOrigin};

    use super::*;
    use crate::ports::{
        CachedDatabase, DatabaseStatus, FetchFailure, IMPORT_SOURCE_COMMUNITY_RULES,
        IMPORT_SOURCE_STEAM_DEPENDENCIES, ImportRecord,
    };
    use crate::test_support::{
        FakeRimSortImporter, FakeRuleDatabaseFetcher, InMemoryAppSettingsStore,
        InMemoryImportManifestStore, InMemoryRuleStore, session_fixture,
    };

    const BASE: &str = "base";
    const CACHE: &str = "cache";

    /// Everything a test shares by reference with the use case.
    struct World {
        app: InMemoryAppSettingsStore,
        fetcher: FakeRuleDatabaseFetcher,
        manifest: InMemoryImportManifestStore,
        rules: InMemoryRuleStore,
        importer: FakeRimSortImporter,
        settings: Settings,
    }

    fn row(database: RuleDatabase, is_cached: bool) -> DatabaseStatus {
        DatabaseStatus {
            database,
            enabled: true,
            path: PathBuf::from(format!("{CACHE}/{database:?}.json")),
            cached: is_cached.then(|| CachedDatabase {
                sha256: "old".to_string(),
                bytes: 1,
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            }),
            last_failure: None,
            last_attempt_at: None,
        }
    }

    fn updated() -> RefreshOutcome {
        RefreshOutcome::Updated {
            sha256: "new".to_string(),
            bytes: 2,
        }
    }

    fn failed() -> RefreshOutcome {
        RefreshOutcome::Failed {
            failure: FetchFailure::unclassified("offline"),
        }
    }

    fn record(file: &str) -> ImportRecord {
        ImportRecord {
            file: file.to_string(),
            sha256: "new".to_string(),
            bytes: 2,
            imported_at: "2026-10-01T00:00:00Z".to_string(),
        }
    }

    fn importer_result() -> ImportedRules {
        ImportedRules {
            community_rules: Some(Vec::new()),
            steam_dependencies: Some(Vec::new()),
            provenance: BTreeMap::from([
                (
                    IMPORT_SOURCE_COMMUNITY_RULES.to_string(),
                    record("communityRules.json"),
                ),
                (
                    IMPORT_SOURCE_STEAM_DEPENDENCIES.to_string(),
                    record("steamDB.json"),
                ),
            ]),
            ..ImportedRules::default()
        }
    }

    /// Both sources uncached, enabled by default policy, every download
    /// succeeding.
    fn world() -> World {
        World::with(
            vec![
                row(RuleDatabase::CommunityRules, false),
                row(RuleDatabase::SteamWorkshop, false),
                row(RuleDatabase::RimmergeRules, false),
            ],
            BTreeMap::from([
                (RuleDatabase::CommunityRules, updated()),
                (RuleDatabase::SteamWorkshop, updated()),
                (RuleDatabase::RimmergeRules, updated()),
            ]),
        )
    }

    impl World {
        fn with(
            rows: Vec<DatabaseStatus>,
            outcomes: BTreeMap<RuleDatabase, RefreshOutcome>,
        ) -> Self {
            Self {
                app: InMemoryAppSettingsStore::default(),
                fetcher: FakeRuleDatabaseFetcher::new(outcomes)
                    .with_status(rows)
                    .tracking_cache(),
                manifest: InMemoryImportManifestStore::new(),
                rules: InMemoryRuleStore::new(),
                importer: FakeRimSortImporter::new(importer_result()),
                settings: Settings::default(),
            }
        }

        fn use_case(
            &self,
        ) -> GetRecommendedRules<
            &InMemoryAppSettingsStore,
            &FakeRuleDatabaseFetcher,
            &InMemoryImportManifestStore,
        > {
            GetRecommendedRules::new(&self.app, &self.fetcher, &self.manifest)
        }

        fn import_use_case(
            &self,
        ) -> ImportRimSort<&FakeRimSortImporter, &InMemoryRuleStore, &InMemoryImportManifestStore>
        {
            ImportRimSort::new(&self.importer, &self.rules, &self.manifest)
        }

        fn context(&self, first_run: FirstRun) -> RecommendedRulesContext<'_> {
            RecommendedRulesContext {
                base: Path::new(BASE),
                cache_dir: Path::new(CACHE),
                profile_dir: Path::new("profile"),
                settings: &self.settings,
                first_run,
            }
        }

        /// A whole click: phase 1 then phase 2 on a fresh session.
        fn click(
            &self,
            session: &mut Session,
            progress: &mut Vec<RecommendedRulesProgress>,
        ) -> Result<RecommendedRulesReport, GetRecommendedRulesError> {
            let use_case = self.use_case();
            let mut record = |event| progress.push(event);
            let fetched = use_case.fetch(&self.context(FirstRun::Answered), &mut record)?;
            Ok(use_case.import(session, fetched, &self.import_use_case(), &mut record))
        }
    }

    fn requested(world: &World) -> Vec<Vec<RuleDatabase>> {
        world
            .fetcher
            .requests()
            .into_iter()
            .map(|(_, databases)| databases)
            .collect()
    }

    #[test]
    fn happy_path_turns_on_downloads_community_then_steam_and_imports_both_once() {
        let world = World {
            app: InMemoryAppSettingsStore::loaded(crate::AppSettings {
                network: NetworkPolicy {
                    fetch_community_rules: false,
                    fetch_steam_workshop: false,
                    ..NetworkPolicy::default()
                },
                ..crate::AppSettings::default()
            }),
            ..world()
        };
        let mut session = session_fixture(&["a"]);
        let mut progress = Vec::new();

        let report = world.click(&mut session, &mut progress).expect("click");

        assert_eq!(
            requested(&world),
            vec![
                vec![RuleDatabase::CommunityRules],
                vec![RuleDatabase::SteamWorkshop]
            ],
            "one source per call, community first"
        );
        assert_eq!(
            progress,
            vec![
                RecommendedRulesProgress::Downloading {
                    database: RuleDatabase::CommunityRules
                },
                RecommendedRulesProgress::Downloading {
                    database: RuleDatabase::SteamWorkshop
                },
                RecommendedRulesProgress::Importing {
                    databases: BTreeSet::from([
                        RuleDatabase::CommunityRules,
                        RuleDatabase::SteamWorkshop
                    ])
                },
            ]
        );
        assert_eq!(world.app.save_count(), 1, "toggles saved once");
        assert!(
            world
                .app
                .saved()
                .is_some_and(|saved| saved.network.fetch_steam_workshop)
        );
        assert_eq!(world.rules.save_count(), 1);
        assert_eq!(world.manifest.save_count(), 1);
        assert_eq!(world.importer.requests().len(), 1);
        assert!(matches!(report.import, ImportStep::Imported { .. }));
        assert_eq!(report.downloads.len(), 2);
    }

    #[test]
    fn a_failed_steam_download_still_imports_community() {
        let world = World {
            fetcher: FakeRuleDatabaseFetcher::new(BTreeMap::from([
                (RuleDatabase::CommunityRules, updated()),
                (RuleDatabase::SteamWorkshop, failed()),
            ]))
            .with_status(vec![
                row(RuleDatabase::CommunityRules, false),
                row(RuleDatabase::SteamWorkshop, false),
            ])
            .tracking_cache(),
            ..world()
        };
        let mut session = session_fixture(&["a"]);

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        let ImportStep::Imported { databases, .. } = report.import else {
            panic!("expected Imported, got {:?}", report.import);
        };
        assert_eq!(databases, BTreeSet::from([RuleDatabase::CommunityRules]));
        let paths = world.importer.requests();
        assert!(paths[0].community_rules.is_some());
        assert_eq!(paths[0].steam_db, None);
        assert_eq!(report.downloads[&RuleDatabase::SteamWorkshop], failed());
        assert!(
            !world
                .manifest
                .saved()
                .contains_key(IMPORT_SOURCE_STEAM_DEPENDENCIES),
            "a source that was not imported has no import record"
        );
        assert!(
            world
                .manifest
                .saved()
                .contains_key(IMPORT_SOURCE_COMMUNITY_RULES)
        );
    }

    #[test]
    fn a_rerun_after_a_partial_failure_fetches_and_imports_only_the_missing_source() {
        let world = World::with(
            vec![
                row(RuleDatabase::CommunityRules, false),
                row(RuleDatabase::SteamWorkshop, false),
            ],
            BTreeMap::from([
                (RuleDatabase::CommunityRules, updated()),
                (RuleDatabase::SteamWorkshop, failed()),
            ]),
        );
        let mut session = session_fixture(&["a"]);
        world.click(&mut session, &mut Vec::new()).expect("first");
        let world = World {
            fetcher: FakeRuleDatabaseFetcher::new(BTreeMap::from([
                (RuleDatabase::CommunityRules, updated()),
                (RuleDatabase::SteamWorkshop, updated()),
            ]))
            .with_status(world.fetcher.status_rows())
            .tracking_cache(),
            ..world
        };

        let second = world.click(&mut session, &mut Vec::new()).expect("second");

        assert_eq!(requested(&world), vec![vec![RuleDatabase::SteamWorkshop]]);
        let ImportStep::Imported { databases, .. } = second.import else {
            panic!("expected Imported, got {:?}", second.import);
        };
        assert_eq!(databases, BTreeSet::from([RuleDatabase::SteamWorkshop]));
        let paths = world.importer.requests();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1].community_rules, None);
        assert!(paths[1].steam_db.is_some());
    }

    #[test]
    fn an_imported_source_with_a_newer_cache_is_not_reimported() {
        let world = World::with(
            vec![
                row(RuleDatabase::CommunityRules, true),
                row(RuleDatabase::SteamWorkshop, false),
            ],
            BTreeMap::from([(RuleDatabase::SteamWorkshop, updated())]),
        );
        world
            .manifest
            .save(
                Path::new("profile"),
                &BTreeMap::from([(
                    IMPORT_SOURCE_COMMUNITY_RULES.to_string(),
                    ImportRecord {
                        sha256: "older".to_string(),
                        ..record("communityRules.json")
                    },
                )]),
            )
            .expect("seed manifest");
        let mut session = session_fixture(&["a", "b"]);
        session.apply_import(ImportedRules {
            community_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortCommunity,
                comment: None,
                overrides_declared: false,
            })]),
            ..ImportedRules::default()
        });

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        let ImportStep::Imported { databases, .. } = report.import else {
            panic!("expected Imported, got {:?}", report.import);
        };
        assert_eq!(databases, BTreeSet::from([RuleDatabase::SteamWorkshop]));
        let paths = world.importer.requests();
        assert_eq!(paths[0].community_rules, None);
        assert!(paths[0].steam_db.is_some());
        assert_eq!(session.rules().pairs.len(), 1, "community rule kept");
    }

    #[test]
    fn a_failed_steam_download_with_an_older_cache_imports_the_cached_copy() {
        // Steam is off but already cached from an earlier run: the click
        // turns it on and tries to refresh it; the refresh fails, and the
        // older complete copy is imported.
        let world = World {
            app: InMemoryAppSettingsStore::loaded(crate::AppSettings {
                network: NetworkPolicy {
                    fetch_steam_workshop: false,
                    ..NetworkPolicy::default()
                },
                ..crate::AppSettings::default()
            }),
            fetcher: FakeRuleDatabaseFetcher::new(BTreeMap::from([(
                RuleDatabase::SteamWorkshop,
                failed(),
            )]))
            .with_status(vec![
                row(RuleDatabase::CommunityRules, true),
                row(RuleDatabase::SteamWorkshop, true),
            ])
            .tracking_cache(),
            ..world()
        };
        let mut session = session_fixture(&["a"]);

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        assert_eq!(requested(&world), vec![vec![RuleDatabase::SteamWorkshop]]);
        let ImportStep::Imported { databases, .. } = report.import else {
            panic!("expected Imported, got {:?}", report.import);
        };
        assert_eq!(
            databases,
            BTreeSet::from([RuleDatabase::CommunityRules, RuleDatabase::SteamWorkshop])
        );
        assert!(world.importer.requests()[0].steam_db.is_some());
    }

    #[test]
    fn network_off_refuses_before_any_write_or_fetch() {
        let world = World {
            app: InMemoryAppSettingsStore::loaded(crate::AppSettings {
                network: NetworkPolicy {
                    allow_network: false,
                    ..NetworkPolicy::default()
                },
                ..crate::AppSettings::default()
            }),
            ..world()
        };

        let result = world
            .use_case()
            .fetch(&world.context(FirstRun::Answered), &mut |_| {});

        assert_eq!(
            result.expect_err("refused"),
            GetRecommendedRulesError::Unavailable(Unavailable::NetworkOff)
        );
        assert_eq!(world.fetcher.call_count(), 0);
        assert_eq!(world.app.save_count(), 0);
    }

    #[test]
    fn awaiting_first_run_refuses_before_any_write_or_fetch() {
        let world = world();

        let result = world
            .use_case()
            .fetch(&world.context(FirstRun::Pending), &mut |_| {});

        assert_eq!(
            result.expect_err("refused"),
            GetRecommendedRulesError::Unavailable(Unavailable::AwaitingFirstRun)
        );
        assert_eq!(world.fetcher.call_count(), 0);
        assert_eq!(world.app.save_count(), 0);
    }

    #[test]
    fn a_damaged_settings_file_refuses_and_is_left_untouched() {
        let world = World {
            app: InMemoryAppSettingsStore::recovered(),
            ..world()
        };

        let result = world
            .use_case()
            .fetch(&world.context(FirstRun::Answered), &mut |_| {});

        assert_eq!(
            result.expect_err("refused"),
            GetRecommendedRulesError::SettingsDamaged
        );
        assert_eq!(world.app.save_count(), 0);
        assert_eq!(world.fetcher.call_count(), 0);
    }

    #[test]
    fn a_settings_save_failure_fetches_nothing() {
        let world = World {
            app: InMemoryAppSettingsStore::loaded(crate::AppSettings {
                network: NetworkPolicy {
                    fetch_steam_workshop: false,
                    ..NetworkPolicy::default()
                },
                ..crate::AppSettings::default()
            }),
            ..world()
        };
        world.app.fail_next_save();

        let result = world
            .use_case()
            .fetch(&world.context(FirstRun::Answered), &mut |_| {});

        assert!(matches!(
            result.expect_err("refused"),
            GetRecommendedRulesError::Settings(_)
        ));
        assert_eq!(world.fetcher.call_count(), 0);
    }

    #[test]
    fn only_missing_sources_are_fetched() {
        let world = World {
            fetcher: FakeRuleDatabaseFetcher::new(BTreeMap::from([(
                RuleDatabase::SteamWorkshop,
                updated(),
            )]))
            .with_status(vec![
                row(RuleDatabase::CommunityRules, true),
                row(RuleDatabase::SteamWorkshop, false),
            ])
            .tracking_cache(),
            ..world()
        };
        let mut session = session_fixture(&["a"]);

        world.click(&mut session, &mut Vec::new()).expect("click");

        assert_eq!(requested(&world), vec![vec![RuleDatabase::SteamWorkshop]]);
    }

    #[test]
    fn an_import_only_run_never_calls_the_fetcher() {
        let world = World {
            fetcher: FakeRuleDatabaseFetcher::new(BTreeMap::new())
                .with_status(vec![
                    row(RuleDatabase::CommunityRules, true),
                    row(RuleDatabase::SteamWorkshop, true),
                ])
                .tracking_cache(),
            ..world()
        };
        let mut session = session_fixture(&["a"]);
        let mut progress = Vec::new();

        let report = world.click(&mut session, &mut progress).expect("click");

        assert_eq!(world.fetcher.call_count(), 0);
        assert!(report.downloads.is_empty());
        assert!(matches!(report.import, ImportStep::Imported { .. }));
        assert_eq!(progress.len(), 1, "only the Importing event");
    }

    #[test]
    fn a_done_profile_is_a_no_op() {
        let world = world();
        world
            .manifest
            .save(Path::new("profile"), &importer_result().provenance)
            .expect("seed manifest");
        let saves_before = world.manifest.save_count();
        let mut session = session_fixture(&["a"]);

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        assert_eq!(report.import, ImportStep::NotNeeded);
        assert!(report.downloads.is_empty());
        assert_eq!(world.fetcher.call_count(), 0);
        assert_eq!(world.app.save_count(), 0);
        assert_eq!(world.rules.save_count(), 0);
        assert_eq!(world.manifest.save_count(), saves_before);
    }

    #[test]
    fn running_twice_has_the_same_effect_as_once() {
        let world = world();
        let mut session = session_fixture(&["a"]);
        world.click(&mut session, &mut Vec::new()).expect("first");
        let rules_after_first = world.rules.last_saved();
        let calls_after_first = world.fetcher.call_count();

        let second = world.click(&mut session, &mut Vec::new()).expect("second");

        assert_eq!(second.import, ImportStep::NotNeeded);
        assert_eq!(world.fetcher.call_count(), calls_after_first);
        assert_eq!(world.importer.requests().len(), 1);
        assert_eq!(world.rules.save_count(), 1);
        assert_eq!(world.rules.last_saved(), rules_after_first);
    }

    #[test]
    fn the_import_never_passes_user_rules() {
        let world = world();
        let mut session = session_fixture(&["a", "b"]);
        session.apply_import(ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            ..ImportedRules::default()
        });
        let user_pairs_before = session.rules().pairs.len();

        world.click(&mut session, &mut Vec::new()).expect("click");

        assert_eq!(world.importer.requests()[0].user_rules, None);
        assert_eq!(session.rules().pairs.len(), user_pairs_before);
    }

    #[test]
    fn a_rule_store_failure_reports_failed_and_leaves_rules_unchanged() {
        let world = world();
        world.rules.fail_next_save();
        let mut session = session_fixture(&["a"]);
        let rules_before = session.rules().clone();

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        assert!(matches!(
            report.import,
            ImportStep::Failed {
                error: ImportFailure::Store(_)
            }
        ));
        assert_eq!(session.rules(), &rules_before);
        assert_eq!(world.manifest.save_count(), 0);
    }

    #[test]
    fn an_importer_failure_reports_failed_and_leaves_rules_unchanged() {
        let world = world();
        world.importer.fail_next_import();
        let mut session = session_fixture(&["a"]);
        let rules_before = session.rules().clone();

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        assert!(matches!(
            report.import,
            ImportStep::Failed {
                error: ImportFailure::Import(_)
            }
        ));
        assert_eq!(session.rules(), &rules_before);
        assert_eq!(world.rules.save_count(), 0);
        assert_eq!(world.manifest.save_count(), 0);
    }

    #[test]
    fn a_manifest_failure_reports_imported_manifest_not_recorded() {
        let world = world();
        world.manifest.fail_next_save();
        let mut session = session_fixture(&["a"]);

        let report = world.click(&mut session, &mut Vec::new()).expect("click");

        assert!(matches!(
            report.import,
            ImportStep::ImportedManifestNotRecorded { .. }
        ));
        assert_eq!(world.rules.save_count(), 1, "the rules were saved");
    }

    #[test]
    fn only_a_saved_import_changes_the_session() {
        let databases = BTreeSet::from([RuleDatabase::SteamWorkshop]);
        let cases = [
            (ImportStep::NotNeeded, false),
            (ImportStep::ProfileChanged, false),
            (
                ImportStep::Failed {
                    error: ImportFailure::Import(crate::ports::ImportError("boom".to_string())),
                },
                false,
            ),
            (
                ImportStep::ImportedManifestNotRecorded {
                    databases: databases.clone(),
                },
                true,
            ),
            (
                ImportStep::Imported {
                    databases,
                    imported: ImportedRules::default(),
                },
                true,
            ),
        ];

        for (step, should_change) in cases {
            assert_eq!(step.changes_session(), should_change, "{step:?}");
        }
    }

    #[test]
    fn a_changed_profile_skips_the_import() {
        let world = world();
        let mut session = session_fixture(&["a"]);
        let progress = RefCell::new(Vec::new());
        let use_case = world.use_case();
        let other_profile = RecommendedRulesContext {
            profile_dir: Path::new("another-profile"),
            ..world.context(FirstRun::Answered)
        };
        let fetched = use_case
            .fetch(&other_profile, &mut |event| {
                progress.borrow_mut().push(event)
            })
            .expect("fetch");

        let report = use_case.import(
            &mut session,
            fetched,
            &world.import_use_case(),
            &mut |event| progress.borrow_mut().push(event),
        );

        assert_eq!(report.import, ImportStep::ProfileChanged);
        assert_eq!(report.downloads.len(), 2, "the cache was still updated");
        assert_eq!(world.rules.save_count(), 0);
        assert!(world.importer.requests().is_empty());
    }
}
