//! [`Session`]'s finding ledger: lazy per-source ledgers, finding pages and filters, and decisions
//! on findings.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    Action, Decision, DecisionSet, DefRef, FindingKey, Ledger, OrderSource, ResolveError,
};
use rim_resolve::ledger::{self, BuildLedgerInput};
use rim_resolve::preflight::{self, PreflightItem};

use super::Session;
use super::rules::rule_key_from_promoted;
use super::{apply_merge_states, duplicate_template_children, mods_by_id};
use crate::changes::{self, ChangeFilter, ChangePage};
use crate::finding_index::{FindingFilter, FindingIndex, FindingPage};
use crate::merge_workspace::PreviewSlot;

impl Session {
    pub(super) fn invalidate_ledgers(&mut self) {
        self.ledgers = [None, None];
        self.pristine_ledgers = [None, None];
        self.finding_index = [None, None];
        self.merges.clear_all();
        // Every patch's scoped ledger is derived from the profile's own
        // (`rim_resolve::ledger::scoped`), so a profile-wide change goes
        // stale for all of them at once — cheaper to drop the whole map
        // than to track which patches actually admit the changed finding.
        self.scoped.clear();
    }

    /// [`Self::invalidate_ledgers`]'s narrower sibling for a profile
    /// `Merge`/`ShipAsset` decision on `key`
    /// the ledger/index still
    /// need rebuilding (that finding's own resolution status just
    /// changed), but only `key`'s own cached preview goes stale — every
    /// other finding's diff is built from that finding's own stored
    /// choices and the (unchanged) def sources, never from `key`'s, so
    /// wiping the whole `merges` cache the way `invalidate_ledgers` does
    /// would only force needless rebuilds for findings this decision never
    /// touched. `Session::decide` is the only caller, right after storing
    /// such a decision — the caller (`DecideMerge`/`DecidePatchMerge`) is
    /// about to rebuild and re-cache `key`'s own preview anyway.
    fn invalidate_ledgers_for_merge_decision(&mut self, key: &FindingKey) {
        self.ledgers = [None, None];
        self.pristine_ledgers = [None, None];
        self.finding_index = [None, None];
        self.merges.clear_profile_finding(key);
        self.scoped.clear();
    }

    /// Builds and caches `source`'s ledger/index if not already cached,
    /// then returns both. The only place in this file that needs to
    /// assert the just-populated slots are `Some` — every caller gets
    /// already-dereferenced references instead of re-deriving that
    /// invariant itself.
    pub(super) fn ensure_ledger(&mut self, source: OrderSource) -> (&Ledger, &FindingIndex) {
        let slot = Self::slot(source);
        if self.ledgers[slot].is_none() {
            let mods_by_id = mods_by_id(&self.report);
            let mut built = ledger::build(&BuildLedgerInput {
                report: &self.report,
                sort_outcome: &self.sort,
                rules: &self.effective_rules,
                tagging: &self.tagging,
                decisions: &self.decisions,
                threshold: self.rules.settings.threshold,
                source,
                current: &self.orders.current,
                suggested: &self.orders.suggested,
                mods_by_id: &mods_by_id,
                show_dangling_def_references: self.rules.settings.show_dangling_def_references,
            });
            apply_merge_states(&mut built, &self.merges, &PreviewSlot::profile(source));
            self.finding_index[slot] = Some(FindingIndex::build(&built));
            self.pristine_ledgers[slot] = Some(built.clone());
            self.ledgers[slot] = Some(built);
        }
        match (&self.ledgers[slot], &self.finding_index[slot]) {
            (Some(ledger), Some(index)) => (ledger, index),
            _ => unreachable!("both slots were just populated together above"),
        }
    }

    /// The ledger for `source`, building and caching it on first request.
    /// Does not run the clean-merge redecision or the identical-copy
    /// redecision (see [`Session::redecide_clean_merge_at`]'s own doc
    /// comment for why both stay lazy, per finding): a caller that wants
    /// every entry redecided (`RenderMergeMod`) does so explicitly.
    pub fn ledger(&mut self, source: OrderSource) -> &Ledger {
        self.ensure_ledger(source).0
    }

    /// The hard problems in `source`'s order, as [`Apply`](crate::use_cases::Apply)
    /// would write it (see [`rim_resolve::preflight`]). Builds `source`'s
    /// ledger if it is not cached yet, which is why this takes `&mut self`;
    /// the ledger is read only to mark problems the user already decided.
    pub fn hard_problems(&mut self, source: OrderSource) -> Vec<PreflightItem> {
        self.ensure_ledger(source);
        let Some(ledger) = &self.ledgers[Self::slot(source)] else {
            unreachable!("ensure_ledger populated this slot just above");
        };
        preflight::hard_problems(&self.report, self.orders.get(source), ledger)
    }

    /// One filtered, paged view over `source`'s ledger.
    ///
    /// Settles the lazy clean-merge redecision and the lazy
    /// identical-copy redecision (see [`Session::redecide_clean_merge_at`]/
    /// [`Session::redecide_identical_copies_at`]) for every item the page
    /// is about to return *before* returning it: page, redecide each
    /// returned key, and — since a redecision can change an entry's
    /// status/confidence, which the cached [`FindingIndex`]'s own
    /// `sorted`/`needs_input_by_mod` (built once, from the ledger as it
    /// looked before any redecision ran) no longer reflects — rebuild the
    /// index and page again whenever anything changed, until a pass
    /// changes nothing. Without this, a page could be returned, its items
    /// promoted out from under it by a later per-item
    /// [`Session::resolution`] call (exactly what `list_findings_inner`
    /// does to build each row's DTO), and the *next*, otherwise-identical
    /// call would then disagree with the first — the index's `sorted`
    /// order and `needs_input_by_mod` stay stale, while `page`'s own
    /// status filter reads live status, so the two drift apart.
    ///
    /// Terminates because both [`Session::redecide_clean_merge_at`] and
    /// [`Session::redecide_identical_copies_at`] are idempotent per key (a
    /// promoted entry never un-promotes, and a second call on an
    /// unchanged entry returns `false`): each loop iteration either
    /// promotes at least one *previously-untouched* entry into a state a
    /// later pass can no longer change, or changes nothing and returns —
    /// so the loop runs at most once per entry the ledger holds.
    pub fn findings(&mut self, source: OrderSource, filter: &FindingFilter) -> FindingPage {
        let slot = Self::slot(source);
        loop {
            let page = {
                let (ledger, index) = self.ensure_ledger(source);
                index.page(ledger, filter)
            };
            let mut changed = false;
            for key in &page.items {
                if self.redecide_clean_merge_at(source, key) {
                    changed = true;
                }
                if self.redecide_identical_copies_at(source, key) {
                    changed = true;
                }
            }
            if !changed {
                return page;
            }
            if let Some(ledger) = self.ledgers[slot].as_ref() {
                self.finding_index[slot] = Some(FindingIndex::build(ledger));
            }
        }
    }

    /// Live `NeedsInput` finding counts per mod (keyed by
    /// [`ModId::base`]) in `source`'s ledger, building and caching the
    /// ledger on first request like [`Session::ledger`] does.
    pub fn needs_input_by_mod(&mut self, source: OrderSource) -> &BTreeMap<ModId, usize> {
        self.ensure_ledger(source).1.needs_input_by_mod()
    }

    /// What `mod_id` changes: the defs it owns, the templates it
    /// registers, the foreign defs it patches, and the assets it
    /// overrides. Pure and
    /// order-independent — see `crate::changes`'s own doc comment for
    /// why — so, like [`Self::mods`], this never builds or caches
    /// anything and takes `&self`. `mod_id` resolves through
    /// [`ModId::base`], matching every other mod-scoped query in this
    /// crate.
    #[must_use]
    pub fn changes(&self, mod_id: &ModId, filter: &ChangeFilter) -> ChangePage {
        changes::query(mod_id, &self.report, &self.sources, filter)
    }

    /// Searches every def and `Name`-attributed template the scan
    /// indexed (active mods only), ranked name matches before
    /// type-only matches,
    /// capped at [`crate::MAX_PAGE_SIZE`]. Pure, like [`Self::mods`]/
    /// [`Self::changes`].
    #[must_use]
    pub fn search_defs(&self, query: &str, limit: usize) -> Vec<(DefRef, usize)> {
        changes::search(query, limit, &self.sources)
    }

    /// The full [`rim_resolve::domain::Resolution`] for one finding in
    /// `source`'s ledger, if it's currently live. Applies the
    /// clean-merge redecision and the identical-copy redecision (see
    /// [`Session::redecide_clean_merge_at`]/
    /// [`Session::redecide_identical_copies_at`]) to this one finding
    /// first — every finding the inbox displays, paged or single, goes
    /// through here, which keeps both redecisions lazy, per requested
    /// finding.
    pub fn resolution(
        &mut self,
        source: OrderSource,
        key: &FindingKey,
    ) -> Option<&rim_resolve::domain::Resolution> {
        self.ensure_ledger(source);
        self.redecide_clean_merge_at(source, key);
        self.redecide_identical_copies_at(source, key);
        let (ledger, index) = self.ensure_ledger(source);
        let entry_index = index.index_of(key)?;
        Some(&ledger.entries[entry_index])
    }

    /// A snapshot of every decision on file, for a caller that wants to
    /// roll back [`Session::decide`]/[`Session::revert_decision`] if
    /// persisting the change fails (see [`Session::restore_decisions`]).
    #[must_use]
    pub fn decisions_snapshot(&self) -> DecisionSet {
        self.decisions.clone()
    }

    /// Replaces every decision with `decisions` (restoring a
    /// [`Session::decisions_snapshot`]) and always recomputes the sort —
    /// used only on the rare failed-persist rollback path, where
    /// correctness matters far more than skipping a redundant sort.
    pub fn restore_decisions(&mut self, decisions: DecisionSet) {
        self.decisions = decisions;
        self.recompute_sort();
    }

    /// Records `decision`, replacing any earlier one on the same key, and
    /// re-sorts only when doing so actually changed what the sorter
    /// builds — compared by re-deriving [`DecisionSet::sorter_overrides`]
    /// before and after the insert, since a decision can change the
    /// *set* of overrides without the naive "is this action kind one of
    /// the sorter-affecting ones" check noticing (e.g. replacing an
    /// earlier `Reorder` on the same key with `Accept` removes an
    /// override without the new action itself being one). Returns
    /// whether a resort happened.
    ///
    /// Does not persist — callers (the `Decide` use case) do that through
    /// [`crate::ports::DecisionStore`] using [`Session::decisions`] and
    /// [`Session::paths`], rolling back via [`Session::restore_decisions`]
    /// if the save fails. A `PromoteRule` decision's own `RuleSet`
    /// mutation (the `Action::PromoteRule` arm just below) is a second,
    /// separate persistence concern this method itself never touches
    /// either: `Decide::new` alone can record such a decision in memory
    /// but can never save the promotion (rejected instead with
    /// `DecideError::PromoteNeedsRuleStore`, since promoting in memory
    /// with no way to ever persist
    /// it is worse than refusing up front); `Decide::with_rule_store`
    /// snapshots and persists both the decision and the rule set through
    /// [`crate::ports::RuleStore`], rolling back both via
    /// [`Session::rules_snapshot`]/[`Session::restore_rules`] on either
    /// save failing.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] (and changes nothing) when
    /// [`DecisionSet::insert`]'s validation rejects `decision.action` —
    /// currently unreachable, since every `Action` (including `Merge` and
    /// `ShipAsset`) validates; kept as a seam for a future rule.
    pub fn decide(&mut self, decision: Decision) -> Result<bool, ResolveError> {
        // The "Promote" alternative: `Action::PromoteRule`'s real effect is
        // `Session::promote_imported_rule` itself — a `RuleSet` mutation,
        // not a `SorterOverrides` entry (see that action's own doc
        // comment) — so it's special-cased here rather than falling
        // through to the generic sorter-overrides-comparison path below,
        // which would never notice a rule set change.
        if let Action::PromoteRule { rule } = &decision.action {
            let key = rule_key_from_promoted(rule);
            self.decisions.insert(decision)?;
            let promoted = self.promote_imported_rule(&key);
            if !promoted {
                // `promote_imported_rule` only recomputes/invalidates when
                // it actually changes something; a no-op promotion still
                // needs the ledgers invalidated so the just-recorded
                // decision shows up.
                self.invalidate_ledgers();
            }
            return Ok(promoted);
        }
        // Built once, from `report`/`sources` alone (see this function's
        // own doc comment for why), and reused for both the before and
        // after `sorter_overrides` calls below — the decision this method
        // is about to insert must never change which children the template-children
        // input names, or the comparison would compare apples to oranges.
        let template_children =
            duplicate_template_children(&self.report, &self.sources, &mods_by_id(&self.report));
        let before = self.decisions.sorter_overrides(&template_children);
        // `Merge`/`ShipAsset` never move a mod (never a `sorter_overrides`
        // entry), so `resort` below is always `false` for them — captured
        // now, before `decision` moves into `insert`, so the narrower
        // invalidation path can name which finding just changed (a
        // merge decision's
        // own preview is about to be rebuilt and re-cached by its own use
        // case anyway, so wiping every *other* finding's cached preview
        // too — as the generic `invalidate_ledgers` path does — is pure
        // waste, not a correctness requirement).
        let merge_decision_key = matches!(
            decision.action,
            Action::Merge { .. } | Action::ShipAsset { .. }
        )
        .then(|| decision.key.clone());
        self.decisions.insert(decision)?;
        let resort = before != self.decisions.sorter_overrides(&template_children);
        if resort {
            self.recompute_sort();
        } else if let Some(key) = merge_decision_key {
            self.invalidate_ledgers_for_merge_decision(&key);
        } else {
            self.invalidate_ledgers();
        }
        Ok(resort)
    }

    /// Removes the decision on `key`, if any, with the same
    /// overrides-comparison-based resort as [`Session::decide`]. Returns
    /// the removed decision.
    pub fn revert_decision(&mut self, key: &FindingKey) -> Option<Decision> {
        let template_children =
            duplicate_template_children(&self.report, &self.sources, &mods_by_id(&self.report));
        let before = self.decisions.sorter_overrides(&template_children);
        let removed = self.decisions.remove(key)?;
        if before != self.decisions.sorter_overrides(&template_children) {
            self.recompute_sort();
        } else {
            self.invalidate_ledgers();
        }
        Some(removed)
    }
}
