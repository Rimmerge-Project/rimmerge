//! A minimal, dependency-free reader of a ZIP archive's *directory*: the
//! end-of-central-directory record and the central directory's entry
//! names. Nothing is decompressed and no entry body is read.
//!
//! The Waylith install ships its art as `Assets.zip` (355 MB for Core), and
//! this crate only needs the entry *names*, so the caller reads the last
//! few KiB of the file for [`find_end_of_central_directory`], then exactly
//! the central directory it points at, and hands both to this module as
//! plain bytes (the file reads live in `infra::zip_listing`).
//!
//! The input is untrusted: every read is bounds-checked (`.get(..)`), every
//! arithmetic operation is checked, and anything this reader does not model
//! exactly (ZIP64, a multi-disk archive, a truncated or inconsistent
//! directory) is a typed [`ZipDirectoryError`], never a guess.
//!
//! Layout references: PKWARE APPNOTE 4.3.12 (central directory file header)
//! and 4.3.16 (end of central directory record).

use thiserror::Error;

/// The end-of-central-directory record is 22 bytes plus a comment of up to
/// 65,535 bytes: the most a caller ever needs to read from the end of a
/// file to find it.
pub const MAX_END_RECORD_TAIL_BYTES: usize = 22 + 65_535;

/// The largest central directory this reader accepts: the real Core
/// archive's is about 0.5 MB, so 16 MiB is a defensive bound, not a
/// measurement.
pub const MAX_CENTRAL_DIRECTORY_BYTES: u32 = 16 * 1024 * 1024;

const END_RECORD_SIGNATURE: [u8; 4] = *b"PK\x05\x06";
const ZIP64_LOCATOR_SIGNATURE: [u8; 4] = *b"PK\x06\x07";
const ENTRY_SIGNATURE: [u8; 4] = *b"PK\x01\x02";
const END_RECORD_FIXED_BYTES: usize = 22;
const ZIP64_LOCATOR_BYTES: usize = 20;
const ENTRY_FIXED_BYTES: usize = 46;

/// Why an archive's directory could not be listed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ZipDirectoryError {
    /// No end-of-central-directory record in the tail that was read: not a
    /// ZIP archive, or truncated.
    #[error("no end-of-central-directory record (not a ZIP archive, or truncated)")]
    NoEndRecord,
    /// The archive uses ZIP64 (more than 65,534 entries, or a directory
    /// past 4 GiB), which this reader does not model.
    #[error("ZIP64 archives are not supported")]
    Zip64,
    /// The archive spans more than one disk.
    #[error("multi-disk archives are not supported")]
    MultiDisk,
    /// The central directory is larger than [`MAX_CENTRAL_DIRECTORY_BYTES`].
    #[error("central directory is {size} bytes, over the {limit}-byte limit")]
    DirectoryTooLarge {
        /// The size the end record declares.
        size: u32,
        /// [`MAX_CENTRAL_DIRECTORY_BYTES`].
        limit: u32,
    },
    /// The directory is cut short, has a bad entry signature, or its entry
    /// count disagrees with the end record.
    #[error("malformed central directory: {0}")]
    Malformed(&'static str),
}

/// Where the central directory sits, read from the end record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndOfCentralDirectory {
    /// Number of entries the directory holds.
    pub entry_count: usize,
    /// Directory length in bytes (at most [`MAX_CENTRAL_DIRECTORY_BYTES`]).
    pub directory_size: u32,
    /// Directory offset from the start of the archive.
    pub directory_offset: u32,
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let raw: [u8; 2] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u16::from_le_bytes(raw))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let raw: [u8; 4] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

/// Finds and validates the end record inside `tail`, the last bytes of the
/// archive (at most [`MAX_END_RECORD_TAIL_BYTES`] of them are ever needed).
///
/// The record is accepted at the last position whose declared comment
/// length reaches exactly the end of `tail`. A decoy signature inside the
/// real record's comment is therefore rejected, unless its own declared
/// length also reaches the end of the tail.
///
/// # Errors
///
/// [`ZipDirectoryError::NoEndRecord`] when none is found,
/// [`ZipDirectoryError::Zip64`] / [`ZipDirectoryError::MultiDisk`] for
/// archives this reader does not model, and
/// [`ZipDirectoryError::DirectoryTooLarge`] past the size limit.
pub fn find_end_of_central_directory(
    tail: &[u8],
) -> Result<EndOfCentralDirectory, ZipDirectoryError> {
    let last_start = tail
        .len()
        .checked_sub(END_RECORD_FIXED_BYTES)
        .ok_or(ZipDirectoryError::NoEndRecord)?;
    let position = (0..=last_start)
        .rev()
        .find(|&start| is_end_record_at(tail, start))
        .ok_or(ZipDirectoryError::NoEndRecord)?;
    read_end_record(tail, position)
}

fn is_end_record_at(tail: &[u8], start: usize) -> bool {
    let Some(signature) = start.checked_add(4).and_then(|end| tail.get(start..end)) else {
        return false;
    };
    if signature != END_RECORD_SIGNATURE {
        return false;
    }
    let Some(comment_length) = start.checked_add(20).and_then(|at| read_u16_at(tail, at)) else {
        return false;
    };
    start
        .checked_add(END_RECORD_FIXED_BYTES)
        .and_then(|end| end.checked_add(usize::from(comment_length)))
        == Some(tail.len())
}

fn read_end_record(
    tail: &[u8],
    position: usize,
) -> Result<EndOfCentralDirectory, ZipDirectoryError> {
    let field16 = |relative: usize| {
        position
            .checked_add(relative)
            .and_then(|at| read_u16_at(tail, at))
            .ok_or(ZipDirectoryError::NoEndRecord)
    };
    let field32 = |relative: usize| {
        position
            .checked_add(relative)
            .and_then(|at| read_u32_at(tail, at))
            .ok_or(ZipDirectoryError::NoEndRecord)
    };
    let has_zip64_locator = position
        .checked_sub(ZIP64_LOCATOR_BYTES)
        .and_then(|start| tail.get(start..start.checked_add(4)?))
        .is_some_and(|signature| signature == ZIP64_LOCATOR_SIGNATURE);
    let (this_disk, directory_disk) = (field16(4)?, field16(6)?);
    let (entries_on_disk, total_entries) = (field16(8)?, field16(10)?);
    let (directory_size, directory_offset) = (field32(12)?, field32(16)?);

    if has_zip64_locator
        || total_entries == u16::MAX
        || directory_size == u32::MAX
        || directory_offset == u32::MAX
    {
        return Err(ZipDirectoryError::Zip64);
    }
    if this_disk != 0 || directory_disk != 0 || entries_on_disk != total_entries {
        return Err(ZipDirectoryError::MultiDisk);
    }
    if directory_size > MAX_CENTRAL_DIRECTORY_BYTES {
        return Err(ZipDirectoryError::DirectoryTooLarge {
            size: directory_size,
            limit: MAX_CENTRAL_DIRECTORY_BYTES,
        });
    }
    Ok(EndOfCentralDirectory {
        entry_count: usize::from(total_entries),
        directory_size,
        directory_offset,
    })
}

/// Every entry name in `directory` (exactly the central directory's bytes),
/// in directory order. `expected_entries` is the end record's count; a
/// directory that holds a different number is
/// [`ZipDirectoryError::Malformed`].
///
/// Names are decoded as UTF-8, lossily: a name in the legacy code page
/// comes out with replacement characters, which can never equal a texture
/// key and so is harmless to this crate's lookups.
///
/// # Errors
///
/// [`ZipDirectoryError::Malformed`] for a bad signature, a truncated
/// entry, or a count mismatch.
pub fn central_directory_names(
    directory: &[u8],
    expected_entries: usize,
) -> Result<Vec<String>, ZipDirectoryError> {
    let mut names = Vec::with_capacity(expected_entries);
    let mut offset = 0_usize;
    while offset < directory.len() {
        if names.len() == expected_entries {
            return Err(ZipDirectoryError::Malformed("more entries than declared"));
        }
        let header = directory
            .get(offset..offset.saturating_add(ENTRY_FIXED_BYTES))
            .filter(|header| header.len() == ENTRY_FIXED_BYTES)
            .ok_or(ZipDirectoryError::Malformed("truncated entry header"))?;
        if header.get(..4) != Some(ENTRY_SIGNATURE.as_slice()) {
            return Err(ZipDirectoryError::Malformed("bad entry signature"));
        }
        let (Some(name_length), Some(extra_length), Some(comment_length)) = (
            read_u16_at(header, 28),
            read_u16_at(header, 30),
            read_u16_at(header, 32),
        ) else {
            return Err(ZipDirectoryError::Malformed("truncated entry header"));
        };
        let name_length = usize::from(name_length);
        let extra_length = usize::from(extra_length);
        let comment_length = usize::from(comment_length);
        let name_start = offset.saturating_add(ENTRY_FIXED_BYTES);
        let name_end = name_start
            .checked_add(name_length)
            .ok_or(ZipDirectoryError::Malformed("entry name overflows"))?;
        let name_bytes = directory
            .get(name_start..name_end)
            .ok_or(ZipDirectoryError::Malformed("truncated entry name"))?;
        names.push(String::from_utf8_lossy(name_bytes).into_owned());
        offset = name_end
            .checked_add(extra_length)
            .and_then(|end| end.checked_add(comment_length))
            .filter(|&end| end <= directory.len())
            .ok_or(ZipDirectoryError::Malformed("truncated entry extras"))?;
    }
    if names.len() != expected_entries {
        return Err(ZipDirectoryError::Malformed("fewer entries than declared"));
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A central-directory entry header for `name`, with `extra` and
    /// `comment` bytes after it.
    fn entry(name: &str, extra: usize, comment: usize) -> Vec<u8> {
        let mut bytes = Vec::from(ENTRY_SIGNATURE);
        bytes.extend_from_slice(&[0; 24]);
        bytes.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&u16::try_from(extra).unwrap().to_le_bytes());
        bytes.extend_from_slice(&u16::try_from(comment).unwrap().to_le_bytes());
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend(std::iter::repeat_n(0, extra + comment));
        bytes
    }

    struct EndRecord {
        this_disk: u16,
        directory_disk: u16,
        entries_on_disk: u16,
        total_entries: u16,
        size: u32,
        offset: u32,
        comment: Vec<u8>,
    }

    impl EndRecord {
        fn single_disk(entries: u16, size: u32, offset: u32) -> Self {
            Self {
                this_disk: 0,
                directory_disk: 0,
                entries_on_disk: entries,
                total_entries: entries,
                size,
                offset,
                comment: Vec::new(),
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let mut bytes = Vec::from(END_RECORD_SIGNATURE);
            bytes.extend_from_slice(&self.this_disk.to_le_bytes());
            bytes.extend_from_slice(&self.directory_disk.to_le_bytes());
            bytes.extend_from_slice(&self.entries_on_disk.to_le_bytes());
            bytes.extend_from_slice(&self.total_entries.to_le_bytes());
            bytes.extend_from_slice(&self.size.to_le_bytes());
            bytes.extend_from_slice(&self.offset.to_le_bytes());
            bytes.extend_from_slice(&u16::try_from(self.comment.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(&self.comment);
            bytes
        }
    }

    #[test]
    fn the_end_record_is_found_behind_leading_bytes() {
        let mut tail = vec![7_u8; 100];
        tail.extend(EndRecord::single_disk(3, 300, 4_000).bytes());

        let end = find_end_of_central_directory(&tail).unwrap();

        assert_eq!(
            end,
            EndOfCentralDirectory {
                entry_count: 3,
                directory_size: 300,
                directory_offset: 4_000,
            }
        );
    }

    #[test]
    fn a_comment_containing_the_signature_is_not_mistaken_for_the_record() {
        // A signature plus a non-zero comment-length field, so the decoy's
        // own declared comment does not reach the end of the tail.
        let mut decoy = Vec::from(END_RECORD_SIGNATURE);
        decoy.extend_from_slice(&[0; 16]);
        decoy.extend_from_slice(&9_u16.to_le_bytes());
        let mut real = EndRecord::single_disk(2, 50, 60);
        real.comment = decoy;
        let tail = real.bytes();

        let end = find_end_of_central_directory(&tail).unwrap();

        assert_eq!(end.entry_count, 2);
    }

    #[test]
    fn bytes_without_an_end_record_are_refused() {
        assert_eq!(
            find_end_of_central_directory(&[1, 2, 3]),
            Err(ZipDirectoryError::NoEndRecord)
        );
        assert_eq!(
            find_end_of_central_directory(&[0; 64]),
            Err(ZipDirectoryError::NoEndRecord)
        );
    }

    #[test]
    fn a_saturated_field_means_zip64_and_is_refused() {
        let tail = EndRecord::single_disk(u16::MAX, 10, 10).bytes();

        assert_eq!(
            find_end_of_central_directory(&tail),
            Err(ZipDirectoryError::Zip64)
        );
    }

    #[test]
    fn a_zip64_locator_before_the_record_is_refused() {
        let mut tail = Vec::from(ZIP64_LOCATOR_SIGNATURE);
        tail.extend_from_slice(&[0; ZIP64_LOCATOR_BYTES - 4]);
        tail.extend(EndRecord::single_disk(1, 10, 10).bytes());

        assert_eq!(
            find_end_of_central_directory(&tail),
            Err(ZipDirectoryError::Zip64)
        );
    }

    #[test]
    fn a_multi_disk_archive_is_refused() {
        let mut record = EndRecord::single_disk(2, 10, 10);
        record.this_disk = 1;
        let mut split_entries = EndRecord::single_disk(2, 10, 10);
        split_entries.entries_on_disk = 1;
        let mut directory_elsewhere = EndRecord::single_disk(2, 10, 10);
        directory_elsewhere.directory_disk = 1;

        for record in [record, split_entries, directory_elsewhere] {
            assert_eq!(
                find_end_of_central_directory(&record.bytes()),
                Err(ZipDirectoryError::MultiDisk)
            );
        }
    }

    #[test]
    fn an_oversized_directory_is_refused() {
        let tail = EndRecord::single_disk(2, MAX_CENTRAL_DIRECTORY_BYTES + 1, 10).bytes();

        assert_eq!(
            find_end_of_central_directory(&tail),
            Err(ZipDirectoryError::DirectoryTooLarge {
                size: MAX_CENTRAL_DIRECTORY_BYTES + 1,
                limit: MAX_CENTRAL_DIRECTORY_BYTES,
            })
        );
    }

    #[test]
    fn names_are_listed_in_directory_order_skipping_extras_and_comments() {
        let mut directory = entry("Textures/A.png.dds.split", 0, 0);
        directory.extend(entry("Sounds/B.ogg", 5, 3));
        directory.extend(entry("Textures/", 0, 0));

        let names = central_directory_names(&directory, 3).unwrap();

        assert_eq!(
            names,
            vec!["Textures/A.png.dds.split", "Sounds/B.ogg", "Textures/"]
        );
    }

    #[test]
    fn a_count_mismatch_in_either_direction_is_malformed() {
        let directory = entry("a", 0, 0);

        assert!(matches!(
            central_directory_names(&directory, 2),
            Err(ZipDirectoryError::Malformed(_))
        ));
        assert!(matches!(
            central_directory_names(&directory, 0),
            Err(ZipDirectoryError::Malformed(_))
        ));
    }

    #[test]
    fn a_truncated_or_unsigned_entry_is_malformed() {
        let directory = entry("Textures/long-name.dds", 0, 0);
        let mut unsigned = directory.clone();
        unsigned[0] = b'X';

        assert!(matches!(
            central_directory_names(&directory[..directory.len() - 3], 1),
            Err(ZipDirectoryError::Malformed(_))
        ));
        assert!(matches!(
            central_directory_names(&directory[..20], 1),
            Err(ZipDirectoryError::Malformed(_))
        ));
        assert!(matches!(
            central_directory_names(&unsigned, 1),
            Err(ZipDirectoryError::Malformed(_))
        ));
    }

    #[test]
    fn an_empty_directory_with_zero_entries_is_an_empty_listing() {
        assert_eq!(central_directory_names(&[], 0), Ok(Vec::new()));
    }
}
