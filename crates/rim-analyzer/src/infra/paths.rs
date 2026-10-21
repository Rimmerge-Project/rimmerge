//! Default path and game-version resolution for the CLI, kept separate
//! from `main.rs` so it's testable without a real RimWorld install.
//!
//! Install **detection** lives here too ([`default_game_dir_candidates`],
//! [`detect_game_dir`], [`is_game_dir`]), so no machine-specific install path
//! is hardcoded in production code. [`is_game_dir`] is the *only* definition
//! of "this directory is a RimWorld install" in the workspace — detection and
//! the real-install test guards both call it, so the two can never drift.
//!
//! **Platform support.** Windows/MSVC and native Linux/GNU are supported;
//! Linux also resolves Steam/Flatpak libraries and Proton config layouts.
//! See `docs/install.md` for the Linux baseline and support boundaries.
//! macOS candidates only improve discovery diagnostics; macOS is not supported.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::domain::{GamePlatform, GameVersion};

/// The Steam app id for RimWorld — the `workshop/content/<id>` folder
/// name, and the `steamapps` subfolder every Steam library shares.
pub const RIMWORLD_STEAM_APP_ID: &str = "294100";

/// The selected install's runtime platform, regardless of the host OS.
/// As with [`unity_data_dir`], absent Linux data defaults to Windows/Proton.
#[must_use]
pub fn game_platform(game_dir: &Path) -> GamePlatform {
    if game_dir.join("RimWorldLinux_Data").is_dir() {
        GamePlatform::Linux
    } else {
        GamePlatform::Windows
    }
}

/// The install's Unity data folder: `RimWorldLinux_Data` on native Linux,
/// or `RimWorldWin64_Data` on Windows (and Proton/Wine installs on Linux).
///
/// Returns `RimWorldLinux_Data` if it exists as a directory; otherwise defaults to
/// `RimWorldWin64_Data`.
#[must_use]
pub fn unity_data_dir(game_dir: &Path) -> PathBuf {
    match game_platform(game_dir) {
        GamePlatform::Linux => game_dir.join("RimWorldLinux_Data"),
        GamePlatform::Windows => game_dir.join("RimWorldWin64_Data"),
    }
}

/// `<game_dir>/<unity_data>/Managed`: the game's own managed
/// assembly folder. Core/DLC ship no `Assemblies/` folder of their own —
/// their assembly names have to be seeded from here.
#[must_use]
pub fn managed_assemblies_dir(game_dir: &Path) -> PathBuf {
    unity_data_dir(game_dir).join("Managed")
}

/// `<game_dir>/<unity_data>/globalgamemanagers`: Unity's own
/// serialized resource manifest, scanned (never fully parsed) for Core's
/// built-in texture container paths — see
/// `extract::asset_index::resource_container_paths`.
#[must_use]
pub fn global_game_managers_path(game_dir: &Path) -> PathBuf {
    unity_data_dir(game_dir).join("globalgamemanagers")
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

/// The owning Steam library's `workshop/content/294100` directory on Linux,
/// retaining game/library links. Without verified ownership, preserves the
/// lexical `<game_dir>/../../workshop/content/294100` default.
#[must_use]
pub fn default_workshop_dir(game_dir: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Ok(Some(install)) = steam_install_for_game(game_dir) {
        return install
            .steamapps_dir
            .join("workshop/content")
            .join(RIMWORLD_STEAM_APP_ID);
    }
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

/// `%USERPROFILE%\AppData\LocalLow\Ludeon Studios\RimWorld by Ludeon Studios\Config\ModsConfig.xml`
/// on Windows.
#[cfg(windows)]
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

/// The native Unity config under an absolute `$XDG_CONFIG_HOME`, or
/// `$HOME/.config` when HOME is absolute. This context-free helper never
/// selects a Proton prefix.
#[cfg(not(windows))]
pub fn default_mods_config_path() -> Result<PathBuf> {
    native_mods_config_path(path_var("HOME").as_deref(), path_var("XDG_CONFIG_HOME"))
}

/// Resolves the default config for the selected install. On Linux, an
/// unknown or ambiguous runtime context requires an explicit config path.
///
/// # Errors
/// Returns an error when the config is absent or its context cannot be identified.
pub fn mods_config_path_for_install(game_dir: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        mods_config_path_for_install_from(
            game_dir,
            path_var("HOME").as_deref(),
            path_var("XDG_CONFIG_HOME"),
            &default_steam_roots(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = game_dir;
        default_mods_config_path()
    }
}

#[cfg(not(windows))]
fn absolute_home(home: Option<&Path>) -> Option<&Path> {
    home.filter(|path| path.is_absolute())
}

/// The Linux Steam client that owns a registered game installation.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxSteamClient {
    /// Steam installed directly on the host.
    Native,
    /// Steam installed as com.valvesoftware.Steam.
    Flatpak,
}

/// Verified ownership and library location, before resolving game-folder links.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamInstallContext {
    /// The client whose library records identify this installation.
    pub client: LinuxSteamClient,
    /// This installation's steamapps directory, including its Proton prefix.
    pub steamapps_dir: PathBuf,
}

/// Identifies a Linux installation through its given path and resolved target.
///
/// # Errors
/// Returns an error for an inaccessible install or conflicting client/library records.
#[cfg(target_os = "linux")]
pub fn steam_install_for_game(game_dir: &Path) -> Result<Option<SteamInstallContext>> {
    let resolved = game_dir
        .canonicalize()
        .context("selected install cannot be resolved")?;
    steam_install_context_from(
        game_dir,
        &resolved,
        path_var("HOME").as_deref(),
        &default_steam_roots(),
    )
}

#[cfg(target_os = "linux")]
fn steam_install_context_from(
    given: &Path,
    resolved: &Path,
    home: Option<&Path>,
    steam_roots: &[PathBuf],
) -> Result<Option<SteamInstallContext>> {
    let flatpak_root = absolute_home(home)
        .map(|home| home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    let mut candidates = vec![given];
    if given != resolved {
        candidates.push(resolved);
    }
    let mut found: Option<SteamInstallContext> = None;
    for candidate in candidates {
        for root in steam_roots {
            if !is_steam_managed_install_under(candidate, std::slice::from_ref(root)) {
                continue;
            }
            // A shared physical root must not erase the owning client's identity.
            let client = if flatpak_root.as_ref().is_some_and(|flatpak| root == flatpak) {
                LinuxSteamClient::Flatpak
            } else {
                LinuxSteamClient::Native
            };
            // Ownership above verified the steamapps/common/install shape.
            let steamapps_dir = candidate
                .parent()
                .and_then(Path::parent)
                .context("selected install has no Steam library")?
                .to_path_buf();
            if let Some(previous) = &found {
                if previous.client != client
                    || library_key(&previous.steamapps_dir) != library_key(&steamapps_dir)
                {
                    anyhow::bail!(
                        "conflicting Steam client or library records; select paths explicitly"
                    );
                }
            } else {
                found = Some(SteamInstallContext {
                    client,
                    steamapps_dir,
                });
            }
        }
    }
    Ok(found)
}

#[cfg(not(windows))]
fn native_mods_config_path(
    home: Option<&Path>,
    xdg_config_home: Option<PathBuf>,
) -> Result<PathBuf> {
    let base = xdg_config_home
        .filter(|path| path.is_absolute())
        .or_else(|| absolute_home(home).map(|home| home.join(".config")))
        .context("an absolute HOME or XDG_CONFIG_HOME is required")?;
    Ok(base.join("unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml"))
}

#[cfg(target_os = "linux")]
fn mods_config_path_for_install_from(
    game_dir: &Path,
    home: Option<&Path>,
    xdg_config_home: Option<PathBuf>,
    steam_roots: &[PathBuf],
) -> Result<PathBuf> {
    let home = absolute_home(home);
    let given = game_dir;
    let game_dir = game_dir
        .canonicalize()
        .context("selected install cannot be resolved; select ModsConfig.xml explicitly")?;
    let native = game_dir.join("RimWorldLinux_Data").is_dir()
        || ["RimWorldLinux", "RimWorldLinux.x86_64"]
            .iter()
            .any(|name| game_dir.join(name).is_file());
    let windows = game_dir.join("RimWorldWin64_Data").is_dir()
        || game_dir.join("RimWorldWin64.exe").is_file();
    if !is_game_dir(&game_dir) || native == windows {
        anyhow::bail!("unknown or mixed game layout; select ModsConfig.xml explicitly");
    }
    let install = steam_install_context_from(given, &game_dir, home, steam_roots)?;
    if install.is_none()
        && [given, game_dir.as_path()].iter().any(|path| {
            path.parent()
                .is_some_and(|parent| parent.ends_with("steamapps/common"))
        })
    {
        anyhow::bail!(
            "Steam install ownership cannot be verified; select ModsConfig.xml explicitly"
        );
    }
    let config = if windows {
        let install = install.as_ref().context(
            "no registered Steam library for this Proton install; select ModsConfig.xml explicitly",
        )?;
        install.steamapps_dir.join("compatdata").join(RIMWORLD_STEAM_APP_ID).join("pfx/drive_c/users/steamuser/AppData/LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml")
    } else if install
        .as_ref()
        .is_some_and(|install| install.client == LinuxSteamClient::Flatpak)
    {
        home.context("HOME is required for the Flatpak Steam config")?
            .join(".var/app/com.valvesoftware.Steam/.config/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml")
    } else {
        native_mods_config_path(home, xdg_config_home)?
    };
    if !config.is_file() {
        anyhow::bail!(
            "config for the selected install is absent: {}; select ModsConfig.xml explicitly",
            config.display()
        );
    }
    Ok(config)
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
/// `RIMMERGE_PROFILE_DIR`: the base directory where `config.json`, `profiles/`,
/// `databases/` and app-global settings live. Overrides the platform default.
pub const PROFILE_DIR_VAR: &str = "RIMMERGE_PROFILE_DIR";

/// The base directory for Rimmerge's own files (`config.json`, `profiles/`,
/// `databases/`):
///
/// 1. `RIMMERGE_PROFILE_DIR` when set and non-empty.
/// 2. On Windows: `%LOCALAPPDATA%\rimmerge`.
/// 3. On Unix/Linux: `$XDG_DATA_HOME/rimmerge`, falling back to `$HOME/.local/share/rimmerge`.
///
/// Returns `None` when neither the override nor the platform data directory
/// environment variable is set.
#[must_use]
pub fn default_profile_base_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        default_profile_base_dir_from(
            path_var(PROFILE_DIR_VAR),
            path_var("XDG_DATA_HOME"),
            path_var("HOME"),
            path_var("LOCALAPPDATA"),
        )
    }

    #[cfg(not(windows))]
    {
        default_profile_base_dir_from(
            path_var(PROFILE_DIR_VAR),
            path_var("XDG_DATA_HOME"),
            path_var("HOME"),
            None,
        )
    }
}

fn default_profile_base_dir_from(
    profile_override: Option<PathBuf>,
    xdg_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
) -> Option<PathBuf> {
    profile_override.or_else(|| {
        #[cfg(windows)]
        {
            let _ = (xdg_data_home, home);
            local_app_data.map(|local| local.join("rimmerge"))
        }

        #[cfg(not(windows))]
        {
            let _ = local_app_data;
            xdg_data_home
                .filter(|path| path.is_absolute())
                .map(|xdg| xdg.join("rimmerge"))
                .or_else(|| {
                    absolute_home(home.as_deref())
                        .map(|home| home.join(".local").join("share").join("rimmerge"))
                })
        }
    })
}

/// The default database folder for RimSort rules (`RimSort/dbs`):
///
/// - On Windows: `%LOCALAPPDATA%\RimSort\dbs`
/// - On Unix/Linux: `$XDG_DATA_HOME/RimSort/dbs` or `$HOME/.local/share/RimSort/dbs`
///
/// Returns `None` if the platform environment variable is unset.
#[must_use]
pub fn default_rimsort_dbs_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        default_rimsort_dbs_dir_from(
            path_var("XDG_DATA_HOME"),
            path_var("HOME"),
            path_var("LOCALAPPDATA"),
        )
    }

    #[cfg(not(windows))]
    {
        default_rimsort_dbs_dir_from(path_var("XDG_DATA_HOME"), path_var("HOME"), None)
    }
}

fn default_rimsort_dbs_dir_from(
    xdg_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let _ = (xdg_data_home, home);
        local_app_data.map(|local| local.join("RimSort").join("dbs"))
    }

    #[cfg(not(windows))]
    {
        let _ = local_app_data;
        xdg_data_home
            .filter(|path| path.is_absolute())
            .map(|xdg| xdg.join("RimSort").join("dbs"))
            .or_else(|| {
                absolute_home(home.as_deref()).map(|home| {
                    home.join(".local")
                        .join("share")
                        .join("RimSort")
                        .join("dbs")
                })
            })
    }
}

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
/// ones [`detect_game_dir`] uses (`%ProgramFiles(x86)%\Steam`,
/// `%ProgramFiles%\Steam`); a Steam installed elsewhere is not found and
/// degrades to `false`, like everything else that can't be read.
///
/// `steam_appid.txt` is deliberately not consulted: every Steam copy has one,
/// so a copy taken out of Steam would pass. Windows names compare ASCII
/// case-insensitively; Unix names retain case. Windows library paths compare
/// lexically (`library_key`) to preserve Steam's spelling rather than resolve
/// junctions or add an extended path prefix. Unix library paths resolve existing
/// aliases while preserving case and literal backslashes. The manifest is read
/// with a 64 KiB bound and each
/// `libraryfolders.vdf` with a 1 MiB one; a missing, non-regular, oversized,
/// or unparseable file means `false`, never an error.
#[must_use]
pub fn is_steam_managed_install(game_dir: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        steam_install_for_game(game_dir).is_ok_and(|install| install.is_some())
    }
    #[cfg(not(target_os = "linux"))]
    {
        is_steam_managed_install_under(game_dir, &default_steam_roots())
    }
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
            .is_some_and(|name| path_name_matches(name, expected))
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
        .is_some_and(|install_dir| path_name_matches(&install_dir, folder_name));
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

fn path_name_matches(actual: &str, expected: &str) -> bool {
    #[cfg(windows)]
    {
        actual.eq_ignore_ascii_case(expected)
    }
    #[cfg(not(windows))]
    {
        actual == expected
    }
}

/// Keeps the existing Windows lexical comparison, including its case folding.
#[cfg(windows)]
fn library_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// Resolves Unix aliases without folding case or interpreting backslashes.
#[cfg(not(windows))]
fn library_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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

/// The Steam roots this OS puts Steam in by default, most likely first.
/// Reads environment variables only; none is known to exist.
#[cfg(windows)]
fn default_steam_roots() -> Vec<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|program_files| PathBuf::from(program_files).join("Steam"))
        .collect()
}

#[cfg(windows)]
fn platform_candidates() -> Vec<PathBuf> {
    let steam_roots = default_steam_roots();

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
fn default_steam_roots() -> Vec<PathBuf> {
    absolute_home(path_var("HOME").as_deref())
        .map(|home| {
            home.join("Library")
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
    steam_roots_from_home(path_var("HOME").as_deref())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn steam_roots_from_home(home: Option<&Path>) -> Vec<PathBuf> {
    let Some(home) = absolute_home(home) else {
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

    #[test]
    fn default_workshop_dir_is_a_sibling_of_common() {
        let game_dir = Path::new("Steam")
            .join("steamapps")
            .join("common")
            .join("RimWorld");
        let workshop = default_workshop_dir(&game_dir);
        assert_eq!(
            workshop,
            Path::new("Steam")
                .join("steamapps")
                .join("workshop")
                .join("content")
                .join("294100")
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
            rimworld_under_steam_library(Path::new("Steam")),
            Path::new("Steam")
                .join("steamapps")
                .join("common")
                .join("RimWorld")
        );
    }

    #[test]
    fn unity_data_dir_detects_linux_folder_when_present() {
        let scratch = Scratch::new("unity-linux");
        let game_dir = scratch.path();
        std::fs::create_dir_all(game_dir.join("RimWorldLinux_Data")).expect("create linux data");
        assert_eq!(
            unity_data_dir(game_dir),
            game_dir.join("RimWorldLinux_Data")
        );
        assert_eq!(
            managed_assemblies_dir(game_dir),
            game_dir.join("RimWorldLinux_Data").join("Managed")
        );
        assert_eq!(
            global_game_managers_path(game_dir),
            game_dir
                .join("RimWorldLinux_Data")
                .join("globalgamemanagers")
        );
    }

    #[test]
    fn unity_data_dir_falls_back_to_win64_when_linux_folder_absent() {
        let scratch = Scratch::new("unity-win");
        let game_dir = scratch.path();
        assert_eq!(
            unity_data_dir(game_dir),
            game_dir.join("RimWorldWin64_Data")
        );
        assert_eq!(
            managed_assemblies_dir(game_dir),
            game_dir.join("RimWorldWin64_Data").join("Managed")
        );
        assert_eq!(
            global_game_managers_path(game_dir),
            game_dir
                .join("RimWorldWin64_Data")
                .join("globalgamemanagers")
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
        assert_eq!(PROFILE_DIR_VAR, "RIMMERGE_PROFILE_DIR");
    }

    #[cfg(windows)]
    #[test]
    fn windows_data_defaults_use_local_app_data_and_preserve_override_precedence() {
        let local = PathBuf::from(r"C:\Synthetic\Local");
        for (override_dir, local_app_data, expected) in [
            (
                Some(PathBuf::from("explicit")),
                Some(local.clone()),
                Some(PathBuf::from("explicit")),
            ),
            (None, Some(local.clone()), Some(local.join("rimmerge"))),
            (None, None, None),
        ] {
            assert_eq!(
                default_profile_base_dir_from(
                    override_dir,
                    Some(PathBuf::from("ignored-xdg")),
                    Some(PathBuf::from("ignored-home")),
                    local_app_data
                ),
                expected
            );
        }
        assert_eq!(
            default_rimsort_dbs_dir_from(None, None, Some(local.clone())),
            Some(local.join("RimSort/dbs"))
        );
        assert_eq!(
            default_rimsort_dbs_dir_from(
                Some(PathBuf::from("ignored-xdg")),
                Some(PathBuf::from("ignored-home")),
                None
            ),
            None
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn invalid_home_never_produces_relative_defaults() {
        let xdg = PathBuf::from("/synthetic-xdg");
        for home in [None, Some(""), Some("."), Some("relative/home")] {
            let home = home.map(PathBuf::from);
            assert!(native_mods_config_path(home.as_deref(), None).is_err());
            assert_eq!(
                default_profile_base_dir_from(None, None, home.clone(), None),
                None
            );
            assert_eq!(default_rimsort_dbs_dir_from(None, home.clone(), None), None);
            assert_eq!(
                native_mods_config_path(home.as_deref(), Some(xdg.clone())).expect("XDG config"),
                xdg.join("unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml")
            );
            assert_eq!(
                default_profile_base_dir_from(None, Some(xdg.clone()), home.clone(), None),
                Some(xdg.join("rimmerge"))
            );
            assert_eq!(
                default_rimsort_dbs_dir_from(Some(xdg.clone()), home.clone(), None),
                Some(xdg.join("RimSort/dbs"))
            );
            assert_eq!(
                default_profile_base_dir_from(Some(PathBuf::from("explicit")), None, home, None),
                Some(PathBuf::from("explicit"))
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_roots_require_an_absolute_home() {
        for home in [None, Some(""), Some("."), Some("relative/home")] {
            assert!(steam_roots_from_home(home.map(Path::new)).is_empty());
        }
        let home = Path::new("/synthetic-home");
        assert_eq!(
            steam_roots_from_home(Some(home)),
            vec![
                home.join(".steam/steam"),
                home.join(".local/share/Steam"),
                home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn relative_or_empty_xdg_data_is_ignored_without_restricting_explicit_overrides() {
        let home = PathBuf::from("/synthetic-home");
        for xdg in [None, Some(PathBuf::new()), Some(PathBuf::from("relative"))] {
            assert_eq!(
                default_profile_base_dir_from(None, xdg.clone(), Some(home.clone()), None),
                Some(home.join(".local/share/rimmerge"))
            );
            assert_eq!(
                default_rimsort_dbs_dir_from(xdg.clone(), Some(home.clone()), None),
                Some(home.join(".local/share/RimSort/dbs"))
            );
            assert_eq!(
                default_profile_base_dir_from(None, xdg.clone(), None, None),
                None
            );
            assert_eq!(default_rimsort_dbs_dir_from(xdg.clone(), None, None), None);
            assert_eq!(
                default_profile_base_dir_from(Some(PathBuf::from("explicit")), xdg, None, None),
                Some(PathBuf::from("explicit"))
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_profile_base_prefers_override_then_xdg_then_home() {
        let override_dir = PathBuf::from("/override");
        let xdg_dir = PathBuf::from("/xdg");
        let home_dir = PathBuf::from("/home/tester");

        assert_eq!(
            default_profile_base_dir_from(
                Some(override_dir.clone()),
                Some(xdg_dir.clone()),
                Some(home_dir.clone()),
                None,
            ),
            Some(override_dir)
        );
        assert_eq!(
            default_profile_base_dir_from(
                None,
                Some(xdg_dir.clone()),
                Some(home_dir.clone()),
                None
            ),
            Some(xdg_dir.join("rimmerge"))
        );
        assert_eq!(
            default_profile_base_dir_from(None, None, Some(home_dir.clone()), None),
            Some(home_dir.join(".local").join("share").join("rimmerge"))
        );
        assert_eq!(default_profile_base_dir_from(None, None, None, None), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_rimsort_dbs_prefers_xdg_then_home() {
        let xdg_dir = PathBuf::from("/xdg");
        let home_dir = PathBuf::from("/home/tester");

        assert_eq!(
            default_rimsort_dbs_dir_from(Some(xdg_dir.clone()), Some(home_dir.clone()), None),
            Some(xdg_dir.join("RimSort").join("dbs"))
        );
        assert_eq!(
            default_rimsort_dbs_dir_from(None, Some(home_dir.clone()), None),
            Some(
                home_dir
                    .join(".local")
                    .join("share")
                    .join("RimSort")
                    .join("dbs")
            )
        );
        assert_eq!(default_rimsort_dbs_dir_from(None, None, None), None);
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

    #[cfg(windows)]
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

    #[cfg(windows)]
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
    #[cfg(target_os = "linux")]
    #[test]
    fn steam_context_keeps_linked_flatpak_library_and_rejects_conflicting_targets() {
        let scratch = Scratch::new("linked-context");
        let home = scratch.path().join("home");
        let steam = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        let library = scratch.path().join("secondary");
        write_library_list(&steam, &libraryfolders_vdf(&[display(&library)]));
        let game = place_game(&library, "RimWorld", Some(&manifest_naming("RimWorld")));
        std::fs::create_dir_all(game.join("Data/Core")).expect("core");
        std::fs::create_dir_all(game.join("RimWorldLinux_Data")).expect("native runtime");
        std::fs::write(game.join("Version.txt"), "1.6.0").expect("version");
        let flatpak = home.join(".var/app/com.valvesoftware.Steam/.config/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml");
        write_config(&native_mods_config_path(Some(&home), None).expect("native decoy"));
        write_config(&flatpak);
        let outside_alias = scratch.path().join("outside-alias");
        std::os::unix::fs::symlink(&game, &outside_alias).expect("outside alias");
        let roots = std::slice::from_ref(&steam);
        let expected_library = library.join("steamapps");
        let context = steam_install_context_from(&outside_alias, &game, Some(&home), roots)
            .expect("context")
            .expect("Steam");
        assert_eq!(context.client, LinuxSteamClient::Flatpak);
        assert_eq!(context.steamapps_dir, expected_library);
        assert_eq!(
            mods_config_path_for_install_from(&outside_alias, Some(&home), None, roots)
                .expect("outside alias config"),
            flatpak
        );
        let external = scratch.path().join("external-game");
        std::fs::rename(&game, &external).expect("move runtime outside Steam");
        std::os::unix::fs::symlink(&external, &game).expect("Steam game link");
        let context = steam_install_context_from(&game, &external, Some(&home), roots)
            .expect("linked context")
            .expect("Steam");
        assert_eq!(context.steamapps_dir, expected_library);
        assert_eq!(
            mods_config_path_for_install_from(&game, Some(&home), None, roots)
                .expect("linked config"),
            flatpak
        );
        assert_eq!(
            context
                .steamapps_dir
                .join("workshop/content")
                .join(RIMWORLD_STEAM_APP_ID),
            library.join("steamapps/workshop/content/294100")
        );
        let library_alias = scratch.path().join("library-alias");
        std::os::unix::fs::symlink(&library, &library_alias).expect("library link");
        let through_library = library_alias.join("steamapps/common/RimWorld");
        let context = steam_install_context_from(&through_library, &external, Some(&home), roots)
            .expect("linked library context")
            .expect("Steam");
        assert_eq!(
            context
                .steamapps_dir
                .canonicalize()
                .expect("library target"),
            expected_library.canonicalize().expect("registered library")
        );
        assert_eq!(
            mods_config_path_for_install_from(&through_library, Some(&home), None, roots)
                .expect("linked library config"),
            flatpak
        );
        std::fs::remove_dir(external.join("RimWorldLinux_Data")).expect("remove Linux runtime");
        std::fs::create_dir(external.join("RimWorldWin64_Data")).expect("Proton runtime");
        let proton = library.join("steamapps/compatdata/294100/pfx/drive_c/users/steamuser/AppData/LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml");
        write_config(&proton);
        assert_eq!(
            mods_config_path_for_install_from(&game, Some(&home), None, roots)
                .expect("linked Proton prefix"),
            proton
        );
        let native = home.join(".local/share/Steam");
        write_library_list(&native, &libraryfolders_vdf(&[]));
        let other = place_game(&native, "RimWorld", Some(&manifest_naming("RimWorld")));
        std::fs::remove_file(&game).expect("remove game link");
        std::os::unix::fs::symlink(&other, &game).expect("conflicting target");
        std::fs::create_dir_all(other.join("Data/Core")).expect("other core");
        std::fs::create_dir(other.join("RimWorldLinux_Data")).expect("other native runtime");
        std::fs::write(other.join("Version.txt"), "1.6.0").expect("other version");
        let roots = [steam, native];
        assert!(steam_install_context_from(&game, &other, Some(&home), &roots).is_err());
        let error = mods_config_path_for_install_from(&game, Some(&home), None, &roots)
            .expect_err("conflict must not select the native decoy");
        assert!(error.to_string().contains("conflicting Steam"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn shared_native_and_flatpak_roots_require_explicit_context() {
        for native_is_link in [false, true] {
            let scratch = Scratch::new(if native_is_link {
                "native-root-link"
            } else {
                "flatpak-root-link"
            });
            let home = scratch.path().join("home");
            let native = home.join(".local/share/Steam");
            let flatpak = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
            let (physical, alias) = if native_is_link {
                (&flatpak, &native)
            } else {
                (&native, &flatpak)
            };
            write_library_list(physical, &libraryfolders_vdf(&[]));
            let game = place_game(physical, "RimWorld", Some(&manifest_naming("RimWorld")));
            std::fs::create_dir_all(game.join("Data/Core")).expect("core");
            std::fs::create_dir(game.join("RimWorldLinux_Data")).expect("native runtime");
            std::fs::write(game.join("Version.txt"), "1.6.0").expect("version");
            std::fs::create_dir_all(alias.parent().expect("root parent")).expect("parent");
            std::os::unix::fs::symlink(physical, alias).expect("shared Steam root");
            write_config(&native_mods_config_path(Some(&home), None).expect("native config"));
            write_config(&home.join(".var/app/com.valvesoftware.Steam/.config/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml"));
            let roots = steam_roots_from_home(Some(&home));
            for root in [&native, &flatpak] {
                let given = root.join("steamapps/common/RimWorld");
                let resolved = given.canonicalize().expect("resolved game");
                let error = steam_install_context_from(&given, &resolved, Some(&home), &roots)
                    .expect_err("shared roots must not choose a client");
                assert!(error.to_string().contains("conflicting Steam"));
                let error = mods_config_path_for_install_from(&given, Some(&home), None, &roots)
                    .expect_err("shared roots must not choose either existing config");
                assert!(error.to_string().contains("conflicting Steam"));
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn separate_native_and_flatpak_clients_keep_their_own_routes() {
        let scratch = Scratch::new("two-steam-clients");
        let home = scratch.path().join("home");
        let native = home.join(".local/share/Steam");
        let flatpak = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        let roots = [native.clone(), flatpak.clone()];
        for (root, client) in [
            (&native, LinuxSteamClient::Native),
            (&flatpak, LinuxSteamClient::Flatpak),
        ] {
            write_library_list(root, &libraryfolders_vdf(&[]));
            let game = place_game(root, "RimWorld", Some(&manifest_naming("RimWorld")));
            let context = steam_install_context_from(&game, &game, Some(&home), &roots)
                .expect("unambiguous")
                .expect("Steam");
            assert_eq!(context.client, client);
            assert_eq!(context.steamapps_dir, root.join("steamapps"));
        }
        write_library_list(&flatpak, &libraryfolders_vdf(&[display(&native)]));
        let shared = native.join("steamapps/common/RimWorld");
        assert!(steam_install_context_from(&shared, &shared, Some(&home), &roots).is_err());
    }

    #[cfg(target_os = "linux")]
    fn write_config(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("config dir");
        std::fs::write(path, "<ModsConfigData/>").expect("synthetic config");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn selected_native_install_honors_xdg_and_never_falls_back_to_another_config() {
        let scratch = Scratch::new("native-config");
        let home = scratch.path().join("home");
        let xdg = scratch.path().join("xdg");
        let game = scratch.path().join("game");
        std::fs::create_dir_all(game.join("Data/Core")).expect("core");
        std::fs::create_dir_all(game.join("RimWorldLinux_Data")).expect("native data");
        std::fs::write(game.join("Version.txt"), "1.6.0").expect("version");
        let old = native_mods_config_path(Some(&home), None).expect("HOME config");
        let expected =
            native_mods_config_path(None, Some(xdg.clone())).expect("XDG config without HOME");
        write_config(&old);
        assert!(
            mods_config_path_for_install_from(&game, Some(&home), Some(xdg.clone()), &[]).is_err()
        );
        write_config(&expected);
        assert_eq!(
            mods_config_path_for_install_from(&game, None, Some(xdg.clone()), &[])
                .expect("native XDG"),
            expected
        );
        for home in [None, Some(""), Some("."), Some("relative/home")] {
            let home = home.map(Path::new);
            assert!(mods_config_path_for_install_from(&game, home, None, &[]).is_err());
            assert_eq!(
                mods_config_path_for_install_from(&game, home, Some(xdg.clone()), &[])
                    .expect("absolute XDG config with invalid HOME"),
                expected
            );
        }
        for xdg in [None, Some(PathBuf::new()), Some(PathBuf::from("relative"))] {
            assert_eq!(
                mods_config_path_for_install_from(&game, Some(&home), xdg, &[])
                    .expect("HOME fallback"),
                old
            );
        }
        assert!(mods_config_path_for_install_from(&game, None, None, &[]).is_err());
        std::fs::create_dir_all(game.join("RimWorldWin64_Data")).expect("mixed data");
        assert!(mods_config_path_for_install_from(&game, Some(&home), None, &[]).is_err());
        std::fs::remove_dir(game.join("RimWorldWin64_Data")).expect("remove Windows data");
        std::fs::write(game.join("RimWorldWin64.exe"), "synthetic").expect("mixed executable");
        assert!(mods_config_path_for_install_from(&game, Some(&home), None, &[]).is_err());
        assert!(
            mods_config_path_for_install_from(
                &scratch.path().join("absent"),
                Some(&home),
                None,
                &[]
            )
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn selected_flatpak_and_proton_install_use_only_their_own_configuration() {
        let scratch = Scratch::new("context-config");
        let home = scratch.path().join("home");
        let steam = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        let secondary = scratch.path().join("secondary");
        write_library_list(
            &steam,
            &libraryfolders_vdf(&[display(&secondary), display(&secondary)]),
        );
        let game = place_game(&secondary, "RimWorld", Some(&manifest_naming("RimWorld")));
        std::fs::create_dir_all(game.join("Data/Core")).expect("core");
        std::fs::create_dir_all(game.join("RimWorldLinux_Data")).expect("native data");
        std::fs::write(game.join("Version.txt"), "1.6.0").expect("version");
        let native = native_mods_config_path(Some(&home), None).expect("native");
        let flatpak = home.join(".var/app/com.valvesoftware.Steam/.config/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml");
        let proton_suffix = "steamapps/compatdata/294100/pfx/drive_c/users/steamuser/AppData/LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml";
        let proton = secondary.join(proton_suffix);
        let wrong_proton = steam.join(proton_suffix);
        write_config(&native);
        write_config(&wrong_proton);
        assert!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .is_err()
        );
        let manifest = secondary.join("steamapps/appmanifest_294100.acf");
        std::fs::remove_file(&manifest).expect("remove Steam ownership record");
        assert!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .is_err()
        );
        std::fs::write(&manifest, manifest_naming("RimWorld")).expect("restore manifest");
        write_config(&flatpak);
        assert_eq!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .expect("Flatpak"),
            flatpak
        );
        let alias = scratch.path().join("aliased-install");
        std::os::unix::fs::symlink(&game, &alias).expect("install alias");
        assert_eq!(
            mods_config_path_for_install_from(
                &alias,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .expect("aliased Flatpak install"),
            flatpak
        );
        let native_root = home.join(".local/share/Steam");
        write_library_list(&native_root, &libraryfolders_vdf(&[display(&secondary)]));
        assert!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                &[steam.clone(), native_root]
            )
            .is_err()
        );
        std::fs::remove_dir(game.join("RimWorldLinux_Data")).expect("remove native data");
        std::fs::create_dir_all(game.join("RimWorldWin64_Data")).expect("Proton data");
        assert!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .is_err()
        );
        write_config(&proton);
        assert_eq!(
            mods_config_path_for_install_from(
                &game,
                Some(&home),
                None,
                std::slice::from_ref(&steam)
            )
            .expect("selected Proton library"),
            proton
        );
        assert!(mods_config_path_for_install_from(&game, Some(&home), None, &[]).is_err());
        std::fs::remove_file(&proton).expect("remove config");
        std::fs::create_dir(&proton).expect("directory instead of file");
        assert!(mods_config_path_for_install_from(&game, Some(&home), None, &[steam]).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_library_and_manifest_matching_preserve_case_and_unix_separators() {
        let (_scratch, steam, game) =
            steam_layout("unix-case", "RimWorld", Some(&manifest_naming("RimWorld")));
        assert_ne!(
            library_key(&steam),
            library_key(&PathBuf::from(display(&steam).to_uppercase()))
        );
        std::fs::write(
            steam.join("steamapps/appmanifest_294100.acf"),
            manifest_naming("rimworld"),
        )
        .expect("wrong-case manifest");
        assert!(!is_steam_managed_install_under(&game, &[steam]));
        let scratch = Scratch::new("unix-library-case");
        let library = scratch.path().join("Library");
        let game = place_game(&library, "RimWorld", Some(&manifest_naming("RimWorld")));
        let root = scratch.path().join("steam");
        for spelling in [
            display(&library).to_uppercase(),
            display(&library).replace('/', "\\"),
        ] {
            write_library_list(&root, &libraryfolders_vdf(&[spelling]));
            assert!(!is_steam_managed_install_under(
                &game,
                std::slice::from_ref(&root)
            ));
        }
        write_library_list(&root, &libraryfolders_vdf(&[display(&library)]));
        assert!(is_steam_managed_install_under(&game, &[root]));
    }
}
