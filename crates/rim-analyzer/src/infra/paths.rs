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
//! `x86_64-pc-windows-msvc`, `GameLayout` knows only the two Windows
//! layouts, and CI is `windows-latest`). Real macOS/Linux
//! support is separate, unimplemented work.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::domain::{GameLayout, GameVersion, UNITY_DATA_DIR};

/// The Steam app id for RimWorld — the `workshop/content/<id>` folder
/// name, and the `steamapps` subfolder every Steam library shares.
pub const RIMWORLD_STEAM_APP_ID: &str = "294100";

/// Which [`GameLayout`] `game_dir` is: Waylith when `RimWorld.exe` is a
/// file at the root and `RimWorldWin64.exe` is absent; Unity otherwise.
///
/// The Unity executable decides, not a leftover `RimWorldWin64_Data/` folder
/// (a branch switch can leave one behind). `Assembly-CSharp.dll` is not
/// required: a Waylith root missing it is a broken install that should
/// warn when read, not silently fall back to Unity paths. Unity is the
/// fallback on purpose: a folder with neither executable (a scratch
/// fixture, a half-deleted install) keeps every 1.6 path and message it
/// always had. Reads the filesystem; never writes.
#[must_use]
pub fn detect_game_layout(game_dir: &Path) -> GameLayout {
    let has_waylith_executable = game_dir
        .join(GameLayout::Waylith.executable_name())
        .is_file();
    let has_unity_executable = game_dir.join(GameLayout::Unity.executable_name()).exists();
    if has_waylith_executable && !has_unity_executable {
        GameLayout::Waylith
    } else {
        GameLayout::Unity
    }
}

/// The file name of a Waylith art archive: `<game_dir>/Assets.zip` for
/// Core's built-in textures, `<game_dir>/Data/<DLC>/Assets.zip` per DLC.
pub const ASSETS_ZIP_FILE_NAME: &str = "Assets.zip";

/// `<game_dir>/RimWorld.runtimeconfig.json`: the Waylith engine's own
/// configuration, which names the assembly redirects it applies.
#[must_use]
pub fn runtime_config_path(game_dir: &Path) -> PathBuf {
    game_dir.join("RimWorld.runtimeconfig.json")
}

/// `<game_dir>/RimWorldWin64_Data/globalgamemanagers`: Unity's own
/// serialized resource manifest, scanned (never fully parsed) for Core's
/// built-in texture container paths — see
/// `extract::asset_index::resource_container_paths`.
#[must_use]
pub fn global_game_managers_path(game_dir: &Path) -> PathBuf {
    game_dir.join(UNITY_DATA_DIR).join("globalgamemanagers")
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

/// The most bytes read from Steam's `appmanifest_<id>.acf`; a real one is
/// well under 2 KiB, so anything larger is not a manifest.
const STEAM_MANIFEST_MAX_BYTES: u64 = 64 * 1024;

/// The most bytes read from a Steam root's `libraryfolders.vdf`; it lists
/// every installed app per library, so it is far larger than a manifest, yet
/// a real one stays in the tens of KiB.
const STEAM_LIBRARY_LIST_MAX_BYTES: u64 = 1024 * 1024;

/// True when Steam itself installed `game_dir`: the folder sits in
/// `<library>/steamapps/common/`, `<library>/steamapps/appmanifest_294100.acf`
/// (Steam's own record of where app 294100 lives) names this folder as its
/// `"installdir"`, **and** `<library>` is one a Steam root on this machine
/// lists in its `libraryfolders.vdf` (or is that Steam root itself).
///
/// The library check rejects a backed-up copy of a whole Steam library: its
/// folder and manifest look exactly like the live ones, but
/// `steam://run/294100` would start the live copy. The roots are the default
/// ones [`detect_game_dir`] uses (the registry's Steam folders, then
/// `%ProgramFiles(x86)%\Steam` and `%ProgramFiles%\Steam`); a Steam none of
/// those names degrades to `false`, like everything else that can't be read.
///
/// `steam_appid.txt` is deliberately not consulted: every Steam copy has one,
/// so a copy taken out of Steam would pass. Names compare ASCII
/// case-insensitively (Windows paths). Library paths compare lexically
/// (`library_key`), never through `canonicalize`: Steam writes the path the
/// user typed, and a canonical path would differ from it by a `\\?\` prefix
/// or a resolved junction/`subst` drive, so a mismatch only costs the safe
/// Executable route. The manifest is read with a 64 KiB bound and each
/// `libraryfolders.vdf` with a 1 MiB one; a missing, non-regular, oversized,
/// or unparseable file means `false`, never an error.
#[must_use]
pub fn is_steam_managed_install(game_dir: &Path) -> bool {
    is_steam_managed_install_under(game_dir, &default_steam_roots())
}

/// [`is_steam_managed_install`] against an explicit list of Steam roots, so
/// tests never depend on this machine's real Steam.
fn is_steam_managed_install_under(game_dir: &Path, steam_roots: &[PathBuf]) -> bool {
    let Some(folder_name) = game_dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(common) = game_dir.parent() else {
        return false;
    };
    let Some(steamapps) = common.parent() else {
        return false;
    };
    let is_named = |path: &Path, expected: &str| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(expected))
    };
    if !is_named(common, "common") || !is_named(steamapps, "steamapps") {
        return false;
    }
    let Some(library) = steamapps.parent() else {
        return false;
    };
    let manifest = steamapps.join(format!("appmanifest_{RIMWORLD_STEAM_APP_ID}.acf"));
    let names_this_folder = read_bounded_regular_file(&manifest, STEAM_MANIFEST_MAX_BYTES)
        .and_then(|text| manifest_install_dir(&text))
        .is_some_and(|install_dir| install_dir.eq_ignore_ascii_case(folder_name));
    names_this_folder && is_listed_steam_library(library, steam_roots)
}

/// Whether `library` is a Steam root whose `libraryfolders.vdf` is readable,
/// or a library that file lists. (Steam normally lists the root itself too;
/// accepting a root with a readable file keeps that working if it did not.)
fn is_listed_steam_library(library: &Path, steam_roots: &[PathBuf]) -> bool {
    let wanted = library_key(library);
    steam_roots.iter().any(|root| {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        let Some(text) = read_bounded_regular_file(&vdf, STEAM_LIBRARY_LIST_MAX_BYTES) else {
            return false;
        };
        library_key(root) == wanted
            || steam_library_dirs(&text)
                .iter()
                .any(|listed| library_key(listed) == wanted)
    })
}

/// A library path reduced for lexical comparison: `/` as `\`, no trailing
/// separator, ASCII lowercase.
fn library_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// The text of `path` when it is a regular file (not a symlink or folder)
/// of at most `max_bytes` bytes that reads as UTF-8; `None` otherwise.
fn read_bounded_regular_file(path: &Path, max_bytes: u64) -> Option<String> {
    use std::io::Read;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        // One byte past the bound, so a file that grew since `metadata`
        // is still caught below instead of being silently truncated.
        .take(max_bytes + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= max_bytes).then_some(text)
}

/// The `"installdir"` value of an `appmanifest_*.acf`, the first such
/// line's second token.
fn manifest_install_dir(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let mut tokens = quoted_tokens(line);
        let key = tokens.next()?;
        if !key.eq_ignore_ascii_case("installdir") {
            return None;
        }
        tokens.next().filter(|value| !value.is_empty())
    })
}

/// Every place a RimWorld install is plausibly installed on this machine,
/// most-likely first. Reads environment variables and, on a Steam root
/// that has one, `steamapps/libraryfolders.vdf`; nothing else touches the
/// filesystem, and the returned paths are *candidates* — none is known to
/// exist. Duplicates are removed, keeping the first occurrence.
///
/// Windows order: every Steam library named by a `libraryfolders.vdf`
/// under a Steam root (in the file's own order), then those roots' own
/// `steamapps\common` folders, then the two standard GOG locations. The
/// Steam roots are the registry's (`HKCU\Software\Valve\Steam\SteamPath`,
/// `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath`, read-only and
/// local) followed by `%ProgramFiles(x86)%\Steam` / `%ProgramFiles%\Steam`,
/// so a Steam installed outside Program Files is found too.
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
/// library it names to that library's RimWorld folder. An unreadable,
/// absent, non-regular or oversized (over `STEAM_LIBRARY_LIST_MAX_BYTES`)
/// file yields nothing.
fn rimworld_dirs_from_steam_root(steam_root: &Path) -> Vec<PathBuf> {
    let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");
    let Some(text) = read_bounded_regular_file(&vdf_path, STEAM_LIBRARY_LIST_MAX_BYTES) else {
        return Vec::new();
    };
    steam_library_dirs(&text)
        .iter()
        .map(|library| rimworld_under_steam_library(library))
        .collect()
}

/// Drops a candidate repeated under a different case, separator or trailing
/// slash (`library_key`), keeping the first spelling: Windows paths are
/// case-insensitive, and Steam writes some of its own roots in lowercase.
/// On a case-sensitive file system this could merge two distinct paths;
/// only Windows is a supported target (`deny.toml`).
fn dedup_preserving_order(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(library_key(candidate)))
        .collect()
}

/// The Steam roots on this machine, most likely first: the ones Steam
/// itself recorded in the registry (so a Steam installed on any drive is
/// found), then the two `%ProgramFiles*%\Steam` defaults. Reads the
/// registry (read-only, local) and environment variables; none is known to
/// exist on disk.
#[cfg(windows)]
fn default_steam_roots() -> Vec<PathBuf> {
    let (current_user_root, machine_root) = registry_steam_roots();
    let correctly_cased_roots: Vec<PathBuf> = machine_root
        .into_iter()
        .chain(
            ["ProgramFiles(x86)", "ProgramFiles"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|program_files| PathBuf::from(program_files).join("Steam")),
        )
        .collect();
    steam_roots_from(current_user_root, correctly_cased_roots)
}

/// The Steam install folders Steam's own registry keys name: the current
/// user's `SteamPath` (which Steam writes lowercased) and the machine-wide
/// `InstallPath`. A missing or unreadable key yields `None`, never an error.
#[cfg(windows)]
fn registry_steam_roots() -> (Option<PathBuf>, Option<PathBuf>) {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let read = |hive, subkey: &str, value: &str| -> Option<PathBuf> {
        // REG_EXPAND_SZ is read unexpanded; harmless, since a path with an
        // unexpanded `%VAR%` fails the later stat like any other missing one.
        let text: String = RegKey::predef(hive)
            .open_subkey(subkey)
            .ok()?
            .get_value(value)
            .ok()?;
        (!text.trim().is_empty()).then(|| PathBuf::from(text))
    };
    (
        read(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        read(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ),
    )
}

/// `current_user_root` (the registry's `SteamPath`) then `other_roots`
/// (the machine-wide `InstallPath`, then the `%ProgramFiles*%\Steam`
/// defaults), with `/` read as `\`. A root repeated under a different case,
/// separator or trailing slash (`library_key`) is dropped, keeping the first
/// position; its spelling is the first one from `other_roots` when there is
/// one, because Steam writes `SteamPath` in lowercase and the other sources
/// carry the folder's real case.
#[cfg(any(windows, test))]
fn steam_roots_from(current_user_root: Option<PathBuf>, other_roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let normalized = |root: PathBuf| PathBuf::from(root.to_string_lossy().replace('/', "\\"));
    let current_user_root = current_user_root.map(normalized);
    let other_roots: Vec<PathBuf> = other_roots.into_iter().map(normalized).collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut roots = Vec::new();
    for root in current_user_root.iter().chain(&other_roots) {
        if !seen.insert(library_key(root)) {
            continue;
        }
        let real_case = other_roots
            .iter()
            .find(|other| library_key(other) == library_key(root));
        roots.push(real_case.unwrap_or(root).clone());
    }
    roots
}

#[cfg(windows)]
fn platform_candidates() -> Vec<PathBuf> {
    windows_candidates_from(
        &default_steam_roots(),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
    )
}

/// The Windows candidate list over injected Steam roots: every library each
/// root's `libraryfolders.vdf` names (file order), then each root's own
/// `steamapps\common`, then the two standard GOG locations.
#[cfg(any(windows, test))]
fn windows_candidates_from(
    steam_roots: &[PathBuf],
    program_files_x86: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = steam_roots
        .iter()
        .flat_map(|root| rimworld_dirs_from_steam_root(root))
        .collect();
    candidates.extend(
        steam_roots
            .iter()
            .map(|root| rimworld_under_steam_library(root)),
    );
    if let Some(program_files_x86) = program_files_x86 {
        candidates.push(
            program_files_x86
                .join("GOG Galaxy")
                .join("Games")
                .join("RimWorld"),
        );
    }
    candidates.push(PathBuf::from(r"C:\GOG Games\RimWorld"));
    candidates
}

#[cfg(target_os = "macos")]
fn default_steam_roots() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Steam")
        })
        .into_iter()
        .collect()
}

#[cfg(target_os = "macos")]
fn platform_candidates() -> Vec<PathBuf> {
    let steam_roots = default_steam_roots();
    let mut candidates: Vec<PathBuf> = steam_roots
        .iter()
        .map(|root| rimworld_under_steam_library(root))
        .collect();
    candidates.extend(
        steam_roots
            .iter()
            .flat_map(|root| rimworld_dirs_from_steam_root(root)),
    );
    candidates.push(PathBuf::from("/Applications/RimWorld.app"));
    candidates
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_steam_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    vec![
        home.join(".steam").join("steam"),
        home.join(".local").join("share").join("Steam"),
        home.join(".var")
            .join("app")
            .join("com.valvesoftware.Steam")
            .join(".local")
            .join("share")
            .join("Steam"),
    ]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_candidates() -> Vec<PathBuf> {
    let steam_roots = default_steam_roots();
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

    fn layout_dir(files: &[&str], folders: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in files {
            std::fs::write(dir.path().join(name), b"x").expect("file");
        }
        for name in folders {
            std::fs::create_dir(dir.path().join(name)).expect("folder");
        }
        dir
    }

    #[test]
    fn a_root_with_only_rimworld_exe_is_waylith() {
        let dir = layout_dir(&["RimWorld.exe"], &[]);
        let with_dll = layout_dir(&["RimWorld.exe", "Assembly-CSharp.dll"], &[]);

        assert_eq!(detect_game_layout(dir.path()), GameLayout::Waylith);
        assert_eq!(detect_game_layout(with_dll.path()), GameLayout::Waylith);
    }

    #[test]
    fn a_stray_unity_data_folder_does_not_hide_a_waylith_install() {
        let dir = layout_dir(
            &["RimWorld.exe", "Assembly-CSharp.dll"],
            &["RimWorldWin64_Data"],
        );

        assert_eq!(detect_game_layout(dir.path()), GameLayout::Waylith);
    }

    #[test]
    fn the_unity_executable_beside_rimworld_exe_keeps_the_unity_layout() {
        let dir = layout_dir(
            &["RimWorld.exe", "RimWorldWin64.exe", "Assembly-CSharp.dll"],
            &[],
        );

        assert_eq!(detect_game_layout(dir.path()), GameLayout::Unity);
    }

    #[test]
    fn an_install_without_a_file_named_rimworld_exe_falls_back_to_unity() {
        let empty = layout_dir(&[], &[]);
        let only_unity_data = layout_dir(&[], &["RimWorldWin64_Data"]);
        let folder_named_exe = layout_dir(&[], &["RimWorld.exe"]);

        assert_eq!(detect_game_layout(empty.path()), GameLayout::Unity);
        assert_eq!(
            detect_game_layout(only_unity_data.path()),
            GameLayout::Unity
        );
        assert_eq!(
            detect_game_layout(folder_named_exe.path()),
            GameLayout::Unity
        );
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

    /// A scratch dir under the OS temp root, removed on drop. Manual because
    /// this crate has no `tempfile` dependency (see `discovery.rs`'s tests).
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test_name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "rim-analyzer-paths-{test_name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create scratch dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn manifest_naming(install_dir: &str) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"294100\"\n\t\"installdir\"\t\t\"{install_dir}\"\n}}\n"
        )
    }

    /// A `libraryfolders.vdf` body listing `libraries`, written as Steam
    /// does (one numbered block each, backslashes doubled).
    fn libraryfolders_vdf(libraries: &[String]) -> String {
        let blocks: String = libraries
            .iter()
            .enumerate()
            .map(|(index, library)| {
                let escaped = library.replace('\\', "\\\\");
                format!("\t\"{index}\"\n\t{{\n\t\t\"path\"\t\t\"{escaped}\"\n\t}}\n")
            })
            .collect();
        format!("\"libraryfolders\"\n{{\n{blocks}}}\n")
    }

    /// Makes `steam_root` a Steam root whose `libraryfolders.vdf` is `body`.
    fn write_library_list(steam_root: &Path, body: &str) {
        let steamapps = steam_root.join("steamapps");
        std::fs::create_dir_all(&steamapps).expect("create steamapps");
        std::fs::write(steamapps.join("libraryfolders.vdf"), body).expect("write vdf");
    }

    fn display(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// `<library>/steamapps/common/<folder>`, plus the manifest when given.
    fn place_game(library: &Path, folder: &str, manifest: Option<&str>) -> PathBuf {
        let steamapps = library.join("steamapps");
        let game_dir = steamapps.join("common").join(folder);
        std::fs::create_dir_all(&game_dir).expect("create game dir");
        if let Some(body) = manifest {
            std::fs::write(steamapps.join("appmanifest_294100.acf"), body).expect("write manifest");
        }
        game_dir
    }

    /// A live Steam root (`<scratch>/steam`) listing itself, with the game
    /// installed in it and a valid manifest unless `manifest` says otherwise.
    fn steam_layout(
        test_name: &str,
        folder: &str,
        manifest: Option<&str>,
    ) -> (Scratch, PathBuf, PathBuf) {
        let scratch = Scratch::new(test_name);
        let steam_root = scratch.path().join("steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&[display(&steam_root)]));
        let game_dir = place_game(&steam_root, folder, manifest);
        (scratch, steam_root, game_dir)
    }

    #[test]
    fn a_folder_named_by_its_steam_app_manifest_is_steam_managed() {
        let (_scratch, steam_root, game_dir) =
            steam_layout("named", "RimWorld", Some(&manifest_naming("RimWorld")));

        assert!(is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_copy_beside_the_steam_install_is_not_steam_managed() {
        let (_scratch, steam_root, game_dir) = steam_layout(
            "backup",
            "RimWorld_backup",
            Some(&manifest_naming("RimWorld")),
        );

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn steam_appid_txt_alone_is_not_steam_managed() {
        let (_scratch, steam_root, game_dir) = steam_layout("appid", "RimWorld", None);
        std::fs::write(game_dir.join("steam_appid.txt"), "294100").expect("write appid");

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    /// A game folder at `game_relative` under a scratch dir, a valid manifest
    /// at `manifest_dir`, and a Steam root listing every ancestor of the game
    /// folder as a library, so only the path shape can make the install not
    /// Steam-managed.
    fn shape_layout(
        test_name: &str,
        game_relative: &[&str],
        manifest_dir: &[&str],
    ) -> (Scratch, PathBuf, PathBuf) {
        let scratch = Scratch::new(test_name);
        let under_scratch = |parts: &[&str]| {
            parts
                .iter()
                .fold(scratch.path().to_path_buf(), |path, part| path.join(part))
        };
        let game_dir = under_scratch(game_relative);
        std::fs::create_dir_all(&game_dir).expect("create game dir");
        std::fs::write(
            under_scratch(manifest_dir).join("appmanifest_294100.acf"),
            manifest_naming("RimWorld"),
        )
        .expect("write manifest");
        let every_ancestor: Vec<String> = game_dir.ancestors().skip(1).map(display).collect();
        let steam_root = scratch.path().join("steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&every_ancestor));
        (scratch, steam_root, game_dir)
    }

    #[test]
    fn a_folder_outside_steamapps_common_is_not_steam_managed() {
        let (_scratch, steam_root, game_dir) = shape_layout("gog", &["Games", "RimWorld"], &[]);

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_parent_not_named_common_is_not_steam_managed() {
        let (_scratch, steam_root, game_dir) = shape_layout(
            "notcommon",
            &["steamapps", "notcommon", "RimWorld"],
            &["steamapps"],
        );

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_grandparent_not_named_steamapps_is_not_steam_managed() {
        let (_scratch, steam_root, game_dir) = shape_layout(
            "notsteamapps",
            &["notsteamapps", "common", "RimWorld"],
            &["notsteamapps"],
        );

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn the_manifest_and_folder_names_compare_case_insensitively() {
        let scratch = Scratch::new("case");
        let library = scratch.path().join("Library");
        let game_dir = library.join("SteamApps").join("Common").join("rimworld");
        std::fs::create_dir_all(&game_dir).expect("create game dir");
        std::fs::write(
            library.join("SteamApps").join("appmanifest_294100.acf"),
            manifest_naming("RIMWORLD"),
        )
        .expect("write manifest");
        let steam_root = scratch.path().join("steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&[display(&library)]));

        assert!(is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_registry_steam_root_outside_program_files_is_searched_first() {
        let roots = steam_roots_from(
            Some(PathBuf::from("q:/steam")),
            vec![
                PathBuf::from(r"E:\Other"),
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"Q:\Steam"),
            ],
        );

        assert_eq!(
            roots,
            vec![
                PathBuf::from(r"Q:\Steam"),
                PathBuf::from(r"E:\Other"),
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
            ],
            "the user's root leads, `/` becomes `\\`, and the lowercase duplicate takes the real-case spelling"
        );
    }

    #[test]
    fn detection_finds_rimworld_in_a_library_listed_by_a_non_default_steam_root() {
        let scratch = Scratch::new("detect-nondefault");
        let steam_root = scratch.path().join("Steam");
        let library = scratch.path().join("Games").join("Library");
        write_library_list(
            &steam_root,
            &libraryfolders_vdf(&[display(&steam_root), display(&library)]),
        );
        let game_dir = place_game(&library, "RimWorld", None);
        std::fs::write(game_dir.join("Version.txt"), "1.6.0 rev1\n").expect("write version");
        std::fs::create_dir_all(game_dir.join("Data").join("Core")).expect("create core");

        let roots = steam_roots_from(Some(steam_root), Vec::new());
        let detected = detect_from(dedup_preserving_order(windows_candidates_from(
            &roots, None,
        )));

        assert_eq!(detected, Some(game_dir));
    }

    #[test]
    fn a_lowercase_registry_root_listed_in_real_case_yields_one_candidate() {
        let scratch = Scratch::new("registry-case");
        let steam_root = scratch.path().join("Steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&[display(&steam_root)]));
        let registry_spelling =
            PathBuf::from(display(&steam_root).replace('\\', "/").to_lowercase());
        let roots = steam_roots_from(Some(registry_spelling), Vec::new());

        let candidates = dedup_preserving_order(windows_candidates_from(&roots, None));

        let own_candidate = rimworld_under_steam_library(&steam_root);
        let for_this_root: Vec<&PathBuf> = candidates
            .iter()
            .filter(|candidate| library_key(candidate) == library_key(&own_candidate))
            .collect();
        assert_eq!(
            for_this_root,
            vec![&own_candidate],
            "one candidate, in the real case the library list spells"
        );
    }

    #[test]
    fn a_real_case_root_beats_the_lowercase_registry_spelling_without_a_library_list() {
        let scratch = Scratch::new("registry-case-no-vdf");
        let steam_root = scratch.path().join("Steam");
        let registry_spelling =
            PathBuf::from(display(&steam_root).replace('\\', "/").to_lowercase());
        let roots = steam_roots_from(Some(registry_spelling), vec![steam_root.clone()]);

        let candidates = dedup_preserving_order(windows_candidates_from(&roots, None));

        assert_eq!(
            candidates.first(),
            Some(&rimworld_under_steam_library(&steam_root))
        );
    }

    #[test]
    fn a_steam_copy_under_a_registry_root_is_steam_managed() {
        let (_scratch, steam_root, game_dir) = steam_layout(
            "registry-root",
            "RimWorld",
            Some(&manifest_naming("RimWorld")),
        );
        // As Steam writes `SteamPath`: lowercase, forward slashes.
        let registry_spelling =
            PathBuf::from(display(&steam_root).replace('\\', "/").to_lowercase());
        // No HKLM or Program Files spelling to swap in: the lowercase `/`
        // form itself must be accepted (Steam outside Program Files).
        let roots = steam_roots_from(Some(registry_spelling), Vec::new());

        assert!(is_steam_managed_install_under(&game_dir, &roots));
    }

    #[test]
    fn an_oversized_library_list_yields_no_candidates() {
        let scratch = Scratch::new("oversized-vdf");
        let steam_root = scratch.path().join("Steam");
        let padding = " ".repeat(STEAM_LIBRARY_LIST_MAX_BYTES as usize + 1);
        write_library_list(
            &steam_root,
            &format!("{}{padding}", libraryfolders_vdf(&["Q:\\Lib".to_owned()])),
        );

        assert!(rimworld_dirs_from_steam_root(&steam_root).is_empty());
    }

    #[test]
    fn a_listed_library_compares_by_case_separators_and_trailing_slash() {
        let scratch = Scratch::new("listed-spelling");
        let library = scratch.path().join("Library");
        let game_dir = place_game(&library, "RimWorld", Some(&manifest_naming("RimWorld")));
        let respelled = format!(
            "{}\\",
            display(&library).to_ascii_uppercase().replace('\\', "/")
        );
        let steam_root = scratch.path().join("steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&[respelled]));

        assert!(is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn an_oversized_or_unreadable_manifest_is_not_steam_managed() {
        let padding = " ".repeat(STEAM_MANIFEST_MAX_BYTES as usize);
        let oversized = format!("{}{padding}", manifest_naming("RimWorld"));
        let (_scratch, steam_root, game_dir) =
            steam_layout("oversized", "RimWorld", Some(&oversized));
        assert!(
            !is_steam_managed_install_under(&game_dir, &[steam_root]),
            "oversized"
        );

        let (_scratch, steam_root, game_dir) = steam_layout("dirmanifest", "RimWorld", None);
        let steamapps = game_dir.parent().and_then(Path::parent).expect("steamapps");
        std::fs::create_dir(steamapps.join("appmanifest_294100.acf")).expect("folder manifest");
        assert!(
            !is_steam_managed_install_under(&game_dir, &[steam_root]),
            "not a regular file"
        );

        let (_scratch, steam_root, game_dir) = steam_layout("binary", "RimWorld", None);
        let steamapps = game_dir.parent().and_then(Path::parent).expect("steamapps");
        let invalid_utf8 = [manifest_naming("RimWorld").as_bytes(), &[0xFF]].concat();
        std::fs::write(steamapps.join("appmanifest_294100.acf"), invalid_utf8)
            .expect("write binary manifest");
        assert!(
            !is_steam_managed_install_under(&game_dir, &[steam_root]),
            "not UTF-8"
        );
    }

    #[test]
    fn a_backed_up_steam_library_is_not_steam_managed() {
        let scratch = Scratch::new("backed-up");
        let steam_root = scratch.path().join("steam");
        write_library_list(&steam_root, &libraryfolders_vdf(&[display(&steam_root)]));
        let backup = scratch.path().join("backup").join("ExtraLibrary");
        let game_dir = place_game(&backup, "RimWorld", Some(&manifest_naming("RimWorld")));

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_library_listed_in_a_steam_roots_library_folders_is_steam_managed() {
        let scratch = Scratch::new("second-library");
        let library = scratch.path().join("ExtraLibrary");
        let game_dir = place_game(&library, "RimWorld", Some(&manifest_naming("RimWorld")));
        let steam_root = scratch.path().join("steam");
        write_library_list(
            &steam_root,
            &libraryfolders_vdf(&[display(&steam_root), display(&library)]),
        );

        assert!(is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn the_steam_roots_own_library_is_steam_managed_even_when_not_listed() {
        let scratch = Scratch::new("root-library");
        let steam_root = scratch.path().join("steam");
        write_library_list(
            &steam_root,
            &libraryfolders_vdf(&[display(&scratch.path().join("elsewhere"))]),
        );
        let game_dir = place_game(&steam_root, "RimWorld", Some(&manifest_naming("RimWorld")));

        assert!(is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_steam_root_without_a_readable_library_list_does_not_vouch_for_its_own_library() {
        let scratch = Scratch::new("root-without-list");
        let steam_root = scratch.path().join("steam");
        let game_dir = place_game(&steam_root, "RimWorld", Some(&manifest_naming("RimWorld")));

        assert!(!is_steam_managed_install_under(&game_dir, &[steam_root]));
    }

    #[test]
    fn a_missing_unreadable_or_oversized_library_list_is_not_steam_managed() {
        let scratch = Scratch::new("bad-list");
        let library = scratch.path().join("ExtraLibrary");
        let game_dir = place_game(&library, "RimWorld", Some(&manifest_naming("RimWorld")));
        let listing = libraryfolders_vdf(&[display(&library)]);

        let missing_root = scratch.path().join("missing-root");
        assert!(
            !is_steam_managed_install_under(&game_dir, &[missing_root]),
            "missing"
        );

        let folder_root = scratch.path().join("folder-root");
        std::fs::create_dir_all(folder_root.join("steamapps").join("libraryfolders.vdf"))
            .expect("folder in place of the list");
        assert!(
            !is_steam_managed_install_under(&game_dir, &[folder_root]),
            "not a regular file"
        );

        let oversized_root = scratch.path().join("oversized-root");
        let padding = " ".repeat(STEAM_LIBRARY_LIST_MAX_BYTES as usize);
        write_library_list(&oversized_root, &format!("{listing}{padding}"));
        assert!(
            !is_steam_managed_install_under(&game_dir, &[oversized_root]),
            "oversized"
        );

        let valid_root = scratch.path().join("valid-root");
        write_library_list(&valid_root, &listing);
        assert!(
            is_steam_managed_install_under(&game_dir, &[valid_root]),
            "sanity: the same library is managed once the list is readable"
        );
    }
}
