//! Scans one mod's loaded folders: resolves `LoadFolders.xml`, then walks
//! `Defs/`, `Patches/`, `Textures/`, and `Assemblies/` under each loaded
//! folder into a [`ScannedMod`].

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use walkdir::WalkDir;

use crate::domain::{
    AssemblyInfo, DefEntry, FolderPolicy, GameVersion, ManifestOrder, Mod, ModId, PatchOp, RefSite,
    RefSiteOwner, ScanCost, ScannedMod, TemplateEntry, TexturePathCandidate, UndecodableTexture,
    Warning, XmlLocator,
};
use crate::extract::file_order::ntfs_collation_key;
use crate::extract::ref_sites::TextInterner;
use crate::extract::side_loaded_assemblies::{self, AssemblyLocation};
use crate::extract::{
    asset_index, defs, languages, load_folders, manifest_xml, patches, pe_metadata, sounds,
    textures,
};

use super::discovery::DiscoveredMod;

/// Files larger than this are skipped (with a warning) rather than read:
/// a defensive bound against a hostile or corrupt mod file exhausting
/// memory.
const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;

/// Why a bounded read produced no bytes.
#[derive(Debug, Error)]
pub enum ReadError {
    /// The file could not be opened or read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The file is larger than the read limit.
    #[error("skipped: {size} bytes exceeds the {limit}-byte read limit")]
    TooLarge {
        /// The file's size in bytes (or, for a file that outgrew its stat,
        /// the number of bytes read before the cut-off).
        size: u64,
        /// The limit it exceeded.
        limit: u64,
    },
}

/// Reads `path` in full, refusing anything over [`MAX_READ_BYTES`]. The
/// bound every mod-file read in this crate goes through — `About.xml`,
/// `LoadFolders.xml`, `ExpansionDefs.xml`, and every `Defs`/`Patches`/
/// `Assemblies` file — so a hostile or corrupt file can't exhaust memory.
pub(crate) fn read_bounded(path: &Path) -> Result<Vec<u8>, ReadError> {
    read_bounded_with_limit(path, MAX_READ_BYTES)
}

/// `read_bounded` with an explicit limit: the one bounded-read
/// implementation, shared with `rim-io`'s own small-file reads and used by
/// tests to probe the boundary without a multi-hundred-megabyte fixture.
///
/// # Errors
///
/// [`ReadError::Io`] when the file cannot be opened or read;
/// [`ReadError::TooLarge`] when it is larger than `limit`.
pub fn read_bounded_with_limit(path: &Path, limit: u64) -> Result<Vec<u8>, ReadError> {
    use std::io::Read as _;

    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > limit {
        return Err(ReadError::TooLarge { size, limit });
    }
    // The size above is only a fast refusal with an exact number: the
    // bound that holds is on the read itself, so a file that grows between
    // the stat and the read is still cut off one byte past the limit.
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(ReadError::TooLarge {
            size: bytes.len() as u64,
            limit,
        });
    }
    Ok(bytes)
}

/// Reads up to `limit` leading bytes of `path`, returning whatever was
/// actually read — fewer than `limit` bytes for a shorter/truncated file,
/// and an empty vector for any read failure (the file vanished mid-scan, a
/// permission error, ...), the same best-effort policy the rest of this
/// module's walk takes toward individual files. Never reads past `limit`
/// regardless of the file's real size, so a caller checking only a file's
/// header never has to bound its own read separately.
fn read_prefix(path: &Path, limit: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buffer = Vec::with_capacity(limit);
    let _ = file.take(limit as u64).read_to_end(&mut buffer);
    buffer
}

/// Everything a mod's loaded folders can contribute, accumulated across
/// every folder in one place instead of threading five separate `&mut`
/// parameters through the walk.
#[derive(Default)]
struct Extracted {
    defs: Vec<DefEntry>,
    templates: Vec<TemplateEntry>,
    patch_ops: Vec<PatchOp>,
    textures: BTreeMap<String, u64>,
    sounds: BTreeSet<String>,
    translation_keys: BTreeSet<String>,
    inline_types: BTreeSet<String>,
    inline_node_path_hashes: HashSet<u64>,
    /// Mirrors [`crate::extract::defs::DefsFile::child_value_hashes`],
    /// accumulated across every `Defs/**/*.xml` file this mod ships — kept
    /// off [`ScannedMod`] itself (see
    /// [`crate::domain::ScanOutput::child_value_hashes_by_mod`]'s own doc
    /// comment for why) and returned separately by [`scan_one_mod`] instead.
    child_value_hashes: HashSet<u64>,
    assemblies: Vec<AssemblyInfo>,
    warnings: Vec<Warning>,
    texture_path_candidates: Vec<TexturePathCandidate>,
    /// Accumulated across every loaded folder this mod scans (`scan_folder`
    /// is called once per folder) — never overwritten between calls.
    scan_cost: ScanCost,
    /// Summed across every `Defs/**/*.xml` file this mod ships — see
    /// [`crate::extract::defs::DefsFile::nameless_def_count`]'s own doc
    /// comment.
    nameless_def_count: usize,
    bundle_textures: BTreeSet<String>,
    /// Normalized texture keys contributed by at least one *non*-`.dds`
    /// image file, accumulated across every loaded folder — the union
    /// [`UndecodableTexture::has_png_sibling`] is read off once the whole
    /// mod has been walked (a sibling can live in a different folder than
    /// the `.dds` file itself).
    non_dds_texture_keys: BTreeSet<String>,
    /// Every non-shadowed `.dds` file's own path and normalized key, queued
    /// during the `Textures/` walk for the header read that happens once,
    /// after every loaded folder has been visited (see
    /// [`scan_one_mod`]'s own tail) — not inline in [`scan_folder`], since
    /// [`Self::non_dds_texture_keys`] (needed for `has_png_sibling`) isn't
    /// complete until every folder has been walked.
    dds_candidates: Vec<(PathBuf, String)>,
    undecodable_textures: Vec<UndecodableTexture>,
    /// Mirrors [`crate::extract::defs::DefsFile::nested_may_require`],
    /// accumulated across every `Defs/**/*.xml` file this mod ships.
    nested_may_require: Vec<(String, XmlLocator)>,
    /// Mirrors [`crate::extract::defs::DefsFile::ref_sites`] (accumulated
    /// across every `Defs/**/*.xml` file) plus every candidate this mod's
    /// own `Patches/**/*.xml` mutating ops contribute from their own
    /// `<value>` — kept off [`ScannedMod`] itself for the same reason
    /// [`Self::child_value_hashes`] is; see
    /// [`crate::domain::ScanOutput::ref_sites_by_mod`]'s own doc comment.
    ref_sites: Vec<RefSite>,
}

/// [`scan_folder`]'s own `AssetBundles/` walk needs three things beyond
/// what it already receives for `Defs/`/`Patches/`/`Assemblies/` — grouped
/// here so its parameter list doesn't grow by three more independent
/// values.
struct BundleScanContext {
    /// This mod's own installed directory name
    /// (`ModContentPack.FolderName` — the folder RimWorld actually
    /// mounted, not necessarily its `packageId`) — the first of the two
    /// prefixes [`asset_index::bundle_asset_key`] accepts.
    folder_name: String,
    /// The `packageId` fallback prefix (`ContentFinder<T>.TryFindAssetInModBundles`'s
    /// non-vanilla lookup) — `None` for Core and every DLC, which
    /// `bundle_asset_key` must never try.
    package_id: Option<String>,
    /// Bundle files (by absolute path) a higher-priority loaded folder
    /// already supplies at the same relative path — the `AssetBundles/`
    /// analogue of [`shadowed_paths`], computed once per mod the same way.
    shadowed_bundles: HashSet<PathBuf>,
}

/// Resolves loaded folders and extracts everything under them for one
/// already-discovered mod. Never fails: extraction problems become
/// [`Warning`]s attached to the mod, and the mod is still returned with
/// whatever was successfully extracted.
///
/// `active_id` is the *exact* id this mod was listed under in
/// `ModsConfig.xml` — possibly `_steam`-suffixed — and becomes
/// [`Mod::id`](crate::domain::Mod); `discovered.id` (parsed from this
/// mod's own `About.xml`, never suffixed) is used only to locate the mod
/// on disk. Keeping the exact active id on the resulting [`Mod`] is what
/// lets [`LoadOrder`](crate::domain::LoadOrder) position lookups find it
/// later.
///
/// The third element of the returned tuple is this mod's own
/// [`Extracted::child_value_hashes`] — see
/// [`crate::domain::ScanOutput::child_value_hashes_by_mod`]'s own doc
/// comment for why it travels separately from [`ScannedMod`] rather than
/// as one of its fields.
pub fn scan_one_mod(
    active_id: &ModId,
    discovered: &DiscoveredMod,
    game_version: GameVersion,
    active_bases: &HashSet<ModId>,
    folder_policy: FolderPolicy,
) -> (ScannedMod, HashSet<u64>, Vec<RefSite>, Vec<Warning>) {
    let mut extracted = Extracted::default();
    let (load_folders_bytes, load_folders_version_matched, read_warning) =
        read_load_folders(&discovered.path.join("LoadFolders.xml"), game_version);
    if let Some(message) = read_warning {
        extracted
            .warnings
            .push(Warning::new(Some(active_id.clone()), message));
    }
    let (resolved_folders, if_mod_active_targets, load_folders_warning) = resolve_loaded_folders(
        discovered,
        load_folders_bytes.as_deref(),
        game_version,
        active_bases,
        folder_policy,
    );
    if let Some(message) = load_folders_warning {
        extracted
            .warnings
            .push(Warning::new(Some(active_id.clone()), message));
    }
    let loaded_folders: Vec<PathBuf> = resolved_folders.iter().map(|f| f.path.clone()).collect();

    let recursive = folder_policy == FolderPolicy::Everything;
    let shadowed = shadowed_paths(&loaded_folders, "Defs", &["xml"], recursive)
        .into_iter()
        .chain(shadowed_paths(
            &loaded_folders,
            "Patches",
            &["xml"],
            recursive,
        ))
        .collect();
    let shadowed_assemblies = shadowed_paths(&loaded_folders, "Assemblies", &["dll"], recursive);
    // A `.dds` file's own winning copy, across this mod's loaded folders —
    // the `AssetBundles/` walk needs the analogous shadow set for bundle
    // files themselves. Neither set changes `out.textures`/`ScanCost`
    // accounting (see `scan_folder`'s own doc comment): only the header
    // check and the manifest read respect it.
    let shadowed_dds = shadowed_paths(&loaded_folders, "Textures", &["dds"], recursive);
    let bundle_context = BundleScanContext {
        folder_name: discovered
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        package_id: (!discovered.source.is_vanilla())
            .then(|| active_id.base().as_str().to_string()),
        shadowed_bundles: shadowed_bundle_paths(&loaded_folders, recursive),
    };
    for folder in &resolved_folders {
        scan_folder(
            &folder.path,
            active_id,
            recursive,
            &shadowed,
            &shadowed_assemblies,
            &shadowed_dds,
            &bundle_context,
            &folder.if_mod_active,
            &mut extracted,
        );
    }
    // The DDS header check runs only now, after every loaded folder has
    // been walked — `extracted.non_dds_texture_keys` (needed for
    // `has_png_sibling`) isn't complete until then, since a sibling can
    // live in a different folder than the `.dds` file itself.
    for (path, key) in std::mem::take(&mut extracted.dds_candidates) {
        let header = read_prefix(&path, textures::DDS_HEADER_BYTES);
        let verdict = textures::classify_dds(&header);
        if verdict.undecodable {
            extracted.undecodable_textures.push(UndecodableTexture {
                mod_id: active_id.clone(),
                path: key.clone(),
                width: verdict.width,
                height: verdict.height,
                fourcc: verdict.fourcc,
                has_png_sibling: extracted.non_dds_texture_keys.contains(&key),
            });
        }
    }

    if let Some(message) = side_loaded_assembly_note(&discovered.path) {
        extracted
            .warnings
            .push(Warning::new(Some(active_id.clone()), message));
    }

    let (manifest_order, manifest_warning) = read_manifest(&discovered.path);
    if let Some(message) = manifest_warning {
        extracted
            .warnings
            .push(Warning::new(Some(active_id.clone()), message));
    }

    let info = Mod {
        id: active_id.clone(),
        name: discovered.about.name.clone(),
        authors: discovered.about.authors.clone(),
        url: discovered.about.url.clone(),
        path: discovered.path.clone(),
        source: discovered.source,
        supported_versions: discovered.about.supported_versions.clone(),
        declared: discovered.about.declared.clone(),
        loaded_folders,
        hard_dependents: 0,
        soft_dependents: 0,
        awareness_dependents: 0,
        is_framework_candidate: false,
        generated: discovered.generated.clone(),
        workshop_id: discovered.workshop_id,
        load_folders_version_matched,
    };

    // Held until analysis, alongside every other mod's — growth slack left
    // over from extending this per file would otherwise be kept as well.
    let mut ref_sites = extracted.ref_sites;
    ref_sites.shrink_to_fit();
    (
        ScannedMod {
            info,
            defs: extracted.defs,
            templates: extracted.templates,
            patch_ops: extracted.patch_ops,
            textures: extracted.textures,
            sounds: extracted.sounds,
            translation_keys: extracted.translation_keys,
            inline_types: extracted.inline_types,
            manifest_order,
            inline_node_path_hashes: extracted.inline_node_path_hashes,
            assemblies: extracted.assemblies,
            if_mod_active_targets,
            texture_path_candidates: extracted.texture_path_candidates,
            scan_cost: extracted.scan_cost,
            nameless_def_count: extracted.nameless_def_count,
            bundle_textures: extracted.bundle_textures,
            undecodable_textures: extracted.undecodable_textures,
            nested_may_require: extracted.nested_may_require,
        },
        extracted.child_value_hashes,
        ref_sites,
        extracted.warnings,
    )
}

/// Reads `path` (a mod's own `LoadFolders.xml`) once for both
/// [`resolve_loaded_folders`]'s own use and the
/// `load_folders_version_matched` guard, returning the raw bytes (for the
/// former), the guard's own verdict, and a [`Warning`] message when the file
/// exists but couldn't be read at all. A read failure (a genuine permission
/// error, or the file exceeding [`MAX_READ_BYTES`]) must not collapse into
/// the same `None` a missing file produces, which the guard would read as
/// "ships no `LoadFolders.xml`" — unguarded — instead of "exists but
/// under-scanned".
///
/// The guard's own verdict: `None` when the mod ships no `LoadFolders.xml` at
/// all (a genuine "not found" read error). `Some(false)` when the file exists
/// but couldn't be read, or [`load_folders::has_exact_version_block`] found
/// no exact `<vX.Y>` block for `game_version` — **independent of `<default>`,
/// a deliberately stricter question than [`resolve_loaded_folders`]'s own
/// fallback-aware resolution** (see
/// [`load_folders::has_exact_version_block`]'s own doc comment for why: a
/// file shaped like `<v1.4>`/`<v1.5>`/`<default>`/no-`<v1.6>` must read
/// `Some(false)` here, even though `resolve_version_entry`'s own fallback
/// resolves it). A parse error is folded into `Some(false)` too — a block for
/// the running version might sit in the part that failed to parse, so this
/// stays conservative rather than guessing `Some(true)`. `Some(true)` only
/// when the exact block genuinely exists.
fn read_load_folders(
    path: &Path,
    game_version: GameVersion,
) -> (Option<Vec<u8>>, Option<bool>, Option<String>) {
    match read_bounded(path) {
        Ok(bytes) => {
            let matched = matches!(
                load_folders::has_exact_version_block(&bytes, game_version),
                Ok(true)
            );
            (Some(bytes), Some(matched), None)
        }
        Err(error) if is_missing_file(&error) => (None, None, None),
        Err(error) => (
            None,
            Some(false),
            Some(format!(
                "LoadFolders.xml: {error}; treating as under-scanned"
            )),
        ),
    }
}

/// Whether `error` means "no such file" (the ordinary, expected case —
/// most mods ship no `LoadFolders.xml` at all) rather than a genuine read
/// failure (permission denied, or the file exceeding [`MAX_READ_BYTES`])
/// — a small, directly testable predicate so [`read_load_folders`]'s own
/// branch doesn't need a real filesystem error to exercise in a test.
fn is_missing_file(error: &ReadError) -> bool {
    matches!(error, ReadError::Io(io_error) if io_error.kind() == std::io::ErrorKind::NotFound)
}

/// One folder [`resolve_loaded_folders`] selected for a mod, in load
/// priority order (highest priority first — see that function's own doc
/// comment), plus the `IfModActive`/`IfModActiveAll` gate the `<li>` that
/// named it carried, if any (empty for the default folder rule, which has
/// no such gates). `scan_one_mod` stamps this onto every [`PatchOp`] its
/// own `Patches/` walk finds under this one folder
/// ([`PatchOp::load_folder_gate`]).
struct LoadedFolder {
    path: PathBuf,
    if_mod_active: Vec<ModId>,
}

/// Resolves the loaded folders for one mod, in **load priority order**
/// (highest-priority folder first) — `LoadFolders.xml` when present and it
/// has an entry (version-specific or `<default>`) for `game_version`,
/// otherwise the default folder rule. A pure query: the caller pushes the
/// returned warning message, if any, rather than this function mutating
/// shared state itself.
///
/// This *is* [`Mod::loaded_folders`](crate::domain::Mod::loaded_folders)'s
/// own order — `ModContentPack.foldersToLoadDescendingOrder` — and also the
/// order [`scan_one_mod`] walks folders in, so it now decides the order of
/// `defs`/`templates`/`patch_ops`/`assemblies` too, not just which folder
/// wins a same-relative-path collision. Ground-truthed against decompiled
/// `Verse.ModContentPack.InitLoadFolders`, whose local `AddFolders`
/// function walks a parsed `<li>` list from its *last* entry down to its
/// first when building `foldersToLoadDescendingOrder` — so among two `<li>`
/// entries, the one listed **last** in the XML is the one that ends up
/// first (highest priority) here. The default folder rule's own
/// construction order (version folder, then `Common`, then root) already
/// *is* priority order — `InitLoadFolders`'s fallback branch `.Add()`s each
/// one directly, with no `AddFolders` reversal — so no reversal is needed
/// on that branch.
fn resolve_loaded_folders(
    discovered: &DiscoveredMod,
    load_folders_bytes: Option<&[u8]>,
    game_version: GameVersion,
    active_bases: &HashSet<ModId>,
    folder_policy: FolderPolicy,
) -> (Vec<LoadedFolder>, Vec<ModId>, Option<String>) {
    if folder_policy == FolderPolicy::Everything {
        let root = vec![LoadedFolder {
            path: discovered.path.clone(),
            if_mod_active: Vec::new(),
        }];
        return (root, Vec::new(), None);
    }

    let (folders, if_mod_active_targets, warning) = match load_folders_bytes
        .map(|bytes| load_folders::resolve_version_entry(bytes, game_version, active_bases))
    {
        Some(Ok(Some(entry))) => {
            let mut folders = existing_dirs(&discovered.path, &entry.folders);
            folders.reverse();
            (folders, entry.if_mod_active_targets, None)
        }
        Some(Ok(None)) | None => {
            let folders = default_folders_on_disk(&discovered.path, game_version);
            (folders, Vec::new(), None)
        }
        Some(Err(e)) => {
            let folders = default_folders_on_disk(&discovered.path, game_version);
            let message = format!("LoadFolders.xml: {e}; falling back to default folder rule");
            (folders, Vec::new(), Some(message))
        }
    };
    (
        dedup_folders_by_path(folders),
        if_mod_active_targets,
        warning,
    )
}

/// Deduplicates [`LoadedFolder`]s by physical path, keeping the first
/// (highest-priority, since the caller has already reordered into
/// priority order) occurrence of each. A matched `LoadFolders.xml` block
/// can name the same physical folder more than once — the common
/// real-install idiom is one compat folder gated behind two different
/// `<li IfModActive="…">`s, each independently satisfied — and
/// [`load_folders::resolve_version_entry`] already filters each `<li>` by
/// its own gate before this function ever sees it, so by construction, if
/// either gate passes the folder is present here (possibly twice).
///
/// Leaving a duplicate in place is worse than a merely-wasted rescan: the
/// real engine's own `GetAllFilesForMod`/`XmlAssetsInModFolder`
/// `TryAdd`s per relative-path key regardless of how many times the
/// folder itself was named, so it loads the content once — but
/// [`shadowed_paths`]' claimed-key set is keyed on the file's *absolute*
/// path, and a duplicated folder's two occurrences enumerate the exact
/// same absolute paths. Its second pass claims a key already claimed by
/// its first pass and marks that same absolute path shadowed, so every
/// file in the folder ends up shadowed on *both* occurrences and the
/// folder loads zero times instead of once. Deduping here, before
/// [`shadowed_paths`]/[`scan_folder`] ever see the list, is what keeps a
/// duplicate `<li>` from being anything but a no-op.
fn dedup_folders_by_path(folders: Vec<LoadedFolder>) -> Vec<LoadedFolder> {
    let mut seen_paths = HashSet::new();
    folders
        .into_iter()
        .filter(|folder| seen_paths.insert(folder.path.clone()))
        .collect()
}

/// Absolute paths of every `<subdir_name>/**` file matching `extensions`
/// that a higher-priority folder in `folders_in_priority_order` (index 0
/// highest) already supplies at the same folder-relative path —
/// ground-truthed against decompiled `Verse.DirectXmlLoader
/// .XmlAssetsInModFolder`/`ModContentPack.GetAllFilesForMod`, both of which
/// build a `Dictionary<string, FileInfo>` over exactly this key (a file's
/// path relative to its loaded folder's own root, e.g. `Defs\Things.xml`)
/// via `TryAdd`, so only the *first* folder (in `ModContentPack
/// .foldersToLoadDescendingOrder` order) to offer a given key is ever read
/// — RimWorld's own engine never parses a shadowed file's contents, so this
/// analyzer must not either, or it reports patch failures, duplicate defs,
/// and doubled assembly references a real game log never shows. `.NET`'s
/// default `Dictionary<string, _>` key comparer is ordinal (case-sensitive,
/// no folding) and never normalizes path separators beyond what
/// `Path.Combine` already wrote — [`PathBuf`]'s own `Eq`/`Hash` are exactly
/// that (byte-exact `OsStr` component comparison), so using it directly as
/// the key here needs no extra folding to match. Called once for `Defs`,
/// once for `Patches` (`ModContentPack.LoadPatches`/`LoadDefs` both route
/// through the identical method, so the rule is the same for both), and
/// once for `Assemblies` (`GetAllFilesForModPreserveOrder` applies the same
/// per-relative-path first-wins rule, walking lowest-priority-first and
/// overwriting rather than `TryAdd`ing — the opposite walk direction
/// produces the identical winner, since either way it's the
/// highest-priority folder's own copy that survives).
fn shadowed_paths(
    folders_in_priority_order: &[PathBuf],
    subdir_name: &str,
    extensions: &[&str],
    recursive: bool,
) -> HashSet<PathBuf> {
    let mut shadowed = HashSet::new();
    let mut claimed_keys: HashSet<PathBuf> = HashSet::new();
    for folder in folders_in_priority_order {
        for dir in find_subdirs(folder, subdir_name, recursive) {
            for path in walk_files(&dir, extensions) {
                let Ok(key) = path.strip_prefix(folder) else {
                    continue;
                };
                if !claimed_keys.insert(key.to_path_buf()) {
                    shadowed.insert(path);
                }
            }
        }
    }
    shadowed
}

/// The `AssetBundles/` analogue of [`shadowed_paths`]: a bundle file's own
/// extensionless name has no `extensions` list [`walk_files`] could filter
/// on, so this walks [`list_dir_files`] directly instead, otherwise
/// applying the identical same-relative-path, highest-priority-folder-wins
/// rule.
fn shadowed_bundle_paths(
    folders_in_priority_order: &[PathBuf],
    recursive: bool,
) -> HashSet<PathBuf> {
    let mut shadowed = HashSet::new();
    let mut claimed_keys: HashSet<PathBuf> = HashSet::new();
    for folder in folders_in_priority_order {
        for dir in find_subdirs(folder, "AssetBundles", recursive) {
            for path in list_dir_files(&dir) {
                let Ok(key) = path.strip_prefix(folder) else {
                    continue;
                };
                if !claimed_keys.insert(key.to_path_buf()) {
                    shadowed.insert(path);
                }
            }
        }
    }
    shadowed
}

/// Every regular file directly inside `dir` — not recursive, since a
/// bundle and its `.manifest` sidecar both sit flat in `AssetBundles/` —
/// sorted by raw file name for determinism.
fn list_dir_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// Joins `mod_root` with each candidate folder's relative path (`""`
/// meaning the root itself) and keeps only the ones that actually exist as
/// directories — `LoadFolders.xml` sometimes names folders a mod doesn't
/// ship. Document order in, document order out; [`resolve_loaded_folders`]
/// reverses the result itself once existence has narrowed it, matching the
/// real engine's own order of operations (`AddFolders` walks the parsed
/// `<li>` list, existence is checked per-entry inside that same walk).
fn existing_dirs(mod_root: &Path, relative: &[load_folders::VersionFolder]) -> Vec<LoadedFolder> {
    relative
        .iter()
        .map(|f| LoadedFolder {
            path: relative_to_path(mod_root, &f.relative),
            if_mod_active: f.if_mod_active.clone(),
        })
        .filter(|f| f.path.is_dir())
        .collect()
}

fn default_folders_on_disk(mod_root: &Path, game_version: GameVersion) -> Vec<LoadedFolder> {
    let names = subdirectory_names(mod_root);
    load_folders::default_folders(&names, game_version)
        .into_iter()
        .map(|r| LoadedFolder {
            path: relative_to_path(mod_root, &r),
            if_mod_active: Vec::new(),
        })
        .collect()
}

fn relative_to_path(mod_root: &Path, relative: &str) -> PathBuf {
    if relative.is_empty() {
        mod_root.to_path_buf()
    } else {
        mod_root.join(relative)
    }
}

fn subdirectory_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// Reads and parses every file in `files`, handing each successfully
/// parsed file's whole result to `sink` and every read or parse failure
/// into `out_warnings` — the read-parse-warn shape shared by every walk in
/// [`scan_folder`]. `parse` also receives the file's own [`Arc<Path>`]
/// (shared, not cloned per element) so it can build each item's
/// [`crate::domain::XmlLocator`].
///
/// `sink` takes the parse's whole result rather than one item at a time —
/// letting the `Defs/` walk split one file's [`defs::DefsFile`] into two
/// output vectors (defs, templates) without a second parse.
///
/// Takes the already-enumerated file list rather than a directory to walk
/// itself, so the `Defs/`/`Patches/` walks can hand it
/// [`engine_enumeration_order`]'s output (the order the game itself reads
/// files in) while every other caller keeps the plain sorted
/// [`walk_files`] — the two orders matter for different reasons and must
/// not be conflated into one shared traversal here.
///
/// `shadowed` is skipped silently — no warning, no read — matching the
/// real engine, which never opens a shadowed file's contents either (see
/// [`shadowed_paths`]'s own doc comment). Every call site outside the
/// `Defs/`/`Patches/` walks passes an empty set: nothing else in this
/// module's walk is deduped this way.
/// Combines a mutating patch op's own resolved `sub_path` with one of its
/// `<value>`'s own [`crate::extract::patches::PatchValueRefSite::field_path`]s
/// (relative to `<value>`'s own root) into the def-relative path the value
/// actually lands at — the same convention a def-tree
/// [`crate::domain::RefSite::field_path`] already uses, so both shapes
/// vote under one key. An empty relative path (a candidate at `<value>`'s
/// own root) collapses onto `sub_path` alone; no `sub_path` at all (a
/// whole-def-root op) leaves the relative path untouched.
fn combine_sub_path(sub_path: Option<&str>, relative: &str) -> String {
    match (sub_path, relative.is_empty()) {
        (Some(sub), true) => sub.to_string(),
        (Some(sub), false) => format!("{sub}/{relative}"),
        (None, _) => relative.to_string(),
    }
}

/// One patch file's own [`patches::PatchValueRefSite`]s as domain
/// [`RefSite`]s owned by `mod_id`. Every site of one op shares that op's
/// own [`RefSiteOwner::Patch`](crate::domain::RefSiteOwner::Patch), and
/// repeated def types and field paths are shared across the file — see
/// [`RefSite`]'s own doc comment for why.
fn patch_ref_sites(mod_id: &ModId, sites: Vec<patches::PatchValueRefSite>) -> Vec<RefSite> {
    let mut text = TextInterner::default();
    let mut owner: Option<Arc<RefSiteOwner>> = None;
    sites
        .into_iter()
        .map(|site| {
            let site_owner = match &owner {
                Some(shared) if patch_owner_matches(shared, &site) => Arc::clone(shared),
                _ => {
                    let fresh = Arc::new(RefSiteOwner::Patch {
                        mod_id: mod_id.clone(),
                        def_name: site.target.def_name.clone(),
                        locator: site.op_locator,
                    });
                    owner = Some(Arc::clone(&fresh));
                    fresh
                }
            };
            RefSite {
                def_type: text.intern(&site.target.def_type),
                field_path: text.intern(&combine_sub_path(
                    site.target.sub_path.as_deref(),
                    &site.field_path,
                )),
                shape: site.shape,
                value: site.value,
                owner: site_owner,
                may_require: site.may_require.into_boxed_slice(),
                may_require_any_of: site.may_require_any_of.into_boxed_slice(),
            }
        })
        .collect()
}

/// Whether `owner` (a [`RefSiteOwner::Patch`] built for an earlier site of
/// the same file) is exactly the owner `site` itself would get.
fn patch_owner_matches(owner: &RefSiteOwner, site: &patches::PatchValueRefSite) -> bool {
    matches!(owner, RefSiteOwner::Patch { def_name, locator, .. }
        if *def_name == site.target.def_name && *locator == site.op_locator)
}

fn extract_files<T, E: std::fmt::Display>(
    files: Vec<PathBuf>,
    mod_id: &ModId,
    shadowed: &HashSet<PathBuf>,
    parse: impl Fn(&[u8], &Arc<Path>) -> Result<T, E>,
    mut sink: impl FnMut(T),
    out_warnings: &mut Vec<Warning>,
) {
    for path in files {
        if shadowed.contains(&path) {
            continue;
        }
        let bytes = match read_bounded(&path) {
            Ok(b) => b,
            Err(e) => {
                out_warnings.push(file_warning(mod_id, &path, &e));
                continue;
            }
        };
        let file: Arc<Path> = Arc::from(path.as_path());
        match parse(&bytes, &file) {
            Ok(result) => sink(result),
            Err(e) => out_warnings.push(file_warning(mod_id, &path, &e)),
        }
    }
}

/// Walks one loaded folder's `Defs/`, `Patches/`, `Textures/`, and
/// `Assemblies/` subdirectories. In normal mode these must be *direct*
/// children of `folder`, matching how RimWorld itself resolves them; in
/// `--all-folders` diagnostic mode `folder` is the whole mod root, so
/// every subdirectory with one of these names — at any depth — counts.
///
/// `shadowed`/`shadowed_assemblies` (from [`shadowed_paths`], computed
/// once per mod across every loaded folder) dedup the `Defs/`/`Patches/`
/// and `Assemblies/` walks respectively, by relative-path — see that
/// function's own doc comment. `load_folder_gate` is this one folder's own
/// `IfModActive`/`IfModActiveAll` gate (empty for the default folder rule),
/// stamped onto every [`PatchOp`] this call's own `Patches/` walk finds.
/// `shadowed_dds` is the same rule for `Textures/**/*.dds` specifically —
/// it gates only the DDS header check, never `out.textures`/`ScanCost`
/// accounting (which stays whole-mod, summing every folder's own copy, as
/// it always has). `bundle_context` carries what the new `AssetBundles/`
/// walk needs (see its own doc comment).
// Nine parameters: one per independent per-mod fact this private walk
// needs (five already existed; the two this change adds group three more
// independent values into `bundle_context` rather than adding them
// separately). A struct wrapping the rest would just move the same count
// into a second type with no caller but this one.
#[allow(clippy::too_many_arguments)]
fn scan_folder(
    folder: &Path,
    mod_id: &ModId,
    recursive: bool,
    shadowed: &HashSet<PathBuf>,
    shadowed_assemblies: &HashSet<PathBuf>,
    shadowed_dds: &HashSet<PathBuf>,
    bundle_context: &BundleScanContext,
    load_folder_gate: &[ModId],
    out: &mut Extracted,
) {
    // Collected outside the closure below (rather than pushed straight to
    // `out.warnings` from inside it) so the closure only ever captures
    // `out.defs`/.../`out.ref_sites` — Edition 2024's disjoint closure
    // capture is what lets the `&mut out.warnings` argument right after it
    // borrow a genuinely separate field; reaching into `out.warnings` from
    // inside the closure too would make the two borrows overlap.
    let mut ref_sites_truncated_files = 0u32;
    for dir in find_subdirs(folder, "Defs", recursive) {
        extract_files(
            engine_enumeration_order(&dir, &["xml"]),
            mod_id,
            shadowed,
            defs::index,
            |defs_file: defs::DefsFile| {
                out.defs.extend(defs_file.defs);
                out.templates.extend(defs_file.templates);
                out.inline_types.extend(defs_file.inline_types);
                out.inline_node_path_hashes
                    .extend(defs_file.inline_node_path_hashes);
                out.child_value_hashes.extend(defs_file.child_value_hashes);
                out.texture_path_candidates
                    .extend(defs_file.texture_path_candidates);
                out.nameless_def_count += defs_file.nameless_def_count;
                out.nested_may_require.extend(defs_file.nested_may_require);
                out.ref_sites.extend(defs_file.ref_sites);
                if defs_file.ref_sites_truncated {
                    ref_sites_truncated_files += 1;
                }
            },
            &mut out.warnings,
        );
    }
    if ref_sites_truncated_files > 0 {
        out.warnings.push(Warning::new(
            Some(mod_id.clone()),
            format!(
                "reference-site scan truncated in {ref_sites_truncated_files} file(s): some \
                 dangling-def-reference evidence may be incomplete"
            ),
        ));
    }

    let mut patch_tree_truncated_files = 0u32;
    for dir in find_subdirs(folder, "Patches", recursive) {
        extract_files(
            engine_enumeration_order(&dir, &["xml"]),
            mod_id,
            shadowed,
            patches::walk_with_ref_sites,
            |(ops, value_ref_sites, tree_depth_truncated): (
                Vec<PatchOp>,
                Vec<patches::PatchValueRefSite>,
                bool,
            )| {
                out.patch_ops.extend(ops.into_iter().map(|mut op| {
                    op.load_folder_gate = load_folder_gate.to_vec();
                    op
                }));
                out.ref_sites
                    .extend(patch_ref_sites(mod_id, value_ref_sites));
                if tree_depth_truncated {
                    patch_tree_truncated_files += 1;
                }
            },
            &mut out.warnings,
        );
    }
    if patch_tree_truncated_files > 0 {
        out.warnings.push(Warning::new(
            Some(mod_id.clone()),
            format!(
                "patch operation nesting exceeded the depth limit in \
                 {patch_tree_truncated_files} file(s): some deeply nested \
                 operations were not scanned"
            ),
        ));
    }

    for dir in find_subdirs(folder, "Textures", recursive) {
        for (path, size) in walk_files_with_size(&dir, &textures::IMAGE_EXTENSIONS) {
            let Ok(relative) = path.strip_prefix(&dir) else {
                continue;
            };
            // Raw file count/bytes, not deduplicated by normalized key — a
            // mod shipping both `Wall.png` and `Wall.dds` counts 2 files here
            // even though they collapse to one `out.textures` key below.
            out.scan_cost.texture_files += 1;
            out.scan_cost.texture_bytes += size;
            let is_dds = relative
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dds"));
            if is_dds {
                out.scan_cost.dds_files += 1;
            }
            if let Some(key) = textures::normalize(relative) {
                *out.textures.entry(key.clone()).or_insert(0) += size;
                if is_dds {
                    // Only the winning (non-shadowed) copy is ever read for
                    // its header — RimWorld never opens a shadowed file's
                    // contents either (see `shadowed_paths`' own doc
                    // comment). The header read itself happens once, after
                    // every folder has been walked — see [`scan_one_mod`].
                    if !shadowed_dds.contains(&path) {
                        out.dds_candidates.push((path.clone(), key));
                    }
                } else {
                    out.non_dds_texture_keys.insert(key);
                }
            }
        }
    }

    for dir in find_subdirs(folder, "AssetBundles", recursive) {
        for path in list_dir_files(&dir) {
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Only extensionless files are bundles (`ModAssetBundlesHandler.IsAcceptableExtension`) — a `.manifest`
            // sidecar (or a `.manifest.txt`/anything else with a dot) is
            // never itself walked as a candidate bundle; it's read below,
            // keyed off its own bundle file's path.
            if file_name.contains('.') {
                continue;
            }
            if !asset_index::is_os_eligible_bundle_name(file_name) {
                continue;
            }
            if bundle_context.shadowed_bundles.contains(&path) {
                continue;
            }
            let manifest_path = path.with_extension("manifest");
            match read_bounded(&manifest_path) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for entry in asset_index::parse_bundle_manifest(&text) {
                        if let Some(key) = asset_index::bundle_asset_key(
                            &entry,
                            &bundle_context.folder_name,
                            bundle_context.package_id.as_deref(),
                        ) {
                            out.bundle_textures.insert(key);
                        }
                    }
                }
                Err(e) if is_missing_file(&e) => {
                    out.warnings.push(file_warning(
                        mod_id,
                        &path,
                        &"bundle has no manifest; its textures are unknown",
                    ));
                }
                Err(e) => {
                    out.warnings.push(file_warning(mod_id, &manifest_path, &e));
                }
            }
        }
    }

    for dir in find_subdirs(folder, "Sounds", recursive) {
        for path in walk_files(&dir, &sounds::SOUND_EXTENSIONS) {
            if let Ok(relative) = path.strip_prefix(&dir)
                && let Some(key) = sounds::normalize(relative)
            {
                out.sounds.insert(key);
            }
        }
    }

    // `Languages/<lang>/Keyed/**/*.xml` — the language folder name varies
    // per mod (and per file, for language packs shipping several), so
    // `Keyed` is searched recursively under `Languages` regardless of
    // `recursive` (which governs the *outer* Defs/Patches/Textures/
    // Assemblies/Sounds folders' own depth relative to the version folder,
    // an unrelated concern).
    for languages_dir in find_subdirs(folder, "Languages", recursive) {
        for keyed_dir in find_subdirs(&languages_dir, "Keyed", true) {
            extract_files(
                walk_files(&keyed_dir, &["xml"]),
                mod_id,
                &HashSet::new(),
                |bytes, _file| languages::index(bytes),
                |keys: Vec<String>| out.translation_keys.extend(keys),
                &mut out.warnings,
            );
        }
    }

    for dir in find_subdirs(folder, "Assemblies", recursive) {
        for (path, size) in walk_files_with_size(&dir, &["dll"]) {
            if shadowed_assemblies.contains(&path) {
                continue;
            }
            out.scan_cost.assembly_bytes += size;
            let read = read_assembly(&path);
            if let Some(message) = read.warning {
                out.warnings.push(file_warning(mod_id, &path, &message));
            }
            out.assemblies.push(read.info);
        }
    }
}

/// The order RimWorld's own `DirectoryInfo.GetFiles(pattern,
/// AllDirectories)` walk returns matching files in — breadth-first, not
/// the plain recursive-sorted order this module's own private
/// `walk_files` uses. One directory
/// level at a time: every matching, non-dot-prefixed file directly in a
/// directory, in [`ntfs_collation_key`] order, then every subdirectory
/// (also in that order) queued for the same treatment — mirroring `.NET`'s
/// `FileSystemEnumerator`, which yields one directory's own matches before
/// descending into any of its subdirectories, and visits subdirectories in
/// the order it met them (a FIFO queue), not depth-first.
///
/// Dot-prefixed file names (including `._` — a macOS resource-fork
/// leftover) are skipped here, mirroring `DirectXmlLoader
/// .XmlAssetsInModFolder`'s own filter — a directory whose *name* starts
/// with `.` is still walked, since the rule is about file names, not
/// directories, on the real install.
///
/// `pub` (re-exported as `infra::engine_enumeration_order`) so `rim-io`'s
/// asset locator can walk a `Textures/` directory the same way:
/// `ModContentLoader<T>.LoadAllForMod` reads a mod's texture files
/// through the identical `GetAllFilesForMod` mechanism this function
/// models for `Defs/`/`Patches/`.
pub fn engine_enumeration_order(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(dir.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        let mut files: Vec<(Vec<u16>, PathBuf)> = Vec::new();
        let mut subdirs: Vec<(Vec<u16>, PathBuf)> = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            let key = ntfs_collation_key(name_str);
            if file_type.is_dir() {
                subdirs.push((key, entry.path()));
            } else if file_type.is_file() && !name_str.starts_with('.') {
                let matches_extension = entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        extensions
                            .iter()
                            .any(|allowed| allowed.eq_ignore_ascii_case(ext))
                    });
                if matches_extension {
                    files.push((key, entry.path()));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        subdirs.sort_by(|a, b| a.0.cmp(&b.0));
        result.extend(files.into_iter().map(|(_, path)| path));
        queue.extend(subdirs.into_iter().map(|(_, path)| path));
    }
    result
}

/// One shipped assembly's extracted info, plus an optional warning
/// message when metadata parsing fell back to `parse_failed`. A pure
/// query: the caller decides what to do with the warning.
struct AssemblyRead {
    info: AssemblyInfo,
    warning: Option<String>,
}

fn read_assembly(path: &Path) -> AssemblyRead {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let fallback = |file_name: &str| AssemblyInfo {
        file_name: file_name.to_string(),
        name: file_name.to_lowercase(),
        references: Vec::new(),
        version: None,
        runtime_patches: Vec::new(),
        type_hierarchy: Vec::new(),
        parse_failed: true,
    };

    let bytes = match read_bounded(path) {
        Ok(b) => b,
        Err(e) => {
            return AssemblyRead {
                info: fallback(&file_name),
                warning: Some(e.to_string()),
            };
        }
    };
    match pe_metadata::read(&bytes) {
        Ok(meta) => AssemblyRead {
            info: AssemblyInfo {
                file_name,
                name: meta.name,
                references: meta.references,
                version: Some(meta.version),
                runtime_patches: meta.runtime_patches,
                type_hierarchy: meta.type_hierarchy,
                parse_failed: false,
            },
            warning: None,
        },
        Err(e) => AssemblyRead {
            info: fallback(&file_name),
            warning: Some(e.to_string()),
        },
    }
}

/// Media directory names this walk never needs to descend into: none of them
/// can hold a loadable `.dll` by construction (on a real workshop tree,
/// pruning these plus every dev/inert name skips about two thirds of all
/// files and nearly halves a full-tree walk). Every
/// `side_loaded_assemblies::is_always_ignored` name is *also* pruned (see
/// [`is_prunable`]) — safe only because dev/inert wins unconditionally over a
/// nested `Assemblies/` component
/// (`a_dev_directory_wins_over_an_assemblies_component_nested_inside_it`):
/// otherwise a dev directory could contain a real, *rescued* `Loaded` path,
/// and pruning it from the walk entirely would silently drop a genuinely
/// `Loaded` count.
const PRUNABLE_MEDIA_DIRS: [&str; 3] = ["textures", "sounds", "languages"];

/// Whether `entry` is a directory this walk can skip descending into
/// entirely — a media directory, or one `side_loaded_assemblies::
/// is_always_ignored` already treats as ignored regardless of content.
fn is_prunable(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && entry.file_name().to_str().is_some_and(|name| {
            let lower = name.to_lowercase();
            PRUNABLE_MEDIA_DIRS.contains(&lower.as_str())
                || side_loaded_assemblies::is_always_ignored(&lower)
        })
}

/// Reads and parses `<mod_root>/About/Manifest.xml`, when present. A missing
/// file is silently `(ManifestOrder::default(), None)` — Manifest.xml is
/// optional, most mods have none — but one that exists and fails to read
/// (permission denied, over the [`MAX_READ_BYTES`] cap) or fails to parse is
/// `(ManifestOrder::default(), Some(warning))`, same convention as a
/// malformed `LoadFolders.xml` above: never fatal to the scan, but not
/// silently swallowed either. Only a genuinely absent file
/// (`io::ErrorKind::NotFound`) stays silent; permission errors and an
/// oversized file both warn.
fn read_manifest(mod_root: &Path) -> (ManifestOrder, Option<String>) {
    let path = mod_root.join("About").join("Manifest.xml");
    let bytes = match read_bounded(&path) {
        Ok(bytes) => bytes,
        Err(ReadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return (ManifestOrder::default(), None);
        }
        Err(e) => {
            return (
                ManifestOrder::default(),
                Some(format!("{}: cannot read Manifest.xml: {e}", path.display())),
            );
        }
    };
    match manifest_xml::parse(&bytes) {
        Ok(order) => (order, None),
        Err(e) => (
            ManifestOrder::default(),
            Some(format!("{}: invalid Manifest.xml: {e}", path.display())),
        ),
    }
}

/// Detects a mod whose real engine side-loads from outside `Assemblies/` at
/// runtime (see `extract::side_loaded_assemblies`'s module doc) and, if so,
/// returns the note text to attach as a [`Warning`].
///
/// Walks `mod_root` as a whole, not the loaded folders `scan_folder` was just
/// given — a side-loaded engine typically lives in a folder that is a sibling
/// of the version folder (e.g. `Lunar/Components/`), not something
/// `LoadFolders.xml`/the default folder rule would ever resolve as loaded, so
/// it would never be visited by the `Assemblies/` walk above regardless of
/// `folder_policy`. Uses its own `WalkDir` (not the shared [`walk_files`]) so
/// [`is_prunable`] can skip whole directories via `filter_entry` —
/// `walk_files` is shared with the XML/def scan, where `Languages/` genuinely
/// does hold relevant `.xml` files, so pruning it there would be a real
/// correctness bug, not a speedup.
fn side_loaded_assembly_note(mod_root: &Path) -> Option<String> {
    let mut counts = side_loaded_assemblies::SideLoadCounts::default();
    let mut offending_top_level: BTreeSet<String> = BTreeSet::new();
    let entries = WalkDir::new(mod_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_prunable(entry))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        });
    for entry in entries {
        let path = entry.into_path();
        let Ok(relative) = path.strip_prefix(mod_root) else {
            continue;
        };
        match side_loaded_assemblies::classify(relative) {
            AssemblyLocation::Loaded => counts.loaded += 1,
            AssemblyLocation::SideLoaded => {
                counts.side_loaded += 1;
                if let Some(top_level) = relative
                    .components()
                    .next()
                    .and_then(|c| c.as_os_str().to_str())
                {
                    offending_top_level.insert(top_level.to_string());
                }
            }
            AssemblyLocation::Ignored => {}
        }
    }
    side_loaded_assemblies::note(counts, &offending_top_level)
}

fn file_warning(mod_id: &ModId, path: &Path, error: &dyn std::fmt::Display) -> Warning {
    Warning::new(Some(mod_id.clone()), format!("{}: {error}", path.display()))
}

/// Subdirectories of `folder` named `name` (case-insensitive). Direct
/// children only, unless `recursive` — used by `--all-folders` mode to
/// find e.g. `1.6/Defs` when `folder` is the mod root rather than `1.6`.
fn find_subdirs(folder: &Path, name: &str, recursive: bool) -> Vec<PathBuf> {
    if recursive {
        return WalkDir::new(folder)
            .sort_by_file_name()
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_dir()
                    && e.file_name()
                        .to_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(walkdir::DirEntry::into_path)
            .collect();
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().is_dir()
                && e.file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

/// Every file under `dir` (recursive) whose extension matches one of
/// `extensions`, case-insensitively, in deterministic (sorted) order —
/// this is what makes a scan's `patch_ops`/`defs` order reproducible
/// across runs (see `analysis::source_index`'s "file order" guarantee).
///
/// **Not** what `Defs/`/`Patches/` use any more — see
/// [`engine_enumeration_order`] for those, which models the real engine's
/// breadth-first, NTFS-collation-ordered walk. This plain
/// `sort_by_file_name()` (raw `OsStr`, byte-wise, case-sensitive) walk
/// stays for `Textures/`, `Sounds/`, `Languages/Keyed/`, and listing
/// `Assemblies/` files for reading — only determinism (the same scan
/// twice produces byte-identical output) matters for those, never a
/// load-order-sensitive relative order between two files.
fn walk_files(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    walk_files_with_size(dir, extensions)
        .into_iter()
        .map(|(path, _size)| path)
        .collect()
}

/// Like [`walk_files`], but also returns each file's size in bytes, read off
/// the [`walkdir::DirEntry`] the walk already produced rather than a second
/// `std::fs::metadata` call — the `ScanCost` `Textures/`/`Assemblies/` byte
/// totals ride this for free: on Windows, `DirEntry::metadata()` reads back
/// the size the directory enumeration itself already cached. A metadata read
/// that fails (the file vanished mid-walk) contributes `0` rather than
/// aborting the walk — the same best-effort policy the rest of this module's
/// scan takes toward individual files.
///
/// **The free-ness is Windows-specific**: this crate targets
/// `x86_64-pc-windows-msvc` only (this crate's own `CLAUDE.md`), so that's
/// the only platform this claim needs to hold on, but it is not a universal
/// property of `walkdir`/`DirEntry::metadata()` — on Unix-like platforms a
/// directory read (`readdir`) does not populate a full `stat`, so
/// `metadata()` there issues its own `lstat`/`stat` syscall per file, same as
/// calling `std::fs::metadata` a second time would. `walk_files` (shared with
/// the `Defs`/`Patches`/`Languages` walks) calls this and discards the size
/// for every file it visits — free here by the same construction, again
/// Windows-only.
fn walk_files_with_size(dir: &Path, extensions: &[&str]) -> Vec<(PathBuf, u64)> {
    WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    extensions
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(ext))
                })
        })
        .map(|e| {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.into_path(), size)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Source;
    use crate::extract::about_xml::AboutXmlData;
    use std::fs;

    fn v16() -> GameVersion {
        GameVersion::new(1, 6)
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rim-analyzer-modscan-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn discovered_mod(path: PathBuf) -> DiscoveredMod {
        DiscoveredMod {
            id: ModId::new("test.mod"),
            path: path.clone(),
            source: Source::Local,
            generated: None,
            workshop_id: None,
            about: AboutXmlData {
                id: ModId::new("test.mod"),
                name: "Test Mod".to_string(),
                authors: Vec::new(),
                url: None,
                supported_versions: Vec::new(),
                declared: crate::domain::DeclaredOrder::default(),
            },
        }
    }

    fn paths(resolved: &[LoadedFolder]) -> Vec<PathBuf> {
        resolved.iter().map(|f| f.path.clone()).collect()
    }

    #[test]
    fn everything_policy_bypasses_load_folders_and_uses_mod_root() {
        let root = tempdir("everything-policy");
        let discovered = discovered_mod(root.clone());

        let (resolved, targets, warning) = resolve_loaded_folders(
            &discovered,
            None,
            v16(),
            &HashSet::new(),
            FolderPolicy::Everything,
        );

        assert_eq!(paths(&resolved), vec![root.clone()]);
        assert!(targets.is_empty());
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_load_folders_xml_falls_back_to_default_rule_on_disk() {
        let root = tempdir("default-rule-on-disk");
        fs::create_dir_all(root.join("1.6")).unwrap();
        let discovered = discovered_mod(root.clone());

        let (resolved, _, warning) = resolve_loaded_folders(
            &discovered,
            None,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            paths(&resolved),
            vec![root.join("1.6"), root.clone()],
            "the default folder rule's own construction order is already priority order"
        );
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn load_folders_xml_entries_are_filtered_to_folders_that_exist() {
        let root = tempdir("existence-filter");
        fs::create_dir_all(root.join("1.6").join("Compat")).unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li>/</li>
                <li>1.6/Compat</li>
                <li>DoesNotExist</li>
              </v1.6>
            </loadFolders>"#;

        let (resolved, _, warning) = resolve_loaded_folders(
            &discovered,
            Some(xml),
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            paths(&resolved),
            vec![root.join("1.6").join("Compat"), root.clone()],
            "priority order reverses the document order: the last-listed \
             `<li>` (here `1.6/Compat`) is the one an explicit LoadFolders.xml \
             block wins with for a same-relative-path file"
        );
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    /// The real-install shape this regression guards: one compat folder
    /// named by two different `<li IfModActive="…">`s in the same matched
    /// version block, both mods active so both gates pass. The engine's
    /// own `TryAdd`-keyed file dictionary loads the folder once regardless
    /// of how many `<li>`s name it — a naive dedup-free resolution instead
    /// marks every one of its own files shadowed by itself and the folder
    /// loads zero times (see [`dedup_folders_by_path`]'s own doc comment).
    #[test]
    fn load_folders_xml_dedups_a_folder_named_by_two_different_li_gates_that_both_pass() {
        let root = tempdir("dedup-two-passing-gates");
        fs::create_dir_all(root.join("Compat")).unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActive="mod.a">Compat</li>
                <li IfModActive="mod.b">Compat</li>
              </v1.6>
            </loadFolders>"#;
        let active = HashSet::from([ModId::new("mod.a"), ModId::new("mod.b")]);

        let (resolved, _, warning) = resolve_loaded_folders(
            &discovered,
            Some(xml),
            v16(),
            &active,
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            paths(&resolved),
            vec![root.join("Compat")],
            "the same physical folder named by two different, both-satisfied \
             IfModActive gates must appear once, not once per <li>"
        );
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    /// The other half of the same shape: only one of the two `<li>`s'
    /// gates is satisfied. `resolve_version_entry` already filters out the
    /// other one before this function ever sees it, so the dedup above is
    /// a no-op here — locked down so a future change to either function
    /// can't silently start dropping or doubling this folder.
    #[test]
    fn load_folders_xml_keeps_one_folder_when_only_one_of_two_li_gates_passes() {
        let root = tempdir("dedup-one-passing-gate");
        fs::create_dir_all(root.join("Compat")).unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActive="mod.a">Compat</li>
                <li IfModActive="missing.mod">Compat</li>
              </v1.6>
            </loadFolders>"#;
        let active = HashSet::from([ModId::new("mod.a")]);

        let (resolved, _, warning) = resolve_loaded_folders(
            &discovered,
            Some(xml),
            v16(),
            &active,
            FolderPolicy::LoadFolders,
        );

        assert_eq!(paths(&resolved), vec![root.join("Compat")]);
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_load_folders_xml_warns_and_falls_back_to_default_rule() {
        let root = tempdir("malformed-fallback");
        fs::create_dir_all(root.join("1.6")).unwrap();
        let discovered = discovered_mod(root.clone());

        let (resolved, _, warning) = resolve_loaded_folders(
            &discovered,
            Some(b"<NotLoadFolders/>"),
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(paths(&resolved), vec![root.join("1.6"), root.clone()]);
        assert!(warning.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_one_mod_carries_the_discovered_workshop_id_onto_the_resulting_mod() {
        let root = tempdir("workshop-id-threaded");
        let mut discovered = discovered_mod(root.clone());
        discovered.workshop_id = Some(2_009_463_077);

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(scanned.info.workshop_id, Some(2_009_463_077));
        let _ = fs::remove_dir_all(&root);
    }

    /// The side-loaded-assembly note actually reaches `Report.warnings`
    /// through `scan_one_mod` end to end — other tests call
    /// `side_loaded_assembly_note` directly; this drives the full function
    /// instead.
    #[test]
    fn scan_one_mod_surfaces_the_side_loaded_assembly_note_as_a_warning() {
        let root = tempdir("scan-one-mod-side-load-warning");
        fs::create_dir_all(root.join("Assemblies")).unwrap();
        fs::write(root.join("Assemblies").join("Loader.dll"), b"").unwrap();
        fs::create_dir_all(root.join("Lunar").join("Components")).unwrap();
        for name in ["Engine.dll", "Patches.dll", "Utils.dll"] {
            fs::write(root.join("Lunar").join("Components").join(name), b"").unwrap();
        }
        let discovered = discovered_mod(root.clone());

        let (_, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(
            warnings.iter().any(|w| w
                .message
                .contains("assemblies outside `Assemblies/` (Lunar/)")),
            "expected a side-loaded-assembly warning, got: {warnings:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_bounded_rejects_files_over_the_size_limit() {
        let root = tempdir("read-bounded");
        let path = root.join("small.txt");
        fs::write(&path, b"hello").unwrap();
        assert!(read_bounded(&path).is_ok());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_bounded_with_limit_reads_a_file_exactly_at_the_limit() {
        let root = tempdir("read-bounded-at-limit");
        let path = root.join("exact.txt");
        fs::write(&path, b"hello").unwrap();
        assert_eq!(read_bounded_with_limit(&path, 5).unwrap(), b"hello");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_bounded_with_limit_rejects_files_over_the_given_limit() {
        let root = tempdir("read-bounded-with-limit");
        let path = root.join("small.txt");
        fs::write(&path, b"hello").unwrap();
        assert!(matches!(
            read_bounded_with_limit(&path, 1),
            Err(ReadError::TooLarge { size: 5, limit: 1 })
        ));
        let _ = fs::remove_dir_all(&root);
    }

    /// Writes `count` empty `.dll` files under `dir` (created if needed) —
    /// content is irrelevant, `side_loaded_assembly_note` only looks at
    /// paths.
    fn write_dlls(dir: &Path, names: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        for name in names {
            fs::write(dir.join(name), b"").unwrap();
        }
    }

    #[test]
    fn a_loader_stub_with_a_bigger_side_loaded_folder_fires() {
        let root = tempdir("side-load-fires");
        write_dlls(&root.join("Assemblies"), &["Loader.dll"]);
        write_dlls(
            &root.join("Lunar").join("Components"),
            &["Engine.dll", "Patches.dll", "Utils.dll"],
        );

        let note = side_loaded_assembly_note(&root);

        assert_eq!(
            note.as_deref(),
            Some(
                "3 assemblies outside `Assemblies/` (Lunar/) were not analyzed \
                 — runtime patches and assembly references in them are not \
                 represented in this report."
            )
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_old_assemblies_rollback_folder_does_not_fire() {
        let root = tempdir("side-load-rollback-inert");
        write_dlls(&root.join("Assemblies"), &["Mod.dll", "Mod.pdb.dll"]);
        write_dlls(
            &root.join("OldAssemblies"),
            &["Mod.dll", "Mod.old.dll", "Mod.older.dll"],
        );

        assert!(side_loaded_assembly_note(&root).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_mod_with_only_source_and_obj_leftovers_does_not_fire() {
        // No `Assemblies/` folder at all here — just a `Source/` tree's
        // own `obj`/`bin` build output, the shape 103 real mods on the
        // user's install have (see this module's doc comment).
        let root = tempdir("side-load-dev-leftovers");
        write_dlls(&root.join("Source").join("obj").join("Debug"), &["Mod.dll"]);
        write_dlls(&root.join("Source").join("bin").join("Debug"), &["Mod.dll"]);

        assert!(side_loaded_assembly_note(&root).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn more_dlls_inside_assemblies_than_outside_does_not_fire() {
        let root = tempdir("side-load-inside-majority");
        write_dlls(&root.join("Assemblies"), &["A.dll", "B.dll", "C.dll"]);
        write_dlls(&root.join("Extra"), &["D.dll"]);

        assert!(side_loaded_assembly_note(&root).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn side_loaded_assembly_note_is_deterministic_across_runs() {
        // Meaningful, not vacuous: calling the same pure function twice on
        // unchanged input is always equal to itself, so the real determinism
        // claim worth pinning is the *offending-folder ordering* inside the
        // message — `Zeta` and `Alpha` are written in the "wrong"
        // (non-alphabetical, non-BTreeSet) order on purpose, so an
        // implementation that fell back to insertion or filesystem-walk order
        // here would print "(Zeta/, Alpha/, Middle/)" and fail this
        // exact-text assertion, not merely a `first == second`
        // self-comparison.
        let root = tempdir("side-load-determinism");
        write_dlls(&root.join("Assemblies"), &["Loader.dll"]);
        write_dlls(&root.join("Zeta").join("Components"), &["Z1.dll", "Z2.dll"]);
        write_dlls(
            &root.join("Alpha").join("Components"),
            &["A1.dll", "A2.dll"],
        );
        write_dlls(&root.join("Middle"), &["M1.dll"]);

        let first = side_loaded_assembly_note(&root);
        let second = side_loaded_assembly_note(&root);

        assert_eq!(first, second);
        assert_eq!(
            first.as_deref(),
            Some(
                "5 assemblies outside `Assemblies/` (Alpha/, Middle/, Zeta/) were not analyzed \
                 — runtime patches and assembly references in them are not represented in this \
                 report."
            )
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Tests the whole disk-to-report `Manifest.xml` wiring end to end — this
    /// drives `scan_one_mod` over a real `About/Manifest.xml` file, which
    /// would stay green if `read_manifest` looked at
    /// `<mod_root>/Manifest.xml` instead (missing the `About/` segment).
    #[test]
    fn scan_one_mod_reads_manifest_xml_from_the_about_folder() {
        let root = tempdir("manifest-xml-wiring");
        fs::create_dir_all(root.join("About")).unwrap();
        fs::write(
            root.join("About").join("Manifest.xml"),
            br#"<Manifest>
                  <loadAfter><li>some.other.mod</li></loadAfter>
                  <dependencies><li>example.patchlib</li></dependencies>
                </Manifest>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(
            scanned.manifest_order.load_after,
            vec!["some.other.mod".to_string()]
        );
        assert_eq!(
            scanned.manifest_order.dependencies,
            vec!["example.patchlib".to_string()]
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A mod with no `Manifest.xml` at all scans clean — no warning, an
    /// all-empty `ManifestOrder` (the common real shape: most mods don't
    /// ship this format).
    #[test]
    fn scan_one_mod_with_no_manifest_xml_is_silent() {
        let root = tempdir("manifest-xml-absent");
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(warnings.is_empty());
        assert_eq!(
            scanned.manifest_order,
            crate::domain::ManifestOrder::default()
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A malformed `Manifest.xml` warns (same convention as a malformed
    /// `LoadFolders.xml`) rather than aborting the scan or panicking.
    #[test]
    fn scan_one_mod_warns_on_a_malformed_manifest_xml() {
        let root = tempdir("manifest-xml-malformed");
        fs::create_dir_all(root.join("About")).unwrap();
        fs::write(root.join("About").join("Manifest.xml"), b"<NotManifest/>").unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            scanned.manifest_order,
            crate::domain::ManifestOrder::default()
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Manifest.xml"));
        let _ = fs::remove_dir_all(&root);
    }

    // -- texture path candidates ------------------------------------------

    /// A `texPath`-family field on a loaded def reaches
    /// `scanned.texture_path_candidates` end to end.
    #[test]
    fn scan_one_mod_collects_texture_path_candidates_from_loaded_defs() {
        let root = tempdir("texture-path-candidates-loaded");
        fs::create_dir_all(root.join("1.6").join("Defs")).unwrap();
        fs::write(
            root.join("1.6").join("Defs").join("Things.xml"),
            br#"<Defs>
                  <ThingDef>
                    <defName>Colonist</defName>
                    <texPath>Things/Pawn/Colonist</texPath>
                  </ThingDef>
                </Defs>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            scanned.texture_path_candidates,
            vec![crate::domain::TexturePathCandidate {
                def_type: "ThingDef".to_string(),
                def_name: "Colonist".to_string(),
                field: "texPath".to_string(),
                path: "Things/Pawn/Colonist".to_string(),
                graphic_class: None,
                // A `texPath` written directly under the def, not inside
                // a `<graphicData>` block: nothing for
                // `analysis::conflicts`' graphic-class chain walk to
                // attach to (see `TexturePathCandidate::container_tag`).
                container_tag: Some("ThingDef".to_string()),
            }]
        );
        let _ = fs::remove_dir_all(&root);
    }

    // -- ScanCost accumulation --------------------------------------------

    /// `ScannedMod::textures` is a `BTreeMap` precisely so a mod shipping
    /// both `Wall.png` and `Wall.dds` collapses to one normalized key whose
    /// value is the *sum* of both files' bytes.
    /// `ScanCost.texture_files`/`dds_files` stay raw (non-deduplicated)
    /// counts: 2 files, 1 of them `.dds`.
    #[test]
    fn scan_cost_collapses_png_and_dds_of_the_same_key_summing_their_bytes() {
        let root = tempdir("scan-cost-png-dds-collapse");
        fs::create_dir_all(root.join("1.6").join("Textures").join("Things")).unwrap();
        fs::write(
            root.join("1.6")
                .join("Textures")
                .join("Things")
                .join("Wall.png"),
            vec![0u8; 100],
        )
        .unwrap();
        fs::write(
            root.join("1.6")
                .join("Textures")
                .join("Things")
                .join("Wall.dds"),
            vec![0u8; 900],
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(scanned.scan_cost.texture_files, 2);
        assert_eq!(scanned.scan_cost.dds_files, 1);
        assert_eq!(scanned.scan_cost.texture_bytes, 1000);
        assert_eq!(scanned.textures.get("things/wall"), Some(&1000));
        let _ = fs::remove_dir_all(&root);
    }

    /// With no `LoadFolders.xml`, the default folder rule resolves *two*
    /// loaded folders for this fixture — the mod root and its `1.6/` version
    /// folder (see `no_load_folders_xml_falls_back_to_default_rule_on_disk`
    /// above) — so `scan_folder` runs twice, once per folder. `ScanCost` must
    /// accumulate `+=` across both calls, not overwrite: a regression to
    /// `out.scan_cost = ScanCost { .. }` on each call would leave only the
    /// *last*-scanned folder's counts, which this test pins.
    #[test]
    fn scan_cost_accumulates_texture_bytes_across_multiple_loaded_folders() {
        let root = tempdir("scan-cost-multi-folder");
        fs::create_dir_all(root.join("Textures")).unwrap();
        fs::create_dir_all(root.join("1.6").join("Textures")).unwrap();
        fs::write(root.join("Textures").join("Wall.png"), vec![0u8; 50]).unwrap();
        fs::write(
            root.join("1.6").join("Textures").join("Wall.png"),
            vec![0u8; 70],
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            scanned.scan_cost.texture_files, 2,
            "one file per loaded folder"
        );
        assert_eq!(scanned.scan_cost.texture_bytes, 120);
        assert_eq!(scanned.textures.get("wall"), Some(&120));
        let _ = fs::remove_dir_all(&root);
    }

    /// `ScanCost.assembly_bytes` is non-zero for a malformed/unparsable
    /// `.dll` — it still contributes its byte size, since the size rides
    /// `walk_files_with_size`'s own metadata read, independent of whether
    /// `pe_metadata::read` later succeeds.
    #[test]
    fn scan_cost_counts_assembly_bytes_even_when_the_dll_fails_to_parse() {
        let root = tempdir("scan-cost-assembly-bytes");
        fs::create_dir_all(root.join("1.6").join("Assemblies")).unwrap();
        fs::write(
            root.join("1.6").join("Assemblies").join("NotAssembly.dll"),
            vec![0u8; 37],
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(scanned.assemblies.len(), 1);
        assert!(scanned.assemblies[0].parse_failed);
        assert_eq!(scanned.scan_cost.assembly_bytes, 37);
        let _ = fs::remove_dir_all(&root);
    }

    // -- engine_enumeration_order (breadth-first, NTFS-collation-ordered) --

    /// Regression for the previous behaviour, which sorted every file under
    /// a directory together regardless of depth (`WalkDir::sort_by_file_name`
    /// over the whole recursive tree) — a subdirectory file named earlier
    /// alphabetically than a root file would have come first. The real
    /// engine's own directory enumeration (`FileSystemEnumerator<T>.MoveNext`)
    /// always exhausts one directory's own matches before descending, so a
    /// root file must sort before every subdirectory file regardless of
    /// name.
    #[test]
    fn engine_enumeration_order_root_files_come_before_any_subdirectory_file() {
        let root = tempdir("engine-order-root-first");
        fs::create_dir_all(root.join("Sub")).unwrap();
        fs::write(root.join("ZZZ.xml"), b"").unwrap();
        fs::write(root.join("Sub").join("AAA.xml"), b"").unwrap();

        let order = engine_enumeration_order(&root, &["xml"]);

        assert_eq!(
            order,
            vec![root.join("ZZZ.xml"), root.join("Sub").join("AAA.xml")]
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Two sibling subdirectories are both fully visited before either
    /// one's own subdirectory — a level-order (breadth-first) walk, not a
    /// depth-first one.
    #[test]
    fn engine_enumeration_order_subdirectories_are_visited_breadth_first() {
        let root = tempdir("engine-order-bfs");
        fs::create_dir_all(root.join("A").join("Deep")).unwrap();
        fs::create_dir_all(root.join("B")).unwrap();
        fs::write(root.join("A").join("Shallow.xml"), b"").unwrap();
        fs::write(root.join("A").join("Deep").join("Deepest.xml"), b"").unwrap();
        fs::write(root.join("B").join("AlsoShallow.xml"), b"").unwrap();

        let order = engine_enumeration_order(&root, &["xml"]);

        assert_eq!(
            order,
            vec![
                root.join("A").join("Shallow.xml"),
                root.join("B").join("AlsoShallow.xml"),
                root.join("A").join("Deep").join("Deepest.xml"),
            ],
            "both level-1 directories' own files must appear before the level-2 file"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// `._`-prefixed macOS resource-fork leftovers (and any other
    /// dot-prefixed file name) are never read, matching
    /// `DirectXmlLoader.XmlAssetsInModFolder`'s own filter.
    #[test]
    fn engine_enumeration_order_dotfiles_are_skipped() {
        let root = tempdir("engine-order-dotfiles");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Real.xml"), b"").unwrap();
        fs::write(root.join("._Real.xml"), b"").unwrap();
        fs::write(root.join(".hidden.xml"), b"").unwrap();

        let order = engine_enumeration_order(&root, &["xml"]);

        assert_eq!(order, vec![root.join("Real.xml")]);
        let _ = fs::remove_dir_all(&root);
    }

    // -- priority order and its downstream consequences ----------------------

    /// `scan_one_mod` walks loaded folders in **priority** order (highest
    /// first), not document order — the last-listed `<li>` in an explicit
    /// `LoadFolders.xml` block wins priority (`ModContentPack
    /// .InitLoadFolders`'s own reversal), so its own `Patches/` ops must
    /// appear first in `scanned.patch_ops`. Regression for the previous
    /// behaviour, which scanned folders in document order.
    #[test]
    fn scan_one_mod_scans_patch_ops_in_priority_folder_order() {
        let root = tempdir("priority-order-patch-ops");
        fs::create_dir_all(root.join("First").join("Patches")).unwrap();
        fs::create_dir_all(root.join("Second").join("Patches")).unwrap();
        fs::write(
            root.join("First").join("Patches").join("A.xml"),
            br#"<Patch><Operation Class="PatchOperationTest"><xpath>from.first</xpath></Operation></Patch>"#,
        )
        .unwrap();
        fs::write(
            root.join("Second").join("Patches").join("B.xml"),
            br#"<Patch><Operation Class="PatchOperationTest"><xpath>from.second</xpath></Operation></Patch>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li>First</li>
                <li>Second</li>
              </v1.6>
            </loadFolders>"#;
        fs::write(root.join("LoadFolders.xml"), xml).unwrap();

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        let xpaths: Vec<_> = scanned
            .patch_ops
            .iter()
            .map(|op| op.xpath.as_deref())
            .collect();
        assert_eq!(
            xpaths,
            vec![Some("from.second"), Some("from.first")],
            "the last-listed <li> (Second) is highest priority and must scan first"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A `LoadFolders.xml` folder's own `IfModActive`/`IfModActiveAll` gate
    /// is stamped onto every `PatchOp` that folder's own `Patches/` walk
    /// finds — recorded now for a later compat-folder exclusion, unused by
    /// any gating logic yet.
    #[test]
    fn scan_one_mod_stamps_each_ops_own_folders_if_mod_active_gate() {
        let root = tempdir("load-folder-gate-stamp");
        fs::create_dir_all(root.join("Compat").join("Patches")).unwrap();
        fs::write(
            root.join("Compat").join("Patches").join("A.xml"),
            br#"<Patch><Operation Class="PatchOperationTest"><xpath>gated</xpath></Operation></Patch>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActive="some.framework">Compat</li>
              </v1.6>
            </loadFolders>"#;
        fs::write(root.join("LoadFolders.xml"), xml).unwrap();
        let mut active_bases = HashSet::new();
        active_bases.insert(ModId::new("some.framework"));

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &active_bases,
            FolderPolicy::LoadFolders,
        );

        assert_eq!(scanned.patch_ops.len(), 1);
        assert_eq!(
            scanned.patch_ops[0].load_folder_gate,
            vec![ModId::new("some.framework")]
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Two loaded folders each shipping `Defs/Same.xml` with the identical
    /// `defName` — a shape that can only arise across folders with
    /// distinct `Defs/` subdirectories (the shadow rule already dedupes an
    /// identical relative path within `Defs/`), pinning that `scanned.defs`
    /// preserves scan (priority) order so a downstream "first wins" rule
    /// can rely on it.
    #[test]
    fn scan_one_mod_lists_defs_in_priority_folder_order() {
        let root = tempdir("priority-order-defs");
        fs::create_dir_all(root.join("First").join("Defs")).unwrap();
        fs::create_dir_all(root.join("Second").join("Defs")).unwrap();
        fs::write(
            root.join("First").join("Defs").join("A.xml"),
            br#"<Defs><ThingDef><defName>FromFirst</defName></ThingDef></Defs>"#,
        )
        .unwrap();
        fs::write(
            root.join("Second").join("Defs").join("B.xml"),
            br#"<Defs><ThingDef><defName>FromSecond</defName></ThingDef></Defs>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li>First</li>
                <li>Second</li>
              </v1.6>
            </loadFolders>"#;
        fs::write(root.join("LoadFolders.xml"), xml).unwrap();

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        let names: Vec<_> = scanned.defs.iter().map(|d| d.def_name.as_str()).collect();
        assert_eq!(names, vec!["FromSecond", "FromFirst"]);
        let _ = fs::remove_dir_all(&root);
    }

    /// The same relative `Assemblies/` path shipped by two loaded folders
    /// deduplicates to the highest-priority folder's own copy — the
    /// "duplicate DLL in root and a version folder doesn't count twice".
    #[test]
    fn scan_one_mod_deduplicates_same_relative_path_assemblies_keeping_the_highest_priority_copy() {
        let root = tempdir("assembly-dedup");
        fs::create_dir_all(root.join("1.6").join("Assemblies")).unwrap();
        fs::create_dir_all(root.join("Assemblies")).unwrap();
        // Distinct byte sizes so the surviving copy is identifiable.
        fs::write(
            root.join("1.6").join("Assemblies").join("Mod.dll"),
            vec![0u8; 11],
        )
        .unwrap();
        fs::write(root.join("Assemblies").join("Mod.dll"), vec![0u8; 22]).unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        // Default folder rule: version folder (1.6) is higher priority than
        // the mod root.
        assert_eq!(
            scanned.assemblies.len(),
            1,
            "the root copy must be shadowed"
        );
        assert_eq!(scanned.scan_cost.assembly_bytes, 11);
        let _ = fs::remove_dir_all(&root);
    }

    /// End-to-end regression for the "folder listed twice loads zero
    /// times" bug: a folder named by two different, both-satisfied
    /// `IfModActive` gates must still contribute its own def exactly
    /// once, not zero times (the bug) and not twice.
    #[test]
    fn scan_one_mod_scans_a_folder_named_by_two_passing_gates_exactly_once() {
        let root = tempdir("dedup-scan-both-gates-pass");
        fs::create_dir_all(root.join("Compat").join("Defs")).unwrap();
        fs::write(
            root.join("Compat").join("Defs").join("A.xml"),
            br#"<Defs><ThingDef><defName>FromCompat</defName></ThingDef></Defs>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActive="mod.a">Compat</li>
                <li IfModActive="mod.b">Compat</li>
              </v1.6>
            </loadFolders>"#;
        fs::write(root.join("LoadFolders.xml"), xml).unwrap();
        let active = HashSet::from([ModId::new("mod.a"), ModId::new("mod.b")]);

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &active,
            FolderPolicy::LoadFolders,
        );

        let names: Vec<_> = scanned.defs.iter().map(|d| d.def_name.as_str()).collect();
        assert_eq!(names, vec!["FromCompat"]);
        let _ = fs::remove_dir_all(&root);
    }

    /// The single-passing-gate case: the def still loads exactly once.
    #[test]
    fn scan_one_mod_scans_a_folder_named_by_two_gates_when_only_one_passes() {
        let root = tempdir("dedup-scan-one-gate-passes");
        fs::create_dir_all(root.join("Compat").join("Defs")).unwrap();
        fs::write(
            root.join("Compat").join("Defs").join("A.xml"),
            br#"<Defs><ThingDef><defName>FromCompat</defName></ThingDef></Defs>"#,
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActive="mod.a">Compat</li>
                <li IfModActive="missing.mod">Compat</li>
              </v1.6>
            </loadFolders>"#;
        fs::write(root.join("LoadFolders.xml"), xml).unwrap();
        let active = HashSet::from([ModId::new("mod.a")]);

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &active,
            FolderPolicy::LoadFolders,
        );

        let names: Vec<_> = scanned.defs.iter().map(|d| d.def_name.as_str()).collect();
        assert_eq!(names, vec!["FromCompat"]);
        let _ = fs::remove_dir_all(&root);
    }

    fn write_bundle(dir: &Path, name: &str, manifest_entries: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), vec![0u8; 4]).unwrap();
        let manifest = format!(
            "ManifestFileVersion: 0\nAssets:\n{}\nDependencies: {{}}\n",
            manifest_entries
                .iter()
                .map(|e| format!("- {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        fs::write(dir.join(format!("{name}.manifest")), manifest).unwrap();
    }

    #[test]
    fn mac_and_linux_suffixed_bundles_are_ignored_on_windows() {
        let root = tempdir("bundle-os-suffix");
        let bundles = root.join("AssetBundles");
        write_bundle(
            &bundles,
            "things_win",
            &["Assets/Data/test.mod/Textures/Win.png"],
        );
        write_bundle(
            &bundles,
            "things_mac",
            &["Assets/Data/test.mod/Textures/Mac.png"],
        );
        write_bundle(
            &bundles,
            "things_linux",
            &["Assets/Data/test.mod/Textures/Linux.png"],
        );
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            scanned.bundle_textures,
            std::collections::BTreeSet::from(["win".to_string()])
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_manifest_without_a_bundle_file_contributes_nothing() {
        let root = tempdir("bundle-manifest-orphan");
        let bundles = root.join("AssetBundles");
        fs::create_dir_all(&bundles).unwrap();
        // A `.manifest` sidecar with no matching extensionless bundle file
        // beside it — never itself walked as a candidate bundle (it has an
        // extension), so it's silently ignored rather than read.
        fs::write(
            bundles.join("orphan.manifest"),
            "Assets:\n- Assets/Data/test.mod/Textures/Orphan.png\n",
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(scanned.bundle_textures.is_empty());
        assert!(warnings.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bundle_with_no_manifest_warns_and_contributes_nothing() {
        let root = tempdir("bundle-no-manifest");
        let bundles = root.join("AssetBundles");
        fs::create_dir_all(&bundles).unwrap();
        fs::write(bundles.join("standalone"), vec![0u8; 4]).unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, warnings) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(scanned.bundle_textures.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("no manifest"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn higher_priority_folder_bundle_shadows_lower_priority_same_path() {
        let root = tempdir("bundle-shadow");
        // Default folder rule: version folder (1.6) is higher priority than
        // the mod root — same convention the assembly-dedup test above uses.
        write_bundle(
            &root.join("1.6").join("AssetBundles"),
            "things",
            &["Assets/Data/test.mod/Textures/Winner.png"],
        );
        write_bundle(
            &root.join("AssetBundles"),
            "things",
            &["Assets/Data/test.mod/Textures/Loser.png"],
        );
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(
            scanned.bundle_textures,
            std::collections::BTreeSet::from(["winner".to_string()]),
            "the root copy's manifest must be shadowed, same as Defs/Patches/Assemblies"
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn dds_bytes(undecodable: bool) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[0..4].copy_from_slice(b"DDS ");
        header[4..8].copy_from_slice(&124u32.to_le_bytes());
        let (width, height): (u32, u32) = if undecodable { (65, 64) } else { (64, 64) };
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        header[80..84].copy_from_slice(&0x4u32.to_le_bytes()); // DDPF_FOURCC
        header[84..88].copy_from_slice(b"DXT5");
        header
    }

    #[test]
    fn only_the_winning_copy_of_a_shadowed_dds_is_checked() {
        let root = tempdir("dds-shadow");
        // The winning (1.6) copy decodes fine; the shadowed root copy would
        // fail the header check if it were ever read.
        fs::create_dir_all(root.join("1.6").join("Textures")).unwrap();
        fs::write(
            root.join("1.6").join("Textures").join("Wall.dds"),
            dds_bytes(false),
        )
        .unwrap();
        fs::create_dir_all(root.join("Textures")).unwrap();
        fs::write(root.join("Textures").join("Wall.dds"), dds_bytes(true)).unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert!(
            scanned.undecodable_textures.is_empty(),
            "only the winning, decodable copy should ever be read"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn png_sibling_is_recorded() {
        let root = tempdir("dds-png-sibling");
        fs::create_dir_all(root.join("Textures").join("Things")).unwrap();
        fs::write(
            root.join("Textures").join("Things").join("Wall.dds"),
            dds_bytes(true),
        )
        .unwrap();
        fs::write(
            root.join("Textures").join("Things").join("Wall.png"),
            vec![0u8; 4],
        )
        .unwrap();
        let discovered = discovered_mod(root.clone());

        let (scanned, _, _, _) = scan_one_mod(
            &ModId::new("test.mod"),
            &discovered,
            v16(),
            &HashSet::new(),
            FolderPolicy::LoadFolders,
        );

        assert_eq!(scanned.undecodable_textures.len(), 1);
        assert!(scanned.undecodable_textures[0].has_png_sibling);
        let _ = fs::remove_dir_all(&root);
    }
}
