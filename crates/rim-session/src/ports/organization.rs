//! The port for `<profile>/organization.json`, implemented by `rim-io`.

use std::path::Path;
use std::sync::Arc;

use super::StoreError;
use crate::organization::{ModOrganization, OrganizationLoad};

/// Why saving `organization.json` failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrganizationSaveError {
    /// The document is larger than the file may hold (the reader would
    /// refuse it). A limit the user can act on, not an I/O failure.
    #[error("the organization is {bytes} bytes, more than the {limit} bytes the file may hold")]
    TooLarge {
        /// The size the document would have.
        bytes: usize,
        /// The most bytes the file may hold.
        limit: usize,
    },
    /// The write itself failed.
    #[error(transparent)]
    Failed(#[from] StoreError),
}

/// Loads and saves a profile's organization (groups and mod colours).
///
/// `load` has no error type: every outcome is a state the session models
/// ([`OrganizationLoad`]), so a damaged file never fails a project load.
pub trait OrganizationStore {
    /// Reads the profile's organization. A missing file is
    /// [`OrganizationLoad::Missing`]; a file that cannot be used is
    /// [`OrganizationLoad::Unusable`] with the reason.
    fn load(&self, dir: &Path) -> OrganizationLoad;

    /// Writes the organization, replacing the file atomically.
    ///
    /// # Errors
    ///
    /// [`OrganizationSaveError::TooLarge`] when the document is over the
    /// file's size limit (nothing is written), [`OrganizationSaveError::Failed`]
    /// when the write fails.
    fn save(&self, dir: &Path, organization: &ModOrganization)
    -> Result<(), OrganizationSaveError>;

    /// Keeps a copy of the unusable file beside it
    /// (`organization.json.unusable-<unix seconds>`) so "start over" loses
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the copy fails (the caller then does
    /// not overwrite the file).
    fn archive_unusable(&self, dir: &Path) -> Result<(), StoreError>;
}

/// Borrow-lending impl, the same convention as `RuleStore for &T`: a test
/// shares one in-memory store with the use case under test.
impl<T: OrganizationStore + ?Sized> OrganizationStore for &T {
    fn load(&self, dir: &Path) -> OrganizationLoad {
        (**self).load(dir)
    }

    fn save(
        &self,
        dir: &Path,
        organization: &ModOrganization,
    ) -> Result<(), OrganizationSaveError> {
        (**self).save(dir, organization)
    }

    fn archive_unusable(&self, dir: &Path) -> Result<(), StoreError> {
        (**self).archive_unusable(dir)
    }
}

/// Shared-handle impl, so the desktop app can hand one `Arc<dyn
/// OrganizationStore + Send + Sync>` to every use case.
impl<T: OrganizationStore + ?Sized> OrganizationStore for Arc<T> {
    fn load(&self, dir: &Path) -> OrganizationLoad {
        (**self).load(dir)
    }

    fn save(
        &self,
        dir: &Path,
        organization: &ModOrganization,
    ) -> Result<(), OrganizationSaveError> {
        (**self).save(dir, organization)
    }

    fn archive_unusable(&self, dir: &Path) -> Result<(), StoreError> {
        (**self).archive_unusable(dir)
    }
}
