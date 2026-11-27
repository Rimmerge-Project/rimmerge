//! Which engine build an install ships, and the paths that follow from it.
//!
//! Pure: no filesystem access. Detecting the layout from a folder is
//! `infra::paths::detect_game_layout`.

use std::path::{Path, PathBuf};

/// The Unity player's data folder name (`<game_dir>/RimWorldWin64_Data`),
/// the parent of `Managed/` and `globalgamemanagers`.
pub const UNITY_DATA_DIR: &str = "RimWorldWin64_Data";

/// The Unity layout's executable file name.
pub const UNITY_EXECUTABLE_NAME: &str = "RimWorldWin64.exe";

/// The Waylith layout's executable file name.
pub const WAYLITH_EXECUTABLE_NAME: &str = "RimWorld.exe";

/// Which engine build an install ships: the stable branch's Unity player
/// (1.6 and earlier) or the unstable branch's own CoreCLR engine,
/// "Waylith" (1.7). They differ in where the executable, the vanilla
/// assemblies and the vanilla art live; everything else (`Version.txt`,
/// `Data/<DLC>/`, the user-data folders) is the same.
///
/// Detected, never configured: see `infra::paths::detect_game_layout`.
/// `is_game_dir` is layout-independent and stays the one definition of
/// "an install".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameLayout {
    /// `RimWorldWin64.exe` plus `RimWorldWin64_Data/` (`Managed/`,
    /// `globalgamemanagers`, per-DLC `AssetBundles/`).
    Unity,
    /// `RimWorld.exe`, `Assembly-CSharp.dll` and every other DLL at the
    /// install root, `Assets.zip` at the root and in each DLC folder.
    Waylith,
}

impl GameLayout {
    /// The executable's file name in the install root.
    #[must_use]
    pub const fn executable_name(self) -> &'static str {
        match self {
            Self::Unity => UNITY_EXECUTABLE_NAME,
            Self::Waylith => WAYLITH_EXECUTABLE_NAME,
        }
    }

    /// The folder holding the game's own assemblies: `Managed/` for Unity,
    /// the install root itself for Waylith.
    #[must_use]
    pub fn vanilla_assemblies_dir(self, game_dir: &Path) -> PathBuf {
        match self {
            Self::Unity => game_dir.join(UNITY_DATA_DIR).join("Managed"),
            Self::Waylith => game_dir.to_path_buf(),
        }
    }

    /// `Assembly-CSharp.dll`, the game's own code.
    #[must_use]
    pub fn assembly_csharp_path(self, game_dir: &Path) -> PathBuf {
        self.vanilla_assemblies_dir(game_dir)
            .join("Assembly-CSharp.dll")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_layout_names_its_own_executable_and_assembly_paths() {
        let game_dir = Path::new("game");

        assert_eq!(GameLayout::Unity.executable_name(), "RimWorldWin64.exe");
        assert_eq!(GameLayout::Waylith.executable_name(), "RimWorld.exe");
        assert_eq!(
            GameLayout::Unity.assembly_csharp_path(game_dir),
            game_dir
                .join("RimWorldWin64_Data")
                .join("Managed")
                .join("Assembly-CSharp.dll")
        );
        assert_eq!(
            GameLayout::Waylith.assembly_csharp_path(game_dir),
            game_dir.join("Assembly-CSharp.dll")
        );
        assert_eq!(
            GameLayout::Waylith.vanilla_assemblies_dir(game_dir),
            game_dir
        );
    }
}
