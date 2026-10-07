//! [`ModInventory`]: everything this crate knows about every mod
//! discovery found (active or not) or that `ModsConfig.xml` once named
//! but discovery never found — the one input [`crate::ActiveSet`] and the
//! activate/deactivate planning functions resolve an id against.
//!

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{DeclaredOrder, EdgeStrength, InactiveMod, ModId, Report, Source};

use crate::active_set::ActiveSet;

/// What this crate knows about one entry: an active
/// [`rim_analyzer::domain::Mod`], an inactive
/// [`rim_analyzer::domain::InactiveMod`], or a `Report::missing_mods` id
/// discovery never found at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryEntry {
    /// Display name. For a missing mod (nothing else to read one from)
    /// this is the id's own text.
    pub name: String,
    /// `None` only for a missing mod — it was never found on disk, so
    /// there is no `About.xml` to have read a source from.
    pub source: Option<Source>,
    /// This mod's own declared `modDependencies`, as the base ids they
    /// name (`About.xml` never carries a `_steam` suffix). Always empty
    /// for a missing mod: it can still be a closure *member* (something
    /// else's dependency, happening to be missing), but it can never be a
    /// closure *source* — it has no declared dependencies of its own to
    /// walk.
    pub declared_dependencies: Vec<ModId>,
    /// `false` only for a missing mod.
    pub present_on_disk: bool,
    /// The Steam Workshop published-file id, when discovery read one
    /// (`Mod::workshop_id` / `InactiveMod::workshop_id`). `None` for a
    /// local mod, and always `None` for a missing mod.
    pub workshop_id: Option<u64>,
}

fn declared_dependency_ids(declared: &DeclaredOrder) -> Vec<ModId> {
    declared.dependencies.iter().map(|d| d.id.clone()).collect()
}

/// `Report.mods` (active) ∪ `Report.inactive_mods` ∪ `Report.missing_mods`,
/// keyed by the exact id `ModsConfig.xml` would need for that entry (bare,
/// or `_steam`-suffixed — see
/// [`rim_analyzer::domain::InactiveMod::id`]'s own doc comment), plus the
/// `Hard`-strength report-edge dependents [`Self::dependents_of`] needs
/// (declared `modDependencies` plus `Hard` edges — `Soft`/`Awareness` stay
/// out).
#[derive(Debug, Clone, Default)]
pub struct ModInventory {
    entries: BTreeMap<ModId, InventoryEntry>,
    /// `before`'s base id -> every `after` base id with a `Hard`-strength
    /// edge to it. These are always already-active mods: the analyzer
    /// only ever computes edges between active mods.
    hard_dependents: BTreeMap<ModId, BTreeSet<ModId>>,
}

impl ModInventory {
    /// Builds an inventory from one scan's [`Report`].
    #[must_use]
    pub fn from_report(report: &Report) -> Self {
        let mut entries = BTreeMap::new();
        for m in &report.mods {
            entries.insert(
                m.id.clone(),
                InventoryEntry {
                    name: m.name.clone(),
                    source: Some(m.source),
                    declared_dependencies: declared_dependency_ids(&m.declared),
                    present_on_disk: true,
                    workshop_id: m.workshop_id,
                },
            );
        }
        for m in &report.inactive_mods {
            // `report.mods` and `report.inactive_mods` are contractually
            // disjoint (the analyzer's own scan/discovery split, `rim-analyzer`'s
            // own `CLAUDE.md`) — `or_insert_with` keeps the active copy on
            // the rare chance that contract is ever violated, rather than
            // an inactive entry silently overwriting an active one; the
            // `debug_assert!` turns that same violation loud in every
            // debug/test build instead of leaving it to be noticed later
            // as a mysteriously-wrong `present_on_disk`/`source` reading.
            debug_assert!(
                !entries.contains_key(&m.id),
                "{} is in both report.mods and report.inactive_mods",
                m.id
            );
            entries
                .entry(m.id.clone())
                .or_insert_with(|| InventoryEntry {
                    name: m.name.clone(),
                    source: Some(m.source),
                    declared_dependencies: declared_dependency_ids(&m.declared),
                    present_on_disk: true,
                    workshop_id: m.workshop_id,
                });
        }
        for id in &report.missing_mods {
            entries.entry(id.clone()).or_insert_with(|| InventoryEntry {
                name: id.to_string(),
                source: None,
                declared_dependencies: Vec::new(),
                present_on_disk: false,
                workshop_id: None,
            });
        }

        let mut hard_dependents: BTreeMap<ModId, BTreeSet<ModId>> = BTreeMap::new();
        for edge_report in &report.edges {
            let edge = &edge_report.edge;
            if edge.strength() == EdgeStrength::Hard {
                hard_dependents
                    .entry(edge.before.base())
                    .or_default()
                    .insert(edge.after.base());
            }
        }

        Self {
            entries,
            hard_dependents,
        }
    }

    /// Builds a discovery-only inventory:
    /// `discovered` is every mod on disk, active or not — from
    /// `rim_analyzer::infra::inventory`'s own `discovered` field, already
    /// shaped as [`InactiveMod`] for identity and declared dependencies —
    /// plus `missing`, every active id discovery found no directory for.
    ///
    /// No [`Report`] means no report edges, so this inventory's own
    /// `hard_dependents` is always empty: [`Self::dependents_of`] falls
    /// back to declared `modDependencies` alone against an inventory
    /// built this way, exactly the CLI's own disclosed contract (`mods
    /// deactivate`'s "dependents (declared): ..." — a full scan is the
    /// only way to see `Hard`-strength report edges at all).
    #[must_use]
    pub fn from_inventory_output(discovered: &[InactiveMod], missing: &[ModId]) -> Self {
        let mut entries = BTreeMap::new();
        for m in discovered {
            entries.insert(
                m.id.clone(),
                InventoryEntry {
                    name: m.name.clone(),
                    source: Some(m.source),
                    declared_dependencies: declared_dependency_ids(&m.declared),
                    present_on_disk: true,
                    workshop_id: m.workshop_id,
                },
            );
        }
        for id in missing {
            entries.entry(id.clone()).or_insert_with(|| InventoryEntry {
                name: id.to_string(),
                source: None,
                declared_dependencies: Vec::new(),
                present_on_disk: false,
                workshop_id: None,
            });
        }
        Self {
            entries,
            hard_dependents: BTreeMap::new(),
        }
    }

    /// The entry for `id`, by exact match.
    #[must_use]
    pub fn entry(&self, id: &ModId) -> Option<&InventoryEntry> {
        self.entries.get(id)
    }

    /// Every entry in this inventory, keyed by its exact id, in id order
    /// (a `BTreeMap`) — `apps/cli`'s `mods list`
    /// walks this to build the inactive/all listing without needing to
    /// carry the raw discovery lists alongside a [`ModInventory`] too.
    pub fn entries(&self) -> impl Iterator<Item = (&ModId, &InventoryEntry)> {
        self.entries.iter()
    }

    /// Every entry with no directory on disk, id-ordered — a discovery-
    /// only inventory's own `missing` list (every active id
    /// `resolve_active_mods` couldn't find a folder for), or a
    /// `from_report` inventory's `Report.missing_mods` reconstructed.
    /// `apps/cli`'s `mods list` reads this directly rather than
    /// recomputing "is this active id missing" per id: the two disagree
    /// the moment an active id repeats
    /// in a hand-edited `ModsConfig.xml` — recomputing over `active_ids`
    /// would count a missing entry once per occurrence, this map-backed
    /// walk counts it once, since `self.entries` is already deduplicated
    /// by id.
    pub fn missing_ids(&self) -> impl Iterator<Item = &ModId> {
        self.entries
            .iter()
            .filter(|(_, entry)| !entry.present_on_disk)
            .map(|(id, _)| id)
    }

    /// Whether `id` (exact) resolves to anything in this inventory.
    #[must_use]
    pub fn contains(&self, id: &ModId) -> bool {
        self.entries.contains_key(id)
    }

    /// The entry whose own id shares `base`: an exact match on `base`
    /// itself first, else the lexicographically smallest exact id sharing
    /// it. Since `X` sorts before `X_steam`, this prefers a primary copy
    /// over a Workshop one when both are inactive — the right answer for
    /// resolving a *declared* dependency name, which never itself carries
    /// `_steam`.
    #[must_use]
    pub fn find_by_base(&self, base: &ModId) -> Option<(&ModId, &InventoryEntry)> {
        if let Some((id, entry)) = self.entries.get_key_value(base) {
            return Some((id, entry));
        }
        self.entries.iter().find(|(id, _)| id.base() == *base)
    }

    /// Every mod in `active`'s own list, other than `id` itself, that
    /// declares `id` in its `modDependencies` or carries a `Hard`-strength
    /// report edge to it — base-id compared throughout.
    #[must_use]
    pub fn dependents_of(&self, id: &ModId, active: &ActiveSet) -> Vec<ModId> {
        let target = id.base();
        let hard = self.hard_dependents.get(&target);
        let mut result: BTreeSet<ModId> = BTreeSet::new();
        for active_id in active.ids() {
            let active_base = active_id.base();
            if active_base == target {
                continue;
            }
            let declares = self.entries.get(active_id).is_some_and(|entry| {
                entry
                    .declared_dependencies
                    .iter()
                    .any(|d| d.base() == target)
            });
            let has_hard_edge = hard.is_some_and(|deps| deps.contains(&active_base));
            if declares || has_hard_edge {
                result.insert(active_id.clone());
            }
        }
        result.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::report_fixture_with_inactive;

    #[test]
    fn from_report_indexes_active_inactive_and_missing_mods() {
        let mut report = report_fixture_with_inactive(&["a"], &["b"]);
        report.missing_mods.push(ModId::new("c"));
        let inventory = ModInventory::from_report(&report);

        assert!(inventory.contains(&ModId::new("a")));
        assert!(
            inventory
                .entry(&ModId::new("a"))
                .is_some_and(|e| e.present_on_disk && e.source.is_some())
        );
        assert!(
            inventory
                .entry(&ModId::new("b"))
                .is_some_and(|e| e.present_on_disk && e.source.is_some())
        );
        let missing = inventory
            .entry(&ModId::new("c"))
            .expect("missing mod still gets an entry");
        assert!(!missing.present_on_disk);
        assert!(missing.source.is_none());
        assert!(missing.declared_dependencies.is_empty());
        assert_eq!(missing.name, "c");
    }

    #[test]
    fn find_by_base_prefers_an_exact_match_then_the_smallest_steam_suffixed_copy() {
        let report = report_fixture_with_inactive(&[], &["foo.bar", "foo.bar_steam"]);
        let inventory = ModInventory::from_report(&report);

        let (id, _) = inventory
            .find_by_base(&ModId::new("foo.bar"))
            .expect("a primary copy exists");
        assert_eq!(id, &ModId::new("foo.bar"));

        // A workshop-only inactive mod (no primary copy) is still found
        // through its own bare id — never through `_steam` (see
        // `InactiveMod::id`'s own doc comment).
        let workshop_only = report_fixture_with_inactive(&[], &["only.workshop"]);
        let inventory = ModInventory::from_report(&workshop_only);
        assert!(
            inventory
                .find_by_base(&ModId::new("only.workshop"))
                .is_some()
        );
    }

    /// [`ModInventory::from_inventory_output`] builds the same shape of
    /// entries [`ModInventory::from_report`] would for the mods it's
    /// given (active or inactive alike — a discovery-only inventory
    /// doesn't distinguish the two), plus a missing entry, and carries no
    /// `Hard`-strength dependents at all (no `Report` means no edges).
    #[test]
    fn from_inventory_output_indexes_discovered_and_missing_mods_with_no_hard_edges() {
        let report = report_fixture_with_inactive(&["a"], &["b"]);
        let inventory =
            ModInventory::from_inventory_output(&report.inactive_mods, &[ModId::new("c")]);

        // `report.inactive_mods` only ever carries mods `report_fixture_with_inactive`
        // marked inactive — "a" (active in the source report) is
        // deliberately absent from `discovered` here, since this
        // constructor's own contract is "every mod `infra::inventory`'s
        // own `discovered` field lists", not "every mod in a Report".
        assert!(inventory.entry(&ModId::new("a")).is_none());
        let b = inventory
            .entry(&ModId::new("b"))
            .expect("b was passed as discovered");
        assert!(b.present_on_disk);
        assert!(b.source.is_some());

        let missing = inventory
            .entry(&ModId::new("c"))
            .expect("c was passed as missing");
        assert!(!missing.present_on_disk);
        assert!(missing.source.is_none());
        assert_eq!(missing.name, "c");

        let active = ActiveSet::trusted(vec![ModId::new("a"), ModId::new("b")]);
        assert!(
            inventory
                .dependents_of(&ModId::new("a"), &active)
                .is_empty(),
            "no Report means no Hard-strength edges to report as dependents"
        );
    }

    #[test]
    fn workshop_id_is_carried_from_active_inactive_and_discovery_only_mods() {
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .mod_with("active.mod", |m| m.workshop_id = Some(111))
            .inactive("inactive.mod")
            .inactive("local.mod")
            .missing_mod("ghost.mod")
            .build();
        report.inactive_mods[0].workshop_id = Some(222);

        let from_report = ModInventory::from_report(&report);
        let discovery_only = ModInventory::from_inventory_output(&report.inactive_mods, &[]);

        let id = |raw: &str| ModId::new(raw);
        let workshop = |inv: &ModInventory, raw: &str| inv.entry(&id(raw)).map(|e| e.workshop_id);
        assert_eq!(workshop(&from_report, "active.mod"), Some(Some(111)));
        assert_eq!(workshop(&from_report, "inactive.mod"), Some(Some(222)));
        assert_eq!(workshop(&from_report, "local.mod"), Some(None));
        assert_eq!(workshop(&from_report, "ghost.mod"), Some(None));
        assert_eq!(workshop(&discovery_only, "inactive.mod"), Some(Some(222)));
    }

    #[test]
    fn entries_iterates_every_entry_in_id_order() {
        let report = report_fixture_with_inactive(&["b"], &["a"]);
        let inventory = ModInventory::from_report(&report);

        let ids: Vec<ModId> = inventory.entries().map(|(id, _)| id.clone()).collect();

        assert_eq!(ids, vec![ModId::new("a"), ModId::new("b")]);
    }

    #[test]
    fn missing_ids_lists_only_entries_with_no_directory_on_disk() {
        let mut report = report_fixture_with_inactive(&["a"], &["b"]);
        report.missing_mods.push(ModId::new("c"));
        let inventory = ModInventory::from_report(&report);

        let missing: Vec<ModId> = inventory.missing_ids().cloned().collect();

        assert_eq!(missing, vec![ModId::new("c")]);
    }

    /// A discovery-only inventory built from `active`/`missing` slices
    /// carrying the same id twice (a hand-edited `ModsConfig.xml`'s own
    /// possible shape) still reports that id once — `self.entries` is a
    /// map, so a caller reading `missing_ids()` can never double-count a
    /// duplicated active entry the way iterating the raw active list and
    /// re-checking each one would.
    #[test]
    fn missing_ids_deduplicates_a_repeated_id() {
        let inventory =
            ModInventory::from_inventory_output(&[], &[ModId::new("ghost"), ModId::new("ghost")]);

        let missing: Vec<ModId> = inventory.missing_ids().cloned().collect();

        assert_eq!(missing, vec![ModId::new("ghost")]);
    }

    #[test]
    fn dependents_of_combines_declared_dependencies_and_hard_edges() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("target.mod")
            .mod_("declares.mod")
            .mod_("hard.mod")
            .mod_("unrelated.mod")
            .dependency("declares.mod", "target.mod")
            .hard_edge("hard.mod", "target.mod")
            .build();
        let inventory = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![
            ModId::new("target.mod"),
            ModId::new("declares.mod"),
            ModId::new("hard.mod"),
            ModId::new("unrelated.mod"),
        ]);

        let dependents = inventory.dependents_of(&ModId::new("target.mod"), &active);

        assert_eq!(
            dependents,
            vec![ModId::new("declares.mod"), ModId::new("hard.mod")]
        );
    }

    /// The two exclusions, negative: a `Soft`-strength edge is not a
    /// dependent (only `Hard` counts), and a mod that declares the
    /// dependency but isn't currently active — whether it's simply
    /// inactive or was active on disk but has since been dropped from
    /// the working set — must not be reported either, even though its
    /// own `InventoryEntry` still carries the declaration.
    #[test]
    fn dependents_of_ignores_soft_edges_and_declarers_that_are_not_active() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("target")
            .mod_("soft")
            .mod_("pending.deactivated")
            .inactive("inactive.declarer")
            .soft_edge("soft", "target")
            .dependency("pending.deactivated", "target")
            .inactive_dependency("inactive.declarer", "target")
            .build();
        let inventory = ModInventory::from_report(&report);
        // Only `target`/`soft` are active — `pending.deactivated` (once
        // active, now removed from the working set) and
        // `inactive.declarer` (never active) are both known to the
        // inventory but absent here.
        let active = ActiveSet::trusted(vec![ModId::new("target"), ModId::new("soft")]);

        let dependents = inventory.dependents_of(&ModId::new("target"), &active);

        assert!(
            dependents.is_empty(),
            "a Soft edge and a non-active declarer must both be excluded: got {dependents:?}"
        );
    }
}
