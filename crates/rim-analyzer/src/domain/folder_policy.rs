//! [`FolderPolicy`]: how a mod's loaded folders are selected.

/// Selects between RimWorld's real `LoadFolders.xml`/default-folder rule
/// and the `--all-folders` diagnostic override that scans everything a mod
/// ships regardless of what would actually load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderPolicy {
    /// Resolve folders via `LoadFolders.xml` (or the default rule) — what
    /// RimWorld itself would load.
    LoadFolders,
    /// Diagnostic override: scan every folder a mod ships, ignoring
    /// `LoadFolders.xml` entirely.
    Everything,
}
