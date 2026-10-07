//! The import diff: what applying a shared list to this install would do.
//!
//! Pure and deterministic. The result is a closed sum type per listed
//! entry, so every consumer branches exhaustively.

use std::collections::{BTreeMap, BTreeSet};

use rim_resolve::domain::GeneratedModIdentity;

use rim_analyzer::domain::ModId;

use super::{ListedName, SharedModEntry, SharedModList, WorkshopId, count_as_u32};
use crate::active_set::is_core;
use crate::mod_inventory::ModInventory;

/// Core's package id: never deactivated, and put first when a list omits it.
pub(super) use crate::active_set::CORE_MOD_ID as CORE_PACKAGE_ID;
/// Every DLC's package id starts with this (Core's own id has no trailing dot).
pub(super) const DLC_PACKAGE_PREFIX: &str = "ludeon.rimworld.";

/// What the receiver's side contributes to a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportContext {
    /// The profile's own generated merge mod: never deactivated by an
    /// import, because `Apply` manages its presence and position.
    pub own_merge_mod: ModId,
    /// The installed game's version text, when known.
    pub game_version: Option<String>,
    /// Whether the session holds activation edits not yet rescanned
    /// (always `false` from the CLI, which has no working set).
    pub has_pending_changes: bool,
}

/// The diff of a shared list against this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    /// What an import scans with: the installed entries in imported order,
    /// by their exact inventory ids, deduplicated, with Core first if the
    /// list lacks it, then this profile's own merge mod at the end when the
    /// file has it.
    pub order: Vec<ModId>,
    /// One per listed entry, in list order.
    pub entries: Vec<ImportedEntry>,
    /// Active now (in the file) but not in `order`, in file order. Core and
    /// the own merge mod are never listed.
    pub deactivated: Vec<ModId>,
    /// Kept mods whose relative order differs from the file's.
    pub moved: u32,
    /// Whether the list named Core.
    pub core: CorePlacement,
    /// How the list's game version compares with the installed one.
    pub version: VersionCheck,
    /// A fact from the session: an import replaces unscanned edits.
    pub replaces_pending_changes: bool,
}

/// What the plan does with one listed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportedEntry {
    /// Installed under this exact id and already active.
    AlreadyActive {
        /// The exact inventory id.
        id: ModId,
    },
    /// Installed under this exact id and inactive: the import activates it.
    Activated {
        /// The exact inventory id.
        id: ModId,
    },
    /// Another copy of the same package (`_steam` or not) is used: the one
    /// already active, or, when none is, the installed copy found by base id.
    MatchedOtherCopy {
        /// The id as listed.
        listed: ModId,
        /// The exact inventory id of the copy that is used.
        installed: ModId,
        /// Whether that copy is active now.
        activation: Activation,
    },
    /// No copy is installed.
    NotInstalled {
        /// The id as listed.
        listed: ModId,
        /// The sender's name for it.
        name: Option<ListedName>,
        /// What the receiver can do about it.
        kind: MissingKind,
    },
    /// The same package listed again; the first position is used.
    Duplicate {
        /// The base id.
        id: ModId,
        /// 1-based position of the first listing.
        first_position: u32,
    },
}

impl ImportedEntry {
    /// The exact inventory id this entry puts into the order, if any.
    #[must_use]
    pub fn installed_id(&self) -> Option<&ModId> {
        match self {
            Self::AlreadyActive { id } | Self::Activated { id } => Some(id),
            Self::MatchedOtherCopy { installed, .. } => Some(installed),
            Self::NotInstalled { .. } | Self::Duplicate { .. } => None,
        }
    }
}

/// Whether a matched copy is active before the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Active already.
    AlreadyActive,
    /// Inactive: the import activates it.
    Activated,
}

/// Why a listed mod is missing, and so what the receiver can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingKind {
    /// A Workshop mod: it can be subscribed to.
    Workshop(WorkshopId),
    /// A DLC the receiver does not own.
    Dlc,
    /// Made by Rimmerge on the sender's machine: never needed, never
    /// activated.
    RimmergeMergeMod,
    /// The list carries no Workshop link for it.
    NoLink,
}

/// Where Core ends up in the planned order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorePlacement {
    /// Core was in the list; the list's position is kept.
    Listed,
    /// Core was not in the list and is put first.
    AddedFirst,
    /// No installed Core, so the order has none.
    Missing,
}

/// The list's game version against the installed one, by major.minor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    /// One side's version is missing or not recognizable.
    Unknown,
    /// Same major.minor (a build or revision difference does not count).
    Same,
    /// A different major.minor.
    Differs {
        /// The list's version text.
        listed: String,
        /// The installed version text.
        game: String,
    },
}

/// Diffs `list` against this install. `file_ids` is `ModsConfig.xml`'s
/// active list, by exact ids, in file order.
#[must_use]
pub fn plan_import(
    list: &SharedModList,
    inventory: &ModInventory,
    file_ids: &[ModId],
    context: ImportContext,
) -> ImportPlan {
    let active = ActiveIndex::new(file_ids, inventory);
    let (entries, mut order) = resolve_entries(list, inventory, &active);
    let core = place_core(&mut order, inventory);
    let deactivated = deactivated_ids(file_ids, &order, &context.own_merge_mod);
    // Counted before the merge mod is re-added: it is not the list's to
    // place, so it never counts as moved.
    let moved = count_moved(file_ids, &order);
    keep_own_merge_mod(&mut order, file_ids, inventory, &context.own_merge_mod);
    ImportPlan {
        moved,
        version: check_version(list, context.game_version.as_deref()),
        replaces_pending_changes: context.has_pending_changes,
        order,
        entries,
        deactivated,
        core,
    }
}

/// The file's active ids, indexed for resolution.
struct ActiveIndex<'a> {
    ids: BTreeSet<&'a ModId>,
    /// Base id -> the first active, on-disk copy sharing it (file order).
    present_by_base: BTreeMap<ModId, &'a ModId>,
}

impl<'a> ActiveIndex<'a> {
    fn new(file_ids: &'a [ModId], inventory: &ModInventory) -> Self {
        let mut present_by_base = BTreeMap::new();
        for id in file_ids {
            if inventory.entry(id).is_some_and(|e| e.present_on_disk) {
                present_by_base.entry(id.base()).or_insert(id);
            }
        }
        Self {
            ids: file_ids.iter().collect(),
            present_by_base,
        }
    }
}

fn resolve_entries(
    list: &SharedModList,
    inventory: &ModInventory,
    active: &ActiveIndex<'_>,
) -> (Vec<ImportedEntry>, Vec<ModId>) {
    let mut first_positions: BTreeMap<ModId, u32> = BTreeMap::new();
    let mut entries = Vec::with_capacity(list.entries().len());
    let mut order = Vec::new();
    for (index, entry) in list.entries().iter().enumerate() {
        let position = count_as_u32(index + 1);
        let base = entry.id.as_mod_id().base();
        if let Some(&first_position) = first_positions.get(&base) {
            entries.push(ImportedEntry::Duplicate {
                id: base,
                first_position,
            });
            continue;
        }
        first_positions.insert(base.clone(), position);
        let imported = resolve_entry(entry, &base, inventory, active);
        order.extend(imported.installed_id().cloned());
        entries.push(imported);
    }
    (entries, order)
}

fn resolve_entry(
    entry: &SharedModEntry,
    base: &ModId,
    inventory: &ModInventory,
    active: &ActiveIndex<'_>,
) -> ImportedEntry {
    let listed = entry.id.as_mod_id();
    // The sender's merge mod is never activated, even when this machine
    // has a generated mod of the same id.
    if GeneratedModIdentity::is_generated(listed) {
        return not_installed(entry, MissingKind::RimmergeMergeMod);
    }
    // A copy of this package that is already active is kept, whichever
    // spelling the list uses: re-importing your own export (base ids) must
    // not swap a `_steam` copy for a local one.
    if let Some(&active_copy) = active.present_by_base.get(base) {
        let listed_is_the_active_copy = active.ids.contains(listed)
            && inventory.entry(listed).is_some_and(|e| e.present_on_disk);
        return if listed_is_the_active_copy {
            ImportedEntry::AlreadyActive { id: listed.clone() }
        } else {
            ImportedEntry::MatchedOtherCopy {
                listed: listed.clone(),
                installed: active_copy.clone(),
                activation: Activation::AlreadyActive,
            }
        };
    }
    if inventory.entry(listed).is_some_and(|e| e.present_on_disk) {
        return ImportedEntry::Activated { id: listed.clone() };
    }
    if let Some((installed, found)) = inventory.find_by_base(base)
        && found.present_on_disk
    {
        return ImportedEntry::MatchedOtherCopy {
            listed: listed.clone(),
            installed: installed.clone(),
            activation: Activation::Activated,
        };
    }
    not_installed(entry, missing_kind(base, entry.workshop_id))
}

fn not_installed(entry: &SharedModEntry, kind: MissingKind) -> ImportedEntry {
    ImportedEntry::NotInstalled {
        listed: entry.id.as_mod_id().clone(),
        name: entry.name.clone(),
        kind,
    }
}

fn missing_kind(base: &ModId, workshop_id: Option<WorkshopId>) -> MissingKind {
    if base.as_str().starts_with(DLC_PACKAGE_PREFIX) {
        return MissingKind::Dlc;
    }
    match workshop_id {
        Some(id) => MissingKind::Workshop(id),
        None => MissingKind::NoLink,
    }
}

/// Puts Core first when the order lacks it and an installed Core exists.
fn place_core(order: &mut Vec<ModId>, inventory: &ModInventory) -> CorePlacement {
    if order.iter().any(is_core) {
        return CorePlacement::Listed;
    }
    match inventory
        .find_by_base(&ModId::new(CORE_PACKAGE_ID))
        .filter(|(_, found)| found.present_on_disk)
    {
        Some((core_id, _)) => {
            order.insert(0, core_id.clone());
            CorePlacement::AddedFirst
        }
        None => CorePlacement::Missing,
    }
}

/// Appends the receiver's own merge mod when the file has it: a list never
/// carries it, but the import must not drop it. The end is where `Apply`
/// puts it.
fn keep_own_merge_mod(
    order: &mut Vec<ModId>,
    file_ids: &[ModId],
    inventory: &ModInventory,
    own_merge_mod: &ModId,
) {
    let own_base = own_merge_mod.base();
    let already_kept = order.iter().any(|id| id.base() == own_base);
    if already_kept {
        return;
    }
    let in_file = file_ids
        .iter()
        .find(|id| id.base() == own_base && inventory.entry(id).is_some_and(|e| e.present_on_disk));
    order.extend(in_file.cloned());
}

fn deactivated_ids(file_ids: &[ModId], order: &[ModId], own_merge_mod: &ModId) -> Vec<ModId> {
    let kept: BTreeSet<&ModId> = order.iter().collect();
    let own_base = own_merge_mod.base();
    let mut seen: BTreeSet<&ModId> = BTreeSet::new();
    file_ids
        .iter()
        .filter(|id| !is_core(id) && id.base() != own_base && !kept.contains(id))
        .filter(|id| seen.insert(id))
        .cloned()
        .collect()
}

/// The kept mods (in both the file and the order) that fall outside the
/// longest run keeping the file's relative order.
fn count_moved(file_ids: &[ModId], order: &[ModId]) -> u32 {
    let mut file_index: BTreeMap<&ModId, usize> = BTreeMap::new();
    for (index, id) in file_ids.iter().enumerate() {
        file_index.entry(id).or_insert(index);
    }
    let kept_in_order: Vec<usize> = order
        .iter()
        .filter_map(|id| file_index.get(id).copied())
        .collect();
    let in_place = longest_increasing_run_length(&kept_in_order);
    count_as_u32(kept_in_order.len() - in_place)
}

/// Length of the longest strictly increasing subsequence (patience
/// sorting, `O(n log n)`).
fn longest_increasing_run_length(values: &[usize]) -> usize {
    let mut tails: Vec<usize> = Vec::new();
    for &value in values {
        let slot = tails.partition_point(|&tail| tail < value);
        match tails.get_mut(slot) {
            Some(tail) => *tail = value,
            None => tails.push(value),
        }
    }
    tails.len()
}

fn check_version(list: &SharedModList, game_version: Option<&str>) -> VersionCheck {
    let (Some(listed), Some(game)) = (list.game_version(), game_version) else {
        return VersionCheck::Unknown;
    };
    match (major_minor(listed.as_str()), major_minor(game)) {
        (Some(listed_pair), Some(game_pair)) if listed_pair == game_pair => VersionCheck::Same,
        (Some(_), Some(_)) => VersionCheck::Differs {
            listed: listed.as_str().to_string(),
            game: game.to_string(),
        },
        _ => VersionCheck::Unknown,
    }
}

/// The leading `major.minor` of a version text such as `1.6.4871 rev590`.
fn major_minor(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor_text = parts.next()?;
    let digits_end = minor_text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(minor_text.len());
    let minor = minor_text[..digits_end].parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_list::test_fixtures::{entry, list_of, named_entry};
    use crate::mod_list::{ListedGameVersion, ListedPackageId};
    use rim_resolve::test_support::ReportBuilder;

    const OWN_MERGE: &str = "rimmerge.merge.3f9a1c2b7d5e";

    fn id(raw: &str) -> ModId {
        ModId::new(raw)
    }

    fn ids(raws: &[&str]) -> Vec<ModId> {
        raws.iter().map(|raw| id(raw)).collect()
    }

    fn context() -> ImportContext {
        ImportContext {
            own_merge_mod: id(OWN_MERGE),
            game_version: None,
            has_pending_changes: false,
        }
    }

    fn inventory_of(report: &rim_analyzer::domain::Report) -> ModInventory {
        ModInventory::from_report(report)
    }

    /// Core and `example.framework` active; `someone.localmod` installed
    /// but inactive.
    fn standard_report() -> rim_analyzer::domain::Report {
        ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.framework")
            .inactive("someone.localmod")
            .build()
    }

    fn plan(list: &SharedModList, file: &[&str]) -> ImportPlan {
        plan_import(
            list,
            &inventory_of(&standard_report()),
            &ids(file),
            context(),
        )
    }

    #[test]
    fn an_active_installed_mod_is_already_active() {
        let plan = plan(
            &list_of(&["ludeon.rimworld", "example.framework"]),
            &["ludeon.rimworld", "example.framework"],
        );

        assert_eq!(
            plan.entries,
            vec![
                ImportedEntry::AlreadyActive {
                    id: id("ludeon.rimworld")
                },
                ImportedEntry::AlreadyActive {
                    id: id("example.framework")
                },
            ]
        );
        assert_eq!(plan.order, ids(&["ludeon.rimworld", "example.framework"]));
    }

    #[test]
    fn an_inactive_installed_mod_is_activated() {
        let plan = plan(
            &list_of(&["ludeon.rimworld", "someone.localmod"]),
            &["ludeon.rimworld"],
        );

        assert_eq!(
            plan.entries[1],
            ImportedEntry::Activated {
                id: id("someone.localmod")
            }
        );
        assert_eq!(plan.order, ids(&["ludeon.rimworld", "someone.localmod"]));
    }

    #[test]
    fn another_copy_of_the_package_is_matched() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .inactive("example.framework_steam")
            .build();
        let inventory = inventory_of(&report);

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "example.framework"]),
            &inventory,
            &ids(&["ludeon.rimworld"]),
            context(),
        );

        assert_eq!(
            plan.entries[1],
            ImportedEntry::MatchedOtherCopy {
                listed: id("example.framework"),
                installed: id("example.framework_steam"),
                activation: Activation::Activated,
            }
        );
        assert_eq!(
            plan.order,
            ids(&["ludeon.rimworld", "example.framework_steam"])
        );
    }

    #[test]
    fn a_matched_copy_that_is_already_active_says_so() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.framework_steam")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "example.framework"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "example.framework_steam"]),
            context(),
        );

        assert!(matches!(
            plan.entries[1],
            ImportedEntry::MatchedOtherCopy {
                activation: Activation::AlreadyActive,
                ..
            }
        ));
        assert!(plan.deactivated.is_empty());
    }

    #[test]
    fn a_mod_known_only_as_missing_is_not_installed() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .missing_mod("example.gone")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "example.gone"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "example.gone"]),
            context(),
        );

        assert!(matches!(
            plan.entries[1],
            ImportedEntry::NotInstalled { .. }
        ));
        assert_eq!(plan.order, ids(&["ludeon.rimworld"]));
        assert_eq!(plan.deactivated, ids(&["example.gone"]));
    }

    #[test]
    fn not_installed_is_classified_by_kind() {
        let list = SharedModList::new(
            None,
            vec![
                named_entry("ludeon.rimworld.royalty", "Royalty", None),
                named_entry(OWN_MERGE, "Rimmerge merge patch", Some(5)),
                named_entry("rimmerge.merge.aaaaaaaaaaaa", "Another machine's", None),
                named_entry("example.subscribe", "Subscribe", Some(1_234_567_890)),
                entry("example.nolink"),
            ],
        )
        .expect("list");

        let plan = plan(&list, &["ludeon.rimworld"]);

        let kinds: Vec<_> = plan
            .entries
            .iter()
            .map(|e| match e {
                ImportedEntry::NotInstalled { kind, .. } => *kind,
                other => panic!("expected NotInstalled, got {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                MissingKind::Dlc,
                MissingKind::RimmergeMergeMod,
                MissingKind::RimmergeMergeMod,
                MissingKind::Workshop(WorkshopId::new(1_234_567_890).expect("non-zero")),
                MissingKind::NoLink,
            ]
        );
    }

    #[test]
    fn a_senders_merge_mod_is_never_activated_even_if_installed_here() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .inactive(OWN_MERGE)
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", OWN_MERGE]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld"]),
            context(),
        );

        assert!(matches!(
            plan.entries[1],
            ImportedEntry::NotInstalled {
                kind: MissingKind::RimmergeMergeMod,
                ..
            }
        ));
        assert_eq!(plan.order, ids(&["ludeon.rimworld"]));
    }

    #[test]
    fn a_not_installed_row_keeps_the_listed_name() {
        let list = SharedModList::new(None, vec![named_entry("example.gone", "Gone Mod", None)])
            .expect("list");

        let plan = plan(&list, &["ludeon.rimworld"]);

        assert!(matches!(
            &plan.entries[0],
            ImportedEntry::NotInstalled { name: Some(name), .. } if name.as_str() == "Gone Mod"
        ));
    }

    #[test]
    fn a_duplicate_keeps_the_first_position_and_compares_base_ids() {
        let list = list_of(&[
            "ludeon.rimworld",
            "example.framework",
            "someone.localmod",
            "example.framework_steam",
            "example.framework",
        ]);

        let plan = plan(&list, &["ludeon.rimworld", "example.framework"]);

        assert_eq!(
            plan.entries[3],
            ImportedEntry::Duplicate {
                id: id("example.framework"),
                first_position: 2
            }
        );
        assert_eq!(
            plan.entries[4],
            ImportedEntry::Duplicate {
                id: id("example.framework"),
                first_position: 2
            }
        );
        assert_eq!(
            plan.order,
            ids(&["ludeon.rimworld", "example.framework", "someone.localmod"])
        );
    }

    #[test]
    fn a_list_without_core_gets_core_first() {
        let plan = plan(&list_of(&["example.framework"]), &["ludeon.rimworld"]);

        assert_eq!(plan.core, CorePlacement::AddedFirst);
        assert_eq!(plan.order, ids(&["ludeon.rimworld", "example.framework"]));
    }

    #[test]
    fn a_listed_core_keeps_its_listed_position() {
        let plan = plan(
            &list_of(&["example.framework", "ludeon.rimworld"]),
            &["ludeon.rimworld", "example.framework"],
        );

        assert_eq!(plan.core, CorePlacement::Listed);
        assert_eq!(plan.order, ids(&["example.framework", "ludeon.rimworld"]));
    }

    #[test]
    fn core_is_never_deactivated_even_when_the_list_omits_it() {
        let plan = plan(
            &list_of(&["someone.localmod"]),
            &["ludeon.rimworld", "example.framework"],
        );

        assert_eq!(plan.deactivated, ids(&["example.framework"]));
    }

    #[test]
    fn the_own_merge_mod_is_never_deactivated() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.framework")
            .mod_(OWN_MERGE)
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "example.framework", OWN_MERGE]),
            context(),
        );

        assert_eq!(plan.deactivated, ids(&["example.framework"]));
    }

    #[test]
    fn the_own_merge_mod_stays_in_the_order_at_the_end() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_(OWN_MERGE)
            .mod_("example.framework")
            .build();

        let plan = plan_import(
            &list_of(&["example.framework", "ludeon.rimworld"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", OWN_MERGE, "example.framework"]),
            context(),
        );

        assert_eq!(
            plan.order,
            ids(&["example.framework", "ludeon.rimworld", OWN_MERGE])
        );
        assert_eq!(plan.moved, 1, "the merge mod itself never counts as moved");
        assert!(plan.deactivated.is_empty());
    }

    #[test]
    fn an_own_merge_mod_absent_from_the_file_is_not_added() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .inactive(OWN_MERGE)
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld"]),
            context(),
        );

        assert_eq!(plan.order, ids(&["ludeon.rimworld"]));
    }

    #[test]
    fn deactivated_follows_file_order_without_repeats() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("b.two")
            .mod_("a.one")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "b.two", "a.one", "b.two"]),
            context(),
        );

        assert_eq!(plan.deactivated, ids(&["b.two", "a.one"]));
    }

    fn moved_between(file: &[&str], listed: &[&str]) -> u32 {
        let mut builder = ReportBuilder::new();
        for raw in file.iter().chain(listed) {
            builder = builder.mod_(raw);
        }
        let report = builder.build();
        plan_import(
            &list_of(listed),
            &inventory_of(&report),
            &ids(file),
            context(),
        )
        .moved
    }

    #[test]
    fn moved_is_zero_for_an_identical_order() {
        assert_eq!(
            moved_between(
                &["ludeon.rimworld", "a.one", "b.two"],
                &["ludeon.rimworld", "a.one", "b.two"]
            ),
            0
        );
    }

    #[test]
    fn moved_counts_a_swapped_pair_as_one() {
        assert_eq!(
            moved_between(
                &["ludeon.rimworld", "a.one", "b.two"],
                &["ludeon.rimworld", "b.two", "a.one"]
            ),
            1
        );
    }

    #[test]
    fn moved_ignores_mods_that_are_added_or_removed() {
        assert_eq!(
            moved_between(
                &["ludeon.rimworld", "a.one", "b.two"],
                &["ludeon.rimworld", "c.three", "a.one"]
            ),
            0
        );
    }

    #[test]
    fn moved_counts_a_reversed_run() {
        assert_eq!(
            moved_between(
                &["ludeon.rimworld", "a.one", "b.two", "c.three"],
                &["c.three", "b.two", "a.one", "ludeon.rimworld"]
            ),
            3
        );
    }

    fn version_list(text: &str) -> SharedModList {
        SharedModList::new(ListedGameVersion::new(text), vec![entry("ludeon.rimworld")])
            .expect("list")
    }

    fn version_check(listed: &str, game: Option<&str>) -> VersionCheck {
        let mut context = context();
        context.game_version = game.map(str::to_string);
        plan_import(
            &version_list(listed),
            &inventory_of(&standard_report()),
            &ids(&["ludeon.rimworld"]),
            context,
        )
        .version
    }

    #[test]
    fn a_different_minor_version_differs() {
        assert_eq!(
            version_check("1.5.4409 rev120", Some("1.6.4871 rev590")),
            VersionCheck::Differs {
                listed: "1.5.4409 rev120".to_string(),
                game: "1.6.4871 rev590".to_string()
            }
        );
    }

    #[test]
    fn a_build_difference_within_one_minor_version_is_the_same() {
        assert_eq!(
            version_check("1.6.4800 rev1", Some("1.6.4871 rev590")),
            VersionCheck::Same
        );
    }

    #[test]
    fn a_missing_or_unrecognizable_version_is_unknown() {
        assert_eq!(version_check("1.6", None), VersionCheck::Unknown);
        assert_eq!(
            version_check("nightly", Some("1.6.4871")),
            VersionCheck::Unknown
        );
        let no_listed_version = plan(&list_of(&["ludeon.rimworld"]), &["ludeon.rimworld"]);
        assert_eq!(no_listed_version.version, VersionCheck::Unknown);
    }

    #[test]
    fn pending_changes_are_passed_through() {
        let mut context = context();
        context.has_pending_changes = true;

        let plan = plan_import(
            &list_of(&["ludeon.rimworld"]),
            &inventory_of(&standard_report()),
            &ids(&["ludeon.rimworld"]),
            context,
        );

        assert!(plan.replaces_pending_changes);
    }

    #[test]
    fn a_list_of_only_missing_mods_still_plans_core_alone() {
        let plan = plan(
            &list_of(&["example.gone"]),
            &["ludeon.rimworld", "example.framework"],
        );

        assert_eq!(plan.order, ids(&["ludeon.rimworld"]));
        assert_eq!(plan.deactivated, ids(&["example.framework"]));
    }

    #[test]
    fn an_active_steam_copy_is_kept_when_the_list_names_the_base_id() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.pair_steam")
            .inactive("example.pair")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "example.pair"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "example.pair_steam"]),
            context(),
        );

        assert_eq!(
            plan.entries[1],
            ImportedEntry::MatchedOtherCopy {
                listed: id("example.pair"),
                installed: id("example.pair_steam"),
                activation: Activation::AlreadyActive,
            }
        );
        assert_eq!(plan.order, ids(&["ludeon.rimworld", "example.pair_steam"]));
        assert!(plan.deactivated.is_empty());
    }

    #[test]
    fn the_listed_copy_wins_when_both_copies_are_active() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_("example.pair_steam")
            .mod_("example.pair")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "example.pair"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld", "example.pair_steam", "example.pair"]),
            context(),
        );

        assert_eq!(
            plan.entries[1],
            ImportedEntry::AlreadyActive {
                id: id("example.pair")
            }
        );
    }

    #[test]
    fn an_installed_but_inactive_dlc_is_activated_not_reported_missing() {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .inactive("ludeon.rimworld.royalty")
            .build();

        let plan = plan_import(
            &list_of(&["ludeon.rimworld", "ludeon.rimworld.royalty"]),
            &inventory_of(&report),
            &ids(&["ludeon.rimworld"]),
            context(),
        );

        assert_eq!(
            plan.entries[1],
            ImportedEntry::Activated {
                id: id("ludeon.rimworld.royalty")
            }
        );
    }

    #[test]
    fn core_is_missing_when_the_list_omits_it_and_none_is_installed() {
        let report = ReportBuilder::new().mod_("example.framework").build();

        let plan = plan_import(
            &list_of(&["example.framework"]),
            &inventory_of(&report),
            &ids(&["example.framework"]),
            context(),
        );

        assert_eq!(plan.core, CorePlacement::Missing);
        assert_eq!(plan.order, ids(&["example.framework"]));
    }

    #[test]
    fn listed_ids_are_lowercased_before_matching() {
        let list = SharedModList::new(
            None,
            vec![SharedModEntry {
                id: ListedPackageId::try_from("Example.Framework").expect("valid"),
                name: None,
                workshop_id: None,
            }],
        )
        .expect("list");

        let plan = plan(&list, &["ludeon.rimworld", "example.framework"]);

        assert_eq!(
            plan.entries[0],
            ImportedEntry::AlreadyActive {
                id: id("example.framework")
            }
        );
    }
}
