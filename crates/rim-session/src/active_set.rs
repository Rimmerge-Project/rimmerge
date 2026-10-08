//! [`ActiveSet`]: the working (in-memory, unsaved) active-mod list.
//! Mirrors RimSort's own
//! "unsaved Active list" model: every mutation here is
//! validated against a [`ModInventory`] so a caller can never silently
//! append an id nothing on disk resolves to, and nothing here ever
//! touches `ModsConfig.xml`, the report, or the sorter.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;

use crate::mod_inventory::ModInventory;

/// `ModsConfig.xml`'s own never-removable entry — **not** always first:
/// real installs load framework mods such as an early-loading hook or a
/// runtime-patching library **before** Core (such a hook's whole purpose
/// is to hook the game's own assembly-loading process, which requires
/// loading ahead of everything, Core included — a legitimate, common
/// modding pattern, not a corrupted file), so `ActiveSet::new` never
/// requires Core to come first.
/// Identified by id rather than
/// [`rim_analyzer::domain::Source::Core`]: [`ActiveSet::deactivate`] has
/// no inventory to check a source against, so "is
/// this Core" must be answerable from the id alone — the same way this
/// workspace's existing sort/import code already names it literally
/// (`crates/rim-resolve/src/sort/tiers.rs`, `crates/rim-io/src/mods_config.rs`'s
/// own tests).
pub(crate) const CORE_MOD_ID: &str = "ludeon.rimworld";

pub(crate) fn is_core(id: &ModId) -> bool {
    id.base().as_str() == CORE_MOD_ID
}

/// Everything that can go wrong building or mutating an [`ActiveSet`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActiveSetError {
    /// `id` resolves to nothing in the [`ModInventory`] it was checked
    /// against — never silently appended.
    #[error("{0} is not a known mod")]
    Unknown(ModId),
    /// `id` appears more than once, by exact id — `X` and `X_steam` are
    /// two distinct entries by RimWorld's own rule, so a base-id
    /// collision alone is not this error.
    #[error("{0} appears more than once")]
    Duplicate(ModId),
}

/// The working active-mod list, in RimSort's model of
/// "an Active and an Inactive list, moves between them are edits to an
/// *unsaved* list". [`crate::Session::working`] holds one; it never
/// changes `orders`, ledgers, or the sorter on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSet {
    ids: Vec<ModId>,
}

impl ActiveSet {
    /// Builds a validated set: every id resolves in `inventory`, no exact
    /// duplicates. **Deliberately no "Core must be first" check** —
    /// [`CORE_MOD_ID`]'s own doc comment has the real-install case that
    /// disproved it (an early-loading-style mod legitimately loads ahead of
    /// Core); the only real Core invariant is "never removable"
    /// ([`Self::deactivate`]).
    ///
    /// # Errors
    ///
    /// See [`ActiveSetError`].
    pub fn new(ids: Vec<ModId>, inventory: &ModInventory) -> Result<Self, ActiveSetError> {
        let mut seen = BTreeSet::new();
        for id in &ids {
            if !inventory.contains(id) {
                return Err(ActiveSetError::Unknown(id.clone()));
            }
            if !seen.insert(id.clone()) {
                return Err(ActiveSetError::Duplicate(id.clone()));
            }
        }
        Ok(Self { ids })
    }

    /// Builds a set from an already-trusted list, skipping every check
    /// [`Self::new`] runs — for [`crate::Session`]'s own construction from
    /// `ModsConfig.xml`'s own `<activeMods>` (`orders().current`), which
    /// is guaranteed consistent with the inventory built from the very
    /// report that read it (every active id is either a scanned
    /// [`rim_analyzer::domain::Mod`] or a `Report::missing_mods` entry —
    /// see [`ModInventory::from_report`]). `pub(crate)`: an external
    /// caller must go through the validated [`Self::new`].
    ///
    /// Note this really does skip the *duplicate* check too, not only the
    /// inventory one: a hand-edited `ModsConfig.xml` that repeats one
    /// `<li>` entry round-trips here with that id twice — `ids()` still
    /// returns both, [`Self::activate`]'s own de-dup only ever applies to
    /// what *it* appends, and nothing in this crate scrubs a pre-existing
    /// duplicate on load. The sorter and every other consumer of
    /// `orders.current` tolerate the same file-level duplicate.
    #[must_use]
    pub(crate) fn trusted(ids: Vec<ModId>) -> Self {
        Self { ids }
    }

    /// The ids, in order.
    #[must_use]
    pub fn ids(&self) -> &[ModId] {
        &self.ids
    }

    /// Whether `id` (exact) is in this set.
    #[must_use]
    pub fn contains(&self, id: &ModId) -> bool {
        self.ids.iter().any(|x| x == id)
    }

    /// Whether some id in this set shares `id`'s base — the `_steam`-aware
    /// "already active" check [`Self::activate`] uses.
    #[must_use]
    pub fn contains_base(&self, id: &ModId) -> bool {
        let base = id.base();
        self.ids.iter().any(|x| x.base() == base)
    }

    /// Appends every id in `ids` not already active (base-id compared, so
    /// requesting `X` when `X_steam` is already active is a no-op), in the
    /// order given, skipping a later exact repeat of one this call already
    /// scheduled. Validates every id against `inventory` *before* mutating
    /// anything, so a single unknown id leaves this set entirely untouched
    /// rather than partially updated. Returns what was actually appended.
    ///
    /// # Errors
    ///
    /// Returns [`ActiveSetError::Unknown`] naming the first id `inventory`
    /// doesn't recognize.
    pub fn activate(
        &mut self,
        ids: Vec<ModId>,
        inventory: &ModInventory,
    ) -> Result<Vec<ModId>, ActiveSetError> {
        for id in &ids {
            if !inventory.contains(id) {
                return Err(ActiveSetError::Unknown(id.clone()));
            }
        }
        let mut added = Vec::new();
        for id in ids {
            if self.contains_base(&id) || added.contains(&id) {
                continue;
            }
            self.ids.push(id.clone());
            added.push(id);
        }
        Ok(added)
    }

    /// Force-appends `id` with no inventory validation — a no-op if `id`
    /// (exact) is already present. `pub(crate)`, for
    /// [`crate::Session::set_current_order`]'s own reconciliation only:
    /// that method's contract is "the file now *is* this order", so an id
    /// it just wrote to the real `ModsConfig.xml` is trusted by
    /// construction and must not silently make the working set look
    /// stale relative to a reality it hasn't even been told about yet.
    pub(crate) fn force_activate(&mut self, id: ModId) {
        if !self.contains(&id) {
            self.ids.push(id);
        }
    }

    /// Removes every id in `ids` present in this set (exact match), except
    /// Core, which is never removable. Returns what was actually removed.
    pub fn deactivate(&mut self, ids: &[ModId]) -> Vec<ModId> {
        let mut removed = Vec::new();
        for id in ids {
            if is_core(id) {
                continue;
            }
            if let Some(position) = self.ids.iter().position(|x| x == id) {
                self.ids.remove(position);
                removed.push(id.clone());
            }
        }
        removed
    }

    /// What's in `self` but not `other` (`added`), and what's in `other`
    /// but not `self` (`removed`) — both by exact id, sorted.
    #[must_use]
    pub fn diff(&self, other: &ActiveSet) -> ActiveSetDiff {
        let mine: BTreeSet<ModId> = self.ids.iter().cloned().collect();
        let theirs: BTreeSet<ModId> = other.ids.iter().cloned().collect();
        ActiveSetDiff {
            added: mine.difference(&theirs).cloned().collect(),
            removed: theirs.difference(&mine).cloned().collect(),
        }
    }
}

/// What changed between two [`ActiveSet`]s — [`ActiveSet::diff`]'s result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveSetDiff {
    /// Present in the newer set, absent from the older one.
    pub added: Vec<ModId>,
    /// Present in the older set, absent from the newer one.
    pub removed: Vec<ModId>,
}

impl ActiveSetDiff {
    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// [`crate::Session::pending_changes`]'s own two-diff result: what the
/// working set has that the last scan hasn't seen yet (`unscanned`), and
/// what the scanned order has that the file on disk doesn't yet
/// (`unapplied`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingActiveChanges {
    /// `working` vs. the scanned `orders().current`.
    pub unscanned: ActiveSetDiff,
    /// The scanned `orders().current` vs. `file_active_mods`.
    pub unapplied: ActiveSetDiff,
}

/// [`crate::use_cases::ActivateMods`]'s plan — what [`plan_activate`]
/// would add and why, before anything is mutated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivatePlan {
    /// Every id to append, in order: each requested id's own inactive
    /// dependency closure (dependencies first), then the id itself.
    pub to_add: Vec<ModId>,
    /// A requested (or closure-walked) mod's own declared dependencies
    /// that resolve to nothing usable on disk — reported, never added.
    pub unresolvable_dependencies: BTreeMap<ModId, Vec<ModId>>,
    /// Requested ids already active (base-id compared) — skipped, not
    /// re-added.
    pub already_active: Vec<ModId>,
}

/// [`crate::use_cases::DeactivateMods`]'s plan — what [`plan_deactivate`]
/// would remove and who still depends on it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeactivatePlan {
    /// Every requested id that would actually be removed (Core excluded,
    /// and an id not currently active contributes nothing here).
    pub to_remove: Vec<ModId>,
    /// Per removed id, every other currently-active mod that declares it
    /// in `modDependencies` or carries a `Hard`-strength edge to it
    /// — a warning, never a block.
    pub dependents_still_active: BTreeMap<ModId, Vec<ModId>>,
    /// Requested ids refused because they're Core.
    pub refused: Vec<ModId>,
}

/// Plans an activation: the dependency closure order, unresolvable
/// dependencies, and which requested ids are already active — pure over
/// `active`/`inventory`, so `apps/cli` can reuse it with no
/// [`crate::Session`] at all.
#[must_use]
pub fn plan_activate(
    active: &ActiveSet,
    inventory: &ModInventory,
    ids: &[ModId],
    with_dependencies: bool,
) -> ActivatePlan {
    let mut plan = ActivatePlan::default();
    let mut scheduled: BTreeSet<ModId> = BTreeSet::new();

    for id in ids {
        if active.contains_base(id) {
            plan.already_active.push(id.clone());
            continue;
        }
        // A *different* exact id sharing this base may already be
        // scheduled — e.g. requesting `[b, a_steam]` where `b`'s own
        // declared dependency resolves to the bare `a`: `a_steam`'s base
        // is already covered by `b`'s closure,
        // so it's reported the same way an already-active id is, never
        // silently dropped. Checked *before* this id's own closure walk
        // runs — a self-referencing or cyclic closure (`c` depending on
        // itself, or `a`/`b` depending on each other) legitimately
        // schedules `id`'s own base *during* its own walk, and that case
        // must fall through to the plain "already scheduled, don't
        // double-push" skip below instead, since `id` is already
        // correctly present in `to_add` from the walk itself.
        if scheduled.contains(&id.base()) {
            plan.already_active.push(id.clone());
            continue;
        }
        if with_dependencies {
            visit_dependency_closure(id, active, inventory, &mut scheduled, &mut plan);
        }
        if scheduled.insert(id.base()) {
            plan.to_add.push(id.clone());
        }
    }
    plan
}

/// Post-order walk of `id`'s own declared dependency chain: each
/// dependency (and, transitively, its own dependencies) is appended
/// before `id` itself — "dependencies first". `scheduled`
/// is a `BTreeSet` of base ids already visited across the whole
/// [`plan_activate`] call, so a cycle simply stops re-entering an id
/// already on the list — deterministic, first-seen order.
fn visit_dependency_closure(
    id: &ModId,
    active: &ActiveSet,
    inventory: &ModInventory,
    scheduled: &mut BTreeSet<ModId>,
    plan: &mut ActivatePlan,
) {
    let Some(entry) = inventory.entry(id) else {
        return;
    };
    let mut missing = Vec::new();
    for dependency in &entry.declared_dependencies {
        let base = dependency.base();
        if active.contains_base(&base) || scheduled.contains(&base) {
            continue;
        }
        let Some((resolved_id, resolved_entry)) = inventory.find_by_base(&base) else {
            missing.push(dependency.clone());
            continue;
        };
        if !resolved_entry.present_on_disk {
            // Known to the inventory (a `missing_mods` placeholder) but
            // not actually present on disk — nothing to activate, and it
            // has no declared dependencies of its own to walk further
            // (`ModInventory`'s own doc comment).
            missing.push(dependency.clone());
            continue;
        }
        let resolved_id = resolved_id.clone();
        scheduled.insert(resolved_id.base());
        visit_dependency_closure(&resolved_id, active, inventory, scheduled, plan);
        plan.to_add.push(resolved_id);
    }
    if !missing.is_empty() {
        plan.unresolvable_dependencies
            .entry(id.clone())
            .or_default()
            .extend(missing);
    }
}

/// Plans a deactivation: which requested ids would actually be removed,
/// who still declares each one a dependency, and which were refused as
/// Core — pure, the same "no `Session` needed" shape as [`plan_activate`].
#[must_use]
pub fn plan_deactivate(
    active: &ActiveSet,
    inventory: &ModInventory,
    ids: &[ModId],
) -> DeactivatePlan {
    let mut plan = DeactivatePlan::default();
    for id in ids {
        if is_core(id) {
            plan.refused.push(id.clone());
        } else if active.contains(id) {
            plan.to_remove.push(id.clone());
        }
    }
    for id in &plan.to_remove {
        let dependents = inventory.dependents_of(id, active);
        if !dependents.is_empty() {
            plan.dependents_still_active.insert(id.clone(), dependents);
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::report_fixture_with_inactive;

    fn inventory(active: &[&str], inactive: &[&str]) -> ModInventory {
        ModInventory::from_report(&report_fixture_with_inactive(active, inactive))
    }

    #[test]
    fn new_accepts_every_known_id_in_order() {
        let inv = inventory(&["a", "b"], &[]);

        let set = ActiveSet::new(vec![ModId::new("a"), ModId::new("b")], &inv)
            .expect("both ids are known");

        assert_eq!(set.ids(), [ModId::new("a"), ModId::new("b")]);
    }

    #[test]
    fn new_rejects_an_unknown_id() {
        let inv = inventory(&["a"], &[]);

        let error =
            ActiveSet::new(vec![ModId::new("ghost")], &inv).expect_err("ghost resolves to nothing");

        assert_eq!(error, ActiveSetError::Unknown(ModId::new("ghost")));
    }

    #[test]
    fn new_rejects_an_exact_duplicate() {
        let inv = inventory(&["a"], &[]);

        let error = ActiveSet::new(vec![ModId::new("a"), ModId::new("a")], &inv)
            .expect_err("a appears twice");

        assert_eq!(error, ActiveSetError::Duplicate(ModId::new("a")));
    }

    #[test]
    fn new_allows_two_distinct_steam_suffixed_and_bare_copies() {
        let inv = inventory(&["foo.bar", "foo.bar_steam"], &[]);

        let set = ActiveSet::new(
            vec![ModId::new("foo.bar"), ModId::new("foo.bar_steam")],
            &inv,
        )
        .expect("bare and _steam are two distinct ids");

        assert_eq!(set.ids().len(), 2);
    }

    /// The real-install shape: an early-loading/runtime-patching-
    /// style mod loading before Core is legitimate, not a validation
    /// error (`CORE_MOD_ID`'s own doc comment has the full write-up).
    #[test]
    fn new_accepts_core_anywhere_in_the_list() {
        let inv = inventory(&["a", "ludeon.rimworld"], &[]);

        let set = ActiveSet::new(vec![ModId::new("a"), ModId::new("ludeon.rimworld")], &inv)
            .expect("a mod loading before Core is valid");

        assert_eq!(set.ids(), [ModId::new("a"), ModId::new("ludeon.rimworld")]);
    }

    #[test]
    fn new_accepts_core_first() {
        let inv = inventory(&["ludeon.rimworld", "a"], &[]);

        let set = ActiveSet::new(vec![ModId::new("ludeon.rimworld"), ModId::new("a")], &inv)
            .expect("Core first is valid");

        assert_eq!(set.ids()[0], ModId::new("ludeon.rimworld"));
    }

    #[test]
    fn activate_appends_in_the_given_order_and_skips_already_active() {
        let inv = inventory(&["a"], &["b", "c"]);
        let mut set = ActiveSet::trusted(vec![ModId::new("a")]);

        let added = set
            .activate(vec![ModId::new("c"), ModId::new("b")], &inv)
            .expect("both are known");

        assert_eq!(added, [ModId::new("c"), ModId::new("b")]);
        assert_eq!(
            set.ids(),
            [ModId::new("a"), ModId::new("c"), ModId::new("b")]
        );
    }

    #[test]
    fn activate_is_steam_aware_for_the_already_active_check() {
        let inv = inventory(&[], &["x.mod", "x.mod_steam"]);
        let mut set = ActiveSet::trusted(vec![ModId::new("x.mod")]);

        let added = set
            .activate(vec![ModId::new("x.mod_steam")], &inv)
            .expect("known id");

        assert!(
            added.is_empty(),
            "x.mod is already active under its base id"
        );
        assert_eq!(set.ids(), [ModId::new("x.mod")]);
    }

    #[test]
    fn activate_rejects_an_unknown_id_and_leaves_the_set_untouched() {
        let inv = inventory(&["a"], &["b"]);
        let mut set = ActiveSet::trusted(vec![ModId::new("a")]);

        let error = set
            .activate(vec![ModId::new("b"), ModId::new("ghost")], &inv)
            .expect_err("ghost is unknown");

        assert_eq!(error, ActiveSetError::Unknown(ModId::new("ghost")));
        assert_eq!(
            set.ids(),
            [ModId::new("a")],
            "no id — not even the known one before the unknown one — was appended"
        );
    }

    #[test]
    fn deactivate_removes_matching_exact_ids() {
        let mut set = ActiveSet::trusted(vec![ModId::new("a"), ModId::new("b")]);

        let removed = set.deactivate(&[ModId::new("b")]);

        assert_eq!(removed, [ModId::new("b")]);
        assert_eq!(set.ids(), [ModId::new("a")]);
    }

    #[test]
    fn deactivate_refuses_core() {
        let mut set = ActiveSet::trusted(vec![ModId::new("ludeon.rimworld"), ModId::new("a")]);

        let removed = set.deactivate(&[ModId::new("ludeon.rimworld")]);

        assert!(removed.is_empty());
        assert_eq!(set.ids(), [ModId::new("ludeon.rimworld"), ModId::new("a")]);
    }

    #[test]
    fn deactivate_is_a_no_op_for_an_id_not_present() {
        let mut set = ActiveSet::trusted(vec![ModId::new("a")]);

        let removed = set.deactivate(&[ModId::new("b")]);

        assert!(removed.is_empty());
        assert_eq!(set.ids(), [ModId::new("a")]);
    }

    #[test]
    fn diff_reports_added_and_removed_by_exact_id() {
        let newer = ActiveSet::trusted(vec![ModId::new("a"), ModId::new("c")]);
        let older = ActiveSet::trusted(vec![ModId::new("a"), ModId::new("b")]);

        let diff = newer.diff(&older);

        assert_eq!(diff.added, [ModId::new("c")]);
        assert_eq!(diff.removed, [ModId::new("b")]);
        assert!(!diff.is_empty());
    }

    #[test]
    fn diff_of_a_set_with_itself_is_empty() {
        let set = ActiveSet::trusted(vec![ModId::new("a")]);

        assert!(set.diff(&set).is_empty());
    }

    #[test]
    fn plan_activate_walks_the_dependency_closure_deps_first() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("a.mod")
            .inactive("b.mod")
            .inactive_dependency("a.mod", "b.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("a.mod")], true);

        assert_eq!(plan.to_add, [ModId::new("b.mod"), ModId::new("a.mod")]);
        assert!(plan.unresolvable_dependencies.is_empty());
        assert!(plan.already_active.is_empty());
    }

    #[test]
    fn plan_activate_reports_an_unresolvable_dependency_without_adding_it() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("a.mod")
            .inactive_dependency("a.mod", "missing.framework")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("a.mod")], true);

        assert_eq!(plan.to_add, [ModId::new("a.mod")]);
        assert_eq!(
            plan.unresolvable_dependencies.get(&ModId::new("a.mod")),
            Some(&vec![ModId::new("missing.framework")])
        );
    }

    #[test]
    fn plan_activate_with_dependencies_false_leaves_the_dependency_out_and_unreported() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("a.mod")
            .inactive("b.mod")
            .inactive_dependency("a.mod", "b.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("a.mod")], false);

        assert_eq!(
            plan.to_add,
            [ModId::new("a.mod")],
            "the dependency itself is left out of to_add"
        );
        assert!(
            plan.unresolvable_dependencies.is_empty(),
            "the dependency chain is never walked, so nothing about it is reported either"
        );
    }

    #[test]
    fn plan_activate_reports_already_active_ids_and_leaves_working_out_of_to_add() {
        let inv = inventory(&["a"], &["b"]);
        let active = ActiveSet::trusted(vec![ModId::new("a")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("a"), ModId::new("b")], false);

        assert_eq!(plan.already_active, [ModId::new("a")]);
        assert_eq!(plan.to_add, [ModId::new("b")]);
    }

    /// A two-mod dependency cycle (`a` depends on
    /// `b`, `b` depends on `a`) neither infinitely recurses nor drops
    /// either mod — the second re-entry into an already-`scheduled` id is
    /// a no-op (nothing more to walk), and the *first* re-entry's own
    /// post-order push still lands both ids in `to_add`, dependency
    /// before dependent at each level.
    #[test]
    fn plan_activate_walks_a_two_mod_cycle_without_looping_or_dropping_either_mod() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("a.mod")
            .inactive("b.mod")
            .inactive_dependency("a.mod", "b.mod")
            .inactive_dependency("b.mod", "a.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("a.mod")], true);

        assert_eq!(plan.to_add, [ModId::new("a.mod"), ModId::new("b.mod")]);
        assert!(plan.unresolvable_dependencies.is_empty());
    }

    /// The self-dependency case: a mod declaring itself as its own
    /// dependency is scheduled exactly once, not looped on forever.
    #[test]
    fn plan_activate_treats_a_self_dependency_as_a_closed_cycle_of_one() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("c.mod")
            .inactive_dependency("c.mod", "c.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(&active, &inv, &[ModId::new("c.mod")], true);

        assert_eq!(plan.to_add, [ModId::new("c.mod")]);
        assert!(plan.unresolvable_dependencies.is_empty());
    }

    /// Requesting `b` (whose own declared
    /// dependency resolves to the bare `a`) and `a_steam` in the same
    /// call must not silently drop `a_steam` — its base is already
    /// covered by `b`'s own closure, so it's reported in
    /// `already_active` rather than vanishing from both fields.
    #[test]
    fn plan_activate_routes_a_request_whose_base_another_requests_closure_already_covers_to_already_active()
     {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .inactive("a")
            .inactive("a_steam")
            .inactive("b")
            .inactive_dependency("b", "a")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![ModId::new("core.mod")]);

        let plan = plan_activate(
            &active,
            &inv,
            &[ModId::new("b"), ModId::new("a_steam")],
            true,
        );

        assert_eq!(
            plan.to_add,
            [ModId::new("a"), ModId::new("b")],
            "b's own closure resolves the declared dependency to the bare `a`"
        );
        assert_eq!(
            plan.already_active,
            [ModId::new("a_steam")],
            "a_steam's base is already scheduled — reported, not dropped"
        );
    }

    #[test]
    fn plan_deactivate_refuses_core_and_reports_dependents() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("ludeon.rimworld")
            .mod_("target.mod")
            .mod_("dependent.mod")
            .dependency("dependent.mod", "target.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let active = ActiveSet::trusted(vec![
            ModId::new("ludeon.rimworld"),
            ModId::new("target.mod"),
            ModId::new("dependent.mod"),
        ]);

        let plan = plan_deactivate(
            &active,
            &inv,
            &[ModId::new("ludeon.rimworld"), ModId::new("target.mod")],
        );

        assert_eq!(plan.refused, [ModId::new("ludeon.rimworld")]);
        assert_eq!(plan.to_remove, [ModId::new("target.mod")]);
        assert_eq!(
            plan.dependents_still_active.get(&ModId::new("target.mod")),
            Some(&vec![ModId::new("dependent.mod")])
        );
    }
}
