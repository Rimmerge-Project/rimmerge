//! Predicting each patch's outcome under an order: gate name maps, active top-level operations, and
//! owner checks.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::indices::{ActiveMods, DisplayNameIndex, patch_op_active};
use rim_analyzer::domain::{LoadOrder, Mod, ModId, Selector, XmlLocator};
use rim_merge::patch_eval::{PatchContribution, ReplayContext};

use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::def_sources;

/// Builds a [`DisplayNameIndex`] for [`patch_op_active`]'s own gate
/// lookup — every enclosing `PatchOperationFindMod`'s `<match>`/
/// `<nomatch>` names a display name, and
/// `rim_analyzer::analysis::indices::gate_open` looks it up lowercased.
/// It must not be a bare, verbatim-keyed map: every gate lookup would
/// silently miss, closing every `AnyActive` gate and opening every
/// `NoneActive` one (see [`VerifyOrder::execute_with_progress`]'s own
/// local comment). [`DisplayNameIndex`] (`rim-analyzer`'s own newtype
/// hardening for this exact class of bug) makes that mistake a compile
/// error — every insert and lookup normalizes on its own, so this
/// function's own tie-break below ("first mod to claim a name wins") is
/// all that's left for it to get right.
///
/// Same "lowercased name -> id, first mod to claim it wins" resolution as
/// `rim_analyzer::analysis::edges::build_name_map`, over
/// [`rim_analyzer::domain::Mod`] rather than `ScannedMod` — this crate only
/// ever has the already-built `Report::mods`, never a scan's own
/// `ScannedMod`s (the same reasoning
/// `super::import_game_log` shares this function for, rather than keeping
/// its own copy). **Keep in sync with
/// `rim_analyzer::analysis::edges::build_name_map`** — a change to that
/// function's own resolution rule (the tie-break, or what counts as
/// "empty") should be mirrored here too.
pub(crate) fn build_gate_name_map(mods: &[Mod]) -> DisplayNameIndex {
    let mut map = DisplayNameIndex::new();
    for m in mods {
        if m.name.is_empty() {
            continue;
        }
        if map.get(&m.name).is_none() {
            map.insert(&m.name, m.id.clone());
        }
    }
    map
}

/// Whether `mod_id` is one of `(def_type, def_name)`'s own owners
/// *and* that def has at least one other owner — the exact condition
/// under which reordering `mod_id` could change which owner the game's
/// "last owner wins" rule picks, and therefore change the raw tree the
/// counterfactual experiment holds fixed (the counterfactual's scope
/// limit).
///
/// A sole owner is deliberately **not** excluded from the experiment:
/// with nothing to lose the winner race against, moving it relative to
/// the def's *patchers* cannot change the winner, so the experiment
/// stays sound. Reads the same two maps [`has_any_owner`]/[`has_raw_owner`]
/// already consult, per selector, rather than a third source of truth.
pub(super) fn is_co_owner_with_another(
    session: &Session,
    def_type: &str,
    def_name: &str,
    selector: Selector,
    mod_id: &ModId,
) -> bool {
    let key = (def_type.to_string(), def_name.to_string());
    // `templates`' entries carry the registered template alongside the
    // registrant, so the two maps are counted the same way but read
    // differently — never flattened into one `Vec<ModId>` here, which
    // would allocate per job for a membership test.
    match selector {
        Selector::DefName => session
            .sources()
            .owners_by_def
            .get(&key)
            .is_some_and(|owners| owners.len() > 1 && owners.contains(mod_id)),
        Selector::NameAttr => session
            .sources()
            .templates
            .get(&key)
            .is_some_and(|registrants| {
                registrants.len() > 1
                    && registrants
                        .iter()
                        .any(|(registrant, _)| registrant == mod_id)
            }),
    }
}

/// [`def_sources::top_level_operations`], filtered to operations that
/// would actually run under `order` — [`patch_op_active`] applied to
/// each one's own representative op (the same shallowest-descendant
/// recovery [`representative_op`] uses for *display* purposes, which
/// this function deliberately does **not** reuse for gating — see below),
/// mirroring exactly what the analyzer's own edge producers already do
/// (`crates/rim-analyzer/src/analysis/edges.rs`'s
/// `patch_removed_node_edges`/`patch_injected_node_edges`, which gate
/// *per op*, never per top-level node). This is the **one** place this
/// whole use case reads a def's own patchers — every caller below (the
/// zero-owner fast path, what gets replayed,
/// `classify_cause`'s own `other_patchers`) sees only this gated result,
/// never the raw list.
///
/// **Gating asks whether *anything* under a top-level node runs**:
/// [`representative_op`]'s own shallowest-child summary is sound for
/// *display* (one row needs one class/xpath to show) but wrong for
/// *gating* — a `Sequence` whose first child carries an unsatisfied
/// `MayRequire` while a later sibling has none would be removed wholesale
/// and never verified at all, even though that later sibling genuinely
/// runs, a shape that is common on real installs. So this checks whether
/// *any* indexed op sharing this top-level node's own
/// `(mod, file, first ordinal)` identity is active — the same shape
/// `representative_op`'s own filter already narrows to, just without the
/// `min_by_key` that picks only one.
///
/// `pub(super)`: [`super::contributes_nothing::ContributesNothing`] is the
/// second caller, sharing this module's gating helpers rather than
/// duplicating them.
pub(crate) fn active_top_level_operations(
    indexed: &[rim_analyzer::analysis::IndexedPatchOp],
    order: &LoadOrder,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> Vec<(ModId, XmlLocator)> {
    def_sources::top_level_operations(indexed, order)
        .into_iter()
        .filter(|(mod_id, locator)| {
            indexed.iter().any(|entry| {
                &entry.mod_id == mod_id
                    && entry.op.locator.file == locator.file
                    && entry.op.locator.element_path.first() == locator.element_path.first()
                    && patch_op_active(&entry.op, active, name_map)
            })
        })
        .collect()
}

/// The two `ReplayContext` fields that never vary per def within one
/// [`VerifyOrder::execute`] call — bundled purely to keep
/// [`zero_owner_outcomes`]'s own parameter count within clippy's limit
/// (`too_many_arguments`), not a type this module needs for any other
/// reason.
pub(crate) struct ReplayEnvironment<'a> {
    pub(crate) active_mods: &'a BTreeSet<ModId>,
    pub(crate) mod_names_by_display: &'a BTreeMap<String, ModId>,
}

/// The zero-owner fast path's own outcomes:
/// replays every gated-active top-level op against a synthetic, empty
/// `<{def_type}></{def_type}>` placeholder — see the zero-owner branch's
/// own comment in [`VerifyOrder::execute`] for why this is sound (the
/// outcome is certain regardless of content, so a real replay's own
/// success-suppression/identity logic applies exactly as it does to a
/// genuinely-owned def) despite there being no real winner's XML to
/// build a tree from.
///
/// Returns the *whole* [`rim_merge::patch_eval::ReplayOutcome`], not just
/// its `top_level_outcomes` — discarding `.error` here would let a
/// malformed or unsupported operation on a zero-owner def silently
/// truncate the outcomes list with no signal at all to the caller;
/// `execute_with_progress`'s own zero-owner branch reads `.error` and
/// records a `skipped` entry naming it, the same truncation visibility
/// the replayed path gets from checking `EffectiveDef::completeness`.
pub(crate) fn zero_owner_outcomes<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    def_type: &str,
    def_name: &str,
    selector: Selector,
    top_level: &[(ModId, XmlLocator)],
    env: &ReplayEnvironment<'_>,
) -> Result<rim_merge::patch_eval::ReplayOutcome, def_sources::DefSourceLookupError> {
    let op_texts = def_sources::load_operation_texts(reader, top_level)?;
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
        active_mods: env.active_mods,
        mod_names_by_display: env.mod_names_by_display,
        def_type,
        def_name,
        selector,
        def_exists: &def_exists,
        // The whole point of this fast path: the def genuinely does not
        // exist anywhere, so a bare def-head existence test must answer
        // `false`, not "the placeholder's own root node exists" (see
        // `ReplayContext::this_def_present`'s own doc comment).
        this_def_present: false,
        behaviours: session.mod_knowledge().patch_operations(),
    };
    let placeholder = def_sources::parse_tree(&format!("<{def_type}></{def_type}>"))?;
    Ok(rim_merge::patch_eval::replay(
        placeholder,
        &contributions,
        &context,
    ))
}

/// Whether `(def_type, def_name)` under `selector` has any active owner
/// at all, without needing a replay to find out — including a def that
/// exists only via a whole-def `<xpath>Defs</xpath>` patch injection
/// (`session.sources().injected_def_owners`, which is common on real
/// installs; missing it would wrongly fast-path such a def as
/// `DeadTarget`). **Only
/// [`has_raw_owner`] answers "can this def actually be replayed"** — an
/// injected-only def is real (this function correctly says so) but has
/// no literal `Defs/**/*.xml` source to read, so [`VerifyOrder::execute`]
/// must check both, never this one alone, before attempting a replay.
/// `injected_def_owners` is keyed the same `(def_type, def_name)` shape
/// [`super::edges::injected_def_owners`] always produces — concrete
/// `defName` injections only, never a `Selector::NameAttr` target, so it
/// is consulted only from the `DefName` arm.
pub(crate) fn has_any_owner(
    session: &Session,
    def_type: &str,
    def_name: &str,
    selector: Selector,
) -> bool {
    match selector {
        Selector::DefName => {
            let key = (def_type.to_string(), def_name.to_string());
            session
                .sources()
                .owners_by_def
                .get(&key)
                .is_some_and(|owners| !owners.is_empty())
                || session
                    .sources()
                    .injected_def_owners
                    .get(&key)
                    .is_some_and(|owners| !owners.is_empty())
        }
        Selector::NameAttr => session
            .sources()
            .templates
            .get(&(def_type.to_string(), def_name.to_string()))
            .is_some_and(|registrants| !registrants.is_empty()),
    }
}

/// Whether `(def_type, def_name)` under `selector` has a *raw* owner —
/// one [`def_sources::def_owner_and_raw`] can actually read XML back
/// for. Identical to [`has_any_owner`] except it never consults
/// `injected_def_owners`: a whole-def patch injection has no literal
/// source file `session.sources().defs` ever indexed (synthesized
/// dynamically by RimWorld's own patch engine, not written to disk as
/// its own `Defs/**/*.xml` entry), so a def that is real only via
/// injection must still be routed away from a replay attempt that could
/// only ever fail to find a source — see the injected-only branch in
/// [`VerifyOrder::execute`].
pub(crate) fn has_raw_owner(
    session: &Session,
    def_type: &str,
    def_name: &str,
    selector: Selector,
) -> bool {
    match selector {
        Selector::DefName => session
            .sources()
            .owners_by_def
            .get(&(def_type.to_string(), def_name.to_string()))
            .is_some_and(|owners| !owners.is_empty()),
        Selector::NameAttr => session
            .sources()
            .templates
            .get(&(def_type.to_string(), def_name.to_string()))
            .is_some_and(|registrants| !registrants.is_empty()),
    }
}
