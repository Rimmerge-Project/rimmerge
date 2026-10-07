//! In-memory doubles for every port — one per port trait.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{GameVersion, GeneratedMarker, ModId, Report, Source, XmlLocator};
use rim_analyzer::extract::about_xml::AboutDetails;
use rim_analyzer::extract::rimmerge_marker;
use rim_merge::emit::{FileContent, RenderedMod};
use rim_resolve::domain::{AssignmentId, AssignmentProject, DecisionSet, PatchId, PatchProject};

use crate::ProjectPaths;
use crate::app_settings::AppSettings;
use crate::game_launch::LaunchUnavailable;
use crate::ports::{
    AboutImage, AboutReadError, AppSettingsLoad, AppSettingsStore, AssetLocator,
    AssignmentProjectStore, BackReferenceShape, CachedDatabase, ConfigError, DatabaseStatus,
    DecisionStore, DefCacheCarrier, DefCacheCarrierProbe, DefSourceError, DefSourceReader,
    ElementExpectation, GameExecutable, GameLauncher, GameLogError, GameLogReader, ImportError,
    ImportManifestStore, ImportRecord, ImportedRules, KindChoice, LaunchFailure, LaunchRoute,
    LoadedModKnowledge, LoadedRules, LogFormats, LogShapes, MergeModError, MergeModWriteReport,
    MergeModWriter, ModAboutReader, ModKnowledgeStore, ModScanner, ModsConfigFile, ModsConfigStore,
    ParsedGameLog, PatchProjectStore, PatchStackBlockShape, ProfileNotificationState,
    ProfileNotificationStateStore, RefreshOutcome, RimSortImporter, RimSortPaths, RuleDatabase,
    RuleDatabaseFetcher, RuleStore, RulesLoadWarning, ScanArtifacts, ScanError, ScanProgress,
    ScanProgressStage, StackBlockSource, StoreError, StoredRules, TextureBytes,
    TextureFallbackShape, TextureFallbackSource,
};
use crate::ports::{
    IMPORT_SOURCE_COMMUNITY_RULES, IMPORT_SOURCE_STEAM_DEPENDENCIES, IMPORT_SOURCE_USER_RULES,
};

/// Always returns the given report/evidence/sources; invokes `progress`
/// once so tests can assert it was wired up. Defaults to an empty
/// [`SourceIndex`] — most use-case tests never touch the merge editor's
/// own read path, so seeding one for every fixture would be needless
/// ceremony; a test that does (see [`bionic_heart_fixture`]) builds its
/// own.
pub struct FakeScanner {
    report: Report,
    evidence: Vec<rim_resolve::domain::TagEvidence>,
    sources: SourceIndex,
    /// The `active_override` the most recent `scan_and_analyze` call
    /// received — a wiring assertion:
    /// a test can check the override actually reached the scanner without
    /// this fake having to behave any differently depending on it. `&self`
    /// (not `&mut self`) on the trait forces the interior mutability.
    ///
    /// **Doubly-`Option`, not a bare `Option<Vec<ModId>>`**: the outer
    /// `Option` is whether
    /// `scan_and_analyze` has ever been called at all (`None`); the inner
    /// one is what it was called with (`Some(None)` = no override,
    /// `Some(Some(ids))` = an override) — a bare `Option<Vec<ModId>>`
    /// could not tell "never called" apart from "called with no
    /// override", both of which read `None`.
    last_override: RefCell<Option<Option<Vec<ModId>>>>,
}

impl FakeScanner {
    /// Builds a fake that always returns `report`/`evidence` with an
    /// empty [`SourceIndex`].
    #[must_use]
    pub fn new(report: Report, evidence: Vec<rim_resolve::domain::TagEvidence>) -> Self {
        Self {
            report,
            evidence,
            sources: SourceIndex::default(),
            last_override: RefCell::new(None),
        }
    }

    /// [`Self::new`], but with `sources` returned instead of an empty
    /// index.
    #[must_use]
    pub fn with_sources(
        report: Report,
        evidence: Vec<rim_resolve::domain::TagEvidence>,
        sources: SourceIndex,
    ) -> Self {
        Self {
            report,
            evidence,
            sources,
            last_override: RefCell::new(None),
        }
    }

    /// The `active_override` the most recent `scan_and_analyze` call
    /// received: `None` if it was never called at all, `Some(None)` if
    /// the most recent call carried no override, `Some(Some(ids))`
    /// otherwise.
    #[must_use]
    pub fn last_override(&self) -> Option<Option<Vec<ModId>>> {
        self.last_override.borrow().clone()
    }
}

impl ModScanner for FakeScanner {
    fn scan_and_analyze(
        &self,
        _paths: &ProjectPaths,
        active_override: Option<&[ModId]>,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanArtifacts, ScanError> {
        *self.last_override.borrow_mut() = Some(active_override.map(<[ModId]>::to_vec));
        progress(ScanProgress {
            stage: ScanProgressStage::Scanning,
            done: 1,
            total: 1,
        });
        Ok(ScanArtifacts {
            report: self.report.clone(),
            evidence: self.evidence.clone(),
            sources: self.sources.clone(),
        })
    }
}

/// A `DefSourceReader` fake backed by a plain lookup table — seed it with
/// the XML text a locator should read back, or with a canned error (for
/// exercising staleness/I/O propagation) via [`Self::with_error`].
#[derive(Debug, Clone, Default)]
pub struct InMemoryDefSourceReader {
    elements: BTreeMap<XmlLocator, Result<String, DefSourceError>>,
}

impl InMemoryDefSourceReader {
    /// Builds a fake seeded with `elements`' XML text.
    #[must_use]
    pub fn new(elements: BTreeMap<XmlLocator, String>) -> Self {
        Self {
            elements: elements
                .into_iter()
                .map(|(locator, text)| (locator, Ok(text)))
                .collect(),
        }
    }

    /// Makes `locator` fail with `error` instead of returning text.
    #[must_use]
    pub fn with_error(mut self, locator: XmlLocator, error: DefSourceError) -> Self {
        self.elements.insert(locator, Err(error));
        self
    }
}

impl DefSourceReader for InMemoryDefSourceReader {
    fn read_element(
        &self,
        locator: &XmlLocator,
        _expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        self.elements.get(locator).cloned().unwrap_or_else(|| {
            Err(DefSourceError::Io {
                file: locator.file.to_path_buf(),
                message: format!("no element indexed at {:?}", locator.element_path),
            })
        })
    }
}

/// An `AssetLocator` fake backed by a plain `texture_key -> path` table for
/// locating, and a `path -> result` table for reading — seed the latter
/// with [`Self::with_bytes`]/[`Self::with_read_error`], one entry per path
/// a test's `locate_texture` call is expected to return.
#[derive(Debug, Default)]
pub struct FakeAssetLocator {
    by_key: BTreeMap<String, PathBuf>,
    /// `texture_key -> path` for [`AssetLocator::locate_non_dds_texture`]
    /// only; a key with no entry has no image copy.
    non_dds_by_key: BTreeMap<String, PathBuf>,
    bytes_by_path: BTreeMap<PathBuf, Result<TextureBytes, DefSourceError>>,
    /// `(mod_root, image) -> located path`; this seeds
    /// [`AssetLocator::locate_about_image`]'s result — a key with no entry
    /// here defaults to `None`, "no such image".
    about_images: BTreeMap<(PathBuf, AboutImage), Option<PathBuf>>,
    /// The `(mod_root, image)` most recently passed to
    /// [`AssetLocator::locate_about_image`] — a wiring assertion: a test
    /// can check the use case derived exactly this path from session
    /// data, never one it made up. A `std::sync::Mutex`, not a
    /// `RefCell`: this fake gets plugged into `apps/desktop`'s
    /// `Adapters::asset_locator`, which is `Arc<dyn AssetLocator + Send +
    /// Sync>` — a `RefCell` field would make the whole fake `!Sync`.
    last_about_image_request: std::sync::Mutex<Option<(PathBuf, AboutImage)>>,
}

impl FakeAssetLocator {
    /// Builds a fake seeded with `by_key`.
    #[must_use]
    pub fn new(by_key: BTreeMap<String, PathBuf>) -> Self {
        Self {
            by_key,
            ..Self::default()
        }
    }

    /// Makes [`AssetLocator::locate_non_dds_texture`] find `path` for `key`.
    #[must_use]
    pub fn with_non_dds(mut self, key: &str, path: PathBuf) -> Self {
        self.non_dds_by_key.insert(key.to_string(), path);
        self
    }

    /// Makes [`AssetLocator::read_texture`] return `texture` for `path`.
    #[must_use]
    pub fn with_bytes(mut self, path: PathBuf, texture: TextureBytes) -> Self {
        self.bytes_by_path.insert(path, Ok(texture));
        self
    }

    /// Makes [`AssetLocator::read_texture`] fail with `error` for `path`.
    #[must_use]
    pub fn with_read_error(mut self, path: PathBuf, error: DefSourceError) -> Self {
        self.bytes_by_path.insert(path, Err(error));
        self
    }

    /// Makes [`AssetLocator::locate_about_image`] return `located` for
    /// `(mod_root, image)`.
    #[must_use]
    pub fn with_about_image(
        mut self,
        mod_root: PathBuf,
        image: AboutImage,
        located: Option<PathBuf>,
    ) -> Self {
        self.about_images.insert((mod_root, image), located);
        self
    }

    /// The `(mod_root, image)` most recently passed to
    /// [`AssetLocator::locate_about_image`], if it's been called at all.
    #[must_use]
    pub fn last_about_image_request(&self) -> Option<(PathBuf, AboutImage)> {
        self.last_about_image_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl AssetLocator for FakeAssetLocator {
    fn locate_texture(
        &self,
        _mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        Ok(self.by_key.get(texture_key).cloned())
    }

    fn locate_non_dds_texture(
        &self,
        _mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        Ok(self.non_dds_by_key.get(texture_key).cloned())
    }

    fn read_texture(&self, path: &Path) -> Result<TextureBytes, DefSourceError> {
        self.bytes_by_path.get(path).cloned().unwrap_or_else(|| {
            Err(DefSourceError::Io {
                file: path.to_path_buf(),
                message: "no bytes seeded on this fake".to_string(),
            })
        })
    }

    fn locate_about_image(&self, mod_root: &Path, image: AboutImage) -> Option<PathBuf> {
        *self
            .last_about_image_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((mod_root.to_path_buf(), image));
        self.about_images
            .get(&(mod_root.to_path_buf(), image))
            .cloned()
            .flatten()
    }
}

/// A [`ModAboutReader`] fake seeded per `mod_root` — a root with no entry
/// returns [`AboutReadError::NotFound`], matching the real
/// `FileModAboutReader`'s own behaviour for a folder with no
/// `About.xml`.
#[derive(Debug, Default)]
pub struct FakeModAboutReader {
    by_root: BTreeMap<PathBuf, Result<AboutDetails, AboutReadError>>,
    /// The `(mod_root, source, game_version, game_dir)` most recently
    /// passed to [`ModAboutReader::read_details`] — a wiring assertion,
    /// the same shape [`FakeAssetLocator::last_about_image_request`]
    /// offers. A `std::sync::Mutex`, not a `RefCell`, for the same
    /// `Adapters`-is-`Send + Sync` reason that field is.
    last_request: std::sync::Mutex<Option<(PathBuf, Source, GameVersion, PathBuf)>>,
}

impl FakeModAboutReader {
    /// Makes [`ModAboutReader::read_details`] return `result` for
    /// `mod_root`.
    #[must_use]
    pub fn with_result(
        mut self,
        mod_root: PathBuf,
        result: Result<AboutDetails, AboutReadError>,
    ) -> Self {
        self.by_root.insert(mod_root, result);
        self
    }

    /// The `(mod_root, source, game_version, game_dir)` most recently
    /// passed to [`ModAboutReader::read_details`], if it's been called at
    /// all.
    #[must_use]
    pub fn last_request(&self) -> Option<(PathBuf, Source, GameVersion, PathBuf)> {
        self.last_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ModAboutReader for FakeModAboutReader {
    fn read_details(
        &self,
        mod_root: &Path,
        source: Source,
        game_version: GameVersion,
        game_dir: &Path,
    ) -> Result<AboutDetails, AboutReadError> {
        *self
            .last_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((
            mod_root.to_path_buf(),
            source,
            game_version,
            game_dir.to_path_buf(),
        ));
        self.by_root
            .get(mod_root)
            .cloned()
            .unwrap_or(Err(AboutReadError::NotFound))
    }
}

/// A `MergeModWriter` fake that records every [`RenderedMod`] it was asked
/// to write, and every folder name it was asked to remove, without
/// touching a filesystem. `dir`/`mods_dir` is never distinguished by
/// [`Self::write`]/[`Self::remove`]/[`Self::set_marker`] (only the folder
/// name is tracked) — fine for every caller that only ever writes/reads
/// against one directory at a time; [`Self::set_marker_in`] is the one
/// escape hatch for a test that needs two directories to disagree.
#[derive(Debug, Default)]
pub struct InMemoryMergeModWriter {
    writes: RefCell<Vec<RenderedMod>>,
    removed: RefCell<Vec<String>>,
    /// Folder names currently "installed", per the last `write`/`remove`
    /// call — backs [`MergeModWriter::exists`].
    installed: RefCell<BTreeSet<String>>,
    /// The marker [`MergeModWriter::read_marker`] returns for a folder name
    /// — derived automatically from the written [`RenderedMod`]'s own
    /// `rimmerge.json` file content on every [`MergeModWriter::write`]
    /// (parsed the same way the real writer's `read_marker` parses the
    /// on-disk file), or set directly via [`Self::set_marker`] for a test
    /// that wants to simulate a folder this fake never actually wrote (or
    /// wrote as something else) — e.g. `ExportPatch`'s "the existing
    /// folder isn't this patch's own" (`ForeignFolder`) check.
    markers: RefCell<BTreeMap<String, GeneratedMarker>>,
    /// Directory-scoped overrides for [`Self::exists`]/[`Self::read_marker`]
    /// (`Some` for present-with-a-marker, `Some(None)` for
    /// present-with-no-marker), keyed by the exact `(dir, folder_name)`
    /// pair given to [`Self::set_marker_in`] — unlike [`Self::markers`]/
    /// [`Self::installed`], which (like the real
    /// `MergeModFolderWriter`'s own simplification for this fake — see
    /// their own doc comment) never distinguish `dir`. Needed only by a
    /// test with *two* directories in play at once (an `out_dir` and the
    /// game's `Mods/`) that must disagree about the same folder name;
    /// every other test's single-directory [`Self::set_marker`] is
    /// unaffected — an empty map here falls through to `markers`/`installed`.
    markers_by_dir: RefCell<BTreeMap<(PathBuf, String), Option<GeneratedMarker>>>,
}

impl InMemoryMergeModWriter {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every [`RenderedMod`] this fake was asked to write, in order.
    #[must_use]
    pub fn writes(&self) -> Vec<RenderedMod> {
        self.writes.borrow().clone()
    }

    /// Every folder name this fake was asked to remove, in order.
    #[must_use]
    pub fn removed(&self) -> Vec<String> {
        self.removed.borrow().clone()
    }

    /// Makes [`MergeModWriter::exists`] report `folder_name` as already
    /// present and [`MergeModWriter::read_marker`] return `marker` for it,
    /// without a real [`MergeModWriter::write`] call — the fake's own way
    /// to seed "a folder is already there with someone else's marker (or
    /// none)" for a test exercising the foreign-folder refusal.
    pub fn set_marker(&self, folder_name: &str, marker: GeneratedMarker) {
        self.installed.borrow_mut().insert(folder_name.to_string());
        self.markers
            .borrow_mut()
            .insert(folder_name.to_string(), marker);
    }

    /// [`Self::set_marker`], but scoped to `dir` only — every other
    /// directory's [`MergeModWriter::exists`]/[`MergeModWriter::read_marker`]
    /// for `folder_name` are unaffected. For a test that needs an
    /// `out_dir` and the game's `Mods/` to disagree about the same
    /// (derived) folder name, e.g. "the install target is foreign but the
    /// export target isn't".
    pub fn set_marker_in(&self, dir: &Path, folder_name: &str, marker: GeneratedMarker) {
        self.markers_by_dir
            .borrow_mut()
            .insert((dir.to_path_buf(), folder_name.to_string()), Some(marker));
    }
}

/// Parses `rendered`'s own rendered `rimmerge.json` file the same way the
/// real [`MergeModWriter::read_marker`] parses it off disk — `None` when
/// there is no such file (never emitted for a rendered mod in practice) or
/// its content doesn't parse as a marker.
fn marker_from_rendered(rendered: &RenderedMod) -> Option<GeneratedMarker> {
    let marker_file = rendered
        .files
        .iter()
        .find(|file| file.relative_path == Path::new("rimmerge.json"))?;
    match &marker_file.content {
        FileContent::Text(text) => rimmerge_marker::parse(text.as_bytes()),
        FileContent::CopyFrom(_) => None,
    }
}

impl MergeModWriter for InMemoryMergeModWriter {
    fn write(
        &self,
        mods_dir: &Path,
        _backup_dir: &Path,
        rendered: &RenderedMod,
    ) -> Result<MergeModWriteReport, MergeModError> {
        self.writes.borrow_mut().push(rendered.clone());
        self.installed
            .borrow_mut()
            .insert(rendered.folder_name.clone());
        match marker_from_rendered(rendered) {
            Some(marker) => {
                self.markers
                    .borrow_mut()
                    .insert(rendered.folder_name.clone(), marker);
            }
            None => {
                self.markers.borrow_mut().remove(&rendered.folder_name);
            }
        }
        Ok(MergeModWriteReport {
            mod_path: mods_dir.join(&rendered.folder_name),
            backup_path: None,
        })
    }

    fn remove(
        &self,
        mods_dir: &Path,
        _backup_dir: &Path,
        folder_name: &str,
    ) -> Result<Option<PathBuf>, MergeModError> {
        self.removed.borrow_mut().push(folder_name.to_string());
        self.installed.borrow_mut().remove(folder_name);
        self.markers.borrow_mut().remove(folder_name);
        Ok(Some(mods_dir.join(folder_name)))
    }

    fn exists(&self, mods_dir: &Path, folder_name: &str) -> bool {
        let key = (mods_dir.to_path_buf(), folder_name.to_string());
        if self.markers_by_dir.borrow().contains_key(&key) {
            return true;
        }
        self.installed.borrow().contains(folder_name)
    }

    fn read_marker(
        &self,
        dir: &Path,
        folder_name: &str,
    ) -> Result<Option<GeneratedMarker>, MergeModError> {
        let key = (dir.to_path_buf(), folder_name.to_string());
        if let Some(over) = self.markers_by_dir.borrow().get(&key) {
            return Ok(over.clone());
        }
        Ok(self.markers.borrow().get(folder_name).cloned())
    }
}

/// A single in-memory `ModsConfig.xml`, plus every write it was asked to
/// make. `write_with_backup` never touches the filesystem: it returns
/// `<path>.bak-fake` deterministically.
#[derive(Default)]
pub struct InMemoryModsConfigStore {
    file: RefCell<Option<ModsConfigFile>>,
    writes: RefCell<Vec<ModsConfigFile>>,
}

impl InMemoryModsConfigStore {
    /// Builds a fake seeded with `file` as the current `ModsConfig.xml`.
    #[must_use]
    pub fn new(file: ModsConfigFile) -> Self {
        Self {
            file: RefCell::new(Some(file)),
            writes: RefCell::new(Vec::new()),
        }
    }

    /// Every file this fake was asked to write, in order.
    #[must_use]
    pub fn writes(&self) -> Vec<ModsConfigFile> {
        self.writes.borrow().clone()
    }
}

impl ModsConfigStore for InMemoryModsConfigStore {
    fn read(&self, _path: &Path) -> Result<ModsConfigFile, ConfigError> {
        self.file
            .borrow()
            .clone()
            .ok_or_else(|| ConfigError("no ModsConfig.xml set on this fake".to_string()))
    }

    fn write_with_backup(
        &self,
        path: &Path,
        file: &ModsConfigFile,
    ) -> Result<PathBuf, ConfigError> {
        self.writes.borrow_mut().push(file.clone());
        *self.file.borrow_mut() = Some(file.clone());
        Ok(PathBuf::from(format!("{}.bak-fake", path.display())))
    }
}

/// An in-memory decisions store: `load` returns the empty set until
/// something has been `save`d. [`InMemoryDecisionStore::fail_next_save`]
/// injects a one-shot save failure, for testing a use case's rollback.
#[derive(Default)]
pub struct InMemoryDecisionStore {
    saved: RefCell<Option<DecisionSet>>,
    fail_next_save: Cell<bool>,
}

impl InMemoryDecisionStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last set this fake was asked to save, if any.
    #[must_use]
    pub fn last_saved(&self) -> Option<DecisionSet> {
        self.saved.borrow().clone()
    }

    /// Makes the next [`DecisionStore::save`] call fail with a
    /// [`StoreError`], then resets — a one-shot failure injection for
    /// testing a use case's rollback-on-save-failure behavior.
    pub fn fail_next_save(&self) {
        self.fail_next_save.set(true);
    }
}

impl DecisionStore for InMemoryDecisionStore {
    fn load(&self, _dir: &Path) -> Result<DecisionSet, StoreError> {
        Ok(self.saved.borrow().clone().unwrap_or_default())
    }

    fn save(&self, _dir: &Path, set: &DecisionSet) -> Result<(), StoreError> {
        if self.fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        *self.saved.borrow_mut() = Some(set.clone());
        Ok(())
    }
}

/// A [`ModKnowledgeStore`] fake returning whatever it was built with —
/// [`FakeModKnowledgeStore::empty`] is the "nothing loaded" default every
/// test that doesn't care about mod knowledge uses.
#[derive(Debug, Clone, Default)]
pub struct FakeModKnowledgeStore {
    loaded: LoadedModKnowledge,
    loaded_with: Cell<Option<bool>>,
}

impl FakeModKnowledgeStore {
    /// The `source_enabled` argument of the last [`ModKnowledgeStore::load`]
    /// call, or `None` if it was never called.
    #[must_use]
    pub fn loaded_with(&self) -> Option<bool> {
        self.loaded_with.get()
    }

    /// A store that knows nothing at all.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// A store returning `loaded` from every call.
    #[must_use]
    pub fn returning(loaded: LoadedModKnowledge) -> Self {
        Self {
            loaded,
            loaded_with: Cell::new(None),
        }
    }
}

impl ModKnowledgeStore for FakeModKnowledgeStore {
    /// Records what `source_enabled` it was called with (so a caller can
    /// assert `LoadProject` really threads
    /// `Settings::fetch_rimmerge_rules` through) but does not
    /// itself act on it: this fake stands in for the file read, not for
    /// the gating rule, which `rim-io`'s own store owns and tests.
    fn load(&self, source_enabled: bool) -> LoadedModKnowledge {
        self.loaded_with.set(Some(source_enabled));
        self.loaded.clone()
    }
}

/// An in-memory rules store: `load` returns [`StoredRules::default`]
/// until something has been `save`d. [`InMemoryRuleStore::fail_next_save`]
/// injects a one-shot save failure, for testing a use case's rollback;
/// [`InMemoryRuleStore::warn_on_next_load`] injects a one-shot
/// [`RulesLoadWarning`], for testing that `LoadProject` actually threads
/// `RuleStore::load`'s warnings onto the built [`Session`].
#[derive(Default)]
pub struct InMemoryRuleStore {
    saved: RefCell<Option<StoredRules>>,
    fail_next_save: Cell<bool>,
    saves: Cell<usize>,
    warn_on_next_load: RefCell<Vec<RulesLoadWarning>>,
}

impl InMemoryRuleStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last rules this fake was asked to save, if any.
    #[must_use]
    pub fn last_saved(&self) -> Option<StoredRules> {
        self.saved.borrow().clone()
    }

    /// How many saves have succeeded.
    #[must_use]
    pub fn save_count(&self) -> usize {
        self.saves.get()
    }

    /// Makes the next [`RuleStore::save`] call fail with a [`StoreError`],
    /// then resets — a one-shot failure injection for testing a use
    /// case's rollback-on-save-failure behavior.
    pub fn fail_next_save(&self) {
        self.fail_next_save.set(true);
    }

    /// Makes the next [`RuleStore::load`] call return `warnings` alongside
    /// its rules, then resets — a one-shot injection for testing that a
    /// caller (`LoadProject`) actually threads `RuleStore::load`'s
    /// warnings through rather than discarding them.
    pub fn warn_on_next_load(&self, warnings: Vec<RulesLoadWarning>) {
        *self.warn_on_next_load.borrow_mut() = warnings;
    }
}

impl RuleStore for InMemoryRuleStore {
    fn load(&self, _dir: &Path) -> Result<LoadedRules, StoreError> {
        Ok(LoadedRules {
            rules: self.saved.borrow().clone().unwrap_or_default(),
            warnings: self.warn_on_next_load.take(),
        })
    }

    fn save(&self, _dir: &Path, rules: &StoredRules) -> Result<(), StoreError> {
        if self.fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        *self.saved.borrow_mut() = Some(rules.clone());
        self.saves.set(self.saves.get() + 1);
        Ok(())
    }
}

/// An in-memory patch project store: `load_all` returns every project
/// `save` was given (deleted ones removed), keyed by
/// [`rim_resolve::domain::PatchProject::id`].
/// [`InMemoryPatchProjectStore::fail_next_save`]/
/// [`InMemoryPatchProjectStore::fail_next_delete`]/
/// [`InMemoryPatchProjectStore::fail_next_load`] inject one-shot
/// failures, for testing a use case's rollback (or, for `load_all`,
/// [`crate::use_cases::LoadProjectError::Patches`]).
#[derive(Default)]
pub struct InMemoryPatchProjectStore {
    projects: RefCell<BTreeMap<PatchId, PatchProject>>,
    deleted: RefCell<BTreeSet<PatchId>>,
    fail_next_save: Cell<bool>,
    fail_next_delete: Cell<bool>,
    fail_next_load: Cell<bool>,
}

impl InMemoryPatchProjectStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The project currently on file for `id`, if any (the last `save`,
    /// unless it was later `delete`d).
    #[must_use]
    pub fn last_saved(&self, id: &PatchId) -> Option<PatchProject> {
        self.projects.borrow().get(id).cloned()
    }

    /// Whether `id` was ever passed to [`PatchProjectStore::delete`].
    #[must_use]
    pub fn was_deleted(&self, id: &PatchId) -> bool {
        self.deleted.borrow().contains(id)
    }

    /// Makes the next [`PatchProjectStore::save`] call fail, then resets.
    pub fn fail_next_save(&self) {
        self.fail_next_save.set(true);
    }

    /// Makes the next [`PatchProjectStore::delete`] call fail, then
    /// resets.
    pub fn fail_next_delete(&self) {
        self.fail_next_delete.set(true);
    }

    /// Makes the next [`PatchProjectStore::load_all`] call fail, then
    /// resets — for exercising [`crate::use_cases::LoadProjectError::Patches`].
    pub fn fail_next_load(&self) {
        self.fail_next_load.set(true);
    }
}

impl PatchProjectStore for InMemoryPatchProjectStore {
    fn load_all(&self, _profile_dir: &Path) -> Result<Vec<PatchProject>, StoreError> {
        if self.fail_next_load.replace(false) {
            return Err(StoreError("simulated load failure".to_string()));
        }
        Ok(self.projects.borrow().values().cloned().collect())
    }

    fn save(&self, _profile_dir: &Path, project: &PatchProject) -> Result<(), StoreError> {
        if self.fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        self.projects
            .borrow_mut()
            .insert(project.id().clone(), project.clone());
        Ok(())
    }

    fn delete(&self, _profile_dir: &Path, id: &PatchId) -> Result<(), StoreError> {
        if self.fail_next_delete.replace(false) {
            return Err(StoreError("simulated delete failure".to_string()));
        }
        self.projects.borrow_mut().remove(id);
        self.deleted.borrow_mut().insert(id.clone());
        Ok(())
    }
}

/// [`InMemoryPatchProjectStore`]'s [`AssignmentProjectStore`] twin —
/// identical shape, one file-per-id semantics, the same one-shot
/// `fail_next_*` injection.
#[derive(Default)]
pub struct InMemoryAssignmentProjectStore {
    projects: RefCell<BTreeMap<AssignmentId, AssignmentProject>>,
    deleted: RefCell<BTreeSet<AssignmentId>>,
    fail_next_save: Cell<bool>,
    fail_next_delete: Cell<bool>,
    fail_next_load: Cell<bool>,
}

impl InMemoryAssignmentProjectStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The project currently on file for `id`, if any (the last `save`,
    /// unless it was later `delete`d).
    #[must_use]
    pub fn last_saved(&self, id: &AssignmentId) -> Option<AssignmentProject> {
        self.projects.borrow().get(id).cloned()
    }

    /// Whether `id` was ever passed to [`AssignmentProjectStore::delete`].
    #[must_use]
    pub fn was_deleted(&self, id: &AssignmentId) -> bool {
        self.deleted.borrow().contains(id)
    }

    /// Makes the next [`AssignmentProjectStore::save`] call fail, then
    /// resets.
    pub fn fail_next_save(&self) {
        self.fail_next_save.set(true);
    }

    /// Makes the next [`AssignmentProjectStore::delete`] call fail, then
    /// resets.
    pub fn fail_next_delete(&self) {
        self.fail_next_delete.set(true);
    }

    /// Makes the next [`AssignmentProjectStore::load_all`] call fail, then
    /// resets — for exercising `LoadProjectError::Assignments`.
    pub fn fail_next_load(&self) {
        self.fail_next_load.set(true);
    }
}

impl AssignmentProjectStore for InMemoryAssignmentProjectStore {
    fn load_all(&self, _profile_dir: &Path) -> Result<Vec<AssignmentProject>, StoreError> {
        if self.fail_next_load.replace(false) {
            return Err(StoreError("simulated load failure".to_string()));
        }
        Ok(self.projects.borrow().values().cloned().collect())
    }

    fn save(&self, _profile_dir: &Path, project: &AssignmentProject) -> Result<(), StoreError> {
        if self.fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        self.projects
            .borrow_mut()
            .insert(project.id().clone(), project.clone());
        Ok(())
    }

    fn delete(&self, _profile_dir: &Path, id: &AssignmentId) -> Result<(), StoreError> {
        if self.fail_next_delete.replace(false) {
            return Err(StoreError("simulated delete failure".to_string()));
        }
        self.projects.borrow_mut().remove(id);
        self.deleted.borrow_mut().insert(id.clone());
        Ok(())
    }
}

/// Always returns the given `Result`, ignoring the input path — for
/// [`crate::use_cases::ImportGameLog`]'s own tests.
pub struct FakeGameLogReader {
    result: Result<ParsedGameLog, GameLogError>,
}

impl FakeGameLogReader {
    /// Builds a fake that always returns `result`.
    #[must_use]
    pub fn new(result: Result<ParsedGameLog, GameLogError>) -> Self {
        Self { result }
    }
}

impl GameLogReader for FakeGameLogReader {
    fn read(
        &self,
        _path: &Path,
        _formats: &LogFormats<'_>,
        _kind: KindChoice,
    ) -> Result<ParsedGameLog, GameLogError> {
        self.result.clone()
    }
}

/// A [`DefCacheCarrierProbe`] fake flagging exactly the given folders as
/// carriers — for [`crate::use_cases::FindDefCacheCarrier`]'s own tests
/// and for downstream composition-root tests (`apps/desktop`) that want a
/// carrier active without touching a real filesystem.
#[derive(Debug, Clone, Default)]
pub struct FakeDefCacheCarrierProbe {
    carrier_folders: BTreeSet<PathBuf>,
}

impl FakeDefCacheCarrierProbe {
    /// A fake with no carriers at all.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// A fake that flags exactly `folders` as carriers.
    #[must_use]
    pub fn with_carrier_folders(folders: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            carrier_folders: folders.into_iter().collect(),
        }
    }
}

impl DefCacheCarrierProbe for FakeDefCacheCarrierProbe {
    /// Ignores `carriers` beyond the caller's own "is it empty" decision:
    /// this fake stands in for the filesystem walk, not for the matching
    /// rule (which `rim-io`'s real probe owns and tests directly).
    fn has_def_cache_plugin(
        &self,
        loaded_folders: &[PathBuf],
        _carriers: &[DefCacheCarrier],
    ) -> bool {
        loaded_folders
            .iter()
            .any(|folder| self.carrier_folders.contains(folder))
    }
}

/// One invented [`DefCacheCarrier`] with every field filled, for tests
/// that need a non-empty carrier list without depending on what the
/// `rules` repo's `data/def-cache-carriers.json` happens to ship.
#[must_use]
pub fn example_carriers() -> Vec<DefCacheCarrier> {
    vec![DefCacheCarrier {
        id: "example".to_string(),
        plugin_dir: "Plugins".to_string(),
        file_prefix: "ExampleCache".to_string(),
        file_extension: ".dll".to_string(),
        recursive: true,
        log_line_prefix: "EXAMPLECACHE:".to_string(),
    }]
}

/// The three mod-produced log formats, as invented wording: a stack-trace
/// block, a texture fallback and a back-reference stub. They have the same
/// shapes as the `rules` repo's `data/log-shapes.json` rows (formats, not
/// mod names), so log fixtures written against them parse the same way, yet
/// no test depends on what that repo happens to ship.
#[must_use]
pub fn example_log_shapes() -> LogShapes {
    let mut shapes = LogShapes::empty();
    // The rows below are fixed literals in this test double; a row that
    // failed to parse is a bug in this function, caught by the first test
    // that touches it.
    #[allow(clippy::expect_used)]
    {
        let stack_block = PatchStackBlockShape::parse(&StackBlockSource {
            id: "example-stack-block",
            start: "[{mod:text} - Start of stack trace]",
            end: "[End of stack trace]",
            trailer: "Source file:{path:text}",
            xpath_detail_prefix: "xpath=",
            match_marker: "<match>",
            nomatch_marker: "<nomatch>",
        })
        .expect("the example stack block parses");
        shapes
            .push_stack_block(stack_block)
            .expect("an empty role has room");
        let texture_fallback = TextureFallbackShape::parse(&TextureFallbackSource {
            id: "example-texture-fallback",
            head: "DDS loading failed for '{path:text}': {reason:text}",
            dimensions: "DDS loading failed for '{path:text}': Cannot load compressed texture with non multiple of 4 dimensions of {width:int}x{height:int} and format {format:token}",
            trailer: "Loading from png instead.",
        })
        .expect("the example texture fallback parses");
        shapes
            .push_texture_fallback(texture_fallback)
            .expect("an empty role has room");
        let back_reference = BackReferenceShape::parse(
            "example-back-reference",
            "[Ref {id:hex}]",
            "[Ref {id:hex}] Duplicate stacktrace, see ref for original",
        )
        .expect("the example back-reference parses");
        shapes
            .push_back_reference(back_reference)
            .expect("an empty role has room");
    }
    shapes
}

/// Returns the given [`ImportedRules`], masked like the real importer: an
/// origin whose path is `None` comes back `None` with no provenance record,
/// and its snapshot is never made. [`Self::requests`] logs every call.
pub struct FakeRimSortImporter {
    result: ImportedRules,
    requests: RefCell<Vec<RimSortPaths>>,
    fail_next_import: Cell<bool>,
}

impl FakeRimSortImporter {
    /// Builds a fake that always returns `result`.
    #[must_use]
    pub fn new(result: ImportedRules) -> Self {
        Self {
            result,
            requests: RefCell::new(Vec::new()),
            fail_next_import: Cell::new(false),
        }
    }

    /// Makes the next [`RimSortImporter::import`] call fail once.
    pub fn fail_next_import(&self) {
        self.fail_next_import.set(true);
    }

    /// Every `paths` argument [`RimSortImporter::import`] was called with,
    /// in order.
    #[must_use]
    pub fn requests(&self) -> Vec<RimSortPaths> {
        self.requests.borrow().clone()
    }
}

impl RimSortImporter for FakeRimSortImporter {
    fn import(
        &self,
        paths: &RimSortPaths,
        _active: &BTreeMap<ModId, Option<u64>>,
    ) -> Result<ImportedRules, ImportError> {
        self.requests.borrow_mut().push(paths.clone());
        if self.fail_next_import.replace(false) {
            return Err(ImportError("injected importer failure".to_string()));
        }
        let mut result = self.result.clone();
        if paths.user_rules.is_none() {
            result.user_rules = None;
            result.provenance.remove(IMPORT_SOURCE_USER_RULES);
        }
        if paths.community_rules.is_none() {
            result.community_rules = None;
            result.provenance.remove(IMPORT_SOURCE_COMMUNITY_RULES);
        }
        if paths.steam_db.is_none() {
            result.steam_dependencies = None;
            result.provenance.remove(IMPORT_SOURCE_STEAM_DEPENDENCIES);
        }
        Ok(result)
    }
}

/// In-memory [`ImportManifestStore`] fake for
/// [`crate::use_cases::ImportRimSort`]'s own tests. `save` merges into
/// what's already "on file" — the same per-source union semantics the
/// real store has — so [`Self::saved`] always reflects every record ever
/// given across every call, never just the last one.
/// [`Self::fail_next_save`] injects a one-shot failure, for testing that
/// a manifest-write failure doesn't roll back an already-saved import.
#[derive(Default)]
pub struct InMemoryImportManifestStore {
    records: RefCell<BTreeMap<String, ImportRecord>>,
    fail_next_save: Cell<bool>,
    saves: Cell<usize>,
}

impl InMemoryImportManifestStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every record ever saved, merged across every call.
    #[must_use]
    pub fn saved(&self) -> BTreeMap<String, ImportRecord> {
        self.records.borrow().clone()
    }

    /// How many saves have succeeded.
    #[must_use]
    pub fn save_count(&self) -> usize {
        self.saves.get()
    }

    /// Makes the next [`ImportManifestStore::save`] call fail with a
    /// [`StoreError`], then resets.
    pub fn fail_next_save(&self) {
        self.fail_next_save.set(true);
    }
}

impl ImportManifestStore for InMemoryImportManifestStore {
    fn load(&self, _profile_dir: &Path) -> BTreeMap<String, ImportRecord> {
        self.records.borrow().clone()
    }

    fn save(
        &self,
        _profile_dir: &Path,
        records: &BTreeMap<String, ImportRecord>,
    ) -> Result<(), StoreError> {
        if self.fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        self.records.borrow_mut().extend(records.clone());
        self.saves.set(self.saves.get() + 1);
        Ok(())
    }
}

/// Scripted [`RuleDatabaseFetcher`] fake for
/// [`crate::use_cases::RefreshRuleDatabases`]'s own tests. `refresh`
/// returns exactly the outcome scripted for each requested database
/// (looked up by value, not call order) — asking for one with no
/// scripted outcome is a test-setup bug, so it panics loudly rather than
/// silently fabricating a `Skipped`. `call_count()`/`requests()` are the
/// load-bearing assertions: an outcome-only test would still pass if the
/// network-off guard
/// migrated from `RefreshRuleDatabases` into the adapter and the port
/// were called anyway — only the call count actually proves it wasn't.
pub struct FakeRuleDatabaseFetcher {
    outcomes: BTreeMap<RuleDatabase, RefreshOutcome>,
    omit: BTreeSet<RuleDatabase>,
    status: RefCell<Vec<DatabaseStatus>>,
    is_tracking_cache: bool,
    refresh_requests: RefCell<Vec<(PathBuf, Vec<RuleDatabase>)>>,
    status_requests: RefCell<Vec<BTreeMap<RuleDatabase, bool>>>,
}

impl FakeRuleDatabaseFetcher {
    /// Builds a fake that returns `outcomes[database]` for each database
    /// `refresh` is actually asked for.
    #[must_use]
    pub fn new(outcomes: BTreeMap<RuleDatabase, RefreshOutcome>) -> Self {
        Self {
            outcomes,
            omit: BTreeSet::new(),
            status: RefCell::new(Vec::new()),
            is_tracking_cache: false,
            refresh_requests: RefCell::new(Vec::new()),
            status_requests: RefCell::new(Vec::new()),
        }
    }

    /// Makes `refresh` silently return no entry at all for each database
    /// in `omit`, even when requested — simulating a
    /// [`RuleDatabaseFetcher`] implementation that violates its own
    /// contract ("one entry per requested database") for
    /// [`crate::use_cases::RefreshRuleDatabases`]'s own test of that
    /// case. Every other requested database still needs a scripted
    /// outcome as usual.
    #[must_use]
    pub fn omitting(mut self, omit: impl IntoIterator<Item = RuleDatabase>) -> Self {
        self.omit = omit.into_iter().collect();
        self
    }

    /// Sets what `status` returns (default: empty).
    #[must_use]
    pub fn with_status(mut self, status: Vec<DatabaseStatus>) -> Self {
        self.status = RefCell::new(status);
        self
    }

    /// Makes the fake behave like the real adapter's cache: `status`
    /// reports each row's `enabled` from the caller's map, and `refresh`
    /// records its outcome into the rows `status` returns (an
    /// `Updated` outcome caches the source and clears its last failure;
    /// `Failed` records the failure and leaves any older cached copy).
    /// Off by default, so a plain scripted `status` stays fixed.
    #[must_use]
    pub fn tracking_cache(mut self) -> Self {
        self.is_tracking_cache = true;
        self
    }

    /// The status rows as they stand now (after any tracked refreshes).
    #[must_use]
    pub fn status_rows(&self) -> Vec<DatabaseStatus> {
        self.status.borrow().clone()
    }

    /// How many times [`RuleDatabaseFetcher::refresh`] was called.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.refresh_requests.borrow().len()
    }

    /// Every `(cache_dir, databases)` pair [`RuleDatabaseFetcher::refresh`]
    /// was called with, in order.
    #[must_use]
    pub fn requests(&self) -> Vec<(PathBuf, Vec<RuleDatabase>)> {
        self.refresh_requests.borrow().clone()
    }

    /// Every `enabled` map [`RuleDatabaseFetcher::status`] was called
    /// with, in order.
    #[must_use]
    pub fn status_requests(&self) -> Vec<BTreeMap<RuleDatabase, bool>> {
        self.status_requests.borrow().clone()
    }
}

impl FakeRuleDatabaseFetcher {
    fn record_outcomes(&self, results: &[(RuleDatabase, RefreshOutcome)]) {
        let mut rows = self.status.borrow_mut();
        for (database, outcome) in results {
            let Some(row) = rows.iter_mut().find(|row| row.database == *database) else {
                continue;
            };
            match outcome {
                RefreshOutcome::Updated { sha256, bytes } => {
                    row.cached = Some(CachedDatabase {
                        sha256: sha256.clone(),
                        bytes: *bytes,
                        fetched_at: jiff::Timestamp::UNIX_EPOCH,
                    });
                    row.last_failure = None;
                }
                RefreshOutcome::Unchanged { .. } => row.last_failure = None,
                RefreshOutcome::Failed { failure } => row.last_failure = Some(failure.clone()),
                RefreshOutcome::Skipped { .. } => {}
            }
        }
    }
}

impl RuleDatabaseFetcher for FakeRuleDatabaseFetcher {
    fn status(
        &self,
        _cache_dir: &Path,
        enabled: &BTreeMap<RuleDatabase, bool>,
    ) -> Vec<DatabaseStatus> {
        self.status_requests.borrow_mut().push(enabled.clone());
        let mut rows = self.status.borrow().clone();
        if self.is_tracking_cache {
            for row in &mut rows {
                row.enabled = enabled.get(&row.database).copied().unwrap_or(false);
            }
        }
        rows
    }

    fn refresh(
        &self,
        cache_dir: &Path,
        databases: &[RuleDatabase],
    ) -> Vec<(RuleDatabase, RefreshOutcome)> {
        self.refresh_requests
            .borrow_mut()
            .push((cache_dir.to_path_buf(), databases.to_vec()));
        let results: Vec<(RuleDatabase, RefreshOutcome)> = databases
            .iter()
            .filter(|database| !self.omit.contains(database))
            .map(|database| {
                let outcome = self.outcomes.get(database).cloned().unwrap_or_else(|| {
                    panic!(
                        "FakeRuleDatabaseFetcher: no scripted outcome for {database:?} — every \
                         database `refresh` is actually called with must have one, unless it \
                         was explicitly passed to `omitting`"
                    )
                });
                (*database, outcome)
            })
            .collect();
        if self.is_tracking_cache {
            self.record_outcomes(&results);
        }
        results
    }
}

/// An in-memory per-profile notification state store: `load` returns
/// [`ProfileNotificationState::default`] until something has been saved.
/// [`InMemoryProfileNotificationStateStore::fail_next_save`] injects a
/// one-shot save failure; [`InMemoryProfileNotificationStateStore::save_count`]
/// counts successful saves, so a test can assert a call wrote nothing.
#[derive(Default)]
pub struct InMemoryProfileNotificationStateStore {
    state: RefCell<ProfileNotificationState>,
    should_fail_next_save: Cell<bool>,
    saves: Cell<usize>,
}

impl InMemoryProfileNotificationStateStore {
    /// Builds an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next [`ProfileNotificationStateStore::save`] fail with a
    /// [`StoreError`], then resets.
    pub fn fail_next_save(&self) {
        self.should_fail_next_save.set(true);
    }

    /// How many saves have succeeded.
    #[must_use]
    pub fn save_count(&self) -> usize {
        self.saves.get()
    }

    /// Replaces the stored state without counting as a save.
    pub fn seed(&self, state: ProfileNotificationState) {
        *self.state.borrow_mut() = state;
    }
}

impl ProfileNotificationStateStore for InMemoryProfileNotificationStateStore {
    fn load(&self, _profile_dir: &Path) -> ProfileNotificationState {
        self.state.borrow().clone()
    }

    fn save(
        &self,
        _profile_dir: &Path,
        state: &ProfileNotificationState,
    ) -> Result<(), StoreError> {
        if self.should_fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        *self.state.borrow_mut() = state.clone();
        self.saves.set(self.saves.get() + 1);
        Ok(())
    }
}

/// An in-memory [`AppSettingsStore`]. `load` answers `Missing` until
/// something is seeded or saved; [`Self::recovered`] makes it answer
/// `Recovered`. [`Self::fail_next_save`] injects a one-shot save failure
/// and [`Self::save_count`] counts successful saves.
#[derive(Default)]
pub struct InMemoryAppSettingsStore {
    settings: RefCell<Option<AppSettings>>,
    is_recovered: bool,
    should_fail_next_save: Cell<bool>,
    saves: Cell<usize>,
    racing_writer: RefCell<Option<AppSettings>>,
}

impl InMemoryAppSettingsStore {
    /// A store holding `settings`.
    #[must_use]
    pub fn loaded(settings: AppSettings) -> Self {
        Self {
            settings: RefCell::new(Some(settings)),
            ..Self::default()
        }
    }

    /// A store whose file is damaged (`load` answers `Recovered`).
    #[must_use]
    pub fn recovered() -> Self {
        Self {
            is_recovered: true,
            ..Self::default()
        }
    }

    /// A store whose first write finds `winner`'s file already there:
    /// `load` still answers `Missing`, then `save_if_missing` keeps the
    /// winner's settings and writes nothing.
    #[must_use]
    pub fn losing_a_race_to(winner: AppSettings) -> Self {
        Self {
            racing_writer: RefCell::new(Some(winner)),
            ..Self::default()
        }
    }

    /// A store whose first save fails, as [`Self::fail_next_save`] would.
    #[must_use]
    pub fn failing_next_save() -> Self {
        Self {
            should_fail_next_save: Cell::new(true),
            ..Self::default()
        }
    }

    /// Makes the next save fail with a [`StoreError`], then resets.
    pub fn fail_next_save(&self) {
        self.should_fail_next_save.set(true);
    }

    /// How many saves have succeeded.
    #[must_use]
    pub fn save_count(&self) -> usize {
        self.saves.get()
    }

    /// The settings last saved or seeded, if any.
    #[must_use]
    pub fn saved(&self) -> Option<AppSettings> {
        *self.settings.borrow()
    }
}

impl AppSettingsStore for InMemoryAppSettingsStore {
    fn load(&self, _base: &Path) -> AppSettingsLoad {
        if self.is_recovered {
            return AppSettingsLoad::Recovered {
                reason: "corrupt".to_string(),
            };
        }
        match *self.settings.borrow() {
            Some(settings) => AppSettingsLoad::Loaded(settings),
            None => AppSettingsLoad::Missing,
        }
    }

    fn save(&self, _base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
        if self.should_fail_next_save.replace(false) {
            return Err(StoreError("simulated save failure".to_string()));
        }
        *self.settings.borrow_mut() = Some(*settings);
        self.saves.set(self.saves.get() + 1);
        Ok(())
    }

    fn save_if_missing(&self, base: &Path, settings: &AppSettings) -> Result<(), StoreError> {
        if let Some(winner) = self.racing_writer.borrow_mut().take() {
            *self.settings.borrow_mut() = Some(winner);
            return Ok(());
        }
        // A damaged file is never overwritten by a pin.
        if self.is_recovered || self.settings.borrow().is_some() {
            return Ok(());
        }
        self.save(base, settings)
    }
}

/// The install folder [`FakeGameLauncher::executable`] reports.
pub const FAKE_LAUNCHER_INSTALL: &str = "launcher-install";

/// A [`GameLauncher`] fake: the route it reports is scripted (and can change
/// between calls), `launch` records the route kind it was asked to start
/// instead of starting anything, and an optional scripted failure makes
/// `launch` fail.
pub struct FakeGameLauncher {
    route: RefCell<Result<LaunchRoute, LaunchUnavailable>>,
    failure: Option<LaunchFailure>,
    launched: RefCell<Vec<LaunchRoute>>,
    route_queries: Cell<usize>,
}

impl FakeGameLauncher {
    fn with_route(route: Result<LaunchRoute, LaunchUnavailable>) -> Self {
        Self {
            route: RefCell::new(route),
            failure: None,
            launched: RefCell::new(Vec::new()),
            route_queries: Cell::new(0),
        }
    }

    /// Reports the Steam route.
    #[must_use]
    pub fn steam() -> Self {
        Self::with_route(Ok(LaunchRoute::Steam))
    }

    /// Reports the executable route for the invented install folder
    /// [`FAKE_LAUNCHER_INSTALL`], which differs from any folder a test passes
    /// as the game dir, so a launch built from the wrong source shows up.
    #[must_use]
    pub fn executable() -> Self {
        let executable = GameExecutable::of_install(Path::new(FAKE_LAUNCHER_INSTALL));
        Self::with_route(Ok(LaunchRoute::Executable(executable)))
    }

    /// Reports that the install can't be started from here.
    #[must_use]
    pub fn unavailable() -> Self {
        Self::with_route(Err(LaunchUnavailable::ExecutableMissing))
    }

    /// Makes every `launch` call fail with `failure`.
    #[must_use]
    pub fn failing_with(mut self, failure: LaunchFailure) -> Self {
        self.failure = Some(failure);
        self
    }

    /// From now on `route` reports the install as unavailable.
    pub fn become_unavailable(&self) {
        *self.route.borrow_mut() = Err(LaunchUnavailable::ExecutableMissing);
    }

    /// The route of every successful `launch` call, in order.
    #[must_use]
    pub fn launched(&self) -> Vec<LaunchRoute> {
        self.launched.borrow().clone()
    }

    /// How many times `route` was asked.
    #[must_use]
    pub fn route_queries(&self) -> usize {
        self.route_queries.get()
    }
}

impl GameLauncher for FakeGameLauncher {
    fn route(&self, _game_dir: &Path) -> Result<LaunchRoute, LaunchUnavailable> {
        self.route_queries.set(self.route_queries.get() + 1);
        self.route.borrow().clone()
    }

    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.launched.borrow_mut().push(route.clone());
        Ok(())
    }
}
