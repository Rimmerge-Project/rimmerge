//! [`Session::mod_info`]: resolves one mod (active, inactive, or missing)
//! into its full [`crate::mod_info::ModInfo`].

use rim_analyzer::domain::{InactiveMod, Mod, ModId};
use rim_resolve::domain::OrderSource;

use super::Session;
use crate::finding_index::{FindingFilter, MAX_PAGE_SIZE};
use crate::mod_info::{
    ActiveModInfo, InactiveModInfo, MissingModInfo, ModInfo, PendingChange, ResolvedMod,
    UnknownMod, classify_homepage, resolve_installed_mod, workshop_url,
};

impl Session {
    /// Resolves `id` through `resolve_installed_mod` (exact id first, then
    /// [`ModId::base`]; active before inactive in each pass), then
    /// `report.missing_mods`, and builds that
    /// mod's full [`ModInfo`]. `&mut self`: an active mod's findings/
    /// needs-input counts read the lazily-built ledger for `source`
    /// ([`Session::findings`]/[`Session::needs_input_by_mod`]), the same
    /// capability `list_order`/`get_mod` already need.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownMod`] when `id` names nothing in the current
    /// report at all.
    pub fn mod_info(&mut self, id: &ModId, source: OrderSource) -> Result<ModInfo, UnknownMod> {
        match resolve_installed_mod(&self.report, id) {
            Some(ResolvedMod::Active(entry)) => {
                let entry = entry.clone();
                return Ok(ModInfo::Active(Box::new(
                    self.active_mod_info(entry, source),
                )));
            }
            Some(ResolvedMod::Inactive(entry)) => {
                return Ok(ModInfo::Inactive(Box::new(self.inactive_mod_info(entry))));
            }
            None => {}
        }
        let target = id.base();
        if self.report.missing_mods.iter().any(|m| m.base() == target) {
            return Ok(ModInfo::Missing(self.missing_mod_info(&target)));
        }
        Err(UnknownMod(id.clone()))
    }

    /// Builds an active mod's info from its already-resolved [`Mod`] row.
    /// Every `&self`-borrowed fact (`report`/`sort`/`orders`/`tagging`)
    /// is cloned out before the `findings`/`needs_input_by_mod` calls
    /// below (both need `&mut self`) — `Session` is single-writer, so
    /// every one of its methods needs the whole borrow to itself.
    fn active_mod_info(&mut self, mod_entry: Mod, source: OrderSource) -> ActiveModInfo {
        let target = mod_entry.id.base();
        let cost = self
            .report
            .mod_costs
            .iter()
            .find(|c| c.mod_id.base() == target)
            .cloned();
        let game_version = self.report.metadata.game_version.clone();
        let order = self.orders.get(source).clone();
        let current = self.orders.current.clone();
        let placement = self.sort.placements.get(&mod_entry.id).cloned();
        let pending = self.pending_changes();
        let tags = self.tagging.tags_of(&mod_entry.id).clone();

        let position = order
            .position(&mod_entry.id)
            // Every active mod is emitted exactly once in both the
            // current and the suggested order; `0` is an unreachable-in-
            // practice fallback, not a claim about a real position.
            .unwrap_or(0);
        let previous_position = (source != OrderSource::Current)
            .then(|| current.position(&mod_entry.id))
            .flatten();
        let tier = placement.map_or(rim_resolve::sort::Tier::Body, |p| p.tier);

        let findings_total = self
            .findings(
                source,
                &FindingFilter {
                    mod_id: Some(mod_entry.id.clone()),
                    offset: 0,
                    limit: MAX_PAGE_SIZE,
                    ..FindingFilter::default()
                },
            )
            .total;
        let needs_input_count = self
            .needs_input_by_mod(source)
            .get(&target)
            .copied()
            .unwrap_or_default();
        let is_deactivation_pending = pending
            .unscanned
            .removed
            .iter()
            .any(|id| id.base() == target);

        ActiveModInfo {
            mod_id: mod_entry.id.clone(),
            name: mod_entry.name,
            authors: mod_entry.authors,
            homepage: classify_homepage(mod_entry.url.as_deref()),
            source: mod_entry.source,
            supports_game_version: mod_entry
                .supported_versions
                .iter()
                .any(|v| v.trim() == game_version.trim()),
            supported_versions: mod_entry.supported_versions,
            declared: mod_entry.declared,
            root: mod_entry.path,
            loaded_folders: mod_entry.loaded_folders,
            cost,
            tags,
            hard_dependents: mod_entry.hard_dependents,
            soft_dependents: mod_entry.soft_dependents,
            awareness_dependents: mod_entry.awareness_dependents,
            is_framework_candidate: mod_entry.is_framework_candidate,
            generated: mod_entry.generated,
            workshop_id: mod_entry.workshop_id,
            workshop_url: mod_entry.workshop_id.map(workshop_url),
            position,
            previous_position,
            tier,
            findings_total,
            needs_input_count,
            pending: is_deactivation_pending.then_some(PendingChange::DeactivationPending),
        }
    }

    fn inactive_mod_info(&self, inactive: &InactiveMod) -> InactiveModInfo {
        let target = inactive.id.base();
        let pending = self.pending_changes();
        let is_activation_pending = pending.unscanned.added.iter().any(|id| id.base() == target);

        InactiveModInfo {
            mod_id: inactive.id.clone(),
            name: inactive.name.clone(),
            authors: inactive.authors.clone(),
            source: inactive.source,
            supported_versions: inactive.supported_versions.clone(),
            declared: inactive.declared.clone(),
            root: inactive.path.clone(),
            workshop_id: inactive.workshop_id,
            workshop_url: inactive.workshop_id.map(workshop_url),
            generated: inactive.generated.clone(),
            pending: is_activation_pending.then_some(PendingChange::ActivationPending),
        }
    }

    fn missing_mod_info(&self, target: &ModId) -> MissingModInfo {
        let required_by = self
            .report
            .missing_dependencies
            .iter()
            .filter(|dep| dep.dependency.id.base() == *target)
            .map(|dep| dep.mod_id.clone())
            .collect();
        MissingModInfo {
            mod_id: target.clone(),
            required_by,
        }
    }
}

#[cfg(test)]
#[path = "mod_info_tests.rs"]
mod tests;
