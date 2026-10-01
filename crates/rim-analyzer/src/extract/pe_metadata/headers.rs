//! PE/CLI header walking and the primitive readers: sections, RVAs, the
//! metadata root, blobs, strings.

use super::tables::read_tables;
use super::{AssemblyMetadata, PeMetadataError};

pub(super) fn read_bytes(data: &[u8], offset: usize, len: usize) -> Result<&[u8], PeMetadataError> {
    let end = offset.checked_add(len).ok_or(PeMetadataError::Truncated)?;
    data.get(offset..end).ok_or(PeMetadataError::Truncated)
}

pub(super) fn read_u16(data: &[u8], offset: usize) -> Result<u16, PeMetadataError> {
    let b = read_bytes(data, offset, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32, PeMetadataError> {
    let b = read_bytes(data, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(super) fn read_u64(data: &[u8], offset: usize) -> Result<u64, PeMetadataError> {
    let b = read_bytes(data, offset, 8)?;
    Ok(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

// ---------------------------------------------------------------------
// PE sections and RVA resolution
// ---------------------------------------------------------------------

pub(super) struct Section {
    pub(super) virtual_address: u32,
    pub(super) virtual_size: u32,
    pub(super) size_of_raw_data: u32,
    pub(super) pointer_to_raw_data: u32,
}

pub(super) fn read_sections(
    bytes: &[u8],
    offset: usize,
    count: usize,
) -> Result<Vec<Section>, PeMetadataError> {
    (0..count)
        .map(|i| {
            let base = offset
                .checked_add(i * 40)
                .ok_or(PeMetadataError::Truncated)?;
            Ok(Section {
                virtual_size: read_u32(bytes, base + 8)?,
                virtual_address: read_u32(bytes, base + 12)?,
                size_of_raw_data: read_u32(bytes, base + 16)?,
                pointer_to_raw_data: read_u32(bytes, base + 20)?,
            })
        })
        .collect()
}

pub(super) fn rva_to_offset(sections: &[Section], rva: u32) -> Result<usize, PeMetadataError> {
    let section = sections
        .iter()
        .find(|s| {
            let extent = s.virtual_size.max(s.size_of_raw_data);
            rva >= s.virtual_address && rva < s.virtual_address.saturating_add(extent)
        })
        .ok_or(PeMetadataError::RvaOutOfRange(rva))?;
    let delta = rva
        .checked_sub(section.virtual_address)
        .ok_or(PeMetadataError::RvaOutOfRange(rva))?;
    section
        .pointer_to_raw_data
        .checked_add(delta)
        .map(|offset| offset as usize)
        .ok_or(PeMetadataError::RvaOutOfRange(rva))
}

// ---------------------------------------------------------------------
// Metadata root -> streams
// ---------------------------------------------------------------------

pub(super) fn read_metadata_root(
    bytes: &[u8],
    metadata_offset: usize,
) -> Result<AssemblyMetadata, PeMetadataError> {
    const BSJB_SIGNATURE: u32 = 0x424A_5342;

    if read_u32(bytes, metadata_offset)? != BSJB_SIGNATURE {
        return Err(PeMetadataError::InvalidMetadataSignature);
    }
    let version_length = read_u32(bytes, metadata_offset + 12)? as usize;
    let version_end = metadata_offset
        .checked_add(16 + version_length)
        .ok_or(PeMetadataError::Truncated)?;
    // Flags(2) then Streams(2) immediately follow the padded version string.
    let streams_count = read_u16(bytes, version_end + 2)? as usize;

    let mut cursor = version_end + 4;
    let mut strings_stream = None;
    let mut tables_stream = None;
    let mut blob_stream = None;
    for _ in 0..streams_count {
        let stream_offset = read_u32(bytes, cursor)? as usize;
        let stream_size = read_u32(bytes, cursor + 4)? as usize;
        let name_start = cursor + 8;
        let (name, name_field_len) = read_padded_cstr(bytes, name_start)?;
        cursor = name_start + name_field_len;

        let absolute_offset = metadata_offset
            .checked_add(stream_offset)
            .ok_or(PeMetadataError::Truncated)?;
        match name.as_str() {
            "#~" => tables_stream = Some(absolute_offset),
            "#Strings" => strings_stream = Some((absolute_offset, stream_size)),
            "#Blob" => blob_stream = Some((absolute_offset, stream_size)),
            _ => {}
        }
    }

    let tables_offset = tables_stream.ok_or(PeMetadataError::MissingStream("#~"))?;
    let (strings_offset, strings_size) =
        strings_stream.ok_or(PeMetadataError::MissingStream("#Strings"))?;
    // `#Blob` is absent only for a metadata image with no blob-heap
    // references at all (no signatures, no custom attributes) — treated as an
    // empty heap rather than a hard error, since every fact this reader needs
    // from the tables stream itself still parses fine without one; the
    // features that DO need it (`TypeSpec` unwrapping, runtime-patch
    // attribute decoding) then simply find nothing.
    let (blob_offset, blob_size) = blob_stream.unwrap_or((0, 0));

    read_tables(
        bytes,
        tables_offset,
        strings_offset,
        strings_size,
        blob_offset,
        blob_size,
    )
}

/// Reads a null-terminated stream name starting at `start`, returning it
/// plus the padded (4-byte-aligned) field length consumed.
pub(super) fn read_padded_cstr(
    bytes: &[u8],
    start: usize,
) -> Result<(String, usize), PeMetadataError> {
    let search_window = bytes.get(start..).ok_or(PeMetadataError::Truncated)?;
    let null_rel = search_window
        .iter()
        .position(|&b| b == 0)
        .ok_or(PeMetadataError::Truncated)?;
    let raw_len = null_rel + 1;
    let padded_len = raw_len.div_ceil(4) * 4;
    let name = String::from_utf8_lossy(&search_window[..null_rel]).into_owned();
    Ok((name, padded_len))
}

// ---------------------------------------------------------------------
// #~ tables stream: table row layout
// ---------------------------------------------------------------------

/// Decodes one ECMA-335 "compressed unsigned integer" at `offset`,
/// returning the value and the number of bytes it occupied (1, 2, or 4).
/// Used for blob-heap length prefixes, `SerString` lengths, and a handful
/// of signature-blob fields (`ParamCount`, a `TypeDefOrRef` coded index).
pub(super) fn compressed_uint(
    bytes: &[u8],
    offset: usize,
) -> Result<(u32, usize), PeMetadataError> {
    let b0 = *bytes.get(offset).ok_or(PeMetadataError::Truncated)?;
    if b0 & 0x80 == 0 {
        return Ok((u32::from(b0), 1));
    }
    if b0 & 0xC0 == 0x80 {
        let b1 = *bytes.get(offset + 1).ok_or(PeMetadataError::Truncated)?;
        return Ok(((u32::from(b0 & 0x3F) << 8) | u32::from(b1), 2));
    }
    if b0 & 0xE0 == 0xC0 {
        let rest = read_bytes(bytes, offset + 1, 3)?;
        let value = (u32::from(b0 & 0x1F) << 24)
            | (u32::from(rest[0]) << 16)
            | (u32::from(rest[1]) << 8)
            | u32::from(rest[2]);
        return Ok((value, 4));
    }
    Err(PeMetadataError::InvalidCompressedInteger)
}

/// Reads one blob-heap entry at `index` (a byte offset into `#Blob`): a
/// compressed length prefix followed by that many content bytes.
///
/// Both the length prefix and the content are read from a window bounded
/// by `blob_size` — the `#~` tables stream's own declared `#Blob` stream
/// size — the same way [`read_heap_string`] windows `#Strings`, never from
/// the full file. Without that window, a length prefix that overruns the
/// heap's declared end (or a `TypeSpec`/attribute blob crafted to claim
/// more content than the heap actually holds) would silently read into
/// whatever bytes the file happens to have afterward — some other stream,
/// or past the metadata root entirely — instead of failing.
pub(super) fn read_blob(
    bytes: &[u8],
    blob_offset: usize,
    blob_size: usize,
    index: u32,
) -> Result<&[u8], PeMetadataError> {
    let index = index as usize;
    if index >= blob_size {
        return Err(PeMetadataError::Truncated);
    }
    let heap = read_bytes(bytes, blob_offset, blob_size)?;
    let (len, len_bytes) = compressed_uint(heap, index)?;
    let content_start = index
        .checked_add(len_bytes)
        .ok_or(PeMetadataError::Truncated)?;
    read_bytes(heap, content_start, len as usize)
}

/// Decodes one `SerString` (ECMA-335 II.23.3): a compressed length prefix
/// plus that many UTF-8 bytes, or the single byte `0xFF` for a null
/// string. Returns the decoded text (lossy) and the number of bytes
/// consumed from `bytes` starting at `offset`.
pub(super) fn read_ser_string(
    bytes: &[u8],
    offset: usize,
) -> Result<(Option<String>, usize), PeMetadataError> {
    let first = *bytes.get(offset).ok_or(PeMetadataError::Truncated)?;
    if first == 0xFF {
        return Ok((None, 1));
    }
    let (len, len_bytes) = compressed_uint(bytes, offset)?;
    let start = offset
        .checked_add(len_bytes)
        .ok_or(PeMetadataError::Truncated)?;
    let content = read_bytes(bytes, start, len as usize)?;
    Ok((
        Some(String::from_utf8_lossy(content).into_owned()),
        len_bytes + len as usize,
    ))
}

// ---------------------------------------------------------------------
// Runtime-patch custom-attribute decoding
// ---------------------------------------------------------------------

/// Reads a 2-or-4-byte little-endian index field — a heap offset, a
/// simple table-row index, or a packed coded-index tag+row — the width
/// already resolved by [`Widths::compute`].
pub(super) fn read_index(
    bytes: &[u8],
    offset: usize,
    width: usize,
) -> Result<u32, PeMetadataError> {
    if width == 2 {
        Ok(u32::from(read_u16(bytes, offset)?))
    } else {
        read_u32(bytes, offset)
    }
}

pub(super) fn read_heap_string(
    bytes: &[u8],
    strings_offset: usize,
    strings_size: usize,
    index: u32,
) -> Result<String, PeMetadataError> {
    let index = index as usize;
    if index >= strings_size {
        return Err(PeMetadataError::Truncated);
    }
    let start = strings_offset
        .checked_add(index)
        .ok_or(PeMetadataError::Truncated)?;
    let window = read_bytes(bytes, start, strings_size - index)?;
    let null_rel = window
        .iter()
        .position(|&b| b == 0)
        .ok_or(PeMetadataError::Truncated)?;
    Ok(String::from_utf8_lossy(&window[..null_rel]).into_owned())
}
