//! Normalizes a sound file path (as found under a `Sounds/` folder) into
//! the key RimWorld actually resolves by — extension-independent,
//! case-insensitive, `/`-separated — mirroring
//! [`crate::extract::textures::normalize`] for the sibling `Sounds/` scan.

use std::path::Path;

/// Audio file extensions RimWorld loads as sounds — the single source of
/// truth shared with `infra::mod_scan`'s `Sounds/` file walk. RimWorld's
/// audio backend (FMOD via Unity) loads `wav`, `mp3`, `ogg`, `xm`, `it`,
/// `mod`, and `s3m` from a mod's `Sounds/` folder — not `.wav` alone.
pub const SOUND_EXTENSIONS: [&str; 7] = ["wav", "mp3", "ogg", "xm", "it", "mod", "s3m"];

/// Normalizes `relative_path` — a path already relative to a `Sounds/`
/// folder, e.g. `Pawn/Melee/Punch.wav` — into its lookup key: extension
/// stripped, backslashes normalized to `/`, lowercased.
///
/// Returns `None` for paths that aren't audio files RimWorld would load.
#[must_use]
pub fn normalize(relative_path: &Path) -> Option<String> {
    let extension = relative_path.extension()?.to_str()?;
    if !SOUND_EXTENSIONS
        .iter()
        .any(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return None;
    }

    let without_extension = relative_path.with_extension("");
    let key = without_extension
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_extension_and_lowercases() {
        let key = normalize(&PathBuf::from("Pawn/Melee/Punch.WAV")).unwrap();
        assert_eq!(key, "pawn/melee/punch");
    }

    #[test]
    fn normalizes_backslash_separators() {
        let key = normalize(&PathBuf::from(r"Pawn\Melee\Punch.wav")).unwrap();
        assert_eq!(key, "pawn/melee/punch");
    }

    /// RimWorld's FMOD-backed audio loader accepts more than `.wav` —
    /// `.ogg` is common among mods shipping compressed music/ambience.
    #[test]
    fn accepts_ogg_files() {
        let key = normalize(&PathBuf::from("Ambient/Wind.ogg")).unwrap();
        assert_eq!(key, "ambient/wind");
    }

    #[test]
    fn rejects_non_audio_extensions() {
        assert_eq!(normalize(&PathBuf::from("readme.txt")), None);
    }

    #[test]
    fn rejects_extensionless_paths() {
        assert_eq!(normalize(&PathBuf::from("noext")), None);
    }
}
