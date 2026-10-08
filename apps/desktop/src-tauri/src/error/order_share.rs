//! Mapping the load-order export and import failures.

use rim_session::mod_list::ExportListError;
use rim_session::ports::ModListFileError;
use rim_session::use_cases::{ExportOrderError, ImportOrderError, PreviewImportError};

use super::{CommandError, CommandErrorCode};

impl From<ModListFileError> for CommandError {
    fn from(error: ModListFileError) -> Self {
        Self::new(CommandErrorCode::ModListIoFailed, error.to_string())
    }
}

impl From<PreviewImportError> for CommandError {
    fn from(error: PreviewImportError) -> Self {
        match error {
            PreviewImportError::Unreadable(inner) => inner.into(),
        }
    }
}

impl From<ExportOrderError> for CommandError {
    /// A list over the size limit is `invalid_input` rather than a code of
    /// its own: no real install reaches it, and the message names the
    /// limit.
    fn from(error: ExportOrderError) -> Self {
        let message = error.to_string();
        match error {
            ExportOrderError::Config(inner) => inner.into(),
            ExportOrderError::Write(inner) => inner.into(),
            ExportOrderError::List(
                ExportListError::NothingActive | ExportListError::NothingRepresentable { .. },
            ) => Self::new(CommandErrorCode::NothingToExport, message),
            ExportOrderError::List(ExportListError::TooManyEntries { .. }) => {
                Self::invalid_input(message)
            }
        }
    }
}

impl From<ImportOrderError> for CommandError {
    /// An unknown, repeated, Core-less or oversized order means the
    /// inventory changed since the preview (or the caller is buggy):
    /// `invalid_input`.
    /// `ScanDidNotMatch` means the scan ran over something other than the
    /// validated order, which only a bug in this crate can cause:
    /// `internal`.
    fn from(error: ImportOrderError) -> Self {
        match error {
            ImportOrderError::Unknown(_)
            | ImportOrderError::Duplicate(_)
            | ImportOrderError::CoreMissing
            | ImportOrderError::NothingInstalled
            | ImportOrderError::TooMany { .. } => Self::invalid_input(error.to_string()),
            ImportOrderError::ScanDidNotMatch => Self::internal(error.to_string()),
        }
    }
}
