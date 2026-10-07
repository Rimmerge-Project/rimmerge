//! The port for a shared mod-list file (`.rml`, a `ModsConfig.xml`-shaped
//! list, or text), implemented by `rim-io`.

use std::path::Path;

use crate::mod_list::{ParsedModList, Rejection, SharedModList};

/// What reading a mod-list file produced. A document that cannot be read
/// as a list is an outcome the caller shows, not an error: the file was
/// read fine, it just is not an importable list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModListRead {
    /// The file held a list.
    Parsed(ParsedModList),
    /// The file is not an importable list.
    Rejected(Rejection),
}

/// A mod-list file could not be read or written (an I/O failure, not a
/// bad document).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ModListFileError(pub String);

/// Reads and writes a shared mod-list file.
pub trait ModListFileStore {
    /// Reads `path`, detecting its format by content (never by
    /// extension) and enforcing every bound in
    /// [`crate::mod_list::ModListLimits`].
    ///
    /// # Errors
    ///
    /// Returns [`ModListFileError`] when `path` cannot be opened or read.
    fn read(&self, path: &Path) -> Result<ModListRead, ModListFileError>;

    /// Writes `list` to `path` as a RimWorld mod list (`.rml`),
    /// replacing any file there atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ModListFileError`] when the file cannot be written.
    fn write(&self, path: &Path, list: &SharedModList) -> Result<(), ModListFileError>;
}
