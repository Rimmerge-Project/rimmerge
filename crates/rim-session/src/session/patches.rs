//! [`Session`]'s patch projects: the patch map, per-patch scoped ledgers, and patch-scoped
//! decisions.

use std::collections::BTreeSet;

use rim_resolve::domain::{
    Decision, FindingKey, Ledger, OrderSource, PatchDecisionError, PatchId, PatchProject,
};
use rim_resolve::ledger::{self, ScopedLedgerInput};

use super::Session;
use super::apply_merge_states;
use crate::finding_index::{FindingFilter, FindingIndex, FindingPage};
use crate::merge_workspace::PreviewSlot;

/// No patch project with this id is loaded in the session.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no patch project with id {0}")]
pub struct UnknownPatch(pub PatchId);

/// Everything that can go wrong recording or reverting a decision on a
/// compat patch — the patch twin of [`ResolveError`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchDecideError {
    /// No patch project with the given id is loaded in this session.
    #[error(transparent)]
    Unknown(#[from] UnknownPatch),
    /// The decision failed [`rim_resolve::domain::PatchProject::decide`]'s
    /// own validation (out-of-scope key, an unpatchable action, or a
    /// choice/asset naming an owner outside the patch's scope).
    #[error(transparent)]
    Invalid(#[from] PatchDecisionError),
}

impl Session {
    /// Drops one patch's own caches: its scoped ledger/index (both order
    /// sources) and its merge preview slots. Never touches the profile's
    /// own ledgers, index, or previews, nor another patch's — a patch
    /// decision never re-sorts or re-ledgers the profile.
    pub(super) fn invalidate_patch_caches(&mut self, id: &PatchId) {
        self.scoped.retain(|(patch_id, _), _| patch_id != id);
        self.merges.clear_patch(id);
    }

    /// Every compat patch project loaded with this profile, in id order.
    pub fn patches(&self) -> impl Iterator<Item = &PatchProject> {
        self.patches.values()
    }

    /// One patch project by id, if it's loaded.
    #[must_use]
    pub fn patch(&self, id: &PatchId) -> Option<&PatchProject> {
        self.patches.get(id)
    }

    /// Inserts or replaces a patch project, clearing its own caches.
    /// `pub(crate)`: only the patch use cases should reach for this
    /// directly (via `CreatePatch`/`UpdatePatch`, or a rollback through
    /// [`Session::restore_patch`]) — a decision or revert on an
    /// already-loaded project goes through [`Session::patch_decide`]/
    /// [`Session::patch_revert`] instead, which validate through
    /// [`PatchProject`] itself.
    pub(crate) fn upsert_patch(&mut self, project: PatchProject) {
        let id = project.id().clone();
        self.patches.insert(id.clone(), project);
        self.invalidate_patch_caches(&id);
    }

    /// Removes a patch project, clearing its own caches. Returns the
    /// removed project, if it was loaded.
    pub(crate) fn remove_patch(&mut self, id: &PatchId) -> Option<PatchProject> {
        let removed = self.patches.remove(id);
        self.invalidate_patch_caches(id);
        removed
    }

    /// A snapshot of one patch project, for a caller that wants to roll
    /// back a mutation ([`Session::patch_decide`]/[`Session::patch_revert`]/
    /// [`Session::upsert_patch`]) if persisting it fails via
    /// [`Session::restore_patch`]. `None` when no such patch is loaded.
    #[must_use]
    pub fn patch_snapshot(&self, id: &PatchId) -> Option<PatchProject> {
        self.patches.get(id).cloned()
    }

    /// Restores a [`Session::patch_snapshot`] — used only on the rare
    /// failed-persist rollback path. Behaviorally identical to
    /// [`Session::upsert_patch`] (same clear-its-own-caches shape); kept as
    /// its own named method so a rollback call site reads as a rollback,
    /// not an upsert.
    pub(crate) fn restore_patch(&mut self, project: PatchProject) {
        self.upsert_patch(project);
    }

    // -- Patch maker projects ---

    /// Builds (and caches) `id`'s scoped ledger under `source`:
    /// [`rim_resolve::ledger::scoped`] over a *pristine* copy of the
    /// profile's own ledger (built first if not already cached), with
    /// that patch's own cached merge previews overlaid via
    /// `apply_merge_states` exactly like the profile ledger's own.
    ///
    /// Deliberately reads `self.pristine_ledgers`, never `self.ledgers`:
    /// [`Session::redecide_clean_merge_at`] (the lazy clean-merge
    /// promotion) mutates `self.ledgers` in place the moment something
    /// else — the profile inbox, [`Session::resolution`] — views the same
    /// finding, and nothing invalidates a cached `self.scoped` entry when
    /// that happens. Deriving from `self.ledgers` directly would make a
    /// patch's own suggestion for an undecided finding depend on
    /// incidental call order elsewhere in the session (whether the
    /// profile happened to view it first) rather than only on this
    /// patch's own scope and decisions.
    fn ensure_scoped_ledger(
        &mut self,
        id: &PatchId,
        source: OrderSource,
    ) -> Result<(), UnknownPatch> {
        let scoped_key = (id.clone(), source);
        if self.scoped.contains_key(&scoped_key) {
            return Ok(());
        }
        let project = self
            .patches
            .get(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let scope = project.scope().clone();
        let decisions = project.decisions().clone();
        let threshold = self.rules.settings.threshold;

        self.ensure_ledger(source);
        let slot = Self::slot(source);
        let profile_ledger = match &self.pristine_ledgers[slot] {
            Some(ledger) => ledger,
            None => unreachable!("ensure_ledger just populated this slot"),
        };
        let mut built = ledger::scoped(&ScopedLedgerInput {
            profile: profile_ledger,
            scope: &scope,
            decisions: &decisions,
            threshold,
        });
        let preview_slot = PreviewSlot::patch(source, id.clone());
        apply_merge_states(&mut built, &self.merges, &preview_slot);
        let index = FindingIndex::build(&built);
        self.scoped.insert(scoped_key, (built, index));
        Ok(())
    }

    /// One patch's ledger for `source`, building and caching it on first
    /// request.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_ledger(
        &mut self,
        id: &PatchId,
        source: OrderSource,
    ) -> Result<&Ledger, UnknownPatch> {
        self.ensure_scoped_ledger(id, source)?;
        match self.scoped.get(&(id.clone(), source)) {
            Some((ledger, _)) => Ok(ledger),
            None => unreachable!("just populated above"),
        }
    }

    /// One filtered, paged view over `id`'s own scoped ledger for
    /// `source`.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_findings(
        &mut self,
        id: &PatchId,
        source: OrderSource,
        filter: &FindingFilter,
    ) -> Result<FindingPage, UnknownPatch> {
        self.ensure_scoped_ledger(id, source)?;
        match self.scoped.get(&(id.clone(), source)) {
            Some((ledger, index)) => Ok(index.page(ledger, filter)),
            None => unreachable!("just populated above"),
        }
    }

    /// The full resolution for one finding in `id`'s own scoped ledger for
    /// `source`, if it's currently live (admitted by the patch's scope).
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_resolution(
        &mut self,
        id: &PatchId,
        source: OrderSource,
        key: &FindingKey,
    ) -> Result<Option<&rim_resolve::domain::Resolution>, UnknownPatch> {
        self.ensure_scoped_ledger(id, source)?;
        let (ledger, index) = match self.scoped.get(&(id.clone(), source)) {
            Some(pair) => pair,
            None => unreachable!("just populated above"),
        };
        Ok(index
            .index_of(key)
            .map(|entry_index| &ledger.entries[entry_index]))
    }

    /// Every key currently live in `id`'s own scoped ledger for `source`
    /// — builds (and caches) that ledger the same way
    /// [`Session::patch_ledger`] does. Shared by [`Session::patch_orphaned`]
    /// and [`Session::patch_prune_orphaned`], the two callers that both
    /// need "what's live" to find what's not.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    fn patch_live_keys(
        &mut self,
        id: &PatchId,
        source: OrderSource,
    ) -> Result<BTreeSet<FindingKey>, UnknownPatch> {
        self.ensure_scoped_ledger(id, source)?;
        let (ledger, _) = match self.scoped.get(&(id.clone(), source)) {
            Some(pair) => pair,
            None => unreachable!("just populated above"),
        };
        Ok(ledger
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect())
    }

    /// `id`'s currently-orphaned decision keys: decisions whose key is
    /// either no longer live in its own scoped ledger for `source`, or no
    /// longer admitted by its current scope (see [`PatchProject::orphaned`]).
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_orphaned(
        &mut self,
        id: &PatchId,
        source: OrderSource,
    ) -> Result<Vec<FindingKey>, UnknownPatch> {
        let live = self.patch_live_keys(id, source)?;
        let project = match self.patches.get(id) {
            Some(project) => project,
            None => unreachable!("patch_live_keys above already confirmed it's loaded"),
        };
        Ok(project.orphaned(&live).map(|d| d.key.clone()).collect())
    }

    /// [`PatchProject::decide`] on the stored project, clearing only that
    /// patch's own scoped ledgers and preview slots — a patch decision
    /// never touches the sort or the profile ledger.
    ///
    /// # Errors
    ///
    /// Returns [`PatchDecideError::Unknown`] when no such patch is loaded,
    /// or [`PatchDecideError::Invalid`] when the decision fails
    /// [`PatchProject::decide`]'s own validation.
    pub fn patch_decide(
        &mut self,
        id: &PatchId,
        decision: Decision,
    ) -> Result<(), PatchDecideError> {
        let project = self
            .patches
            .get_mut(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        project.decide(decision)?;
        self.invalidate_patch_caches(id);
        Ok(())
    }

    /// Removes the decision on `key` from `id`'s own decisions, if any.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_revert(
        &mut self,
        id: &PatchId,
        key: &FindingKey,
    ) -> Result<Option<Decision>, UnknownPatch> {
        let project = self
            .patches
            .get_mut(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let removed = project.revert(key);
        self.invalidate_patch_caches(id);
        Ok(removed)
    }

    /// Removes and returns every decision `id`'s own project currently
    /// considers orphaned against its own scoped ledger for `source` (see
    /// [`PatchProject::prune_orphaned`]). Derives "live" via
    /// [`Session::patch_live_keys`], the same source [`Session::patch_orphaned`]
    /// uses, so callers needn't build it themselves.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownPatch`] when no such patch is loaded.
    pub fn patch_prune_orphaned(
        &mut self,
        id: &PatchId,
        source: OrderSource,
    ) -> Result<Vec<Decision>, UnknownPatch> {
        let live = self.patch_live_keys(id, source)?;
        let project = self
            .patches
            .get_mut(id)
            .ok_or_else(|| UnknownPatch(id.clone()))?;
        let pruned = project.prune_orphaned(&live);
        self.invalidate_patch_caches(id);
        Ok(pruned)
    }
}
