//! DTO for `get_def_cache_carrier`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether an active mod carries a known def-cache plugin — see
/// `rim_session::use_cases::FindDefCacheCarrier`'s own doc comment for
/// the detection rule. `carrier_mod_id` is `None` when no active
/// mod is a carrier; `ApplyDialog.vue` shows the cache-rebuild note only
/// when it's `Some`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefCacheCarrierDto {
    /// The active mod carrying a def-cache plugin, if any.
    pub carrier_mod_id: Option<String>,
}
