//! Filesystem discovery and parallel scanning: turns a game install, a
//! workshop folder, and `ModsConfig.xml` into
//! [`ScannedMod`]s the analysis layer can
//! build a [`Report`](crate::domain::Report) from.

mod discovery;
mod explain_dangling;
mod mod_scan;
pub mod paths;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use anyhow::Context;
use rayon::prelude::*;

use crate::domain::{
    FolderPolicy, GameVersion, InactiveMod, LoadOrder, ModId, RefSite, ScannedMod, Warning,
};
use crate::extract::about_xml::{self, AboutDetails, AboutXmlError};
use crate::extract::mods_config;

use discovery::{Discovered, DiscoveredMod};

// `ScanOutput`/`ScanProgress`/`ScanStage` all live in `domain::scan` (plain
// data, per the hexagonal layering contract) — re-exported here so
// downstream code can keep writing `infra::ScanOutput` etc.
pub use crate::domain::{ScanOutput, ScanProgress, ScanStage};

// The one piece of `mod_scan`'s own file-walking logic an outside crate
// needs directly: `rim-io`'s asset locator walks a `Textures/` directory
// the same way this crate's own `Defs/`/`Patches/` scan does (see that
// function's own doc comment for why).
pub use mod_scan::{ReadError, engine_enumeration_order, read_bounded_with_limit};

pub use explain_dangling::{build_explained_report, explain_dangling_references};

/// Where to scan and which game version to resolve `ByVersion`/
/// `LoadFolders.xml` entries against.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub game_dir: PathBuf,
    pub workshop_dir: PathBuf,
    pub mods_config_path: PathBuf,
    pub game_version: GameVersion,
    pub folder_policy: FolderPolicy,
    /// When `Some`, this exact list is scanned as "active", in the order
    /// given, instead of `mods_config_path`'s own `<activeMods>` — the file
    /// still must exist (`scan_with_progress` still `ensure!`s it, so
    /// `Report.metadata.mods_config` stays truthful about what was read), but
    /// its active-mod list is never consulted. `None` reads the file's own
    /// list.
    pub active_mods: Option<Vec<ModId>>,
}

/// Reads `ModsConfig.xml`, discovers every mod directory, and scans the
/// active ones in parallel.
///
/// Fails fast when `game_dir` or `mods_config_path` doesn't exist — every
/// other missing/malformed input degrades to a [`Warning`] instead.
///
/// A thin wrapper over [`scan_with_progress`] with a no-op hook — see
/// there for progress reporting.
pub fn scan(config: &ScanConfig) -> anyhow::Result<ScanOutput> {
    scan_with_progress(config, &mut |_| {})
}

/// Same as [`scan`], but calls `on_progress` as work proceeds: once before
/// and once after discovery, once per mod scanned, and once before and
/// after the trailing managed-assembly lookup. See [`ScanProgress`] for
/// how to interpret `done`/`total` across stages.
///
/// `on_progress` is always called from the thread that called
/// `scan_with_progress` — never from a rayon worker thread — so it is
/// free to touch non-`Sync` state (a `RefCell`, an unlocked UI handle)
/// without synchronization.
pub fn scan_with_progress(
    config: &ScanConfig,
    on_progress: &mut dyn FnMut(ScanProgress),
) -> anyhow::Result<ScanOutput> {
    anyhow::ensure!(
        config.game_dir.is_dir(),
        "game_dir does not exist or is not a directory: {}",
        config.game_dir.display()
    );
    anyhow::ensure!(
        config.mods_config_path.is_file(),
        "mods_config does not exist or is not a file: {}",
        config.mods_config_path.display()
    );

    let active_ids = match &config.active_mods {
        Some(active_mods) => active_mods.clone(),
        None => read_active_mods(&config.mods_config_path)?,
    };
    // The bare packageId of every active mod, `_steam` suffix stripped —
    // `LoadFolders.xml`'s `IfModActive`/`IfModNotActive` name mods this
    // way (see `ModId::base`), never with the suffix ModsConfig.xml may add.
    let active_bases: HashSet<ModId> = active_ids.iter().map(ModId::base).collect();

    on_progress(ScanProgress {
        stage: ScanStage::Discovering,
        done: 0,
        total: 1,
    });
    let (discovered, mut warnings) =
        discovery::discover(&config.game_dir, &config.workshop_dir, config.game_version);
    warn_missing_scan_roots(config, &mut warnings);
    on_progress(ScanProgress {
        stage: ScanStage::Discovering,
        done: 1,
        total: 1,
    });

    let (to_scan, missing_mods) = resolve_active_mods(&active_ids, &discovered);
    // Computed from `discovered` before it's consumed (dropped) at the end of
    // this function's scope — everything else below only ever borrows it
    // through `to_scan`/`missing_mods`, never moves it, so this can run any
    // time before then.
    //
    // `inactive_excluding` takes `to_scan` itself (the already-resolved `(id,
    // DiscoveredMod)` pairs), not `active_ids` — see that method's own doc
    // comment for why re-deriving "is this id active" from a bare `ModId`
    // list is unsound.
    let discovered_mod_count = discovered.len();
    let inactive_mods = discovered.inactive_excluding(&to_scan);
    let (scanned_mods, child_value_hashes_by_mod, ref_sites_by_mod, scan_warnings) =
        scan_all_with_progress(
            &to_scan,
            config.game_version,
            &active_bases,
            config.folder_policy,
            on_progress,
        );
    warnings.extend(scan_warnings);

    on_progress(ScanProgress {
        stage: ScanStage::Analyzing,
        done: 0,
        total: 1,
    });
    let vanilla_assembly_names = managed_assembly_names(&config.game_dir, &mut warnings);
    let vanilla_type_hierarchy = vanilla_type_hierarchy(&config.game_dir, &mut warnings);
    let core_resource_textures = core_resource_textures(&config.game_dir, &mut warnings);
    on_progress(ScanProgress {
        stage: ScanStage::Analyzing,
        done: 1,
        total: 1,
    });
    let load_order = LoadOrder::new(active_ids);

    Ok(ScanOutput {
        scanned_mods,
        load_order,
        missing_mods,
        vanilla_assembly_names,
        vanilla_type_hierarchy,
        warnings,
        child_value_hashes_by_mod,
        inactive_mods,
        discovered_mod_count,
        core_resource_textures,
        ref_sites_by_mod,
    })
}

/// [`inventory`]'s own result: discovery-only, no defs/patches/assemblies
/// scan for anything — `mods list|activate| deactivate` need identity and
/// declared dependencies, not a full scan, and discovery already has both for
/// every mod on disk regardless of whether it's active.
#[derive(Debug, Clone)]
pub struct ModInventoryOutput {
    /// `ModsConfig.xml`'s own active list, or `config.active_mods` when
    /// given — in file/override order, unchanged from [`ScanConfig`].
    pub active: Vec<ModId>,
    /// Every mod discovery found on disk, active or not, shaped as
    /// [`InactiveMod`] (identity and declared order only — see that
    /// type's own doc comment for why nothing else is read here).
    pub discovered: Vec<InactiveMod>,
    /// Active-list entries discovery found no directory for.
    pub missing: Vec<ModId>,
}

/// Discovers every mod on disk and reads the active list, without scanning
/// any mod's defs, patches, assemblies, or textures — seconds, not the
/// ~30 s a full [`scan_with_progress`] takes. `config.folder_policy` is
/// ignored: nothing here resolves load folders.
///
/// # Errors
///
/// Same preconditions as [`scan_with_progress`]: `game_dir` must be a
/// directory and `mods_config_path` must be a file.
pub fn inventory(config: &ScanConfig) -> anyhow::Result<ModInventoryOutput> {
    anyhow::ensure!(
        config.game_dir.is_dir(),
        "game_dir does not exist or is not a directory: {}",
        config.game_dir.display()
    );
    anyhow::ensure!(
        config.mods_config_path.is_file(),
        "mods_config does not exist or is not a file: {}",
        config.mods_config_path.display()
    );

    let active_ids = match &config.active_mods {
        Some(active_mods) => active_mods.clone(),
        None => read_active_mods(&config.mods_config_path)?,
    };
    let (discovered, _warnings) =
        discovery::discover(&config.game_dir, &config.workshop_dir, config.game_version);
    let (to_scan, missing) = resolve_active_mods(&active_ids, &discovered);

    // Built the same way `Report.mods ∪ Report.inactive_mods` is: an active
    // entry is keyed by the exact id `to_scan` already resolved it under —
    // `ModsConfig.xml`'s own text, `x.mod_steam` included — never by
    // re-deriving one from the folder alone. `Discovered::all()` (used by
    // `inactive_excluding`, unaffected) assigns a Workshop-only mod's id via
    // `active_mods_id`, which only adds the `_steam` suffix when a *primary*
    // copy also shadows it — for a Workshop-only mod active as `x.mod_steam`,
    // that re-derivation returns the bare `x.mod`, so `discovered.all()` here
    // would key this exact mod wrong: listed under `x.mod` (as if inactive)
    // even though `to_scan` knows it's active under `x.mod_steam`.
    // `inactive_excluding(&to_scan)` itself is unaffected — it matches by
    // path, not id.
    let mut discovered_mods: Vec<InactiveMod> = to_scan
        .iter()
        .map(|&(id, discovered_mod)| discovery::to_inactive_mod(id.clone(), discovered_mod))
        .collect();
    discovered_mods.extend(discovered.inactive_excluding(&to_scan));

    Ok(ModInventoryOutput {
        active: active_ids,
        discovered: discovered_mods,
        missing,
    })
}

/// Reads and parses the active-mod list out of `ModsConfig.xml`.
fn read_active_mods(mods_config_path: &Path) -> anyhow::Result<Vec<ModId>> {
    let bytes = std::fs::read(mods_config_path)
        .with_context(|| format!("reading ModsConfig.xml at {}", mods_config_path.display()))?;
    mods_config::parse_active_mods(&bytes)
        .with_context(|| format!("parsing ModsConfig.xml at {}", mods_config_path.display()))
}

/// Warns (non-fatally — an empty `workshop_dir`/`Mods/` is a normal setup,
/// not an error) when either optional scan root is missing.
fn warn_missing_scan_roots(config: &ScanConfig, warnings: &mut Vec<Warning>) {
    if !config.workshop_dir.is_dir() {
        warnings.push(Warning::new(
            None,
            format!(
                "workshop_dir does not exist: {}",
                config.workshop_dir.display()
            ),
        ));
    }
    let mods_dir = config.game_dir.join("Mods");
    if !mods_dir.is_dir() {
        warnings.push(Warning::new(
            None,
            format!(
                "Mods/ does not exist under game_dir: {}",
                mods_dir.display()
            ),
        ));
    }
}

/// Resolves each `ModsConfig.xml` active-mod entry — in its on-disk
/// order — to the directory [`discovery::discover`] found for it. An
/// entry with no directory on disk is collected as a missing mod instead.
fn resolve_active_mods<'a>(
    active_ids: &'a [ModId],
    discovered: &'a Discovered,
) -> (Vec<(&'a ModId, &'a DiscoveredMod)>, Vec<ModId>) {
    let mut to_scan = Vec::with_capacity(active_ids.len());
    let mut missing_mods = Vec::new();
    for id in active_ids {
        match discovered.lookup(id) {
            Some(m) => to_scan.push((id, m)),
            None => missing_mods.push(id.clone()),
        }
    }
    (to_scan, missing_mods)
}

/// Scans every resolved active mod in parallel, collecting the
/// [`ScannedMod`]s (in `to_scan`'s order — [`rayon`]'s `collect` on an
/// indexed parallel iterator preserves input order regardless of which
/// worker finishes first, which is what keeps a scan's JSON output
/// deterministic across runs), every mod's own
/// [`crate::domain::ScanOutput::child_value_hashes_by_mod`] and
/// [`crate::domain::ScanOutput::ref_sites_by_mod`] contributions, and
/// every warning raised while extracting them. Calls `on_progress` once
/// per mod, from this function's own caller thread only — see
/// [`scan_with_progress`]'s doc comment.
#[allow(clippy::type_complexity)]
fn scan_all_with_progress(
    to_scan: &[(&ModId, &DiscoveredMod)],
    game_version: GameVersion,
    active_bases: &HashSet<ModId>,
    folder_policy: FolderPolicy,
    on_progress: &mut dyn FnMut(ScanProgress),
) -> (
    Vec<ScannedMod>,
    BTreeMap<ModId, HashSet<u64>>,
    BTreeMap<ModId, Vec<RefSite>>,
    Vec<Warning>,
) {
    let total = to_scan.len();
    if total == 0 {
        return (Vec::new(), BTreeMap::new(), BTreeMap::new(), Vec::new());
    }

    // Workers only ever send a zero-sized progress *ping* down this
    // channel as each mod finishes — never the mod's own result, and
    // never a direct call to `on_progress` (which stays single-threaded,
    // called only from the receive loop below). `map_with` hands each
    // rayon worker its own clone of `sender`, so no shared reference to
    // the (non-`Sync`) `Sender` ever crosses a worker boundary.
    let (sender, receiver) = mpsc::channel::<()>();

    let results: Vec<(ModId, ScannedMod, HashSet<u64>, Vec<RefSite>, Vec<Warning>)> =
        std::thread::scope(|scope| {
            let handle = scope.spawn(move || {
                to_scan
                    .par_iter()
                    .map_with(sender, |sender, &(id, m)| {
                        let (scanned, child_value_hashes, ref_sites, mod_warnings) =
                            mod_scan::scan_one_mod(
                                id,
                                m,
                                game_version,
                                active_bases,
                                folder_policy,
                            );
                        let _ = sender.send(());
                        (
                            id.clone(),
                            scanned,
                            child_value_hashes,
                            ref_sites,
                            mod_warnings,
                        )
                    })
                    .collect()
            });

            for (index, ()) in receiver.into_iter().enumerate() {
                on_progress(ScanProgress {
                    stage: ScanStage::Scanning,
                    done: index + 1,
                    total,
                });
            }

            // The receive loop above only ends once every `sender` clone has
            // been dropped, which happens only after `collect()` has already
            // produced its result — so this join is never left waiting on
            // in-flight work. A panicking worker poisons the join instead of
            // this function unwrapping it away.
            handle.join().unwrap_or_default()
        });

    let mut scanned_mods = Vec::with_capacity(results.len());
    let mut child_value_hashes_by_mod = BTreeMap::new();
    let mut ref_sites_by_mod = BTreeMap::new();
    let mut warnings = Vec::new();
    for (id, scanned, child_value_hashes, ref_sites, mod_warnings) in results {
        scanned_mods.push(scanned);
        child_value_hashes_by_mod.insert(id.clone(), child_value_hashes);
        ref_sites_by_mod.insert(id, ref_sites);
        warnings.extend(mod_warnings);
    }
    (
        scanned_mods,
        child_value_hashes_by_mod,
        ref_sites_by_mod,
        warnings,
    )
}

/// `Assembly-CSharp.dll`'s own [`AssemblyMetadata::type_hierarchy`] —
/// every `Verse`/`RimWorld`-namespaced def-type class and its base type,
/// read the same way [`mod_scan::read_assembly`]'s own success path
/// reads a mod's assembly, just for this one specific, fixed file rather
/// than a whole `Assemblies/` folder. `analysis::inheritance`'s own
/// subclass check needs this to resolve a *vanilla* parent's own base
/// chain (`Verse.ThingDef`, `Verse.Def`, ...) the same way it resolves a
/// mod's own; every other `.dll` under `Managed/` (`mscorlib`, Unity's
/// own engine assemblies, ...) declares no RimWorld def-type class, so
/// reading only this one file is enough. Unreadable or unparsable is
/// non-fatal, matching [`managed_assembly_names`]'s own fallback: a
/// [`Warning`] is recorded and the hierarchy is just empty, which
/// [`super::analysis::inheritance::is_subclass`]'s own "can't resolve,
/// don't flag" rule already treats safely.
fn vanilla_type_hierarchy(
    game_dir: &Path,
    warnings: &mut Vec<Warning>,
) -> Vec<(String, Option<String>)> {
    let path = paths::managed_assemblies_dir(game_dir).join("Assembly-CSharp.dll");
    let bytes = match mod_scan::read_bounded(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: cannot read Assembly-CSharp.dll: {e}", path.display()),
            ));
            return Vec::new();
        }
    };
    match crate::extract::pe_metadata::read(&bytes) {
        Ok(meta) => meta.type_hierarchy,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: cannot parse Assembly-CSharp.dll: {e}", path.display()),
            ));
            Vec::new()
        }
    }
}

/// Lowercased `.dll` file stems under `<game_dir>/RimWorldWin64_Data/Managed`
/// — Core/DLC ship no `Assemblies/` folder, so their assembly names have
/// to be seeded from the game install directly. Unreadable is non-fatal:
/// a [`Warning`] is recorded and the ignore set is just empty.
fn managed_assembly_names(game_dir: &Path, warnings: &mut Vec<Warning>) -> HashSet<String> {
    let dir = paths::managed_assemblies_dir(game_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: cannot read Managed assemblies: {e}", dir.display()),
            ));
            return HashSet::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        })
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
        })
        .collect()
}

/// The number of leading bytes read from `globalgamemanagers` before
/// giving up — a defensive bound against a hostile or corrupt file, not a
/// measured real-install size.
const MAX_GLOBAL_GAME_MANAGERS_BYTES: u64 = 64 * 1024 * 1024;

/// Every Core built-in resource texture key, scanned out of
/// `RimWorldWin64_Data/globalgamemanagers` — see
/// [`ScanOutput::core_resource_textures`]'s own doc comment for what this
/// is and how a caller building `Indices` treats too few of them.
/// Unreadable is non-fatal: a [`Warning`] is recorded and the result is
/// just empty, which `analysis::indices::MIN_CORE_RESOURCE_TEXTURES` then
/// reads as "the check disables itself".
fn core_resource_textures(game_dir: &Path, warnings: &mut Vec<Warning>) -> BTreeSet<String> {
    const PREFIX: &str = "textures/";
    let path = paths::global_game_managers_path(game_dir);
    let bytes = match mod_scan::read_bounded_with_limit(&path, MAX_GLOBAL_GAME_MANAGERS_BYTES) {
        Ok(bytes) => bytes,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: cannot read Core's resource index: {e}", path.display()),
            ));
            return BTreeSet::new();
        }
    };
    crate::extract::asset_index::resource_container_paths(&bytes, PREFIX)
        .into_iter()
        .map(|entry| entry[PREFIX.len()..].to_string())
        .collect()
}

/// [`read_about_details`]'s own failure modes — kept distinct from the
/// scan path's own `Warning`-and-skip handling because a caller here
/// (`rim-session`'s `ReadModAbout` use case, through `rim-io`'s
/// `ModAboutReader`) needs to tell "the file no longer exists" (the mod's
/// folder was deleted or renamed since the scan) apart from a genuine
/// read failure or invalid XML.
#[derive(Debug, thiserror::Error)]
pub enum ReadAboutDetailsError {
    /// `<mod_root>/About/About.xml` doesn't exist.
    #[error("{0}: About.xml does not exist")]
    NotFound(PathBuf),
    /// The file exists but couldn't be read (permission denied, or over
    /// the same size cap every mod file read in this crate goes through).
    #[error("{0}: cannot read About.xml: {1}")]
    Io(PathBuf, String),
    /// The file was read but its XML is invalid.
    #[error(transparent)]
    Xml(#[from] AboutXmlError),
}

/// Reads and parses `<mod_root>/About/About.xml`'s lazily-read details
/// (description, mod version, mod icon path) for the mod info panel —
/// the same `About/About.xml` join [`discovery::load_about`] does for the
/// scan path, but read on demand, one mod at a time, rather than during a
/// scan. Normally reached through `rim-session`'s `ModAboutReader` port,
/// not called directly by an interface layer.
///
/// # Errors
///
/// Returns [`ReadAboutDetailsError::NotFound`] when the file doesn't
/// exist, [`ReadAboutDetailsError::Io`] when it exists but can't be read,
/// and [`ReadAboutDetailsError::Xml`] when it can't be parsed.
pub fn read_about_details(
    mod_root: &Path,
    game_version: GameVersion,
) -> Result<AboutDetails, ReadAboutDetailsError> {
    let about_path = mod_root.join("About").join("About.xml");
    if !about_path.is_file() {
        return Err(ReadAboutDetailsError::NotFound(about_path));
    }
    let bytes = mod_scan::read_bounded(&about_path)
        .map_err(|e| ReadAboutDetailsError::Io(about_path.clone(), e.to_string()))?;
    Ok(about_xml::parse_details(&bytes, game_version)?)
}

#[cfg(test)]
mod read_about_details_tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rim-analyzer-infra-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn v16() -> GameVersion {
        GameVersion::new(1, 6)
    }

    #[test]
    fn reads_and_parses_a_mods_about_xml() {
        let root = tempdir("reads");
        std::fs::create_dir_all(root.join("About")).unwrap();
        std::fs::write(
            root.join("About").join("About.xml"),
            br#"<ModMetaData>
                  <packageId>a.b</packageId>
                  <description>hello</description>
                </ModMetaData>"#,
        )
        .unwrap();

        let details = read_about_details(&root, v16()).unwrap();

        assert_eq!(details.package_id, ModId::new("a.b"));
        assert_eq!(details.description.as_deref(), Some("hello"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_about_xml_is_not_found() {
        let root = tempdir("missing");

        let error = read_about_details(&root, v16()).unwrap_err();

        assert!(matches!(error, ReadAboutDetailsError::NotFound(_)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_xml_is_a_parse_error() {
        let root = tempdir("invalid");
        std::fs::create_dir_all(root.join("About")).unwrap();
        std::fs::write(root.join("About").join("About.xml"), b"<NotModMetaData/>").unwrap();

        let error = read_about_details(&root, v16()).unwrap_err();

        assert!(matches!(
            error,
            ReadAboutDetailsError::Xml(AboutXmlError::WrongRoot)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
