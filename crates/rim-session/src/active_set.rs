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

/// Where a move or an anchored activation puts its mods, named by a
/// neighbour's id rather than an index, so it stays exact under filtering
/// and sorting of the *view* and a stale reference is detectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// Directly before this active mod.
    Before(ModId),
    /// Directly after this active mod.
    After(ModId),
    /// At the start of the list.
    Start,
    /// At the end of the list.
    End,
}

/// Why [`ActiveSet::move_to`] or [`ActiveSet::activate_at`] refused. Every
/// refusal leaves the set untouched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// The anchor names a mod that is not in the set (it was deactivated, or
    /// the caller's view is stale).
    #[error("{0} is not active, so nothing can be placed next to it")]
    AnchorNotActive(ModId),
    /// The anchor is one of the mods being moved.
    #[error("{0} is one of the mods being moved, so it cannot be the anchor")]
    AnchorInMovedSet(ModId),
    /// A mod to move is not in the set.
    #[error("{0} is not active, so it cannot be moved")]
    NotActive(ModId),
    /// The same mod was listed twice in one request.
    #[error("{0} was listed more than once")]
    Duplicate(ModId),
    /// A mod to activate resolves to nothing in the inventory.
    #[error("{0} is not a known mod")]
    Unknown(ModId),
}

/// The index in `ids` where a block placed at `anchor` starts.
fn insertion_index(ids: &[ModId], anchor: &Anchor) -> Result<usize, MoveError> {
    let position_of = |id: &ModId| {
        ids.iter()
            .position(|active| active == id)
            .ok_or_else(|| MoveError::AnchorNotActive(id.clone()))
    };
    match anchor {
        Anchor::Start => Ok(0),
        Anchor::End => Ok(ids.len()),
        Anchor::Before(id) => position_of(id),
        Anchor::After(id) => position_of(id).map(|position| position + 1),
    }
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

    /// Moves every id in `ids` next to `anchor`, keeping them together and in
    /// the relative order they already have in this set (so moving a
    /// selection never shuffles it). A move is a permutation: nothing is
    /// added, dropped or duplicated. Everything is validated before the set
    /// changes.
    ///
    /// # Errors
    ///
    /// [`MoveError::Duplicate`] for a repeated id, [`MoveError::NotActive`]
    /// for an id not in the set, [`MoveError::AnchorInMovedSet`] when the
    /// anchor is among `ids`, [`MoveError::AnchorNotActive`] when it is in
    /// no part of the set.
    pub fn move_to(&mut self, ids: &[ModId], anchor: &Anchor) -> Result<(), MoveError> {
        let mut requested = BTreeSet::new();
        for id in ids {
            if !requested.insert(id.clone()) {
                return Err(MoveError::Duplicate(id.clone()));
            }
            if !self.contains(id) {
                return Err(MoveError::NotActive(id.clone()));
            }
        }
        if let Anchor::Before(id) | Anchor::After(id) = anchor
            && requested.contains(id)
        {
            return Err(MoveError::AnchorInMovedSet(id.clone()));
        }
        let (moved, mut remaining): (Vec<ModId>, Vec<ModId>) = self
            .ids
            .iter()
            .cloned()
            .partition(|id| requested.contains(id));
        let at = insertion_index(&remaining, anchor)?;
        remaining.splice(at..at, moved);
        self.ids = remaining;
        Ok(())
    }

    /// Checks that `anchor` could be used now: `Start`/`End` always, a
    /// neighbour only while it is in the set.
    ///
    /// # Errors
    ///
    /// [`MoveError::AnchorNotActive`].
    pub fn check_anchor(&self, anchor: &Anchor) -> Result<(), MoveError> {
        insertion_index(&self.ids, anchor).map(|_| ())
    }

    /// Activates `ids` as one block placed at `anchor`, in the order given
    /// (a plan's closure order: dependencies first), skipping any id already
    /// active by base id. Validated before the set changes. Returns what
    /// was inserted.
    ///
    /// # Errors
    ///
    /// [`MoveError::Unknown`] for an id the inventory does not know,
    /// [`MoveError::AnchorNotActive`] for a stale anchor.
    pub fn activate_at(
        &mut self,
        ids: Vec<ModId>,
        anchor: &Anchor,
        inventory: &ModInventory,
    ) -> Result<Vec<ModId>, MoveError> {
        let added = self
            .to_activate(ids, inventory)
            .map_err(MoveError::Unknown)?;
        let at = insertion_index(&self.ids, anchor)?;
        self.ids.splice(at..at, added.iter().cloned());
        Ok(added)
    }

    /// Activates `ids` with no anchor: a mod that has a slot in `reference`
    /// (the scanned Current order) goes right after its nearest
    /// predecessor there that is in this set, or at the start; any other
    /// mod goes to the end. Deactivating a mod and activating it again
    /// therefore puts it back where it was.
    ///
    /// Dependencies still come first: `ids` is a plan (dependencies before
    /// their dependents), and a mod never lands before a declared dependency
    /// that is in the set by then, whether this same call inserted it or it was
    /// active already (a dependency moved behind the mod's old slot). When
    /// its own slot would put it earlier, it goes right after that dependency
    /// instead.
    ///
    /// Validated before the set changes. Returns what was inserted.
    ///
    /// # Errors
    ///
    /// [`ActiveSetError::Unknown`] for an id the inventory does not know.
    pub(crate) fn activate_at_reference_slots(
        &mut self,
        ids: Vec<ModId>,
        reference: &[ModId],
        inventory: &ModInventory,
    ) -> Result<Vec<ModId>, ActiveSetError> {
        let added = self
            .to_activate(ids, inventory)
            .map_err(ActiveSetError::Unknown)?;
        for id in &added {
            let slot = self.reference_slot(id, reference);
            let floor = self.after_declared_dependencies(id, inventory);
            self.ids.insert(slot.max(floor), id.clone());
        }
        Ok(added)
    }

    /// The first position after every declared dependency of `id` that is in
    /// the set now (placed earlier in this call, or active before it), `0`
    /// when none.
    fn after_declared_dependencies(&self, id: &ModId, inventory: &ModInventory) -> usize {
        let Some(entry) = inventory.entry(id) else {
            return 0;
        };
        entry
            .declared_dependencies
            .iter()
            .filter_map(|dependency| {
                let base = dependency.base();
                self.ids.iter().rposition(|active| active.base() == base)
            })
            .map(|position| position + 1)
            .max()
            .unwrap_or(0)
    }

    /// The ids of `ids` that are not yet active, deduplicated, once every
    /// one is known to `inventory`; the error is the first unknown id.
    fn to_activate(&self, ids: Vec<ModId>, inventory: &ModInventory) -> Result<Vec<ModId>, ModId> {
        if let Some(unknown) = ids.iter().find(|id| !inventory.contains(id)) {
            return Err(unknown.clone());
        }
        let mut added: Vec<ModId> = Vec::new();
        for id in ids {
            if !self.contains_base(&id) && !added.contains(&id) {
                added.push(id);
            }
        }
        Ok(added)
    }

    /// Where `id` goes among `self.ids` to sit in `reference`'s slot.
    fn reference_slot(&self, id: &ModId, reference: &[ModId]) -> usize {
        let Some(slot) = reference.iter().position(|candidate| candidate == id) else {
            return self.ids.len();
        };
        reference[..slot]
            .iter()
            .rev()
            .find_map(|predecessor| self.ids.iter().position(|active| active == predecessor))
            .map_or(0, |position| position + 1)
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
/// what the scanned order has that Rimmerge hasn't written to
/// `ModsConfig.xml` yet (`unapplied`).
///
/// Both are facts about the user's own unsaved work, so the `unapplied`
/// side is measured against the list Rimmerge last loaded or wrote (the
/// acknowledged list), never against the live file: someone else editing
/// the file changes neither field, and the outside-change note is the one
/// place that says so. The launch status and the dashboard's
/// `file_matches` facts are the live comparison
/// ([`crate::Session::file_matches`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingActiveChanges {
    /// `working` vs. the scanned `orders().current`.
    pub unscanned: ActiveSetDiff,
    /// The scanned `orders().current` vs. the acknowledged list: what an
    /// Apply of the user's own edits would add or remove.
    pub unapplied: ActiveSetDiff,
    /// `orders().current` differs from the acknowledged list in membership
    /// or sequence (the generated merge mod ignored on both sides), so an
    /// Apply would write the user's own edit. `unapplied` is a membership
    /// diff and cannot see a reorder: a settled move leaves it empty.
    pub order_unapplied: bool,
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

    fn ids_of(set: &ActiveSet) -> Vec<&str> {
        set.ids().iter().map(ModId::as_str).collect()
    }

    fn working(ids: &[&str]) -> ActiveSet {
        ActiveSet::trusted(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    fn mods(ids: &[&str]) -> Vec<ModId> {
        ids.iter().map(|id| ModId::new(*id)).collect()
    }

    #[test]
    fn move_to_places_mods_before_an_anchor() {
        let mut set = working(&["a", "b", "c", "d"]);

        set.move_to(&mods(&["d"]), &Anchor::Before(ModId::new("b")))
            .expect("valid move");

        assert_eq!(ids_of(&set), ["a", "d", "b", "c"]);
    }

    #[test]
    fn move_to_places_mods_after_an_anchor() {
        let mut set = working(&["a", "b", "c", "d"]);

        set.move_to(&mods(&["a"]), &Anchor::After(ModId::new("c")))
            .expect("valid move");

        assert_eq!(ids_of(&set), ["b", "c", "a", "d"]);
    }

    #[test]
    fn move_to_start_and_end() {
        let mut set = working(&["a", "b", "c"]);

        set.move_to(&mods(&["c"]), &Anchor::Start).expect("valid");
        assert_eq!(ids_of(&set), ["c", "a", "b"]);
        set.move_to(&mods(&["c"]), &Anchor::End).expect("valid");

        assert_eq!(ids_of(&set), ["a", "b", "c"]);
    }

    #[test]
    fn move_to_keeps_the_moved_mods_in_their_own_relative_order() {
        let mut set = working(&["a", "b", "c", "d", "e"]);

        // Requested out of order: the set's own order wins.
        set.move_to(&mods(&["d", "b"]), &Anchor::After(ModId::new("e")))
            .expect("valid move");

        assert_eq!(ids_of(&set), ["a", "c", "e", "b", "d"]);
    }

    #[test]
    fn move_to_rejects_a_stale_anchor_and_leaves_the_set_untouched() {
        let mut set = working(&["a", "b"]);

        let error = set
            .move_to(&mods(&["a"]), &Anchor::After(ModId::new("gone")))
            .expect_err("gone is not active");

        assert_eq!(error, MoveError::AnchorNotActive(ModId::new("gone")));
        assert_eq!(ids_of(&set), ["a", "b"]);
    }

    #[test]
    fn move_to_rejects_an_anchor_inside_the_moved_set() {
        let mut set = working(&["a", "b", "c"]);

        let error = set
            .move_to(&mods(&["a", "b"]), &Anchor::Before(ModId::new("b")))
            .expect_err("b is being moved");

        assert_eq!(error, MoveError::AnchorInMovedSet(ModId::new("b")));
        assert_eq!(ids_of(&set), ["a", "b", "c"]);
    }

    #[test]
    fn move_to_rejects_an_inactive_or_repeated_id_without_changing_anything() {
        let mut set = working(&["a", "b", "c"]);

        let not_active = set
            .move_to(&mods(&["a", "ghost"]), &Anchor::End)
            .expect_err("ghost is not active");
        let repeated = set
            .move_to(&mods(&["a", "a"]), &Anchor::End)
            .expect_err("a is listed twice");

        assert_eq!(not_active, MoveError::NotActive(ModId::new("ghost")));
        assert_eq!(repeated, MoveError::Duplicate(ModId::new("a")));
        assert_eq!(ids_of(&set), ["a", "b", "c"]);
    }

    #[test]
    fn move_to_can_move_core_but_deactivate_still_refuses_it() {
        let mut set = working(&["a", "ludeon.rimworld"]);

        set.move_to(&mods(&["ludeon.rimworld"]), &Anchor::Start)
            .expect("Core is movable");
        let removed = set.deactivate(&mods(&["ludeon.rimworld"]));

        assert_eq!(ids_of(&set), ["ludeon.rimworld", "a"]);
        assert!(removed.is_empty());
    }

    #[test]
    fn move_to_with_nothing_to_move_changes_nothing() {
        let mut set = working(&["a", "b"]);

        set.move_to(&[], &Anchor::Start).expect("empty is valid");

        assert_eq!(ids_of(&set), ["a", "b"]);
    }

    #[test]
    fn activate_at_inserts_the_dependency_closure_before_the_anchor_dependencies_first() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .mod_("z.mod")
            .inactive("a.mod")
            .inactive("b.mod")
            .inactive_dependency("a.mod", "b.mod")
            .build();
        let inv = ModInventory::from_report(&report);
        let mut set = working(&["core.mod", "z.mod"]);
        let plan = plan_activate(&set, &inv, &mods(&["a.mod"]), true);

        let added = set
            .activate_at(plan.to_add, &Anchor::Before(ModId::new("z.mod")), &inv)
            .expect("valid");

        assert_eq!(added, mods(&["b.mod", "a.mod"]));
        assert_eq!(ids_of(&set), ["core.mod", "b.mod", "a.mod", "z.mod"]);
    }

    #[test]
    fn activate_at_after_start_and_end() {
        let inv = inventory(&["a", "b"], &["x"]);
        let mut after = working(&["a", "b"]);
        let mut start = working(&["a", "b"]);
        let mut end = working(&["a", "b"]);

        after
            .activate_at(mods(&["x"]), &Anchor::After(ModId::new("a")), &inv)
            .expect("valid");
        start
            .activate_at(mods(&["x"]), &Anchor::Start, &inv)
            .expect("valid");
        end.activate_at(mods(&["x"]), &Anchor::End, &inv)
            .expect("valid");

        assert_eq!(ids_of(&after), ["a", "x", "b"]);
        assert_eq!(ids_of(&start), ["x", "a", "b"]);
        assert_eq!(ids_of(&end), ["a", "b", "x"]);
    }

    #[test]
    fn activate_at_rejects_a_stale_anchor_or_unknown_id_and_leaves_the_set_untouched() {
        let inv = inventory(&["a"], &["x"]);
        let mut set = working(&["a"]);

        let stale = set
            .activate_at(mods(&["x"]), &Anchor::Before(ModId::new("gone")), &inv)
            .expect_err("gone is not active");
        let unknown = set
            .activate_at(mods(&["x", "ghost"]), &Anchor::End, &inv)
            .expect_err("ghost is unknown");

        assert_eq!(stale, MoveError::AnchorNotActive(ModId::new("gone")));
        assert_eq!(unknown, MoveError::Unknown(ModId::new("ghost")));
        assert_eq!(ids_of(&set), ["a"]);
    }

    #[test]
    fn activate_at_skips_a_mod_already_active_by_base_id() {
        let inv = inventory(&["a"], &["x", "x_steam"]);
        let mut set = working(&["a", "x"]);

        let added = set
            .activate_at(mods(&["x_steam"]), &Anchor::Start, &inv)
            .expect("valid");

        assert!(added.is_empty());
        assert_eq!(ids_of(&set), ["a", "x"]);
    }

    #[test]
    fn reference_slots_put_a_mod_after_its_nearest_active_predecessor() {
        let inv = inventory(&["a", "c"], &["b"]);
        let mut set = working(&["a", "c"]);

        set.activate_at_reference_slots(mods(&["b"]), &mods(&["a", "b", "c"]), &inv)
            .expect("known");

        assert_eq!(ids_of(&set), ["a", "b", "c"]);
    }

    #[test]
    fn reference_slots_skip_predecessors_that_are_not_active() {
        let inv = inventory(&["a", "d"], &["b", "c"]);
        let mut set = working(&["a", "d"]);

        // Both come back at once, in plan order (later slot first): each is
        // placed against the reference, never by arrival order.
        set.activate_at_reference_slots(mods(&["c", "b"]), &mods(&["a", "b", "c", "d"]), &inv)
            .expect("known");

        assert_eq!(ids_of(&set), ["a", "b", "c", "d"]);
    }

    #[test]
    fn reference_slots_put_a_mod_with_no_active_predecessor_at_the_start() {
        let inv = inventory(&["b"], &["a"]);
        let mut set = working(&["b"]);

        set.activate_at_reference_slots(mods(&["a"]), &mods(&["a", "b"]), &inv)
            .expect("known");

        assert_eq!(ids_of(&set), ["a", "b"]);
    }

    #[test]
    fn reference_slots_append_a_mod_the_reference_does_not_know() {
        let inv = inventory(&["a", "b"], &["new"]);
        let mut set = working(&["a", "b"]);

        set.activate_at_reference_slots(mods(&["new"]), &mods(&["a", "b"]), &inv)
            .expect("known");

        assert_eq!(ids_of(&set), ["a", "b", "new"]);
    }

    /// `dependent` declares `dependency`; both inactive, `anchors` active.
    fn dependency_inventory(anchors: &[&str]) -> ModInventory {
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        for id in anchors {
            builder = builder.mod_(id);
        }
        let report = builder
            .inactive("dependent")
            .inactive("dependency")
            .inactive_dependency("dependent", "dependency")
            .build();
        ModInventory::from_report(&report)
    }

    #[test]
    fn reference_slots_keep_a_dependency_before_its_dependent_when_it_has_no_slot() {
        let inv = dependency_inventory(&["m", "z"]);
        let mut set = working(&["m", "z"]);

        // The reference knows `dependent` only: its slot (after `m`) would
        // otherwise leave the slot-less dependency, appended, behind it.
        set.activate_at_reference_slots(
            mods(&["dependency", "dependent"]),
            &mods(&["m", "dependent", "z"]),
            &inv,
        )
        .expect("known");

        assert_eq!(ids_of(&set), ["m", "z", "dependency", "dependent"]);
    }

    #[test]
    fn reference_slots_keep_a_dependency_before_its_dependent_when_its_slot_is_later() {
        let inv = dependency_inventory(&["m", "z"]);
        let mut set = working(&["m", "z"]);

        set.activate_at_reference_slots(
            mods(&["dependency", "dependent"]),
            &mods(&["m", "dependent", "z", "dependency"]),
            &inv,
        )
        .expect("known");

        assert_eq!(ids_of(&set), ["m", "z", "dependency", "dependent"]);
    }

    #[test]
    fn reference_slots_still_use_the_dependents_own_slot_when_it_is_already_after_the_dependency() {
        let inv = dependency_inventory(&["m", "z"]);
        let mut set = working(&["m", "z"]);

        set.activate_at_reference_slots(
            mods(&["dependency", "dependent"]),
            &mods(&["dependency", "m", "z", "dependent"]),
            &inv,
        )
        .expect("known");

        assert_eq!(ids_of(&set), ["dependency", "m", "z", "dependent"]);
    }

    #[test]
    fn reference_slots_keep_a_mod_behind_a_dependency_that_is_already_active() {
        // Current held [dependency, a, dependent]; `dependent` was deactivated
        // and `dependency` moved to the end, so the working set is [a, dependency].
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        builder = builder.mod_("a").mod_("dependency");
        let report = builder
            .inactive("dependent")
            .inactive_dependency("dependent", "dependency")
            .build();
        let inv = ModInventory::from_report(&report);
        let mut set = working(&["a", "dependency"]);

        set.activate_at_reference_slots(
            mods(&["dependent"]),
            &mods(&["dependency", "a", "dependent"]),
            &inv,
        )
        .expect("known");

        assert_eq!(ids_of(&set), ["a", "dependency", "dependent"]);
    }

    #[test]
    fn reference_slots_reject_an_unknown_id_and_leave_the_set_untouched() {
        let inv = inventory(&["a"], &["x"]);
        let mut set = working(&["a"]);

        let error = set
            .activate_at_reference_slots(mods(&["x", "ghost"]), &mods(&["a"]), &inv)
            .expect_err("ghost is unknown");

        assert_eq!(error, ActiveSetError::Unknown(ModId::new("ghost")));
        assert_eq!(ids_of(&set), ["a"]);
    }

    mod move_properties {
        use proptest::prelude::*;

        use super::*;

        const POOL: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

        fn anchor_strategy() -> impl Strategy<Value = Anchor> {
            prop_oneof![
                Just(Anchor::Start),
                Just(Anchor::End),
                (0..POOL.len()).prop_map(|i| Anchor::Before(ModId::new(POOL[i]))),
                (0..POOL.len()).prop_map(|i| Anchor::After(ModId::new(POOL[i]))),
            ]
        }

        proptest! {
            /// Whatever the request, a successful move permutes the set and
            /// a refused one leaves it as it was: never a duplicate, never a
            /// loss.
            #[test]
            fn any_move_is_a_permutation_or_changes_nothing(
                requested in proptest::collection::vec(0..POOL.len(), 0..5),
                anchor in anchor_strategy(),
                active_count in 1..=POOL.len(),
            ) {
                let mut set = working(&POOL[..active_count]);
                let before = set.ids().to_vec();
                let ids: Vec<ModId> = requested.iter().map(|i| ModId::new(POOL[*i])).collect();

                let result = set.move_to(&ids, &anchor);

                let mut after_sorted = set.ids().to_vec();
                after_sorted.sort();
                let mut before_sorted = before.clone();
                before_sorted.sort();
                prop_assert_eq!(after_sorted, before_sorted);
                if result.is_err() {
                    prop_assert_eq!(set.ids(), before.as_slice());
                }
            }

            #[test]
            fn a_move_puts_the_moved_block_right_next_to_its_anchor(
                requested in proptest::collection::btree_set(0..POOL.len(), 1..4),
                anchor_index in 0..POOL.len(),
                is_before in any::<bool>(),
            ) {
                let mut set = working(&POOL);
                let ids: Vec<ModId> = requested.iter().map(|i| ModId::new(POOL[*i])).collect();
                let anchor_id = ModId::new(POOL[anchor_index]);
                let anchor = if is_before {
                    Anchor::Before(anchor_id.clone())
                } else {
                    Anchor::After(anchor_id.clone())
                };

                if set.move_to(&ids, &anchor).is_ok() {
                    let position = |id: &ModId| set.ids().iter().position(|x| x == id);
                    let positions: Vec<usize> = ids.iter().filter_map(position).collect();
                    let first = positions.iter().copied().min();
                    let last = positions.iter().copied().max();
                    let (Some(first), Some(last), Some(anchor_position)) =
                        (first, last, position(&anchor_id))
                    else {
                        return Err(TestCaseError::fail("a moved id or the anchor vanished"));
                    };
                    prop_assert_eq!(last - first + 1, ids.len(), "the block is contiguous");
                    if is_before {
                        prop_assert_eq!(last + 1, anchor_position);
                    } else {
                        prop_assert_eq!(first, anchor_position + 1);
                    }
                }
            }
        }
    }
}
