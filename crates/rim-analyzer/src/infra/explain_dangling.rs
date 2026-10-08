//! The lazy IO half of the dangling-def-reference check
//! (`analysis::references` is the pure half — see that module's own doc
//! comment). Runs **only** for the names that pure pass left
//! [`DanglingCause::Unexplained`] — several hundred on a
//! real install: it byte-searches each candidate file for `>NAME<` before
//! any parse, then confirms with a real `defName` parse, so a name that
//! merely *resembles* another mod's content never gets misattributed.
//!
//! Two candidate sources:
//! - each active mod's own *unloaded* loaded-folder candidates (any
//!   `Defs` directory anywhere under its own root whose parent isn't one
//!   of its [`crate::domain::Mod::loaded_folders`]) — a mod shipping
//!   content in a version or `IfModActive`-gated folder the running
//!   install never actually loads;
//! - every installed-but-inactive mod's own `Defs/`.
//!
//! Bounded by [`MAX_EXPLAIN_BYTES`] across the *whole* pass, not per
//! name — once the budget runs out, every name still pending stays
//! `Unexplained` rather than guessing. A name the search completes in
//! full without finding anywhere becomes [`DanglingCause::DefinedNowhere`]
//! — a different, stronger claim than `Unexplained` ("proven absent" vs.
//! "not proven either way").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::analysis::{self, RunContext};
use crate::domain::{Conflict, DanglingCause, Report, ScanOutput, ScannedMod};
use crate::extract::defs;

use super::mod_scan;

/// Total bytes read across the whole explain pass — a defensive bound,
/// not a measured real-install size (512 MiB).
const MAX_EXPLAIN_BYTES: u64 = 512 * 1024 * 1024;

/// How many candidate files one parallel read-ahead batch holds at most,
/// and how many bytes (a larger file still gets a batch of its own). The
/// files of one batch are read and searched in parallel, but their
/// outcomes are applied sequentially, in candidate order, so which source
/// explains a name never depends on timing; the bounds also cap how much
/// work past an exhausted budget or an emptied pending set a batch does.
const READ_AHEAD_FILES: usize = 1024;
const READ_AHEAD_BYTES: u64 = 32 * 1024 * 1024;

/// Mutates every [`Conflict::DanglingDefReference`] in `conflicts` whose
/// own `cause` is [`DanglingCause::Unexplained`], upgrading it where the
/// filesystem search below can explain it — see this module's own doc
/// comment. `scan` must be the same [`ScanOutput`] the report was built
/// from (`analysis::build_ref`'s own borrow, still alive afterward —
/// never `analysis::build`, which consumes it).
///
/// The standalone entry point, for tests and callers that already hold a
/// built report; production goes through [`build_explained_report`], which
/// overlaps the candidate walk with the report build.
pub fn explain_dangling_references(conflicts: &mut [Conflict], scan: &ScanOutput) {
    // Deliberate duplicate of the pending-name collection inside
    // `explain_within_budget`: it lets the common nothing-to-explain case
    // return before `CandidateFiles::collect` walks every active mod's tree.
    if PendingNames::collect(conflicts).is_empty() {
        return;
    }
    explain_within_budget(conflicts, &CandidateFiles::collect(scan), MAX_EXPLAIN_BYTES);
}

/// [`analysis::build_ref`] followed by [`explain_dangling_references`],
/// the report every composition root builds from a finished scan.
///
/// The explanation's candidate-folder walk (every active mod's whole tree,
/// over a second on a real install) needs only `scan`, so it runs on the
/// rayon pool while `build_ref` runs on the calling thread; only the
/// search itself waits for the report's dangling names. The trade-off
/// against calling the two in sequence: the walk happens even when the
/// report turns out to have no unexplained name, which on an install
/// small enough for that to be likely is also a cheap walk.
#[must_use]
pub fn build_explained_report(scan: &ScanOutput, context: &RunContext) -> Report {
    let (mut report, candidates) = rayon::join(
        || analysis::build_ref(scan, context),
        || CandidateFiles::collect(scan),
    );
    explain_within_budget(&mut report.conflicts, &candidates, MAX_EXPLAIN_BYTES);
    report
}

/// The search behind [`explain_dangling_references`], over already
/// collected `candidates` and with an explicit byte budget.
fn explain_within_budget(conflicts: &mut [Conflict], candidates: &CandidateFiles, mut budget: u64) {
    let mut pending = PendingNames::collect(conflicts);
    if pending.is_empty() {
        return;
    }

    for batch in read_ahead_batches(&candidates.files) {
        // Each file of the batch is read, byte-searched and parsed on its
        // own worker, against the names still pending when the batch
        // starts. The outcomes are then applied one file at a time, in
        // candidate order, exactly as a sequential search would: a name
        // explained by an earlier file is no longer pending when a later
        // file's outcome is applied, so which source explains a name never
        // depends on timing.
        let outcomes: Vec<Option<Vec<String>>> = batch
            .par_iter()
            .map(|file| defined_pending_names(&file.path, &pending.by_name))
            .collect();
        for (file, defined) in batch.iter().zip(outcomes) {
            if pending.is_empty() {
                return;
            }
            if file.len > budget {
                // Budget exhausted: every name still pending stays
                // `Unexplained` — not proven either way.
                return;
            }
            let Some(defined) = defined else {
                // A read failure (permission error, the file vanishing
                // mid-scan, or it exceeding the bounded-read limit) means
                // the search never actually examined this file's own
                // content — treat it exactly like running out of budget:
                // stop here rather than let the search "complete" having
                // silently skipped a candidate, which would risk declaring
                // a name `DefinedNowhere` (proven absent) when it might be
                // sitting, unread, in this very file. Every name still
                // pending stays `Unexplained`, the honest "not proven
                // either way" answer.
                return;
            };
            budget -= file.len;
            pending.explain(defined, &candidates.causes[file.cause]);
        }
    }

    // The search completed in full for every remaining name without
    // finding it anywhere — proven absent, not merely unproven.
    pending.mark_defined_nowhere();
}

/// Which of `pending`'s names the file at `path` really defines: a
/// byte search for `>NAME<` for every name first, then a real `defName`
/// parse to confirm the hits. `None` when the file can't be read; an
/// empty list when nothing matches or the file doesn't parse.
fn defined_pending_names(path: &Path, pending: &BTreeMap<String, usize>) -> Option<Vec<String>> {
    let bytes = mod_scan::read_bounded(path).ok()?;
    let hits = names_in_tag_text(&bytes, pending);
    if hits.is_empty() {
        return Some(Vec::new());
    }
    let file: std::sync::Arc<Path> = std::sync::Arc::from(path);
    let Ok(defs_file) = defs::index(&bytes, &file) else {
        return Some(Vec::new());
    };
    Some(
        hits.into_iter()
            .filter(|name| defs_file.defs.iter().any(|d| d.def_name == *name))
            .collect(),
    )
}

/// Every still-unexplained dangling name, mapped to its own conflict slot.
struct PendingNames<'a> {
    by_name: BTreeMap<String, usize>,
    conflicts: &'a mut [Conflict],
}

impl<'a> PendingNames<'a> {
    fn collect(conflicts: &'a mut [Conflict]) -> Self {
        let mut by_name = BTreeMap::new();
        for (index, conflict) in conflicts.iter().enumerate() {
            if let Conflict::DanglingDefReference(d) = conflict
                && d.cause == DanglingCause::Unexplained
            {
                by_name.insert(d.name.clone(), index);
            }
        }
        Self { by_name, conflicts }
    }

    fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Gives every name in `defined` that is still pending `cause`, and
    /// stops it pending. A name an earlier file already explained is no
    /// longer pending and keeps that earlier cause.
    fn explain(&mut self, defined: Vec<String>, cause: &DanglingCause) {
        for name in defined {
            if let Some(index) = self.by_name.remove(&name)
                && let Conflict::DanglingDefReference(d) = &mut self.conflicts[index]
            {
                d.cause = cause.clone();
            }
        }
    }

    fn mark_defined_nowhere(self) {
        for index in self.by_name.into_values() {
            if let Conflict::DanglingDefReference(d) = &mut self.conflicts[index] {
                d.cause = DanglingCause::DefinedNowhere;
            }
        }
    }
}

/// Every candidate `Defs/**/*.xml` file, in search order: each active mod's
/// own unloaded folders first, then every inactive mod.
struct CandidateFiles {
    files: Vec<CandidateFile>,
    /// The cause a name found in a file is explained by, indexed by
    /// [`CandidateFile::cause`] — one per searched folder.
    causes: Vec<DanglingCause>,
}

struct CandidateFile {
    path: PathBuf,
    len: u64,
    cause: usize,
}

/// How many directory levels deep [`unloaded_defs_folders`]'s search for
/// an unloaded `Defs` folder descends under one mod's own root — generous
/// over any real compat-framework nesting convention (e.g.
/// `1.6/Mods/X/Defs`), while still bounding the walk against a
/// pathological directory tree.
const MAX_UNLOADED_DEFS_SEARCH_DEPTH: usize = 12;

/// One searched `Defs` folder: every `**/*.xml` file under it (path and
/// size, sorted), all explained by the same `cause`.
struct DefsFolder {
    cause: DanglingCause,
    files: Vec<(PathBuf, u64)>,
}

impl CandidateFiles {
    /// Walking every active mod's whole tree for unloaded `Defs` folders is
    /// the costly part of this pass (a thousand mods' worth of `Textures/`
    /// entries), so each mod (and each inactive mod) is walked on its own
    /// rayon worker. `collect` on an indexed parallel iterator keeps input
    /// order, and every walk is itself sorted, so the search order — and
    /// with it which source explains a name — is identical to a
    /// sequential walk.
    fn collect(scan: &ScanOutput) -> Self {
        let unloaded: Vec<Vec<DefsFolder>> = scan
            .scanned_mods
            .par_iter()
            .map(unloaded_defs_folders)
            .collect();
        let inactive: Vec<Option<DefsFolder>> = scan
            .inactive_mods
            .par_iter()
            .map(|inactive| {
                inactive_defs_folder(
                    &inactive.path,
                    DanglingCause::OnlyInInactiveMod {
                        mod_id: inactive.id.clone(),
                    },
                )
            })
            .collect();

        let mut candidates = Self {
            files: Vec::new(),
            causes: Vec::new(),
        };
        let folders = unloaded
            .into_iter()
            .flatten()
            .chain(inactive.into_iter().flatten());
        for folder in folders {
            candidates.push_folder(folder);
        }
        candidates
    }

    fn push_folder(&mut self, folder: DefsFolder) {
        let cause_index = self.causes.len();
        self.causes.push(folder.cause);
        self.files
            .extend(folder.files.into_iter().map(|(path, len)| CandidateFile {
                path,
                len,
                cause: cause_index,
            }));
    }
}

/// Every `Defs` directory anywhere under `scanned_mod`'s own root whose
/// direct parent is **not** one of its own
/// [`Mod::loaded_folders`](crate::domain::Mod::loaded_folders) — at any
/// depth, not only the mod's immediate subdirectories: a compat framework
/// can gate content behind an extra nesting level (`1.6/Mods/X/Defs`), and
/// the mod's own root `Defs/` counts too when `LoadFolders.xml` doesn't
/// include `/` for the running version (the old shallow, one-level scan
/// could only ever find a `Defs` folder nested *inside* an unloaded
/// top-level entry, never the mod's own root, and never anything nested
/// two or more levels deep).
fn unloaded_defs_folders(scanned_mod: &ScannedMod) -> Vec<DefsFolder> {
    let mod_root = &scanned_mod.info.path;
    let unloaded_defs_dirs = WalkDir::new(mod_root)
        .max_depth(MAX_UNLOADED_DEFS_SEARCH_DEPTH)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir() && e.file_name().eq_ignore_ascii_case("Defs"));
    let mut folders = Vec::new();
    for entry in unloaded_defs_dirs {
        let defs_dir = entry.path();
        let Some(parent) = defs_dir.parent() else {
            continue;
        };
        if scanned_mod
            .info
            .loaded_folders
            .iter()
            .any(|folder| folder == parent)
        {
            continue;
        }
        let folder = parent
            .strip_prefix(mod_root)
            .ok()
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .filter(|rel| !rel.is_empty())
            .unwrap_or_else(|| ".".to_string());
        folders.push(DefsFolder {
            cause: DanglingCause::OnlyInUnloadedFolder {
                mod_id: scanned_mod.info.id.clone(),
                folder,
            },
            files: xml_files_in_defs_dir(defs_dir),
        });
    }
    folders
}

/// The `Defs` folder directly under `mod_root`, explained by `cause` — used
/// for an inactive mod, whose own `Defs` is always exactly one level below
/// its root (an inactive mod is never scanned for `LoadFolders.xml`, so it
/// has no notion of "unloaded" nested folders of its own to search).
fn inactive_defs_folder(mod_root: &Path, cause: DanglingCause) -> Option<DefsFolder> {
    let defs_dir = mod_root.join("Defs");
    if !defs_dir.is_dir() {
        return None;
    }
    Some(DefsFolder {
        cause,
        files: xml_files_in_defs_dir(&defs_dir),
    })
}

/// Every `**/*.xml` file under an already-located `Defs` directory, with
/// its size, in a deterministic (sorted) order.
fn xml_files_in_defs_dir(defs_dir: &Path) -> Vec<(PathBuf, u64)> {
    WalkDir::new(defs_dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
        })
        .filter_map(|entry| {
            let len = entry.metadata().ok()?.len();
            Some((entry.into_path(), len))
        })
        .collect()
}

/// `files` split into consecutive read-ahead batches — see
/// [`READ_AHEAD_FILES`].
fn read_ahead_batches(files: &[CandidateFile]) -> Vec<&[CandidateFile]> {
    let mut batches = Vec::new();
    let mut start = 0;
    while start < files.len() {
        let mut end = start + 1;
        let mut bytes = files[start].len;
        while end < files.len()
            && end - start < READ_AHEAD_FILES
            && bytes.saturating_add(files[end].len) <= READ_AHEAD_BYTES
        {
            bytes += files[end].len;
            end += 1;
        }
        batches.push(&files[start..end]);
        start = end;
    }
    batches
}

/// Every name in `pending` that `haystack` contains as the literal bytes
/// `>{name}<` (see [`contains_tag_text`]), in `pending`'s own key order.
///
/// One pass over `haystack` answers every name at once: for a name with
/// no `<` of its own, `>{name}<` occurs at a `>` exactly when the bytes up
/// to the *next* `<` spell the name, so each `>`-to-`<` run is looked up
/// in `pending` directly. Searching name by name instead cost
/// `bytes x names` — several minutes on a real install, where hundreds of
/// names stay pending across every unloaded folder and inactive mod. The
/// rare name containing `<` keeps the per-name search.
fn names_in_tag_text(haystack: &[u8], pending: &BTreeMap<String, usize>) -> Vec<String> {
    let longest_name = pending.keys().map(String::len).max().unwrap_or_default();
    let mut found: BTreeSet<&str> = pending
        .keys()
        .filter(|name| name.contains('<') && contains_tag_text(haystack, name))
        .map(String::as_str)
        .collect();

    let mut next_open = 0;
    for (close, _) in haystack.iter().enumerate().filter(|(_, b)| **b == b'>') {
        let text_start = close + 1;
        if next_open < text_start {
            let Some(offset) = haystack[text_start..].iter().position(|b| *b == b'<') else {
                break;
            };
            next_open = text_start + offset;
        }
        let text = &haystack[text_start..next_open];
        if text.len() <= longest_name
            && let Ok(text) = std::str::from_utf8(text)
            && let Some((name, _)) = pending.get_key_value(text)
        {
            found.insert(name.as_str());
        }
    }
    found.into_iter().map(str::to_string).collect()
}

/// A cheap pre-filter: whether `haystack` contains the literal bytes
/// `>{name}<` — the exact shape a `<defName>NAME</defName>` (or, just as
/// usefully, a `<ThingDef Name="NAME">` sibling text run) leaf produces.
/// Run before the real parse below, never trusted on its own.
fn contains_tag_text(haystack: &[u8], name: &str) -> bool {
    let mut needle = Vec::with_capacity(name.len() + 2);
    needle.push(b'>');
    needle.extend_from_slice(name.as_bytes());
    needle.push(b'<');
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_slice())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::fs;
    use std::path::PathBuf;

    use crate::domain::{
        DanglingDefReference, DeclaredOrder, InactiveMod, LoadOrder, Mod, ModId, ScanCost,
        ScannedMod, Source,
    };

    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rim-analyzer-explain-dangling-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    fn base_mod(id: &str, path: PathBuf) -> ScannedMod {
        ScannedMod {
            info: Mod {
                id: ModId::new(id),
                name: id.to_string(),
                authors: Vec::new(),
                url: None,
                path,
                source: Source::Local,
                supported_versions: Vec::new(),
                declared: DeclaredOrder::default(),
                loaded_folders: Vec::new(),
                hard_dependents: 0,
                soft_dependents: 0,
                awareness_dependents: 0,
                is_framework_candidate: false,
                generated: None,
                workshop_id: None,
                load_folders_version_matched: None,
            },
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            sounds: BTreeSet::new(),
            translation_keys: BTreeSet::new(),
            inline_types: BTreeSet::new(),
            manifest_order: crate::domain::ManifestOrder::default(),
            inline_node_path_hashes: HashSet::new(),
            assemblies: Vec::new(),
            if_mod_active_targets: Vec::new(),
            texture_path_candidates: Vec::new(),
            scan_cost: ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: BTreeSet::new(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    fn empty_scan(
        scanned_mods: Vec<ScannedMod>,
        inactive_mods: Vec<crate::domain::InactiveMod>,
    ) -> ScanOutput {
        let load_order = LoadOrder::new(scanned_mods.iter().map(|m| m.info.id.clone()).collect());
        let discovered_mod_count = scanned_mods.len() + inactive_mods.len();
        ScanOutput {
            scanned_mods,
            load_order,
            missing_mods: Vec::new(),
            vanilla_assembly_names: HashSet::new(),
            vanilla_type_hierarchy: Vec::new(),
            warnings: Vec::new(),
            child_value_hashes_by_mod: BTreeMap::new(),
            inactive_mods,
            discovered_mod_count,
            core_resource_textures: BTreeSet::new(),
            ref_sites_by_mod: BTreeMap::new(),
        }
    }

    fn unexplained_conflict(name: &str) -> Conflict {
        Conflict::DanglingDefReference(DanglingDefReference {
            name: name.to_string(),
            referrers: Vec::new(),
            truncated_referrers: 0,
            cause: DanglingCause::Unexplained,
            likely_sound: false,
        })
    }

    fn cause_of(conflicts: &[Conflict], name: &str) -> DanglingCause {
        conflicts
            .iter()
            .find_map(|c| match c {
                Conflict::DanglingDefReference(d) if d.name == name => Some(d.cause.clone()),
                _ => None,
            })
            .expect("conflict present")
    }

    #[test]
    fn name_defined_only_in_an_unloaded_version_folder_is_explained() {
        let mod_dir = tempdir("unloaded-folder");
        let loaded = mod_dir.join("1.6");
        let unloaded = mod_dir.join("1.6NotOdyssey");
        fs::create_dir_all(loaded.join("Defs")).unwrap();
        fs::create_dir_all(unloaded.join("Defs")).unwrap();
        fs::write(
            unloaded.join("Defs").join("Ghost.xml"),
            r#"<Defs><ThingDef><defName>GhostDef</defName></ThingDef></Defs>"#,
        )
        .unwrap();

        let mut this_mod = base_mod("definer", mod_dir);
        this_mod.info.loaded_folders = vec![loaded];
        let scan = empty_scan(vec![this_mod], Vec::new());

        let mut conflicts = vec![unexplained_conflict("GhostDef")];
        explain_dangling_references(&mut conflicts, &scan);

        assert!(matches!(
            cause_of(&conflicts, "GhostDef"),
            DanglingCause::OnlyInUnloadedFolder { ref mod_id, ref folder }
                if mod_id.as_str() == "definer" && folder == "1.6NotOdyssey"
        ));

        fs::remove_dir_all(scan.scanned_mods[0].info.path.clone()).ok();
    }

    /// Regression: the old shallow, one-level scan could only ever find a
    /// `Defs` folder directly inside an unloaded top-level entry, never
    /// one nested two or more levels deep — a real compat-framework
    /// convention (`1.6/Mods/X/Defs`). `1.6` itself here *is* loaded, so
    /// this exercises the deeper, gated nesting specifically, not merely
    /// an unloaded top-level folder.
    #[test]
    fn name_defined_only_in_a_nested_gated_folder_is_explained() {
        let mod_dir = tempdir("nested-gated-folder");
        let loaded = mod_dir.join("1.6");
        let nested = loaded.join("Mods").join("CompatX");
        fs::create_dir_all(loaded.join("Defs")).unwrap();
        fs::create_dir_all(nested.join("Defs")).unwrap();
        fs::write(
            nested.join("Defs").join("Ghost.xml"),
            r#"<Defs><ThingDef><defName>NestedGhostDef</defName></ThingDef></Defs>"#,
        )
        .unwrap();

        let mut this_mod = base_mod("definer", mod_dir);
        this_mod.info.loaded_folders = vec![loaded];
        let scan = empty_scan(vec![this_mod], Vec::new());

        let mut conflicts = vec![unexplained_conflict("NestedGhostDef")];
        explain_dangling_references(&mut conflicts, &scan);

        assert!(matches!(
            cause_of(&conflicts, "NestedGhostDef"),
            DanglingCause::OnlyInUnloadedFolder { ref mod_id, ref folder }
                if mod_id.as_str() == "definer" && folder == "1.6/Mods/CompatX"
        ));

        fs::remove_dir_all(scan.scanned_mods[0].info.path.clone()).ok();
    }

    /// Regression: the old scan only ever looked one level *inside* an
    /// unloaded top-level entry for its own `Defs` subfolder, so a mod's
    /// own **root** `Defs/` being itself unloaded (`LoadFolders.xml`
    /// selects only a version folder, omitting `/`) was never found at
    /// all — `inactive_defs_folder`'s one-level lookup would have looked for a nonexistent
    /// `Defs/Defs` one level too deep.
    #[test]
    fn name_defined_only_in_the_mod_root_when_root_itself_is_unloaded_is_explained() {
        let mod_dir = tempdir("unloaded-root");
        let loaded = mod_dir.join("1.6");
        fs::create_dir_all(&loaded).unwrap();
        fs::create_dir_all(mod_dir.join("Defs")).unwrap();
        fs::write(
            mod_dir.join("Defs").join("Ghost.xml"),
            r#"<Defs><ThingDef><defName>RootGhostDef</defName></ThingDef></Defs>"#,
        )
        .unwrap();

        let mut this_mod = base_mod("definer", mod_dir);
        // Only the version folder is loaded — the mod root itself, and its
        // own `Defs/`, is not.
        this_mod.info.loaded_folders = vec![loaded];
        let scan = empty_scan(vec![this_mod], Vec::new());

        let mut conflicts = vec![unexplained_conflict("RootGhostDef")];
        explain_dangling_references(&mut conflicts, &scan);

        assert!(matches!(
            cause_of(&conflicts, "RootGhostDef"),
            DanglingCause::OnlyInUnloadedFolder { ref mod_id, ref folder }
                if mod_id.as_str() == "definer" && folder == "."
        ));

        fs::remove_dir_all(scan.scanned_mods[0].info.path.clone()).ok();
    }

    #[test]
    fn name_in_an_inactive_mod_is_explained() {
        let inactive_dir = tempdir("inactive-mod");
        fs::create_dir_all(inactive_dir.join("Defs")).unwrap();
        fs::write(
            inactive_dir.join("Defs").join("Ghost.xml"),
            r#"<Defs><ThingDef><defName>GhostDef2</defName></ThingDef></Defs>"#,
        )
        .unwrap();

        let inactive_mod = crate::domain::InactiveMod {
            id: ModId::new("dormant"),
            name: "Dormant".to_string(),
            authors: Vec::new(),
            path: inactive_dir.clone(),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        };
        let scan = empty_scan(Vec::new(), vec![inactive_mod]);

        let mut conflicts = vec![unexplained_conflict("GhostDef2")];
        explain_dangling_references(&mut conflicts, &scan);

        assert!(matches!(
            cause_of(&conflicts, "GhostDef2"),
            DanglingCause::OnlyInInactiveMod { ref mod_id } if mod_id.as_str() == "dormant"
        ));

        fs::remove_dir_all(inactive_dir).ok();
    }

    fn inactive_mod_defining(root: &Path, id: &str, def_names: &[&str]) -> InactiveMod {
        let path = root.join(id);
        fs::create_dir_all(path.join("Defs")).unwrap();
        let defs: String = def_names
            .iter()
            .map(|name| format!("<ThingDef><defName>{name}</defName></ThingDef>"))
            .collect();
        fs::write(
            path.join("Defs").join("Things.xml"),
            format!("<Defs>{defs}</Defs>"),
        )
        .unwrap();
        InactiveMod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            path,
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        }
    }

    /// Candidate files are searched in parallel but applied in candidate
    /// order: a name two candidates define takes the earlier one's cause,
    /// and a later candidate still explains what the earlier ones lack.
    #[test]
    fn a_name_defined_by_several_candidates_takes_the_earliest_ones_cause() {
        let root = tempdir("earliest-candidate");
        let inactive = vec![
            inactive_mod_defining(&root, "alpha", &["SharedDef"]),
            inactive_mod_defining(&root, "beta", &["SharedDef", "BetaOnlyDef"]),
        ];
        let scan = empty_scan(Vec::new(), inactive);
        let mut conflicts = vec![
            unexplained_conflict("BetaOnlyDef"),
            unexplained_conflict("SharedDef"),
        ];

        explain_dangling_references(&mut conflicts, &scan);

        assert_eq!(
            cause_of(&conflicts, "SharedDef"),
            DanglingCause::OnlyInInactiveMod {
                mod_id: ModId::new("alpha")
            }
        );
        assert_eq!(
            cause_of(&conflicts, "BetaOnlyDef"),
            DanglingCause::OnlyInInactiveMod {
                mod_id: ModId::new("beta")
            }
        );

        fs::remove_dir_all(root).ok();
    }

    /// `build_explained_report` is `build_ref` followed by the explanation:
    /// a hyperlink to a def only an inactive mod defines comes out
    /// explained, and the whole report equals the two steps run in turn.
    #[test]
    fn build_explained_report_equals_build_ref_followed_by_the_explanation() {
        let root = tempdir("explained-report");
        let referrer_dir = root.join("referrer");
        fs::create_dir_all(&referrer_dir).unwrap();
        let file: std::sync::Arc<Path> = std::sync::Arc::from(referrer_dir.join("Things.xml"));
        let defs_file = defs::index(
            br#"<Defs>
                  <ThingDef>
                    <defName>Spawner</defName>
                    <descriptionHyperlinks><ThingDef>GhostDef4</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
            &file,
        )
        .unwrap();
        let mut referrer = base_mod("referrer", referrer_dir);
        referrer.defs = defs_file.defs;
        let mut scan = empty_scan(
            vec![referrer],
            vec![inactive_mod_defining(&root, "dormant", &["GhostDef4"])],
        );
        scan.ref_sites_by_mod
            .insert(ModId::new("referrer"), defs_file.ref_sites);
        let context = RunContext {
            game_dir: root.clone(),
            workshop_dir: root.clone(),
            mods_config_path: root.join("ModsConfig.xml"),
            game_version: crate::domain::GameVersion::new(1, 6),
        };

        let report = build_explained_report(&scan, &context);
        let mut sequential = analysis::build_ref(&scan, &context);
        explain_dangling_references(&mut sequential.conflicts, &scan);

        assert_eq!(
            cause_of(&report.conflicts, "GhostDef4"),
            DanglingCause::OnlyInInactiveMod {
                mod_id: ModId::new("dormant")
            }
        );
        assert_eq!(
            serde_json::to_value(&report.conflicts).unwrap(),
            serde_json::to_value(&sequential.conflicts).unwrap()
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn one_pass_tag_text_search_matches_the_per_name_search() {
        let pending: BTreeMap<String, usize> = [
            "Steel",
            "Ste",
            "",
            "a>b",
            "x<y",
            "Wood",
            "Missing",
            "Ünïcode",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name.to_string(), index))
        .collect();
        let haystacks: [&[u8]; 7] = [
            b"<defName>Steel</defName><li>Wood</li>",
            b"<a>Ste</a><b></b>",
            b">a>b< >>Wood< trailing >Missing",
            b"<v>x<y</v>",
            ">Ünïcode<".as_bytes(),
            b">>><<<",
            b"",
        ];

        for haystack in haystacks {
            let expected: Vec<String> = pending
                .keys()
                .filter(|name| contains_tag_text(haystack, name))
                .cloned()
                .collect();

            let found = names_in_tag_text(haystack, &pending);

            assert_eq!(
                found,
                expected,
                "haystack {:?}",
                String::from_utf8_lossy(haystack)
            );
        }
    }

    #[test]
    fn explain_budget_exhaustion_is_unexplained_not_a_panic() {
        let inactive_dir = tempdir("budget-exhaustion");
        fs::create_dir_all(inactive_dir.join("Defs")).unwrap();
        // Genuinely defines the name, so only the budget can stop the
        // search from explaining it.
        fs::write(
            inactive_dir.join("Defs").join("Big.xml"),
            r#"<Defs><ThingDef><defName>Anything</defName></ThingDef></Defs>"#,
        )
        .unwrap();
        let inactive_mod = crate::domain::InactiveMod {
            id: ModId::new("dormant"),
            name: "Dormant".to_string(),
            authors: Vec::new(),
            path: inactive_dir.clone(),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        };
        let scan = empty_scan(Vec::new(), vec![inactive_mod]);
        let mut conflicts = vec![unexplained_conflict("Anything")];

        explain_within_budget(&mut conflicts, &CandidateFiles::collect(&scan), 10);

        // Never panics, and the name stays `Unexplained` — neither found
        // nor proven absent.
        assert_eq!(cause_of(&conflicts, "Anything"), DanglingCause::Unexplained);

        fs::remove_dir_all(inactive_dir).ok();
    }

    /// Regression: a candidate file that fails to read (here, a genuine
    /// OS-level sharing violation — the file is held open with exclusive
    /// access for the whole search, on this Windows-only workspace) must
    /// leave the search incomplete, exactly like running out of budget —
    /// never a silently-skipped candidate that lets the search "complete"
    /// and wrongly declare a name `DefinedNowhere` when it might be
    /// sitting, unread, in that very file.
    #[test]
    fn a_candidate_that_fails_to_read_leaves_the_result_unexplained() {
        let inactive_dir = tempdir("read-failure");
        fs::create_dir_all(inactive_dir.join("Defs")).unwrap();
        let unreadable = inactive_dir.join("Defs").join("Locked.xml");
        fs::write(
            &unreadable,
            r#"<Defs><ThingDef><defName>LockedGhostDef</defName></ThingDef></Defs>"#,
        )
        .unwrap();

        use std::os::windows::fs::OpenOptionsExt;
        // `share_mode(0)` denies every other handle read/write/delete
        // access for as long as this one stays open — a real, deterministic
        // OS-level read failure, not a simulated one.
        let _exclusive_lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&unreadable)
            .expect("open with exclusive access");

        let inactive_mod = crate::domain::InactiveMod {
            id: ModId::new("dormant"),
            name: "Dormant".to_string(),
            authors: Vec::new(),
            path: inactive_dir.clone(),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        };
        let scan = empty_scan(Vec::new(), vec![inactive_mod]);
        let mut conflicts = vec![unexplained_conflict("LockedGhostDef")];

        explain_dangling_references(&mut conflicts, &scan);

        assert_eq!(
            cause_of(&conflicts, "LockedGhostDef"),
            DanglingCause::Unexplained,
            "a read failure must never be silently skipped into a false DefinedNowhere"
        );

        drop(_exclusive_lock);
        fs::remove_dir_all(inactive_dir).ok();
    }
}
