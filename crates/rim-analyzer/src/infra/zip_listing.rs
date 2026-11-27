//! Lists a ZIP archive's entry names without reading its body: the tail
//! read and the central-directory read around the pure parser in
//! [`crate::extract::zip_directory`].
//!
//! Waylith's `Assets.zip` is hundreds of megabytes, so only two small
//! ranges are ever read: the last [`MAX_END_RECORD_TAIL_BYTES`] bytes (the
//! end record) and the central directory it points at (bounded by
//! [`crate::extract::zip_directory::MAX_CENTRAL_DIRECTORY_BYTES`]).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use thiserror::Error;

use crate::extract::zip_directory::{
    MAX_END_RECORD_TAIL_BYTES, ZipDirectoryError, central_directory_names,
    find_end_of_central_directory,
};

/// Why an archive could not be listed.
#[derive(Debug, Error)]
pub enum ZipListingError {
    /// The file could not be opened, sized, or read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The archive's directory is not one this reader accepts.
    #[error(transparent)]
    Directory(#[from] ZipDirectoryError),
    /// The end record points outside the file.
    #[error("central directory lies outside the file")]
    DirectoryOutOfBounds,
}

/// Every entry name in the ZIP archive at `path`, in directory order.
///
/// # Errors
///
/// [`ZipListingError::Io`] on a read failure, [`ZipListingError::Directory`]
/// for ZIP64, multi-disk or malformed archives, and
/// [`ZipListingError::DirectoryOutOfBounds`] when the end record names a
/// directory that does not fit in the file.
pub fn zip_entry_names(path: &Path) -> Result<Vec<String>, ZipListingError> {
    let mut file = File::open(path)?;
    let file_length = file.metadata()?.len();
    let tail_length = file_length.min(MAX_END_RECORD_TAIL_BYTES as u64);
    file.seek(SeekFrom::Start(file_length.saturating_sub(tail_length)))?;
    let mut tail = Vec::new();
    (&mut file).take(tail_length).read_to_end(&mut tail)?;

    let end = find_end_of_central_directory(&tail)?;
    let fits_in_file = u64::from(end.directory_offset)
        .checked_add(u64::from(end.directory_size))
        .is_some_and(|directory_end| directory_end <= file_length);
    if !fits_in_file {
        return Err(ZipListingError::DirectoryOutOfBounds);
    }

    file.seek(SeekFrom::Start(u64::from(end.directory_offset)))?;
    let mut directory = Vec::new();
    (&mut file)
        .take(u64::from(end.directory_size))
        .read_to_end(&mut directory)?;
    Ok(central_directory_names(&directory, end.entry_count)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal stored (method 0) archive holding `names` with empty
    /// bodies: local headers, the central directory, the end record.
    fn archive_bytes(names: &[&str]) -> Vec<u8> {
        let mut locals = Vec::new();
        let mut directory = Vec::new();
        for name in names {
            let local_offset = u32::try_from(locals.len()).unwrap();
            locals.extend_from_slice(b"PK\x03\x04");
            locals.extend_from_slice(&[0; 22]);
            locals.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
            locals.extend_from_slice(&0_u16.to_le_bytes());
            locals.extend_from_slice(name.as_bytes());

            directory.extend_from_slice(b"PK\x01\x02");
            directory.extend_from_slice(&[0; 24]);
            directory.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
            directory.extend_from_slice(&[0; 12]);
            directory.extend_from_slice(&local_offset.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }
        let mut bytes = locals;
        let directory_offset = u32::try_from(bytes.len()).unwrap();
        let count = u16::try_from(names.len()).unwrap();
        bytes.extend_from_slice(&directory);
        bytes.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&u32::try_from(directory.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&directory_offset.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes
    }

    fn write_archive(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Assets.zip");
        File::create(&path).unwrap().write_all(bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn an_archive_is_listed_by_name_in_directory_order() {
        let (_dir, path) = write_archive(&archive_bytes(&[
            "Textures/A.png.dds.split",
            "Sounds/B.ogg",
        ]));

        let names = zip_entry_names(&path).unwrap();

        assert_eq!(names, vec!["Textures/A.png.dds.split", "Sounds/B.ogg"]);
    }

    #[test]
    fn an_empty_archive_lists_nothing() {
        let (_dir, path) = write_archive(&archive_bytes(&[]));

        assert_eq!(zip_entry_names(&path).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_a_directory_error() {
        let (_dir, path) = write_archive(&[0x41; 300]);

        assert!(matches!(
            zip_entry_names(&path),
            Err(ZipListingError::Directory(ZipDirectoryError::NoEndRecord))
        ));
    }

    #[test]
    fn an_end_record_pointing_past_the_file_is_out_of_bounds() {
        let mut bytes = archive_bytes(&["Textures/A.png.dds"]);
        let record = bytes.len() - 22;
        bytes[record + 16..record + 20].copy_from_slice(&1_000_000_u32.to_le_bytes());
        let (_dir, path) = write_archive(&bytes);

        assert!(matches!(
            zip_entry_names(&path),
            Err(ZipListingError::DirectoryOutOfBounds)
        ));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();

        assert!(matches!(
            zip_entry_names(&dir.path().join("nope.zip")),
            Err(ZipListingError::Io(_))
        ));
    }
}
