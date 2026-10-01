//! [`Rescan`]: re-runs [`crate::use_cases::LoadProject::execute_with_active_set`]
//! over a session's own working set.
//! The working set is the *only* in-memory-only state this crate carries
//! (every other mutating use case persists immediately — see this crate's
//! own `CLAUDE.md`), which is exactly what makes re-reading
//! decisions/rules/patches/assignments from disk here safe: nothing else
//! about the session could have gone stale relative to what's on disk.

use crate::Session;
use crate::ports::{
    AssignmentProjectStore, DecisionStore, ModKnowledgeStore, ModScanner, ModsConfigStore,
    PatchProjectStore, RuleStore, ScanProgress,
};
use crate::use_cases::{LoadProject, LoadProjectError};

/// Rebuilds a session against its own working set.
pub struct Rescan<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge> {
    load_project: LoadProject<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge>,
}

impl<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge>
    Rescan<Scanner, Config, Decisions, Rules, Patches, Assignments, Knowledge>
where
    Scanner: ModScanner,
    Config: ModsConfigStore,
    Decisions: DecisionStore,
    Rules: RuleStore,
    Patches: PatchProjectStore,
    Assignments: AssignmentProjectStore,
    Knowledge: ModKnowledgeStore,
{
    /// Wraps an already-built [`LoadProject`] — the composition root
    /// builds one the same way it does for a fresh load, and hands it to
    /// both use cases.
    #[must_use]
    pub fn new(
        load_project: LoadProject<
            Scanner,
            Config,
            Decisions,
            Rules,
            Patches,
            Assignments,
            Knowledge,
        >,
    ) -> Self {
        Self { load_project }
    }

    /// Re-scans `session`'s own paths with its working set
    /// (`session.working().ids()`) as the active list, returning a
    /// brand-new [`Session`] the composition root swaps in, on the same
    /// selected order the old one had (a user's choice is not a scan
    /// result, and the interface holding the selection is not told about
    /// the swap) — this crate
    /// builds no in-place session mutation for a rescan, since
    /// [`Session::new`] is the one place every derived field (tagging,
    /// sort, ledgers) is computed consistently.
    ///
    /// # Errors
    ///
    /// See [`LoadProjectError`].
    pub fn execute(
        &self,
        session: &Session,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<Session, LoadProjectError> {
        let paths = session.paths().clone();
        let active = session.working().ids().to_vec();
        let mut rescanned = self
            .load_project
            .execute_with_active_set(paths, active, progress)?;
        rescanned.select(session.selected());
        Ok(rescanned)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use rim_analyzer::domain::ModId;

    use super::*;
    use crate::Session;
    use crate::ports::{ScanArtifacts, ScanError};
    use crate::test_support::{
        FakeModKnowledgeStore, FakeScanner, InMemoryAssignmentProjectStore, InMemoryDecisionStore,
        InMemoryModsConfigStore, InMemoryPatchProjectStore, InMemoryRuleStore,
        report_fixture_with_inactive,
    };
    use crate::use_cases::ActivateMods;

    /// Shares one [`FakeScanner`] with the [`LoadProject`] it's handed
    /// to — `LoadProject::new` takes `Scanner` by value, but a test also
    /// wants to read `last_override()` back afterward, which needs its
    /// own handle to the same fake.
    struct SharedScanner(Rc<FakeScanner>);

    impl ModScanner for SharedScanner {
        fn scan_and_analyze(
            &self,
            paths: &crate::ProjectPaths,
            active_override: Option<&[ModId]>,
            progress: &mut dyn FnMut(ScanProgress),
        ) -> Result<ScanArtifacts, ScanError> {
            self.0.scan_and_analyze(paths, active_override, progress)
        }
    }

    fn paths() -> crate::ProjectPaths {
        crate::ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        }
    }

    /// A session with `a` active and `b` inactive — so `b` is a known id
    /// [`ActivateMods`] can actually schedule.
    fn session_with_b_inactive() -> Session {
        Session::new(
            paths(),
            report_fixture_with_inactive(&["a"], &["b"]),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn rescan_reaches_the_scanner_with_the_working_set_and_the_new_session_matches_it() {
        let mut session = session_with_b_inactive();
        let plan = ActivateMods::plan(&session, &[ModId::new("b")], false);
        ActivateMods::execute(&mut session, &plan).expect("b is known");

        let scanner = Rc::new(FakeScanner::new(
            report_fixture_with_inactive(&["a", "b"], &[]),
            Vec::new(),
        ));
        let load_project = LoadProject::new(
            SharedScanner(scanner.clone()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );
        let rescan = Rescan::new(load_project);

        let new_session = rescan
            .execute(&session, &mut |_| {})
            .expect("rescan must succeed");

        assert_eq!(
            scanner.last_override(),
            Some(Some(vec![ModId::new("a"), ModId::new("b")])),
            "the scanner must actually receive the working set as its override"
        );
        assert_eq!(
            new_session.orders().current.as_slice(),
            [ModId::new("a"), ModId::new("b")]
        );
        let pending = new_session.pending_changes();
        assert!(
            pending.unscanned.is_empty(),
            "the new session's working set is exactly what it was just built from"
        );
        assert_eq!(
            pending.unapplied.added,
            [ModId::new("b")],
            "b is scanned now but not yet written to ModsConfig.xml"
        );
    }

    /// Deactivating a `MissingMod`
    /// finding's own mod and rescanning orphans the decision through the
    /// *existing* path (the profile's `DecisionSet` is never pruned
    /// automatically; a decision simply stops matching any live finding)
    /// — no new logic, asserted here.
    #[test]
    fn a_decision_on_a_now_gone_missing_mod_is_orphaned_by_the_existing_path_after_a_rescan() {
        use rim_resolve::domain::{Action, Decision, FindingKey, OrderSource};

        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("missing.mod")
            .build();
        let mut session = Session::new(
            paths(),
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a"), ModId::new("missing.mod")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("missing.mod"),
        };
        session
            .decide(Decision {
                key: key.clone(),
                action: Action::Accept,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("Accept is always a valid action");
        assert!(
            session.resolution(OrderSource::Current, &key).is_some(),
            "the decision is live while the mod is still missing"
        );

        // Persist the decision so a real rescan (which re-reads decisions
        // from disk, per this use case's own doc comment) actually sees
        // it.
        let decision_store = InMemoryDecisionStore::new();
        decision_store
            .save(std::path::Path::new("profile"), session.decisions())
            .expect("seeding the fake store never fails");

        // Deactivate the missing mod (the natural way to resolve that
        // finding) and rescan without it.
        let deactivate_plan =
            crate::use_cases::DeactivateMods::plan(&session, &[ModId::new("missing.mod")]);
        crate::use_cases::DeactivateMods::execute(&mut session, &deactivate_plan);

        let scanner = FakeScanner::new(
            rim_resolve::test_support::ReportBuilder::new()
                .mod_("a")
                .build(),
            Vec::new(),
        );
        let load_project = LoadProject::new(
            scanner,
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a"), ModId::new("missing.mod")],
                known_expansions: Vec::new(),
            }),
            decision_store,
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );
        let rescan = Rescan::new(load_project);

        let mut new_session = rescan
            .execute(&session, &mut |_| {})
            .expect("rescan must succeed");

        assert!(
            new_session.decisions().get(&key).is_some(),
            "the profile's own DecisionSet is never pruned automatically"
        );
        assert!(
            new_session.resolution(OrderSource::Current, &key).is_none(),
            "no live finding matches this key any more, once missing.mod is gone — \
             the existing 'no live finding' path is what orphans it, not new logic"
        );
    }

    #[test]
    fn rescan_keeps_the_selected_order() {
        let mut session = session_with_b_inactive();
        session.select(rim_resolve::domain::OrderSource::Suggested);
        let load_project = LoadProject::new(
            FakeScanner::new(report_fixture_with_inactive(&["a"], &["b"]), Vec::new()),
            InMemoryModsConfigStore::new(crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        );

        let new_session = Rescan::new(load_project)
            .execute(&session, &mut |_| {})
            .expect("rescan must succeed");

        assert_eq!(
            new_session.selected(),
            rim_resolve::domain::OrderSource::Suggested
        );
    }
}
