//! DTOs for `get_startup_costs`: one row per active mod, straight off
//! `Report.mod_costs` (the analyzer's own output — this command does
//! no computation of its own beyond the mapping). Sorting is a frontend
//! concern (`StartupPage.vue`'s own sortable table), so this returns the
//! whole list unsorted, in the report's own scan-order.
//!
//! **The two log-derived columns (bad-dimension DDS count,
//! observed timers) are deliberately not part of this DTO at all** — see
//! `dto/game_log.rs`'s own doc comment: they come from a
//! [`super::game_log::GameLogSummaryDto`] the frontend already holds
//! (from `import_game_log`) and are joined client-side by `mod_id`,
//! since `Session` never caches or persists an imported log's summary
//! (the log is chosen by the user each time). Keeping this DTO
//! import-agnostic means it never goes stale relative to whatever the
//! user last imported (or didn't).

use rim_analyzer::domain::ModCost;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One active mod's startup-cost row. Mirrors [`ModCost`] — `u64` fields
/// carry `#[ts(type = "number")]` per this crate's own convention (a
/// byte/file total never remotely approaches `Number.MAX_SAFE_INTEGER`).
///
/// **Deliberately carries no display name**: a resolved-server-side
/// `name` field would go unread by every
/// caller — `StartupPage.vue` renders a mod's name through
/// `useModLabel().label(modId)`, the same shell-wide id/name toggle
/// every other mod-id display in this app already goes through, so a
/// second, independent name source here would be dead weight at best
/// and a second, silently-divergent source of truth at worst.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModCostRowDto {
    /// The mod this row is about.
    pub mod_id: String,
    /// See [`ModCost::patch_ops`].
    pub patch_ops: usize,
    /// See [`ModCost::slow_xpath_ops`].
    pub slow_xpath_ops: usize,
    /// See [`ModCost::texture_files`].
    #[ts(type = "number")]
    pub texture_files: u64,
    /// See [`ModCost::texture_bytes`].
    #[ts(type = "number")]
    pub texture_bytes: u64,
    /// See [`ModCost::dds_files`].
    #[ts(type = "number")]
    pub dds_files: u64,
    /// See [`ModCost::assembly_count`].
    pub assembly_count: usize,
    /// See [`ModCost::assembly_bytes`].
    #[ts(type = "number")]
    pub assembly_bytes: u64,
    /// See [`ModCost::def_count`].
    pub def_count: usize,
    /// See [`ModCost::content_only`].
    pub content_only: bool,
    /// See [`ModCost::overridden_texture_bytes`].
    #[ts(type = "number")]
    pub overridden_texture_bytes: u64,
}

/// Builds one row from `cost`.
#[must_use]
pub fn mod_cost_row(cost: &ModCost) -> ModCostRowDto {
    ModCostRowDto {
        mod_id: cost.mod_id.as_str().to_string(),
        patch_ops: cost.patch_ops,
        slow_xpath_ops: cost.slow_xpath_ops,
        texture_files: cost.texture_files,
        texture_bytes: cost.texture_bytes,
        dds_files: cost.dds_files,
        assembly_count: cost.assembly_count,
        assembly_bytes: cost.assembly_bytes,
        def_count: cost.def_count,
        content_only: cost.content_only,
        overridden_texture_bytes: cost.overridden_texture_bytes,
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;

    fn sample_cost() -> ModCost {
        ModCost {
            mod_id: ModId::new("x.mod"),
            patch_ops: 3,
            slow_xpath_ops: 1,
            texture_files: 10,
            texture_bytes: 2048,
            dds_files: 4,
            assembly_count: 1,
            assembly_bytes: 512,
            def_count: 7,
            content_only: false,
            overridden_texture_bytes: 128,
            nameless_def_count: 0,
        }
    }

    #[test]
    fn mod_cost_row_maps_every_field() {
        let row = mod_cost_row(&sample_cost());

        assert_eq!(row.mod_id, "x.mod");
        assert_eq!(row.patch_ops, 3);
        assert_eq!(row.slow_xpath_ops, 1);
        assert_eq!(row.texture_files, 10);
        assert_eq!(row.texture_bytes, 2048);
        assert_eq!(row.dds_files, 4);
        assert_eq!(row.assembly_count, 1);
        assert_eq!(row.assembly_bytes, 512);
        assert_eq!(row.def_count, 7);
        assert!(!row.content_only);
        assert_eq!(row.overridden_texture_bytes, 128);
    }
}
