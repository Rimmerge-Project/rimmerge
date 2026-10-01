//! [`LoadProject`]: scans a RimWorld install, loads persisted decisions
//! and rules, and builds a fresh [`Session`].

use rim_analyzer::domain::ModId;

use crate::ports::{
    AssignmentProjectStore, ConfigError, DecisionStore, ModKnowledgeStore, ModScanner,
    ModsConfigStore, PatchProjectStore, RuleStore, ScanError, ScanProgress, StoreError,
};
use crate::{ProjectPaths, Session};

/// Everything that can go wrong loading a project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadProjectError {
    /// `ModsConfig.xml` couldn't be read or parsed.
    #[error("reading ModsConfig.xml: {0}")]
    Config(#[from] ConfigError),
    /// The scan or analysis failed.
    #[error("scanning and analyzing the install: {0}")]
    Scan(#[from] ScanError),
    /// The saved decisions file couldn't be loaded.
    #[error("loading saved decisions: {0}")]
    Decisions(StoreError),
    /// The saved rules file couldn't be loaded.
    #[error("loading saved rules: {0}")]
    Rules(StoreError),
    /// The saved compat patch projects couldn't be loaded.
    #[error("loading saved patches: {0}")]
    Patches(StoreError),
    /// The saved assignment (patch maker) projects couldn't be loaded.
    #[error("loading saved assignments: {0}")]
    Assignments(StoreError),
}

/// Scans a RimWorld install, loads persisted decisions, rules, compat
/// patch projects, and assignment (patch maker) projects, and builds a
/// fresh [`Session`].
pub struct LoadProject<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge> {
    scanner: Scanner,
    config_store: Config,
    decision_store: Decisions,
    rule_store: Rules,
    patch_store: Patches,
    assignment_store: Assignments,
    knowledge_store: Knowledge,
    /// Whether `knowledge_store.load` may read the fetched cache file —
    /// `NetworkPolicy::fetch_rimmerge_rules`, read once by the composition
    /// root before it builds this use case. **Not** a per-profile
    /// [`crate::Settings`] field: the cache this toggle gates is
    /// app-global, so the value must come from the app-global policy, not
    /// from the profile whose rules are about to load. See
    /// `crate::app_settings::NetworkPolicy`'s own doc comment for why this
    /// moved out of `Settings`.
    fetch_rimmerge_rules: bool,
}

impl<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge>
    LoadProject<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge>
where
    Scanner: ModScanner,
    Config: ModsConfigStore,
    Decisions: DecisionStore,
    Rules: RuleStore,
    Patches: PatchProjectStore,
    Assignments: AssignmentProjectStore,
    Knowledge: ModKnowledgeStore,
{
    /// Builds the use case from its ports. `patch_store`/`assignment_store`
    /// are real-backed by `rim-io::JsonPatchProjectStore`/
    /// `rim-io::JsonAssignmentProjectStore` in both composition roots
    /// (`apps/cli`, `apps/desktop/src-tauri`); [`crate::ports::NoPatchStore`]/
    /// [`crate::ports::NoAssignmentStore`] remain only as fixtures for
    /// tests that don't need either kind of project to persist.
    ///
    /// `knowledge_store` is the seventh port
    /// real-backed by
    /// `rim_io::FsModKnowledgeStore` in both composition roots;
    /// `crate::test_support::FakeModKnowledgeStore::empty` is the "knows
    /// nothing" fixture for tests that don't care. `fetch_rimmerge_rules`
    /// is the composition root's own already-loaded
    /// `NetworkPolicy::fetch_rimmerge_rules` — see this struct's own field
    /// doc comment.
    // One argument per port (seven) plus the one policy flag: the
    // established shape every multi-port use case constructor in this
    // crate already takes, never bundled into a params struct — adding a
    // struct here for one extra `bool` would be inconsistent with every
    // sibling constructor's own signature.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        scanner: Scanner,
        config_store: Config,
        decision_store: Decisions,
        rule_store: Rules,
        patch_store: Patches,
        assignment_store: Assignments,
        knowledge_store: Knowledge,
        fetch_rimmerge_rules: bool,
    ) -> Self {
        Self {
            scanner,
            config_store,
            decision_store,
            rule_store,
            patch_store,
            assignment_store,
            knowledge_store,
            fetch_rimmerge_rules,
        }
    }

    /// Scans `paths`, loads persisted state, and builds a [`Session`].
    ///
    /// # Errors
    ///
    /// Returns [`LoadProjectError`] when reading `ModsConfig.xml`,
    /// scanning, or loading persisted decisions/rules/patches/assignments
    /// fails.
    pub fn execute(
        &self,
        paths: ProjectPaths,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<Session, LoadProjectError> {
        self.load(paths, None, progress)
    }

    /// [`Self::execute`], but scanning `active` as the active-mod list
    /// verbatim instead of `ModsConfig.xml`'s own `<activeMods>`
    /// — the file
    /// still must exist (its `version`/`knownExpansions` seed the built
    /// [`Session`]), but its active list is never read. The returned
    /// session's `orders().current` is `active`, not whatever
    /// `ModsConfig.xml` last had on disk: this is the whole contract a
    /// rescan of a pending, unsaved working set needs.
    ///
    /// # Errors
    ///
    /// Same as [`Self::execute`].
    pub fn execute_with_active_set(
        &self,
        paths: ProjectPaths,
        active: Vec<ModId>,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<Session, LoadProjectError> {
        self.load(paths, Some(active), progress)
    }

    fn load(
        &self,
        paths: ProjectPaths,
        active_override: Option<Vec<ModId>>,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<Session, LoadProjectError> {
        let mut mods_config = self.config_store.read(&paths.mods_config)?;
        // Captured before an override (if any) overwrites `active_mods`
        // below — `Session::set_file_active_mods`
        // needs the file's *real* list, since `mods_config.active_mods`
        // itself is about to become the working set being previewed
        // instead.
        let file_active_mods = mods_config.active_mods.clone();
        let artifacts =
            self.scanner
                .scan_and_analyze(&paths, active_override.as_deref(), progress)?;
        let has_override = active_override.is_some();
        if let Some(active) = active_override {
            mods_config.active_mods = active;
        }
        let decisions = self
            .decision_store
            .load(&paths.profile_dir)
            .map_err(LoadProjectError::Decisions)?;
        let loaded_rules = self
            .rule_store
            .load(&paths.profile_dir)
            .map_err(LoadProjectError::Rules)?;
        let patches = self
            .patch_store
            .load_all(&paths.profile_dir)
            .map_err(LoadProjectError::Patches)?;
        let assignments = self
            .assignment_store
            .load_all(&paths.profile_dir)
            .map_err(LoadProjectError::Assignments)?;

        // The mod-specific knowledge that lives in data
        // — read here, once,
        // rather than anywhere on the sort path. Never fallible: the
        // adapter falls back to the vendored defaults and reports the
        // reason as a warning, alongside the rules file's own.
        //
        // `self.fetch_rimmerge_rules` (the composition root's own
        // `NetworkPolicy::fetch_rimmerge_rules`, given at construction —
        // see this struct's own field doc comment) decides whether the
        // cached copy may be read at all.
        let loaded_knowledge = self.knowledge_store.load(self.fetch_rimmerge_rules);

        let mut session = Session::new(
            paths,
            artifacts.report,
            artifacts.evidence,
            artifacts.sources,
            loaded_rules.rules,
            decisions,
            mods_config,
            patches,
            assignments,
        );
        let mut warnings = loaded_rules.warnings;
        warnings.extend(loaded_knowledge.warnings);
        session.set_rule_load_warnings(warnings);
        session.set_mod_knowledge(loaded_knowledge.knowledge);
        // `Session::new`'s own default for `file_active_mods` (whatever
        // `mods_config.active_mods` held when it ran) is wrong exactly
        // when an override was in play: that field is the *working* set
        // being previewed, not the file's own list, which is what
        // `file_active_mods` captured above still holds untouched.
        if has_override {
            session.set_file_active_mods(file_active_mods);
        }
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeModKnowledgeStore, FakeScanner, InMemoryAssignmentProjectStore, InMemoryDecisionStore,
        InMemoryModsConfigStore, InMemoryPatchProjectStore, InMemoryRuleStore, report_fixture,
    };

    fn paths() -> ProjectPaths {
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        }
    }

    #[test]
    fn builds_a_session_with_both_orders_from_a_fresh_profile() {
        let report = report_fixture(&["a", "b"]);
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![
                    rim_analyzer::domain::ModId::new("a"),
                    rim_analyzer::domain::ModId::new("b"),
                ],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let mut progressed = Vec::new();
        let session = use_case
            .execute(paths(), &mut |p| progressed.push(p))
            .expect("load must succeed");

        assert!(!progressed.is_empty(), "progress callback must be invoked");
        assert_eq!(session.orders().current.as_slice().len(), 2);
    }

    /// `LoadProject::new`'s own `fetch_rimmerge_rules` argument (the
    /// composition root's already-loaded `NetworkPolicy::fetch_rimmerge_rules`)
    /// must reach the knowledge store, which is what lets the toggle gate
    /// *consumption* of a cached copy and not merely a refresh. Asserted
    /// on the store's recorded argument rather than on behaviour, because
    /// the gating rule itself lives in `rim-io`'s real store (and is
    /// tested there) — this is about the wiring being present at all.
    #[test]
    fn the_given_fetch_rimmerge_rules_flag_reaches_the_knowledge_store() {
        for enabled in [true, false] {
            let use_case = LoadProject::new(
                FakeScanner::new(report_fixture(&["a"]), Vec::new()),
                InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                    version: "1.6".to_string(),
                    active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                    known_expansions: Vec::new(),
                }),
                InMemoryDecisionStore::new(),
                InMemoryRuleStore::new(),
                InMemoryPatchProjectStore::new(),
                InMemoryAssignmentProjectStore::new(),
                FakeModKnowledgeStore::empty(),
                enabled,
            );

            use_case
                .execute(paths(), &mut |_| {})
                .expect("load must succeed");

            assert_eq!(
                use_case.knowledge_store.loaded_with(),
                Some(enabled),
                "LoadProject must pass fetch_rimmerge_rules = {enabled} through"
            );
        }
    }

    #[test]
    fn propagates_a_patch_store_load_failure() {
        let report = report_fixture(&["a"]);
        let patch_store = InMemoryPatchProjectStore::new();
        patch_store.fail_next_load();
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            patch_store,
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let result = use_case.execute(paths(), &mut |_| {});

        assert!(matches!(result, Err(LoadProjectError::Patches(_))));
    }

    /// The assignment-store twin of [`propagates_a_patch_store_load_failure`]
    /// — `LoadProject`'s own sixth port actually reaching
    /// `LoadProjectError::Assignments` rather than being silently ignored.
    #[test]
    fn propagates_an_assignment_store_load_failure() {
        let report = report_fixture(&["a"]);
        let assignment_store = InMemoryAssignmentProjectStore::new();
        assignment_store.fail_next_load();
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            assignment_store,
            FakeModKnowledgeStore::empty(),
            true,
        );

        let result = use_case.execute(paths(), &mut |_| {});

        assert!(matches!(result, Err(LoadProjectError::Assignments(_))));
    }

    #[test]
    fn propagates_a_config_read_failure() {
        let report = report_fixture(&["a"]);
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::default(),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let result = use_case.execute(paths(), &mut |_| {});

        assert!(matches!(result, Err(LoadProjectError::Config(_))));
    }

    /// `RuleStore::load`'s own warnings must actually reach the built
    /// [`crate::Session`] via `set_rule_load_warnings` — not just read
    /// and discarded. Uses `InMemoryRuleStore::warn_on_next_load` rather
    /// than asserting against the
    /// wiring this module's `execute` already visibly does, so a future
    /// refactor that accidentally drops the `set_rule_load_warnings` call
    /// fails this test.
    #[test]
    fn threads_rule_store_load_warnings_onto_the_session() {
        let report = report_fixture(&["a"]);
        let rule_store = InMemoryRuleStore::new();
        rule_store.warn_on_next_load(vec![crate::ports::RulesLoadWarning::DroppedClusterRules {
            rule_ids: vec!["framework".to_string()],
        }]);
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            rule_store,
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let session = use_case
            .execute(paths(), &mut |_| {})
            .expect("load must succeed");

        assert_eq!(
            session.rule_load_warnings(),
            &[crate::ports::RulesLoadWarning::DroppedClusterRules {
                rule_ids: vec!["framework".to_string()],
            }],
            "RuleStore::load's warnings must reach the session"
        );
    }

    /// A project loaded with one saved patch project must surface it on
    /// the resulting [`crate::Session`] — `LoadProject`'s own fifth port
    /// actually reaching `Session::new`.
    #[test]
    fn loads_a_previously_saved_patch_project() {
        let report = report_fixture(&["a", "b"]);
        let patch_store = InMemoryPatchProjectStore::new();
        let project = rim_resolve::domain::PatchProject::new(
            "abcdef012345".parse().expect("valid patch id"),
            "AB compat".to_string(),
            rim_resolve::domain::PatchModIdentity::new("sample.abcompat", "A + B Compatibility")
                .expect("valid identity"),
            rim_resolve::domain::PatchScope::new([
                rim_analyzer::domain::ModId::new("a"),
                rim_analyzer::domain::ModId::new("b"),
            ])
            .expect("two distinct members"),
            jiff::Timestamp::UNIX_EPOCH,
        );
        let id = project.id().clone();
        patch_store
            .save(std::path::Path::new("profile"), &project)
            .expect("seeding the fake store must succeed");
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![
                    rim_analyzer::domain::ModId::new("a"),
                    rim_analyzer::domain::ModId::new("b"),
                ],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            patch_store,
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let session = use_case
            .execute(paths(), &mut |_| {})
            .expect("load must succeed");

        assert!(session.patch(&id).is_some());
    }

    /// The assignment-project twin of [`loads_a_previously_saved_patch_project`]
    /// — `LoadProject`'s own sixth port actually reaching `Session::new`.
    #[test]
    fn loads_a_previously_saved_assignment_project() {
        let report = report_fixture(&["a"]);
        let assignment_store = InMemoryAssignmentProjectStore::new();
        let project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &rim_analyzer::domain::ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "Example race patch".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "Sample Part Patch")
                .expect("valid identity"),
            [rim_analyzer::domain::ModId::new("example.framework")]
                .into_iter()
                .collect(),
            [rim_analyzer::domain::ModId::new("some.race.mod")]
                .into_iter()
                .collect(),
            rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartAssignmentDef".to_string(),
                refs: std::collections::BTreeSet::new(),
                fields: std::collections::BTreeMap::new(),
                target_shapes: std::collections::BTreeMap::new(),
            },
            jiff::Timestamp::UNIX_EPOCH,
        );
        let id = project.id().clone();
        assignment_store
            .save(std::path::Path::new("profile"), &project)
            .expect("seeding the fake store must succeed");
        let use_case = LoadProject::new(
            FakeScanner::new(report, Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            assignment_store,
            FakeModKnowledgeStore::empty(),
            true,
        );

        let session = use_case
            .execute(paths(), &mut |_| {})
            .expect("load must succeed");

        assert!(session.assignment(&id).is_some());
    }

    /// `execute` (no override) must reach the scanner with `None` —
    /// today's unchanged behaviour, pinned so a regression that always
    /// passed `Some` (silently discarding the file's own active list)
    /// fails loudly, not just under
    /// `execute_with_active_set_reaches_the_scanner_with_the_override`
    /// below. `Some(None)`, not bare `None`: the scanner *was* called
    /// (once), just with no override — see `FakeScanner::last_override`'s
    /// own doc comment for why the two are distinguishable at all.
    #[test]
    fn execute_reaches_the_scanner_with_no_override() {
        let scanner = FakeScanner::new(report_fixture(&["a"]), Vec::new());
        let use_case = LoadProject::new(
            scanner,
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        assert_eq!(
            use_case.scanner.last_override(),
            None,
            "must read None before the scanner has ever been called"
        );

        use_case
            .execute(paths(), &mut |_| {})
            .expect("load must succeed");

        assert_eq!(use_case.scanner.last_override(), Some(None));
    }

    /// A wiring assertion: `execute_with_active_set` must
    /// reach the scanner with `Some(active)`, and the resulting session's
    /// `orders().current` must be exactly `active` — not the file's own
    /// `<activeMods>` (which names only `"a"` here, while the working set
    /// being previewed is `["b", "a"]`).
    #[test]
    fn execute_with_active_set_reaches_the_scanner_with_the_override_and_becomes_current() {
        let scanner = FakeScanner::new(report_fixture(&["a", "b"]), Vec::new());
        let use_case = LoadProject::new(
            scanner,
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![rim_analyzer::domain::ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );
        let working_set = vec![
            rim_analyzer::domain::ModId::new("b"),
            rim_analyzer::domain::ModId::new("a"),
        ];

        let session = use_case
            .execute_with_active_set(paths(), working_set.clone(), &mut |_| {})
            .expect("load must succeed");

        assert_eq!(
            use_case.scanner.last_override(),
            Some(Some(working_set.clone()))
        );
        assert_eq!(session.orders().current.as_slice(), working_set.as_slice());
    }
}
