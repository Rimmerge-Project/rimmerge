//! [`ContributesNothing`]: the "this mod contributes nothing" finding producer.
//!
//! Finds every active mod whose every observable contribution is inert
//! under a chosen order: it ships no assembly, every def it owns is
//! overridden by a later-loading copy, every texture, sound, and keyed
//! translation string it ships is overridden, it registers no `Name`-
//! attributed template, it ships no unindexed (defName-less) def
//! content, and every mutating patch op it has either replays to a
//! failure or never runs at all (gated off). One
//! [`rim_resolve::domain::Finding::ContributesNothing`] per such mod.
//!
//! **The seam**: the variant lives in
//! `rim-resolve` (`FindingKey`/`Finding::ContributesNothing`, its
//! suggestion row, its `FindingKind`), exactly where
//! [`rim_resolve::domain::Finding::PatchWillFail`]'s does — but the
//! replay this producer needs lives in `rim-merge`/`rim-session`, not
//! `rim-resolve`, so the producer lives here, beside [`super::VerifyOrder`],
//! sharing that module's gating helpers rather than duplicating them.
//!
//! **Not built on [`super::VerifyOrder::execute`]**: that pass replays
//! only defs with an *active foreign* patcher — the wrong population
//! here, which asks whether *every* mutating op of *one* mod is inert,
//! including a def only that one mod itself patches (never a candidate
//! for `VerifyOrder` at all, since it discards defs with no foreign
//! patcher as posing no cross-mod risk). This runs the cheap,
//! report-only disqualifiers first and only pays for a replay on
//! survivors — see [`ContributesNothing::execute`]'s own doc comment for
//! the exact order.
//!
//! **The guard (mandatory)**: a mod whose
//! [`rim_analyzer::domain::Mod::load_folders_version_matched`] reads
//! `Some(false)` (its `LoadFolders.xml` has no block for the running
//! version) is excluded outright, before any other check.
//!
//! The analyzer resolves such a mod's folders with RimWorld's real
//! descending-version-then-`<default>` fallback, confirmed from the
//! decompiled engine, so a `Some(false)` mod's folders are typically
//! resolved *correctly*. Excluding it outright is therefore
//! **over-conservative rather than load-bearing** — it suppresses findings
//! for mods that are, in the common case, scanned correctly. It stays on
//! purpose: relaxing it needs a real-install measurement of this guard's
//! own candidate set first, rather than loosening it blind.
//!
//! **Why each disqualifier exists** (keep this list current if another
//! surfaces):
//!
//! - **Unscoped patch ops.** [`mod_patch_ops_all_inert`] cannot reach
//!   every op through `SourceIndex::patch_ops_by_def`, which never
//!   indexes an op [`rim_analyzer::extract::xpath_target::parse_all`]
//!   couldn't resolve to any def at all — those land in
//!   `SourceIndex::unscoped_op_counts` instead, so a mod with real, live,
//!   unscoped ops would read as "all inert" having checked nothing. The
//!   pass reconciles explicitly:
//!   `cost.patch_ops` must equal the number of distinct physical ops this
//!   function actually finds indexed under `mod_id`'s own keys, or the
//!   pass refuses to call the mod inert at all (unverifiable, so
//!   conservative) rather than trusting the index to be complete.
//! - **Content channels.** Translations and sounds each get a
//!   winner-under-`order` check (textures use the byte proxy described
//!   below). A mod's own `<Defs>` children with
//!   neither a `Name` nor a `<defName>` (RimWorld itself loads these fine
//!   — vanilla's own `SongDef`s use exactly this shape) are invisible to
//!   every such check, and a `Name`-attributed template has no
//!   override-status signal available to this crate at all, so both get
//!   an unconditional "cannot verify, so not inert" bail. The sound check
//!   (against `Conflict::SoundOverride`) shares the translation check's
//!   disclosed gap — see below.
//! - **Cross-order texture verdict.** The texture check reads
//!   `ModCost.texture_bytes`/`overridden_texture_bytes`, both computed by
//!   the analyzer against the order active *at scan time* — correct for
//!   `OrderSource::Current` but silently wrong for `Suggested`. Recomputing
//!   the texture winner from `Conflict::TextureOverride.owners` instead
//!   (mirroring the translation/sound checks) is **not** an option:
//!   `Conflict::TextureOverride` only ever names *contested* keys, so a mod
//!   shipping nothing but uncontested, uniquely-owned textures would be
//!   invisible to it entirely and read as "texture-inert" by omission. The
//!   byte proxy itself is sound (`texture_bytes >= sum(textures.values())
//!   >= overridden_texture_bytes` always holds, and the disclosed same-mod
//!   double-count cannot manufacture a false positive); the only defect
//!   is the order coupling, so [`ContributesNothing::execute`] refuses
//!   outright for any `source` other than [`OrderSource::Current`]
//!   ([`ContributesNothingError::UnsupportedOrderSource`]) rather than
//!   silently mixing a `Suggested`-order def/sound/translation verdict
//!   with a `Current`-order texture one.
//! - **A discarded replay error.** The zero-owner fast path checks
//!   `ReplayOutcome::error` before reading `top_level_outcomes` — a
//!   malformed or unsupported op truncates that list with no signal,
//!   exactly the failure mode `zero_owner_outcomes`'s own doc comment
//!   exists to prevent (`super::verify_order` guards this the same way).
//!   The error becomes the same "unverifiable, not inert" `Err` the
//!   owned-def branch's `Completeness::Partial` check produces.
//!
//! **Disclosed gap, not fixed here (accepted, same shape for sounds and
//! translations)**: this crate has no per-mod count of sound files or
//! keyed translation strings at all — only
//! [`rim_analyzer::domain::Conflict::SoundOverride`]/
//! [`rim_analyzer::domain::Conflict::KeyedTranslationCollision`], one
//! entry per *colliding* key. A mod whose only sound/translation content
//! never collides with any other active mod is invisible to those two
//! checks and could, in principle, still reach the replay stage and be
//! wrongly flagged. Recorded here rather than guessed around, the same
//! way the `LoadFolders.xml` fallback rule is recorded rather than
//! guessed at.
//!
//! **Also known and out of scope**: `Languages/*/DefInjected/` and
//! `Strings/` are not scanned by this analyzer at all
//! (`infra::mod_scan` only ever walks `Languages/<lang>/Keyed/**`), so a
//! mod whose only real content lives there is invisible to every check
//! in this module, not just the translation one.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::indices::{ActiveMods, DisplayNameIndex};
use rim_analyzer::domain::{Conflict, LoadOrder, ModCost, ModId};
use rim_merge::effective::{self, Completeness, EffectiveInput};
use rim_merge::patch_eval::{PatchContribution, ReplayContext};
use rim_resolve::domain::{Finding, OrderSource};

use super::def_sources::{self, DefSourceLookupError};
use super::verify_order::{
    ReplayEnvironment, active_top_level_operations, build_gate_name_map, has_any_owner,
    has_raw_owner, zero_owner_outcomes,
};
use crate::Session;
use crate::ports::DefSourceReader;

/// [`ContributesNothing::execute`]'s result.
#[derive(Debug, Clone, PartialEq)]
pub struct ContributesNothingReport {
    /// Which order this was checked against.
    pub source: OrderSource,
    /// One [`Finding::ContributesNothing`] per mod found inert.
    pub findings: Vec<Finding>,
}

/// Runs the "contributes nothing" pass.
pub struct ContributesNothing<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> ContributesNothing<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Checks every active mod (except one the guard excludes) and
    /// reports every one whose every observable contribution is inert
    /// under `source`'s own order.
    ///
    /// Order of work, cheapest first:
    /// 1. Ships a DLL (`ModCost::assembly_count > 0`) -> not inert.
    /// 2. Owns a def no later-loading mod overrides -> not inert.
    /// 3. Ships `<Defs>` content this analyzer cannot index at all
    ///    (`ModCost::nameless_def_count > 0`) -> not inert.
    /// 4. Ships an unoverridden texture, sound, or keyed translation key,
    ///    or registers any `Name`-attributed template -> not inert.
    /// 5. Only then: replay its own mutating patch ops; every one must
    ///    reach a replay failure or be gated off, and every one of
    ///    `ModCost::patch_ops` must be individually accounted for.
    ///
    /// # Errors
    ///
    /// Returns [`ContributesNothingError::UnsupportedOrderSource`] for
    /// any `source` other than [`OrderSource::Current`] — see this
    /// module's own doc comment for why: the texture disqualifier is
    /// tied to `ModCost`'s own scan-time order, and this pass refuses to
    /// silently mix that with a different order's def/sound/translation
    /// verdicts rather than risk it.
    pub fn execute(
        &self,
        session: &Session,
        source: OrderSource,
    ) -> Result<ContributesNothingReport, ContributesNothingError> {
        if source != OrderSource::Current {
            return Err(ContributesNothingError::UnsupportedOrderSource);
        }
        let order = session.orders().get(source).clone();
        let report = session.report();

        let guarded: BTreeSet<&ModId> = report
            .mods
            .iter()
            .filter(|m| m.load_folders_version_matched == Some(false))
            .map(|m| &m.id)
            .collect();

        // Check 2's own data, computed once for the whole pass rather than
        // per mod: for every def with at least one owner, whichever owner
        // loads latest under `order` is the one whose copy actually
        // survives — a sole owner's own def counts too (a one-element
        // `max_by_key` is that element), so a mod owning a never-contested
        // def is correctly never excluded here.
        let winning_def_owner: BTreeSet<&ModId> = session
            .sources()
            .owners_by_def
            .values()
            .filter_map(|owners| owners.iter().max_by_key(|id| order.position(id)))
            .collect();

        // Check 4's two "wins at least one colliding key under `order`"
        // checks — identical shape, each reading a different `Conflict`
        // variant. Textures are deliberately *not* checked this way — see
        // this module's own doc comment for why `ModCost.texture_bytes`/
        // `overridden_texture_bytes` is the correct signal instead
        // (below), sound as long as `source` is guaranteed
        // `OrderSource::Current` (checked at the top of this function).
        let winning_sound_owner: BTreeSet<&ModId> = report
            .conflicts
            .iter()
            .filter_map(|conflict| match conflict {
                Conflict::SoundOverride(over) => {
                    over.owners.iter().max_by_key(|id| order.position(id))
                }
                _ => None,
            })
            .collect();
        let winning_translation_owner: BTreeSet<&ModId> = report
            .conflicts
            .iter()
            .filter_map(|conflict| match conflict {
                Conflict::KeyedTranslationCollision(collision) => {
                    collision.owners.iter().max_by_key(|id| order.position(id))
                }
                _ => None,
            })
            .collect();

        // A mod registering any `Name`-attributed template at all is
        // never treated as inert on that account — there is no per-child
        // "winner" concept this crate can cheaply evaluate (each child
        // independently resolves to its own nearest registration),
        // so an unconditional "cannot verify -> not inert" bail is the
        // only sound answer.
        let mods_with_templates: BTreeSet<&ModId> = session
            .sources()
            .templates
            .values()
            .flat_map(|registrants| registrants.iter().map(|(id, _)| id))
            .collect();

        let mod_costs_by_id: BTreeMap<&ModId, &ModCost> = report
            .mod_costs
            .iter()
            .map(|cost| (&cost.mod_id, cost))
            .collect();

        // Check 5's shared replay environment, built once — mirrors
        // `VerifyOrder::execute_with_progress`'s own identical
        // construction. Bundled into `ReplayGating` so
        // `mod_patch_ops_all_inert`'s own parameter count stays within
        // clippy's default limit without an unjustified `#[allow]`.
        // `name_map` must come from `build_gate_name_map` — see
        // `verify_order`'s own doc comment on that function: a bare
        // verbatim-keyed map makes every gated-off op read as inert,
        // which is exactly the wrong direction for this finding (it makes
        // `ContributesNothing` flag a mod whose patches actually run).
        // `DisplayNameIndex`'s own normalize-on-both-sides contract makes
        // rebuilding that mistake by hand a compile error, not just a
        // documented pitfall.
        let active_mods_gate = ActiveMods::from_ids(session.active_base_ids());
        let name_map = build_gate_name_map(&report.mods);
        let active_mods = session.active_base_ids();
        let mod_names_by_display: BTreeMap<String, ModId> = report
            .mods
            .iter()
            .map(|m| (m.name.clone(), m.id.clone()))
            .collect();
        let env = ReplayEnvironment {
            active_mods: &active_mods,
            mod_names_by_display: &mod_names_by_display,
        };
        let gating = ReplayGating {
            order: &order,
            active_mods_gate: &active_mods_gate,
            name_map: &name_map,
            env: &env,
        };

        let mut findings = Vec::new();
        for mod_entry in &report.mods {
            if guarded.contains(&mod_entry.id) {
                continue;
            }
            let Some(cost) = mod_costs_by_id.get(&mod_entry.id) else {
                continue;
            };
            if cost.assembly_count > 0 {
                continue;
            }
            if winning_def_owner.contains(&mod_entry.id) {
                continue;
            }
            if cost.nameless_def_count > 0 {
                continue;
            }
            // The byte proxy, safe only because `source ==
            // OrderSource::Current` is enforced at the top of this
            // function — see this module's own doc comment.
            if cost.texture_bytes > cost.overridden_texture_bytes {
                continue;
            }
            if winning_sound_owner.contains(&mod_entry.id) {
                continue;
            }
            if winning_translation_owner.contains(&mod_entry.id) {
                continue;
            }
            if mods_with_templates.contains(&mod_entry.id) {
                continue;
            }

            let patches_are_inert = if cost.patch_ops == 0 {
                // Vacuously true: no mutating op to fail or be gated off.
                true
            } else {
                mod_patch_ops_all_inert(&self.reader, session, &mod_entry.id, cost, &gating)
                    // A replay that couldn't be verified (a stale source,
                    // an injected-only def, a truncated fold, an
                    // unscoped op) is never guessed at — see this
                    // module's own doc comment: a false positive here
                    // costs the user a mod.
                    .unwrap_or(false)
            };
            if !patches_are_inert {
                continue;
            }

            findings.push(Finding::ContributesNothing {
                mod_id: mod_entry.id.clone(),
            });
        }

        Ok(ContributesNothingReport { source, findings })
    }
}

/// [`ContributesNothing::execute`] refused to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContributesNothingError {
    /// Only [`OrderSource::Current`] is supported — see
    /// [`ContributesNothing::execute`]'s own doc comment for why.
    #[error(
        "ContributesNothing only supports OrderSource::Current: the texture disqualifier is tied to the analyzer's own scan-time order"
    )]
    UnsupportedOrderSource,
}

/// Everything [`mod_patch_ops_all_inert`] needs beyond `session`/`mod_id`/
/// `cost`, invariant across every mod one [`ContributesNothing::execute`]
/// call checks — see that function's own field doc comments for what
/// each one is for (identical to `VerifyOrder::execute_with_progress`'s
/// own local variables of the same names).
struct ReplayGating<'a> {
    order: &'a LoadOrder,
    active_mods_gate: &'a ActiveMods,
    name_map: &'a DisplayNameIndex,
    env: &'a ReplayEnvironment<'a>,
}

/// Whether every one of `mod_id`'s own mutating patch ops either replays
/// to a failure or never runs at all (gated off) under `gating.order`.
///
/// `Err` means this pass could not verify — and so could not rule out —
/// at least one of `mod_id`'s own ops: an unscoped op, an
/// injected-only def with no raw XML to replay, a truncated fold, or a
/// zero-owner replay that stopped early. The caller treats every
/// `Err` the same as `Ok(false)`, never as a confirmed answer either way.
fn mod_patch_ops_all_inert<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    mod_id: &ModId,
    cost: &ModCost,
    gating: &ReplayGating<'_>,
) -> Result<bool, DefSourceLookupError> {
    // Part 1: an unscoped op (its own xpath resolved to no def at
    // all — `SourceIndex::unscoped_op_counts`' own doc comment) can never
    // be replayed, so its mere existence makes this mod unverifiable.
    if session
        .sources()
        .unscoped_op_counts
        .get(mod_id)
        .is_some_and(|&count| count > 0)
    {
        return Err(DefSourceLookupError::MissingSource(format!(
            "{mod_id}: has unscoped mutating patch ops with no def target — not replayable"
        )));
    }

    let Some(keys) = session.sources().ops_by_mod.get(mod_id) else {
        // `cost.patch_ops > 0` (the only time this function is called)
        // and zero unscoped ops, yet no entry here at all — an index
        // inconsistency this pass can't explain, not evidence of
        // anything. Conservative, matching the reconciliation rule below.
        return Err(DefSourceLookupError::MissingSource(format!(
            "{mod_id}: {} mutating patch op(s) reported but none indexed",
            cost.patch_ops
        )));
    };

    // Part 2: every one of `cost.patch_ops` raw mutating ops must be
    // individually accounted for here before this loop is allowed to
    // conclude anything — a physical op indexed under more than one key
    // (a multi-`defName` head) is deduped by its own locator so it's
    // still counted once, matching `cost.patch_ops`'s own flat, raw
    // count. A mismatch means the index doesn't fully explain this mod's
    // own op count, so finishing the loop's "every op is inert" claim
    // would be a guess, not a finding.
    let mut examined_locators: BTreeSet<&rim_analyzer::domain::XmlLocator> = BTreeSet::new();
    for key in keys.keys() {
        if let Some(indexed) = session.sources().patch_ops_by_def.get(key) {
            examined_locators.extend(
                indexed
                    .iter()
                    .filter(|entry| &entry.mod_id == mod_id)
                    .map(|entry| &entry.op.locator),
            );
        }
    }
    if examined_locators.len() != cost.patch_ops {
        return Err(DefSourceLookupError::MissingSource(format!(
            "{mod_id}: {} mutating patch op(s) reported, only {} indexed under its own keys",
            cost.patch_ops,
            examined_locators.len()
        )));
    }

    for (def_type, def_name, selector) in keys.keys() {
        let selector = *selector;
        let Some(indexed) =
            session
                .sources()
                .patch_ops_by_def
                .get(&(def_type.clone(), def_name.clone(), selector))
        else {
            continue;
        };

        let top_level = active_top_level_operations(
            indexed,
            gating.order,
            gating.active_mods_gate,
            gating.name_map,
        );
        if !top_level.iter().any(|(id, _)| id == mod_id) {
            // Every one of this mod's own ops on this def is gated off.
            continue;
        }

        if !has_any_owner(session, def_type, def_name, selector) {
            let outcome = zero_owner_outcomes(
                reader, session, def_type, def_name, selector, &top_level, gating.env,
            )?;
            // A malformed/unsupported op stops this
            // replay early, leaving `top_level_outcomes` truncated with
            // no signal of its own — read `.error` first, exactly the
            // check `zero_owner_outcomes`'s own doc comment (and
            // `super::verify_order`'s identical branch) says a caller
            // must make before trusting the outcomes list at all.
            if let Some(error) = &outcome.error {
                return Err(DefSourceLookupError::MissingSource(format!(
                    "{def_type}/{def_name}: replay stopped — {error}"
                )));
            }
            if outcome
                .top_level_outcomes
                .iter()
                .any(|outcome| &outcome.mod_id == mod_id && outcome.succeeded)
            {
                return Ok(false);
            }
            continue;
        }

        if !has_raw_owner(session, def_type, def_name, selector) {
            return Err(DefSourceLookupError::MissingSource(format!(
                "{def_type}/{def_name}: owned only by a patch injection (no raw Defs/ source) — not replayable"
            )));
        }

        let (winner_id, raw) = def_sources::def_owner_and_raw(
            reader,
            session,
            gating.order,
            def_type,
            def_name,
            selector,
        )?;
        let template_chain = def_sources::template_chain(
            reader,
            session,
            gating.order,
            def_type,
            &winner_id,
            raw.parent_name.as_deref(),
        )?;
        let op_texts = def_sources::load_operation_texts(reader, &top_level)?;
        let contributions: Vec<PatchContribution<'_>> = op_texts
            .iter()
            .map(|(id, text)| PatchContribution {
                mod_id: id,
                operation_xml: text.as_str(),
            })
            .collect();

        let def_index = def_sources::LazyDefExists::new(session);
        let def_exists = |dt: &str, dn: &str| def_index.get(dt, dn);
        let context = ReplayContext {
            active_mods: gating.env.active_mods,
            mod_names_by_display: gating.env.mod_names_by_display,
            def_type,
            def_name,
            selector,
            def_exists: &def_exists,
            this_def_present: true,
            behaviours: session.mod_knowledge().patch_operations(),
        };

        let effective = effective::compute(EffectiveInput {
            winner: &winner_id,
            raw,
            contributions: &contributions,
            context,
            templates: &template_chain.set,
            template_owners: &template_chain.owners,
        });

        // A truncated fold might have stopped before ever reaching one of
        // `mod_id`'s own contributions — this pass can't tell whether the
        // ones it never reached would have succeeded, so it refuses to
        // guess (same conservative rule as the injected-only branch
        // above).
        if let Completeness::Partial { stopped_at } = &effective.completeness {
            return Err(DefSourceLookupError::MissingSource(format!(
                "{def_type}/{def_name}: replay stopped — {stopped_at}"
            )));
        }

        if effective
            .top_level_outcomes
            .iter()
            .any(|outcome| &outcome.mod_id == mod_id && outcome.succeeded)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
    use rim_analyzer::domain::{
        Conflict, DefEntry, FindModGate, KeyedTranslationCollision, PatchOp, Selector,
        SoundOverride, TemplateEntry, XmlLocator,
    };

    use super::*;
    use crate::test_support::{InMemoryDefSourceReader, locator, session_with_sources_and_mods};

    fn cost(id: &str) -> ModCost {
        ModCost {
            mod_id: ModId::new(id),
            patch_ops: 0,
            slow_xpath_ops: 0,
            texture_files: 0,
            texture_bytes: 0,
            dds_files: 0,
            assembly_count: 0,
            assembly_bytes: 0,
            def_count: 0,
            content_only: true,
            overridden_texture_bytes: 0,
            nameless_def_count: 0,
        }
    }

    fn make_op(mod_id: &ModId, class: &str, xpath: &str, op_locator: XmlLocator) -> IndexedPatchOp {
        IndexedPatchOp {
            mod_id: mod_id.clone(),
            op: PatchOp {
                injected_template_names: std::collections::BTreeSet::new(),
                class: class.to_string(),
                xpath: Some(xpath.to_string()),
                target: Some(rim_analyzer::domain::DefTarget {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                    selector: Selector::DefName,
                    sub_path: xpath
                        .split_once(r#"defName="Wall"]/"#)
                        .map(|(_, rest)| rest.to_string()),
                }),
                find_mod_context: Vec::new(),
                find_mod_names: Vec::new(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                is_mutating: true,
                injected_types: BTreeSet::new(),
                injected_paths: BTreeSet::new(),
                conditional_xpath: None,
                is_list_item: false,
                load_folder_gate: Vec::new(),
                sequence_tail: true,
                conditional_branch: None,
                conditional_nomatch_creates: false,
                names_single_def: true,
                value_child_names: std::collections::BTreeSet::new(),
                toggle_active: true,
                value_root_names: Vec::new(),
                value_digest: None,
                locator: op_locator,
            },
        }
    }

    /// Registers `mod_id`'s own op under both `SourceIndex::patch_ops_by_def`
    /// and `SourceIndex::ops_by_mod` — the two maps a real scan always
    /// keeps in sync, which most fixtures below need together.
    fn index_op(sources: &mut SourceIndex, key: (String, String, Selector), op: IndexedPatchOp) {
        let mod_id = op.mod_id.clone();
        sources
            .patch_ops_by_def
            .entry(key.clone())
            .or_default()
            .push(op);
        *sources
            .ops_by_mod
            .entry(mod_id)
            .or_default()
            .entry(key)
            .or_default() += 1;
    }

    /// The one, positive case: a mod that ships nothing observable at
    /// all — no assembly, no defs, no textures, no patch ops — is
    /// flagged.
    #[test]
    fn a_mod_with_no_observable_contribution_at_all_is_flagged() {
        let sources = SourceIndex::default();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("inert.mod")
            .with_mod_costs(vec![cost("inert.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["inert.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert_eq!(
            result.findings,
            vec![Finding::ContributesNothing {
                mod_id: ModId::new("inert.mod")
            }]
        );
    }

    /// Negative 1: a mod shipping a DLL is never flagged, regardless of
    /// anything else.
    #[test]
    fn a_mod_shipping_an_assembly_is_never_flagged() {
        let sources = SourceIndex::default();
        let mut costs = cost("dll.mod");
        costs.assembly_count = 1;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("dll.mod")
            .with_mod_costs(vec![costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["dll.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// Negative 2: a mod that owns a def nobody overrides (a live patch —
    /// or rather, in this case, live def ownership) is never flagged.
    #[test]
    fn a_mod_owning_an_unoverridden_def_is_never_flagged() {
        let mut sources = SourceIndex::default();
        sources.owners_by_def.insert(
            ("ThingDef".to_string(), "Wall".to_string()),
            vec![ModId::new("owner.mod")],
        );
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("owner.mod")
            .with_mod_costs(vec![cost("owner.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["owner.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// Negative 3: a mod with one live (surviving) mutating patch op —
    /// against a real, owned def whose raw XML the op's own target
    /// genuinely exists in (`/comps`, this codebase's own canonical
    /// growable-list fixture target) — is never flagged.
    #[test]
    fn a_mod_with_one_live_patch_op_is_never_flagged() {
        let mut sources = SourceIndex::default();
        let core_locator = locator("core_wall.xml", 0);
        sources.defs.insert(
            (
                ModId::new("core.mod"),
                ("ThingDef".to_string(), "Wall".to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: core_locator.clone(),
            }],
        );
        sources.owners_by_def.insert(
            ("ThingDef".to_string(), "Wall".to_string()),
            vec![ModId::new("core.mod")],
        );

        let p_locator = locator("patch.xml", 0);
        let op = make_op(
            &ModId::new("patch.mod"),
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
            p_locator.clone(),
        );
        index_op(
            &mut sources,
            (
                "ThingDef".to_string(),
                "Wall".to_string(),
                Selector::DefName,
            ),
            op,
        );

        let mut elements = BTreeMap::new();
        elements.insert(
            core_locator,
            "<ThingDef><defName>Wall</defName><comps></comps></ThingDef>".to_string(),
        );
        elements.insert(
            p_locator,
            r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                 <value><li>Comp1</li></value>
               </Operation>"#
                .to_string(),
        );

        let mut mod_costs = cost("patch.mod");
        mod_costs.patch_ops = 1;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .mod_("patch.mod")
            .with_mod_costs(vec![mod_costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["core.mod", "patch.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(elements));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(
            result.findings.is_empty(),
            "a mod whose own Add genuinely applies to an existing node must never be flagged: {:?}",
            result.findings
        );
    }

    /// Mirrors `verify_order`'s own
    /// `an_op_gated_on_an_active_mod_is_still_verified`: a mod whose
    /// *only* op sits under a **satisfied** `PatchOperationFindMod` gate,
    /// and which genuinely succeeds once run, must never be flagged. A
    /// verbatim-keyed gate name map makes `gate_open`'s lowercased lookup
    /// miss "Mod B" entirely, reading the gate as closed — `patch.mod`'s
    /// own op would then be silently excluded from this whole pass rather
    /// than replayed, so this def would never be checked at all and the
    /// loop would find nothing to disprove inertness, flagging a mod whose
    /// patch genuinely runs. This is exactly the wrong direction for this
    /// finding: it costs the user a mod whose content is live. Mixed-case
    /// "Mod B" is load-bearing, same reason as the `verify_order` test.
    #[test]
    fn a_mod_whose_only_op_sits_under_a_satisfied_find_mod_gate_is_never_flagged() {
        let mut sources = SourceIndex::default();
        let core_locator = locator("core_wall.xml", 0);
        sources.defs.insert(
            (
                ModId::new("core.mod"),
                ("ThingDef".to_string(), "Wall".to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: core_locator.clone(),
            }],
        );
        sources.owners_by_def.insert(
            ("ThingDef".to_string(), "Wall".to_string()),
            vec![ModId::new("core.mod")],
        );

        let p_locator = locator("patch.xml", 0);
        let mut op = make_op(
            &ModId::new("patch.mod"),
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
            p_locator.clone(),
        );
        op.op.find_mod_context = vec![FindModGate::AnyActive(vec!["Mod B".to_string()])];
        index_op(
            &mut sources,
            (
                "ThingDef".to_string(),
                "Wall".to_string(),
                Selector::DefName,
            ),
            op,
        );

        let mut elements = BTreeMap::new();
        elements.insert(
            core_locator,
            "<ThingDef><defName>Wall</defName><comps></comps></ThingDef>".to_string(),
        );
        elements.insert(
            p_locator,
            r#"<Operation Class="PatchOperationFindMod">
                 <mods><li>Mod B</li></mods>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                   <value><li>Comp1</li></value>
                 </match>
               </Operation>"#
                .to_string(),
        );

        let mut mod_costs = cost("patch.mod");
        mod_costs.patch_ops = 1;
        // `mod.b`'s own display name is mixed case on purpose — see this
        // test's own doc comment.
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("core.mod")
            .mod_("patch.mod")
            .mod_with("mod.b", |m| m.name = "Mod B".to_string())
            .with_mod_costs(vec![mod_costs])
            .build();
        let session =
            session_with_sources_and_mods(sources, report, &["core.mod", "patch.mod", "mod.b"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(elements));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(
            result.findings.is_empty(),
            "an op gated on an active mod actually runs and succeeds — must never be flagged: {:?}",
            result.findings
        );
    }

    /// The fourth, most important negative: a mod that is inert on every
    /// *observable* column (no assembly, no unoverridden def, no
    /// unoverridden texture/translation, no patch ops) but ships a
    /// `LoadFolders.xml` with no block for the running version must
    /// **never** be flagged — the guard.
    #[test]
    fn a_guarded_mod_that_is_otherwise_inert_is_never_flagged() {
        let sources = SourceIndex::default();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_with("guarded.mod", |m| {
                m.load_folders_version_matched = Some(false);
            })
            .with_mod_costs(vec![cost("guarded.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["guarded.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(
            result.findings.is_empty(),
            "a mod under-scanned by LoadFolders.xml must never be flagged, even though every observable column reads inert: {:?}",
            result.findings
        );
    }

    /// Drives the real `execute()` against an *unguarded* twin of the
    /// exact same otherwise-inert fixture (`load_folders_version_matched:
    /// Some(true)` — the only difference) and asserts it **is** flagged,
    /// proving the guard test above is the thing standing between this
    /// fixture and a finding, not a coincidence: disabling `execute`'s own
    /// guard filter fails the guarded test and leaves this one unaffected.
    #[test]
    fn without_the_guard_the_same_otherwise_inert_mod_is_flagged() {
        let sources = SourceIndex::default();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_with("unguarded.mod", |m| {
                m.load_folders_version_matched = Some(true);
            })
            .with_mod_costs(vec![cost("unguarded.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["unguarded.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert_eq!(
            result.findings,
            vec![Finding::ContributesNothing {
                mod_id: ModId::new("unguarded.mod")
            }]
        );
    }

    /// `SourceIndex::owners_by_def`'s own def-override case: pins that a
    /// *contested* def whose owner has since been overridden is excluded
    /// from the "unoverridden def" disqualifier the same way a sole-owner
    /// def is included in it, both driven off the same `owners_by_def`
    /// data alone (this producer never reads `Report.conflicts::DefOverride`
    /// at all).
    #[test]
    fn a_mod_whose_only_def_is_overridden_by_a_later_mod_is_a_candidate() {
        let mut sources = SourceIndex::default();
        sources.owners_by_def.insert(
            ("ThingDef".to_string(), "Wall".to_string()),
            vec![ModId::new("loser.mod"), ModId::new("winner.mod")],
        );
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("loser.mod")
            .mod_("winner.mod")
            .with_mod_costs(vec![cost("loser.mod"), cost("winner.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["loser.mod", "winner.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(
            result.findings.contains(&Finding::ContributesNothing {
                mod_id: ModId::new("loser.mod")
            }),
            "{:?}",
            result.findings
        );
        assert!(
            !result.findings.contains(&Finding::ContributesNothing {
                mod_id: ModId::new("winner.mod")
            }),
            "the winner still owns the def, so it must never be flagged: {:?}",
            result.findings
        );
    }

    /// A mod whose own translation key is overridden by a later mod is a
    /// candidate; the later mod (the real winner) is not.
    #[test]
    fn a_mod_whose_only_translation_key_is_overridden_is_a_candidate() {
        let sources = SourceIndex::default();
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("loser.mod")
            .mod_("winner.mod")
            .with_mod_costs(vec![cost("loser.mod"), cost("winner.mod")])
            .build();
        report.conflicts.push(Conflict::KeyedTranslationCollision(
            KeyedTranslationCollision {
                key: "Greeting".to_string(),
                owners: vec![ModId::new("loser.mod"), ModId::new("winner.mod")],
            },
        ));
        let session = session_with_sources_and_mods(sources, report, &["loser.mod", "winner.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.contains(&Finding::ContributesNothing {
            mod_id: ModId::new("loser.mod")
        }));
        assert!(!result.findings.contains(&Finding::ContributesNothing {
            mod_id: ModId::new("winner.mod")
        }));
    }

    /// `OrderSource::Suggested` is refused outright, never silently
    /// evaluated against a mismatched texture verdict.
    #[test]
    fn suggested_order_source_is_refused() {
        let sources = SourceIndex::default();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a.mod")
            .with_mod_costs(vec![cost("a.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["a.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass.execute(&session, OrderSource::Suggested);

        assert_eq!(result, Err(ContributesNothingError::UnsupportedOrderSource));
    }

    /// A mod whose only texture key is overridden by a later mod (the
    /// byte proxy — sound only for `OrderSource::Current`, which
    /// every test in this module uses).
    #[test]
    fn a_mod_whose_only_texture_key_is_fully_overridden_is_a_candidate() {
        let sources = SourceIndex::default();
        let mut loser_cost = cost("loser.mod");
        loser_cost.texture_files = 1;
        loser_cost.texture_bytes = 100;
        loser_cost.overridden_texture_bytes = 100;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("loser.mod")
            .with_mod_costs(vec![loser_cost])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["loser.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert_eq!(
            result.findings,
            vec![Finding::ContributesNothing {
                mod_id: ModId::new("loser.mod")
            }]
        );
    }

    /// A mod whose only texture key is *not fully* overridden (some bytes
    /// survive) is never flagged — including a mod shipping only
    /// uncontested, uniquely-owned textures (`overridden_texture_bytes: 0`),
    /// which must never be flagged on the texture channel.
    #[test]
    fn a_mod_with_any_unoverridden_texture_bytes_is_never_flagged() {
        let sources = SourceIndex::default();
        let mut costs = cost("textured.mod");
        costs.texture_files = 2;
        costs.texture_bytes = 200;
        costs.overridden_texture_bytes = 100;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("textured.mod")
            .with_mod_costs(vec![costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["textured.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// A mod that loses a `SoundOverride` conflict is a candidate;
    /// the winner is not.
    #[test]
    fn a_mod_whose_only_sound_key_is_overridden_is_a_candidate() {
        let sources = SourceIndex::default();
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("loser.mod")
            .mod_("winner.mod")
            .with_mod_costs(vec![cost("loser.mod"), cost("winner.mod")])
            .build();
        report
            .conflicts
            .push(Conflict::SoundOverride(SoundOverride {
                path: "shot_fire".to_string(),
                owners: vec![ModId::new("loser.mod"), ModId::new("winner.mod")],
                same_author: false,
            }));
        let session = session_with_sources_and_mods(sources, report, &["loser.mod", "winner.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.contains(&Finding::ContributesNothing {
            mod_id: ModId::new("loser.mod")
        }));
        assert!(!result.findings.contains(&Finding::ContributesNothing {
            mod_id: ModId::new("winner.mod")
        }));
    }

    /// A mod registering a `Name`-attributed template is never
    /// flagged, regardless of anything else — there is no per-child
    /// "winner" concept this producer can cheaply verify.
    #[test]
    fn a_mod_registering_a_template_is_never_flagged() {
        let mut sources = SourceIndex::default();
        sources.templates.insert(
            ("ThingDef".to_string(), "WallBase".to_string()),
            vec![(
                ModId::new("template.mod"),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "WallBase".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: locator("templates.xml", 0),
                },
            )],
        );
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("template.mod")
            .with_mod_costs(vec![cost("template.mod")])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["template.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// A mod whose only content is defName-less `<Defs>`
    /// children (RimWorld's own vanilla `SongDef`s use this shape) is
    /// never flagged, even though it scores zero on every other column.
    #[test]
    fn a_mod_with_nameless_def_content_is_never_flagged() {
        let sources = SourceIndex::default();
        let mut costs = cost("music.mod");
        costs.nameless_def_count = 8;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("music.mod")
            .with_mod_costs(vec![costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["music.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// The core reconciliation check: a mod with one *unscoped* mutating
    /// op (never indexed under any def key at all) is never flagged, even
    /// though `ops_by_mod` has no entry for it either — the exact shape
    /// of the real-install false positives this check prevents.
    #[test]
    fn a_mod_with_only_unscoped_patch_ops_is_never_flagged() {
        let mut sources = SourceIndex::default();
        sources
            .unscoped_op_counts
            .insert(ModId::new("compat.mod"), 3);
        let mut costs = cost("compat.mod");
        costs.patch_ops = 3;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("compat.mod")
            .with_mod_costs(vec![costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["compat.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(
            result.findings.is_empty(),
            "unscoped ops can never be replayed, so this mod must never be flagged: {:?}",
            result.findings
        );
    }

    /// A mod whose `cost.patch_ops` count doesn't match what's
    /// actually indexed under its own keys (a partial/inconsistent
    /// index, distinct from a genuine unscoped op) is never flagged
    /// either — the reconciliation check catches this case too, not just
    /// the `unscoped_op_counts`-populated one.
    #[test]
    fn a_mod_whose_indexed_op_count_does_not_reconcile_is_never_flagged() {
        let mut sources = SourceIndex::default();
        let p_locator = locator("patch.xml", 0);
        let op = make_op(
            &ModId::new("mystery.mod"),
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
            p_locator,
        );
        index_op(
            &mut sources,
            (
                "ThingDef".to_string(),
                "Wall".to_string(),
                Selector::DefName,
            ),
            op,
        );
        // Reports 2 mutating ops; only 1 is actually indexed above.
        let mut costs = cost("mystery.mod");
        costs.patch_ops = 2;
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("mystery.mod")
            .with_mod_costs(vec![costs])
            .build();
        let session = session_with_sources_and_mods(sources, report, &["mystery.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }

    /// A mod with no `Report.mod_costs` row at all (a stale/older report)
    /// is skipped defensively rather than guessed at.
    #[test]
    fn a_mod_missing_a_mod_cost_row_is_never_flagged() {
        let sources = SourceIndex::default();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("uncosted.mod")
            .build();
        let session = session_with_sources_and_mods(sources, report, &["uncosted.mod"]);
        let pass = ContributesNothing::new(InMemoryDefSourceReader::new(BTreeMap::new()));

        let result = pass
            .execute(&session, OrderSource::Current)
            .expect("OrderSource::Current is supported");

        assert!(result.findings.is_empty());
    }
}
