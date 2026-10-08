//! [`Apply`]: persists decisions and rules; when asked, writes the
//! selected order to `ModsConfig.xml` (with a backup) and/or renders the
//! generated merge mod into the game's `Mods/` folder atomically.

use std::path::PathBuf;

use rim_resolve::domain::{FindingKey, GeneratedModIdentity, OrderSource};

use super::plan_merge::PlanMergeError;
use super::render_merge_mod::{RenderMergeMod, RenderMergeModError};
use crate::Session;
use crate::ports::{
    AssetLocator, ConfigError, DecisionStore, DefSourceReader, MergeModError, MergeModWriter,
    ModsConfigStore, RuleStore, StoreError,
};

/// The previous merge-mod generation's backup folder, relative to the
/// profile directory — exactly one generation is ever kept.
const MERGE_MOD_BACKUP_DIR: &str = "merge-mod.prev";

/// Everything that can go wrong applying a session.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    /// Saving decisions failed.
    #[error("saving decisions: {0}")]
    Decisions(StoreError),
    /// Saving rules failed.
    #[error("saving rules: {0}")]
    Rules(StoreError),
    /// Writing `ModsConfig.xml` failed.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Building a merge preview (for a decision with no cached one yet)
    /// failed.
    #[error("planning a merge: {0}")]
    Plan(#[from] PlanMergeError),
    /// Locating a `ShipAsset` texture's source file failed.
    #[error("locating an asset: {0}")]
    Asset(crate::ports::DefSourceError),
    /// Rendering the merge mod's files failed.
    #[error("rendering the merge mod: {0}")]
    Emit(#[from] rim_merge::emit::EmitError),
    /// Writing or removing the merge mod folder failed.
    #[error(transparent)]
    MergeMod(#[from] MergeModError),
    /// `options.write_mods_config` was set but [`Session::is_stale`] is
    /// true. Decisions and rules were still saved (unaffected by this
    /// refusal); only the `ModsConfig.xml` write is skipped. Rationale:
    /// the `Suggested` order was computed without the pending mods, so
    /// writing `Current` plus them appended would silently ship an
    /// un-analysed order — a
    /// [`crate::use_cases::Rescan`] is one click away and already has a
    /// progress UI.
    #[error(
        "the working set has {added} pending activation(s) and {removed} pending \
         deactivation(s) not yet reflected in the current scan; rescan before writing \
         ModsConfig.xml"
    )]
    StaleActiveSet {
        /// How many mods were activated since the last scan.
        added: usize,
        /// How many mods were deactivated since the last scan.
        removed: usize,
    },
}

/// What to write on this apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyOptions {
    /// The order written to `ModsConfig.xml`. Named by the caller, never read
    /// from [`Session::selected`], so the order a user confirmed is the one
    /// that lands even if the session's selection has since moved.
    pub source: OrderSource,
    /// Write the `source` order to `ModsConfig.xml`, with a backup.
    pub write_mods_config: bool,
    /// Render the generated merge mod into the game's `Mods/` folder (or
    /// remove it when no `Merge`/`ShipAsset` decision remains), atomically.
    pub write_merge_mod: bool,
}

/// What [`Apply::execute`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOutcome {
    /// The backup path `ModsConfig.xml` was copied to, if it was written.
    pub backup_path: Option<PathBuf>,
    /// Whether `ModsConfig.xml` was written at all.
    pub wrote_mods_config: bool,
    /// The generated merge mod's folder, if [`ApplyOptions::write_merge_mod`]
    /// resulted in a write.
    pub merge_mod_path: Option<PathBuf>,
    /// The previous generation's backup path, if one existed to back up.
    pub merge_mod_backup_path: Option<PathBuf>,
    /// `Merge` decisions skipped because their preview wasn't
    /// [`MergeState::Complete`], and `ShipAsset` decisions skipped
    /// because their mod or texture couldn't be found — the merge mod was
    /// still written (or removed) without them.
    pub skipped_merges: Vec<FindingKey>,
}

/// Persists decisions and rules; when asked, writes the selected order to
/// `ModsConfig.xml` (with a timestamped backup) and/or renders the
/// generated merge mod.
pub struct Apply<Config, Decisions, Rules, Writer, Reader, Assets> {
    config_store: Config,
    decision_store: Decisions,
    rule_store: Rules,
    merge_writer: Writer,
    renderer: RenderMergeMod<Reader, Assets>,
}

impl<Config, Decisions, Rules, Writer, Reader, Assets>
    Apply<Config, Decisions, Rules, Writer, Reader, Assets>
where
    Config: ModsConfigStore,
    Decisions: DecisionStore,
    Rules: RuleStore,
    Writer: MergeModWriter,
    Reader: DefSourceReader,
    Assets: AssetLocator,
{
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(
        config_store: Config,
        decision_store: Decisions,
        rule_store: Rules,
        merge_writer: Writer,
        reader: Reader,
        asset_locator: Assets,
    ) -> Self {
        Self {
            config_store,
            decision_store,
            rule_store,
            merge_writer,
            renderer: RenderMergeMod::new(reader, asset_locator),
        }
    }

    /// # Errors
    ///
    /// See [`ApplyError`].
    pub fn execute(
        &self,
        session: &mut Session,
        options: ApplyOptions,
    ) -> Result<ApplyOutcome, ApplyError> {
        self.decision_store
            .save(&session.paths().profile_dir, session.decisions())
            .map_err(ApplyError::Decisions)?;
        self.rule_store
            .save(&session.paths().profile_dir, session.rules())
            .map_err(ApplyError::Rules)?;

        let mut merge_mod_path = None;
        let mut merge_mod_backup_path = None;
        let mut skipped_merges = Vec::new();

        if options.write_merge_mod {
            let mods_dir = session.paths().game_dir.join("Mods");
            let backup_dir = session.paths().profile_dir.join(MERGE_MOD_BACKUP_DIR);
            let (rendered, skipped) = self.render_merge_mod(session)?;
            skipped_merges = skipped;
            match rendered {
                Some(rendered) => {
                    let report = self.merge_writer.write(&mods_dir, &backup_dir, &rendered)?;
                    merge_mod_path = Some(report.mod_path);
                    merge_mod_backup_path = report.backup_path;
                }
                None => {
                    let identity =
                        GeneratedModIdentity::for_profile(session.paths().profile_hash());
                    self.merge_writer
                        .remove(&mods_dir, &backup_dir, &identity.folder_name)?;
                }
            }
        }

        let backup_path = if options.write_mods_config {
            if session.is_stale() {
                let pending = session.pending_changes();
                return Err(ApplyError::StaleActiveSet {
                    added: pending.unscanned.added.len(),
                    removed: pending.unscanned.removed.len(),
                });
            }
            let mut order = session.orders().get(options.source).as_slice().to_vec();
            if options.write_merge_mod {
                let own_merge_mod = session.own_merge_mod_id();
                if merge_mod_path.is_some() {
                    if !order.contains(&own_merge_mod) {
                        order.push(own_merge_mod);
                    }
                } else {
                    order.retain(|id| *id != own_merge_mod);
                }
            }
            let load_order = rim_analyzer::domain::LoadOrder::new(order);
            let file = session.mods_config_file_for(&load_order);
            let backup = self
                .config_store
                .write_with_backup(&session.paths().mods_config, &file)?;
            // The file on disk now *is* `load_order` regardless of which
            // source was selected (`Current` or `Suggested`) — keep the
            // in-memory session in sync so a later apply (or `ExportPatch`
            // install) doesn't read a stale `current` and write it back,
            // silently undoing this one (`Session::set_current_order`'s
            // own doc comment).
            session.set_current_order(load_order);
            Some(backup)
        } else {
            None
        };

        Ok(ApplyOutcome {
            backup_path,
            wrote_mods_config: options.write_mods_config,
            merge_mod_path,
            merge_mod_backup_path,
            skipped_merges,
        })
    }

    /// Delegates to [`RenderMergeMod`] — `None` when there's nothing to
    /// render at all (no complete merge and no located asset).
    fn render_merge_mod(
        &self,
        session: &mut Session,
    ) -> Result<(Option<rim_merge::emit::RenderedMod>, Vec<FindingKey>), ApplyError> {
        let render = self.renderer.execute(session)?;
        Ok((render.rendered, render.skipped))
    }
}

impl From<RenderMergeModError> for ApplyError {
    fn from(error: RenderMergeModError) -> Self {
        match error {
            RenderMergeModError::Plan(error) => Self::Plan(error),
            RenderMergeModError::Asset(error) => Self::Asset(error),
            RenderMergeModError::Emit(error) => Self::Emit(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, Decision, DefKey};

    use super::*;
    use crate::ports::ModsConfigFile;
    use crate::test_support::{
        FakeAssetLocator, InMemoryDecisionStore, InMemoryDefSourceReader, InMemoryMergeModWriter,
        InMemoryModsConfigStore, InMemoryRuleStore, bionic_heart_fixture_flat, session_fixture,
        session_with_sources,
    };

    fn config_store() -> InMemoryModsConfigStore {
        InMemoryModsConfigStore::new(ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a")],
            known_expansions: Vec::new(),
        })
    }

    fn use_case(
        store: InMemoryModsConfigStore,
    ) -> Apply<
        InMemoryModsConfigStore,
        InMemoryDecisionStore,
        InMemoryRuleStore,
        InMemoryMergeModWriter,
        InMemoryDefSourceReader,
        FakeAssetLocator,
    > {
        Apply::new(
            store,
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryMergeModWriter::new(),
            InMemoryDefSourceReader::default(),
            FakeAssetLocator::default(),
        )
    }

    #[test]
    fn always_saves_decisions_and_rules() {
        let mut session = session_fixture(&["a"]);
        let use_case = use_case(config_store());

        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: false,
                    write_merge_mod: false,
                },
            )
            .expect("apply must succeed");

        assert!(!outcome.wrote_mods_config);
        assert!(outcome.backup_path.is_none());
        assert!(use_case.decision_store.last_saved().is_some());
        assert!(use_case.rule_store.last_saved().is_some());
    }

    #[test]
    fn writes_mods_config_only_when_asked() {
        let mut session = session_fixture(&["a"]);
        let use_case = use_case(config_store());

        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: true,
                    write_merge_mod: false,
                },
            )
            .expect("apply must succeed");

        assert!(outcome.wrote_mods_config);
        assert!(outcome.backup_path.is_some());
        assert_eq!(use_case.config_store.writes().len(), 1);
        assert!(outcome.merge_mod_path.is_none());
    }

    #[test]
    fn writing_the_merge_mod_with_no_merge_decisions_writes_nothing() {
        let mut session = session_fixture(&["a"]);
        let use_case = use_case(config_store());

        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: false,
                    write_merge_mod: true,
                },
            )
            .expect("apply must succeed");

        assert!(outcome.merge_mod_path.is_none());
        assert!(use_case.merge_writer.writes().is_empty());
    }

    fn merge_finding_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "HediffDef".to_string(),
                def_name: "BionicHeart".to_string(),
            },
            owners: [
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork"),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn a_complete_merge_decision_renders_and_writes_the_merge_mod() {
        // `bionic_heart_fixture_flat`, not `bionic_heart_fixture`: this
        // test is about the apply pipeline, not the structural guard — see the flat
        // fixture's own doc comment.
        let fixture = bionic_heart_fixture_flat();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        session
            .decide(Decision {
                key: merge_finding_key(),
                action: Action::Merge {
                    key: DefKey {
                        def_type: "HediffDef".to_string(),
                        def_name: "BionicHeart".to_string(),
                    },
                    choices: std::collections::BTreeMap::new(),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("merge is always a valid action");

        let use_case = Apply::new(
            config_store(),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryMergeModWriter::new(),
            fixture.reader,
            FakeAssetLocator::default(),
        );

        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: false,
                    write_merge_mod: true,
                },
            )
            .expect("apply must succeed");

        assert!(outcome.merge_mod_path.is_some());
        assert!(outcome.skipped_merges.is_empty());
        assert_eq!(use_case.merge_writer.writes().len(), 1);
    }

    /// A working set with a
    /// pending, unscanned activation refuses to write `ModsConfig.xml` —
    /// but still saves decisions and rules.
    #[test]
    fn a_stale_active_set_refuses_to_write_mods_config_but_still_saves_decisions_and_rules() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .inactive("b")
            .build();
        let mut session = Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let inventory = crate::mod_inventory::ModInventory::from_report(session.report());
        session
            .working_mut()
            .activate(vec![ModId::new("b")], &inventory)
            .expect("b is known");
        assert!(session.is_stale());

        let use_case = use_case(config_store());

        let error = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: true,
                    write_merge_mod: false,
                },
            )
            .expect_err("a stale active set must refuse the write");

        assert_eq!(
            error,
            ApplyError::StaleActiveSet {
                added: 1,
                removed: 0
            }
        );
        assert!(use_case.decision_store.last_saved().is_some());
        assert!(use_case.rule_store.last_saved().is_some());
        assert!(use_case.config_store.writes().is_empty());
    }

    /// The negative: the staleness gate guards the `ModsConfig.xml`
    /// write specifically, not `apply` as a whole — a stale active set
    /// with `write_mods_config: false` proceeds normally (decisions/rules
    /// only), and staleness itself is untouched by that call.
    #[test]
    fn a_stale_active_set_does_not_block_an_apply_that_never_asked_to_write_mods_config() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .inactive("b")
            .build();
        let mut session = Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let inventory = crate::mod_inventory::ModInventory::from_report(session.report());
        session
            .working_mut()
            .activate(vec![ModId::new("b")], &inventory)
            .expect("b is known");
        assert!(session.is_stale());

        let use_case = use_case(config_store());

        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: false,
                    write_merge_mod: false,
                },
            )
            .expect("write_mods_config: false must never be refused for staleness");

        assert!(!outcome.wrote_mods_config);
        assert!(use_case.config_store.writes().is_empty());
        assert!(
            session.is_stale(),
            "this call neither rescanned nor otherwise changed the working set"
        );
    }

    /// The escape hatch: once a
    /// [`crate::use_cases::Rescan`] has caught the working set up, apply
    /// writes exactly it.
    #[test]
    fn after_a_rescan_apply_writes_exactly_the_working_set() {
        let paths = crate::ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        };
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .inactive("b")
            .build();
        let mut session = Session::new(
            paths.clone(),
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let inventory = crate::mod_inventory::ModInventory::from_report(session.report());
        session
            .working_mut()
            .activate(vec![ModId::new("b")], &inventory)
            .expect("b is known");

        let scanner = crate::test_support::FakeScanner::new(
            rim_resolve::test_support::ReportBuilder::new()
                .mod_("a")
                .mod_("b")
                .build(),
            Vec::new(),
        );
        let load_project = crate::use_cases::LoadProject::new(
            scanner,
            InMemoryModsConfigStore::new(ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            crate::test_support::InMemoryPatchProjectStore::new(),
            crate::test_support::InMemoryAssignmentProjectStore::new(),
            crate::test_support::FakeModKnowledgeStore::empty(),
            true,
        );
        let rescan = crate::use_cases::Rescan::new(load_project);
        let mut session = rescan
            .execute(&session, &mut |_| {})
            .expect("rescan must succeed");
        assert!(!session.is_stale());

        let use_case = use_case(config_store());
        let outcome = use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Current,
                    write_mods_config: true,
                    write_merge_mod: false,
                },
            )
            .expect("a rescanned session is not stale");

        assert!(outcome.wrote_mods_config);
        let writes = use_case.config_store.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            writes[0].active_mods,
            vec![ModId::new("a"), ModId::new("b")]
        );
    }

    #[test]
    fn apply_writes_the_source_named_in_options_not_the_selected_one() {
        let mut session = session_fixture(&["a", "b"]);
        session
            .decide(Decision {
                key: rim_resolve::domain::FindingKey::UndeclaredHardDependency {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                action: Action::Reorder {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("reorder is always a valid action");
        let suggested = session.orders().suggested.as_slice().to_vec();
        assert_ne!(
            suggested,
            session.orders().current.as_slice(),
            "sanity check: the two orders must differ for this test to mean anything"
        );
        assert_eq!(session.selected(), OrderSource::Current);
        let store = InMemoryModsConfigStore::new(ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a"), ModId::new("b")],
            known_expansions: Vec::new(),
        });
        let use_case = use_case(store);

        use_case
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Suggested,
                    write_mods_config: true,
                    write_merge_mod: false,
                },
            )
            .expect("apply must succeed");

        let writes = use_case.config_store.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].active_mods, suggested);
    }

    #[test]
    fn file_matches_is_true_after_applying_that_source() {
        let mut session = session_fixture(&["a", "b"]);
        session
            .decide(Decision {
                key: rim_resolve::domain::FindingKey::UndeclaredHardDependency {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                action: Action::Reorder {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("reorder is always a valid action");
        assert!(
            !session.file_matches(OrderSource::Suggested),
            "sanity check: the file starts out holding the current order"
        );
        let store = InMemoryModsConfigStore::new(ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a"), ModId::new("b")],
            known_expansions: Vec::new(),
        });

        use_case(store)
            .execute(
                &mut session,
                ApplyOptions {
                    source: OrderSource::Suggested,
                    write_mods_config: true,
                    write_merge_mod: false,
                },
            )
            .expect("apply must succeed");

        assert!(session.file_matches(OrderSource::Suggested));
    }
}
