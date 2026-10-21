//! The game runtime's platform, independent of the analyzer's host OS.

/// The platform whose assets the selected RimWorld installation loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePlatform {
    /// Native Linux, including installations managed by Flatpak Steam.
    Linux,
    /// Windows, including installations running through Proton.
    Windows,
}
