//! What a session knows about its organization: loaded, never loaded, or
//! a file it could not use.

use thiserror::Error;

use super::{ModOrganization, OrganizationDocumentError, OrganizationError};

/// Why `organization.json` could not be used. A state the session models,
/// not an error: the project still loads, organization is just read-only.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationIssue {
    /// The file exists but could not be read.
    #[error("the organization file could not be read")]
    Unreadable,
    /// The file is larger than the reader accepts.
    #[error("the organization file is larger than {limit_bytes} bytes")]
    TooLarge {
        /// [`super::OrganizationLimits::MAX_FILE_BYTES`].
        limit_bytes: usize,
    },
    /// The file is not valid JSON of the expected shape, or holds a value
    /// that is not valid (a bad name, id or colour).
    #[error("the organization file is not valid")]
    Malformed,
    /// The file was written by a newer version of the app.
    #[error("the organization file has version {found}, which this version does not read")]
    NewerVersion {
        /// The version the file states.
        found: u32,
    },
    /// The file parsed but describes a forest that breaks a rule.
    #[error("the organization file breaks a rule: {0}")]
    Invalid(OrganizationDocumentError),
}

/// What [`crate::ports::OrganizationStore::load`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizationLoad {
    /// No file yet: a fresh profile.
    Missing,
    /// A valid organization.
    Loaded(ModOrganization),
    /// A file that cannot be used.
    Unusable(OrganizationIssue),
}

/// The session's organization. A closed state: writes are only possible
/// from [`OrganizationState::Loaded`], so a session built without the
/// store, or one that could not read the file, can never save an empty
/// organization over the user's data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OrganizationState {
    /// The session was built without an organization store (the CLI, most
    /// tests).
    #[default]
    NotLoaded,
    /// Read, or created fresh when the profile had none.
    Loaded(ModOrganization),
    /// The file is present but unusable; organization is read-only until
    /// the user starts over.
    Recovered {
        /// Why the file could not be used.
        issue: OrganizationIssue,
    },
}

impl From<OrganizationLoad> for OrganizationState {
    fn from(load: OrganizationLoad) -> Self {
        match load {
            OrganizationLoad::Missing => Self::Loaded(ModOrganization::new()),
            OrganizationLoad::Loaded(organization) => Self::Loaded(organization),
            OrganizationLoad::Unusable(issue) => Self::Recovered { issue },
        }
    }
}

/// Organization cannot be read or written in this session.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationUnavailable {
    /// The session was built without an organization store.
    #[error("organization is not loaded in this session")]
    NotLoaded,
    /// The organization file could not be used.
    #[error("organization is read-only: {0}")]
    Recovered(OrganizationIssue),
}

impl OrganizationState {
    /// The loaded organization.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationUnavailable`] unless the state is `Loaded`.
    pub fn loaded(&self) -> Result<&ModOrganization, OrganizationUnavailable> {
        match self {
            Self::Loaded(organization) => Ok(organization),
            Self::NotLoaded => Err(OrganizationUnavailable::NotLoaded),
            Self::Recovered { issue } => Err(OrganizationUnavailable::Recovered(issue.clone())),
        }
    }

    /// The loaded organization, for editing.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationUnavailable`] unless the state is `Loaded`.
    pub(crate) fn loaded_mut(&mut self) -> Result<&mut ModOrganization, OrganizationUnavailable> {
        match self {
            Self::Loaded(organization) => Ok(organization),
            Self::NotLoaded => Err(OrganizationUnavailable::NotLoaded),
            Self::Recovered { issue } => Err(OrganizationUnavailable::Recovered(issue.clone())),
        }
    }
}

/// A read of the organization was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationQueryError {
    /// Organization is not available in this session.
    #[error(transparent)]
    Unavailable(#[from] OrganizationUnavailable),
    /// The organization refused the question (an unknown group).
    #[error(transparent)]
    Rejected(#[from] OrganizationError),
}
