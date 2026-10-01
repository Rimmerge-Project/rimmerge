//! The named conflict scenarios: each builds a `Session` around one specific def/patch situation
//! the merge editor and ledger must handle.

mod agreeing_items;
mod biome_lists;
mod containers;
mod document_order;
mod overrides;
mod patch_collisions;
mod templates;

pub use agreeing_items::*;
pub use biome_lists::*;
pub use containers::*;
pub use document_order::*;
pub use overrides::*;
pub use patch_collisions::*;
pub use templates::*;

use std::path::Path;
use std::sync::Arc;

use rim_analyzer::domain::XmlLocator;

/// Builds an [`XmlLocator`] good enough for a fixture — a stand-in file
/// path plus a single ordinal. `pub(crate)` so other test modules
/// building their own small `SourceIndex` fixtures (e.g.
/// `use_cases::plan_merge`'s staleness test) can reuse it.
pub(crate) fn locator(file: &str, ordinal: u32) -> XmlLocator {
    XmlLocator::new(Arc::from(Path::new(file)), vec![ordinal])
}
