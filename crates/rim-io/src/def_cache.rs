//! [`FsDefCacheCarrierProbe`]: the real, filesystem-backed
//! `rim_session::ports::DefCacheCarrierProbe`. A def-cache
//! plugin is not a
//! mod with its own packageId — it is a DLL shipped *inside* a
//! performance mod's own plugin folder, a path `rim-analyzer`'s scan
//! never walks (it walks `Assemblies/`). This probe does the one cheap
//! directory check the carrier detection needs: does the named plugin folder of
//! a mod's own resolved loaded folder contain a file whose name starts
//! with the carrier's prefix and ends with its extension?
//!
//! **Which prefix, which folder, which extension is data** (the `rules`
//! repo's `data/def-cache-carriers.json`, embedded into this binary at
//! compile time — see `build.rs`): that is knowledge about one specific
//! third-party plugin, not about RimWorld. The *walk* is code, and is
//! name-free.

use std::path::{Path, PathBuf};

use rim_session::ports::{DefCacheCarrier, DefCacheCarrierProbe};
use walkdir::WalkDir;

/// Real, filesystem-backed [`DefCacheCarrierProbe`].
#[derive(Debug, Clone, Copy, Default)]
pub struct FsDefCacheCarrierProbe;

impl FsDefCacheCarrierProbe {
    /// Builds the probe. Stateless — safe to construct once per call or
    /// share across the app's lifetime.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl DefCacheCarrierProbe for FsDefCacheCarrierProbe {
    fn has_def_cache_plugin(
        &self,
        loaded_folders: &[PathBuf],
        carriers: &[DefCacheCarrier],
    ) -> bool {
        loaded_folders.iter().any(|folder| {
            carriers
                .iter()
                .any(|carrier| plugin_dir_carries(&folder.join(&carrier.plugin_dir), carrier))
        })
    }
}

/// A no-op (not an error) when `plugin_dir` doesn't exist — most mods
/// carry no plugin folder at all. `filter_map(Result::ok)` also silently
/// skips any entry `walkdir` can't read (e.g. a permission error partway
/// through the tree) — acceptable here since [`DefCacheCarrierProbe`] has
/// no error channel of its own (a probe reports only "is this a carrier",
/// never "I couldn't tell"), but worth naming: an unreadable subtree reads
/// as "not a carrier" rather than "unknown".
fn plugin_dir_carries(plugin_dir: &Path, carrier: &DefCacheCarrier) -> bool {
    if !plugin_dir.is_dir() {
        return false;
    }
    let walk = if carrier.recursive {
        WalkDir::new(plugin_dir)
    } else {
        WalkDir::new(plugin_dir).max_depth(1)
    };
    walk.into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .any(|entry| matches_carrier(entry.file_name().to_str().unwrap_or(""), carrier))
}

/// `<file_prefix>*<file_extension>`, case-insensitive — matches the
/// carrier's own DLL and any forked/renamed variant without pinning to
/// one exact file name.
fn matches_carrier(file_name: &str, carrier: &DefCacheCarrier) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.starts_with(&carrier.file_prefix.to_ascii_lowercase())
        && lower.ends_with(&carrier.file_extension.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An invented carrier — this crate's own tests never name a real
    /// third-party plugin; the embedded bundle's own row is pinned by
    /// `mod_knowledge.rs`'s own contract test instead.
    fn carriers() -> Vec<DefCacheCarrier> {
        vec![DefCacheCarrier {
            id: "example".to_string(),
            plugin_dir: "Plugins".to_string(),
            file_prefix: "ExampleCache".to_string(),
            file_extension: ".dll".to_string(),
            recursive: true,
            log_line_prefix: "EXAMPLECACHE:".to_string(),
        }]
    }

    #[test]
    fn detects_a_carrier_dll_nested_under_the_plugin_dir() {
        let root = tempfile::tempdir().expect("tempdir");
        let nested = root.path().join("Plugins").join("Inner");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::write(nested.join("ExampleCache.dll"), b"").expect("write dll");

        assert!(
            FsDefCacheCarrierProbe::new()
                .has_def_cache_plugin(&[root.path().to_path_buf()], &carriers())
        );
    }

    #[test]
    fn a_mod_with_no_plugin_folder_at_all_is_not_a_carrier() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("Assemblies")).expect("create dir");

        assert!(
            !FsDefCacheCarrierProbe::new()
                .has_def_cache_plugin(&[root.path().to_path_buf()], &carriers())
        );
    }

    #[test]
    fn a_plugin_folder_with_no_matching_dll_is_not_a_carrier() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("Plugins")).expect("create dir");
        std::fs::write(root.path().join("Plugins").join("Other.dll"), b"").expect("write dll");

        assert!(
            !FsDefCacheCarrierProbe::new()
                .has_def_cache_plugin(&[root.path().to_path_buf()], &carriers())
        );
    }

    #[test]
    fn the_file_name_match_is_case_insensitive() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("Plugins")).expect("create dir");
        std::fs::write(root.path().join("Plugins").join("EXAMPLECACHE.DLL"), b"")
            .expect("write dll");

        assert!(
            FsDefCacheCarrierProbe::new()
                .has_def_cache_plugin(&[root.path().to_path_buf()], &carriers())
        );
    }

    /// `recursive: false` must genuinely stop at the folder itself — the
    /// data says how deep to look, and the code must honour it.
    #[test]
    fn a_non_recursive_carrier_never_looks_into_a_subfolder() {
        let root = tempfile::tempdir().expect("tempdir");
        let nested = root.path().join("Plugins").join("Inner");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::write(nested.join("ExampleCache.dll"), b"").expect("write dll");
        let mut shallow = carriers();
        shallow[0].recursive = false;

        assert!(
            !FsDefCacheCarrierProbe::new()
                .has_def_cache_plugin(&[root.path().to_path_buf()], &shallow)
        );
    }

    #[test]
    fn an_empty_carrier_list_never_matches_anything() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("Plugins")).expect("create dir");
        std::fs::write(root.path().join("Plugins").join("ExampleCache.dll"), b"")
            .expect("write dll");

        assert!(
            !FsDefCacheCarrierProbe::new().has_def_cache_plugin(&[root.path().to_path_buf()], &[])
        );
    }
}
