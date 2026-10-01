//! Mapping scan, config, store, import, active-set, project-load, and apply errors.

use super::{CommandError, CommandErrorCode};

impl From<rim_session::ports::ScanError> for CommandError {
    fn from(error: rim_session::ports::ScanError) -> Self {
        Self::new(CommandErrorCode::ScanFailed, error.to_string())
    }
}

impl From<rim_session::ports::ConfigError> for CommandError {
    fn from(error: rim_session::ports::ConfigError) -> Self {
        Self::new(CommandErrorCode::ModsConfigIoFailed, error.to_string())
    }
}

impl From<rim_session::ports::StoreError> for CommandError {
    fn from(error: rim_session::ports::StoreError) -> Self {
        Self::new(CommandErrorCode::ProfileIoFailed, error.to_string())
    }
}

impl From<rim_session::ports::ImportError> for CommandError {
    fn from(error: rim_session::ports::ImportError) -> Self {
        Self::new(CommandErrorCode::RimsortImportFailed, error.to_string())
    }
}

impl From<rim_session::ActiveSetError> for CommandError {
    /// [`rim_session::ActiveSetError::Unknown`]/`::Duplicate` are both a
    /// caller-supplied id failing a domain validation rule — the same
    /// `invalid_input` category `SectionError::AlreadyExists` already
    /// uses for an equivalent case, not a dedicated code: the frontend
    /// only ever renders the message, never branches on which of the two
    /// it was.
    fn from(error: rim_session::ActiveSetError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_session::use_cases::LoadProjectError> for CommandError {
    fn from(error: rim_session::use_cases::LoadProjectError) -> Self {
        use rim_session::use_cases::LoadProjectError as E;
        match error {
            E::Config(e) => e.into(),
            E::Scan(e) => e.into(),
            E::Decisions(e) | E::Rules(e) | E::Patches(e) | E::Assignments(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::ImportRimSortError> for CommandError {
    fn from(error: rim_session::use_cases::ImportRimSortError) -> Self {
        use rim_session::use_cases::ImportRimSortError as E;
        match error {
            E::Import(e) => e.into(),
            E::Store(e) => e.into(),
            // A manifest-write failure is not a failed
            // import — `rule_store.save` already succeeded and the
            // session already reflects the new rules, so the message
            // must say that explicitly rather than reusing `Store`'s own
            // bare "saving imported rules: {0}" framing, which would
            // tell the user the opposite of what happened at the one
            // moment they most need to know whether to re-import.
            E::Manifest(e) => Self::new(
                CommandErrorCode::ProfileIoFailed,
                format!(
                    "the import saved successfully; recording its provenance record failed: {e}"
                ),
            ),
        }
    }
}

impl From<rim_session::use_cases::ApplyError> for CommandError {
    fn from(error: rim_session::use_cases::ApplyError) -> Self {
        use rim_session::use_cases::ApplyError as E;
        match error {
            E::Decisions(e) | E::Rules(e) => e.into(),
            E::Config(e) => e.into(),
            E::Plan(e) => e.into(),
            // Locating a `ShipAsset` texture failed — the same
            // "something changed since the scan" family as `E::Plan`,
            // but over a texture file rather than a def/patch element.
            E::Asset(e) => Self::new(CommandErrorCode::MergeSourceFailed, e.to_string()),
            // `render` only ever receives plans `RenderMergeMod` already
            // filtered down to `MergeState::Complete`, so this is
            // structurally unreachable from any command — `internal`
            // names that rather than inventing a dedicated code for a
            // path that can't actually be hit.
            E::Emit(e) => Self::internal(e.to_string()),
            E::MergeMod(e) => Self::new(CommandErrorCode::MergeModIoFailed, e.to_string()),
            E::StaleActiveSet { added, removed } => Self::new(
                CommandErrorCode::StaleActiveSet,
                E::StaleActiveSet { added, removed }.to_string(),
            ),
        }
    }
}
