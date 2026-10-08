//! [`RmlFileStore`]: reads and writes a shared mod-list file.
//!
//! Reading detects the document by content, never by extension: after a
//! BOM and whitespace, a first `<` means XML (a RimWorld `.rml`
//! `<savedModList>`, or a `ModsConfig.xml`-shaped `<ModsConfigData>` such
//! as RimSort's "export list"), anything else is the text format. The
//! input is untrusted, so [`ModListLimits`] bounds the bytes read, the
//! XML node count, the nesting depth and the entry count before anything
//! grows, and a DTD or DOCTYPE is refused outright.
//!
//! Writing produces a `.rml` byte for byte as RimWorld's own "Save list"
//! does (UTF-8 with BOM, CRLF, tab indentation, `meta` and `modList`),
//! atomically.

mod read;
mod write;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use rim_session::mod_list::{ModListLimits, Rejection, SharedModList};
use rim_session::ports::{ModListFileError, ModListFileStore, ModListRead};

use crate::atomic::write_atomically;

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

/// Reads at most [`ModListLimits::MAX_INPUT_BYTES`] bytes from `reader`:
/// `Ok(None)` when it holds more. Reads one byte past the limit and no
/// further, so an oversized input is never held whole. Public so an
/// interface that reads a list from somewhere other than a file (the CLI's
/// stdin) is bounded by the same constant.
pub fn read_bounded(reader: impl Read) -> std::io::Result<Option<Vec<u8>>> {
    let limit = ModListLimits::MAX_INPUT_BYTES;
    let mut bytes = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() <= limit).then_some(bytes))
}

/// Parses bytes already read (by [`read_bounded`]) the way
/// [`RmlFileStore::read`] parses a file: format detected by content, the
/// same bounds, the same lossy decoding. For an interface that reads a list
/// from somewhere other than a file (the CLI's stdin).
#[must_use]
pub fn parse_mod_list_bytes(bytes: &[u8]) -> ModListRead {
    match read::parse_bytes(bytes) {
        Ok(parsed) => ModListRead::Parsed(parsed),
        Err(rejection) => ModListRead::Rejected(rejection),
    }
}

impl ModListFileStore for RmlFileStore {
    fn read(&self, path: &Path) -> Result<ModListRead, ModListFileError> {
        let file = File::open(path).map_err(|error| to_file_error(path, error))?;
        let Some(bytes) = read_bounded(file).map_err(|error| to_file_error(path, error))? else {
            return Ok(ModListRead::Rejected(Rejection::TooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES,
            }));
        };
        Ok(parse_mod_list_bytes(&bytes))
    }

    fn write(&self, path: &Path, list: &SharedModList) -> Result<(), ModListFileError> {
        write_atomically(path, &write::render_rml(list)).map_err(|error| to_file_error(path, error))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;
    use std::rc::Rc;

    use super::*;

    /// An endless reader of `b'a'` that counts the bytes handed out.
    struct CountingReader(Rc<Cell<usize>>);

    impl Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(b'a');
            self.0.set(self.0.get() + buf.len());
            Ok(buf.len())
        }
    }

    #[test]
    fn an_endless_input_is_cut_off_one_byte_past_the_limit() {
        let handed_out = Rc::new(Cell::new(0));

        let result = read_bounded(CountingReader(Rc::clone(&handed_out))).expect("reads");

        assert_eq!(result, None);
        assert_eq!(handed_out.get(), ModListLimits::MAX_INPUT_BYTES + 1);
    }

    #[test]
    fn an_input_exactly_at_the_limit_is_kept() {
        let bytes = vec![b'x'; ModListLimits::MAX_INPUT_BYTES];

        let result = read_bounded(bytes.as_slice()).expect("reads");

        assert_eq!(result.map(|kept| kept.len()), Some(bytes.len()));
    }
}
