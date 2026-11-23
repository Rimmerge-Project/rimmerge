//! [`Session`]'s load orders: the sorter's outcome, the current/proposed orders, the selected
//! source, the working active set, and the mod lists.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{LoadOrder, ModId};
use rim_resolve::domain::{BothOrders, OrderSource};
use rim_resolve::sort::{SortOutcome, TieBreak, compute_disturbance};

use super::Session;
use super::{compute, mods_by_id};
use crate::active_set::{ActiveSet, PendingActiveChanges};
use crate::file_baseline::{
    BaselineChange, BaselineIssue, ExternalFileChange, FileBaseline, FileMembership, FileProbe,
};
use crate::game_launch::{OrderOnDisk, UnappliedReason};
use crate::mod_index::{self, ModFilter, ModPage};
use crate::mod_lists::{
    self, GroupedListQuery, GroupedModList, ModList, ModListError, ModListQuery,
};
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

    /// The Mods page's Active list: the working set in its own order,
    /// every row (up to [`crate::MAX_MOD_LIST_ROWS`]) matching `query`.
    /// Pure (`&self`).
    ///
    /// # Errors
    ///
    /// [`ModListError`] when the query's group filter names a group that
    /// does not exist or organization is unavailable.
    pub fn working_mod_list(&self, query: &ModListQuery) -> Result<ModList, ModListError> {
        mod_lists::working_list(
            &self.report,
            &self.tagging,
            &self.working,
            &self.update_times,
            &FileMembership::of(&self.file_active_mods),
            &self.organization,
            query,
        )
    }

    /// The Mods page's Inactive list: every mod on disk whose base id is
    /// not in the working set, by name (A first) unless `query.sort` says
    /// otherwise. Pure (`&self`).
    ///
    /// # Errors
    ///
    /// See [`Session::working_mod_list`].
    pub fn inactive_mod_list(&self, query: &ModListQuery) -> Result<ModList, ModListError> {
        mod_lists::inactive_list(
            &self.report,
            &self.tagging,
            &self.working,
            &self.update_times,
            &FileMembership::of(&self.file_active_mods),
            &self.organization,
            query,
        )
    }

    /// The Mods page's Active pane in Groups view: the rows of
    /// [`Session::working_mod_list`] laid out as folders. Pure (`&self`).
    ///
    /// # Errors
    ///
    /// [`ModListError::OrganizationUnavailable`] unless groups are loaded
    /// (even with no group filter), [`ModListError::UnknownGroup`] for a
    /// stale group scope.
    pub fn working_mod_groups(
        &self,
        query: &GroupedListQuery,
    ) -> Result<GroupedModList, ModListError> {
        mod_lists::working_groups(
            &self.report,
            &self.tagging,
            &self.working,
            &self.update_times,
            &FileMembership::of(&self.file_active_mods),
            &self.organization,
            query,
        )
    }

    /// The Inactive pane in Groups view. See [`Session::working_mod_groups`].
    ///
    /// # Errors
    ///
    /// See [`Session::working_mod_groups`].
    pub fn inactive_mod_groups(
        &self,
        query: &GroupedListQuery,
    ) -> Result<GroupedModList, ModListError> {
        mod_lists::inactive_groups(
            &self.report,
            &self.tagging,
            &self.working,
            &self.update_times,
            &FileMembership::of(&self.file_active_mods),
            &self.organization,
            query,
        )
    }

    /// Changes which order is selected for ledger/finding queries.
    pub fn select(&mut self, source: OrderSource) {
        self.selected = source;
    }

    /// The selection rule of a manual reorder: a user who places mods
    /// means their own order, so a selected Suggested becomes Current
    /// (Apply writes the selected order). Returns whether it switched.
    /// Used by [`crate::use_cases::MoveMods`] and an anchored
    /// [`crate::use_cases::ActivateMods::execute_at`], never by an
    /// un-anchored activation.
    pub(crate) fn switch_to_current_after_reorder(&mut self) -> bool {
        let is_suggested = self.selected == OrderSource::Suggested;
        if is_suggested {
            self.selected = OrderSource::Current;
        }
        is_suggested
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
    /// directly, need to go stale. The one exception is a new *sequence*
    /// over the *same* ids under [`TieBreak::PreserveCurrent`], where
    /// `suggested` ranks by `current`'s positions and is re-sorted. The
    /// working set also follows `current`'s sequence whenever both hold the
    /// same ids (`Self::settle_working_onto_current`).
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
        let is_reorder_of_same_ids =
            previous == next && self.orders.current.as_slice() != order.as_slice();
        self.orders.current = order;
        self.settle_working_onto_current();
        self.file_active_mods = self.orders.current.as_slice().to_vec();
        self.file_watch.reset(&self.file_active_mods);
        self.current_order_replaced(is_reorder_of_same_ids);
    }

    /// What to read to refresh the file baseline: the path and the epoch
    /// the result must still match when it is committed.
    #[must_use]
    pub fn file_probe(&self) -> FileProbe {
        FileProbe {
            path: self.paths.mods_config.clone(),
            epoch: self.file_watch.epoch(),
        }
    }

    /// Whether the baseline is what the file holds now, or the last valid
    /// list because the latest read failed.
    #[must_use]
    pub fn file_baseline(&self) -> FileBaseline {
        self.file_watch.baseline()
    }

    /// What someone else did to the file since Rimmerge last loaded or wrote
    /// it, by exact id (a swap between two copies of a mod counts).
    #[must_use]
    pub fn external_file_change(&self) -> ExternalFileChange {
        self.file_watch.change_to(&self.file_active_mods)
    }

    /// Swaps one read of the file into the baseline. A success replaces
    /// `file_active_mods`; a failure keeps it and records the reason, so a
    /// truncated or missing file never reads as an empty list. A read
    /// whose probe predates a load or write of Rimmerge's own is dropped.
    /// Never touches `working`, the orders or the file.
    pub(crate) fn observe_file(
        &mut self,
        probe: &FileProbe,
        outcome: Result<Vec<ModId>, BaselineIssue>,
    ) -> BaselineChange {
        if probe.epoch != self.file_watch.epoch() || probe.path != self.paths.mods_config {
            return BaselineChange::Unchanged;
        }
        let baseline_before = self.file_watch.baseline();
        match outcome {
            Ok(ids) => {
                self.file_watch.set_baseline(FileBaseline::Fresh);
                let list_changed = ids != self.file_active_mods;
                self.file_active_mods = ids;
                change_if(list_changed || baseline_before != FileBaseline::Fresh)
            }
            Err(reason) => {
                let stale = FileBaseline::Stale { reason };
                self.file_watch.set_baseline(stale);
                change_if(baseline_before != stale)
            }
        }
    }

    /// What every replacement of `orders.current` after load must do:
    /// invalidate the ledger family and the selected-order caches, and
    /// re-sort when Suggested ranks by Current's own sequence.
    ///
    /// `PreserveCurrent` ranks unconstrained mods by `current`'s own
    /// sequence, so a new sequence over the same ids makes `suggested`
    /// stale. A changed id set never re-sorts (a just-written id is not in
    /// the report yet).
    fn current_order_replaced(&mut self, is_reorder_of_same_ids: bool) {
        self.invalidate_ledgers();
        // `orders.current` just got a new value — same reasoning as
        // `Self::recompute_sort`'s own clear.
        self.inspections.clear();
        self.assignment_instances.clear();
        self.assignment_coverage.clear();
        self.def_graphics.clear();
        if is_reorder_of_same_ids && self.rules.settings.tie_break == TieBreak::PreserveCurrent {
            self.recompute_sort();
        }
    }

    /// The file-truth direction of the settle rule: when `working` and
    /// `orders.current` hold exactly the same ids (duplicates respected),
    /// `working` takes Current's sequence. Runs at the end of
    /// [`Self::set_current_order`] (Apply, an install export, a revert),
    /// where the file is the truth. A rescan scans `working`'s sequence, so
    /// without this a rescan right after an `Apply` would scan the
    /// pre-apply order and revert Current. A `working` with pending
    /// membership edits keeps its own sequence: nothing was settled yet.
    fn settle_working_onto_current(&mut self) {
        if self.working_has_current_members() {
            self.working = ActiveSet::trusted(self.orders.current.as_slice().to_vec());
        }
    }

    /// The edit direction of the settle rule: after an edit to `working`
    /// made from the Mods page (a move, an activation, a deactivation),
    /// when `working` and `orders.current` hold exactly the same ids,
    /// Current takes `working`'s sequence. The edit is the user's intent,
    /// so it is not undone. Nothing is written: `file_active_mods` stays
    /// what the file held, so the order shows as not yet in the file until
    /// an Apply. Invalidates exactly what [`Self::set_current_order`]
    /// does. With pending membership edits nothing is settled, and the
    /// next rescan carries the sequence.
    pub(crate) fn settle_current_onto_working(&mut self) {
        if !self.working_has_current_members()
            || self.working.ids() == self.orders.current.as_slice()
        {
            return;
        }
        self.orders.current = LoadOrder::new(self.working.ids().to_vec());
        self.current_order_replaced(true);
    }

    /// Whether `working` and `orders.current` hold the same ids, duplicates
    /// respected, in any sequence.
    fn working_has_current_members(&self) -> bool {
        let mut working_ids = self.working.ids().to_vec();
        let mut current_ids = self.orders.current.as_slice().to_vec();
        working_ids.sort();
        current_ids.sort();
        working_ids == current_ids
    }

    /// How many mods the suggested order places differently from the
    /// current one, measured over the live orders — never a snapshot from
    /// the last sort, which goes stale the moment `Apply` calls
    /// [`Self::set_current_order`].
    #[must_use]
    pub fn moved_mods(&self) -> usize {
        compute_disturbance(&self.orders.suggested, &self.orders.current).positions_changed
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

    /// `pub(crate)`: only the use cases in this crate
    /// ([`crate::use_cases::ActivateMods`], [`crate::use_cases::DeactivateMods`],
    /// [`crate::use_cases::MoveMods`], [`crate::use_cases::DiscardPendingChanges`])
    /// mutate the working set directly.
    pub(crate) fn working_mut(&mut self) -> &mut ActiveSet {
        &mut self.working
    }

    /// What's changed but not yet reflected further along the pipeline:
    /// `unscanned` is [`Self::working`] against the scanned
    /// `orders().current` (what a [`crate::use_cases::Rescan`] would pick
    /// up), `unapplied` is `orders().current` against the list Rimmerge
    /// last loaded or wrote (what an `apply` of the user's own edits would
    /// change), and `order_unapplied` is the same comparison by sequence,
    /// which is what a settled reorder changes. The live file is never
    /// consulted: an outside edit is not the user's unsaved work (see
    /// [`Self::external_file_change`]). All empty/false for a freshly
    /// loaded, unedited session.
    #[must_use]
    pub fn pending_changes(&self) -> PendingActiveChanges {
        let current = ActiveSet::trusted(self.orders.current.as_slice().to_vec());
        let acknowledged = ActiveSet::trusted(self.file_watch.acknowledged().to_vec());
        PendingActiveChanges {
            unscanned: self.working.diff(&current),
            unapplied: current.diff(&acknowledged),
            order_unapplied: !self.is_same_order_ignoring_merge_mod(
                self.orders.current.as_slice(),
                acknowledged.ids(),
            ),
        }
    }

    /// Whether two id lists are the same sequence once the profile's
    /// generated merge mod is dropped from both.
    fn is_same_order_ignoring_merge_mod(&self, left: &[ModId], right: &[ModId]) -> bool {
        let merge_mod = self.own_merge_mod_id();
        left.iter()
            .filter(|id| **id != merge_mod)
            .eq(right.iter().filter(|id| **id != merge_mod))
    }

    /// Whether `ModsConfig.xml` (as last read: at load, after a write of
    /// Rimmerge's own, or by the periodic baseline refresh) already lists
    /// exactly `source`'s order, in the same sequence. It follows an
    /// outside edit of the file once the refresh has seen it.
    ///
    /// The profile's generated merge mod is ignored on both sides: `Apply`
    /// appends it to the file, but a suggested order only contains it after
    /// the next scan, so comparing it would report "not applied" right
    /// after a successful apply.
    #[must_use]
    pub fn file_matches(&self, source: OrderSource) -> bool {
        self.is_same_order_ignoring_merge_mod(
            self.orders.get(source).as_slice(),
            &self.file_active_mods,
        )
    }

    /// Whether `ModsConfig.xml` (as last read) lists exactly `source`'s ids in
    /// the same sequence, the generated merge mod **included**: writing that
    /// order as it is would change nothing, so an outside edit of the list
    /// would be lost to no one. Stricter than [`Self::file_matches`], which
    /// ignores the merge mod on purpose (a freshly applied file has it and a
    /// suggested order may not yet): an outside edit that only turned the
    /// merge mod on or off is not a match here, because an `Apply` would put
    /// it back.
    #[must_use]
    pub fn file_holds_exactly(&self, source: OrderSource) -> bool {
        self.orders.get(source).as_slice() == self.file_active_mods.as_slice()
    }

    /// [`Self::file_holds_exactly`] for the order the interface has selected:
    /// the fact that says whether applying what the user sees would overwrite
    /// anything of the file's.
    #[must_use]
    pub fn file_holds_selected_exactly(&self) -> bool {
        self.file_holds_exactly(self.selected())
    }

    /// Whether the working set has changes a rescan hasn't picked up yet
    /// — [`crate::use_cases::Apply`]'s own refuse-until-rescan gate.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        !self.pending_changes().unscanned.is_empty()
    }

    /// Whether `ModsConfig.xml` (as last read, see [`Self::file_matches`])
    /// holds the *selected* order, for the Launch RimWorld button.
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

fn change_if(has_changed: bool) -> BaselineChange {
    if has_changed {
        BaselineChange::Changed
    } else {
        BaselineChange::Unchanged
    }
}
