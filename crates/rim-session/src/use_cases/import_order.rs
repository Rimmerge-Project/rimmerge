//! [`ImportOrder`]: turns an import preview's order into a scan, without
//! ever staging it in [`Session::working`].
//!
//! Two calls bracket the composition root's scan, the same split the
//! desktop's rescan already uses:
//!
//! 1. [`ImportOrder::validate`] checks the order against the live
//!    session's inventory and returns a [`ValidatedImport`].
//! 2. The root scans with [`ValidatedImport::order`] through
//!    [`crate::use_cases::LoadProject::execute_with_active_set`], which
//!    makes the list's order the new session's `orders.current`.
//! 3. [`ImportOrder::finish`] re-checks that Current against the
//!    [`ValidatedImport`] and selects it on that new session.
//!
//! Either the whole import lands (a new session whose Current is the
//! imported order) or nothing changes. Putting a same-set reorder into
//! `working` would not do: `is_stale` is set-based, so a failed or
//! cancelled rescan would leave `Apply` free to write the old order with
//! no warning.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::OrderSource;

use crate::Session;
use crate::active_set::{ActiveSet, ActiveSetError, is_core};
use crate::mod_inventory::ModInventory;
use crate::mod_list::{ImportPlan, ImportedEntry, MissingKind};

/// An order that passed [`ImportOrder::validate`]: every id is an
/// installed mod of the session it was checked against, none repeats, and
/// Core is in it. Only [`ImportOrder::validate`] builds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedImport {
    order: Vec<ModId>,
}

impl ValidatedImport {
    /// The validated order.
    #[must_use]
    pub fn order(&self) -> &[ModId] {
        &self.order
    }
}

/// Why an order cannot be imported. It can only happen when the inventory
/// changed between preview and import, or the caller is buggy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportOrderError {
    /// An id names no installed mod.
    #[error("{0} is not an installed mod")]
    Unknown(ModId),
    /// An id appears more than once.
    #[error("{0} appears more than once")]
    Duplicate(ModId),
    /// The order has no Core, whether the list omitted it and none is
    /// installed (`CorePlacement::Missing`) or the caller dropped it.
    #[error("the order does not include Core")]
    CoreMissing,
    /// The scanned session's Current is not the validated order: the scan
    /// ran over something else, or the session was swapped in between.
    #[error("the scanned session does not hold the validated order")]
    ScanDidNotMatch,
}

/// Validates an import order and selects it once scanned.
#[derive(Debug, Clone, Copy)]
pub struct ImportOrder;

impl ImportOrder {
    /// Checks `order` against `session`'s inventory.
    ///
    /// The result is only as fresh as `session`: the composition root must
    /// validate, scan and swap the new session in under the same
    /// `load_lock`, and refuse the swap if `working` changed meanwhile.
    /// [`Self::finish`] re-checks the scanned session against the order.
    ///
    /// # Errors
    ///
    /// See [`ImportOrderError`]. An id that is known but not on disk (a
    /// missing mod) is [`ImportOrderError::Unknown`]: a scan cannot
    /// activate what is not installed.
    pub fn validate(
        session: &Session,
        order: Vec<ModId>,
    ) -> Result<ValidatedImport, ImportOrderError> {
        Self::validate_order(&ModInventory::from_report(session.report()), order)
    }

    /// [`Self::validate`] against an inventory alone, for an interface with
    /// no loaded session (the CLI's discovery-only import). One rule for
    /// both: every id installed, none repeated, Core present.
    ///
    /// # Errors
    ///
    /// See [`ImportOrderError`]; never [`ImportOrderError::ScanDidNotMatch`].
    pub fn validate_order(
        inventory: &ModInventory,
        order: Vec<ModId>,
    ) -> Result<ValidatedImport, ImportOrderError> {
        let set = ActiveSet::new(order, inventory).map_err(|error| match error {
            ActiveSetError::Unknown(id) => ImportOrderError::Unknown(id),
            ActiveSetError::Duplicate(id) => ImportOrderError::Duplicate(id),
        })?;
        if let Some(absent) = set.ids().iter().find(|id| {
            inventory
                .entry(id)
                .is_none_or(|entry| !entry.present_on_disk)
        }) {
            return Err(ImportOrderError::Unknown(absent.clone()));
        }
        if !set.ids().iter().any(is_core) {
            return Err(ImportOrderError::CoreMissing);
        }
        Ok(ValidatedImport {
            order: set.ids().to_vec(),
        })
    }

    /// Selects `Current` on the session the scan returned: the imported
    /// order is Current, and under Suggested `Apply` would write a re-sort
    /// and drop the order being shared.
    ///
    /// # Errors
    ///
    /// [`ImportOrderError::ScanDidNotMatch`] when `new_session`'s Current
    /// is not `imported`'s order; the selection is left unchanged.
    pub fn finish(
        new_session: &mut Session,
        imported: &ValidatedImport,
    ) -> Result<(), ImportOrderError> {
        if new_session.orders().current.as_slice() != imported.order() {
            return Err(ImportOrderError::ScanDidNotMatch);
        }
        new_session.select(OrderSource::Current);
        Ok(())
    }
}

/// What a lossy import costs, for an interface that must get the user's
/// consent before it writes (the CLI's `--yes`). Derived from the plan,
/// never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLoss {
    /// Active mods the import deactivates.
    pub deactivated: usize,
    /// Listed mods that are not installed and so cannot be activated
    /// (the sender's merge mod is not counted: it is never needed).
    pub not_installed: usize,
}

impl ImportLoss {
    /// The loss `plan` carries, or `None` when it deactivates nothing and
    /// every listed mod is installed (duplicates and other-copy matches
    /// lose nothing).
    #[must_use]
    pub fn of(plan: &ImportPlan) -> Option<Self> {
        let not_installed = plan
            .entries
            .iter()
            .filter(|entry| match entry {
                // The sender's merge mod is expected to be absent and is
                // never activated, so it is not a loss.
                ImportedEntry::NotInstalled { kind, .. } => match kind {
                    MissingKind::Workshop(_) | MissingKind::Dlc | MissingKind::NoLink => true,
                    MissingKind::RimmergeMergeMod => false,
                },
                ImportedEntry::AlreadyActive { .. }
                | ImportedEntry::Activated { .. }
                | ImportedEntry::MatchedOtherCopy { .. }
                | ImportedEntry::Duplicate { .. } => false,
            })
            .count();
        let deactivated = plan.deactivated.len();
        (deactivated > 0 || not_installed > 0).then_some(Self {
            deactivated,
            not_installed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectPaths;
    use crate::mod_list::{CorePlacement, ImportContext, parse_text, plan_import};
    use crate::ports::ModsConfigFile;
    use crate::test_support::{
        FakeModKnowledgeStore, FakeScanner, InMemoryAssignmentProjectStore, InMemoryDecisionStore,
        InMemoryModsConfigStore, InMemoryPatchProjectStore, InMemoryRuleStore,
    };
    use crate::use_cases::LoadProject;
    use rim_resolve::test_support::ReportBuilder;

    fn paths() -> ProjectPaths {
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        }
    }

    fn ids(raws: &[&str]) -> Vec<ModId> {
        raws.iter().map(ModId::new).collect()
    }

    /// Core and `example.base` active (in the file), `example.extra`
    /// inactive, `example.gone` known but missing from disk.
    fn session() -> Session {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.base")
            .inactive("example.extra")
            .missing_mod("example.gone")
            .build();
        Session::new(
            paths(),
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: ids(&["ludeon.rimworld", "example.base"]),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    }

    fn load_project(
        scanned: &[&str],
        file: &[&str],
    ) -> LoadProject<
        FakeScanner,
        InMemoryModsConfigStore,
        InMemoryDecisionStore,
        InMemoryRuleStore,
        InMemoryPatchProjectStore,
        InMemoryAssignmentProjectStore,
        FakeModKnowledgeStore,
    > {
        let mut builder = ReportBuilder::new().core("ludeon.rimworld");
        for id in scanned.iter().filter(|id| **id != "ludeon.rimworld") {
            builder = builder.mod_(id);
        }
        LoadProject::new(
            FakeScanner::new(builder.build(), Vec::new()),
            InMemoryModsConfigStore::new(ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: ids(file),
                known_expansions: Vec::new(),
            }),
            InMemoryDecisionStore::new(),
            InMemoryRuleStore::new(),
            InMemoryPatchProjectStore::new(),
            InMemoryAssignmentProjectStore::new(),
            FakeModKnowledgeStore::empty(),
            true,
        )
    }

    #[test]
    fn validate_accepts_an_installed_order_and_keeps_it_as_given() {
        let order = ids(&["example.extra", "ludeon.rimworld", "example.base"]);

        let validated = ImportOrder::validate(&session(), order.clone()).expect("valid");

        assert_eq!(validated.order(), order.as_slice());
        assert_eq!(validated.order(), order.as_slice());
    }

    #[test]
    fn validate_rejects_an_unknown_id() {
        let error = ImportOrder::validate(&session(), ids(&["ludeon.rimworld", "example.nope"]))
            .expect_err("unknown");

        assert_eq!(error, ImportOrderError::Unknown(ModId::new("example.nope")));
    }

    #[test]
    fn validate_rejects_a_known_id_that_is_not_on_disk() {
        let error = ImportOrder::validate(&session(), ids(&["ludeon.rimworld", "example.gone"]))
            .expect_err("missing mod");

        assert_eq!(error, ImportOrderError::Unknown(ModId::new("example.gone")));
    }

    #[test]
    fn validate_rejects_a_repeated_id() {
        let error = ImportOrder::validate(
            &session(),
            ids(&["ludeon.rimworld", "example.base", "example.base"]),
        )
        .expect_err("duplicate");

        assert_eq!(
            error,
            ImportOrderError::Duplicate(ModId::new("example.base"))
        );
    }

    #[test]
    fn validate_rejects_an_order_without_core() {
        let error = ImportOrder::validate(&session(), ids(&["example.base"])).expect_err("no Core");

        assert_eq!(error, ImportOrderError::CoreMissing);
    }

    #[test]
    fn validate_rejects_the_order_of_a_plan_that_has_no_installed_core() {
        let report = ReportBuilder::new().mod_("example.base").build();
        let inventory = ModInventory::from_report(&report);
        let list = parse_text("ludeon.rimworld\nexample.base\n")
            .expect("parses")
            .list;
        let plan = plan_import(
            &list,
            &inventory,
            &ids(&["example.base"]),
            ImportContext {
                own_merge_mod: ModId::new("rimmerge.merge.3f9a1c2b7d5e"),
                game_version: None,
                has_pending_changes: false,
            },
        );
        assert_eq!(plan.core, CorePlacement::Missing);
        let session = Session::new(
            paths(),
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: ids(&["example.base"]),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );

        let error = ImportOrder::validate(&session, plan.order).expect_err("refused");

        assert_eq!(error, ImportOrderError::CoreMissing);
    }

    #[test]
    fn the_scanned_session_has_the_import_as_current_and_is_not_stale() {
        let session = session();
        let order = ids(&["example.extra", "ludeon.rimworld", "example.base"]);
        let validated = ImportOrder::validate(&session, order.clone()).expect("valid");
        let load = load_project(
            &["example.extra", "ludeon.rimworld", "example.base"],
            &["ludeon.rimworld", "example.base"],
        );

        let mut scanned = load
            .execute_with_active_set(paths(), validated.order().to_vec(), &mut |_| {})
            .expect("scan succeeds");
        ImportOrder::finish(&mut scanned, &validated).expect("matches");

        assert_eq!(scanned.orders().current.as_slice(), order.as_slice());
        assert!(!scanned.is_stale(), "working agrees with the scanned order");
        assert_eq!(
            scanned.file_active_mods(),
            ids(&["ludeon.rimworld", "example.base"]),
            "the file's own list is kept, so Apply sees the import as unapplied"
        );
        assert!(!scanned.file_matches(OrderSource::Current));
        assert!(scanned.pending_changes().unscanned.is_empty());
        assert_eq!(scanned.selected(), OrderSource::Current);
    }

    #[test]
    fn finish_refuses_a_session_whose_current_is_not_the_validated_order() {
        let mut scanned = session();
        scanned.select(OrderSource::Suggested);
        let validated = ImportOrder::validate(&scanned, ids(&["example.base", "ludeon.rimworld"]))
            .expect("valid");

        let error = ImportOrder::finish(&mut scanned, &validated).expect_err("mismatch");

        assert_eq!(error, ImportOrderError::ScanDidNotMatch);
        assert_eq!(scanned.selected(), OrderSource::Suggested);
    }

    #[test]
    fn finish_selects_current_when_the_scan_matches_even_after_suggested() {
        let mut scanned = session();
        scanned.select(OrderSource::Suggested);
        let validated = ImportOrder::validate(&scanned, ids(&["ludeon.rimworld", "example.base"]))
            .expect("valid");

        ImportOrder::finish(&mut scanned, &validated).expect("matches");

        assert_eq!(scanned.selected(), OrderSource::Current);
    }

    #[test]
    fn importing_your_own_export_keeps_the_own_merge_mod_and_changes_nothing() {
        let own_id = rim_resolve::domain::GeneratedModIdentity::for_profile(paths().profile_hash())
            .package_id;
        let active = ["ludeon.rimworld", "example.base", own_id.as_str()];
        let session = crate::test_support::session_fixture_with_generated(&active, own_id.as_str());
        let preview = crate::use_cases::ImportPreview::from_text(
            "ludeon.rimworld
example.base
",
            &crate::use_cases::ImportTarget::from_session(&session),
        );
        let crate::use_cases::ImportPreview::Ready(ready) = preview else {
            panic!("expected a ready preview");
        };
        let validated = ImportOrder::validate(&session, ready.plan.order).expect("valid");
        let load = load_project(&active, &active);

        let mut scanned = load
            .execute_with_active_set(paths(), validated.order().to_vec(), &mut |_| {})
            .expect("scan succeeds");
        ImportOrder::finish(&mut scanned, &validated).expect("matches");

        assert!(scanned.orders().current.as_slice().contains(&own_id));
        assert!(
            scanned.pending_changes().unapplied.is_empty(),
            "a self round trip is an empty diff, merge mod included"
        );
    }

    fn plan_for(list_text: &str, active: &[&str], inactive: &[&str]) -> ImportPlan {
        let mut builder = ReportBuilder::new().core("ludeon.rimworld");
        for id in active {
            builder = builder.mod_(id);
        }
        for id in inactive {
            builder = builder.inactive(id);
        }
        let file: Vec<ModId> = std::iter::once("ludeon.rimworld")
            .chain(active.iter().copied())
            .map(ModId::new)
            .collect();
        plan_import(
            &parse_text(list_text).expect("parses").list,
            &ModInventory::from_report(&builder.build()),
            &file,
            ImportContext {
                own_merge_mod: ModId::new("rimmerge.merge.3f9a1c2b7d5e"),
                game_version: None,
                has_pending_changes: false,
            },
        )
    }

    #[test]
    fn a_lossless_import_needs_no_confirmation() {
        let plan = plan_for(
            "example.extra\nludeon.rimworld\nexample.base\n",
            &["example.base"],
            &["example.extra"],
        );

        assert_eq!(ImportLoss::of(&plan), None);
    }

    #[test]
    fn deactivating_a_mod_needs_confirmation() {
        let plan = plan_for("ludeon.rimworld\n", &["example.base"], &[]);

        assert_eq!(
            ImportLoss::of(&plan),
            Some(ImportLoss {
                deactivated: 1,
                not_installed: 0
            })
        );
    }

    #[test]
    fn a_not_installed_entry_needs_confirmation_even_when_nothing_is_deactivated() {
        let plan = plan_for(
            "ludeon.rimworld\nexample.base\nghost.mod\n",
            &["example.base"],
            &[],
        );

        assert_eq!(
            ImportLoss::of(&plan),
            Some(ImportLoss {
                deactivated: 0,
                not_installed: 1
            })
        );
    }

    #[test]
    fn the_senders_merge_mod_alone_is_not_a_loss() {
        let plan = plan_for(
            "ludeon.rimworld
example.base
rimmerge.merge.aaaaaaaaaaaa
",
            &["example.base"],
            &[],
        );
        assert!(plan.entries.iter().any(|entry| matches!(
            entry,
            ImportedEntry::NotInstalled {
                kind: MissingKind::RimmergeMergeMod,
                ..
            }
        )));

        assert_eq!(ImportLoss::of(&plan), None);
    }

    #[test]
    fn a_missing_dlc_still_counts_as_a_loss() {
        let plan = plan_for(
            "ludeon.rimworld
example.base
ludeon.rimworld.royalty
",
            &["example.base"],
            &[],
        );

        assert_eq!(
            ImportLoss::of(&plan),
            Some(ImportLoss {
                deactivated: 0,
                not_installed: 1
            })
        );
    }

    #[test]
    fn a_duplicate_listing_alone_loses_nothing() {
        let plan = plan_for(
            "ludeon.rimworld\nexample.base\nexample.base\n",
            &["example.base"],
            &[],
        );

        assert_eq!(ImportLoss::of(&plan), None);
    }

    #[test]
    fn validate_order_applies_the_same_rule_without_a_session() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.framework")
            .build();
        let inventory = ModInventory::from_report(&report);
        let ids = |raws: &[&str]| raws.iter().map(ModId::new).collect::<Vec<_>>();

        let ok =
            ImportOrder::validate_order(&inventory, ids(&["ludeon.rimworld", "example.framework"]));
        let no_core = ImportOrder::validate_order(&inventory, ids(&["example.framework"]));
        let unknown = ImportOrder::validate_order(&inventory, ids(&["ludeon.rimworld", "no.such"]));

        assert_eq!(ok.expect("valid").order().len(), 2);
        assert_eq!(no_core, Err(ImportOrderError::CoreMissing));
        assert_eq!(
            unknown,
            Err(ImportOrderError::Unknown(ModId::new("no.such")))
        );
    }
}
