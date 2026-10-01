//! Pure extraction functions over file bytes: no filesystem walking, no
//! analysis — parse one file's contents into domain types. Everything
//! here is unit-testable with inline fixtures.

pub mod about_xml;
pub mod asset_index;
pub mod defs;
pub mod expansion_defs;
pub mod file_order;
pub mod graphics;
pub mod languages;
pub mod load_folders;
pub mod manifest_xml;
pub mod mods_config;
pub mod patches;
pub mod pe_metadata;
pub(crate) mod ref_sites;
pub mod rich_text;
pub mod rimmerge_marker;
pub mod side_loaded_assemblies;
pub mod sounds;
pub mod textures;
pub(crate) mod xml_util;

/// The raw-nesting guard every XML reader runs before `roxmltree` parses,
/// shared with `rim-io`'s own `roxmltree` call.
pub use xml_util::{MAX_RAW_ELEMENT_DEPTH, raw_element_nesting_exceeds};
pub mod xpath_expr;
pub mod xpath_target;
