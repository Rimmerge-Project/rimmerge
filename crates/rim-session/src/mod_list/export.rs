//! Building a shareable list from the receiver's own active order.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use thiserror::Error;

use super::{
    ListedGameVersion, ListedName, ListedPackageId, SharedModEntry, SharedModList,
    SharedModListError, WorkshopId,
};
use crate::mod_inventory::{InventoryEntry, ModInventory};

/// A list built from the active order, plus the active ids that could not
/// be put in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedModList {
    /// The shareable list.
    pub list: SharedModList,
    /// Active ids that fail the package-id grammar, in file order: the
    /// caller reports them rather than dropping them silently.
    pub unrepresentable: Vec<ModId>,
}

/// Why no list could be built from the active order.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExportListError {
    /// No active mod remains once the own merge mod is left out.
    #[error("no active mods to export")]
    NothingActive,
    /// Every active mod's id fails the package-id grammar.
    #[error("none of the active mod ids can be exported")]
    NothingRepresentable {
        /// The ids that were refused, in file order.
        unrepresentable: Vec<ModId>,
    },
    /// More active mods than a list may hold.
    #[error("a shared list holds at most {limit} mods")]
    TooManyEntries {
        /// [`super::ModListLimits::MAX_ENTRIES`].
        limit: usize,
    },
}

impl SharedModList {
    /// Builds the list to share from `active`, the order in
    /// `ModsConfig.xml` in file order.
    ///
    /// Ids are exported as base ids (no `_steam`), once each: when a local
    /// copy and its Workshop copy are both active, the first one's facts
    /// are used. Names and Workshop ids come from `inventory`; a mod it
    /// does not know exports with its id as its name and no Workshop id.
    /// `own_merge_mod` (this machine's generated merge mod) is left out,
    /// since a receiver cannot get it.
    pub fn from_active(
        active: &[ModId],
        inventory: &ModInventory,
        own_merge_mod: &ModId,
        game_version: Option<&str>,
    ) -> Result<ExportedModList, ExportListError> {
        let own_base = own_merge_mod.base();
        let mut seen_bases = BTreeSet::new();
        let mut entries = Vec::with_capacity(active.len());
        let mut unrepresentable = Vec::new();
        for id in active {
            let base = id.base();
            if base == own_base || !seen_bases.insert(base) {
                continue;
            }
            match exported_entry(id, inventory) {
                Some(entry) => entries.push(entry),
                None => unrepresentable.push(id.clone()),
            }
        }
        let version = game_version.and_then(ListedGameVersion::new);
        match Self::new(version, entries) {
            Ok(list) => Ok(ExportedModList {
                list,
                unrepresentable,
            }),
            Err(SharedModListError::Empty) if unrepresentable.is_empty() => {
                Err(ExportListError::NothingActive)
            }
            Err(SharedModListError::Empty) => {
                Err(ExportListError::NothingRepresentable { unrepresentable })
            }
            Err(SharedModListError::TooManyEntries { limit }) => {
                Err(ExportListError::TooManyEntries { limit })
            }
        }
    }
}

fn exported_entry(id: &ModId, inventory: &ModInventory) -> Option<SharedModEntry> {
    let base = id.base();
    let known = inventory
        .entry(id)
        .or_else(|| inventory.find_by_base(&base).map(|(_, found)| found));
    let id = ListedPackageId::try_from(base.as_str()).ok()?;
    Some(SharedModEntry {
        name: ListedName::new(known.map_or(base.as_str(), |found| found.name.as_str())),
        workshop_id: known
            .and_then(|found: &InventoryEntry| found.workshop_id)
            .and_then(WorkshopId::new),
        id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rim_resolve::test_support::ReportBuilder;

    const OWN_MERGE: &str = "rimmerge.merge.3f9a1c2b7d5e";

    fn ids(raws: &[&str]) -> Vec<ModId> {
        raws.iter().map(ModId::new).collect()
    }

    fn inventory() -> ModInventory {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_with("example.framework", |m| {
                m.name = "Example Framework".to_string();
                m.workshop_id = Some(1_234_567_890);
            })
            .mod_("someone.localmod")
            .mod_(OWN_MERGE)
            .mod_with("dup.copy_steam", |m| m.workshop_id = Some(42))
            .build();
        ModInventory::from_report(&report)
    }

    fn export(active: &[&str], version: Option<&str>) -> ExportedModList {
        SharedModList::from_active(&ids(active), &inventory(), &ModId::new(OWN_MERGE), version)
            .expect("exports")
    }

    #[test]
    fn exports_names_workshop_ids_and_the_game_version_in_order() {
        let exported = export(
            &["ludeon.rimworld", "example.framework", "someone.localmod"],
            Some("1.6.4871 rev590"),
        );

        let entries = exported.list.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].id.as_str(), "example.framework");
        assert_eq!(
            entries[1].name.as_ref().map(ListedName::as_str),
            Some("Example Framework")
        );
        assert_eq!(
            entries[1].workshop_id.map(WorkshopId::get),
            Some(1_234_567_890)
        );
        assert_eq!(entries[2].workshop_id, None);
        assert_eq!(
            exported.list.game_version().map(ListedGameVersion::as_str),
            Some("1.6.4871 rev590")
        );
        assert!(exported.unrepresentable.is_empty());
    }

    #[test]
    fn leaves_out_the_own_merge_mod() {
        let exported = export(&["ludeon.rimworld", OWN_MERGE, "someone.localmod"], None);

        let exported_ids: Vec<_> = exported
            .list
            .entries()
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(exported_ids, vec!["ludeon.rimworld", "someone.localmod"]);
    }

    #[test]
    fn exports_base_ids_with_the_steam_copys_facts() {
        let exported = export(&["dup.copy_steam"], None);

        let only = &exported.list.entries()[0];
        assert_eq!(only.id.as_str(), "dup.copy");
        assert_eq!(only.workshop_id.map(WorkshopId::get), Some(42));
    }

    #[test]
    fn an_unknown_mod_exports_with_its_id_as_its_name_and_no_workshop_id() {
        let exported = export(&["stranger.unknown"], None);

        let only = &exported.list.entries()[0];
        assert_eq!(
            only.name.as_ref().map(ListedName::as_str),
            Some("stranger.unknown")
        );
        assert_eq!(only.workshop_id, None);
    }

    #[test]
    fn an_id_outside_the_grammar_is_reported_not_dropped_silently() {
        let exported = export(&["ludeon.rimworld", "nodot"], None);

        assert_eq!(exported.list.entries().len(), 1);
        assert_eq!(exported.unrepresentable, ids(&["nodot"]));
    }

    #[test]
    fn nothing_active_is_an_error() {
        let outcome = SharedModList::from_active(
            &ids(&[OWN_MERGE]),
            &inventory(),
            &ModId::new(OWN_MERGE),
            None,
        );

        assert_eq!(outcome, Err(ExportListError::NothingActive));
    }

    #[test]
    fn a_local_copy_and_its_workshop_copy_export_once() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("dup.pair")
            .mod_with("dup.pair_steam", |m| m.workshop_id = Some(77))
            .build();

        let exported = SharedModList::from_active(
            &ids(&["ludeon.rimworld", "dup.pair", "dup.pair_steam"]),
            &ModInventory::from_report(&report),
            &ModId::new(OWN_MERGE),
            None,
        )
        .expect("exports");

        let exported_ids: Vec<_> = exported
            .list
            .entries()
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(exported_ids, vec!["ludeon.rimworld", "dup.pair"]);
    }

    #[test]
    fn when_every_id_is_refused_the_error_names_them() {
        let outcome = SharedModList::from_active(
            &ids(&["nodot", "also bad"]),
            &inventory(),
            &ModId::new(OWN_MERGE),
            None,
        );

        assert_eq!(
            outcome,
            Err(ExportListError::NothingRepresentable {
                unrepresentable: ids(&["nodot", "also bad"])
            })
        );
    }

    #[test]
    fn importing_your_own_export_is_an_empty_diff() {
        use crate::mod_list::{ImportContext, ImportedEntry, plan_import};

        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.framework")
            .mod_("a.b_steam")
            .inactive("a.b")
            .mod_("someone.localmod")
            .mod_(OWN_MERGE)
            .build();
        let inventory = ModInventory::from_report(&report);
        let file = ids(&[
            "ludeon.rimworld",
            "example.framework",
            "a.b_steam",
            "someone.localmod",
            OWN_MERGE,
        ]);
        let own = ModId::new(OWN_MERGE);
        let exported = SharedModList::from_active(&file, &inventory, &own, None).expect("exports");

        let plan = plan_import(
            &exported.list,
            &inventory,
            &file,
            ImportContext {
                own_merge_mod: own,
                game_version: None,
                has_pending_changes: false,
            },
        );

        assert!(plan.deactivated.is_empty(), "{:?}", plan.deactivated);
        assert_eq!(plan.moved, 0);
        assert!(plan.entries.iter().all(|entry| matches!(
            entry,
            ImportedEntry::AlreadyActive { .. }
                | ImportedEntry::MatchedOtherCopy {
                    activation: crate::mod_list::Activation::AlreadyActive,
                    ..
                }
        )));
        assert_eq!(plan.order, file, "the own merge mod is kept too");
    }

    #[test]
    fn export_then_text_round_trips_ids_and_workshop_ids() {
        let exported = export(
            &["ludeon.rimworld", "example.framework", "someone.localmod"],
            Some("1.6.4871 rev590"),
        );

        let parsed = crate::mod_list::parse_text(&crate::mod_list::render_text(&exported.list))
            .expect("parses");

        assert_eq!(parsed.list, exported.list);
    }
}
