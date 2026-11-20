//! [`EditOrganization`]: applies one [`OrganizationEdit`] to the session's
//! organization and saves `organization.json` once.

use crate::Session;
use crate::organization::{
    EditOutcome, OrganizationEdit, OrganizationError, OrganizationUnavailable,
};
use crate::ports::{OrganizationSaveError, OrganizationStore};

/// Why an organization edit did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditOrganizationError {
    /// Organization is not loaded or is read-only after an unusable file.
    #[error(transparent)]
    Unavailable(#[from] OrganizationUnavailable),
    /// An organization rule refused the edit; nothing changed.
    #[error(transparent)]
    Rejected(#[from] OrganizationError),
    /// Saving `organization.json` failed or the document would not fit the
    /// file ([`OrganizationSaveError::TooLarge`]); the edit was rolled back.
    #[error("saving the organization: {0}")]
    Save(#[from] OrganizationSaveError),
}

/// Applies an [`OrganizationEdit`], persists it through
/// [`OrganizationStore`], and rolls the session back when the save fails.
///
/// Organization is a separate store the sort path cannot read, so this
/// touches no cache, no order, no working set and no `ModsConfig.xml`. An
/// edit that changes nothing (adding mods that are already members) does
/// not write the file.
pub struct EditOrganization<Store> {
    store: Store,
}

impl<Store: OrganizationStore> EditOrganization<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// # Errors
    ///
    /// [`EditOrganizationError::Unavailable`] unless the organization is
    /// loaded (nothing is saved), [`EditOrganizationError::Rejected`] when
    /// a rule refuses the edit, and [`EditOrganizationError::Save`] when the
    /// save fails or the document would not fit the file, in which case the
    /// edit is rolled back so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        edit: OrganizationEdit,
    ) -> Result<EditOutcome, EditOrganizationError> {
        let profile_dir = session.paths().profile_dir.clone();
        let organization = session.organization_for_edit()?;
        let snapshot = organization.clone();
        let outcome = edit.apply(organization)?;
        if *organization == snapshot {
            return Ok(outcome);
        }
        if let Err(error) = self.store.save(&profile_dir, organization) {
            *organization = snapshot;
            return Err(error.into());
        }
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "edit_organization_tests.rs"]
mod tests;
