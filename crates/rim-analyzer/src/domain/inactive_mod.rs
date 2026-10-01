//! [`InactiveMod`]: a mod [`crate::infra`] discovery found on disk whose id
//! is not (currently) in `ModsConfig.xml`'s active list.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::declared_order::DeclaredOrder;
use super::mod_entry::GeneratedMarker;
use super::mod_id::ModId;
use super::source::Source;

/// A mod discovery found on disk, but that isn't active right now. Carries
/// identity and declared load-order hints only — unlike an active
/// [`super::Mod`], an inactive mod is never scanned for defs, patches,
/// assemblies, or textures: activating it is what would make that content
/// relevant at all, and scanning every inactive mod on every run just to
/// populate a list nobody asked to grow would slow every scan for a fact only
/// the mods page and the activate/deactivate flow need.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InactiveMod {
    /// The id `ModsConfig.xml`'s `<activeMods>` would need to name this
    /// exact copy: bare for a `primary` (`Data`/`Mods`) entry, `_steam`-
    /// suffixed for a Workshop entry whose base id also has a `primary`
    /// copy on disk. Mirrors [`super::super::infra`]'s
    /// `Discovered::lookup` resolution rule in reverse — see
    /// `Discovered::inactive`'s own doc comment for the two sides of that
    /// mirror.
    pub id: ModId,
    pub name: String,
    pub authors: Vec<String>,
    pub path: PathBuf,
    pub source: Source,
    pub supported_versions: Vec<String>,
    /// The Steam Workshop published-file id, when `source` is
    /// [`Source::Workshop`]. See [`super::Mod::workshop_id`].
    pub workshop_id: Option<u64>,
    /// `Some` when the folder carries a `rimmerge.json` marker. See
    /// [`super::Mod::generated`].
    pub generated: Option<GeneratedMarker>,
    /// Declared load-order and dependency hints from this mod's own
    /// `About.xml`. Kept — unlike every other scanned fact — because the
    /// "activate its inactive dependencies too" closure needs it, and
    /// `About.xml` is already parsed by discovery for every mod regardless of
    /// whether it ends up active.
    pub declared: DeclaredOrder,
}
