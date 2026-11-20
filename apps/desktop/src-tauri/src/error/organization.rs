//! Mapping the library organization failures (groups and mod colours).

use rim_session::ModListError;
use rim_session::organization::{
    OrganizationError, OrganizationQueryError, OrganizationUnavailable,
};
use rim_session::ports::OrganizationSaveError;
use rim_session::use_cases::{
    EditOrganizationError, RemoveUninstalledMembersError, ResetOrganizationError,
};

use super::{CommandError, CommandErrorCode};

impl From<OrganizationUnavailable> for CommandError {
    fn from(error: OrganizationUnavailable) -> Self {
        Self::new(CommandErrorCode::OrganizationUnavailable, error.to_string())
    }
}

impl From<OrganizationError> for CommandError {
    /// A missing group, a taken name, a cycle and a hit limit each have a
    /// code the interface words. `SameGroup` is a request no screen builds,
    /// so `invalid_input`. The document-only failures live in
    /// `OrganizationDocumentError` and never reach an edit, so no arm here
    /// is `internal`.
    fn from(error: OrganizationError) -> Self {
        let message = error.to_string();
        match error {
            OrganizationError::GroupNotFound(_) => {
                Self::new(CommandErrorCode::GroupNotFound, message)
            }
            OrganizationError::NameTaken { .. } => {
                Self::new(CommandErrorCode::GroupNameTaken, message)
            }
            OrganizationError::WouldCreateCycle => Self::new(CommandErrorCode::GroupCycle, message),
            OrganizationError::TooDeep { .. }
            | OrganizationError::TooManyGroups { .. }
            | OrganizationError::GroupIdsExhausted
            | OrganizationError::TooManyMembers { .. }
            | OrganizationError::TooManyColors { .. } => {
                Self::new(CommandErrorCode::OrganizationLimit, message)
            }
            OrganizationError::SameGroup => Self::invalid_input(message),
        }
    }
}

impl From<OrganizationQueryError> for CommandError {
    fn from(error: OrganizationQueryError) -> Self {
        match error {
            OrganizationQueryError::Unavailable(inner) => inner.into(),
            OrganizationQueryError::Rejected(inner) => inner.into(),
        }
    }
}

impl From<OrganizationSaveError> for CommandError {
    /// A document over the file's size limit is a limit the user can act on;
    /// any other failure is the profile's I/O.
    fn from(error: OrganizationSaveError) -> Self {
        match error {
            too_large @ OrganizationSaveError::TooLarge { .. } => {
                Self::new(CommandErrorCode::OrganizationLimit, too_large.to_string())
            }
            OrganizationSaveError::Failed(inner) => inner.into(),
        }
    }
}

impl From<EditOrganizationError> for CommandError {
    fn from(error: EditOrganizationError) -> Self {
        match error {
            EditOrganizationError::Unavailable(inner) => inner.into(),
            EditOrganizationError::Rejected(inner) => inner.into(),
            EditOrganizationError::Save(inner) => inner.into(),
        }
    }
}

impl From<RemoveUninstalledMembersError> for CommandError {
    fn from(error: RemoveUninstalledMembersError) -> Self {
        match error {
            RemoveUninstalledMembersError::Query(inner) => inner.into(),
            RemoveUninstalledMembersError::Edit(inner) => inner.into(),
        }
    }
}

impl From<ResetOrganizationError> for CommandError {
    /// "Start over" with nothing to start over from is a request no screen
    /// builds (the button only shows while recovered): `invalid_input`.
    fn from(error: ResetOrganizationError) -> Self {
        let message = error.to_string();
        match error {
            ResetOrganizationError::NotRecovered => Self::invalid_input(message),
            ResetOrganizationError::Archive(inner) => inner.into(),
            ResetOrganizationError::Store(inner) => inner.into(),
        }
    }
}

impl From<ModListError> for CommandError {
    fn from(error: ModListError) -> Self {
        let message = error.to_string();
        match error {
            ModListError::UnknownGroup(_) => Self::new(CommandErrorCode::GroupNotFound, message),
            ModListError::OrganizationUnavailable(inner) => inner.into(),
        }
    }
}
