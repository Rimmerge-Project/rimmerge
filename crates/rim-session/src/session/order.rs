//! [`Session`]'s load orders: the sorter's outcome, the current/proposed orders, the selected
//! source, the working active set, and the mod lists.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{LoadOrder, ModId};
use rim_resolve::domain::{BothOrders, GeneratedModIdentity, OrderSource};
use rim_resolve::sort::SortOutcome;

use super::Session;
use super::{compute, mods_by_id};
use crate::active_set::{ActiveSet, PendingActiveChanges};
use crate::game_launch::{OrderOnDisk, UnappliedReason};
use crate::mod_index::{self, ModFilter, ModPage};
use crate::ports::ModsConfigFile;
use crate::settings::SortProvenance;

impl Session {
    pub(super) fn recompute_sort(&mut self) {
        let profile_hash = self.paths.profile_hash().to_string();
        let (tagging, sort_outcome, effective_rules) = compute(
            &self.report,
            &self.evidence,
            &self.rules,
            &self.decisions,
            &self.orders.current,
            &profile_hash,
            &self.sources,
        );
        self.tagging = tagging;
        self.orders.suggested = sort_outcome.order.clone();
        self.sort = sort_outcome;
        self.effective_rules = effective_rules;
        self.invalidate_ledgers();
        // `orders.suggested` just got a new value — every inspection
        // computed against it (or, harmlessly, `Current`) may now answer
        // differently. See `Self::inspections`' own doc comment for why
        // this is separate from `invalidate_ledgers`.
        self.inspections.clear();
        // Same reasoning as `inspections`: which instance "wins" a
        // duplicate defName under `Selector::DefName` depends on the
        // selected order.
        self.assignment_instances.clear();
        self.assignment_coverage.clear();
        self.def_graphics.clear();
    }

    /// Searches, filters, and pages the active mod list. Pure — unlike
    /// the ledger/finding accessors, this never builds or caches
    /// anything (it only reads [`Session::report`]/[`Session::tagging`],
    /// both already computed), so it takes `&self`.
    #[must_use]
    pub fn mods(&self, filter: &ModFilter) -> ModPage {
        mod_index::query(&self.report, &self.tagging, filter)
    }

    /// Searches, filters, and pages `report.inactive_mods`
    /// — the Inactive tab's own
    /// list. Pure, like [`Self::mods`]: only `search`/`source` apply
    /// ([`mod_index::query_inactive`]'s own doc comment has why).
    #[must_use]
    pub fn inactive_mods(&self, filter: &ModFilter) -> ModPage {
        mod_index::query_inactive(&self.report.inactive_mods, filter)
    }

    /// Changes which order is selected for ledger/finding queries.
    pub fn select(&mut self, source: OrderSource) {
        self.selected = source;
    }

    /// The [`ModsConfigFile`] to write for `order`, preserving the
    /// original file's `version`/`known_expansions`.
    #[must_use]
    pub fn mods_config_file_for(&self, order: &LoadOrder) -> ModsConfigFile {
        ModsConfigFile {
            version: self.mods_config_version.clone(),
            active_mods: order.as_slice().to_vec(),
            known_expansions: self.known_expansions.clone(),
        }
    }

    /// Which settings produced [`Session::orders`]`().suggested` right
    /// now — the "smallest
    /// honest place" for this: [`Session::recompute_sort`] always follows
    /// a settings change, so `self.rules.settings` is never stale
    /// relative to the last sort.
    #[must_use]
    pub fn sort_provenance(&self) -> SortProvenance {
        self.rules.settings.sort_provenance()
    }

    /// The most recent sort outcome.
    #[must_use]
    pub fn sort_outcome(&self) -> &SortOutcome {
        &self.sort
    }

    /// The current and suggested load orders.
    #[must_use]
    pub fn orders(&self) -> &BothOrders {
        &self.orders
    }

    /// Replaces [`BothOrders::current`] and invalidates both ledgers (and
    /// every patch's scoped ledger) the same way a decision/rule/tag
    /// change does — `current`'s active-mod set feeds `active_base_ids`,
    /// the `Current`-source ledger, and every scoped ledger built over it.
    ///
    /// Callers that write `ModsConfig.xml` with an order other than
    /// [`Self::orders`]`().current` (`ExportPatch`'s own `install`, and
    /// [`crate::use_cases::Apply`] whenever it appends or removes the
    /// generated merge mod's package id) must call this right after a
    /// successful write. Without it, the in-memory session and the file
    /// on disk drift: a later `Apply { write_mods_config: true }` with
    /// `selected() == OrderSource::Current` would then write the *stale*
    /// in-memory order back out, silently deactivating whatever the
    /// earlier write appended — exactly what would let an `install`
    /// immediately followed by an `apply`, with no reload in between, undo
    /// the install. This
    /// does **not** re-run the sorter (`recompute_sort`): the newly
    /// appended id (e.g. a just-installed patch) isn't in `self.report`
    /// yet — only the next scan picks it up — so there is nothing new for
    /// the sorter to place; only the ledgers, which read `current`
    /// directly, need to go stale.
    ///
    /// [`Self::file_active_mods`] is reset to `order` here too —
    /// this method's own contract is "the file now *is* this", and that
    /// applies to the file's *own* record, not just the in-memory sorter
    /// input. [`Self::working`] is reconciled rather than replaced: the
    /// delta between the *previous* `orders.current` and `order` (an id
    /// the caller just wrote to the real file, added or removed) is
    /// folded into it via [`crate::ActiveSet::force_activate`]/
    /// [`crate::ActiveSet::deactivate`], while any unrelated id already
    /// pending in `working` for a different reason is left exactly as it
    /// was. Without this reconciliation, a call here can never make the
    /// session look stale purely as a side effect of `Apply`/
    /// `ExportPatch::install` writing a package id `working` never heard
    /// about — see this method's own body for the mechanism.
    pub fn set_current_order(&mut self, order: LoadOrder) {
        let previous: BTreeSet<ModId> = self.orders.current.as_slice().iter().cloned().collect();
        let next: BTreeSet<ModId> = order.as_slice().iter().cloned().collect();
        for id in next.difference(&previous) {
            self.working.force_activate(id.clone());
        }
        let newly_absent: Vec<ModId> = previous.difference(&next).cloned().collect();
        if !newly_absent.is_empty() {
            self.working.deactivate(&newly_absent);
        }
        self.orders.current = order;
        self.file_active_mods = self.orders.current.as_slice().to_vec();
        self.invalidate_ledgers();
        // `orders.current` just got a new value — same reasoning as
        // `Self::recompute_sort`'s own clear.
        self.inspections.clear();
        self.assignment_instances.clear();
        self.assignment_coverage.clear();
        self.def_graphics.clear();
    }

    /// Which order is currently selected for ledger/finding queries.
    #[must_use]
    pub fn selected(&self) -> OrderSource {
        self.selected
    }

    /// Every currently active mod's base id.
    #[must_use]
    pub fn active_base_ids(&self) -> BTreeSet<ModId> {
        self.orders
            .current
            .as_slice()
            .iter()
            .map(ModId::base)
            .collect()
    }

    /// Every currently active mod's base id, mapped to its own workshop
    /// id when it has one (`Mod.workshop_id`, `None` for a local/Core/DLC
    /// mod) — the richer view [`crate::ports::RimSortImporter::import`]
    /// filters against, so `rim-io`'s `steamDB.json` importer can match
    /// an upload by workshop id first and fall back to `packageId` only
    /// for a mod with none (two uploads sharing one `packageId` must not be
    /// confused for one another).
    ///
    /// Unlike [`Self::active_base_ids`], this method drops an id from
    /// `orders.current` that has no entry in the report's own mod scan
    /// (listed in `ModsConfig.xml` but not found on disk) instead of
    /// passing it through with no workshop id, so a rule naming such a mod
    /// is treated as inactive at import time (counted in
    /// `skipped_inactive_rules`/`skipped_inactive_steam`, not imported).
    /// This is a deliberate narrowing: a workshop-id lookup has nothing
    /// safe to match against for a mod Rimmerge cannot confirm is present.
    #[must_use]
    pub fn active_mods_by_base_id(&self) -> BTreeMap<ModId, Option<u64>> {
        let mods_by_id = mods_by_id(&self.report);
        self.orders
            .current
            .as_slice()
            .iter()
            .filter_map(|id| mods_by_id.get(id).map(|m| (id.base(), m.workshop_id)))
            .collect()
    }

    // -- Working active-mod set --------

    /// The working (in-memory, unsaved) active-mod list.
    #[must_use]
    pub fn working(&self) -> &ActiveSet {
        &self.working
    }

    /// `pub(crate)`: only [`crate::use_cases::ActivateMods`]/
    /// [`crate::use_cases::DeactivateMods`] (in this crate) mutate the
    /// working set directly.
    pub(crate) fn working_mut(&mut self) -> &mut ActiveSet {
        &mut self.working
    }

    /// What's changed but not yet reflected further along the pipeline:
    /// `unscanned` is [`Self::working`] against the scanned
    /// `orders().current` (what a [`crate::use_cases::Rescan`] would pick
    /// up), `unapplied` is `orders().current` against the file's own
    /// last-known list (what an `apply` would write). Both empty for a
    /// freshly loaded, unedited session.
    #[must_use]
    pub fn pending_changes(&self) -> PendingActiveChanges {
        let current = ActiveSet::trusted(self.orders.current.as_slice().to_vec());
        let file = ActiveSet::trusted(self.file_active_mods.clone());
        PendingActiveChanges {
            unscanned: self.working.diff(&current),
            unapplied: current.diff(&file),
        }
    }

    /// Whether `ModsConfig.xml` (as last scanned or written) already lists
    /// exactly `source`'s order, in the same sequence.
    ///
    /// The profile's generated merge mod is ignored on both sides: `Apply`
    /// appends it to the file, but a suggested order only contains it after
    /// the next scan, so comparing it would report "not applied" right
    /// after a successful apply.
    #[must_use]
    pub fn file_matches(&self, source: OrderSource) -> bool {
        let merge_mod = GeneratedModIdentity::for_profile(self.paths.profile_hash()).package_id;
        let without_merge_mod = |ids: &[ModId]| -> Vec<ModId> {
            ids.iter().filter(|id| **id != merge_mod).cloned().collect()
        };
        without_merge_mod(self.orders.get(source).as_slice())
            == without_merge_mod(&self.file_active_mods)
    }

    /// Whether the working set has changes a rescan hasn't picked up yet
    /// — [`crate::use_cases::Apply`]'s own refuse-until-rescan gate.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        !self.pending_changes().unscanned.is_empty()
    }

    /// Whether `ModsConfig.xml` (as last scanned or written) holds the
    /// *selected* order, for the Launch RimWorld button.
    ///
    /// Unscanned working-set changes come first: they have to be resolved
    /// before an apply can write anything ([`Self::is_stale`]). Otherwise
    /// this is [`Self::file_matches`] for [`Self::selected`], so the
    /// generated merge mod is ignored the same way.
    #[must_use]
    pub fn order_on_disk(&self) -> OrderOnDisk {
        if self.is_stale() {
            return OrderOnDisk::NotApplied(UnappliedReason::ActivationChangesNotScanned);
        }
        if !self.file_matches(self.selected()) {
            return OrderOnDisk::NotApplied(UnappliedReason::OrderDiffers);
        }
        OrderOnDisk::Applied
    }

    // -- Compat patches ---------
}
