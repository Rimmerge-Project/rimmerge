//! [`CopyOrganization`]: copies groups and colours from another profile's
//! `organization.json` into this profile's organization, in one save.

use std::collections::BTreeSet;
use std::path::Path;

use crate::Session;
use crate::organization::{
    CopyMode, CopyPlan, CopyReport, OrganizationError, OrganizationIssue, OrganizationLoad,
    OrganizationUnavailable, OrganizedModId, plan_copy,
};
use crate::ports::{OrganizationSaveError, OrganizationStore, StoreError};

/// Why a copy was refused. Nothing was changed, here or in the source.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CopyRefusal {
    /// The folder holds no `organization.json`.
    #[error("that folder has no organization file")]
    SourceMissing,
    /// The folder's organization file cannot be used (unreadable, too
    /// large, malformed, newer, or breaking a rule): the same checks that
    /// guard this profile's own file.
    #[error("the organization file in that folder cannot be used: {0}")]
    SourceUnusable(OrganizationIssue),
    /// The source has no groups and no colours, so there is nothing to copy.
    #[error("the organization in that folder has no groups and no colours")]
    SourceEmpty,
    /// The source is exactly this profile's organization (an alias of its own
    /// folder, or an identical copy), so there is nothing to copy. Said apart
    /// from "this profile's own folder" because the folder may be another one.
    #[error("the organization in that folder is the same as this profile's")]
    NothingToCopy,
    /// The result would break an organization limit.
    #[error("the result would break a limit: {0}")]
    Limit(OrganizationError),
    /// The result would be larger than the organization file may be.
    #[error("the result is {bytes} bytes, more than the {limit} bytes the file may hold")]
    TooLargeToSave {
        /// The size the document would have.
        bytes: usize,
        /// The most bytes the file may hold.
        limit: usize,
    },
}

/// Why a copy did not happen or could not be previewed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CopyOrganizationError {
    /// This profile's organization is not loaded or is read-only.
    #[error(transparent)]
    Unavailable(#[from] OrganizationUnavailable),
    /// The copy was refused; nothing changed.
    #[error(transparent)]
    Refused(#[from] CopyRefusal),
    /// Writing `organization.json` failed; the session is unchanged.
    #[error("saving the organization: {0}")]
    Save(StoreError),
}

/// Plans and applies a copy of another profile's organization. Reads the
/// source through [`OrganizationStore::load`] (so its size bound and
/// validation apply) and never writes there; saves only this profile's
/// file, once.
///
/// Organization is a separate store the sort path cannot read, so this
/// touches no order, no working set, no rules and no `ModsConfig.xml`.
pub struct CopyOrganization<Store> {
    store: Store,
}

impl<Store: OrganizationStore> CopyOrganization<Store> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Reads the source profile's organization. Touches no session, so an
    /// interface calls it outside the session lock, then hands the result to
    /// [`Self::preview_loaded`] or [`Self::execute_loaded`] under it.
    #[must_use]
    pub fn read_source(&self, source_dir: &Path) -> OrganizationLoad {
        self.store.load(source_dir)
    }

    /// What copying `source_dir` in `mode` would do. Changes nothing.
    ///
    /// # Errors
    ///
    /// [`CopyOrganizationError::Unavailable`] unless this organization is
    /// loaded, [`CopyOrganizationError::Refused`] with the reason otherwise.
    pub fn preview(
        &self,
        session: &Session,
        source_dir: &Path,
        mode: CopyMode,
    ) -> Result<CopyReport, CopyOrganizationError> {
        // The cheap availability check first: an unavailable organization
        // refuses the copy before the source is read.
        session.organization().loaded()?;
        self.preview_loaded(session, self.read_source(source_dir), mode)
    }

    /// [`Self::preview`] over a source already read with
    /// [`Self::read_source`].
    ///
    /// # Errors
    ///
    /// As [`Self::preview`].
    pub fn preview_loaded(
        &self,
        session: &Session,
        source: OrganizationLoad,
        mode: CopyMode,
    ) -> Result<CopyReport, CopyOrganizationError> {
        Ok(self.plan(session, source, mode)?.report)
    }

    /// Plans afresh from the source as it is now, then saves the result
    /// once and swaps it into the session. A merge that changes nothing
    /// writes nothing.
    ///
    /// # Errors
    ///
    /// As [`Self::preview`], plus [`CopyOrganizationError::Save`] when the
    /// write fails (the session keeps what it had).
    pub fn execute(
        &self,
        session: &mut Session,
        source_dir: &Path,
        mode: CopyMode,
    ) -> Result<CopyReport, CopyOrganizationError> {
        session.organization().loaded()?;
        self.execute_loaded(session, self.read_source(source_dir), mode)
    }

    /// [`Self::execute`] over a source already read with
    /// [`Self::read_source`]. The plan is made against the session as it is
    /// now, so an edit made after the source was read is merged with, never
    /// overwritten.
    ///
    /// # Errors
    ///
    /// As [`Self::execute`].
    pub fn execute_loaded(
        &self,
        session: &mut Session,
        source: OrganizationLoad,
        mode: CopyMode,
    ) -> Result<CopyReport, CopyOrganizationError> {
        let CopyPlan { result, report } = self.plan(session, source, mode)?;
        let profile_dir = session.paths().profile_dir.clone();
        let organization = session.organization_for_edit()?;
        if *organization == result {
            return Ok(report);
        }
        self.store
            .save(&profile_dir, &result)
            .map_err(|error| match error {
                OrganizationSaveError::TooLarge { bytes, limit } => {
                    CopyOrganizationError::Refused(CopyRefusal::TooLargeToSave { bytes, limit })
                }
                OrganizationSaveError::Failed(inner) => CopyOrganizationError::Save(inner),
            })?;
        *organization = result;
        Ok(report)
    }

    fn plan(
        &self,
        session: &Session,
        source: OrganizationLoad,
        mode: CopyMode,
    ) -> Result<CopyPlan, CopyOrganizationError> {
        let target = session.organization().loaded()?;
        let source = match source {
            OrganizationLoad::Missing => return Err(CopyRefusal::SourceMissing.into()),
            OrganizationLoad::Unusable(issue) => {
                return Err(CopyRefusal::SourceUnusable(issue).into());
            }
            OrganizationLoad::Loaded(source) if source.is_empty() => {
                return Err(CopyRefusal::SourceEmpty.into());
            }
            OrganizationLoad::Loaded(source) => source,
        };
        if *target == source {
            return Err(CopyRefusal::NothingToCopy.into());
        }
        let mut plan = plan_copy(target, &source, mode).map_err(CopyRefusal::Limit)?;
        let members: BTreeSet<OrganizedModId> = source
            .groups()
            .flat_map(|(_, group)| group.members().iter().cloned())
            .collect();
        plan.report.not_installed_mods = session.count_not_installed(&members);
        Ok(plan)
    }
}

#[cfg(test)]
#[path = "copy_organization_tests.rs"]
mod tests;
