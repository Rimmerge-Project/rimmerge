//! Game-process ports: def-cache carriers and their probe. The parsed game
//! log and its reader are [`super::game_log`]'s.

use std::path::PathBuf;

/// One def-cache plugin this tool can recognize, as loaded
/// from data (the `rules` repo's `data/def-cache-carriers.json`,
/// embedded into `rim-io` at compile time, or a fetched
/// `rimmergeRules.json` overriding it).
///
/// Detection mechanism: such a plugin is **not** a mod with its own
/// packageId — it is a DLL shipped
/// *inside* some performance mod, under
/// `<loaded folder>/<plugin_dir>/**/<file_prefix>*<file_extension>`, a
/// path `rim-analyzer`'s scan never walks (it walks `Assemblies/`, not
/// `Plugins/`). Which mod carries it changes by fork and game version,
/// so detection is the fact itself — one cheap directory probe per
/// active mod's own `Mod::loaded_folders` — rather than a hardcoded
/// packageId. Which *file name* to look for is likewise not a fact about
/// RimWorld, so it is data, not a literal in this workspace's source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefCacheCarrier {
    /// A short, stable identifier for this carrier, for diagnostics.
    pub id: String,
    /// The subfolder of a mod's own loaded folder the DLL is dropped in.
    pub plugin_dir: String,
    /// The DLL's file-name prefix, matched case-insensitively.
    pub file_prefix: String,
    /// The DLL's file extension, matched case-insensitively.
    pub file_extension: String,
    /// Whether to walk `plugin_dir` recursively.
    pub recursive: bool,
    /// The prefix of this carrier's own `Player.log` lines, matched
    /// exactly on the trimmed line.
    pub log_line_prefix: String,
}

/// Probes whether a mod's own loaded folders carry one of the
/// [`DefCacheCarrier`]s it is handed. A port (not a plain function)
/// because the probe is real filesystem IO this crate never performs
/// itself (see the crate's own doc comment); `rim-io`'s
/// `FsDefCacheCarrierProbe` implements it for real, `crate::test_support`
/// fakes it for tests.
///
/// `carriers` is a parameter rather than adapter state on purpose: the
/// list is loaded per project (`ModKnowledge`), while the adapter itself
/// is built once, at the composition root, before any project exists.
pub trait DefCacheCarrierProbe {
    /// `true` when any of `loaded_folders` carries any of `carriers`.
    fn has_def_cache_plugin(
        &self,
        loaded_folders: &[PathBuf],
        carriers: &[DefCacheCarrier],
    ) -> bool;
}
