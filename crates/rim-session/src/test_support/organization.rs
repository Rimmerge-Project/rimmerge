//! An in-memory [`OrganizationStore`], and builders for organization tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::organization::{
    ModOrganization, OrganizationIssue, OrganizationLoad, OrganizationState, OrganizedModId,
};
use crate::ports::{OrganizationSaveError, OrganizationStore, StoreError};

/// An in-memory organization store. `load` answers what it was seeded with
/// until a `save` replaces it. Thread-safe, so a desktop test can hand it
/// to the app as `Arc<dyn OrganizationStore + Send + Sync>`.
/// [`Self::fail_next_save`]/[`Self::fail_next_archive`] inject one-shot
/// failures for rollback tests.
pub struct InMemoryOrganizationStore {
    current: Mutex<OrganizationLoad>,
    fail_next_save: AtomicBool,
    too_large_next_save: AtomicBool,
    fail_next_archive: AtomicBool,
    /// Applied (as if another writer saved it) right after the next `load`
    /// answers, to simulate an edit landing after a read.
    change_after_next_load: Mutex<Option<ModOrganization>>,
    saves: AtomicUsize,
    archives: AtomicUsize,
    /// Other directories' files: `load` of one of these answers its own
    /// seed, so a test can read a second profile beside the first.
    other_dirs: Mutex<BTreeMap<PathBuf, OrganizationLoad>>,
    saved_dirs: Mutex<Vec<PathBuf>>,
}

impl Default for InMemoryOrganizationStore {
    fn default() -> Self {
        Self::seeded(OrganizationLoad::Missing)
    }
}

impl InMemoryOrganizationStore {
    /// A store with no file yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose `load` answers `load` until something is saved.
    #[must_use]
    pub fn seeded(load: OrganizationLoad) -> Self {
        Self {
            current: Mutex::new(load),
            fail_next_save: AtomicBool::new(false),
            too_large_next_save: AtomicBool::new(false),
            fail_next_archive: AtomicBool::new(false),
            change_after_next_load: Mutex::new(None),
            saves: AtomicUsize::new(0),
            archives: AtomicUsize::new(0),
            other_dirs: Mutex::new(BTreeMap::new()),
            saved_dirs: Mutex::new(Vec::new()),
        }
    }

    /// A store holding an unusable file.
    #[must_use]
    pub fn unusable(issue: OrganizationIssue) -> Self {
        Self::seeded(OrganizationLoad::Unusable(issue))
    }

    /// The organization last saved, if a save has happened.
    #[must_use]
    pub fn last_saved(&self) -> Option<ModOrganization> {
        match &*self.lock() {
            OrganizationLoad::Loaded(organization) if self.save_count() > 0 => {
                Some(organization.clone())
            }
            OrganizationLoad::Loaded(_)
            | OrganizationLoad::Missing
            | OrganizationLoad::Unusable(_) => None,
        }
    }

    /// How many saves succeeded.
    #[must_use]
    pub fn save_count(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }

    /// How many archive copies succeeded.
    #[must_use]
    pub fn archive_count(&self) -> usize {
        self.archives.load(Ordering::SeqCst)
    }

    /// Makes the next `save` fail, then resets.
    pub fn fail_next_save(&self) {
        self.fail_next_save.store(true, Ordering::SeqCst);
    }

    /// Makes the next `save` refuse the document as larger than the file
    /// may hold, then resets.
    pub fn fail_next_save_too_large(&self) {
        self.too_large_next_save.store(true, Ordering::SeqCst);
    }

    /// Makes the next `archive_unusable` fail, then resets.
    pub fn fail_next_archive(&self) {
        self.fail_next_archive.store(true, Ordering::SeqCst);
    }

    /// Makes the backing data become `organization` right after the next
    /// `load` has answered (the load itself still returns the old data):
    /// an edit that landed between a read and whatever the caller does
    /// next. Counts no save.
    pub fn change_after_next_load(&self, organization: ModOrganization) {
        *self
            .change_after_next_load
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(organization);
    }

    /// Makes `load(dir)` answer `load` for that directory (every other
    /// directory still answers the store's own file).
    pub fn seed_dir(&self, dir: impl Into<PathBuf>, load: OrganizationLoad) {
        self.other_dirs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(dir.into(), load);
    }

    /// The directory of every successful `save`, in order.
    #[must_use]
    pub fn saved_dirs(&self) -> Vec<PathBuf> {
        self.saved_dirs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, OrganizationLoad> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl OrganizationStore for InMemoryOrganizationStore {
    fn load(&self, dir: &Path) -> OrganizationLoad {
        if let Some(seeded) = self
            .other_dirs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(dir)
        {
            return seeded.clone();
        }
        let answer = self.lock().clone();
        let late_change = self
            .change_after_next_load
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(organization) = late_change {
            *self.lock() = OrganizationLoad::Loaded(organization);
        }
        answer
    }

    fn save(
        &self,
        dir: &Path,
        organization: &ModOrganization,
    ) -> Result<(), OrganizationSaveError> {
        if self.too_large_next_save.swap(false, Ordering::SeqCst) {
            return Err(OrganizationSaveError::TooLarge { bytes: 2, limit: 1 });
        }
        if self.fail_next_save.swap(false, Ordering::SeqCst) {
            return Err(StoreError("simulated save failure".to_string()).into());
        }
        *self.lock() = OrganizationLoad::Loaded(organization.clone());
        self.saves.fetch_add(1, Ordering::SeqCst);
        self.saved_dirs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(dir.to_path_buf());
        Ok(())
    }

    fn archive_unusable(&self, _dir: &Path) -> Result<(), StoreError> {
        if self.fail_next_archive.swap(false, Ordering::SeqCst) {
            return Err(StoreError("simulated archive failure".to_string()));
        }
        self.archives.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// The organized identity of a fixture id.
///
/// # Panics
///
/// When `id` is not a valid organized id; fixture ids always are.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a test double: a fixture id that fails to parse is a bug in the test"
)]
pub fn organized(id: &str) -> OrganizedModId {
    OrganizedModId::new(&ModId::new(id)).expect("fixture id is valid")
}

/// Sets a fixture session's organization state. The session's own setter
/// is crate-private (only a store load or a reset may change the state in
/// production); this is the fixture door for tests in other crates.
pub fn set_organization_state(session: &mut Session, state: OrganizationState) {
    session.set_organization_state(state);
}
