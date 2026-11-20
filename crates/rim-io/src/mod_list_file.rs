//! [`RmlFileStore`]: reads and writes a shared mod-list file.
//!
//! Reading detects the document by content, never by extension: after a
//! BOM and whitespace, a first `<` means XML (a RimWorld `.rml`
//! `<savedModList>`, a `ModsConfig.xml`-shaped `<ModsConfigData>` such
//! as RimSort's "export list", or a RimWorld save `<savegame>`, of which
//! only the `<meta>` header is read), anything else is the text format. The
//! input is untrusted, so `ModListLimits` bounds the bytes read, the
//! XML node count, the nesting depth and the entry count before anything
//! grows, and a DTD or DOCTYPE is refused outright.
//!
//! Writing produces a `.rml` byte for byte as RimWorld's own "Save list"
//! does (UTF-8 with BOM, CRLF, tab indentation, `meta` and `modList`),
//! atomically.

mod read;
mod save_header;
mod sniff;
mod stream;
mod write;

use std::fs::File;
use std::path::Path;

use rim_session::mod_list::SharedModList;
use rim_session::ports::{ModListFileError, ModListFileStore, ModListRead};

use crate::atomic::write_atomically;

pub use stream::read_mod_list;

/// Reads and writes `.rml` mod lists, plus the other list shapes
/// [`ModListFileStore::read`] accepts.
#[derive(Debug, Default, Clone, Copy)]
pub struct RmlFileStore;

impl RmlFileStore {
    /// Builds the store. Stateless: every call touches the path it is
    /// given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn to_file_error(path: &Path, error: impl std::fmt::Display) -> ModListFileError {
    ModListFileError(format!("{}: {error}", path.display()))
}

impl ModListFileStore for RmlFileStore {
    fn read(&self, path: &Path) -> Result<ModListRead, ModListFileError> {
        let file = File::open(path).map_err(|error| to_file_error(path, error))?;
        read_mod_list(file).map_err(|error| to_file_error(path, error))
    }

    fn write(&self, path: &Path, list: &SharedModList) -> Result<(), ModListFileError> {
        write_atomically(path, &write::render_rml(list)).map_err(|error| to_file_error(path, error))
    }
}
