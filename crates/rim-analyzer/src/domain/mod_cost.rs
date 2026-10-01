//! [`ModCost`]: the per-mod startup-cost row — everything the scan already
//! knows about what one active mod costs to load, computed once by
//! `analysis::mod_cost::compute` and carried on [`super::Report::mod_costs`].

use serde::{Deserialize, Serialize};

use super::mod_id::ModId;

/// One active mod's contribution to RimWorld's startup cost, as far as a
/// static scan can tell. No timings: Rimmerge never runs the game, and a
/// guessed cost model would be decoration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModCost {
    pub mod_id: ModId,
    /// Mutating patch ops only ([`super::PatchOp::is_mutating`]) —
    /// control-flow nodes (`PatchOperationSequence`/`FindMod`/
    /// `Conditional`/`Test`) never change the target document, so they
    /// cost nothing at patch-apply time.
    pub patch_ops: usize,
    /// Every patch op — mutating **or control-flow** — whose own `xpath`
    /// has a shape RimWorld evaluates slowly (see
    /// [`crate::extract::xpath_expr::is_slow_shape`]). Deliberately a
    /// **different population than [`Self::patch_ops`]**: a
    /// `PatchOperationConditional` node is itself non-mutating (excluded
    /// from `patch_ops`) but carries the gating `<xpath>` RimWorld
    /// actually evaluates, so this column counts it while `patch_ops`
    /// does not — the two columns' denominators are not meant to line up.
    pub slow_xpath_ops: usize,
    /// Raw texture file count under this mod's `Textures/` folder(s),
    /// not deduplicated by normalized key (see
    /// [`super::ScanCost::texture_files`]).
    pub texture_files: u64,
    /// Sum of every one of those files' size in bytes.
    pub texture_bytes: u64,
    /// How many of `texture_files` are `.dds`.
    pub dds_files: u64,
    pub assembly_count: usize,
    pub assembly_bytes: u64,
    pub def_count: usize,
    /// No mutating patch ops and no assemblies: this mod only ships
    /// content (defs, textures, sounds, translations) — it can never
    /// itself run code or mutate another mod's def tree.
    pub content_only: bool,
    /// Bytes of this mod's own textures whose normalized key a
    /// later-loading active mod also ships — loaded by the game, then
    /// immediately thrown away in favor of the later copy.
    pub overridden_texture_bytes: u64,
    /// How many of this mod's own `<Defs>` children are real content this
    /// analyzer cannot index at all — see
    /// [`crate::extract::defs::DefsFile::nameless_def_count`]'s own doc
    /// comment. `#[serde(default)]`-safe: `0` for a report cached before this
    /// field existed correctly reads as "none known", the same value a
    /// genuinely-nameless-def-free mod gets.
    #[serde(default)]
    pub nameless_def_count: usize,
}
