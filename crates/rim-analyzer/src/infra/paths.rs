//! Default path and game-version resolution for the CLI, kept separate
//! from `main.rs` so it's testable without a real RimWorld install.
//!
//! Install **detection** lives here too ([`default_game_dir_candidates`],
//! [`detect_game_dir`], [`is_game_dir`]), so no machine-specific install path
//! is hardcoded in production code. [`is_game_dir`] is the *only* definition
//! of "this directory is a RimWorld install" in the workspace — detection and
//! the real-install test guards both call it, so the two can never drift.
//!
//! **Platform support.** The candidate lists below are emitted per OS so a
//! macOS or Linux user gets a sane "looked in these places" error instead of
//! a Windows path they could never have. That is *not* a claim of non-Windows
//! support: this workspace is Windows-only today (`deny.toml` pins
//! `x86_64-pc-windows-msvc`, [`managed_assemblies_dir`] knows only the
//! `RimWorldWin64_Data` layout, and CI is `windows-latest`). Real macOS/Linux
//! support is separate, unimplemented work.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::domain::GameVersion;

/// The Steam app id for RimWorld — the `workshop/content/<id>` folder
/// name, and the `steamapps` subfolder every Steam library shares.
const RIMWORLD_STEAM_APP_ID: &str = "294100";

/// `<game_dir>/RimWorldWin64_Data/Managed`: the game's own managed
/// assembly folder. Core/DLC ship no `Assemblies/` folder of their own —
/// their assembly names have to be seeded from here.
#[must_use]
pub fn managed_assemblies_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("RimWorldWin64_Data").join("Managed")
}

/// `<game_dir>/RimWorldWin64_Data/globalgamemanagers`: Unity's own
/// serialized resource manifest, scanned (never fully parsed) for Core's
/// built-in texture container paths — see
/// `extract::asset_index::resource_container_paths`.
#[must_use]
pub fn global_game_managers_path(game_dir: &Path) -> PathBuf {
    game_dir
        .join("RimWorldWin64_Data")
        .join("globalgamemanagers")
}

/// `<game_dir>/Data/Core/Defs/Misc/ExpansionDefs/ExpansionDefs.xml`: the
/// Core/DLC display-name source (every DLC's `ExpansionDef` ships in
/// Core's `Defs/` folder, not its own).
#[must_use]
pub fn expansion_defs_path(game_dir: &Path) -> PathBuf {
    game_dir
        .join("Data")
        .join("Core")
        .join("Defs")
        .join("Misc")
        .join("ExpansionDefs")
        .join("ExpansionDefs.xml")
}

/// `<game_dir>/../../workshop/content/294100`, i.e. the sibling
/// `workshop/content/294100` folder two levels up from
/// `steamapps/common/RimWorld`.
#[must_use]
pub fn default_workshop_dir(game_dir: &Path) -> PathBuf {
    game_dir
        .parent()
        .and_then(Path::parent)
        .map(|steamapps| {
            steamapps
                .join("workshop")
                .join("content")
                .join(RIMWORLD_STEAM_APP_ID)
        })
        .unwrap_or_else(|| {
            game_dir
                .join("..")
                .join("..")
                .join("workshop")
                .join("content")
                .join(RIMWORLD_STEAM_APP_ID)
        })
}

/// `%USERPROFILE%\AppData\LocalLow\Ludeon Studios\RimWorld by Ludeon Studios\Config\ModsConfig.xml`.
pub fn default_mods_config_path() -> Result<PathBuf> {
    let profile =
        std::env::var("USERPROFILE").context("USERPROFILE environment variable is not set")?;
    Ok(PathBuf::from(profile)
        .join("AppData")
        .join("LocalLow")
        .join("Ludeon Studios")
        .join("RimWorld by Ludeon Studios")
        .join("Config")
        .join("ModsConfig.xml"))
}

/// The game version from `<game_dir>/Version.txt`'s first two components.
pub fn default_game_version(game_dir: &Path) -> Result<GameVersion> {
    let version_path = game_dir.join("Version.txt");
    let text = std::fs::read_to_string(&version_path)
        .with_context(|| format!("reading game version from {}", version_path.display()))?;
    text.trim()
        .parse()
        .with_context(|| format!("parsing game version from {}", version_path.display()))
}

/// `RIMMERGE_GAME_DIR`: the install directory override, and the gate the
/// whole real-install test tier keys on (see `docs/testing.md`).
///
/// The three variable *names* live here, beside [`is_game_dir`], rather
/// than in `rim-io` where the ladder that reads them lives: the test
/// guards in this crate's own `tests/` can't depend on `rim-io` (it
/// depends on this crate), and three copies of a string literal is
/// exactly how a rename silently un-gates a tier. `rim-io` re-exports
/// them.
pub const GAME_DIR_VAR: &str = "RIMMERGE_GAME_DIR";
/// `RIMMERGE_WORKSHOP_DIR`: the Steam workshop content folder override.
/// See [`GAME_DIR_VAR`] for why these names live in this module.
pub const WORKSHOP_DIR_VAR: &str = "RIMMERGE_WORKSHOP_DIR";
/// `RIMMERGE_MODS_CONFIG`: the active `ModsConfig.xml` override. See
/// [`GAME_DIR_VAR`] for why these names live in this module.
pub const MODS_CONFIG_VAR: &str = "RIMMERGE_MODS_CONFIG";

/// The value of environment variable `key`, if it is set to something
/// other than whitespace.
///
/// `var_os`, not `var`: a path is not required to be UTF-8, and a Windows
/// install directory with an unpaired surrogate in its name would come
/// back `Err(NotUnicode)` from `var` and silently fall through to the
/// next rung — resolving to a *different* install rather than the one the
/// user named. `PathBuf::from(OsString)` is lossless.
#[must_use]
pub fn path_var(key: &str) -> Option<PathBuf> {
    let value = std::env::var_os(key)?;
    let path = PathBuf::from(value);
    // An empty (or all-whitespace) value is `set VAR=` — "unset this",
    // not "resolve the install to the empty path".
    if path.as_os_str().is_empty() || path.to_string_lossy().trim().is_empty() {
        return None;
    }
    Some(path)
}

/// True when `path` looks like a real RimWorld install: it has both a
/// `Version.txt` file and a `Data/Core/` directory.
///
/// The single definition of "this is an install" in the workspace. Both
/// halves matter: `Version.txt` alone is satisfied by plenty of unrelated
/// folders, and `Data/Core/` alone by a half-copied tree with no version
/// to parse ([`default_game_version`] reads `Version.txt` immediately
/// after, and the scan reads [`expansion_defs_path`] under `Data/Core/`).
#[must_use]
pub fn is_game_dir(path: &Path) -> bool {
    path.join("Version.txt").is_file() && path.join("Data").join("Core").is_dir()
}

/// Every place a RimWorld install is plausibly installed on this machine,
/// most-likely first. Reads environment variables and, on a Steam root
/// that has one, `steamapps/libraryfolders.vdf`; nothing else touches the
/// filesystem, and the returned paths are *candidates* — none is known to
/// exist. Duplicates are removed, keeping the first occurrence.
///
/// Windows order: every Steam library named by a `libraryfolders.vdf`
/// under `%ProgramFiles(x86)%\Steam` / `%ProgramFiles%\Steam` (in the
/// file's own order), then those two Steam roots' own `steamapps\common`
/// folders, then the two standard GOG locations.
///
/// The Windows registry (`HKCU\Software\Valve\Steam\SteamPath`) is
/// deliberately *not* read: it would cost a new dependency for a case the
/// two `%ProgramFiles*%` probes already cover, including a library on a
/// second drive (the `libraryfolders.vdf` found under the default root
/// lists every library, wherever it lives).
#[must_use]
pub fn default_game_dir_candidates() -> Vec<PathBuf> {
    dedup_preserving_order(platform_candidates())
}

/// The first [`default_game_dir_candidates`] entry that passes
/// [`is_game_dir`], or `None` when this machine has no detectable install.
/// Never falls back to a guess — a caller with nothing to show the user
/// should list the candidates instead (that is what `rim_io`'s
/// `PathResolutionError` does).
#[must_use]
pub fn detect_game_dir() -> Option<PathBuf> {
    detect_from(default_game_dir_candidates())
}

/// The first entry of `candidates` that passes [`is_game_dir`].
///
/// Split out of [`detect_game_dir`] so the selection rule can be tested
/// on both branches against a committed fixture: testing `detect_game_dir`
/// itself can only ever assert "whatever it returned was verified", which
/// is tautologically true on a machine with no install and says nothing
/// about the *order* the candidates are tried in.
fn detect_from(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|candidate| is_game_dir(candidate))
}

/// `<library>/steamapps/common/RimWorld` — where Steam puts the game
/// inside any one of its library folders.
fn rimworld_under_steam_library(library: &Path) -> PathBuf {
    library.join("steamapps").join("common").join("RimWorld")
}

/// Every `"path"  "<value>"` value in a `libraryfolders.vdf`, in file
/// order.
///
/// A ~60-line hand-rolled reader rather than a VDF crate: the file is a
/// nested key/value text format, and the one thing needed from it is the
/// `path` entry of each numbered library block. Keys and values are
/// double-quoted with C-style escapes (`\\` for a backslash, `\"` for a
/// quote), which is the whole of the syntax that matters here. Anything
/// unparseable is skipped, never an error — a malformed Steam config must
/// degrade to "no libraries found", not break path resolution.
fn steam_library_dirs(vdf: &str) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for line in vdf.lines() {
        let mut tokens = quoted_tokens(line);
        let Some(key) = tokens.next() else { continue };
        if key != "path" {
            continue;
        }
        let Some(value) = tokens.next() else { continue };
        if !value.is_empty() {
            libraries.push(PathBuf::from(value));
        }
    }
    libraries
}

/// The double-quoted tokens on one VDF line, with `\\`/`\"` unescaped.
fn quoted_tokens(line: &str) -> impl Iterator<Item = String> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || {
        // Skip to the opening quote of the next token.
        chars.find(|&c| c == '"')?;
        let mut token = String::new();
        loop {
            match chars.next() {
                None => return None, // unterminated token: not a token at all
                Some('"') => return Some(token),
                Some('\\') => match chars.next() {
                    None => return None,
                    Some(escaped) => token.push(escaped),
                },
                Some(other) => token.push(other),
            }
        }
    })
}

/// Reads `<steam_root>/steamapps/libraryfolders.vdf` and maps each
/// library it names to that library's RimWorld folder. An unreadable or
/// absent file yields nothing.
fn rimworld_dirs_from_steam_root(steam_root: &Path) -> Vec<PathBuf> {
    let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(&vdf_path) else {
        return Vec::new();
    };
    steam_library_dirs(&text)
        .iter()
        .map(|library| rimworld_under_steam_library(library))
        .collect()
}

fn dedup_preserving_order(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

#[cfg(windows)]
fn platform_candidates() -> Vec<PathBuf> {
    let steam_roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|program_files| PathBuf::from(program_files).join("Steam"))
        .collect();

    let mut candidates: Vec<PathBuf> = steam_roots
        .iter()
        .flat_map(|root| rimworld_dirs_from_steam_root(root))
        .collect();
    candidates.extend(
        steam_roots
            .iter()
            .map(|root| rimworld_under_steam_library(root)),
    );
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("GOG Galaxy")
                .join("Games")
                .join("RimWorld"),
        );
    }
    candidates.push(PathBuf::from(r"C:\GOG Games\RimWorld"));
    candidates
}

#[cfg(target_os = "macos")]
fn platform_candidates() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return vec![PathBuf::from("/Applications/RimWorld.app")];
    };
    let steam_root = home
        .join("Library")
        .join("Application Support")
        .join("Steam");
    let mut candidates = vec![rimworld_under_steam_library(&steam_root)];
    candidates.extend(rimworld_dirs_from_steam_root(&steam_root));
    candidates.push(PathBuf::from("/Applications/RimWorld.app"));
    candidates
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_candidates() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let steam_roots = [
        home.join(".steam").join("steam"),
        home.join(".local").join("share").join("Steam"),
        home.join(".var")
            .join("app")
            .join("com.valvesoftware.Steam")
            .join(".local")
            .join("share")
            .join("Steam"),
    ];
    let mut candidates: Vec<PathBuf> = steam_roots
        .iter()
        .map(|root| rimworld_under_steam_library(root))
        .collect();
    candidates.extend(
        steam_roots
            .iter()
            .flat_map(|root| rimworld_dirs_from_steam_root(root)),
    );
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(relative: &[&str]) -> PathBuf {
        relative.iter().fold(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures"),
            |path, segment| path.join(segment),
        )
    }

    fn sample_vdf() -> String {
        let path = fixture(&["steam", "libraryfolders.vdf"]);
        std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("reading {}: {error}", path.display());
        })
    }

    #[test]
    fn default_workshop_dir_is_a_sibling_of_common() {
        let game_dir = Path::new(r"Q:\Steam\steamapps\common\RimWorld");
        let workshop = default_workshop_dir(game_dir);
        assert_eq!(
            workshop,
            Path::new(r"Q:\Steam\steamapps\workshop\content\294100")
        );
    }

    #[test]
    fn the_sample_libraryfolders_vdf_yields_every_library_in_file_order() {
        let libraries = steam_library_dirs(&sample_vdf());

        assert_eq!(
            libraries,
            vec![
                PathBuf::from(r"Q:\Steam"),
                PathBuf::from(r"W:\SteamGames"),
                PathBuf::from("/media/example/steam"),
            ],
            "every library block's `path` value, in the file's own order"
        );
    }

    #[test]
    fn vdf_values_unescape_backslashes_and_quotes() {
        let vdf =
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"Q:\\\\Steam\\\\a b\"\n\t}\n}";

        assert_eq!(
            steam_library_dirs(vdf),
            vec![PathBuf::from(r"Q:\Steam\a b")],
            r"`\\` in the file is one literal backslash"
        );
    }

    #[test]
    fn a_key_that_merely_contains_path_is_not_a_library() {
        let vdf = "\t\t\"contentstatsid\"\t\t\"12345\"\n\t\t\"mounted_path\"\t\t\"Q:\\\\Nope\"\n";

        assert!(
            steam_library_dirs(vdf).is_empty(),
            "only the exact key `path` names a library"
        );
    }

    #[test]
    fn a_malformed_vdf_yields_no_libraries_rather_than_an_error() {
        assert!(steam_library_dirs("").is_empty());
        assert!(steam_library_dirs("not a vdf at all").is_empty());
        assert!(
            steam_library_dirs("\t\"path\"\t\"unterminated").is_empty(),
            "a value whose closing quote is missing is not a usable path"
        );
        assert!(
            steam_library_dirs("\t\"path\"\n").is_empty(),
            "a key with no value on the line is skipped"
        );
    }

    #[test]
    fn a_library_becomes_that_librarys_rimworld_folder() {
        assert_eq!(
            rimworld_under_steam_library(Path::new(r"Q:\Steam")),
            PathBuf::from(r"Q:\Steam\steamapps\common\RimWorld")
        );
    }

    #[test]
    fn a_directory_with_version_txt_and_data_core_is_a_game_dir() {
        assert!(is_game_dir(&fixture(&["detect_game", "game"])));
    }

    #[test]
    fn a_directory_missing_either_half_is_not_a_game_dir() {
        assert!(
            !is_game_dir(&fixture(&["detect_game", "version_only"])),
            "Data/Core/ is missing"
        );
        assert!(
            !is_game_dir(&fixture(&["detect_game", "core_only"])),
            "Version.txt is missing"
        );
        assert!(
            !is_game_dir(&fixture(&["detect_game", "does_not_exist"])),
            "a path that isn't there at all is not an install"
        );
    }

    #[test]
    fn candidates_are_non_empty_deduplicated_and_all_named_rimworld() {
        let candidates = default_game_dir_candidates();

        assert!(
            !candidates.is_empty(),
            "every supported platform offers at least one place to look"
        );
        let mut sorted = candidates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            candidates.len(),
            "a library listed by two Steam roots must be offered once"
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.to_string_lossy().contains("RimWorld")),
            "every candidate names the game: {candidates:?}"
        );
    }

    #[test]
    fn dedup_keeps_the_first_occurrence() {
        let deduped = dedup_preserving_order(vec![
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/a"),
            PathBuf::from("/c"),
        ]);

        assert_eq!(
            deduped,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
    }

    #[test]
    fn detection_returns_none_when_no_candidate_is_an_install() {
        let candidates = vec![
            fixture(&["detect_game", "version_only"]),
            fixture(&["detect_game", "core_only"]),
            fixture(&["detect_game", "does_not_exist"]),
        ];

        assert_eq!(detect_from(candidates), None);
    }

    #[test]
    fn detection_returns_the_first_candidate_that_is_an_install_not_merely_the_first() {
        let install = fixture(&["detect_game", "game"]);
        let candidates = vec![
            fixture(&["detect_game", "does_not_exist"]),
            fixture(&["detect_game", "version_only"]),
            install.clone(),
            // A second real install after the first: order decides, so a
            // rule that scanned for "any" install rather than "the first"
            // would be free to return this one instead.
            install.clone(),
        ];

        assert_eq!(detect_from(candidates), Some(install));
    }

    #[test]
    fn a_whitespace_only_environment_variable_is_not_a_path() {
        // `path_var`'s own rule, checked without touching the process
        // environment: the emptiness test is on the value, and a value of
        // spaces must read as "unset", not as the path `"   "`.
        assert!(PathBuf::from("   ").to_string_lossy().trim().is_empty());
        assert!(
            !PathBuf::from("Q:/Steam")
                .to_string_lossy()
                .trim()
                .is_empty()
        );
    }

    #[test]
    fn the_environment_variable_names_are_the_documented_ones() {
        // These three strings are a public contract (root `CLAUDE.md`,
        // every real-install guard, `rimmerge config`'s own help text) —
        // pinned here so a rename has to be deliberate.
        assert_eq!(GAME_DIR_VAR, "RIMMERGE_GAME_DIR");
        assert_eq!(WORKSHOP_DIR_VAR, "RIMMERGE_WORKSHOP_DIR");
        assert_eq!(MODS_CONFIG_VAR, "RIMMERGE_MODS_CONFIG");
    }
}
