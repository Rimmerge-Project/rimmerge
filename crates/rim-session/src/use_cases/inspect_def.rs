//! [`InspectDef`]: answers, for one [`DefRef`], everything the def
//! inspector needs — every owner and patcher under the selected order,
//! the template chain, and the effective def. Never plans, never
//! proposes, never writes: this is the
//! read-only twin of [`super::plan_merge::PlanMerge`], sharing its own
//! reader/template/patch-source plumbing (`super::def_sources`) rather
//! than any of its diff/plan machinery.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::IndexedPatchOp;
use rim_analyzer::domain::{ModId, Selector, XmlLocator};
use rim_merge::effective::{self, EffectiveInput};
use rim_merge::patch_eval::{PatchContribution, ReplayContext};
use rim_merge::plan::Caveat;
use rim_resolve::domain::{DefKey, DefRef, FindingKey, ResolutionStatus};

use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources::{self, Asking, representative_op};
use patchers::{caveat_mod_id, patcher_replay_outcome};
use targets::{
    child_ref, map_lookup_error, name_only_children, name_only_owners_in_order, owners_in_order,
    resolve_target, same_def_ref,
};

mod inspection;
mod patchers;
mod targets;

pub use inspection::{
    DefInspection, InspectDefError, PatchOpSummary, Patcher, TemplateAmbiguity, Toucher,
};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "inspect_def/inspect_def_tests.rs"]
mod tests;

/// Every finding currently naming `def_ref`
/// (`FindingKey::def_ref() == Some(def_ref)`), with its *current*
/// [`ResolutionStatus`] — read fresh from `session`'s ledger on every
/// call, never cached: a cached [`DefInspection`]'s every other field is
/// invariant under a non-resorting decision (see `Session.inspections`'
/// own doc comment), but a finding's status is exactly what such a
/// decision changes, so [`InspectDef::execute`] calls this again on every
/// cache hit rather than trusting the entry's own stale `findings`.
fn current_findings(
    session: &mut Session,
    source: rim_resolve::domain::OrderSource,
    def_ref: &DefRef,
) -> Vec<(FindingKey, ResolutionStatus)> {
    let ledger = session.ledger(source);
    ledger
        .entries
        .iter()
        .filter(|entry| {
            entry
                .key
                .def_ref()
                .is_some_and(|found| same_def_ref(&found, def_ref))
        })
        .map(|entry| (entry.key.clone(), entry.status))
        .collect()
}

/// Answers, for one [`DefRef`], everything the def inspector needs
/// Read-only: never plans,
/// never proposes, never writes.
pub struct InspectDef<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> InspectDef<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Answers `def_ref` under [`Session::selected`]'s order, reusing a
    /// cached inspection (`Session.inspections`) when one is already
    /// there — unlike [`super::plan_merge::PlanMerge::execute_in`], this
    /// never recomputes on a cache hit: nothing an inspection reads (the
    /// selected order, which mods are active, the raw XML on disk)
    /// changes underneath a cached entry except an order change or a
    /// rescan, both of which already clear it (see
    /// `Session::cache_inspection`'s own doc comment).
    ///
    /// # Errors
    ///
    /// See [`InspectDefError`].
    pub fn execute<'a>(
        &self,
        session: &'a mut Session,
        def_ref: &DefRef,
    ) -> Result<&'a DefInspection, InspectDefError> {
        let source = session.selected();
        if session.inspection(source, def_ref).is_some() {
            // A cache hit reuses every field of the cached `DefInspection`
            // except `findings`: nothing else it holds can go stale
            // without `orders` itself changing (which already clears this
            // cache — see `Session.inspections`' own doc comment), but a
            // `ResolutionStatus` can change from a decision that leaves
            // `orders` untouched (e.g. `Action::Accept` on this very
            // def's finding). Refreshed from the live ledger on every hit
            // rather than cached, so the cache-hit benefit stays for
            // everything that's genuinely invariant.
            let findings = current_findings(session, source, def_ref);
            session.refresh_inspection_findings(source, def_ref, findings);
            return Ok(session
                .inspection(source, def_ref)
                .unwrap_or_else(|| unreachable!("just checked Some above")));
        }
        let inspection = self.build(session, source, def_ref)?;
        session.cache_inspection(source, def_ref.clone(), inspection);
        Ok(session
            .inspection(source, def_ref)
            .unwrap_or_else(|| unreachable!("just cached above")))
    }

    fn build(
        &self,
        session: &mut Session,
        source: rim_resolve::domain::OrderSource,
        def_ref: &DefRef,
    ) -> Result<DefInspection, InspectDefError> {
        let order = session.orders().get(source).clone();
        let Some((def_type, def_name, selector)) =
            resolve_target(session.sources(), &order, def_ref)
        else {
            return Err(InspectDefError::NotFound(def_ref.clone()));
        };

        let (winner_id, raw) = def_sources::def_owner_and_raw(
            &self.reader,
            session,
            &order,
            &def_type,
            &def_name,
            selector,
        )
        .map_err(|error| map_lookup_error(def_ref, error))?;

        let template_chain = def_sources::template_chain(
            &self.reader,
            session,
            &order,
            &def_type,
            &winner_id,
            raw.parent_name.as_deref(),
        )
        .map_err(|error| map_lookup_error(def_ref, error))?;
        let parents: Vec<(DefKey, ModId)> = template_chain
            .chain
            .iter()
            .map(|key| {
                let owner = template_chain
                    .owners
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| unreachable!("every chain entry has a recorded owner"));
                (
                    DefKey {
                        def_type: key.0.clone(),
                        def_name: key.1.clone(),
                    },
                    owner,
                )
            })
            .collect();

        let owners_raw: Vec<ModId> = if def_ref.is_name_only() {
            name_only_owners_in_order(session.sources(), &order, &def_name)
        } else {
            owners_in_order(session.sources(), &order, &def_type, &def_name, selector)
        };
        // Owned, bounded by the handful of generated mods a session ever
        // has (never the whole `Report`, which can hold ~1,000 mods on a
        // real install) — cheaper than cloning `Report` just to answer
        // "is this owner generated" a few times below.
        let generated_ids: BTreeSet<ModId> = session
            .report()
            .mods
            .iter()
            .filter(|m| m.generated.is_some())
            .map(|m| m.id.clone())
            .collect();
        let owners: Vec<Toucher> = owners_raw
            .iter()
            .map(|mod_id| Toucher {
                mod_id: mod_id.clone(),
                position: order
                    .position(mod_id)
                    .unwrap_or_else(|| unreachable!("owners_raw is filtered from order itself")),
                is_generated: generated_ids.contains(mod_id),
            })
            .collect();

        let children: Vec<(ModId, DefRef)> = match selector {
            Selector::NameAttr if def_ref.is_name_only() => {
                name_only_children(session.sources(), &def_name)
            }
            Selector::NameAttr => session
                .sources()
                .children_by_template
                .get(&(def_type.clone(), def_name.clone()))
                .map(|children| {
                    children
                        .iter()
                        .map(|(owner, child_key)| {
                            (owner.clone(), child_ref(session.sources(), child_key))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Selector::DefName => Vec::new(),
        };

        // A
        // duplicated template `Name` has no single winner — disclose the
        // real per-child answers rather than letting `winner` above stand
        // unexplained as if it were one. Only ever `Some` for a template
        // (`NameAttr`) with 2+ registrants; a concrete def or an
        // unambiguous template needs no disclosure at all.
        let template_ambiguity = (matches!(selector, Selector::NameAttr) && owners_raw.len() > 1)
            .then(|| {
                let child_mods: BTreeSet<ModId> =
                    children.iter().map(|(owner, _)| owner.clone()).collect();
                let resolutions = child_mods
                    .into_iter()
                    .filter_map(|child| {
                        let asking = Asking::Mod {
                            id: &child,
                            is_vanilla: def_sources::is_vanilla_owner(session, &child),
                        };
                        def_sources::nearest_owner(&owners_raw, &order, session, &asking)
                            .map(|resolved| (child, resolved))
                    })
                    .collect();
                TemplateAmbiguity {
                    registrants: owners_raw.clone(),
                    resolutions,
                }
            });

        let indexed: Vec<IndexedPatchOp> = session
            .sources()
            .patch_ops_by_def
            .get(&(def_type.clone(), def_name.clone(), selector))
            .cloned()
            .unwrap_or_default();
        let top_level = def_sources::top_level_operations(&indexed, &order);
        let op_texts = def_sources::load_operation_texts(&self.reader, &top_level)?;

        let active_mods = session.active_base_ids();
        let mod_names_by_display: BTreeMap<String, ModId> = session
            .report()
            .mods
            .iter()
            .map(|m| (m.name.clone(), m.id.clone()))
            .collect();
        // Lazily built (and shared with `PlanMerge`'s identical need) —
        // see `def_sources::LazyDefExists`'s own doc comment: most
        // inspections never ask, and folding ~200k keys into a set costs
        // real time when they don't.
        let def_index = def_sources::LazyDefExists::new(session);
        let def_exists = |dt: &str, dn: &str| def_index.get(dt, dn);
        let context = ReplayContext {
            active_mods: &active_mods,
            mod_names_by_display: &mod_names_by_display,
            def_type: &def_type,
            def_name: &def_name,
            selector,
            def_exists: &def_exists,
            this_def_present: true,
            behaviours: session.mod_knowledge().patch_operations(),
        };

        // Group top-level ops by mod, preserving first-seen (load) order —
        // `top_level` is already grouped this way by construction (see
        // `def_sources::top_level_operations`'s own doc comment) — and,
        // in the same pass, each mod's own positions in `op_texts`/
        // `contributions` below (both preserve `top_level`'s order
        // exactly, one entry each), needed to place a patcher's own ops
        // relative to the fold's stopper (`patcher_replay_outcome`).
        let mut patcher_order: Vec<ModId> = Vec::new();
        let mut locators_by_mod: BTreeMap<ModId, Vec<XmlLocator>> = BTreeMap::new();
        let mut global_index_by_mod: BTreeMap<ModId, Vec<usize>> = BTreeMap::new();
        for (index, (mod_id, locator)) in top_level.iter().enumerate() {
            if !locators_by_mod.contains_key(mod_id) {
                patcher_order.push(mod_id.clone());
            }
            locators_by_mod
                .entry(mod_id.clone())
                .or_default()
                .push(locator.clone());
            global_index_by_mod
                .entry(mod_id.clone())
                .or_default()
                .push(index);
        }

        // Each patcher's own op summaries — built before the fold below,
        // since they need only `indexed`/`locators_by_mod`, not a replay
        // outcome.
        let patcher_ops: Vec<(ModId, Vec<PatchOpSummary>)> = patcher_order
            .iter()
            .map(|mod_id| {
                let locators = &locators_by_mod[mod_id];
                let ops: Vec<PatchOpSummary> = locators
                    .iter()
                    .map(|locator| {
                        let op = representative_op(&indexed, mod_id, locator).unwrap_or_else(|| {
                            unreachable!("top_level_operations derives every locator from `indexed` itself"
                            )
                        });
                        PatchOpSummary::from_op(op, locator)
                    })
                    .collect();
                (mod_id.clone(), ops)
            })
            .collect();

        let contributions: Vec<PatchContribution<'_>> = op_texts
            .iter()
            .map(|(id, text)| PatchContribution {
                mod_id: id,
                operation_xml: text.as_str(),
            })
            .collect();
        let effective = effective::compute(EffectiveInput {
            winner: &winner_id,
            raw,
            contributions: &contributions,
            context,
            templates: &template_chain.set,
            template_owners: &template_chain.owners,
        });
        let resolved_xml = rim_merge::xml::render(&effective.resolved);

        // `Patcher::replay`/`reached`/`caveats` are all *derived* from
        // this same fold's own outcome, never a second, standalone
        // `patch_eval::replay` call per mod — see `Patcher::replay`'s own
        // doc comment for why re-replaying in isolation could disagree
        // with what the fold actually saw.
        let patchers: Vec<Patcher> = patcher_ops
            .into_iter()
            .map(|(mod_id, ops)| {
                let own_indices = &global_index_by_mod[&mod_id];
                let (replay, reached) =
                    patcher_replay_outcome(&mod_id, own_indices, &effective.completeness);
                let caveats: Vec<Caveat> = effective
                    .caveats
                    .iter()
                    .filter(|caveat| caveat_mod_id(caveat).as_ref() == Some(&mod_id))
                    .cloned()
                    .collect();
                Patcher {
                    position: order
                        .position(&mod_id)
                        .unwrap_or_else(|| unreachable!("every patcher is an active mod")),
                    is_generated: generated_ids.contains(&mod_id),
                    mod_id,
                    ops,
                    replay,
                    reached,
                    caveats,
                }
            })
            .collect();

        let findings = current_findings(session, source, def_ref);

        Ok(DefInspection {
            def_ref: def_ref.clone(),
            source,
            owners,
            winner: winner_id,
            template_ambiguity,
            patchers,
            parents,
            children,
            effective,
            resolved_xml,
            findings,
        })
    }
}
