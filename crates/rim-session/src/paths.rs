//! [`ProjectPaths`]: every filesystem location one Rimmerge session needs.

use std::path::PathBuf;

/// The filesystem locations a session works with: the game install, the
/// workshop content folder, the active `ModsConfig.xml`, and this
/// project's own profile directory (decisions, rules, and imported-rule
/// snapshots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPaths {
    /// The RimWorld install directory (contains `Data/` and `Mods/`).
    pub game_dir: PathBuf,
    /// The Steam workshop content folder for RimWorld (app id `294100`).
    pub workshop_dir: PathBuf,
    /// The active `ModsConfig.xml`.
    pub mods_config: PathBuf,
    /// This project's profile directory: decisions, rules, and imported
    /// RimSort snapshots live under here.
    pub profile_dir: PathBuf,
}

impl ProjectPaths {
    /// The profile's 12-hex-char identity, derived from
    /// [`Self::profile_dir`]'s own final path segment — the same hash
    /// `rim_io::profile_dir` names the directory with, so it never drifts
    /// from the directory this project's state actually lives under and
    /// no separate field is needed to carry it. Used to derive the
    /// generated merge mod's stable identity
    /// ([`rim_resolve::domain::GeneratedModIdentity::for_profile`]).
    ///
    /// Falls back to `"unknown"` when [`Self::profile_dir`] has no final
    /// path segment (e.g. it's empty or `.`/`..`) — a malformed profile
    /// directory is a caller bug, not a reason to panic here.
    #[must_use]
    pub fn profile_hash(&self) -> &str {
        self.profile_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
    }
}
