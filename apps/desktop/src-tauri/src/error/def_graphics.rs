//! Mapping `resolve_def_graphic`/`read_def_texture`'s errors.

use rim_session::ports::DefSourceError;
use rim_session::use_cases::{ReadDefTextureError, ResolveDefGraphicError};

use super::{CommandError, CommandErrorCode};

impl From<ResolveDefGraphicError> for CommandError {
    fn from(error: ResolveDefGraphicError) -> Self {
        match error {
            // The same mapping `inspect_def` gives the same failures.
            ResolveDefGraphicError::Inspect(inspect) => inspect.into(),
        }
    }
}

impl From<ReadDefTextureError> for CommandError {
    fn from(error: ReadDefTextureError) -> Self {
        let message = error.to_string();
        match error {
            ReadDefTextureError::Resolve(resolve) => resolve.into(),
            // A key the def's own resolution did not produce: a forged or
            // stale request, not something a reload fixes.
            ReadDefTextureError::KeyNotInGraphic(_) => Self::invalid_input(message),
            ReadDefTextureError::UnknownOwner(owner) => Self::mod_not_found(owner.as_str()),
            ReadDefTextureError::Locate(source) => match source {
                DefSourceError::UnsupportedFormat { .. } => {
                    Self::new(CommandErrorCode::TextureUnsupportedFormat, message)
                }
                DefSourceError::Io { .. }
                | DefSourceError::Stale { .. }
                | DefSourceError::Xml { .. } => {
                    Self::new(CommandErrorCode::MergeSourceFailed, message)
                }
                // A property of the file itself, not an I/O condition: the
                // same `invalid_input` `read_texture` gives it
                // (`error/merge.rs`; see `DefSourceError::TooLarge`).
                DefSourceError::TooLarge { .. } => Self::invalid_input(message),
            },
        }
    }
}
